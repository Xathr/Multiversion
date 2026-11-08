//! Mojang session check (`hasJoined`) over wasi:http. Non-blocking: start the request, hand its
//! pollable to the reactor, finish when it fires.

use crate::proto::buf::uuid_from_str;
use wasip2::http::outgoing_handler;
use wasip2::http::types::{Fields, FutureIncomingResponse, IncomingBody, Method, OutgoingRequest, Scheme};
use wasip2::io::streams::{InputStream, StreamError};
use wasip2::io::poll::Pollable;

#[derive(Debug, Clone)]
pub struct Property {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub id: u128,
    pub name: String,
    pub properties: Vec<Property>,
}

/// A session check in progress: waiting for the response headers, then reading the body. Each
/// step only takes what is already available, so a slow session server never stalls the proxy.
pub struct PendingAuth {
    stage: Stage,
}

enum Stage {
    Waiting(FutureIncomingResponse),
    // `stream` is declared first so it is dropped before the body it belongs to.
    Reading { stream: InputStream, _body: IncomingBody, bytes: Vec<u8> },
}

/// Where a session check stands after `advance`.
pub enum Progress {
    Done(AuthOutcome),
    /// Waiting for more; watch `pollable()` again and call `advance` when it fires.
    Pending(PendingAuth),
}

impl PendingAuth {
    pub fn pollable(&self) -> Pollable {
        match &self.stage {
            Stage::Waiting(future) => future.subscribe(),
            Stage::Reading { stream, .. } => stream.subscribe(),
        }
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Starts `GET {session_server}/session/minecraft/hasJoined?username=..&serverId=..[&ip=..]`.
pub fn start(session_server: &str, username: &str, server_hash: &str, ip: Option<&str>) -> Result<PendingAuth, String> {
    let (scheme, rest) = session_server
        .split_once("://")
        .ok_or_else(|| format!("session_server has no scheme: {session_server}"))?;
    let (authority, base_path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
    let mut path = format!(
        "/{}session/minecraft/hasJoined?username={}&serverId={}",
        if base_path.is_empty() { String::new() } else { format!("{}/", base_path.trim_end_matches('/')) },
        url_encode(username),
        url_encode(server_hash)
    );
    if let Some(ip) = ip {
        path.push_str("&ip=");
        path.push_str(&url_encode(ip));
    }
    let headers = Fields::from_list(&[("user-agent".to_string(), b"multiversion-pumpkin-plugin".to_vec())])
        .map_err(|e| format!("headers: {e:?}"))?;
    let req = OutgoingRequest::new(headers);
    req.set_method(&Method::Get).map_err(|()| "method")?;
    req.set_scheme(Some(&if scheme == "http" { Scheme::Http } else { Scheme::Https })).map_err(|()| "scheme")?;
    req.set_authority(Some(authority)).map_err(|()| "authority")?;
    req.set_path_with_query(Some(&path)).map_err(|()| "path")?;
    let future = outgoing_handler::handle(req, None).map_err(|e| format!("http: {e:?}"))?;
    Ok(PendingAuth { stage: Stage::Waiting(future) })
}

/// Result of a finished session check.
pub enum AuthOutcome {
    /// Mojang confirmed the session.
    Ok(Profile),
    /// 204 / no content: the player has no valid session for this server.
    InvalidSession,
    /// Something else went wrong (network, 5xx, bad JSON).
    Error(String),
}

/// Takes the session check as far as it can without waiting, once its pollable has fired.
pub fn advance(pending: PendingAuth) -> Progress {
    let (stream, body, mut bytes) = match pending.stage {
        Stage::Waiting(future) => {
            let response = match future.get() {
                Some(Ok(Ok(r))) => r,
                Some(Ok(Err(e))) => return Progress::Done(AuthOutcome::Error(format!("http error: {e:?}"))),
                Some(Err(())) => return Progress::Done(AuthOutcome::Error("response already taken".into())),
                None => return Progress::Pending(PendingAuth { stage: Stage::Waiting(future) }),
            };
            let status = response.status();
            if status == 204 || status == 404 {
                return Progress::Done(AuthOutcome::InvalidSession);
            }
            if status != 200 {
                return Progress::Done(AuthOutcome::Error(format!("session server returned {status}")));
            }
            let Ok(body) = response.consume() else {
                return Progress::Done(AuthOutcome::Error("no body".into()));
            };
            let Ok(stream) = body.stream() else {
                return Progress::Done(AuthOutcome::Error("no body stream".into()));
            };
            (stream, body, Vec::new())
        }
        Stage::Reading { stream, _body, bytes } => (stream, _body, bytes),
    };
    loop {
        match stream.read(1 << 16) {
            // Nothing more yet: come back when the stream has data.
            Ok(chunk) if chunk.is_empty() => {
                return Progress::Pending(PendingAuth { stage: Stage::Reading { stream, _body: body, bytes } });
            }
            Ok(chunk) => {
                bytes.extend_from_slice(&chunk);
                if bytes.len() > 1 << 20 {
                    return Progress::Done(AuthOutcome::Error("response too large".into()));
                }
            }
            Err(StreamError::Closed) => break,
            Err(e) => return Progress::Done(AuthOutcome::Error(format!("read: {e:?}"))),
        }
    }
    drop(stream);
    drop(body);
    Progress::Done(parse_profile(&bytes))
}

pub fn parse_profile(json: &[u8]) -> AuthOutcome {
    let v: serde_json::Value = match serde_json::from_slice(json) {
        Ok(v) => v,
        Err(e) => return AuthOutcome::Error(format!("bad json: {e}")),
    };
    let Some(id) = v.get("id").and_then(|i| i.as_str()).and_then(uuid_from_str) else {
        return AuthOutcome::Error("profile without id".into());
    };
    let name = v.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
    let properties = v
        .get("properties")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    Some(Property {
                        name: p.get("name")?.as_str()?.to_string(),
                        value: p.get("value")?.as_str()?.to_string(),
                        signature: p.get("signature").and_then(|s| s.as_str()).map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    AuthOutcome::Ok(Profile { id, name, properties })
}

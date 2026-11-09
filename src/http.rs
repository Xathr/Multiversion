//! HTTP GET over wasi:http for the update check. A request is started, its pollable is handed to
//! whoever waits (the reactor while players are online, a plain block at startup), and `advance`
//! takes it as far as the data that has arrived allows, so it never stalls the proxy.

use crate::update::{parse_url, redirect_target, Url};
use wasip2::http::outgoing_handler;
use wasip2::http::types::{ErrorCode, Fields, FutureIncomingResponse, IncomingBody, Method, OutgoingRequest, RequestOptions, Scheme};
use wasip2::io::poll::Pollable;
use wasip2::io::streams::{InputStream, StreamError};

const USER_AGENT: &str = concat!("multiversion/", env!("CARGO_PKG_VERSION"), " (Pumpkin plugin)");
pub const MAX_REDIRECTS: usize = 5;
const CONNECT_TIMEOUT_NS: u64 = 4_000_000_000;
const RESPONSE_TIMEOUT_NS: u64 = 8_000_000_000;

/// A GET in progress.
pub struct Pending {
    url: Url,
    max_bytes: usize,
    stage: Stage,
}

enum Stage {
    Waiting(FutureIncomingResponse),
    // `stream` is declared first so it is dropped before the body it belongs to.
    Reading { stream: InputStream, _body: IncomingBody, status: u16, bytes: Vec<u8> },
}

pub enum Reply {
    /// Status 200 and the whole body.
    Body(Vec<u8>),
    /// The server points somewhere else.
    Redirect(String),
}

pub enum Progress {
    Done(Result<Reply, String>),
    /// Waiting for more; wait on `pollable()` and call `advance` again.
    Pending(Pending),
}

impl Pending {
    pub fn pollable(&self) -> Pollable {
        match &self.stage {
            Stage::Waiting(future) => future.subscribe(),
            Stage::Reading { stream, .. } => stream.subscribe(),
        }
    }
}

fn explain(host: &str, e: ErrorCode) -> String {
    match e {
        ErrorCode::HttpRequestDenied => "the http.outbound permission is not granted".to_string(),
        e => format!("{host} ({e:?})"),
    }
}

/// Starts `GET url`. The body may be at most `max_bytes` long.
pub fn start(url: &str, max_bytes: usize) -> Result<Pending, String> {
    let url = parse_url(url)?;
    let headers = Fields::new();
    for (name, value) in [("user-agent", USER_AGENT), ("accept", "application/json, application/wasm, */*")] {
        headers.append(name, value.as_bytes()).map_err(|e| format!("header {name}: {e:?}"))?;
    }
    let request = OutgoingRequest::new(headers);
    let bad = |()| "could not build the request".to_string();
    request.set_method(&Method::Get).map_err(bad)?;
    request.set_scheme(Some(if url.https { &Scheme::Https } else { &Scheme::Http })).map_err(bad)?;
    request.set_authority(Some(&url.authority)).map_err(bad)?;
    request.set_path_with_query(Some(&url.path)).map_err(bad)?;
    let options = RequestOptions::new();
    let _ = options.set_connect_timeout(Some(CONNECT_TIMEOUT_NS));
    let _ = options.set_first_byte_timeout(Some(RESPONSE_TIMEOUT_NS));
    let _ = options.set_between_bytes_timeout(Some(RESPONSE_TIMEOUT_NS));
    let future = outgoing_handler::handle(request, Some(options)).map_err(|e| explain(&url.authority, e))?;
    Ok(Pending { url, max_bytes, stage: Stage::Waiting(future) })
}

/// Takes the request as far as it can go without waiting.
pub fn advance(pending: Pending) -> Progress {
    let Pending { url, max_bytes, stage } = pending;
    let (stream, body, status, mut bytes) = match stage {
        Stage::Waiting(future) => {
            let response = match future.get() {
                Some(Ok(Ok(r))) => r,
                Some(Ok(Err(e))) => return Progress::Done(Err(explain(&url.authority, e))),
                Some(Err(())) => return Progress::Done(Err("response already read".into())),
                None => return Progress::Pending(Pending { url, max_bytes, stage: Stage::Waiting(future) }),
            };
            let status = response.status();
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let Some(location) = response.headers().get("location").into_iter().next() else {
                    return Progress::Done(Err(format!("HTTP {status} without a Location header")));
                };
                return Progress::Done(redirect_target(&url, &String::from_utf8_lossy(&location)).map(Reply::Redirect));
            }
            let Ok(body) = response.consume() else { return Progress::Done(Err("no response body".into())) };
            let Ok(stream) = body.stream() else { return Progress::Done(Err("no response body".into())) };
            (stream, body, status, Vec::new())
        }
        Stage::Reading { stream, _body, status, bytes } => (stream, _body, status, bytes),
    };
    loop {
        match stream.read(1 << 16) {
            Ok(chunk) if chunk.is_empty() => {
                return Progress::Pending(Pending { url, max_bytes, stage: Stage::Reading { stream, _body: body, status, bytes } });
            }
            Ok(chunk) => {
                bytes.extend_from_slice(&chunk);
                if bytes.len() > max_bytes {
                    return Progress::Done(Err(format!("reply from {} is larger than {max_bytes} bytes", url.authority)));
                }
            }
            Err(StreamError::Closed) => break,
            Err(StreamError::LastOperationFailed(e)) => return Progress::Done(Err(format!("{}: {}", url.authority, e.to_debug_string()))),
        }
    }
    drop(stream);
    drop(body);
    if status != 200 {
        // The Market explains errors in a short line of text, such as "Plugin not found on Pumpkin Market".
        let why = std::str::from_utf8(&bytes).ok().map(str::trim).filter(|t| !t.is_empty() && t.len() < 200 && !t.contains('<'));
        return Progress::Done(Err(match why {
            Some(why) => format!("HTTP {status} from {}: {why}", url.authority),
            None => format!("HTTP {status} from {}", url.authority),
        }));
    }
    Progress::Done(Ok(Reply::Body(bytes)))
}

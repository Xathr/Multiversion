//! The identity a proxy in front of this server passes on for a player it has already logged in.
//!
//! Velocity ("modern forwarding") and Vine both answer a login plugin request with the player's
//! address, uuid, name and skin, signed so the backend can tell it came from its own proxy:
//! Velocity with an HMAC-SHA256 over a shared secret, Vine with an Ed25519 signature that also
//! covers a challenge this side picked, so an answer cannot be replayed.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::proto::buf::{Reader, Writer};

pub const VELOCITY_CHANNEL: &str = "velocity:player_info";
pub const VINE_CHANNEL: &str = "vine:player_info";

/// A proxy this plugin can sit behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Velocity,
    Vine,
}

impl Kind {
    /// The name in the console and the value of `[proxy] mode`.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Velocity => "velocity",
            Kind::Vine => "vine",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Kind::Velocity => "Velocity",
            Kind::Vine => "Vine",
        }
    }
}

/// Which proxies a connection is asked about, and whether it must be one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub velocity: bool,
    pub vine: bool,
    /// Players may only come through the proxy: one that connects directly is turned away.
    pub required: bool,
}

/// What `[proxy] mode` means for a new login, given which proxies the config has a secret or
/// key for. `None`: the plugin is not behind a proxy and asks nothing.
///
/// "auto" with a secret or key set behaves as that proxy's mode. With none set it still asks
/// both, only to notice a proxy that someone has put in front (see `Connection`), and players
/// log in directly as before.
pub fn plan(mode: &str, has_velocity: bool, has_vine: bool) -> Option<Plan> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "none" | "off" | "false" => None,
        "velocity" => Some(Plan { velocity: true, vine: false, required: true }),
        "vine" => Some(Plan { velocity: false, vine: true, required: true }),
        _ if has_velocity || has_vine => Some(Plan { velocity: has_velocity, vine: has_vine, required: true }),
        _ => Some(Plan { velocity: true, vine: true, required: false }),
    }
}

/// Why a proxy's answer was not accepted. `key()` is the language key of the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Signed with another secret or key than this side has.
    Signature,
    /// Not the shape the proxy's forwarding has, or a version of it this plugin does not know.
    Data,
    /// Vine answered another challenge than the one sent.
    Challenge,
    /// The key in the config is not an Ed25519 public key.
    Key,
}

impl Refused {
    pub fn key(self) -> &'static str {
        match self {
            Refused::Signature => "proxy.bad_signature",
            Refused::Data => "proxy.bad_data",
            Refused::Challenge => "proxy.bad_challenge",
            Refused::Key => "proxy.bad_key",
        }
    }
}

/// One profile property: name, value, signature.
pub type Property = (String, String, Option<String>);

/// Who the proxy says the player is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forwarded {
    /// The player's own address, as the proxy saw it.
    pub ip: String,
    pub id: u128,
    pub name: String,
    pub properties: Vec<Property>,
}

/// The data of the request sent on `velocity:player_info`: the newest forwarding version this
/// side reads. Version 1 has everything used here (later ones add chat keys, which are not).
pub fn velocity_request() -> Vec<u8> {
    vec![1]
}

/// The data of the request sent on `vine:player_info`: the forwarding version and the challenge.
pub fn vine_request(challenge: &[u8; 16]) -> Vec<u8> {
    let mut out = vec![1];
    out.extend_from_slice(challenge);
    out
}

fn profile(r: &mut Reader) -> Option<Forwarded> {
    let ip = r.string().ok()?.to_string();
    let id = r.uuid().ok()?;
    let name = r.string_max(16).ok()?.to_string();
    let count = usize::try_from(r.varint().ok()?).ok().filter(|n| *n <= 64)?;
    let mut properties = Vec::with_capacity(count);
    for _ in 0..count {
        let name = r.string().ok()?.to_string();
        let value = r.string().ok()?.to_string();
        let signature = if r.bool().ok()? { Some(r.string().ok()?.to_string()) } else { None };
        properties.push((name, value, signature));
    }
    let name_ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    (name_ok && ip.parse::<std::net::IpAddr>().is_ok()).then_some(Forwarded { ip, id, name, properties })
}

/// Compares without stopping at the first difference, so the time taken says nothing about how
/// much of a guessed signature was right.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Checks Velocity's answer against the forwarding secret and reads the player from it.
pub fn read_velocity(data: &[u8], secret: &str) -> Result<Forwarded, Refused> {
    if data.len() < 33 {
        return Err(Refused::Data);
    }
    let (signature, payload) = data.split_at(32);
    if !same(&crate::crypto::hmac_sha256(secret.as_bytes(), payload), signature) {
        return Err(Refused::Signature);
    }
    let mut r = Reader::new(payload);
    // 1 is the plain form; 4 ("lazy session") has the same fields. 2 and 3 carry a chat key
    // after them, which is never asked for (see `velocity_request`).
    match r.varint() {
        Ok(1 | 4) => profile(&mut r).ok_or(Refused::Data),
        _ => Err(Refused::Data),
    }
}

/// Vine's public key from what the config has: the key itself (64 hex characters), or the
/// secret the backend has in Vine's own config, from which Vine derives its key pair.
pub fn vine_key(public_key: &str, secret: &str) -> Result<VerifyingKey, Refused> {
    let public_key = public_key.trim();
    let secret = secret.trim();
    if !public_key.is_empty() {
        let bytes: [u8; 32] = hex::decode(public_key).ok().and_then(|b| b.try_into().ok()).ok_or(Refused::Key)?;
        return VerifyingKey::from_bytes(&bytes).map_err(|_| Refused::Key);
    }
    if secret.is_empty() {
        return Err(Refused::Key);
    }
    let seed: [u8; 32] = match hex::decode(secret).ok().filter(|_| secret.len() == 64).and_then(|b| b.try_into().ok()) {
        Some(seed) => seed,
        None => Sha256::digest(secret.as_bytes()).into(),
    };
    Ok(ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key())
}

/// Checks Vine's answer against its public key and the challenge that was sent, and reads the
/// player from it. The challenge is what makes an answer good for one login only, so the time
/// in the answer is not compared with this machine's clock.
pub fn read_vine(data: &[u8], key: &VerifyingKey, challenge: &[u8; 16]) -> Result<Forwarded, Refused> {
    if data.len() < 64 + 1 + 8 + 16 {
        return Err(Refused::Data);
    }
    let (signature, payload) = data.split_at(64);
    let signature = Signature::from_bytes(signature.try_into().map_err(|_| Refused::Data)?);
    key.verify(payload, &signature).map_err(|_| Refused::Signature)?;
    let mut r = Reader::new(payload);
    if !matches!(r.varint(), Ok(1)) {
        return Err(Refused::Data);
    }
    r.bytes(8).map_err(|_| Refused::Data)?;
    if r.bytes(16).map_err(|_| Refused::Data)? != challenge {
        return Err(Refused::Challenge);
    }
    profile(&mut r).ok_or(Refused::Data)
}

/// A login plugin request: message id, channel, data.
pub fn request(message_id: i32, channel: &str, data: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(message_id);
    w.string(channel);
    w.raw(data);
    w.into_inner()
}

/// A login plugin answer: its message id and the data, if the other side understood the request.
pub fn parse_answer(payload: &[u8]) -> Option<(i32, Option<&[u8]>)> {
    let mut r = Reader::new(payload);
    let id = r.varint().ok()?;
    let understood = r.bool().ok()?;
    Some((id, (understood && !r.remaining().is_empty()).then(|| r.remaining())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn body(version: i32, extra: &[u8]) -> Vec<u8> {
        let mut w = Writer::new();
        w.varint(version);
        w.raw(extra);
        w.string("203.0.113.9");
        w.uuid(0x1234_5678_9abc_def0_1122_3344_5566_7788);
        w.string("Steve");
        w.varint(2);
        w.string("textures").string("abc").bool(true).string("sig");
        w.string("other").string("x").bool(false);
        w.into_inner()
    }

    fn steve() -> Forwarded {
        Forwarded {
            ip: "203.0.113.9".into(),
            id: 0x1234_5678_9abc_def0_1122_3344_5566_7788,
            name: "Steve".into(),
            properties: vec![("textures".into(), "abc".into(), Some("sig".into())), ("other".into(), "x".into(), None)],
        }
    }

    fn velocity(version: i32, secret: &str) -> Vec<u8> {
        let payload = body(version, &[]);
        let mut out = crate::crypto::hmac_sha256(secret.as_bytes(), &payload).to_vec();
        out.extend(payload);
        out
    }

    #[test]
    fn velocity_forwarding_is_read_only_with_the_right_secret() {
        assert_eq!(read_velocity(&velocity(1, "s3cret"), "s3cret"), Ok(steve()));
        assert_eq!(read_velocity(&velocity(4, "s3cret"), "s3cret"), Ok(steve()));
        assert_eq!(read_velocity(&velocity(1, "s3cret"), "other"), Err(Refused::Signature));
        assert_eq!(read_velocity(&velocity(2, "s3cret"), "s3cret"), Err(Refused::Data));
        assert_eq!(read_velocity(&[0; 10], "s3cret"), Err(Refused::Data));
        // one changed byte anywhere and it no longer counts
        let mut changed = velocity(1, "s3cret");
        *changed.last_mut().unwrap() ^= 1;
        assert_eq!(read_velocity(&changed, "s3cret"), Err(Refused::Signature));
    }

    fn vine(secret: &str, challenge: &[u8; 16]) -> Vec<u8> {
        let mut extra = 1_700_000_000u64.to_be_bytes().to_vec();
        extra.extend_from_slice(challenge);
        let payload = body(1, &extra);
        let seed: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
        let mut out = SigningKey::from_bytes(&seed).sign(&payload).to_bytes().to_vec();
        out.extend(payload);
        out
    }

    #[test]
    fn vine_forwarding_needs_the_key_and_the_challenge() {
        let challenge = [7u8; 16];
        let key = vine_key("", "vine secret").unwrap();
        assert_eq!(read_vine(&vine("vine secret", &challenge), &key, &challenge), Ok(steve()));
        // the public key alone does the same as the secret
        let public = hex::encode(key.to_bytes());
        assert_eq!(read_vine(&vine("vine secret", &challenge), &vine_key(&public, "").unwrap(), &challenge), Ok(steve()));
        // an answer to another login's challenge, or from another proxy, is refused
        assert_eq!(read_vine(&vine("vine secret", &[8u8; 16]), &key, &challenge), Err(Refused::Challenge));
        assert_eq!(read_vine(&vine("someone else", &challenge), &key, &challenge), Err(Refused::Signature));
        assert_eq!(read_vine(&[0; 20], &key, &challenge), Err(Refused::Data));
        assert_eq!(vine_key("", "").err(), Some(Refused::Key));
        assert_eq!(vine_key("zz", "").err(), Some(Refused::Key));
        // a 64-character hex secret is the key seed itself, as in Vine
        let hex_secret = "11".repeat(32);
        assert_eq!(vine_key("", &hex_secret).unwrap(), SigningKey::from_bytes(&[0x11; 32]).verifying_key());
    }

    #[test]
    fn the_mode_decides_who_is_asked() {
        assert_eq!(plan("none", true, true), None);
        assert_eq!(plan("velocity", false, true), Some(Plan { velocity: true, vine: false, required: true }));
        assert_eq!(plan("Vine", true, false), Some(Plan { velocity: false, vine: true, required: true }));
        assert_eq!(plan("auto", true, false), Some(Plan { velocity: true, vine: false, required: true }));
        assert_eq!(plan("auto", true, true), Some(Plan { velocity: true, vine: true, required: true }));
        // nothing set up: look for a proxy, but let players in directly
        assert_eq!(plan("auto", false, false), Some(Plan { velocity: true, vine: true, required: false }));
    }

    #[test]
    fn requests_and_answers() {
        assert_eq!(request(5, "a:b", &[9]), [5, 3, b'a', b':', b'b', 9]);
        assert_eq!(vine_request(&[2; 16]).len(), 17);
        assert_eq!(parse_answer(&[5, 1, 7, 8]), Some((5, Some(&[7u8, 8][..]))));
        assert_eq!(parse_answer(&[5, 0]), Some((5, None)));
        assert_eq!(parse_answer(&[5, 1]), Some((5, None)));
        assert_eq!(parse_answer(&[]), None);
    }
}

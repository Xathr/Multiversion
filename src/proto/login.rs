//! Login packets the plugin writes to the client itself, in the client's version.

use super::buf::{Reader, Result, Writer};

/// What a client says in its login start.
#[derive(Debug, PartialEq, Eq)]
pub struct LoginStart<'a> {
    pub name: &'a str,
    /// The uuid the client claims, where its version sends one.
    pub uuid: Option<u128>,
}

/// Reads a login start in the client's version. 1.19 to 1.19.2 also send the player's chat
/// signing key here; it is skipped, because the plugin cannot pass signed chat between versions
/// that sign differently and sends those clients' chat unsigned.
pub fn parse_login_start(protocol: i32, payload: &[u8]) -> Result<LoginStart<'_>> {
    let mut r = Reader::new(payload);
    let name = r.string_max(16)?;
    if (759..=760).contains(&protocol) && r.bool()? {
        r.i64()?; // key expiry
        r.byte_array()?; // public key
        r.byte_array()?; // Mojang's signature of it
    }
    let uuid = match protocol {
        764.. => Some(r.uuid()?),
        760..=763 => r.option(Reader::uuid)?,
        _ => None, // 1.19 sends no uuid
    };
    Ok(LoginStart { name, uuid })
}

/// The encrypted shared secret and verify token from an encryption response. The token is `None`
/// when a 1.19 to 1.19.2 client signed it with its chat key instead of encrypting it.
pub fn parse_encryption_response(protocol: i32, payload: &[u8]) -> Result<(&[u8], Option<&[u8]>)> {
    let mut r = Reader::new(payload);
    let secret = r.byte_array()?;
    if (759..=760).contains(&protocol) && !r.bool()? {
        r.i64()?; // salt
        r.byte_array()?; // signature of token + salt
        return Ok((secret, None));
    }
    Ok((secret, Some(r.byte_array()?)))
}

/// Encryption request; 1.20.5+ clients also read whether to check the session with Mojang.
pub fn encryption_request(protocol: i32, public_der: &[u8], verify_token: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.string(""); // server id
    w.byte_array(public_der);
    w.byte_array(verify_token);
    if protocol >= 766 {
        w.bool(true); // should authenticate
    }
    w.into_inner()
}

/// Login success: profile (properties as name, value, signature), then the fields only some
/// versions have: strict error handling (1.20.5-1.21.1) and the session id (26.2+).
pub fn login_finished(protocol: i32, uuid: u128, name: &str, props: &[(&str, &str, Option<&str>)], session_id: u128) -> Vec<u8> {
    let mut w = Writer::new();
    w.uuid(uuid);
    w.string(name);
    w.varint(props.len() as i32);
    for (n, v, s) in props {
        w.string(n);
        w.string(v);
        w.option(*s, |w, s| {
            w.string(s);
        });
    }
    if protocol >= 776 {
        w.uuid(session_id);
    }
    if (766..768).contains(&protocol) {
        w.bool(true); // strict error handling
    }
    w.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1.20.3 clients get the login packets without the fields 1.20.5 added ("should
    /// authenticate", "strict error handling"); 1.20.5 to 1.21.1 clients get both, later ones only the first.
    #[test]
    fn login_packets_follow_the_client_version() {
        let props = [("textures", "v", None)];
        for (protocol, authenticate, strict) in [(765, false, false), (766, true, true), (767, true, true), (768, true, false)] {
            let mut expected = Writer::new();
            expected.string("").byte_array(&[1, 2, 3]).byte_array(&[9, 9, 9, 9]);
            if authenticate {
                expected.bool(true);
            }
            assert_eq!(encryption_request(protocol, &[1, 2, 3], &[9, 9, 9, 9]), expected.buf, "protocol {protocol}");
            let mut expected = Writer::new();
            expected.uuid(7).string("Steve").varint(1).string("textures").string("v").bool(false);
            if strict {
                expected.bool(true);
            }
            assert_eq!(login_finished(protocol, 7, "Steve", &props, 5), expected.buf, "protocol {protocol}");
        }
        // 1.19 to 1.20.1 take the same two packets as 1.20.3.
        for protocol in [759, 760, 761, 762, 763] {
            assert_eq!(encryption_request(protocol, &[1], &[2]), encryption_request(765, &[1], &[2]));
            assert_eq!(login_finished(protocol, 7, "Steve", &props, 5), login_finished(765, 7, "Steve", &props, 5));
        }
        let mut with_session = Writer::new();
        with_session.uuid(7).string("Steve").varint(0).uuid(5);
        assert_eq!(login_finished(777, 7, "Steve", &[], 5), with_session.buf);
    }

    #[test]
    fn login_start_by_version() {
        let key = |w: &mut Writer| {
            w.bool(true).i64(99).byte_array(&[1; 30]).byte_array(&[2; 40]);
        };
        // 1.20.2+: name and uuid
        let mut w = Writer::new();
        w.string("Steve").uuid(5);
        assert_eq!(parse_login_start(764, &w.buf).unwrap(), LoginStart { name: "Steve", uuid: Some(5) });
        // 1.19.3 to 1.20.1: name and optional uuid
        let mut w = Writer::new();
        w.string("Steve").bool(true).uuid(5);
        for protocol in [761, 762, 763] {
            assert_eq!(parse_login_start(protocol, &w.buf).unwrap().uuid, Some(5));
        }
        let mut w = Writer::new();
        w.string("Steve").bool(false);
        assert_eq!(parse_login_start(762, &w.buf).unwrap().uuid, None);
        // 1.19.1 / 1.19.2: optional chat key, then optional uuid
        let mut w = Writer::new();
        w.string("Steve");
        key(&mut w);
        w.bool(true).uuid(6);
        assert_eq!(parse_login_start(760, &w.buf).unwrap(), LoginStart { name: "Steve", uuid: Some(6) });
        let mut w = Writer::new();
        w.string("Steve").bool(false).bool(false);
        assert_eq!(parse_login_start(760, &w.buf).unwrap().uuid, None);
        // 1.19: optional chat key, no uuid
        let mut w = Writer::new();
        w.string("Steve");
        key(&mut w);
        assert_eq!(parse_login_start(759, &w.buf).unwrap(), LoginStart { name: "Steve", uuid: None });
        // cut short
        assert!(parse_login_start(760, &w.buf[..10]).is_err());
        assert!(parse_login_start(764, &[5, b'S', b't', b'e', b'v', b'e']).is_err());
    }

    #[test]
    fn encryption_response_by_version() {
        let mut plain = Writer::new();
        plain.byte_array(&[1, 2]).byte_array(&[3, 4]);
        for protocol in [761, 763, 777] {
            assert_eq!(parse_encryption_response(protocol, &plain.buf).unwrap(), (&[1u8, 2][..], Some(&[3u8, 4][..])));
        }
        // 1.19 to 1.19.2: a flag says whether the token follows or a signature does
        let mut token = Writer::new();
        token.byte_array(&[1, 2]).bool(true).byte_array(&[3, 4]);
        let mut signed = Writer::new();
        signed.byte_array(&[1, 2]).bool(false).i64(77).byte_array(&[9; 16]);
        for protocol in [759, 760] {
            assert_eq!(parse_encryption_response(protocol, &token.buf).unwrap(), (&[1u8, 2][..], Some(&[3u8, 4][..])));
            assert_eq!(parse_encryption_response(protocol, &signed.buf).unwrap(), (&[1u8, 2][..], None));
        }
        assert!(parse_encryption_response(760, &signed.buf[..5]).is_err());
    }
}

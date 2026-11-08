//! Login packets the plugin writes to the client itself, in the client's version.

use super::buf::Writer;

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
        let mut with_session = Writer::new();
        with_session.uuid(7).string("Steve").varint(0).uuid(5);
        assert_eq!(login_finished(777, 7, "Steve", &[], 5), with_session.buf);
    }
}

//! Chat, 1.19 ↔ 1.18.2.
//!
//! 1.18.2 has one clientbound CHAT packet (text, where it is shown, who sent it) and one
//! serverbound (the text, commands included). 1.19 has SYSTEM_CHAT and a signed PLAYER_CHAT
//! clientbound, and separate signed CHAT and CHAT_COMMAND packets serverbound. A 1.18.2 client
//! has no signing key, so what it types goes to the server unsigned.

use crate::proto::buf::{Reader, Result, Writer};

/// Text components may be this long (in characters), longer than other strings.
const MAX_COMPONENT: usize = 262_144;
/// The longest message a client may send.
const MAX_MESSAGE: usize = 256;

/// 1.19's "game info" chat type: text above the hotbar.
const GAME_INFO: i32 = 2;
/// Where 1.18.2 shows a message: the chat window as a player's message, the chat window as a
/// server message, or above the hotbar.
const POSITION_CHAT: u8 = 0;
const POSITION_SYSTEM: u8 = 1;
const POSITION_GAME_INFO: u8 = 2;

/// SYSTEM_CHAT → CHAT, from nobody.
pub fn system_chat(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 17);
    w.string(r.string_max(MAX_COMPONENT)?);
    w.u8(if r.varint()? == GAME_INFO { POSITION_GAME_INFO } else { POSITION_SYSTEM });
    w.uuid(0);
    Ok(w.into_inner())
}

fn component(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|_| serde_json::Value::String(text.to_string()))
}

/// PLAYER_CHAT → CHAT. The steps above this one already turn player chat into system chat (its
/// signature means nothing to an older client), so this only matters if one ever stops doing
/// that: the message is shown the way the game shows chat, `<name> text`.
pub fn player_chat(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let signed = r.string_max(MAX_COMPONENT)?;
    let unsigned = r.option(|r| r.string_max(MAX_COMPONENT))?;
    r.varint()?; // chat type
    let sender = r.uuid()?;
    let sender_name = r.string_max(MAX_COMPONENT)?;
    let shown = serde_json::json!({ "translate": "chat.type.text", "with": [component(sender_name), component(unsigned.unwrap_or(signed))] });
    let mut w = Writer::with_capacity(p.len() + 64);
    w.string(&shown.to_string()).u8(POSITION_CHAT).uuid(sender);
    Ok(w.into_inner())
}

/// What a 1.18.2 CHAT becomes on the way to the server.
#[derive(Debug, PartialEq, Eq)]
pub enum Typed {
    /// 1.19 CHAT payload.
    Message(Vec<u8>),
    /// 1.19 CHAT_COMMAND payload.
    Command(Vec<u8>),
}

/// Serverbound CHAT: a message, or a command when it starts with a slash. Both go unsigned,
/// stamped with the time `now` (milliseconds since 1970).
pub fn chat(p: &[u8], now: u64) -> Result<Typed> {
    let mut r = Reader::new(p);
    let text = r.string_max(MAX_MESSAGE)?;
    let mut w = Writer::with_capacity(p.len() + 20);
    match text.strip_prefix('/') {
        Some(command) => {
            w.string(command).i64(now as i64).i64(0);
            w.varint(0); // no signed arguments
            w.bool(false); // no signed preview
            Ok(Typed::Command(w.into_inner()))
        }
        None => {
            w.string(text).i64(now as i64).i64(0);
            w.byte_array(&[]); // no signature
            w.bool(false); // no signed preview
            Ok(Typed::Message(w.into_inner()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_chat_names_a_position_and_no_sender() {
        for (ty, position) in [(1, POSITION_SYSTEM), (0, POSITION_SYSTEM), (GAME_INFO, POSITION_GAME_INFO)] {
            let mut w = Writer::new();
            w.string("{\"text\":\"saved\"}").varint(ty);
            let mut want = Writer::new();
            want.string("{\"text\":\"saved\"}").u8(position).uuid(0);
            assert_eq!(system_chat(&w.buf).unwrap(), want.buf);
        }
        assert!(system_chat(&[2, b'h', b'i']).is_err());
    }

    #[test]
    fn player_chat_is_shown_with_the_senders_name() {
        let mut w = Writer::new();
        w.string("{\"text\":\"hi\"}").bool(false).varint(0).uuid(7).string("{\"text\":\"Alex\"}").bool(false).i64(1).i64(2).byte_array(&[1, 2]);
        let out = player_chat(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        let shown: serde_json::Value = serde_json::from_str(r.string_max(MAX_COMPONENT).unwrap()).unwrap();
        assert_eq!(shown["translate"], "chat.type.text");
        assert_eq!(shown["with"][0]["text"], "Alex");
        assert_eq!(shown["with"][1]["text"], "hi");
        assert_eq!((r.u8().unwrap(), r.uuid().unwrap()), (POSITION_CHAT, 7));
        assert!(r.is_empty());
    }

    #[test]
    fn typed_text_is_a_message_or_a_command() {
        let mut w = Writer::new();
        w.string("hello there");
        let mut want = Writer::new();
        want.string("hello there").i64(1234).i64(0).byte_array(&[]).bool(false);
        assert_eq!(chat(&w.buf, 1234).unwrap(), Typed::Message(want.into_inner()));

        let mut w = Writer::new();
        w.string("/gamemode creative");
        let mut want = Writer::new();
        want.string("gamemode creative").i64(1234).i64(0).varint(0).bool(false);
        assert_eq!(chat(&w.buf, 1234).unwrap(), Typed::Command(want.into_inner()));

        let mut long = Writer::new();
        long.string(&"a".repeat(800));
        assert!(chat(&long.buf, 0).is_err());
    }
}

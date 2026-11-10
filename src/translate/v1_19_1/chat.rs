//! Chat and the player list, 1.19.3 ↔ 1.19.1.
//!
//! The two versions sign chat in different ways (1.19.1 chains each message to the sender's
//! previous one and to the last messages seen; 1.19.3 signs per chat session with a message
//! index), and a signature can't be made again without the player's private key. So, like
//! ViaBackwards, nothing signed crosses this step: the server's player chat reaches the client
//! as system chat, decorated here the way the client would have done it from the chat type
//! registry, and the client's messages and commands go to the server without a signature and
//! with an empty "seen messages" acknowledgement.

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

/// Text components may be this long (in characters), longer than other strings.
const MAX_COMPONENT: usize = 262_144;
/// A chat message or command as typed.
const MAX_MESSAGE: usize = 256;
/// A message signature (always this long in 1.19.3).
const SIGNATURE: usize = 256;

fn err(msg: String) -> DecodeError {
    DecodeError::Other(msg)
}

fn component(r: &mut Reader) -> Result<Value> {
    let s = r.string_max(MAX_COMPONENT)?;
    serde_json::from_str(s).map_err(|e| err(format!("text component is not JSON: {e}")))
}

fn optional_component(r: &mut Reader) -> Result<Option<Value>> {
    if r.bool()? { component(r).map(Some) } else { Ok(None) }
}

/// The chat types of the server's registry (from LOGIN): network id → how a message of that
/// type is shown (`translation_key`, `parameters`, `style`), `None` for a type shown nowhere.
#[derive(Default)]
pub struct ChatTypes {
    types: HashMap<i32, Option<Tag>>,
}

const STYLE_FLAGS: [&str; 5] = ["bold", "italic", "underlined", "strikethrough", "obfuscated"];

impl ChatTypes {
    /// Replaces the known types with those of a LOGIN registry codec.
    pub fn load(&mut self, registries: &Tag) {
        self.types.clear();
        let entries = registries.get("minecraft:chat_type").and_then(|t| t.get("value")).and_then(Tag::as_list);
        for entry in entries.into_iter().flatten() {
            if let Some(id) = entry.get("id").and_then(Tag::as_i32) {
                self.types.insert(id, entry.get("element").and_then(|e| e.get("chat")).cloned());
            }
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// The component a client shows for a message of this type, or `None` for a type that is
    /// not shown in chat. A type the registry doesn't list is shown like plain chat rather than
    /// lost.
    pub fn decorate(&self, chat_type: i32, sender: Value, target: Option<Value>, content: Value) -> Option<Value> {
        let Some(known) = self.types.get(&chat_type) else {
            return Some(json!({"translate": "chat.type.text", "with": [sender, content]}));
        };
        let chat = known.as_ref()?;
        let mut out = Map::new();
        out.insert("translate".into(), Value::String(chat.get("translation_key").and_then(Tag::as_str).unwrap_or("chat.type.text").to_string()));
        if let Some(style) = chat.get("style") {
            if let Some(color) = style.get("color").and_then(Tag::as_str) {
                out.insert("color".into(), Value::String(color.to_string()));
            }
            for flag in STYLE_FLAGS {
                if let Some(v) = style.get(flag).and_then(Tag::as_i32) {
                    out.insert(flag.into(), Value::Bool(v != 0));
                }
            }
        }
        let mut args = Vec::new();
        for p in chat.get("parameters").and_then(Tag::as_list).into_iter().flatten() {
            match p.as_str() {
                Some("sender") => args.push(sender.clone()),
                Some("content") => args.push(content.clone()),
                Some("target") => args.push(target.clone().unwrap_or_else(|| json!(""))),
                _ => {}
            }
        }
        out.insert("with".into(), Value::Array(args));
        Some(Value::Object(out))
    }
}

/// The registry codec of a LOGIN packet (the layout is the same in both versions, so the packet
/// itself is passed on as it is).
pub fn login_registries(payload: &[u8]) -> Result<Option<Tag>> {
    let mut r = Reader::new(payload);
    r.bytes(4 + 1 + 1 + 1)?; // entity id, hardcore, game mode, previous game mode
    r.list(|r| r.string().map(|_| ()))?; // dimensions
    nbt::read_named_optional(&mut r)
}

/// SYSTEM_CHAT in the chat window (not the action bar).
fn system_chat(message: &Value) -> Vec<u8> {
    let mut w = Writer::new();
    w.string(&message.to_string()).bool(false);
    w.into_inner()
}

/// The chat type and names that end PLAYER_CHAT and DISGUISED_CHAT, applied to `content`.
fn decorated(types: &ChatTypes, r: &mut Reader, content: Value) -> Result<Option<Vec<u8>>> {
    let chat_type = r.varint()?;
    let sender = component(r)?;
    let target = optional_component(r)?;
    Ok(types.decorate(chat_type, sender, target, content).map(|m| system_chat(&m)))
}

/// PLAYER_CHAT → the payload of a SYSTEM_CHAT, `None` for a chat type that is not shown. The
/// server's own formatting of the message is used when it sent one, the plain text otherwise.
pub fn player_chat(types: &ChatTypes, payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    r.uuid()?; // sender
    r.varint()?; // index
    if r.bool()? {
        r.bytes(SIGNATURE)?;
    }
    let plain = r.string_max(MAX_MESSAGE)?;
    r.bytes(8 + 8)?; // timestamp, salt
    let seen = r.varint()?;
    for _ in 0..seen.max(0) {
        // an earlier message by its place in the client's cache, or (0) its whole signature
        if r.varint()? == 0 {
            r.bytes(SIGNATURE)?;
        }
    }
    let content = optional_component(&mut r)?.unwrap_or_else(|| json!({"text": plain}));
    if r.varint()? == 2 {
        r.long_array()?; // which characters the server's filter hides
    }
    decorated(types, &mut r, content)
}

/// DISGUISED_CHAT (chat sent in a player's name without a signature, e.g. by /say from the
/// console) → the payload of a SYSTEM_CHAT.
pub fn disguised_chat(types: &ChatTypes, payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let content = component(&mut r)?;
    decorated(types, &mut r, content)
}

/// No signature, and nothing acknowledged: a message count of zero and an empty set of the 20
/// last seen messages (three bytes of bits).
fn unsigned_tail(w: &mut Writer) {
    w.varint(0).raw(&[0, 0, 0]);
}

/// CHAT 1.19.1 → 1.19.3. Only the message, timestamp and salt are kept: the signature, the
/// preview flag and the seen-messages lists that follow mean nothing to a 1.19.3 server.
pub fn chat_to_server(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len().min(1024) + 8);
    w.string(r.string_max(MAX_MESSAGE)?);
    w.raw(r.bytes(8 + 8)?); // timestamp, salt
    w.bool(false); // no signature
    unsigned_tail(&mut w);
    Ok(w.into_inner())
}

/// CHAT_COMMAND 1.19.1 → 1.19.3, without argument signatures.
pub fn chat_command_to_server(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len().min(1024) + 8);
    w.string(r.string_max(MAX_MESSAGE)?);
    w.raw(r.bytes(8 + 8)?); // timestamp, salt
    w.varint(0); // no argument signatures
    unsigned_tail(&mut w);
    Ok(w.into_inner())
}

const ADD_PLAYER: u8 = 0x01;
const INITIALIZE_CHAT: u8 = 0x02;
const UPDATE_GAME_MODE: u8 = 0x04;
const UPDATE_LISTED: u8 = 0x08;
const UPDATE_LATENCY: u8 = 0x10;
const UPDATE_DISPLAY_NAME: u8 = 0x20;

/// One player of a PLAYER_INFO_UPDATE.
struct Entry<'a> {
    uuid: u128,
    /// Name and properties (the properties as sent: same layout in both versions).
    profile: Option<(&'a str, &'a [u8])>,
    game_mode: i32,
    latency: i32,
    display_name: Option<&'a str>,
}

/// PLAYER_INFO_UPDATE → 1.19.1 PLAYER_INFO payloads. 1.19.3 sends any mix of actions in one
/// packet; 1.19.1 has one action per packet, and its "add player" carries the game mode,
/// latency and display name too (defaults for what the server left out).
///
/// Chat sessions are dropped (no signed chat reaches the client, so it has no use for the
/// keys), and so is the "listed" flag: 1.19.1 shows every known player in the tab list.
pub fn player_info_update(payload: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let actions = r.u8()?;
    let entries = r.list(|r| {
        let mut e = Entry { uuid: r.uuid()?, profile: None, game_mode: 0, latency: 0, display_name: None };
        if actions & ADD_PLAYER != 0 {
            let name = r.string_max(16)?;
            let start = r.position();
            r.list(|r| {
                r.string()?; // name
                r.string()?; // value
                r.option(|r| r.string().map(|_| ())).map(|_| ()) // signature
            })?;
            let len = r.position() - start;
            r.set_position(start);
            e.profile = Some((name, r.bytes(len)?));
        }
        if actions & INITIALIZE_CHAT != 0 && r.bool()? {
            r.bytes(16 + 8)?; // session id, key expiry
            r.byte_array()?; // public key
            r.byte_array()?; // key signature
        }
        if actions & UPDATE_GAME_MODE != 0 {
            e.game_mode = r.varint()?;
        }
        if actions & UPDATE_LISTED != 0 {
            r.bool()?;
        }
        if actions & UPDATE_LATENCY != 0 {
            e.latency = r.varint()?;
        }
        if actions & UPDATE_DISPLAY_NAME != 0 && r.bool()? {
            e.display_name = Some(r.string_max(MAX_COMPONENT)?);
        }
        Ok(e)
    })?;
    let packet = |action: i32, body: &dyn Fn(&mut Writer, &Entry)| {
        let mut w = Writer::new();
        w.varint(action).varint(entries.len() as i32);
        for e in &entries {
            w.uuid(e.uuid);
            body(&mut w, e);
        }
        w.into_inner()
    };
    let display_name = |w: &mut Writer, e: &Entry| {
        w.option(e.display_name, |w, s| {
            w.string(s);
        });
    };
    if actions & ADD_PLAYER != 0 {
        return Ok(vec![packet(0, &|w, e| {
            let (name, properties) = e.profile.unwrap_or(("", &[0][..]));
            w.string(name).raw(properties);
            w.varint(e.game_mode).varint(e.latency);
            display_name(w, e);
            w.bool(false); // no profile key
        })]);
    }
    let mut out = Vec::new();
    if actions & UPDATE_GAME_MODE != 0 {
        out.push(packet(1, &|w, e| {
            w.varint(e.game_mode);
        }));
    }
    if actions & UPDATE_LATENCY != 0 {
        out.push(packet(2, &|w, e| {
            w.varint(e.latency);
        }));
    }
    if actions & UPDATE_DISPLAY_NAME != 0 {
        out.push(packet(3, &display_name));
    }
    Ok(out)
}

/// PLAYER_INFO_REMOVE → PLAYER_INFO with the "remove player" action.
pub fn player_info_remove(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let players = r.list(Reader::uuid)?;
    let mut w = Writer::with_capacity(payload.len() + 1);
    w.varint(4).varint(players.len() as i32);
    for uuid in players {
        w.uuid(uuid);
    }
    Ok(w.into_inner())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn s(v: &str) -> Tag {
        Tag::String(v.into())
    }

    fn chat_type(id: i32, name: &str, chat: Option<Tag>) -> Tag {
        let mut element = Tag::compound();
        if let Some(chat) = chat {
            element.insert("chat", chat);
        }
        element.insert("narration", Tag::Compound(vec![("translation_key".into(), s("chat.type.text.narrate"))]));
        Tag::Compound(vec![("name".into(), s(name)), ("id".into(), Tag::Int(id)), ("element".into(), element)])
    }

    /// A registry codec with three chat types: plain chat, an outgoing whisper, and one that is
    /// only narrated.
    pub fn registries() -> Tag {
        let chat = Tag::Compound(vec![("translation_key".into(), s("chat.type.text")), ("parameters".into(), Tag::List(vec![s("sender"), s("content")]))]);
        let style = Tag::Compound(vec![("color".into(), s("gray")), ("italic".into(), Tag::Byte(1))]);
        let whisper = Tag::Compound(vec![
            ("translation_key".into(), s("commands.message.display.outgoing")),
            ("style".into(), style),
            ("parameters".into(), Tag::List(vec![s("target"), s("content")])),
        ]);
        let types = Tag::List(vec![chat_type(0, "minecraft:chat", Some(chat)), chat_type(3, "minecraft:msg_command_outgoing", Some(whisper)), chat_type(5, "minecraft:silent", None)]);
        Tag::Compound(vec![("minecraft:chat_type".into(), Tag::Compound(vec![("type".into(), s("minecraft:chat_type")), ("value".into(), types)]))])
    }

    pub fn types() -> ChatTypes {
        let mut t = ChatTypes::default();
        t.load(&registries());
        t
    }

    /// A signed 1.19.3 PLAYER_CHAT.
    pub fn signed_chat(plain: &str, unsigned: Option<&str>, chat_type: i32, target: Option<&str>) -> Vec<u8> {
        let mut w = Writer::new();
        w.uuid(7).varint(3).bool(true).raw(&[0xaa; SIGNATURE]);
        w.string(plain).i64(1_700_000_000_000).i64(99);
        w.varint(2).varint(5).varint(0).raw(&[0xbb; SIGNATURE]); // seen: one by index, one in full
        w.option(unsigned, |w, s| {
            w.string(s);
        });
        w.varint(2).long_array(&[1]); // partly filtered
        w.varint(chat_type).string(r#"{"text":"Alex"}"#);
        w.option(target, |w, s| {
            w.string(s);
        });
        w.into_inner()
    }

    fn shown(payload: &[u8]) -> Value {
        let mut r = Reader::new(payload);
        let v = serde_json::from_str(r.string().unwrap()).unwrap();
        assert!(!r.bool().unwrap(), "chat window, not the action bar");
        assert!(r.is_empty());
        v
    }

    #[test]
    fn player_chat_is_decorated_from_the_chat_type() {
        let types = types();
        let out = player_chat(&types, &signed_chat("hello", None, 0, None)).unwrap().unwrap();
        assert_eq!(shown(&out), json!({"translate": "chat.type.text", "with": [{"text": "Alex"}, {"text": "hello"}]}));
        // the server's formatted version wins over the plain text; the style comes from the type
        let out = player_chat(&types, &signed_chat("hi", Some(r#"{"text":"hi","bold":true}"#), 3, Some(r#"{"text":"Steve"}"#))).unwrap().unwrap();
        assert_eq!(
            shown(&out),
            json!({"translate": "commands.message.display.outgoing", "color": "gray", "italic": true, "with": [{"text": "Steve"}, {"text": "hi", "bold": true}]})
        );
        // a type without a chat decoration is not shown; one the registry lacks still is
        assert_eq!(player_chat(&types, &signed_chat("x", None, 5, None)).unwrap(), None);
        let out = player_chat(&types, &signed_chat("x", None, 42, None)).unwrap().unwrap();
        assert_eq!(shown(&out), json!({"translate": "chat.type.text", "with": [{"text": "Alex"}, {"text": "x"}]}));
    }

    #[test]
    fn unsigned_player_chat_and_disguised_chat() {
        let types = types();
        let mut w = Writer::new();
        w.uuid(7).varint(0).bool(false).string("yo").i64(1).i64(2).varint(0).bool(false).varint(0);
        w.varint(0).string("\"Alex\"").bool(false);
        let out = player_chat(&types, &w.buf).unwrap().unwrap();
        assert_eq!(shown(&out), json!({"translate": "chat.type.text", "with": ["Alex", {"text": "yo"}]}));

        let mut w = Writer::new();
        w.string(r#"{"text":"from the console"}"#).varint(0).string(r#"{"text":"Server"}"#).bool(false);
        let out = disguised_chat(&types, &w.buf).unwrap().unwrap();
        assert_eq!(shown(&out), json!({"translate": "chat.type.text", "with": [{"text": "Server"}, {"text": "from the console"}]}));
    }

    #[test]
    fn malformed_chat_is_an_error() {
        let types = types();
        let good = signed_chat("hello", None, 0, None);
        for len in 0..good.len() {
            assert!(player_chat(&types, &good[..len]).is_err(), "cut at {len}");
        }
        let mut w = Writer::new();
        w.string("not json").varint(0).string("\"a\"").bool(false);
        assert!(disguised_chat(&types, &w.buf).is_err());
        // a seen-messages count far beyond the packet
        let mut w = Writer::new();
        w.uuid(7).varint(0).bool(false).string("yo").i64(1).i64(2).varint(i32::MAX);
        assert!(player_chat(&types, &w.buf).is_err());
    }

    #[test]
    fn chat_types_come_from_the_login_registry() {
        let mut w = Writer::new();
        w.i32(1).bool(false).u8(0).i8(-1).varint(1).string("minecraft:overworld");
        nbt::write_named(&mut w, &registries());
        w.string("minecraft:overworld").string("minecraft:overworld").i64(0);
        let tag = login_registries(&w.buf).unwrap().unwrap();
        let mut t = ChatTypes::default();
        t.load(&tag);
        assert_eq!(t.len(), 3);
        // a registry without chat types leaves none (chat then shows as plain chat)
        t.load(&Tag::compound());
        assert_eq!(t.len(), 0);
        assert!(login_registries(&[0, 0, 0, 1]).is_err());
    }

    /// A 1.19.1 CHAT as a client with a profile key sends it.
    pub fn old_chat(message: &str) -> Vec<u8> {
        let mut w = Writer::new();
        w.string(message).i64(1_700_000_000_000).i64(99).byte_array(&[0xcc; 256]).bool(false);
        w.varint(1).uuid(5).byte_array(&[0xdd; 256]); // last seen
        w.bool(false); // last received
        w.into_inner()
    }

    #[test]
    fn client_chat_goes_out_unsigned() {
        let out = chat_to_server(&old_chat("hello")).unwrap();
        let mut want = Writer::new();
        want.string("hello").i64(1_700_000_000_000).i64(99).bool(false).varint(0).raw(&[0, 0, 0]);
        assert_eq!(out, want.buf);

        let mut w = Writer::new();
        w.string("msg Steve hi").i64(5).i64(6).varint(1).string("message").byte_array(&[0xcc; 256]).bool(false).varint(0).bool(false);
        let out = chat_command_to_server(&w.buf).unwrap();
        let mut want = Writer::new();
        want.string("msg Steve hi").i64(5).i64(6).varint(0).varint(0).raw(&[0, 0, 0]);
        assert_eq!(out, want.buf);

        // too long, cut short, or not text
        let mut w = Writer::new();
        w.string(&"a".repeat(257 * 3 + 1)).i64(0).i64(0);
        assert!(chat_to_server(&w.buf).is_err());
        assert!(chat_command_to_server(&w.buf).is_err());
        assert!(chat_to_server(&[2, b'h', b'i', 0, 0]).is_err());
        assert!(chat_to_server(&[2, 0xff, 0xfe, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(chat_to_server(&[]).is_err());
    }

    #[test]
    fn added_players_get_everything_in_one_packet() {
        let mut p = Writer::new();
        p.u8(ADD_PLAYER | INITIALIZE_CHAT | UPDATE_GAME_MODE | UPDATE_LISTED | UPDATE_LATENCY | UPDATE_DISPLAY_NAME).varint(2);
        p.uuid(1).string("Alex").varint(1).string("textures").string("abc").bool(true).string("sig");
        p.bool(true).uuid(55).i64(123).byte_array(&[1, 2, 3]).byte_array(&[4, 5]); // chat session
        p.varint(1).bool(true).varint(42).bool(true).string("\"Lex\"");
        p.uuid(2).string("Steve").varint(0).bool(false).varint(0).bool(false).varint(7).bool(false);
        let out = player_info_update(&p.buf).unwrap();
        let mut want = Writer::new();
        want.varint(0).varint(2);
        want.uuid(1).string("Alex").varint(1).string("textures").string("abc").bool(true).string("sig");
        want.varint(1).varint(42).bool(true).string("\"Lex\"").bool(false);
        want.uuid(2).string("Steve").varint(0).varint(0).varint(7).bool(false).bool(false);
        assert_eq!(out, vec![want.buf]);
        // only the profile: the rest gets defaults
        let mut p = Writer::new();
        p.u8(ADD_PLAYER).varint(1).uuid(1).string("Alex").varint(0);
        let mut want = Writer::new();
        want.varint(0).varint(1).uuid(1).string("Alex").varint(0).varint(0).varint(0).bool(false).bool(false);
        assert_eq!(player_info_update(&p.buf).unwrap(), vec![want.buf]);
    }

    #[test]
    fn player_updates_become_one_packet_per_action() {
        let mut p = Writer::new();
        p.u8(INITIALIZE_CHAT | UPDATE_GAME_MODE | UPDATE_LISTED | UPDATE_LATENCY | UPDATE_DISPLAY_NAME).varint(1);
        p.uuid(1).bool(false).varint(3).bool(false).varint(120).bool(true).string("\"Lex\"");
        let out = player_info_update(&p.buf).unwrap();
        let mut mode = Writer::new();
        mode.varint(1).varint(1).uuid(1).varint(3);
        let mut latency = Writer::new();
        latency.varint(2).varint(1).uuid(1).varint(120);
        let mut name = Writer::new();
        name.varint(3).varint(1).uuid(1).bool(true).string("\"Lex\"");
        assert_eq!(out, vec![mode.buf, latency.buf, name.buf]);
        // actions 1.19.1 has no packet for
        let mut p = Writer::new();
        p.u8(UPDATE_LISTED).varint(1).uuid(1).bool(false);
        assert!(player_info_update(&p.buf).unwrap().is_empty());
        // a count beyond the packet
        let mut p = Writer::new();
        p.u8(UPDATE_LATENCY).varint(i32::MAX);
        assert!(player_info_update(&p.buf).is_err());

        let mut p = Writer::new();
        p.varint(2).uuid(1).uuid(2);
        let mut want = Writer::new();
        want.varint(4).varint(2).uuid(1).uuid(2);
        assert_eq!(player_info_remove(&p.buf).unwrap(), want.buf);
        assert!(player_info_remove(&[5, 0]).is_err());
    }
}

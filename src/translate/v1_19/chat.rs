//! Chat 1.19.1 ↔ 1.19.
//!
//! 1.19.1 chains every signed message to the ones the player saw before it; 1.19 signs each
//! message on its own. A signature made for one version can't be turned into the other, so the
//! 1.19 client's signatures are dropped and its messages go to the server unsigned. In the other
//! direction player chat is shown as a system message, decorated here with the chat type the
//! server announced in LOGIN (the 1.19 client never sees those types: it gets the 1.19 defaults).

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

/// Chat types of the 1.19 registry that SYSTEM_CHAT uses.
pub const SYSTEM: i32 = 1;
pub const GAME_INFO: i32 = 2;

/// Longest text component either version accepts, in characters.
const MAX_COMPONENT: usize = 262_144;
/// Longest chat message or command a client may send.
const MAX_MESSAGE: usize = 256;
/// Vanilla's limits for the signed arguments of a command.
const MAX_ARGUMENTS: i32 = 8;
const MAX_ARGUMENT_NAME: usize = 16;
/// How many signatures the "last seen" list holds (one per sender).
const LAST_SEEN: usize = 5;
/// The server expects to hear which messages arrived at least this often.
const ACK_EVERY: u32 = 64;

const COLORS: &[&str] = &[
    "black", "dark_blue", "dark_green", "dark_aqua", "dark_red", "dark_purple", "gold", "gray", "dark_gray", "blue", "green", "aqua", "red", "light_purple", "yellow", "white",
];
const DECORATIONS: &[&str] = &["bold", "italic", "underlined", "strikethrough", "obfuscated"];

/// How one chat type shows a message in the chat window: a translation with the sender, target
/// and content as arguments, in a style.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoration {
    key: String,
    style: Vec<(String, Value)>,
    parameters: Vec<String>,
}

impl Decoration {
    /// What vanilla uses for ordinary chat; stands in for a chat type the server never announced.
    fn plain() -> Self {
        Self { key: "chat.type.text".into(), style: Vec::new(), parameters: vec!["sender".into(), "content".into()] }
    }

    /// The "chat" part of a 1.19.1 chat type.
    fn from_nbt(chat: &Tag) -> Option<Self> {
        let key = chat.get("translation_key")?.as_str()?.to_string();
        let mut style = Vec::new();
        if let Some(s) = chat.get("style") {
            if let Some(color) = s.get("color").and_then(Tag::as_str) {
                if COLORS.contains(&color) {
                    style.push(("color".to_string(), Value::String(color.to_string())));
                }
            }
            for name in DECORATIONS {
                if let Some(v) = s.get(name).and_then(Tag::as_i32) {
                    style.push((name.to_string(), Value::Bool(v == 1)));
                }
            }
        }
        let parameters = match chat.get("parameters").and_then(Tag::as_list) {
            Some(list) => list.iter().filter_map(|t| t.as_str().map(str::to_string)).collect(),
            None => Vec::new(),
        };
        Some(Self { key, style, parameters })
    }

    fn apply(&self, sender: &Value, target: Option<&Value>, content: &Value) -> Value {
        let mut out = Map::new();
        out.insert("translate".into(), Value::String(self.key.clone()));
        for (k, v) in &self.style {
            out.insert(k.clone(), v.clone());
        }
        let mut with = Vec::new();
        for p in &self.parameters {
            match p.as_str() {
                "sender" => with.push(sender.clone()),
                "content" => with.push(content.clone()),
                // a missing target still takes its place, so the other arguments stay where
                // the translation expects them
                "target" => with.push(target.cloned().unwrap_or_else(|| Value::String(String::new()))),
                _ => {}
            }
        }
        if !with.is_empty() {
            out.insert("with".into(), Value::Array(with));
        }
        Value::Object(out)
    }
}

/// The server's chat types by id. `None` = the type shows nothing in the chat window.
#[derive(Debug, Default)]
pub struct ChatTypes {
    types: HashMap<i32, Option<Decoration>>,
}

impl ChatTypes {
    fn load(&mut self, registries: &Tag) {
        self.types.clear();
        let Some(list) = registries.get("minecraft:chat_type").and_then(|t| t.get("value")).and_then(Tag::as_list) else {
            return;
        };
        for entry in list {
            let Some(id) = entry.get("id").and_then(Tag::as_i32) else { continue };
            let decoration = entry.get("element").and_then(|e| e.get("chat")).and_then(Decoration::from_nbt);
            self.types.insert(id, decoration);
        }
    }

    /// The message as the chat window shows it, or `None` if this type isn't shown there.
    fn decorate(&self, id: i32, sender: &Value, target: Option<&Value>, content: &Value) -> Option<Value> {
        match self.types.get(&id) {
            Some(Some(d)) => Some(d.apply(sender, target, content)),
            Some(None) => None,
            // Losing a message is worse than showing it in the default look.
            None => Some(Decoration::plain().apply(sender, target, content)),
        }
    }
}

/// Signatures of the newest message from each of the last few senders, newest first: what a
/// 1.19.1 client reports as "last seen" with everything it sends.
#[derive(Debug, Default)]
pub struct Seen {
    entries: Vec<(u128, Vec<u8>)>,
    unacknowledged: u32,
}

impl Seen {
    fn add(&mut self, sender: u128, signature: &[u8]) {
        let mut push = (sender, signature.to_vec());
        for slot in self.entries.iter_mut() {
            std::mem::swap(slot, &mut push);
            if push.0 == sender {
                return; // the sender's older message drops out
            }
        }
        if self.entries.len() < LAST_SEEN {
            self.entries.push(push);
        }
    }

    /// The "last seen" update that ends chat packets: the list, and no "last received".
    fn write(&self, w: &mut Writer) {
        w.varint(self.entries.len() as i32);
        for (sender, signature) in &self.entries {
            w.uuid(*sender).byte_array(signature);
        }
        w.bool(false);
    }
}

fn other(msg: impl Into<String>) -> DecodeError {
    DecodeError::Other(msg.into())
}

fn component(r: &mut Reader) -> Result<Value> {
    serde_json::from_str(r.string_max(MAX_COMPONENT)?).map_err(|e| other(format!("text component: {e}")))
}

fn compound(entries: Vec<(&str, Tag)>) -> Tag {
    Tag::Compound(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn string(s: &str) -> Tag {
    Tag::String(s.to_string())
}

fn decoration_1_19(key: &str, style: Tag, parameters: &[&str]) -> Tag {
    compound(vec![("translation_key", string(key)), ("style", style), ("parameters", Tag::List(parameters.iter().map(|p| string(p)).collect()))])
}

/// The chat type registry a vanilla 1.19 server sends. Only "system" and "game_info" are used
/// (by SYSTEM_CHAT); the rest keeps the ids where a 1.19 client expects them.
pub fn registry_1_19() -> Tag {
    let narrate = || decoration_1_19("chat.type.text.narrate", Tag::compound(), &["sender", "content"]);
    let narration = |decoration: Option<Tag>, priority: &str| {
        let mut entries = vec![("priority", string(priority))];
        entries.extend(decoration.map(|d| ("decoration", d)));
        compound(entries)
    };
    let chat = |decoration: Tag| compound(vec![("decoration", decoration)]);
    let gray_italic = compound(vec![("color", string("gray")), ("italic", Tag::Byte(1))]);
    let elements = vec![
        (
            "minecraft:chat",
            compound(vec![("chat", chat(decoration_1_19("chat.type.text", Tag::compound(), &["sender", "content"]))), ("narration", narration(Some(narrate()), "chat"))]),
        ),
        ("minecraft:system", compound(vec![("chat", Tag::compound()), ("narration", narration(None, "system"))])),
        ("minecraft:game_info", compound(vec![("overlay", Tag::compound())])),
        (
            "minecraft:say_command",
            compound(vec![("chat", chat(decoration_1_19("chat.type.announcement", Tag::compound(), &["sender", "content"]))), ("narration", narration(Some(narrate()), "chat"))]),
        ),
        (
            "minecraft:msg_command",
            compound(vec![
                ("chat", chat(decoration_1_19("commands.message.display.incoming", gray_italic, &["sender", "content"]))),
                ("narration", narration(Some(narrate()), "chat")),
            ]),
        ),
        (
            "minecraft:team_msg_command",
            compound(vec![
                ("chat", chat(decoration_1_19("chat.type.team.text", Tag::compound(), &["team_name", "sender", "content"]))),
                ("narration", narration(Some(narrate()), "chat")),
            ]),
        ),
        (
            "minecraft:emote_command",
            compound(vec![
                ("chat", chat(decoration_1_19("chat.type.emote", Tag::compound(), &["sender", "content"]))),
                ("narration", narration(Some(decoration_1_19("chat.type.emote", Tag::compound(), &["sender", "content"])), "chat")),
            ]),
        ),
        ("minecraft:tellraw_command", compound(vec![("chat", Tag::compound()), ("narration", narration(None, "chat"))])),
    ];
    let value = elements.into_iter().enumerate().map(|(id, (name, element))| compound(vec![("name", string(name)), ("id", Tag::Int(id as i32)), ("element", element)])).collect();
    compound(vec![("type", string("minecraft:chat_type")), ("value", Tag::List(value))])
}

/// LOGIN: remembers the server's chat types and gives the client the 1.19 ones, whose layout
/// differs (1.19.1 dropped the "decoration" level and the overlay / system types).
pub fn login(p: &[u8], types: &mut ChatTypes) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 1024);
    r.copy_to(&mut w, |r| {
        r.i32()?; // entity id
        r.bool()?; // hardcore
        r.u8()?; // game mode
        r.i8()?; // previous game mode
        r.list(|r| r.string().map(|_| ()))?; // worlds
        Ok(())
    })?;
    match nbt::read_named_optional(&mut r)? {
        Some(mut registries) => {
            types.load(&registries);
            registries.insert("minecraft:chat_type", registry_1_19());
            nbt::write_named(&mut w, &registries);
        }
        None => {
            types.load(&Tag::End);
            w.u8(0);
        }
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// What became of a PLAYER_CHAT.
pub struct PlayerChat {
    /// The 1.19 SYSTEM_CHAT payload, or `None` if the chat type isn't shown in the chat window.
    pub system_chat: Option<Vec<u8>>,
    /// A CHAT_ACK payload to send to the server, when one is due.
    pub ack: Option<Vec<u8>>,
}

/// PLAYER_CHAT → SYSTEM_CHAT. 1.19's own PLAYER_CHAT wants a signature in the 1.19 scheme, which
/// nobody can make, so the message is decorated here and shown as a system message.
pub fn player_chat(p: &[u8], types: &ChatTypes, seen: &mut Seen) -> Result<PlayerChat> {
    let mut r = Reader::new(p);
    r.option(Reader::byte_array)?; // previous signature
    let sender = r.uuid()?;
    let signature = r.byte_array()?;
    let plain = r.string_max(MAX_COMPONENT)?;
    let decorated = r.option(component)?;
    r.i64()?; // timestamp
    r.i64()?; // salt
    r.list(|r| {
        r.uuid()?;
        r.byte_array().map(|_| ())
    })?; // last seen
    let unsigned = r.option(component)?;
    if r.varint()? == 2 {
        r.long_array()?; // partially filtered: the mask
    }
    let chat_type = r.varint()?;
    let sender_name = component(&mut r)?;
    let target_name = r.option(component)?;

    // A real client tells the server now and then which signed messages reached it; the server
    // disconnects players who stay silent while messages pile up.
    let mut ack = None;
    if sender != 0 && !signature.is_empty() {
        seen.add(sender, signature);
        seen.unacknowledged += 1;
        if seen.unacknowledged > ACK_EVERY {
            seen.unacknowledged = 0;
            let mut w = Writer::new();
            seen.write(&mut w);
            ack = Some(w.into_inner());
        }
    }

    // the server's unsigned version wins, then its decorated one, then the plain text
    let content = unsigned.or(decorated).unwrap_or_else(|| {
        let mut text = Map::new();
        text.insert("text".into(), Value::String(plain.to_string()));
        Value::Object(text)
    });
    let system_chat = types.decorate(chat_type, &sender_name, target_name.as_ref(), &content).map(|shown| {
        let mut w = Writer::new();
        w.string(&shown.to_string()).varint(SYSTEM);
        w.into_inner()
    });
    Ok(PlayerChat { system_chat, ack })
}

/// SYSTEM_CHAT: 1.19 names a chat type where 1.19.1 has an "action bar" flag.
pub fn system_chat(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    r.copy_to(&mut w, |r| r.string_max(MAX_COMPONENT).map(|_| ()))?;
    w.varint(if r.bool()? { GAME_INFO } else { SYSTEM });
    Ok(w.into_inner())
}

/// SERVER_DATA: 1.19 has no "enforces secure chat" flag at the end.
pub fn server_data(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    r.copy_to(&mut w, |r| {
        r.option(|r| r.string_max(MAX_COMPONENT).map(|_| ()))?; // motd
        r.option(|r| r.string().map(|_| ()))?; // icon
        r.bool()?; // previews chat
        Ok(())
    })?;
    r.bool()?; // enforces secure chat
    Ok(w.into_inner())
}

/// Serverbound CHAT: the message goes on unsigned, with the "last seen" list 1.19.1 adds.
pub fn chat(p: &[u8], seen: &mut Seen) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 8);
    w.string(r.string_max(MAX_MESSAGE)?);
    w.i64(r.i64()?); // timestamp
    w.i64(r.i64()?); // salt
    r.byte_array()?; // the 1.19 signature, useless to a 1.19.1 server
    r.bool()?; // signed preview
    w.byte_array(&[]).bool(false);
    seen.unacknowledged = 0;
    seen.write(&mut w);
    Ok(w.into_inner())
}

/// Serverbound CHAT_COMMAND: argument signatures are emptied, and the "last seen" list added.
pub fn chat_command(p: &[u8], seen: &mut Seen) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 8);
    w.string(r.string_max(MAX_MESSAGE)?);
    w.i64(r.i64()?); // timestamp
    w.i64(r.i64()?); // salt
    let arguments = r.varint()?;
    if !(0..=MAX_ARGUMENTS).contains(&arguments) {
        return Err(other(format!("{arguments} signed command arguments")));
    }
    w.varint(arguments);
    for _ in 0..arguments {
        w.string(r.string_max(MAX_ARGUMENT_NAME)?);
        r.byte_array()?;
        w.byte_array(&[]);
    }
    w.bool(r.bool()?); // signed preview
    seen.unacknowledged = 0;
    seen.write(&mut w);
    Ok(w.into_inner())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    /// The chat types a vanilla 1.19.1 server sends (the parts this step reads).
    pub fn registry_1_19_1() -> Tag {
        let chat = |key: &str, style: Option<Tag>, parameters: &[&str]| {
            let mut entries = vec![("translation_key", string(key))];
            entries.extend(style.map(|s| ("style", s)));
            entries.push(("parameters", Tag::List(parameters.iter().map(|p| string(p)).collect())));
            compound(entries)
        };
        let gray_italic = || compound(vec![("color", string("gray")), ("italic", Tag::Byte(1))]);
        let types = vec![
            ("minecraft:chat", chat("chat.type.text", None, &["sender", "content"])),
            ("minecraft:say_command", chat("chat.type.announcement", None, &["sender", "content"])),
            ("minecraft:msg_command_incoming", chat("commands.message.display.incoming", Some(gray_italic()), &["sender", "content"])),
            ("minecraft:msg_command_outgoing", chat("commands.message.display.outgoing", Some(gray_italic()), &["target", "content"])),
            ("minecraft:team_msg_command_incoming", chat("chat.type.team.text", None, &["target", "sender", "content"])),
            ("minecraft:team_msg_command_outgoing", chat("chat.type.team.sent", None, &["target", "sender", "content"])),
            ("minecraft:emote_command", chat("chat.type.emote", None, &["sender", "content"])),
        ];
        let mut value: Vec<Tag> = types
            .into_iter()
            .enumerate()
            .map(|(id, (name, chat))| compound(vec![("name", string(name)), ("id", Tag::Int(id as i32)), ("element", compound(vec![("chat", chat), ("narration", Tag::compound())]))]))
            .collect();
        // a type with nothing for the chat window
        value.push(compound(vec![("name", string("test:narration_only")), ("id", Tag::Int(7)), ("element", compound(vec![("narration", Tag::compound())]))]));
        compound(vec![
            ("minecraft:dimension_type", compound(vec![("type", string("minecraft:dimension_type")), ("value", Tag::List(Vec::new()))])),
            ("minecraft:chat_type", compound(vec![("type", string("minecraft:chat_type")), ("value", Tag::List(value))])),
        ])
    }

    pub fn server_login(registries: &Tag) -> Vec<u8> {
        let mut w = Writer::new();
        w.i32(42).bool(false).u8(1).i8(-1);
        w.varint(1).string("minecraft:overworld");
        nbt::write_named(&mut w, registries);
        w.string("minecraft:overworld").string("minecraft:overworld").i64(1234);
        w.varint(20).varint(10).varint(8).bool(false).bool(true).bool(false).bool(true);
        w.bool(false); // no death location
        w.into_inner()
    }

    pub fn types() -> ChatTypes {
        let mut t = ChatTypes::default();
        t.load(&registry_1_19_1());
        t
    }

    pub struct Chat<'a> {
        pub sender: u128,
        pub signature: &'a [u8],
        pub plain: &'a str,
        pub decorated: Option<&'a str>,
        pub unsigned: Option<&'a str>,
        pub filter_mask: Option<&'a [i64]>,
        pub chat_type: i32,
        pub sender_name: &'a str,
        pub target_name: Option<&'a str>,
    }

    impl Default for Chat<'_> {
        fn default() -> Self {
            Self { sender: 7, signature: &[1, 2, 3], plain: "hi", decorated: None, unsigned: None, filter_mask: None, chat_type: 0, sender_name: "{\"text\":\"Steve\"}", target_name: None }
        }
    }

    /// A 1.19.1 PLAYER_CHAT as the server sends it.
    pub fn player_chat_packet(c: &Chat) -> Vec<u8> {
        let mut w = Writer::new();
        w.bool(true).byte_array(&[9; 4]); // previous signature
        w.uuid(c.sender).byte_array(c.signature);
        w.string(c.plain);
        w.option(c.decorated, |w, s| {
            w.string(s);
        });
        w.i64(1000).i64(5);
        w.varint(1).uuid(3).byte_array(&[4; 8]); // last seen
        w.option(c.unsigned, |w, s| {
            w.string(s);
        });
        match c.filter_mask {
            Some(mask) => {
                w.varint(2).long_array(mask);
            }
            None => {
                w.varint(0);
            }
        }
        w.varint(c.chat_type).string(c.sender_name);
        w.option(c.target_name, |w, s| {
            w.string(s);
        });
        w.into_inner()
    }

    fn shown(c: &Chat) -> Option<(Value, i32)> {
        let out = player_chat(&player_chat_packet(c), &types(), &mut Seen::default()).unwrap();
        out.system_chat.map(|p| {
            let mut r = Reader::new(&p);
            let json = serde_json::from_str(r.string_max(MAX_COMPONENT).unwrap()).unwrap();
            let ty = r.varint().unwrap();
            assert!(r.is_empty());
            (json, ty)
        })
    }

    #[test]
    fn login_swaps_the_chat_registry_and_keeps_the_rest() {
        let registries = registry_1_19_1();
        let mut types = ChatTypes::default();
        let out = login(&server_login(&registries), &mut types).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!((r.i32().unwrap(), r.bool().unwrap(), r.u8().unwrap(), r.i8().unwrap()), (42, false, 1, -1));
        assert_eq!(r.list(|r| r.string().map(str::to_string)).unwrap(), vec!["minecraft:overworld"]);
        let sent = nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert_eq!(sent.get("minecraft:dimension_type"), registries.get("minecraft:dimension_type"));
        assert_eq!(sent.get("minecraft:chat_type"), Some(&registry_1_19()));
        assert_eq!(r.string().unwrap(), "minecraft:overworld");
        assert_eq!(r.string().unwrap(), "minecraft:overworld");
        assert_eq!(r.i64().unwrap(), 1234);
        assert_eq!((r.varint().unwrap(), r.varint().unwrap(), r.varint().unwrap()), (20, 10, 8));
        assert_eq!((r.bool().unwrap(), r.bool().unwrap(), r.bool().unwrap(), r.bool().unwrap(), r.bool().unwrap()), (false, true, false, true, false));
        assert!(r.is_empty());
        assert_eq!(types.types.len(), 8);
        assert_eq!(types.types.get(&7), Some(&None));
    }

    /// SYSTEM_CHAT's type ids point at "system" and "game_info" in the registry the client got.
    #[test]
    fn the_1_19_registry_has_system_and_game_info_where_system_chat_expects_them() {
        let registry = registry_1_19();
        let list = registry.get("value").and_then(Tag::as_list).unwrap();
        assert_eq!(list.len(), 8);
        let name_of = |id: i32| list.iter().find(|e| e.get("id").and_then(Tag::as_i32) == Some(id)).and_then(|e| e.get("name")).and_then(Tag::as_str).map(str::to_string);
        assert_eq!(name_of(SYSTEM).as_deref(), Some("minecraft:system"));
        assert_eq!(name_of(GAME_INFO).as_deref(), Some("minecraft:game_info"));
        let game_info = list.iter().find(|e| e.get("id").and_then(Tag::as_i32) == Some(GAME_INFO)).unwrap();
        assert!(game_info.get("element").and_then(|e| e.get("overlay")).is_some());
    }

    #[test]
    fn player_chat_is_decorated_with_its_chat_type() {
        let (json, ty) = shown(&Chat::default()).unwrap();
        assert_eq!(json, json!({"translate": "chat.type.text", "with": [{"text": "Steve"}, {"text": "hi"}]}));
        assert_eq!(ty, SYSTEM);
    }

    #[test]
    fn unsigned_content_wins_over_decorated_and_plain() {
        let (json, _) = shown(&Chat { decorated: Some("{\"text\":\"decorated\"}"), ..Chat::default() }).unwrap();
        assert_eq!(json["with"][1], json!({"text": "decorated"}));
        let (json, _) = shown(&Chat { decorated: Some("{\"text\":\"decorated\"}"), unsigned: Some("\"unsigned\""), filter_mask: Some(&[1, 2]), ..Chat::default() }).unwrap();
        assert_eq!(json["with"][1], json!("unsigned"));
    }

    #[test]
    fn style_and_target_come_from_the_chat_type() {
        let (json, _) = shown(&Chat { chat_type: 3, target_name: Some("{\"text\":\"Alex\"}"), ..Chat::default() }).unwrap();
        assert_eq!(json, json!({"translate": "commands.message.display.outgoing", "color": "gray", "italic": true, "with": [{"text": "Alex"}, {"text": "hi"}]}));
        let (json, _) = shown(&Chat { chat_type: 4, target_name: Some("\"Red\""), ..Chat::default() }).unwrap();
        assert_eq!(json, json!({"translate": "chat.type.team.text", "with": ["Red", {"text": "Steve"}, {"text": "hi"}]}));
        // a target the server left out keeps its place
        let (json, _) = shown(&Chat { chat_type: 4, ..Chat::default() }).unwrap();
        assert_eq!(json["with"], json!(["", {"text": "Steve"}, {"text": "hi"}]));
    }

    #[test]
    fn types_without_a_chat_part_are_dropped_and_unknown_ones_use_the_default() {
        assert!(shown(&Chat { chat_type: 7, ..Chat::default() }).is_none());
        let (json, _) = shown(&Chat { chat_type: 99, ..Chat::default() }).unwrap();
        assert_eq!(json["translate"], json!("chat.type.text"));
    }

    #[test]
    fn broken_components_are_refused() {
        assert!(player_chat(&player_chat_packet(&Chat { sender_name: "{not json", ..Chat::default() }), &types(), &mut Seen::default()).is_err());
        assert!(player_chat(&player_chat_packet(&Chat { unsigned: Some("["), ..Chat::default() }), &types(), &mut Seen::default()).is_err());
    }

    #[test]
    fn last_seen_keeps_the_newest_message_of_each_sender() {
        let mut seen = Seen::default();
        for (sender, sig) in [(1u128, 10u8), (2, 20), (1, 11), (3, 30), (4, 40), (5, 50), (6, 60), (2, 21)] {
            seen.add(sender, &[sig]);
        }
        let got: Vec<(u128, u8)> = seen.entries.iter().map(|(s, sig)| (*s, sig[0])).collect();
        assert_eq!(got, vec![(2, 21), (6, 60), (5, 50), (4, 40), (3, 30)]);
    }

    #[test]
    fn signed_messages_are_acknowledged_now_and_then() {
        let types = types();
        let mut seen = Seen::default();
        // unsigned messages and messages without a sender don't count
        for c in [Chat { signature: &[], ..Chat::default() }, Chat { sender: 0, ..Chat::default() }] {
            for _ in 0..200 {
                assert!(player_chat(&player_chat_packet(&c), &types, &mut seen).unwrap().ack.is_none());
            }
        }
        assert!(seen.entries.is_empty());
        let packet = player_chat_packet(&Chat::default());
        for i in 1..=ACK_EVERY {
            assert!(player_chat(&packet, &types, &mut seen).unwrap().ack.is_none(), "message {i}");
        }
        let ack = player_chat(&packet, &types, &mut seen).unwrap().ack.unwrap();
        let mut want = Writer::new();
        want.varint(1).uuid(7).byte_array(&[1, 2, 3]).bool(false);
        assert_eq!(ack, want.into_inner());
        assert!(player_chat(&packet, &types, &mut seen).unwrap().ack.is_none());
    }

    #[test]
    fn system_chat_flag_becomes_a_chat_type() {
        for (overlay, ty) in [(false, SYSTEM), (true, GAME_INFO)] {
            let mut w = Writer::new();
            w.string("{\"text\":\"hello\"}").bool(overlay);
            let mut want = Writer::new();
            want.string("{\"text\":\"hello\"}").varint(ty);
            assert_eq!(system_chat(&w.into_inner()).unwrap(), want.into_inner());
        }
        assert!(system_chat(&[2, b'h', b'i']).is_err());
    }

    #[test]
    fn server_data_loses_the_secure_chat_flag() {
        let mut w = Writer::new();
        w.bool(true).string("{\"text\":\"A server\"}").bool(false).bool(true);
        let mut want = w.buf.clone();
        w.bool(true);
        assert_eq!(server_data(&w.buf).unwrap(), want);
        want.pop();
        assert!(server_data(&want).is_err());
    }

    #[test]
    fn chat_goes_to_the_server_unsigned() {
        let mut seen = Seen::default();
        seen.add(7, &[1, 2, 3]);
        seen.unacknowledged = 9;
        let mut w = Writer::new();
        w.string("hello").i64(1000).i64(5).byte_array(&[8; 256]).bool(true);
        let out = chat(&w.into_inner(), &mut seen).unwrap();
        let mut want = Writer::new();
        want.string("hello").i64(1000).i64(5).varint(0).bool(false);
        want.varint(1).uuid(7).byte_array(&[1, 2, 3]).bool(false);
        assert_eq!(out, want.into_inner());
        assert_eq!(seen.unacknowledged, 0);
        // too long for any client to have typed
        let mut w = Writer::new();
        w.string(&"x".repeat(1000)).i64(0).i64(0).byte_array(&[]).bool(false);
        assert!(chat(&w.into_inner(), &mut seen).is_err());
    }

    #[test]
    fn command_signatures_are_emptied() {
        let mut seen = Seen::default();
        let mut w = Writer::new();
        w.string("say hi").i64(1000).i64(5).varint(1).string("message").byte_array(&[8; 256]).bool(false);
        let out = chat_command(&w.into_inner(), &mut seen).unwrap();
        let mut want = Writer::new();
        want.string("say hi").i64(1000).i64(5).varint(1).string("message").varint(0).bool(false);
        want.varint(0).bool(false);
        assert_eq!(out, want.into_inner());
        // a count no client sends is refused before anything is read for it
        let mut w = Writer::new();
        w.string("say hi").i64(1000).i64(5).varint(i32::MAX);
        assert!(chat_command(&w.into_inner(), &mut seen).is_err());
        let mut w = Writer::new();
        w.string("say hi").i64(1000).i64(5).varint(-1);
        assert!(chat_command(&w.into_inner(), &mut seen).is_err());
    }
}

//! Text components 1.20.3 → 1.20.2.
//!
//! 1.20.3 sends components on the wire as NBT; 1.20.2 reads JSON strings. NBT shapes: a string
//! tag is plain text, a list is a component followed by its siblings, a compound is a full
//! component, and a compound whose only key is "" wraps a value that had to share a list with
//! values of another tag type. Styles store booleans as bytes, and show_entity hovers store the
//! UUID as an int array where 1.20.2 wants a string.

use serde_json::{Map, Number, Value};

use crate::proto::buf::{uuid_to_string, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

const BOOLEAN_KEYS: &[&str] = &["bold", "italic", "underlined", "strikethrough", "obfuscated", "interpret"];
const CONTENT_KEYS: &[&str] = &["text", "translate", "score", "selector", "keybind", "nbt"];

/// Converts a 1.20.3 NBT text component to 1.20.2 JSON.
pub fn to_json(tag: &Tag) -> Value {
    component(tag)
}

pub fn to_json_string(tag: &Tag) -> String {
    to_json(tag).to_string()
}

/// Reads one NBT component and writes it as a JSON string.
pub fn copy_component(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let tag = nbt::read(r)?;
    w.string(&to_json_string(&tag));
    Ok(())
}

/// Bool-prefixed optional component.
pub fn copy_opt_component(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if present {
        copy_component(r, w)?;
    }
    Ok(())
}

fn unwrap(tag: &Tag) -> &Tag {
    match tag {
        Tag::Compound(entries) if entries.len() == 1 && entries[0].0.is_empty() => unwrap(&entries[0].1),
        t => t,
    }
}

fn component(tag: &Tag) -> Value {
    match unwrap(tag) {
        Tag::String(s) => Value::String(s.clone()),
        Tag::List(items) if items.is_empty() => Value::String(String::new()),
        Tag::List(items) => Value::Array(items.iter().map(component).collect()),
        Tag::Compound(entries) => compound(entries),
        other => Value::String(plain(other)),
    }
}

/// A number tag shown as text.
fn plain(tag: &Tag) -> String {
    match tag {
        Tag::Byte(v) => v.to_string(),
        Tag::Short(v) => v.to_string(),
        Tag::Int(v) => v.to_string(),
        Tag::Long(v) => v.to_string(),
        Tag::Float(v) => v.to_string(),
        Tag::Double(v) => v.to_string(),
        _ => String::new(),
    }
}

fn components(tag: &Tag, f: fn(&Tag) -> Value) -> Value {
    match tag {
        Tag::List(items) => Value::Array(items.iter().map(f).collect()),
        other => Value::Array(vec![f(other)]),
    }
}

fn compound(entries: &[(String, Tag)]) -> Value {
    let mut obj = Map::new();
    for (k, v) in entries {
        let value = match k.as_str() {
            // 1.20.2 picks the content type from the keys present
            "type" => continue,
            "extra" => components(v, component),
            "with" => components(v, argument),
            "separator" => component(v),
            "hoverEvent" => hover(v),
            _ => value(Some(k), v),
        };
        obj.insert(k.clone(), value);
    }
    if !CONTENT_KEYS.iter().any(|k| obj.contains_key(*k)) {
        obj.insert("text".into(), Value::String(String::new()));
    }
    Value::Object(obj)
}

/// Translation arguments may be plain numbers or booleans; 1.20.2 shows primitives as text.
fn argument(tag: &Tag) -> Value {
    match unwrap(tag) {
        t @ (Tag::Byte(_) | Tag::Short(_) | Tag::Int(_) | Tag::Long(_) | Tag::Float(_) | Tag::Double(_)) => value(None, t),
        t => component(t),
    }
}

fn hover(tag: &Tag) -> Value {
    let Tag::Compound(entries) = tag else { return value(None, tag) };
    let action = tag.get("action").and_then(Tag::as_str).unwrap_or("");
    let mut obj = Map::new();
    for (k, v) in entries {
        let value = match (k.as_str(), action) {
            ("contents", "show_text") | ("value", _) => component(v),
            ("contents", "show_entity") => entity_tooltip(v),
            ("contents", "show_item") => item_tooltip(v),
            _ => value(Some(k), v),
        };
        obj.insert(k.clone(), value);
    }
    Value::Object(obj)
}

fn entity_tooltip(tag: &Tag) -> Value {
    let Tag::Compound(entries) = tag else { return value(None, tag) };
    let mut obj = Map::new();
    for (k, v) in entries {
        let value = match (k.as_str(), v) {
            ("id", Tag::IntArray(a)) if a.len() == 4 => Value::String(uuid_to_string(int_array_uuid(a))),
            ("name", v) => component(v),
            (_, v) => value(Some(k), v),
        };
        obj.insert(k.clone(), value);
    }
    Value::Object(obj)
}

fn item_tooltip(tag: &Tag) -> Value {
    let Tag::Compound(entries) = tag else { return value(None, tag) };
    let mut obj = Map::new();
    for (k, v) in entries {
        let value = match (k.as_str(), v) {
            // 1.20.2 parses the item tag from an SNBT string
            ("tag", t @ Tag::Compound(_)) => Value::String(snbt(t)),
            (_, v) => value(Some(k), v),
        };
        obj.insert(k.clone(), value);
    }
    Value::Object(obj)
}

pub fn int_array_uuid(a: &[i32]) -> u128 {
    a.iter().fold(0u128, |acc, v| (acc << 32) | u128::from(*v as u32))
}

fn number(v: f64) -> Value {
    Number::from_f64(v).map_or(Value::from(0), Value::Number)
}

/// Any other value, as plain JSON (style booleans become JSON booleans).
fn value(key: Option<&str>, tag: &Tag) -> Value {
    match tag {
        Tag::End => Value::Null,
        Tag::Byte(b) if key.is_some_and(|k| BOOLEAN_KEYS.contains(&k)) => Value::Bool(*b != 0),
        Tag::Byte(v) => Value::from(*v),
        Tag::Short(v) => Value::from(*v),
        Tag::Int(v) => Value::from(*v),
        Tag::Long(v) => Value::from(*v),
        Tag::Float(v) => number(f64::from(*v)),
        Tag::Double(v) => number(*v),
        Tag::String(s) => Value::String(s.clone()),
        Tag::ByteArray(a) => Value::Array(a.iter().map(|v| Value::from(*v)).collect()),
        Tag::IntArray(a) => Value::Array(a.iter().map(|v| Value::from(*v)).collect()),
        Tag::LongArray(a) => Value::Array(a.iter().map(|v| Value::from(*v)).collect()),
        Tag::List(items) => Value::Array(items.iter().map(|t| value(None, t)).collect()),
        Tag::Compound(entries) => Value::Object(entries.iter().map(|(k, v)| (k.clone(), value(Some(k), v))).collect()),
    }
}

/// Stringified NBT, for show_item tags.
pub fn snbt(tag: &Tag) -> String {
    let mut out = String::new();
    write_snbt(tag, &mut out);
    out
}

fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
}

fn snbt_list<T: std::fmt::Display>(prefix: &str, items: &[T], suffix: &str, out: &mut String) {
    out.push('[');
    out.push_str(prefix);
    for (i, v) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{v}{suffix}"));
    }
    out.push(']');
}

fn write_snbt(tag: &Tag, out: &mut String) {
    match tag {
        Tag::End => {}
        Tag::Byte(v) => out.push_str(&format!("{v}b")),
        Tag::Short(v) => out.push_str(&format!("{v}s")),
        Tag::Int(v) => out.push_str(&v.to_string()),
        Tag::Long(v) => out.push_str(&format!("{v}L")),
        Tag::Float(v) => out.push_str(&format!("{v}f")),
        Tag::Double(v) => out.push_str(&format!("{v}d")),
        Tag::String(s) => quote(s, out),
        Tag::ByteArray(a) => snbt_list("B;", a, "b", out),
        Tag::IntArray(a) => snbt_list("I;", a, "", out),
        Tag::LongArray(a) => snbt_list("L;", a, "L", out),
        Tag::List(items) => {
            out.push('[');
            for (i, t) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_snbt(t, out);
            }
            out.push(']');
        }
        Tag::Compound(entries) => {
            out.push('{');
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                quote(k, out);
                out.push(':');
                write_snbt(v, out);
            }
            out.push('}');
        }
    }
}

/// JSON text that 1.20.3 wrote into NBT (item names, lore, block entity names): only
/// show_entity UUIDs differ. `None` = already valid for 1.20.2.
pub fn fix_json(s: &str) -> Option<String> {
    if !s.contains("show_entity") {
        return None;
    }
    let mut v: Value = serde_json::from_str(s).ok()?;
    let mut changed = false;
    fix_entity_ids(&mut v, &mut changed);
    changed.then(|| v.to_string())
}

fn fix_entity_ids(v: &mut Value, changed: &mut bool) {
    match v {
        Value::Array(items) => items.iter_mut().for_each(|x| fix_entity_ids(x, changed)),
        Value::Object(obj) => {
            if obj.get("action").and_then(Value::as_str) == Some("show_entity") {
                if let Some(Value::Array(id)) = obj.get_mut("contents").and_then(|c| c.get_mut("id")) {
                    let parts: Vec<i32> = id.iter().filter_map(|x| x.as_i64()).map(|x| x as i32).collect();
                    if parts.len() == 4 {
                        let uuid = uuid_to_string(int_array_uuid(&parts));
                        obj["contents"]["id"] = Value::String(uuid);
                        *changed = true;
                    }
                }
            }
            obj.values_mut().for_each(|x| fix_entity_ids(x, changed));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn s(v: &str) -> Tag {
        Tag::String(v.into())
    }
    fn c(entries: Vec<(&str, Tag)>) -> Tag {
        Tag::Compound(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    #[test]
    fn plain_string() {
        assert_eq!(to_json(&s("hello")), json!("hello"));
        assert_eq!(to_json_string(&s("a\"b")), r#""a\"b""#);
    }

    #[test]
    fn compound_with_style_booleans() {
        let t = c(vec![("text", s("hi")), ("bold", Tag::Byte(1)), ("italic", Tag::Byte(0)), ("color", s("red")), ("type", s("text"))]);
        assert_eq!(to_json(&t), json!({"text": "hi", "bold": true, "italic": false, "color": "red"}));
    }

    #[test]
    fn nested_extra_and_with() {
        let t = c(vec![
            ("translate", s("chat.type.text")),
            ("with", Tag::List(vec![c(vec![("text", s("Steve")), ("underlined", Tag::Byte(1))]), c(vec![("", Tag::Int(5))]), c(vec![("", s("x"))])])),
            ("extra", Tag::List(vec![s("!"), c(vec![("keybind", s("key.jump"))])])),
        ]);
        assert_eq!(
            to_json(&t),
            json!({
                "translate": "chat.type.text",
                "with": [{"text": "Steve", "underlined": true}, 5, "x"],
                "extra": ["!", {"keybind": "key.jump"}]
            })
        );
    }

    #[test]
    fn lists_and_wrappers() {
        let t = Tag::List(vec![c(vec![("", s("a"))]), c(vec![("text", s("b")), ("obfuscated", Tag::Byte(1))])]);
        assert_eq!(to_json(&t), json!(["a", {"text": "b", "obfuscated": true}]));
        // style only: 1.20.2 needs a content key
        assert_eq!(to_json(&c(vec![("color", s("gold")), ("extra", Tag::List(vec![s("x")]))])), json!({"color": "gold", "text": "", "extra": ["x"]}));
        assert_eq!(to_json(&Tag::List(vec![])), json!(""));
    }

    #[test]
    fn hover_and_click_events() {
        let text_hover = c(vec![
            ("text", s("t")),
            ("clickEvent", c(vec![("action", s("run_command")), ("value", s("/spawn"))])),
            ("hoverEvent", c(vec![("action", s("show_text")), ("contents", c(vec![("text", s("tip")), ("bold", Tag::Byte(1))]))])),
        ]);
        assert_eq!(
            to_json(&text_hover),
            json!({"text": "t", "clickEvent": {"action": "run_command", "value": "/spawn"}, "hoverEvent": {"action": "show_text", "contents": {"text": "tip", "bold": true}}})
        );
        let item_hover = c(vec![
            ("text", s("i")),
            ("hoverEvent", c(vec![("action", s("show_item")), ("contents", c(vec![("id", s("minecraft:stone")), ("count", Tag::Int(2)), ("tag", c(vec![("Damage", Tag::Int(3))]))]))])),
        ]);
        assert_eq!(
            to_json(&item_hover)["hoverEvent"]["contents"],
            json!({"id": "minecraft:stone", "count": 2, "tag": "{\"Damage\":3}"})
        );
        let entity_hover = c(vec![
            ("text", s("e")),
            ("hoverEvent", c(vec![
                ("action", s("show_entity")),
                ("contents", c(vec![("type", s("minecraft:pig")), ("id", Tag::IntArray(vec![1, 2, 3, -1])), ("name", c(vec![("text", s("Pig")), ("italic", Tag::Byte(1))]))])),
            ])),
        ]);
        assert_eq!(
            to_json(&entity_hover)["hoverEvent"]["contents"],
            json!({"type": "minecraft:pig", "id": "00000001-0000-0002-0000-0003ffffffff", "name": {"text": "Pig", "italic": true}})
        );
    }

    #[test]
    fn score_and_nbt_components() {
        let t = c(vec![("score", c(vec![("name", s("@s")), ("objective", s("kills"))]))]);
        assert_eq!(to_json(&t), json!({"score": {"name": "@s", "objective": "kills"}}));
        let t = c(vec![("nbt", s("Pos")), ("entity", s("@s")), ("interpret", Tag::Byte(1)), ("separator", s(", "))]);
        assert_eq!(to_json(&t), json!({"nbt": "Pos", "entity": "@s", "interpret": true, "separator": ", "}));
    }

    #[test]
    fn json_in_nbt_gets_string_uuids() {
        let fixed = fix_json(r#"{"text":"x","hoverEvent":{"action":"show_entity","contents":{"type":"minecraft:pig","id":[0,0,0,1]}}}"#).unwrap();
        let v: Value = serde_json::from_str(&fixed).unwrap();
        assert_eq!(v["hoverEvent"]["contents"]["id"], json!("00000000-0000-0000-0000-000000000001"));
        assert_eq!(fix_json(r#"{"text":"plain"}"#), None);
    }

    #[test]
    fn wire_component() {
        let mut w = Writer::new();
        w.bool(true);
        nbt::write(&mut w, &s("hi"));
        w.u8(7);
        let mut r = Reader::new(&w.buf);
        let mut out = Writer::new();
        copy_opt_component(&mut r, &mut out).unwrap();
        assert_eq!(r.u8().unwrap(), 7);
        let mut r = Reader::new(&out.buf);
        assert!(r.bool().unwrap());
        assert_eq!(r.string().unwrap(), r#""hi""#);
    }
}

//! Text components 1.20.5 ↔ 1.20.3.
//!
//! Packets carry text as NBT in both versions; only item hovers changed (1.20.5 shows an item as
//! `{id, count, components}`, 1.20.3 as `{id, Count, tag}`). Item names, lore and book pages are
//! JSON strings inside 1.20.3 item NBT, so text is also converted to and from JSON here. Both
//! directions keep key order, so a converted name comes back byte for byte.

use super::data_1_20_3 as d;
use crate::proto::nbt::Tag;

fn strip_ns(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}

/// Style keys whose NBT byte is a JSON boolean.
const BOOLEAN_KEYS: [&str; 6] = ["bold", "italic", "underlined", "strikethrough", "obfuscated", "interpret"];

// ---- NBT → JSON ---------------------------------------------------------------------------------

/// A text component (NBT form) as a JSON string, the way 1.20.3 stores it in item NBT.
pub fn to_json(tag: &Tag) -> String {
    let mut out = String::new();
    write_json(tag, None, &mut out);
    out
}

fn write_json(tag: &Tag, key: Option<&str>, out: &mut String) {
    match tag {
        Tag::Byte(v) if key.is_some_and(|k| BOOLEAN_KEYS.contains(&k)) => out.push_str(if *v != 0 { "true" } else { "false" }),
        Tag::Byte(v) => out.push_str(&v.to_string()),
        Tag::Short(v) => out.push_str(&v.to_string()),
        Tag::Int(v) => out.push_str(&v.to_string()),
        Tag::Long(v) => out.push_str(&v.to_string()),
        Tag::Float(v) => out.push_str(&json_float(f64::from(*v))),
        Tag::Double(v) => out.push_str(&json_float(*v)),
        Tag::String(s) => write_json_string(s, out),
        Tag::List(items) => {
            out.push('[');
            for (i, t) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                // heterogeneous lists are stored as compounds with a single empty key
                match t {
                    Tag::Compound(e) if e.len() == 1 && e[0].0.is_empty() => write_json(&e[0].1, None, out),
                    _ => write_json(t, None, out),
                }
            }
            out.push(']');
        }
        Tag::Compound(entries) => {
            out.push('{');
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json_string(k, out);
                out.push(':');
                write_json(v, Some(k), out);
            }
            out.push('}');
        }
        Tag::ByteArray(v) => write_json_numbers(v.iter().map(|x| i64::from(*x)), out),
        Tag::IntArray(v) => write_json_numbers(v.iter().map(|x| i64::from(*x)), out),
        Tag::LongArray(v) => write_json_numbers(v.iter().copied(), out),
        Tag::End => out.push_str("null"),
    }
}

fn write_json_numbers(values: impl Iterator<Item = i64>, out: &mut String) {
    out.push('[');
    for (i, v) in values.enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&v.to_string());
    }
    out.push(']');
}

fn json_float(v: f64) -> String {
    if v.is_finite() { if v.fract() == 0.0 && v.abs() < 1e15 { format!("{v:.1}") } else { v.to_string() } } else { "0".into() }
}

fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

// ---- JSON → NBT ---------------------------------------------------------------------------------

/// A 1.20.3 JSON text component as NBT; plain text if the JSON does not parse (as vanilla's
/// lenient reader does for bare words).
pub fn from_json(json: &str) -> Tag {
    let mut p = Parser { s: json.as_bytes(), i: 0 };
    p.ws();
    match p.value() {
        Some(t) => {
            p.ws();
            if p.i == p.s.len() { t } else { Tag::String(json.to_string()) }
        }
        None => Tag::String(json.to_string()),
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn value(&mut self) -> Option<Tag> {
        self.ws();
        match *self.s.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut entries = Vec::new();
                if self.eat(b'}') {
                    return Some(Tag::Compound(entries));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    if !self.eat(b':') {
                        return None;
                    }
                    let v = self.value()?;
                    entries.push((k, v));
                    if self.eat(b',') {
                        continue;
                    }
                    return if self.eat(b'}') { Some(Tag::Compound(entries)) } else { None };
                }
            }
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                if !self.eat(b']') {
                    loop {
                        items.push(self.value()?);
                        if self.eat(b',') {
                            continue;
                        }
                        if self.eat(b']') {
                            break;
                        }
                        return None;
                    }
                }
                Some(list_of(items))
            }
            b'"' => self.string().map(Tag::String),
            b't' if self.s[self.i..].starts_with(b"true") => {
                self.i += 4;
                Some(Tag::Byte(1))
            }
            b'f' if self.s[self.i..].starts_with(b"false") => {
                self.i += 5;
                Some(Tag::Byte(0))
            }
            b'n' if self.s[self.i..].starts_with(b"null") => {
                self.i += 4;
                Some(Tag::String(String::new()))
            }
            _ => {
                let start = self.i;
                while self.i < self.s.len() && matches!(self.s[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                    self.i += 1;
                }
                let text = std::str::from_utf8(&self.s[start..self.i]).ok()?;
                if text.is_empty() {
                    return None;
                }
                if let Ok(v) = text.parse::<i32>() {
                    Some(Tag::Int(v))
                } else if let Ok(v) = text.parse::<i64>() {
                    Some(Tag::Long(v))
                } else {
                    text.parse::<f64>().ok().map(Tag::Double)
                }
            }
        }
    }
    fn string(&mut self) -> Option<String> {
        if self.s.get(self.i) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = *self.s.get(self.i)?;
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = *self.s.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => {
                            let hex = std::str::from_utf8(self.s.get(self.i..self.i + 4)?).ok()?;
                            self.i += 4;
                            let mut cp = u32::from_str_radix(hex, 16).ok()?;
                            if (0xD800..0xDC00).contains(&cp) && self.s.get(self.i..self.i + 2) == Some(b"\\u") {
                                let low = u32::from_str_radix(std::str::from_utf8(self.s.get(self.i + 2..self.i + 6)?).ok()?, 16).ok()?;
                                self.i += 6;
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3ff);
                            }
                            let ch = char::from_u32(cp).unwrap_or('\u{fffd}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }
}

/// A list; mixed element types are wrapped the way NBT text stores them.
fn list_of(items: Vec<Tag>) -> Tag {
    let same = items.windows(2).all(|w| w[0].type_id() == w[1].type_id());
    if same {
        return Tag::List(items);
    }
    Tag::List(
        items
            .into_iter()
            .map(|t| match t {
                Tag::Compound(_) => t,
                other => Tag::Compound(vec![(String::new(), other)]),
            })
            .collect(),
    )
}

// ---- hovers ---------------------------------------------------------------------------------------

/// Makes item and entity hovers readable for 1.20.3: `count` becomes `Count`, data components
/// (which 1.20.3 cannot read) are left out, and ids 1.20.3 lacks become their stand-ins.
pub fn fix_text(tag: &mut Tag) {
    match tag {
        Tag::Compound(entries) => {
            let action = entries.iter().find(|(k, _)| k == "action").and_then(|(_, v)| v.as_str()).map(|a| strip_ns(a).to_string());
            if let Some(action) = action {
                for (k, v) in entries.iter_mut() {
                    if k == "contents" {
                        match action.as_str() {
                            "show_item" => fix_hover_item(v),
                            "show_entity" => fix_hover_entity(v),
                            _ => {}
                        }
                    }
                }
            }
            for (_, v) in entries.iter_mut() {
                fix_text(v);
            }
        }
        Tag::List(list) => list.iter_mut().for_each(fix_text),
        _ => {}
    }
}

fn fix_hover_item(contents: &mut Tag) {
    if let Tag::String(id) = contents {
        *id = old_item_name(id);
        return;
    }
    if let Some(Tag::String(id)) = contents.get_mut("id") {
        *id = old_item_name(id);
    }
    if let Some(count) = contents.remove("count") {
        contents.insert("Count", Tag::Byte(count.as_i32().unwrap_or(1).clamp(1, 127) as i8));
    }
    contents.remove("components");
}

fn fix_hover_entity(contents: &mut Tag) {
    if let Some(Tag::String(ty)) = contents.get_mut("type") {
        let new = d::NEW_ENTITY_NAMES.iter().position(|n| *n == strip_ns(ty));
        if let Some(name) = new.and_then(|i| d::ENTITIES.get(i)).and_then(|o| d::OLD_ENTITY_NAMES.get(*o as usize)) {
            *ty = format!("minecraft:{name}");
        }
    }
}

fn old_item_name(id: &str) -> String {
    let new = d::NEW_ITEM_NAMES.iter().position(|n| *n == strip_ns(id));
    match new.and_then(|i| d::ITEMS.get(i)).and_then(|o| d::OLD_ITEM_NAMES.get(*o as usize)) {
        Some(name) => format!("minecraft:{name}"),
        None => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(text: &str) -> Tag {
        let mut t = Tag::compound();
        t.insert("text", Tag::String(text.into()));
        t.insert("italic", Tag::Byte(0));
        t.insert("color", Tag::String("gold".into()));
        t
    }

    #[test]
    fn json_round_trip_keeps_order_and_booleans() {
        let mut t = named("Excalibur \"the\" sword");
        // NBT text stores a mixed list with plain entries wrapped under an empty key
        t.insert("extra", Tag::List(vec![Tag::Compound(vec![(String::new(), Tag::String("a".into()))]), named("b")]));
        let json = to_json(&t);
        assert_eq!(json, r#"{"text":"Excalibur \"the\" sword","italic":false,"color":"gold","extra":["a",{"text":"b","italic":false,"color":"gold"}]}"#);
        assert_eq!(from_json(&json), t);
        assert_eq!(from_json(&to_json(&named("x"))), named("x"));
        assert_eq!(from_json("\"plain\""), Tag::String("plain".into()));
        assert_eq!(from_json("not json"), Tag::String("not json".into()));
        assert_eq!(from_json(r#"{"text":"é\n"}"#).get("text"), Some(&Tag::String("é\n".into())));
    }

    #[test]
    fn item_hover_is_made_legacy() {
        let mut contents = Tag::compound();
        contents.insert("id", Tag::String("minecraft:mace".into()));
        contents.insert("count", Tag::Int(3));
        contents.insert("components", Tag::compound());
        let mut hover = Tag::compound();
        hover.insert("action", Tag::String("show_item".into()));
        hover.insert("contents", contents);
        let mut text = named("x");
        text.insert("hoverEvent", hover);
        fix_text(&mut text);
        let c = text.get("hoverEvent").unwrap().get("contents").unwrap();
        assert_eq!(c.get("id"), Some(&Tag::String("minecraft:netherite_axe".into())));
        assert_eq!(c.get("Count"), Some(&Tag::Byte(3)));
        assert!(c.get("components").is_none() && c.get("count").is_none());
    }
}

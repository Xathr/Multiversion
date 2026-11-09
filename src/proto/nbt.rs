//! Network NBT (1.20.2+: root compound has no name). Enough to read, rewrite and write back
//! registry data, text components and item component payloads.

use super::buf::{DecodeError, Reader, Result, Writer};

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    pub fn type_id(&self) -> u8 {
        match self {
            Tag::End => 0,
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(_) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }
    pub fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Tag> {
        match self {
            Tag::Compound(entries) => entries.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn remove(&mut self, key: &str) -> Option<Tag> {
        match self {
            Tag::Compound(entries) => {
                let idx = entries.iter().position(|(k, _)| k == key)?;
                Some(entries.remove(idx).1)
            }
            _ => None,
        }
    }
    pub fn insert(&mut self, key: &str, value: Tag) {
        if let Tag::Compound(entries) = self {
            if let Some(slot) = entries.iter_mut().find(|(k, _)| k == key) {
                slot.1 = value;
            } else {
                entries.push((key.to_string(), value));
            }
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Tag::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Tag::Byte(v) => Some(i32::from(*v)),
            Tag::Short(v) => Some(i32::from(*v)),
            Tag::Int(v) => Some(*v),
            Tag::Long(v) => Some(*v as i32),
            _ => None,
        }
    }
    pub fn as_list(&self) -> Option<&Vec<Tag>> {
        match self {
            Tag::List(l) => Some(l),
            _ => None,
        }
    }
    pub fn as_list_mut(&mut self) -> Option<&mut Vec<Tag>> {
        match self {
            Tag::List(l) => Some(l),
            _ => None,
        }
    }
    pub fn compound() -> Tag {
        Tag::Compound(Vec::new())
    }
}

/// Reads a network NBT value: type byte, then (for non-End) the payload. Root has no name.
pub fn read(r: &mut Reader) -> Result<Tag> {
    let ty = r.u8()?;
    read_payload(r, ty, 0, &mut MAX_TAGS.clone())
}

/// The most tags one NBT value may hold. A tag takes far more memory than its bytes on the wire
/// (a list of single bytes grows about 20 times), so without a cap one packet of a few megabytes
/// could exhaust the plugin's memory. Vanilla's own registry data is well under a tenth of this.
pub const MAX_TAGS: usize = 1 << 20;
/// How deep compounds and lists may nest (vanilla's limit).
pub const MAX_DEPTH: u32 = 512;

/// Reads an optional network NBT (End tag = none).
pub fn read_optional(r: &mut Reader) -> Result<Option<Tag>> {
    let ty = r.u8()?;
    if ty == 0 {
        return Ok(None);
    }
    read_payload(r, ty, 0, &mut MAX_TAGS.clone()).map(Some)
}

fn read_string(r: &mut Reader) -> Result<String> {
    let len = r.u16()? as usize;
    let bytes = r.bytes(len)?;
    // Java modified UTF-8 is close enough to UTF-8 for registry/text data; be lenient.
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn read_payload(r: &mut Reader, ty: u8, depth: u32, budget: &mut usize) -> Result<Tag> {
    if depth > MAX_DEPTH {
        return Err(DecodeError::Other("nbt too deep".into()));
    }
    *budget = budget.checked_sub(1).ok_or_else(|| DecodeError::Other("nbt has too many tags".into()))?;
    Ok(match ty {
        0 => Tag::End,
        1 => Tag::Byte(r.i8()?),
        2 => Tag::Short(r.i16()?),
        3 => Tag::Int(r.i32()?),
        4 => Tag::Long(r.i64()?),
        5 => Tag::Float(r.f32()?),
        6 => Tag::Double(r.f64()?),
        7 => {
            let len = r.i32()?.max(0) as usize;
            Tag::ByteArray(r.bytes(len)?.iter().map(|b| *b as i8).collect())
        }
        8 => Tag::String(read_string(r)?),
        9 => {
            let elem = r.u8()?;
            let len = r.i32()?.max(0) as usize;
            // Every element of a real list takes at least one byte. A list of End tags takes
            // none, so its length says nothing about its size: only the empty one is valid.
            if len > r.len() || (elem == 0 && len > 0) {
                return Err(DecodeError::Other("nbt list longer than the packet".into()));
            }
            let mut v = Vec::with_capacity(len.min(1 << 12));
            for _ in 0..len {
                v.push(read_payload(r, elem, depth + 1, budget)?);
            }
            Tag::List(v)
        }
        10 => {
            let mut entries = Vec::new();
            loop {
                let t = r.u8()?;
                if t == 0 {
                    break;
                }
                let name = read_string(r)?;
                let value = read_payload(r, t, depth + 1, budget)?;
                entries.push((name, value));
            }
            Tag::Compound(entries)
        }
        11 => {
            let len = r.i32()?.max(0) as usize;
            Tag::IntArray((0..len).map(|_| r.i32()).collect::<Result<_>>()?)
        }
        12 => {
            let len = r.i32()?.max(0) as usize;
            Tag::LongArray((0..len).map(|_| r.i64()).collect::<Result<_>>()?)
        }
        other => return Err(DecodeError::Other(format!("unknown nbt tag type {other}"))),
    })
}

/// Writes a network NBT value (type byte + payload, no root name).
pub fn write(w: &mut Writer, tag: &Tag) {
    w.u8(tag.type_id());
    write_payload(w, tag);
}

pub fn write_optional(w: &mut Writer, tag: Option<&Tag>) {
    match tag {
        Some(t) => write(w, t),
        None => {
            w.u8(0);
        }
    }
}

fn write_string(w: &mut Writer, s: &str) {
    w.u16(s.len() as u16);
    w.raw(s.as_bytes());
}

fn write_payload(w: &mut Writer, tag: &Tag) {
    match tag {
        Tag::End => {}
        Tag::Byte(v) => {
            w.i8(*v);
        }
        Tag::Short(v) => {
            w.i16(*v);
        }
        Tag::Int(v) => {
            w.i32(*v);
        }
        Tag::Long(v) => {
            w.i64(*v);
        }
        Tag::Float(v) => {
            w.f32(*v);
        }
        Tag::Double(v) => {
            w.f64(*v);
        }
        Tag::ByteArray(v) => {
            w.i32(v.len() as i32);
            for b in v {
                w.i8(*b);
            }
        }
        Tag::String(s) => write_string(w, s),
        Tag::List(v) => {
            let elem = v.first().map_or(0, Tag::type_id);
            w.u8(elem);
            w.i32(v.len() as i32);
            for t in v {
                write_payload(w, t);
            }
        }
        Tag::Compound(entries) => {
            for (k, v) in entries {
                w.u8(v.type_id());
                write_string(w, k);
                write_payload(w, v);
            }
            w.u8(0);
        }
        Tag::IntArray(v) => {
            w.i32(v.len() as i32);
            for x in v {
                w.i32(*x);
            }
        }
        Tag::LongArray(v) => {
            w.i32(v.len() as i32);
            for x in v {
                w.i64(*x);
            }
        }
    }
}

/// Writes NBT the way 1.20.1 and older send it on the network: the root carries a name (empty).
pub fn write_named(w: &mut Writer, tag: &Tag) {
    w.u8(tag.type_id());
    if !matches!(tag, Tag::End) {
        w.u16(0);
    }
    write_payload(w, tag);
}

/// Reads 1.20.1-style NBT (named root); End tag = none.
pub fn read_named_optional(r: &mut Reader) -> Result<Option<Tag>> {
    let ty = r.u8()?;
    if ty == 0 {
        return Ok(None);
    }
    read_string(r)?;
    read_payload(r, ty, 0, &mut MAX_TAGS.clone()).map(Some)
}

/// Skips over one network NBT value without building it.
pub fn skip(r: &mut Reader) -> Result<()> {
    read(r).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let tag = Tag::Compound(vec![
            ("a".into(), Tag::Int(5)),
            ("b".into(), Tag::List(vec![Tag::String("x".into()), Tag::String("y".into())])),
            ("c".into(), Tag::Compound(vec![("d".into(), Tag::LongArray(vec![1, 2]))])),
        ]);
        let mut w = Writer::new();
        write(&mut w, &tag);
        let back = read(&mut Reader::new(&w.buf)).unwrap();
        assert_eq!(tag, back);
    }

    #[test]
    fn a_list_cannot_claim_more_elements_than_the_packet_holds() {
        // list of End tags, length i32::MAX: nine bytes that would otherwise grow without end
        let bomb = [9u8, 0, 0x7f, 0xff, 0xff, 0xff];
        assert!(read(&mut Reader::new(&bomb)).is_err());
        // list of bytes claiming a million elements with two present
        let short = [9u8, 1, 0, 0x0f, 0x42, 0x40, 1, 2];
        assert!(read(&mut Reader::new(&short)).is_err());
        // the empty list of End tags is what vanilla writes for an empty list
        assert_eq!(read(&mut Reader::new(&[9, 0, 0, 0, 0, 0])).unwrap(), Tag::List(Vec::new()));
    }

    #[test]
    fn tag_count_and_depth_are_capped() {
        // a list of single bytes one over the cap
        let n = MAX_TAGS as u32;
        let mut big = vec![9u8, 1];
        big.extend(n.to_be_bytes());
        big.extend(std::iter::repeat_n(7u8, n as usize));
        assert!(read(&mut Reader::new(&big)).unwrap_err().to_string().contains("too many tags"));
        // the same list comfortably under the cap is fine
        let mut ok = vec![9u8, 1];
        ok.extend(1000u32.to_be_bytes());
        ok.extend(std::iter::repeat_n(7u8, 1000));
        assert!(read(&mut Reader::new(&ok)).is_ok());
        // lists nested deeper than vanilla allows
        let mut deep = Vec::new();
        for _ in 0..MAX_DEPTH + 2 {
            deep.extend([9u8, 9, 0, 0, 0, 1].iter().skip(usize::from(!deep.is_empty())));
        }
        assert!(read(&mut Reader::new(&deep)).is_err());
    }

    /// 1.20.1 NBT is the 1.20.2 form with an empty root name after the type byte.
    #[test]
    fn named_root() {
        let tag = Tag::Compound(vec![("a".into(), Tag::Int(5))]);
        let mut nameless = Writer::new();
        write(&mut nameless, &tag);
        let mut named = Writer::new();
        write_named(&mut named, &tag);
        assert_eq!(named.buf[..3], [10, 0, 0]);
        assert_eq!(named.buf[3..], nameless.buf[1..]);
        assert_eq!(read_named_optional(&mut Reader::new(&named.buf)).unwrap(), Some(tag));
        // "no tag" is a lone End byte either way
        let mut end = Writer::new();
        write_named(&mut end, &Tag::End);
        assert_eq!(end.buf, [0]);
        assert_eq!(read_named_optional(&mut Reader::new(&[0])).unwrap(), None);
    }
}

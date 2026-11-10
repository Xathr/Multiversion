//! World packets 1.19.3 → 1.19.1: chunks, block entities, block states, sounds, spawns, and the
//! registry-id lists (tags, statistics, command argument types).

use super::data_1_19_1 as d;
use super::ids::old;
use super::items::{block_exact, block_state, entity_exact, item_exact, item_id, nbt_copy, strip_ns};
use crate::proto::buf::{DecodeError, Reader, Result, Writer};

const BLOCKS_PER_SECTION: usize = 4096;
/// Text components may be this long (in characters), longer than other strings.
const MAX_COMPONENT: usize = 262_144;

fn err(msg: String) -> DecodeError {
    DecodeError::Other(msg)
}

fn varint_copy(r: &mut Reader, w: &mut Writer) -> Result<i32> {
    let v = r.varint()?;
    w.varint(v);
    Ok(v)
}

fn string_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    w.string(r.string_max(MAX_COMPONENT)?);
    Ok(())
}

fn bits_for(count: u32) -> u32 {
    32 - (count - 1).leading_zeros()
}

fn unpack(longs: &[i64], bits: u32, count: usize) -> Vec<u32> {
    let per_long = 64 / (bits as usize).clamp(1, 64);
    let mask = (1u64 << bits) - 1;
    let mut out = Vec::with_capacity(count);
    'outer: for l in longs {
        let v = *l as u64;
        for i in 0..per_long {
            if out.len() == count {
                break 'outer;
            }
            out.push(((v >> (i as u32 * bits)) & mask) as u32);
        }
    }
    out.resize(count, 0);
    out
}

fn pack(values: &[u32], bits: u32) -> Vec<i64> {
    let per_long = 64 / (bits as usize).clamp(1, 64);
    let mut out = Vec::with_capacity(values.len().div_ceil(per_long));
    for chunk in values.chunks(per_long) {
        let mut v: u64 = 0;
        for (i, x) in chunk.iter().enumerate() {
            v |= (u64::from(*x) & ((1u64 << bits) - 1)) << (i as u32 * bits);
        }
        out.push(v as i64);
    }
    out
}

fn block_palette(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let bits = u32::from(r.u8()?);
    if bits == 0 {
        w.u8(0);
        w.varint(block_state(r.varint()?));
        w.long_array(&r.long_array()?);
        return Ok(());
    }
    if bits <= 8 {
        w.u8(bits as u8);
        let n = r.varint()?;
        w.varint(n);
        for _ in 0..n.max(0) {
            w.varint(block_state(r.varint()?));
        }
        w.long_array(&r.long_array()?);
        return Ok(());
    }
    if bits > 32 {
        return Err(err(format!("block palette of {bits} bits")));
    }
    // direct palette: global ids, repacked in the client's width
    let values = unpack(&r.long_array()?, bits, BLOCKS_PER_SECTION);
    let mapped: Vec<u32> = values.iter().map(|v| block_state(*v as i32).max(0) as u32).collect();
    let obits = bits_for(d::OLD_BLOCK_STATE_COUNT);
    w.u8(obits as u8);
    w.long_array(&pack(&mapped, obits));
    Ok(())
}

fn biome_palette(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let bits = r.u8()?;
    w.u8(bits);
    if bits == 0 {
        w.varint(r.varint()?);
    } else if bits <= 3 {
        let n = r.varint()?;
        w.varint(n);
        for _ in 0..n.max(0) {
            w.varint(r.varint()?);
        }
    }
    w.long_array(&r.long_array()?);
    Ok(())
}

/// 1.19.1 block entity type, or `None` for one it lacks (the chiseled bookshelf). Hanging signs
/// become signs, which keeps their text where the block is shown as a sign.
fn block_entity_type(id: i32) -> Option<i32> {
    let mapped = *d::BLOCK_ENTITIES.get(usize::try_from(id).ok()?)? as i32;
    (mapped != 0 || id == 0).then_some(mapped)
}

pub fn level_chunk_with_light(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 64);
    w.raw(r.bytes(8)?); // chunk x, z
    nbt_copy(&mut r, &mut w)?; // heightmaps
    let data = r.byte_array()?;
    let mut sr = Reader::new(data);
    let mut sections = Writer::with_capacity(data.len());
    while !sr.is_empty() {
        sections.i16(sr.i16()?); // non-air block count
        block_palette(&mut sr, &mut sections)?;
        biome_palette(&mut sr, &mut sections)?;
    }
    w.byte_array(&sections.buf);
    let n = r.varint()?;
    let mut entities = Writer::new();
    let mut kept = 0;
    for _ in 0..n.max(0) {
        let mut e = Writer::new();
        e.u8(r.u8()?); // packed xz
        e.i16(r.i16()?); // y
        let ty = block_entity_type(r.varint()?);
        nbt_copy(&mut r, &mut e)?;
        if let Some(ty) = ty {
            // the type sits between the position and the data
            entities.raw(&e.buf[..3]).varint(ty).raw(&e.buf[3..]);
            kept += 1;
        }
    }
    w.varint(kept);
    w.raw(&entities.buf);
    w.raw(r.remaining()); // light data: same layout
    Ok(w.into_inner())
}

/// `None` for a block entity 1.19.1 doesn't have.
pub fn block_entity_data(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 2);
    w.i64(r.i64()?);
    let Some(ty) = block_entity_type(r.varint()?) else { return Ok(None) };
    w.varint(ty);
    w.raw(r.remaining());
    Ok(Some(w.into_inner()))
}

pub fn block_update(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.i64(r.i64()?);
    w.varint(block_state(r.varint()?));
    Ok(w.into_inner())
}

pub fn section_blocks_update(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.i64(r.i64()?);
    w.bool(r.bool()?); // suppress light updates
    let n = r.varint()?;
    w.varint(n);
    for _ in 0..n.max(0) {
        let v = r.varlong()?;
        w.varlong((i64::from(block_state((v >> 12) as i32)) << 12) | (v & 0xfff));
    }
    Ok(w.into_inner())
}

/// BLOCK_EVENT names the block by its registry id; `None` for a block 1.19.1 doesn't have.
pub fn block_event(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.raw(r.bytes(8 + 2)?); // position, action, parameter
    let Some(block) = block_exact(r.varint()?) else { return Ok(None) };
    w.varint(block);
    Ok(Some(w.into_inner()))
}

pub fn level_event(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    let event = r.i32()?;
    w.i32(event);
    w.i64(r.i64()?);
    let data = r.i32()?;
    w.i32(match event {
        1010 => item_id(data),     // jukebox: the record's item id
        2001 => block_state(data), // block break particles
        _ => data,
    });
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// 1.19.1 sends the explosion's position as floats.
pub fn explode(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    for _ in 0..3 {
        w.f32(r.f64()? as f32);
    }
    w.raw(r.remaining()); // radius, blocks, player motion
    Ok(w.into_inner())
}

/// The key of a sound in the vanilla namespace ("minecraft:x" or plain "x"), if it is one.
fn vanilla_key(name: &str) -> Option<&str> {
    match name.strip_prefix("minecraft:") {
        Some(key) => Some(key),
        None => (!name.contains(':')).then_some(name),
    }
}

/// The name a 1.19.1 client knows a sound by: sounds new in 1.19.3 become their closest older
/// sound, anything else (resource pack sounds included) is kept.
fn sound_name(name: &str) -> String {
    if let Some(key) = vanilla_key(name)
        && !d::OLD_SOUND_NAMES.contains(&key)
    {
        let mapped = d::NEW_SOUND_NAMES.iter().position(|n| *n == key).and_then(|i| d::SOUNDS.get(i)).and_then(|o| d::OLD_SOUND_NAMES.get(*o as usize));
        if let Some(mapped) = mapped {
            return format!("minecraft:{mapped}");
        }
    }
    name.to_string()
}

/// SOUND / SOUND_ENTITY. 1.19.3 sends either a registry id (plus one) or, after a zero, a sound
/// name with an optional fixed range. 1.19.1 has a plain id in those packets and a separate
/// CUSTOM_SOUND packet for names, which exists for positions only: a named sound on an entity
/// is played by id when 1.19.1 has the sound and dropped otherwise.
pub fn sound(payload: &[u8], on_entity: bool) -> Result<Option<(i32, Vec<u8>)>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let by_id = if on_entity { old::SOUND_ENTITY } else { old::SOUND };
    let holder = r.varint()?;
    if holder != 0 {
        let Some(mapped) = usize::try_from(holder.wrapping_sub(1)).ok().and_then(|i| d::SOUNDS.get(i)) else { return Ok(None) };
        w.varint(*mapped as i32);
        w.raw(r.remaining());
        return Ok(Some((by_id, w.into_inner())));
    }
    let name = sound_name(r.string()?);
    r.option(|r| r.f32())?; // fixed range
    if on_entity {
        let Some(id) = vanilla_key(&name).and_then(|key| d::OLD_SOUND_NAMES.iter().position(|n| *n == key)) else { return Ok(None) };
        w.varint(id as i32);
        w.raw(r.remaining());
        return Ok(Some((by_id, w.into_inner())));
    }
    w.string(&name);
    w.raw(r.remaining());
    Ok(Some((old::CUSTOM_SOUND, w.into_inner())))
}

pub fn stop_sound(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 8);
    let flags = r.u8()?;
    w.u8(flags);
    if flags & 1 != 0 {
        varint_copy(&mut r, &mut w)?; // source
    }
    if flags & 2 != 0 {
        w.string(&sound_name(r.string()?));
    }
    Ok(w.into_inner())
}

/// What ADD_ENTITY became, and the entity's id and 1.19.3 type to track.
pub struct Spawn {
    pub payload: Vec<u8>,
    pub entity: i32,
    pub ty: i32,
}

/// ADD_ENTITY: the type id, and for falling blocks the block state they carry as data.
pub fn add_entity(payload: &[u8]) -> Result<Spawn> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let entity = r.varint()?;
    w.varint(entity);
    w.raw(r.bytes(16)?); // uuid
    let ty = r.varint()?;
    w.varint(d::ENTITIES.get(usize::try_from(ty).unwrap_or(usize::MAX)).map_or(0, |v| *v as i32));
    w.raw(r.bytes(3 * 8 + 3)?); // position, pitch, yaw, head yaw
    let data = r.varint()?;
    let falling_block = usize::try_from(ty).ok().and_then(|i| d::NEW_ENTITY_NAMES.get(i)).is_some_and(|n| *n == "falling_block");
    w.varint(if falling_block { block_state(data) } else { data });
    w.raw(r.remaining()); // velocity
    Ok(Spawn { payload: w.into_inner(), entity, ty })
}

/// RESPAWN: 1.19.3 sends which data to keep as two flags (attributes, entity data); 1.19.1 has
/// one "keep everything" bool, which is what the attributes flag used to be.
pub fn respawn(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.string(r.string()?); // dimension type
    w.string(r.string()?); // dimension
    w.raw(r.bytes(8 + 1 + 1 + 1 + 1)?); // seed, game mode, previous, debug, flat
    w.bool(r.u8()? & 1 != 0);
    w.raw(r.remaining()); // death location
    Ok(w.into_inner())
}

/// SERVER_DATA: 1.19.1 also reads whether the server previews chat (it never does now).
pub fn server_data(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 1);
    for _ in 0..2 {
        // description, icon
        if r.bool()? {
            w.bool(true);
            string_copy(&mut r, &mut w)?;
        } else {
            w.bool(false);
        }
    }
    w.bool(false); // previews chat
    w.raw(r.remaining()); // enforces secure chat
    Ok(w.into_inner())
}

/// Tags 1.19.1 has and 1.19.3 dropped; sent empty so nothing on the client looks them up in vain.
const MISSING_TAGS: [(&str, &str); 2] = [("block", "minecraft:non_flammable_wood"), ("item", "minecraft:overworld_natural_logs")];

/// UPDATE_TAGS: block, item and entity ids; entries 1.19.1 doesn't have are left out.
pub fn update_tags(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 64);
    let registries = varint_copy(&mut r, &mut w)?;
    for _ in 0..registries.max(0) {
        let registry = r.string()?;
        w.string(registry);
        let map: Option<fn(i32) -> Option<i32>> = match strip_ns(registry) {
            "block" => Some(block_exact),
            "item" => Some(item_exact),
            "entity_type" => Some(entity_exact),
            _ => None,
        };
        let count = r.varint()?;
        let mut tags = Writer::new();
        let mut names: Vec<&str> = Vec::new();
        for _ in 0..count.max(0) {
            let name = r.string()?;
            names.push(name);
            tags.string(name);
            let ids = r.list(Reader::varint)?;
            let ids: Vec<i32> = match map {
                Some(f) => ids.into_iter().filter_map(f).collect(),
                None => ids,
            };
            tags.varint(ids.len() as i32);
            for id in ids {
                tags.varint(id);
            }
        }
        let mut total = names.len();
        for (_, tag) in MISSING_TAGS.iter().filter(|(reg, tag)| *reg == strip_ns(registry) && !names.contains(tag)) {
            tags.string(tag).varint(0);
            total += 1;
        }
        w.varint(total as i32);
        w.raw(&tags.buf);
    }
    Ok(w.into_inner())
}

/// AWARD_STATS: block, item and entity statistics are keyed by registry id; ones for things
/// 1.19.1 doesn't have are left out.
pub fn award_stats(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let n = r.varint()?;
    let mut body = Writer::with_capacity(payload.len());
    let mut kept = 0;
    for _ in 0..n.max(0) {
        let category = r.varint()?;
        let id = r.varint()?;
        let value = r.varint()?;
        let id = match category {
            0 => block_exact(id),    // mined
            1..=5 => item_exact(id), // crafted, used, broken, picked up, dropped
            6 | 7 => entity_exact(id), // killed, killed by
            _ => Some(id),
        };
        if let Some(id) = id {
            body.varint(category).varint(id).varint(value);
            kept += 1;
        }
    }
    let mut w = Writer::with_capacity(body.buf.len() + 5);
    w.varint(kept);
    w.raw(&body.buf);
    Ok(w.into_inner())
}

/// A number argument's limits: a flags byte, then the minimum and maximum, each only when its
/// flag is set.
fn number_range(r: &mut Reader, w: &mut Writer, size: usize) -> Result<()> {
    let present = r.u8()?;
    w.u8(present);
    w.raw(r.bytes(size * usize::from(present & 1) + size * usize::from((present >> 1) & 1))?);
    Ok(())
}

/// COMMANDS: argument types are sent by registry id. 1.19.3's game mode argument has no 1.19.1
/// counterpart and becomes a single-word string.
pub fn commands(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 16);
    let nodes = varint_copy(&mut r, &mut w)?;
    for _ in 0..nodes.max(0) {
        let flags = r.u8()?;
        w.u8(flags);
        let children = varint_copy(&mut r, &mut w)?;
        for _ in 0..children.max(0) {
            varint_copy(&mut r, &mut w)?;
        }
        if flags & 0x08 != 0 {
            varint_copy(&mut r, &mut w)?; // redirect
        }
        let node = flags & 0x03;
        if node == 1 || node == 2 {
            w.string(r.string()?); // name
        }
        if node != 2 {
            continue;
        }
        let ty = r.varint()?;
        let index = usize::try_from(ty).map_err(|_| err(format!("unknown command argument type {ty}")))?;
        let (Some(name), Some(mapped)) = (d::NEW_ARGUMENT_TYPE_NAMES.get(index), d::ARGUMENT_TYPES.get(index)) else {
            return Err(err(format!("unknown command argument type {ty}")));
        };
        w.varint(*mapped as i32);
        match *name {
            "brigadier:float" | "brigadier:integer" => number_range(&mut r, &mut w, 4)?,
            "brigadier:double" | "brigadier:long" => number_range(&mut r, &mut w, 8)?,
            "brigadier:string" => {
                varint_copy(&mut r, &mut w)?;
            }
            "entity" | "score_holder" => {
                w.u8(r.u8()?);
            }
            "resource_or_tag" | "resource_or_tag_key" | "resource" | "resource_key" => {
                w.string(r.string()?); // registry
            }
            "gamemode" => {
                w.varint(0); // string: a single word
            }
            _ => {}
        }
        if flags & 0x10 != 0 {
            w.string(r.string()?); // suggestions
        }
    }
    varint_copy(&mut r, &mut w)?; // root
    Ok(w.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::nbt::{self, Tag};

    fn pos(names: &[&str], name: &str) -> i32 {
        names.iter().position(|n| *n == name).unwrap() as i32
    }

    /// A 1.19.3 state the 1.19.1 client knows under another number.
    fn moved_state() -> (i32, i32) {
        d::BLOCK_STATES.iter().enumerate().find(|(i, v)| **v as usize != *i).map(|(i, v)| (i as i32, *v as i32)).unwrap()
    }

    #[test]
    fn chunk_states_and_block_entities_are_mapped() {
        let (state, mapped) = moved_state();
        let last = d::NEW_BLOCK_STATE_COUNT as i32 - 1;
        let mut sec = Writer::new();
        sec.i16(1);
        sec.u8(0).varint(state).varint(0); // single value
        sec.u8(0).varint(3).varint(0); // biomes
        sec.i16(2);
        sec.u8(4).varint(2).varint(0).varint(state).long_array(&[0x10; 256]); // indirect
        sec.u8(1).varint(2).varint(0).varint(1).long_array(&[5]);
        sec.i16(4096);
        let direct: Vec<u32> = (0..4096).map(|i| if i % 2 == 0 { state as u32 } else { last as u32 }).collect();
        sec.u8(15).long_array(&pack(&direct, 15));
        sec.u8(6).long_array(&[7]); // direct biomes
        let heightmaps = Tag::Compound(vec![("MOTION_BLOCKING".into(), Tag::LongArray(vec![0; 37]))]);
        let mut w = Writer::new();
        w.i32(1).i32(2);
        nbt::write_named(&mut w, &heightmaps);
        w.byte_array(&sec.buf);
        w.varint(3);
        let sign = Tag::Compound(vec![("Text1".into(), Tag::String("\"hi\"".into()))]);
        w.u8(0x11).i16(64).varint(pos(&d::NEW_BLOCK_ENTITY_NAMES, "hanging_sign"));
        nbt::write_named(&mut w, &sign);
        w.u8(0x22).i16(65).varint(pos(&d::NEW_BLOCK_ENTITY_NAMES, "chiseled_bookshelf")).u8(0);
        w.u8(0x33).i16(66).varint(pos(&d::NEW_BLOCK_ENTITY_NAMES, "beacon")).u8(0);
        w.raw(&[9, 9, 9]); // light: copied
        let out = level_chunk_with_light(&w.buf).unwrap();

        let mut r = Reader::new(&out);
        assert_eq!((r.i32().unwrap(), r.i32().unwrap()), (1, 2));
        assert_eq!(nbt::read_named_optional(&mut r).unwrap(), Some(heightmaps));
        let data = r.byte_array().unwrap();
        let mut s = Reader::new(data);
        assert_eq!(s.i16().unwrap(), 1);
        assert_eq!((s.u8().unwrap(), s.varint().unwrap(), s.varint().unwrap()), (0, mapped, 0));
        assert_eq!((s.u8().unwrap(), s.varint().unwrap(), s.varint().unwrap()), (0, 3, 0));
        assert_eq!(s.i16().unwrap(), 2);
        assert_eq!((s.u8().unwrap(), s.varint().unwrap(), s.varint().unwrap(), s.varint().unwrap()), (4, 2, 0, mapped));
        assert_eq!(s.long_array().unwrap(), vec![0x10; 256]);
        assert_eq!((s.u8().unwrap(), s.varint().unwrap(), s.varint().unwrap(), s.varint().unwrap()), (1, 2, 0, 1));
        assert_eq!(s.long_array().unwrap(), vec![5]);
        assert_eq!(s.i16().unwrap(), 4096);
        assert_eq!(s.u8().unwrap(), 15, "1.19.1 also needs 15 bits for a global id");
        let values = unpack(&s.long_array().unwrap(), 15, 4096);
        assert_eq!((values[0] as i32, values[1] as i32), (mapped, block_state(last)));
        assert_eq!(s.u8().unwrap(), 6);
        assert_eq!(s.long_array().unwrap(), vec![7]);
        assert!(s.is_empty());
        assert_eq!(r.varint().unwrap(), 2, "the bookshelf's block entity is gone");
        assert_eq!((r.u8().unwrap(), r.i16().unwrap(), r.varint().unwrap()), (0x11, 64, pos(&d::OLD_BLOCK_ENTITY_NAMES, "sign")));
        assert_eq!(nbt::read_named_optional(&mut r).unwrap(), Some(sign));
        assert_eq!((r.u8().unwrap(), r.i16().unwrap(), r.varint().unwrap(), r.u8().unwrap()), (0x33, 66, pos(&d::OLD_BLOCK_ENTITY_NAMES, "beacon"), 0));
        assert_eq!(r.remaining(), &[9, 9, 9]);
    }

    #[test]
    fn broken_chunk_sections_are_errors() {
        let chunk = |sec: &[u8]| {
            let mut w = Writer::new();
            w.i32(0).i32(0).u8(0).byte_array(sec).varint(0);
            w.into_inner()
        };
        // a palette width no id fits in
        assert!(level_chunk_with_light(&chunk(&[0, 1, 64, 0])).is_err());
        assert!(level_chunk_with_light(&chunk(&[0, 1, 255, 0])).is_err());
        // a long array longer than the section
        assert!(level_chunk_with_light(&chunk(&[0, 1, 15, 0xff, 0xff, 0xff, 0xff, 0x07])).is_err());
        // a section cut short
        assert!(level_chunk_with_light(&chunk(&[0, 1, 0, 5])).is_err());
    }

    #[test]
    fn block_packets_use_old_states() {
        let (state, mapped) = moved_state();
        let mut p = Writer::new();
        p.i64(77).varint(state);
        let mut want = Writer::new();
        want.i64(77).varint(mapped);
        assert_eq!(block_update(&p.buf).unwrap(), want.buf);

        let mut p = Writer::new();
        p.i64(5).bool(true).varint(2).varlong((i64::from(state) << 12) | 0xabc).varlong(0x123);
        let mut want = Writer::new();
        want.i64(5).bool(true).varint(2).varlong((i64::from(mapped) << 12) | 0xabc).varlong(0x123);
        assert_eq!(section_blocks_update(&p.buf).unwrap(), want.buf);

        let mut p = Writer::new();
        p.i32(2001).i64(9).i32(state).bool(false);
        let mut want = Writer::new();
        want.i32(2001).i64(9).i32(mapped).bool(false);
        assert_eq!(level_event(&p.buf).unwrap(), want.buf);
        let (disc_new, disc_old) = (pos(&d::NEW_ITEM_NAMES, "music_disc_cat"), pos(&d::OLD_ITEM_NAMES, "music_disc_cat"));
        let mut p = Writer::new();
        p.i32(1010).i64(9).i32(disc_new).bool(false);
        let mut want = Writer::new();
        want.i32(1010).i64(9).i32(disc_old).bool(false);
        assert_eq!(level_event(&p.buf).unwrap(), want.buf);
        let mut p = Writer::new();
        p.i32(1000).i64(9).i32(state).bool(false);
        assert_eq!(level_event(&p.buf).unwrap(), p.buf, "other events keep their data");

        let (chest_new, chest_old) = (pos(&d::NEW_BLOCK_NAMES, "ender_chest"), pos(&d::OLD_BLOCK_NAMES, "ender_chest"));
        assert_ne!(chest_new, chest_old);
        let mut p = Writer::new();
        p.i64(3).u8(1).u8(1).varint(chest_new);
        let mut want = Writer::new();
        want.i64(3).u8(1).u8(1).varint(chest_old);
        assert_eq!(block_event(&p.buf).unwrap(), Some(want.buf));
        let mut p = Writer::new();
        p.i64(3).u8(1).u8(1).varint(pos(&d::NEW_BLOCK_NAMES, "chiseled_bookshelf"));
        assert_eq!(block_event(&p.buf).unwrap(), None);
    }

    #[test]
    fn block_entity_data_maps_the_type() {
        let mut p = Writer::new();
        p.i64(1).varint(pos(&d::NEW_BLOCK_ENTITY_NAMES, "beacon")).raw(&[10, 0, 0, 0]);
        let mut want = Writer::new();
        want.i64(1).varint(pos(&d::OLD_BLOCK_ENTITY_NAMES, "beacon")).raw(&[10, 0, 0, 0]);
        assert_eq!(block_entity_data(&p.buf).unwrap(), Some(want.buf));
        let mut p = Writer::new();
        p.i64(1).varint(pos(&d::NEW_BLOCK_ENTITY_NAMES, "chiseled_bookshelf")).u8(0);
        assert_eq!(block_entity_data(&p.buf).unwrap(), None);
        let mut p = Writer::new();
        p.i64(1).varint(-1).u8(0);
        assert_eq!(block_entity_data(&p.buf).unwrap(), None);
    }

    #[test]
    fn explosions_get_float_positions() {
        let mut p = Writer::new();
        p.f64(1.5).f64(-64.25).f64(1000.0).f32(4.0).varint(1).raw(&[1, 2, 3]).f32(0.0).f32(1.0).f32(0.0);
        let mut want = Writer::new();
        want.f32(1.5).f32(-64.25).f32(1000.0).f32(4.0).varint(1).raw(&[1, 2, 3]).f32(0.0).f32(1.0).f32(0.0);
        assert_eq!(explode(&p.buf).unwrap(), want.buf);
    }

    #[test]
    fn sounds_by_id_lose_the_holder_offset() {
        let (new, old_id) = (pos(&d::NEW_SOUND_NAMES, "entity.zombie.ambient"), pos(&d::OLD_SOUND_NAMES, "entity.zombie.ambient"));
        assert_ne!(new, old_id);
        let tail = |w: &mut Writer| {
            w.varint(5).i32(8).i32(16).i32(24).f32(1.0).f32(1.0).i64(42);
        };
        let mut p = Writer::new();
        p.varint(new + 1);
        tail(&mut p);
        let mut want = Writer::new();
        want.varint(old_id);
        tail(&mut want);
        assert_eq!(sound(&p.buf, false).unwrap(), Some((old::SOUND, want.buf)));
        // a sound new in 1.19.3 plays as its closest older one
        let bamboo = pos(&d::NEW_SOUND_NAMES, "block.bamboo_wood.break");
        let mut p = Writer::new();
        p.varint(bamboo + 1).varint(5).varint(77).f32(1.0).f32(1.0).i64(42);
        let mut want = Writer::new();
        want.varint(pos(&d::OLD_SOUND_NAMES, "block.wood.break")).varint(5).varint(77).f32(1.0).f32(1.0).i64(42);
        assert_eq!(sound(&p.buf, true).unwrap(), Some((old::SOUND_ENTITY, want.buf)));
        // an id outside the registry is dropped
        let mut p = Writer::new();
        p.varint(100_000);
        assert_eq!(sound(&p.buf, false).unwrap(), None);
    }

    #[test]
    fn named_sounds_use_the_custom_sound_packet() {
        let mut p = Writer::new();
        p.varint(0).string("mypack:boom").bool(true).f32(16.0).varint(5).i32(8).i32(16).i32(24).f32(1.0).f32(1.0).i64(42);
        let mut want = Writer::new();
        want.string("mypack:boom").varint(5).i32(8).i32(16).i32(24).f32(1.0).f32(1.0).i64(42);
        assert_eq!(sound(&p.buf, false).unwrap(), Some((old::CUSTOM_SOUND, want.buf)));
        // a vanilla name 1.19.1 lacks is renamed
        let mut p = Writer::new();
        p.varint(0).string("minecraft:block.bamboo_wood.break").bool(false).varint(5).i32(8).i32(16).i32(24).f32(1.0).f32(1.0).i64(42);
        let mut want = Writer::new();
        want.string("minecraft:block.wood.break").varint(5).i32(8).i32(16).i32(24).f32(1.0).f32(1.0).i64(42);
        assert_eq!(sound(&p.buf, false).unwrap(), Some((old::CUSTOM_SOUND, want.buf)));
        // on an entity only sounds 1.19.1 has an id for can play
        let mut p = Writer::new();
        p.varint(0).string("minecraft:entity.zombie.ambient").bool(false).varint(5).varint(77).f32(1.0).f32(1.0).i64(42);
        let mut want = Writer::new();
        want.varint(pos(&d::OLD_SOUND_NAMES, "entity.zombie.ambient")).varint(5).varint(77).f32(1.0).f32(1.0).i64(42);
        assert_eq!(sound(&p.buf, true).unwrap(), Some((old::SOUND_ENTITY, want.buf)));
        let mut p = Writer::new();
        p.varint(0).string("mypack:boom").bool(false).varint(5).varint(77).f32(1.0).f32(1.0).i64(42);
        assert_eq!(sound(&p.buf, true).unwrap(), None);

        let mut p = Writer::new();
        p.u8(3).varint(2).string("minecraft:block.bamboo_wood.break");
        let mut want = Writer::new();
        want.u8(3).varint(2).string("minecraft:block.wood.break");
        assert_eq!(stop_sound(&p.buf).unwrap(), want.buf);
        assert_eq!(stop_sound(&[0]).unwrap(), vec![0]);
    }

    #[test]
    fn spawns_map_the_type_and_falling_block_state() {
        let (state, mapped) = moved_state();
        let spawn = |ty: i32, data: i32| {
            let mut w = Writer::new();
            w.varint(12).uuid(99).varint(ty).f64(1.0).f64(2.0).f64(3.0).i8(1).i8(2).i8(3).varint(data).i16(4).i16(5).i16(6);
            w.into_inner()
        };
        let camel = pos(&d::NEW_ENTITY_NAMES, "camel");
        let out = add_entity(&spawn(camel, 0)).unwrap();
        assert_eq!((out.entity, out.ty), (12, camel));
        assert_eq!(out.payload, spawn(pos(&d::OLD_ENTITY_NAMES, "donkey"), 0));
        let out = add_entity(&spawn(pos(&d::NEW_ENTITY_NAMES, "falling_block"), state)).unwrap();
        assert_eq!(out.payload, spawn(pos(&d::OLD_ENTITY_NAMES, "falling_block"), mapped));
        let out = add_entity(&spawn(pos(&d::NEW_ENTITY_NAMES, "zombie"), state)).unwrap();
        assert_eq!(out.payload, spawn(pos(&d::OLD_ENTITY_NAMES, "zombie"), state), "other data is not a block state");
    }

    #[test]
    fn respawn_and_server_data_layouts() {
        let respawn_with = |keep: u8| {
            let mut w = Writer::new();
            w.string("minecraft:overworld").string("minecraft:overworld").i64(7).u8(1).i8(-1).bool(false).bool(true).u8(keep).bool(false);
            w.into_inner()
        };
        assert_eq!(respawn(&respawn_with(3)).unwrap(), respawn_with(1));
        assert_eq!(respawn(&respawn_with(2)).unwrap(), respawn_with(0), "keeping only entity data is not keeping everything");

        let mut p = Writer::new();
        p.bool(true).string("{\"text\":\"A server\"}").bool(false).bool(true);
        let mut want = Writer::new();
        want.bool(true).string("{\"text\":\"A server\"}").bool(false).bool(false).bool(true);
        assert_eq!(server_data(&p.buf).unwrap(), want.buf);
    }

    #[test]
    fn tags_and_stats_leave_out_what_the_client_lacks() {
        let (shears_new, shears_old) = (pos(&d::NEW_ITEM_NAMES, "shears"), pos(&d::OLD_ITEM_NAMES, "shears"));
        let (chest_new, chest_old) = (pos(&d::NEW_BLOCK_NAMES, "ender_chest"), pos(&d::OLD_BLOCK_NAMES, "ender_chest"));
        let (zombie_new, zombie_old) = (pos(&d::NEW_ENTITY_NAMES, "zombie"), pos(&d::OLD_ENTITY_NAMES, "zombie"));
        let mut p = Writer::new();
        p.varint(4);
        p.string("minecraft:item").varint(1).string("minecraft:planks").varint(2).varint(pos(&d::NEW_ITEM_NAMES, "bamboo_planks")).varint(shears_new);
        p.string("minecraft:block").varint(2);
        p.string("minecraft:a").varint(2).varint(pos(&d::NEW_BLOCK_NAMES, "bamboo_planks")).varint(chest_new);
        p.string("minecraft:non_flammable_wood").varint(0);
        p.string("minecraft:entity_type").varint(1).string("minecraft:b").varint(2).varint(pos(&d::NEW_ENTITY_NAMES, "camel")).varint(zombie_new);
        p.string("minecraft:fluid").varint(1).string("minecraft:water").varint(2).varint(1).varint(2);
        let mut want = Writer::new();
        want.varint(4);
        want.string("minecraft:item").varint(2).string("minecraft:planks").varint(1).varint(shears_old);
        want.string("minecraft:overworld_natural_logs").varint(0);
        want.string("minecraft:block").varint(2);
        want.string("minecraft:a").varint(1).varint(chest_old);
        want.string("minecraft:non_flammable_wood").varint(0);
        want.string("minecraft:entity_type").varint(1).string("minecraft:b").varint(1).varint(zombie_old);
        want.string("minecraft:fluid").varint(1).string("minecraft:water").varint(2).varint(1).varint(2);
        assert_eq!(update_tags(&p.buf).unwrap(), want.buf);
        // a tag longer than the packet is refused before anything is allocated for it
        let mut p = Writer::new();
        p.varint(1).string("minecraft:item").varint(1).string("x").varint(i32::MAX);
        assert!(update_tags(&p.buf).is_err());

        let mut p = Writer::new();
        p.varint(6);
        p.varint(0).varint(chest_new).varint(3);
        p.varint(0).varint(pos(&d::NEW_BLOCK_NAMES, "bamboo_planks")).varint(4);
        p.varint(2).varint(shears_new).varint(5);
        p.varint(1).varint(pos(&d::NEW_ITEM_NAMES, "bamboo_raft")).varint(6);
        p.varint(6).varint(pos(&d::NEW_ENTITY_NAMES, "camel")).varint(7);
        p.varint(8).varint(20).varint(8);
        let mut want = Writer::new();
        want.varint(3);
        want.varint(0).varint(chest_old).varint(3);
        want.varint(2).varint(shears_old).varint(5);
        want.varint(8).varint(20).varint(8);
        assert_eq!(award_stats(&p.buf).unwrap(), want.buf);
    }

    #[test]
    fn command_argument_types_are_mapped() {
        let arg = |names: &[&str], name: &str| pos(names, name);
        let mut p = Writer::new();
        p.varint(7);
        p.u8(0).varint(2).varint(1).varint(2); // root
        p.u8(1 | 0x08).varint(0).varint(0).string("gm"); // literal with redirect
        p.u8(2 | 0x04).varint(0).string("mode").varint(arg(&d::NEW_ARGUMENT_TYPE_NAMES, "gamemode"));
        p.u8(2 | 0x10).varint(0).string("n").varint(arg(&d::NEW_ARGUMENT_TYPE_NAMES, "brigadier:integer")).u8(3).i32(1).i32(9).string("minecraft:ask_server");
        p.u8(2).varint(0).string("d").varint(arg(&d::NEW_ARGUMENT_TYPE_NAMES, "dimension"));
        p.u8(2).varint(0).string("r").varint(arg(&d::NEW_ARGUMENT_TYPE_NAMES, "resource")).string("minecraft:entity_type");
        p.u8(2).varint(0).string("f").varint(arg(&d::NEW_ARGUMENT_TYPE_NAMES, "brigadier:double")).u8(2).f64(1.0);
        p.varint(0);
        let mut want = Writer::new();
        want.varint(7);
        want.u8(0).varint(2).varint(1).varint(2);
        want.u8(1 | 0x08).varint(0).varint(0).string("gm");
        want.u8(2 | 0x04).varint(0).string("mode").varint(arg(&d::OLD_ARGUMENT_TYPE_NAMES, "brigadier:string")).varint(0);
        want.u8(2 | 0x10).varint(0).string("n").varint(arg(&d::OLD_ARGUMENT_TYPE_NAMES, "brigadier:integer")).u8(3).i32(1).i32(9).string("minecraft:ask_server");
        want.u8(2).varint(0).string("d").varint(arg(&d::OLD_ARGUMENT_TYPE_NAMES, "dimension"));
        want.u8(2).varint(0).string("r").varint(arg(&d::OLD_ARGUMENT_TYPE_NAMES, "resource")).string("minecraft:entity_type");
        want.u8(2).varint(0).string("f").varint(arg(&d::OLD_ARGUMENT_TYPE_NAMES, "brigadier:double")).u8(2).f64(1.0);
        want.varint(0);
        assert_eq!(commands(&p.buf).unwrap(), want.buf);
        let mut p = Writer::new();
        p.varint(1).u8(2).varint(0).string("x").varint(48);
        assert!(commands(&p.buf).is_err(), "unknown argument type");
    }
}

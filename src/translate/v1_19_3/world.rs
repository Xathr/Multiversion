//! World and player packets 1.19.4 → 1.19.3: chunks, block entities, block states, sounds,
//! tags, statistics, commands, and the packets whose layout changed (LOGIN's registries,
//! PLAYER_POSITION, SERVER_DATA, OPEN_SCREEN) or that only 1.19.4 has (DAMAGE_EVENT,
//! HURT_ANIMATION).

use std::collections::HashSet;

use crate::translate::data_1_19_3 as d;
use super::items::{self, block, block_state, entity_type, item_id, nbt_copy, string_copy, varint_copy};
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

const BLOCKS_PER_SECTION: usize = 4096;

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
        return Err(DecodeError::Other(format!("{bits} bits per block")));
    }
    // direct palette: global ids, repacked in the client's width
    let values = unpack(&r.long_array()?, bits, BLOCKS_PER_SECTION);
    let mapped: Vec<u32> = values.iter().map(|v| block_state(*v as i32).max(0) as u32).collect();
    let obits = bits_for(d::OLD_BLOCK_STATE_COUNT);
    w.u8(obits as u8);
    w.long_array(&pack(&mapped, obits));
    Ok(())
}

/// Biome ids come from the registry in LOGIN, which is the same for both versions.
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

/// 1.19.4 block entity type → 1.19.3, `None` for the two it doesn't have (suspicious sand and
/// the decorated pot; the table can only name a stand-in, and a furnace would be wrong).
fn block_entity_type(ty: i32) -> Option<i32> {
    let i = usize::try_from(ty).ok()?;
    let name = d::NEW_BLOCK_ENTITY_NAMES.get(i)?;
    let mapped = *d::BLOCK_ENTITIES.get(i)? as usize;
    (d::OLD_BLOCK_ENTITY_NAMES.get(mapped) == Some(name)).then_some(mapped as i32)
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
    let mut kept = 0;
    let mut entities = Writer::new();
    for _ in 0..n.max(0) {
        let place = r.bytes(3)?; // packed xz, y
        let ty = r.varint()?;
        let mut tag = Writer::new();
        nbt_copy(&mut r, &mut tag)?;
        if let Some(ty) = block_entity_type(ty) {
            entities.raw(place).varint(ty).raw(&tag.buf);
            kept += 1;
        }
    }
    w.varint(kept);
    w.raw(&entities.buf);
    w.raw(r.remaining()); // light data: same layout
    Ok(w.into_inner())
}

/// `None` when 1.19.3 has no such block entity.
pub fn block_entity_data(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.i64(r.i64()?);
    let Some(ty) = block_entity_type(r.varint()?) else { return Ok(None) };
    w.varint(ty);
    nbt_copy(&mut r, &mut w)?;
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

/// BLOCK_EVENT names the block (not a state). `None` for a block 1.19.3 doesn't have.
pub fn block_event(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.raw(r.bytes(8 + 2)?); // position, action, parameter
    let Some(mapped) = block(r.varint()?) else { return Ok(None) };
    w.varint(mapped);
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
        1010 => item_id(data),     // jukebox: the record's item
        2001 => block_state(data), // block break particles
        _ => data,
    });
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// 1.19.4 sound → 1.19.3, `None` for the ones without a counterpart (the table marks those
/// with 0).
fn sound_id(id: i32) -> Option<i32> {
    let mapped = *usize::try_from(id).ok().and_then(|i| d::SOUNDS.get(i))? as i32;
    if mapped == 0 && id != 0 { None } else { Some(mapped) }
}

/// SOUND / SOUND_ENTITY: both start with the sound (0 = named inline, else id + 1). `None`
/// for a sound 1.19.3 doesn't have.
pub fn sound(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let holder = r.varint()?;
    if holder == 0 {
        return Ok(Some(payload.to_vec())); // inline sound name
    }
    let Some(mapped) = sound_id(holder.wrapping_sub(1)) else { return Ok(None) };
    w.varint(mapped + 1);
    w.raw(r.remaining());
    Ok(Some(w.into_inner()))
}

/// STOP_SOUND names the sound: a new one is stopped under the name its stand-in has.
pub fn stop_sound(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let flags = r.u8()?;
    w.u8(flags);
    if flags & 1 != 0 {
        varint_copy(&mut r, &mut w)?; // source
    }
    if flags & 2 != 0 {
        let name = r.string()?;
        let key = name.strip_prefix("minecraft:").unwrap_or(name);
        let stand_in = d::NEW_SOUND_NAMES
            .iter()
            .position(|n| *n == key)
            .and_then(|i| sound_id(i as i32))
            .and_then(|i| d::OLD_SOUND_NAMES.get(i as usize))
            .filter(|old| **old != key);
        match stand_in {
            Some(old) => w.string(&format!("minecraft:{old}")),
            None => w.string(name),
        };
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// LOGIN: same layout, but the registries hold things 1.19.3 can't read. The damage type and
/// armor trim registries go, and biomes get back the `precipitation` name that 1.19.4 turned
/// into the `has_precipitation` flag.
pub fn login(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    r.copy_to(&mut w, |r| {
        r.bytes(4 + 1 + 1 + 1)?; // entity id, hardcore, game mode, previous game mode
        let worlds = r.varint()?;
        for _ in 0..worlds.max(0) {
            r.string()?;
        }
        Ok(())
    })?;
    match nbt::read_named_optional(&mut r)? {
        Some(mut registries) => {
            registries_to_client(&mut registries);
            nbt::write_named(&mut w, &registries);
        }
        None => {
            w.u8(0);
        }
    }
    w.raw(r.remaining()); // dimension, seed, distances, flags, death location
    Ok(w.into_inner())
}

fn registries_to_client(registries: &mut Tag) {
    for gone in ["minecraft:trim_pattern", "minecraft:trim_material", "minecraft:damage_type"] {
        registries.remove(gone);
    }
    let biomes = registries.get_mut("minecraft:worldgen/biome").and_then(|b| b.get_mut("value")).and_then(Tag::as_list_mut);
    for biome in biomes.into_iter().flatten() {
        let Some(element) = biome.get_mut("element") else { continue };
        if element.get("precipitation").is_some() {
            continue;
        }
        let wet = element.get("has_precipitation").and_then(Tag::as_i32).unwrap_or(0) != 0;
        element.insert("precipitation", Tag::String(if wet { "rain" } else { "none" }.into()));
    }
}

/// 1.19.3 still has the "dismount vehicle" flag at the end.
pub fn player_position(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 1);
    w.raw(r.bytes(3 * 8 + 2 * 4 + 1)?); // position, rotation, relative flags
    w.varint(r.varint()?); // teleport id
    w.bool(false);
    Ok(w.into_inner())
}

/// DAMAGE_EVENT → ENTITY_EVENT 2 (hurt): 1.19.3 plays the animation and sound from that.
pub fn damage_event(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let entity = r.varint()?;
    r.varint()?; // damage type
    r.varint()?; // cause
    r.varint()?; // direct cause
    if r.bool()? {
        r.bytes(3 * 8)?; // source position
    }
    let mut w = Writer::new();
    w.i32(entity).i8(2);
    Ok(w.into_inner())
}

/// HURT_ANIMATION → ANIMATE 1 (hurt); the direction is lost.
pub fn hurt_animation(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let entity = r.varint()?;
    r.f32()?; // yaw
    let mut w = Writer::new();
    w.varint(entity).u8(1);
    Ok(w.into_inner())
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut b = [0u8; 3];
        for (dst, src) in b.iter_mut().zip(chunk) {
            *dst = *src;
        }
        let v = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[((v >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// SERVER_DATA: in 1.19.3 the description is optional and the icon a base64 data URL instead
/// of the PNG's bytes.
pub fn server_data(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() * 4 / 3 + 32);
    w.bool(true);
    string_copy(&mut r, &mut w)?; // description
    let icon = r.bool()?;
    w.bool(icon);
    if icon {
        w.string(&format!("data:image/png;base64,{}", base64(r.byte_array()?)));
    }
    w.raw(r.remaining()); // enforces secure chat
    Ok(w.into_inner())
}

/// What became of an OPEN_SCREEN.
pub enum Screen {
    Open(Vec<u8>),
    /// A menu 1.19.3 doesn't have, with its container id.
    Unknown(i32),
}

/// 1.19.4 put the new smithing table menu (with the template slot) at 21; 1.19.3 has nothing
/// that could show it.
pub fn open_screen(payload: &[u8]) -> Result<Screen> {
    const NEW_SMITHING: i32 = 21;
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let container = varint_copy(&mut r, &mut w)?;
    let menu = r.varint()?;
    if menu == NEW_SMITHING {
        return Ok(Screen::Unknown(container));
    }
    w.varint(if menu > NEW_SMITHING { menu - 1 } else { menu });
    w.raw(r.remaining()); // title
    Ok(Screen::Open(w.into_inner()))
}

/// AWARD_STATS: block, item and entity statistics name their subject by id.
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
            0 => match block(id) {
                Some(b) => b,
                None => continue, // mined a block 1.19.3 doesn't have
            },
            1..=5 => item_id(id), // crafted, used, broken, picked up, dropped
            6 | 7 => entity_type(id), // killed, killed by
            _ => id,
        };
        body.varint(category).varint(id).varint(value);
        kept += 1;
    }
    let mut w = Writer::with_capacity(body.buf.len() + 5);
    w.varint(kept);
    w.raw(&body.buf);
    Ok(w.into_inner())
}

/// UPDATE_TAGS: block, item and entity type tags list ids; damage types are a registry 1.19.3
/// doesn't have at all.
pub fn update_tags(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let n = r.varint()?;
    let mut body = Writer::with_capacity(payload.len());
    let mut kept = 0;
    for _ in 0..n.max(0) {
        let registry = r.string()?;
        let key = registry.strip_prefix("minecraft:").unwrap_or(registry);
        let map: fn(i32) -> Option<i32> = match key {
            "block" => block,
            "item" => |id| Some(item_id(id)),
            "entity_type" => |id| Some(entity_type(id)),
            _ => Some,
        };
        let mut tags = Writer::new();
        let count = varint_copy(&mut r, &mut tags)?;
        for _ in 0..count.max(0) {
            string_copy(&mut r, &mut tags)?;
            // several new ids can share a stand-in: each id is listed once
            let mut seen = HashSet::new();
            let ids: Vec<i32> = r.list(Reader::varint)?.into_iter().filter_map(map).filter(|id| seen.insert(*id)).collect();
            tags.varint(ids.len() as i32);
            for id in ids {
                tags.varint(id);
            }
        }
        if key == "damage_type" {
            continue;
        }
        body.string(registry).raw(&tags.buf);
        kept += 1;
    }
    let mut w = Writer::with_capacity(body.buf.len() + 5);
    w.varint(kept);
    w.raw(&body.buf);
    Ok(w.into_inner())
}

/// A brigadier number argument: flags, then the bounds they announce.
fn number_bounds(r: &mut Reader, w: &mut Writer, size: usize) -> Result<()> {
    let flags = r.u8()?;
    w.u8(flags);
    for bit in [1, 2] {
        if flags & bit != 0 {
            w.raw(r.bytes(size)?);
        }
    }
    Ok(())
}

/// A command argument's parser and its properties.
fn argument(r: &mut Reader, w: &mut Writer) -> Result<()> {
    const STRING: i32 = 5;
    let parser = r.varint()?;
    let index = usize::try_from(parser).map_err(|_| DecodeError::Other(format!("command argument type {parser}")))?;
    let name = *d::NEW_ARGUMENT_TYPE_NAMES.get(index).ok_or_else(|| DecodeError::Other(format!("unknown command argument type {parser}")))?;
    if name == "heightmap" {
        // new in 1.19.4: typed as a single word instead
        w.varint(STRING).varint(0);
        return Ok(());
    }
    w.varint(d::ARGUMENT_TYPES.get(index).map_or(parser, |v| *v as i32));
    match name {
        "brigadier:float" | "brigadier:integer" => number_bounds(r, w, 4)?,
        "brigadier:double" | "brigadier:long" => number_bounds(r, w, 8)?,
        "brigadier:string" => {
            varint_copy(r, w)?;
        }
        "entity" | "score_holder" => {
            w.u8(r.u8()?);
        }
        "time" => {
            r.i32()?; // minimum: new in 1.19.4
        }
        "resource" | "resource_or_tag" => {
            // 1.19.3 would reject a registry it doesn't know; any one it has keeps the command usable
            let registry = r.string()?;
            w.string(if registry == "minecraft:damage_type" { "minecraft:mob_effect" } else { registry });
        }
        "resource_key" | "resource_or_tag_key" => string_copy(r, w)?,
        _ => {}
    }
    Ok(())
}

/// COMMANDS: argument types are numbered, and 1.19.4 added one and gave `time` a minimum.
pub fn commands(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        let flags = r.u8()?;
        w.u8(flags);
        let children = varint_copy(&mut r, &mut w)?;
        for _ in 0..children.max(0) {
            varint_copy(&mut r, &mut w)?;
        }
        if flags & 0x08 != 0 {
            varint_copy(&mut r, &mut w)?; // redirect
        }
        match flags & 0x03 {
            1 => string_copy(&mut r, &mut w)?, // literal
            2 => {
                string_copy(&mut r, &mut w)?; // name
                argument(&mut r, &mut w)?;
                if flags & 0x10 != 0 {
                    string_copy(&mut r, &mut w)?; // suggestions
                }
            }
            _ => {}
        }
    }
    w.raw(r.remaining()); // root index
    Ok(w.into_inner())
}

/// What ADD_ENTITY turned into.
pub struct Spawn {
    pub entity: i32,
    /// The entity's 1.19.4 type.
    pub ty: i32,
    pub payload: Vec<u8>,
}

pub fn add_entity(payload: &[u8]) -> Result<Spawn> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let entity = varint_copy(&mut r, &mut w)?;
    w.raw(r.bytes(16)?); // uuid
    let ty = r.varint()?;
    w.varint(entity_type(ty));
    w.raw(r.bytes(3 * 8 + 3)?); // position, pitch, yaw, head yaw
    let data = r.varint()?;
    // a falling block's data is the block it shows
    w.varint(if items::is_falling_block(ty) { block_state(data) } else { data });
    w.raw(r.bytes(6)?); // velocity
    Ok(Spawn { entity, ty, payload: w.into_inner() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(names: &[&str], name: &str) -> i32 {
        names.iter().position(|n| *n == name).unwrap_or_else(|| panic!("{name}")) as i32
    }

    /// A state that has another number in 1.19.3.
    fn moved_state() -> i32 {
        let s = d::NEW_BLOCK_STATE_COUNT as i32 - 1;
        assert_ne!(block_state(s), s);
        s
    }

    fn chunk(section: &[u8], block_entities: &[(i32, Option<Tag>)]) -> Vec<u8> {
        let mut w = Writer::new();
        w.i32(3).i32(-4);
        nbt::write_named(&mut w, &Tag::Compound(vec![("MOTION_BLOCKING".into(), Tag::LongArray(vec![1, 2]))]));
        w.byte_array(section);
        w.varint(block_entities.len() as i32);
        for (ty, tag) in block_entities {
            w.u8(0x12).i16(64).varint(*ty);
            match tag {
                Some(t) => nbt::write_named(&mut w, t),
                None => {
                    w.u8(0);
                }
            }
        }
        w.bool(true).varint(0).varint(0).varint(0).varint(0).varint(0).varint(0); // light
        w.into_inner()
    }

    #[test]
    fn chunk_palettes_are_remapped() {
        let s = moved_state();
        // single value, then a small palette, then direct ids
        let mut new = Writer::new();
        let mut old = Writer::new();
        new.i16(4096).u8(0).varint(s).varint(0).u8(0).varint(5).varint(0);
        old.i16(4096).u8(0).varint(block_state(s)).varint(0).u8(0).varint(5).varint(0);
        let indexes = pack(&vec![1u32; BLOCKS_PER_SECTION], 4);
        new.i16(7).u8(4).varint(2).varint(0).varint(s).long_array(&indexes).u8(1).varint(2).varint(1).varint(3).long_array(&[0]);
        old.i16(7).u8(4).varint(2).varint(0).varint(block_state(s)).long_array(&indexes).u8(1).varint(2).varint(1).varint(3).long_array(&[0]);
        let mut values = vec![0u32; BLOCKS_PER_SECTION];
        values[0] = s as u32;
        values[4095] = 1;
        new.i16(2).u8(15).long_array(&pack(&values, 15)).u8(0).varint(0).varint(0);
        let mapped: Vec<u32> = values.iter().map(|v| block_state(*v as i32) as u32).collect();
        old.i16(2).u8(15).long_array(&pack(&mapped, 15)).u8(0).varint(0).varint(0);
        assert_eq!(bits_for(d::OLD_BLOCK_STATE_COUNT), 15);
        assert_eq!(level_chunk_with_light(&chunk(&new.buf, &[])).unwrap(), chunk(&old.buf, &[]));
        assert_eq!(unpack(&pack(&values, 15), 15, BLOCKS_PER_SECTION), values);
    }

    #[test]
    fn chunk_block_entities_1_19_3_lacks_are_left_out() {
        let chest = index(&d::NEW_BLOCK_ENTITY_NAMES, "chest");
        let pot = index(&d::NEW_BLOCK_ENTITY_NAMES, "decorated_pot");
        let tag = Tag::Compound(vec![("a".into(), Tag::Int(1))]);
        let out = level_chunk_with_light(&chunk(&[], &[(pot, Some(tag.clone())), (chest, Some(tag.clone())), (chest, None)])).unwrap();
        assert_eq!(out, chunk(&[], &[(chest, Some(tag)), (chest, None)]));
    }

    #[test]
    fn broken_chunks_are_errors() {
        let mut sec = Writer::new();
        sec.i16(1).u8(64).varint(1).i64(0);
        assert!(level_chunk_with_light(&chunk(&sec.buf, &[])).is_err(), "64 bits per block");
        let mut sec = Writer::new();
        sec.i16(1).u8(4).varint(i32::MAX);
        assert!(level_chunk_with_light(&chunk(&sec.buf, &[])).is_err(), "palette longer than the section");
        let good = chunk(&[], &[(1, None)]);
        for cut in 0..good.len() - 7 {
            assert!(level_chunk_with_light(&good[..cut]).is_err(), "cut at {cut}");
        }
    }

    #[test]
    fn block_entity_data_keeps_known_types() {
        let sign = index(&d::NEW_BLOCK_ENTITY_NAMES, "sign");
        let mut w = Writer::new();
        w.i64(99).varint(sign);
        nbt::write_named(&mut w, &Tag::Compound(vec![("Text1".into(), Tag::String("x".into()))]));
        assert_eq!(block_entity_data(&w.buf).unwrap(), Some(w.buf.clone()));
        let mut w = Writer::new();
        w.i64(99).varint(index(&d::NEW_BLOCK_ENTITY_NAMES, "suspicious_sand")).u8(0);
        assert_eq!(block_entity_data(&w.buf).unwrap(), None);
        assert_eq!(block_entity_type(-1), None);
        assert_eq!(block_entity_type(40), None);
    }

    #[test]
    fn block_states_in_updates() {
        let s = moved_state();
        let mut w = Writer::new();
        w.i64(5).varint(s);
        let mut want = Writer::new();
        want.i64(5).varint(block_state(s));
        assert_eq!(block_update(&w.buf).unwrap(), want.buf);

        let mut w = Writer::new();
        w.i64(5).bool(true).varint(2).varlong((i64::from(s) << 12) | 0xabc).varlong((1 << 12) | 0x001);
        let mut want = Writer::new();
        want.i64(5).bool(true).varint(2).varlong((i64::from(block_state(s)) << 12) | 0xabc).varlong((1 << 12) | 0x001);
        assert_eq!(section_blocks_update(&w.buf).unwrap(), want.buf);
        assert!(section_blocks_update(&w.buf[..w.buf.len() - 1]).is_err());
    }

    #[test]
    fn block_events_name_the_old_block() {
        let chest = index(&d::NEW_BLOCK_NAMES, "chest");
        let mut w = Writer::new();
        w.i64(5).u8(1).u8(1).varint(chest);
        let mut want = Writer::new();
        want.i64(5).u8(1).u8(1).varint(index(&d::OLD_BLOCK_NAMES, "chest"));
        assert_eq!(block_event(&w.buf).unwrap(), Some(want.buf));
        let mut w = Writer::new();
        w.i64(5).u8(1).u8(1).varint(index(&d::NEW_BLOCK_NAMES, "decorated_pot"));
        assert_eq!(block_event(&w.buf).unwrap(), None);
    }

    #[test]
    fn level_events_with_ids() {
        let s = moved_state();
        let disc = index(&d::NEW_ITEM_NAMES, "music_disc_cat");
        for (event, data, want) in [(2001, s, block_state(s)), (1010, disc, index(&d::OLD_ITEM_NAMES, "music_disc_cat")), (1000, 77, 77)] {
            let mut w = Writer::new();
            w.i32(event).i64(9).i32(data).bool(false);
            let mut expect = Writer::new();
            expect.i32(event).i64(9).i32(want).bool(false);
            assert_eq!(level_event(&w.buf).unwrap(), expect.buf, "event {event}");
        }
    }

    #[test]
    fn sounds_are_renumbered_or_dropped() {
        let new = index(&d::NEW_SOUND_NAMES, "entity.zombie.ambient");
        let old = index(&d::OLD_SOUND_NAMES, "entity.zombie.ambient");
        assert_ne!(new, old);
        let mut w = Writer::new();
        w.varint(new + 1).varint(0).i32(1).i32(2).i32(3).f32(1.0).f32(1.0).i64(7);
        let mut want = Writer::new();
        want.varint(old + 1).varint(0).i32(1).i32(2).i32(3).f32(1.0).f32(1.0).i64(7);
        assert_eq!(sound(&w.buf).unwrap(), Some(want.buf));
        // a sound without a stand-in
        let mut w = Writer::new();
        w.varint(index(&d::NEW_SOUND_NAMES, "music.overworld.cherry_grove") + 1).varint(0);
        assert_eq!(sound(&w.buf).unwrap(), None);
        // sound 0 itself and inline names stay
        assert_eq!(sound(&[1, 0]).unwrap(), Some(vec![1, 0]));
        let mut w = Writer::new();
        w.varint(0).string("custom:thing").bool(false).varint(0);
        assert_eq!(sound(&w.buf).unwrap(), Some(w.buf.clone()));
        assert_eq!(sound(&[]).ok(), None);
    }

    #[test]
    fn stop_sound_uses_the_stand_in_name() {
        let cherry = d::NEW_SOUND_NAMES.iter().position(|n| n.starts_with("block.cherry_wood.")).unwrap();
        let stand_in = d::OLD_SOUND_NAMES[d::SOUNDS[cherry] as usize];
        let mut w = Writer::new();
        w.u8(3).varint(4).string(&format!("minecraft:{}", d::NEW_SOUND_NAMES[cherry]));
        let mut want = Writer::new();
        want.u8(3).varint(4).string(&format!("minecraft:{stand_in}"));
        assert_eq!(stop_sound(&w.buf).unwrap(), want.buf);
        for p in [vec![0u8], vec![1, 4]] {
            assert_eq!(stop_sound(&p).unwrap(), p);
        }
        let mut w = Writer::new();
        w.u8(2).string("minecraft:entity.zombie.ambient");
        assert_eq!(stop_sound(&w.buf).unwrap(), w.buf);
        let mut w = Writer::new();
        w.u8(2).string("custom:thing");
        assert_eq!(stop_sound(&w.buf).unwrap(), w.buf);
    }

    fn login_packet(registries: &Tag) -> Vec<u8> {
        let mut w = Writer::new();
        w.i32(42).bool(false).u8(1).i8(-1);
        w.varint(1).string("minecraft:overworld");
        nbt::write_named(&mut w, registries);
        w.string("minecraft:overworld").string("minecraft:overworld").i64(1234);
        w.varint(20).varint(10).varint(8).bool(false).bool(true).bool(false).bool(true).bool(false);
        w.into_inner()
    }

    fn biome(name: &str, fields: Vec<(String, Tag)>) -> Tag {
        Tag::Compound(vec![("name".into(), Tag::String(name.into())), ("id".into(), Tag::Int(0)), ("element".into(), Tag::Compound(fields))])
    }

    #[test]
    fn login_registries_fit_1_19_3() {
        let registry = |entries: Vec<Tag>| Tag::Compound(vec![("type".into(), Tag::String("x".into())), ("value".into(), Tag::List(entries))]);
        let new = Tag::Compound(vec![
            ("minecraft:dimension_type".into(), registry(Vec::new())),
            ("minecraft:trim_pattern".into(), registry(Vec::new())),
            ("minecraft:trim_material".into(), registry(Vec::new())),
            ("minecraft:damage_type".into(), registry(Vec::new())),
            (
                "minecraft:worldgen/biome".into(),
                registry(vec![
                    biome("minecraft:plains", vec![("has_precipitation".into(), Tag::Byte(1)), ("temperature".into(), Tag::Float(0.8))]),
                    biome("minecraft:desert", vec![("has_precipitation".into(), Tag::Byte(0))]),
                ]),
            ),
            ("minecraft:chat_type".into(), registry(Vec::new())),
        ]);
        let old = Tag::Compound(vec![
            ("minecraft:dimension_type".into(), registry(Vec::new())),
            (
                "minecraft:worldgen/biome".into(),
                registry(vec![
                    biome(
                        "minecraft:plains",
                        vec![("has_precipitation".into(), Tag::Byte(1)), ("temperature".into(), Tag::Float(0.8)), ("precipitation".into(), Tag::String("rain".into()))],
                    ),
                    biome("minecraft:desert", vec![("has_precipitation".into(), Tag::Byte(0)), ("precipitation".into(), Tag::String("none".into()))]),
                ]),
            ),
            ("minecraft:chat_type".into(), registry(Vec::new())),
        ]);
        assert_eq!(login(&login_packet(&new)).unwrap(), login_packet(&old));
        // registries in an unexpected shape are passed on, not a crash
        let odd = Tag::Compound(vec![("minecraft:worldgen/biome".into(), Tag::Int(3))]);
        assert_eq!(login(&login_packet(&odd)).unwrap(), login_packet(&odd));
        assert!(login(&login_packet(&new)[..40]).is_err());
    }

    #[test]
    fn player_position_gets_the_dismount_flag() {
        let mut w = Writer::new();
        w.f64(1.0).f64(2.0).f64(3.0).f32(90.0).f32(0.0).i8(0).varint(300);
        let mut want = w.clone();
        want.bool(false);
        assert_eq!(player_position(&w.buf).unwrap(), want.buf);
        assert!(player_position(&w.buf[..30]).is_err());
    }

    #[test]
    fn damage_becomes_the_hurt_event() {
        let mut w = Writer::new();
        w.varint(300).varint(1).varint(5).varint(5).bool(true).f64(1.0).f64(2.0).f64(3.0);
        let mut want = Writer::new();
        want.i32(300).i8(2);
        assert_eq!(damage_event(&w.buf).unwrap(), want.buf);
        let mut w = Writer::new();
        w.varint(7).varint(1).varint(0).varint(0).bool(false);
        assert_eq!(damage_event(&w.buf).unwrap(), vec![0, 0, 0, 7, 2]);
        assert!(damage_event(&[7, 1]).is_err());

        let mut w = Writer::new();
        w.varint(300).f32(45.0);
        let mut want = Writer::new();
        want.varint(300).u8(1);
        assert_eq!(hurt_animation(&w.buf).unwrap(), want.buf);
    }

    #[test]
    fn base64_matches_the_standard() {
        for (plain, encoded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64(plain.as_bytes()), encoded);
        }
        assert_eq!(base64(&[0xfb, 0xff, 0xfe]), "+//+");
    }

    #[test]
    fn server_data_icon_becomes_a_data_url() {
        let mut w = Writer::new();
        w.string("{\"text\":\"hi\"}").bool(true).byte_array(b"foobar").bool(true);
        let mut want = Writer::new();
        want.bool(true).string("{\"text\":\"hi\"}").bool(true).string("data:image/png;base64,Zm9vYmFy").bool(true);
        assert_eq!(server_data(&w.buf).unwrap(), want.buf);
        let mut w = Writer::new();
        w.string("{}").bool(false).bool(false);
        let mut want = Writer::new();
        want.bool(true).string("{}").bool(false).bool(false);
        assert_eq!(server_data(&w.buf).unwrap(), want.buf);
        let mut w = Writer::new();
        w.string("{}").bool(true).varint(1000);
        assert!(server_data(&w.buf).is_err(), "icon longer than the packet");
    }

    #[test]
    fn menus_after_the_new_smithing_table_move_down() {
        let menu = |name: &str| index(&d::NEW_MENU_NAMES, name);
        assert_eq!(menu("smithing"), 21);
        for (name, old) in [("generic_9x3", 2), ("legacy_smithing", 20), ("smoker", 21), ("stonecutter", 23)] {
            let mut w = Writer::new();
            w.varint(5).varint(menu(name)).string("{\"text\":\"t\"}");
            let mut want = Writer::new();
            want.varint(5).varint(old).string("{\"text\":\"t\"}");
            match open_screen(&w.buf).unwrap() {
                Screen::Open(p) => assert_eq!(p, want.buf, "{name}"),
                Screen::Unknown(_) => panic!("{name}"),
            }
        }
        let mut w = Writer::new();
        w.varint(5).varint(21).string("{}");
        assert!(matches!(open_screen(&w.buf).unwrap(), Screen::Unknown(5)));
    }

    #[test]
    fn statistics_follow_their_registries() {
        let item = |n: &str| index(&d::NEW_ITEM_NAMES, n);
        let mut w = Writer::new();
        w.varint(5);
        w.varint(0).varint(index(&d::NEW_BLOCK_NAMES, "chest")).varint(3);
        w.varint(0).varint(index(&d::NEW_BLOCK_NAMES, "decorated_pot")).varint(1);
        w.varint(2).varint(item("diamond_sword")).varint(9);
        w.varint(6).varint(index(&d::NEW_ENTITY_NAMES, "zombie")).varint(4);
        w.varint(8).varint(12).varint(1000);
        let mut want = Writer::new();
        want.varint(4);
        want.varint(0).varint(index(&d::OLD_BLOCK_NAMES, "chest")).varint(3);
        want.varint(2).varint(index(&d::OLD_ITEM_NAMES, "diamond_sword")).varint(9);
        want.varint(6).varint(index(&d::OLD_ENTITY_NAMES, "zombie")).varint(4);
        want.varint(8).varint(12).varint(1000);
        assert_eq!(award_stats(&w.buf).unwrap(), want.buf);
        let mut w = Writer::new();
        w.varint(i32::MAX).varint(8);
        assert!(award_stats(&w.buf).is_err());
    }

    #[test]
    fn tags_are_remapped_and_damage_types_go() {
        let block = |n: &str| index(&d::NEW_BLOCK_NAMES, n);
        let cherry = index(&d::NEW_ITEM_NAMES, "cherry_planks");
        let stand_in = item_id(cherry);
        let stand_in_new = items::item_id_to_server(stand_in);
        let mut w = Writer::new();
        w.varint(4);
        w.string("minecraft:block").varint(1).string("minecraft:a").varint(2).varint(block("chest")).varint(block("decorated_pot"));
        w.string("minecraft:damage_type").varint(1).string("minecraft:is_fire").varint(1).varint(3);
        w.string("minecraft:item").varint(1).string("minecraft:planks").varint(2).varint(stand_in_new).varint(cherry);
        w.string("minecraft:fluid").varint(1).string("minecraft:water").varint(2).varint(1).varint(2);
        let mut want = Writer::new();
        want.varint(3);
        want.string("minecraft:block").varint(1).string("minecraft:a").varint(1).varint(index(&d::OLD_BLOCK_NAMES, "chest"));
        want.string("minecraft:item").varint(1).string("minecraft:planks").varint(1).varint(stand_in);
        want.string("minecraft:fluid").varint(1).string("minecraft:water").varint(2).varint(1).varint(2);
        assert_eq!(update_tags(&w.buf).unwrap(), want.buf);
        let mut w = Writer::new();
        w.varint(1).string("minecraft:block").varint(1).string("a").varint(i32::MAX);
        assert!(update_tags(&w.buf).is_err());
    }

    #[test]
    fn command_arguments_fit_1_19_3() {
        let arg = |n: &str| index(&d::NEW_ARGUMENT_TYPE_NAMES, n);
        let old_arg = |n: &str| index(&d::OLD_ARGUMENT_TYPE_NAMES, n);
        let mut w = Writer::new();
        let mut want = Writer::new();
        w.varint(8);
        want.varint(8);
        for p in [&mut w, &mut want] {
            p.u8(0).varint(2).varint(1).varint(2); // root
            p.u8(1 | 0x04).varint(0).string("time"); // literal
        }
        w.u8(2).varint(0).string("t").varint(arg("time")).i32(20);
        want.u8(2).varint(0).string("t").varint(old_arg("time"));
        w.u8(2).varint(0).string("h").varint(arg("heightmap"));
        want.u8(2).varint(0).string("h").varint(old_arg("brigadier:string")).varint(0);
        w.u8(2 | 0x08).varint(0).varint(1).string("u").varint(arg("uuid"));
        want.u8(2 | 0x08).varint(0).varint(1).string("u").varint(old_arg("uuid"));
        w.u8(2).varint(0).string("d").varint(arg("resource")).string("minecraft:damage_type");
        want.u8(2).varint(0).string("d").varint(old_arg("resource")).string("minecraft:mob_effect");
        w.u8(2 | 0x10).varint(0).string("f").varint(arg("brigadier:float")).u8(3).f32(0.0).f32(1.0).string("minecraft:ask_server");
        want.u8(2 | 0x10).varint(0).string("f").varint(old_arg("brigadier:float")).u8(3).f32(0.0).f32(1.0).string("minecraft:ask_server");
        w.u8(2).varint(0).string("l").varint(arg("brigadier:long")).u8(2).i64(9);
        want.u8(2).varint(0).string("l").varint(old_arg("brigadier:long")).u8(2).i64(9);
        w.varint(0);
        want.varint(0);
        assert_eq!(commands(&w.buf).unwrap(), want.buf);

        let mut w = Writer::new();
        w.varint(1).u8(2).varint(0).string("x").varint(49);
        assert!(commands(&w.buf).is_err(), "unknown argument type");
        let mut w = Writer::new();
        w.varint(i32::MAX).u8(0);
        assert!(commands(&w.buf).is_err());
    }

    #[test]
    fn spawns_use_the_old_entity_type() {
        let ty = |n: &str| index(&d::NEW_ENTITY_NAMES, n);
        let old = |n: &str| index(&d::OLD_ENTITY_NAMES, n);
        let s = moved_state();
        let build = |ty: i32, data: i32| {
            let mut w = Writer::new();
            w.varint(7).uuid(42).varint(ty).f64(1.0).f64(2.0).f64(3.0).i8(10).i8(20).i8(30).varint(data).i16(0).i16(5).i16(0);
            w.into_inner()
        };
        let spawn = add_entity(&build(ty("zombie"), s)).unwrap();
        assert_eq!((spawn.entity, spawn.ty), (7, ty("zombie")));
        assert_eq!(spawn.payload, build(old("zombie"), s));
        let spawn = add_entity(&build(ty("falling_block"), s)).unwrap();
        assert_eq!(spawn.payload, build(old("falling_block"), block_state(s)));
        let spawn = add_entity(&build(ty("item_display"), 0)).unwrap();
        assert_eq!(spawn.payload, build(old("armor_stand"), 0));
        let full = build(ty("zombie"), 0);
        for cut in 0..full.len() {
            assert!(add_entity(&full[..cut]).is_err(), "cut at {cut}");
        }
    }
}

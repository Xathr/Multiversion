//! World packets 1.19 → 1.18.2: chunks, block entities, block states, sounds, spawns, the login
//! and respawn dimension, and the registry-id lists (tags, statistics, command argument types).

use std::collections::HashMap;

use super::data_1_18_2 as d;
use super::items::{block_exact, block_state, entity_exact, entity_type, is_falling_block, is_living, item_exact, item_id, nbt_copy, strip_ns};
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

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
        w.long_array_raw(r.long_array_raw()?);
        return Ok(());
    }
    if bits <= 8 {
        w.u8(bits as u8);
        let n = r.varint()?;
        w.varint(n);
        for _ in 0..n.max(0) {
            w.varint(block_state(r.varint()?));
        }
        w.long_array_raw(r.long_array_raw()?);
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
    w.long_array_raw(r.long_array_raw()?);
    Ok(())
}

/// 1.18.2 block entity type, or `None` for one it lacks (the sculk catalyst and shrieker).
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

/// `None` for a block entity 1.18.2 doesn't have.
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

/// BLOCK_EVENT names the block by its registry id; `None` for a block 1.18.2 doesn't have.
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

/// The key of a sound in the vanilla namespace ("minecraft:x" or plain "x"), if it is one.
fn vanilla_key(name: &str) -> Option<&str> {
    match name.strip_prefix("minecraft:") {
        Some(key) => Some(key),
        None => (!name.contains(':')).then_some(name),
    }
}

/// The name a 1.18.2 client knows a sound by: sounds new in 1.19 become their closest older
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

/// The random seed 1.19 added to the end of every sound packet.
const SEED: usize = 8;

/// The packet without its trailing seed.
fn without_seed<'a>(r: &mut Reader<'a>) -> Result<&'a [u8]> {
    let rest = r.remaining();
    let keep = rest.len().checked_sub(SEED).ok_or_else(|| err("sound packet has no seed".into()))?;
    r.bytes(keep)
}

/// SOUND / SOUND_ENTITY: the sound's registry id, and no seed.
pub fn sound(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let id = r.varint()?;
    w.varint(usize::try_from(id).ok().and_then(|i| d::SOUNDS.get(i)).map_or(0, |v| *v as i32));
    w.raw(without_seed(&mut r)?);
    Ok(w.into_inner())
}

/// CUSTOM_SOUND: a sound by name, and no seed.
pub fn custom_sound(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 16);
    w.string(&sound_name(r.string()?));
    w.raw(without_seed(&mut r)?);
    Ok(w.into_inner())
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

/// A painting waiting for its motive: 1.18.2 spawns paintings with a packet that names it, and
/// 1.19 sends it afterwards, as entity data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Painting {
    pub uuid: u128,
    /// Packed block position (x 26 bits, z 26 bits, y 12 bits).
    pub position: i64,
    /// 0 south, 1 west, 2 north, 3 east.
    pub direction: u8,
}

/// What a 1.19 ADD_ENTITY becomes.
#[derive(Debug, PartialEq)]
pub enum Spawn {
    /// 1.18.2 ADD_ENTITY.
    Object(Vec<u8>),
    /// 1.18.2 ADD_MOB.
    Mob(Vec<u8>),
    /// Nothing yet: the painting is spawned when its entity data arrives.
    Painting(Painting),
    /// A painting on a floor or ceiling, which 1.18.2 can't show.
    Nothing,
}

pub struct Spawned {
    pub spawn: Spawn,
    pub entity: i32,
    /// 1.19 entity type.
    pub ty: i32,
}

fn pack_position(x: i32, y: i32, z: i32) -> i64 {
    ((i64::from(x) & 0x3ff_ffff) << 38) | ((i64::from(z) & 0x3ff_ffff) << 12) | (i64::from(y) & 0xfff)
}

/// ADD_ENTITY. 1.19 spawns everything but players and experience orbs with it; 1.18.2 has a
/// separate packet for living entities (with yaw before pitch, a head yaw and no data) and one
/// for paintings.
pub fn add_entity(payload: &[u8]) -> Result<Spawned> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 4);
    let entity = r.varint()?;
    let uuid_bytes = r.bytes(16)?;
    let ty = r.varint()?;
    let position = r.bytes(3 * 8)?;
    let (pitch, yaw, head_yaw) = (r.u8()?, r.u8()?, r.u8()?);
    let data = r.varint()?;
    let velocity = r.bytes(6)?;
    let name = usize::try_from(ty).ok().and_then(|i| d::NEW_ENTITY_NAMES.get(i)).copied().unwrap_or("");
    if name == "painting" {
        // 1.19 gives the facing as one of six directions; a painting can only face four
        let direction = match data {
            2 => 2,
            3 => 0,
            4 => 1,
            5 => 3,
            _ => return Ok(Spawned { spawn: Spawn::Nothing, entity, ty }),
        };
        let mut pr = Reader::new(position);
        let (x, y, z) = (pr.f64()?.floor() as i32, pr.f64()?.floor() as i32, pr.f64()?.floor() as i32);
        let uuid = Reader::new(uuid_bytes).uuid()?;
        return Ok(Spawned { spawn: Spawn::Painting(Painting { uuid, position: pack_position(x, y, z), direction }), entity, ty });
    }
    w.varint(entity).raw(uuid_bytes).varint(entity_type(ty)).raw(position);
    if is_living(ty) {
        w.u8(yaw).u8(pitch).u8(head_yaw).raw(velocity);
        return Ok(Spawned { spawn: Spawn::Mob(w.into_inner()), entity, ty });
    }
    w.u8(pitch).u8(yaw);
    w.i32(if is_falling_block(ty) { block_state(data) } else { data });
    w.raw(velocity);
    Ok(Spawned { spawn: Spawn::Object(w.into_inner()), entity, ty })
}

/// 1.18.2 ADD_PAINTING, once the motive (a 1.19 id) is known.
pub fn add_painting(entity: i32, painting: &Painting, motive: i32) -> Vec<u8> {
    let mut w = Writer::with_capacity(32);
    w.varint(entity).uuid(painting.uuid).varint(super::items::painting_motive(motive)).i64(painting.position).u8(painting.direction);
    w.into_inner()
}

/// The server's dimension types by name. 1.19 names the dimension type in LOGIN and RESPAWN;
/// 1.18.2 wants the whole type there.
#[derive(Default)]
pub struct Dimensions {
    types: HashMap<String, Tag>,
}

impl Dimensions {
    fn get(&self, name: &str) -> Result<&Tag> {
        self.types.get(strip_ns(name)).ok_or_else(|| err(format!("the server never sent dimension type {name}")))
    }
}

/// The entries of one registry in LOGIN's registry data: `{type, value: [{name, id, element}]}`.
fn registry_entries<'a>(registries: &'a mut Tag, name: &str) -> Option<&'a mut Vec<Tag>> {
    registries.get_mut(name)?.get_mut("value")?.as_list_mut()
}

/// LOGIN: the dimension type in full instead of its name, no chat types, a category on every
/// biome (1.18.2 won't read a biome without one), and no last death location.
pub fn login(payload: &[u8], dimensions: &mut Dimensions) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 1024);
    r.copy_to(&mut w, |r| {
        r.i32()?; // entity id
        r.bool()?; // hardcore
        r.u8()?; // game mode
        r.i8()?; // previous game mode
        r.list(|r| r.string().map(|_| ()))?; // worlds
        Ok(())
    })?;
    let mut registries = nbt::read_named_optional(&mut r)?.ok_or_else(|| err("login has no registries".into()))?;
    dimensions.types.clear();
    if let Some(entries) = registry_entries(&mut registries, "minecraft:dimension_type") {
        for entry in entries.iter() {
            if let (Some(name), Some(element)) = (entry.get("name").and_then(Tag::as_str), entry.get("element")) {
                dimensions.types.insert(strip_ns(name).to_string(), element.clone());
            }
        }
    }
    if let Some(entries) = registry_entries(&mut registries, "minecraft:worldgen/biome") {
        for entry in entries.iter_mut() {
            if let Some(element) = entry.get_mut("element")
                && element.get("category").is_none()
            {
                element.insert("category", Tag::String("none".into()));
            }
        }
    }
    registries.remove("minecraft:chat_type");
    nbt::write_named(&mut w, &registries);
    nbt::write_named(&mut w, dimensions.get(r.string()?)?);
    w.string(r.string()?); // world
    // seed, max players, view distance, simulation distance, reduced debug, death screen, debug, flat
    w.i64(r.i64()?);
    for _ in 0..3 {
        varint_copy(&mut r, &mut w)?;
    }
    w.raw(r.bytes(4)?);
    Ok(w.into_inner())
}

/// RESPAWN: the dimension type in full instead of its name, and no last death location.
pub fn respawn(payload: &[u8], dimensions: &Dimensions) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 512);
    nbt::write_named(&mut w, dimensions.get(r.string()?)?);
    w.string(r.string()?); // world
    w.raw(r.bytes(8 + 1 + 1 + 1 + 1 + 1)?); // seed, game mode, previous, debug, flat, keep data
    Ok(w.into_inner())
}

/// PLAYER_INFO: 1.19 ends each added player with their chat signing key.
pub fn player_info(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let action = varint_copy(&mut r, &mut w)?;
    if action != 0 {
        w.raw(r.remaining());
        return Ok(w.into_inner());
    }
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        r.copy_to(&mut w, |r| {
            r.uuid()?;
            r.string()?; // name
            r.list(|r| {
                r.string()?;
                r.string()?;
                r.option(|r| r.string().map(|_| ()))?;
                Ok(())
            })?; // properties
            r.varint()?; // game mode
            r.varint()?; // ping
            r.option(|r| r.string_max(MAX_COMPONENT).map(|_| ()))?; // display name
            Ok(())
        })?;
        r.option(|r| {
            r.i64()?; // key expiry
            r.byte_array()?; // public key
            r.byte_array()?; // signature
            Ok(())
        })?;
    }
    Ok(w.into_inner())
}

/// UPDATE_MOB_EFFECT: 1.19 ends with optional "factor" data (the darkness effect's pulse).
pub fn update_mob_effect(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    r.copy_to(&mut w, |r| {
        r.varint()?; // entity
        r.varint()?; // effect
        r.i8()?; // amplifier
        r.varint()?; // duration
        r.u8()?; // flags
        Ok(())
    })?;
    Ok(w.into_inner())
}

/// Tag registries only 1.19 has.
const REMOVED_TAG_REGISTRIES: [&str; 4] = ["banner_pattern", "instrument", "cat_variant", "painting_variant"];
/// Tags 1.18.2 looks up that 1.19 doesn't send; sent empty.
const MISSING_TAGS: [(&str, &str); 2] = [("block", "minecraft:polar_bears_spawnable_on_in_frozen_ocean"), ("item", "minecraft:occludes_vibration_signals")];
/// The game event tags 1.18.2 knows. Their entries are left out: game events were renumbered
/// in 1.19, and the client never reads these tags (sculk sensors are run by the server).
const GAME_EVENT_TAGS: [&str; 2] = ["minecraft:vibrations", "minecraft:ignore_vibrations_sneaking"];

/// UPDATE_TAGS: block, item and entity ids; entries 1.18.2 doesn't have are left out.
pub fn update_tags(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let registries = r.varint()?;
    let mut body = Writer::with_capacity(payload.len() + 64);
    let mut kept = 0;
    for _ in 0..registries.max(0) {
        let registry = r.string()?;
        let key = strip_ns(registry);
        let map: Option<fn(i32) -> Option<i32>> = match key {
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
            let ids = r.list(Reader::varint)?;
            if key == "game_event" {
                if GAME_EVENT_TAGS.contains(&name) {
                    names.push(name);
                    tags.string(name).varint(0);
                }
                continue;
            }
            // 1.19 renamed the carpets tag
            let name = if (key == "block" || key == "item") && name == "minecraft:wool_carpets" { "minecraft:carpets" } else { name };
            names.push(name);
            tags.string(name);
            let ids: Vec<i32> = match map {
                Some(f) => ids.into_iter().filter_map(f).collect(),
                None => ids,
            };
            tags.varint(ids.len() as i32);
            for id in ids {
                tags.varint(id);
            }
        }
        if REMOVED_TAG_REGISTRIES.contains(&key) {
            continue;
        }
        let mut total = names.len();
        for (_, tag) in MISSING_TAGS.iter().filter(|(reg, tag)| *reg == key && !names.contains(tag)) {
            tags.string(tag).varint(0);
            total += 1;
        }
        body.string(registry).varint(total as i32).raw(&tags.buf);
        kept += 1;
    }
    let mut w = Writer::with_capacity(body.buf.len() + 5);
    w.varint(kept);
    w.raw(&body.buf);
    Ok(w.into_inner())
}

/// AWARD_STATS: block, item and entity statistics are keyed by registry id; ones for things
/// 1.18.2 doesn't have are left out.
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
            0 => block_exact(id),      // mined
            1..=5 => item_exact(id),   // crafted, used, broken, picked up, dropped
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

/// The identifier 1.18.2 names an argument type by.
fn argument_identifier(name: &str) -> String {
    if name.contains(':') { name.to_string() } else { format!("minecraft:{name}") }
}

/// COMMANDS: 1.19 sends argument types by registry id, 1.18.2 by name. The two types 1.19 added
/// (structure mirror and rotation) become a single-word string.
pub fn commands(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + payload.len() / 2 + 16);
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
        let old_name = d::OLD_ARGUMENT_TYPE_NAMES.get(*mapped as usize).copied().unwrap_or("brigadier:string");
        w.string(&argument_identifier(old_name));
        match *name {
            "brigadier:float" | "brigadier:integer" => number_range(&mut r, &mut w, 4)?,
            "brigadier:double" | "brigadier:long" => number_range(&mut r, &mut w, 8)?,
            "brigadier:string" => {
                varint_copy(&mut r, &mut w)?;
            }
            "entity" | "score_holder" => {
                w.u8(r.u8()?);
            }
            "resource_or_tag" | "resource" => {
                w.string(r.string()?); // registry
            }
            "template_mirror" | "template_rotation" => {
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
pub(crate) mod tests {
    use super::super::items::tests::pos;
    use super::*;

    fn entity(name: &str) -> i32 {
        pos(&d::NEW_ENTITY_NAMES, name)
    }

    fn old_entity(name: &str) -> i32 {
        pos(&d::OLD_ENTITY_NAMES, name)
    }

    pub fn spawn(id: i32, ty: i32, data: i32) -> Vec<u8> {
        let mut w = Writer::new();
        w.varint(id).uuid(9).varint(ty).f64(1.5).f64(64.0).f64(-2.5).u8(10).u8(20).u8(30).varint(data).i16(1).i16(2).i16(3);
        w.into_inner()
    }

    #[test]
    fn living_entities_use_the_mob_packet() {
        for (name, shown) in [("zombie", "zombie"), ("armor_stand", "armor_stand"), ("warden", "iron_golem"), ("allay", "vex"), ("frog", "rabbit"), ("tadpole", "pufferfish")] {
            let spawned = add_entity(&spawn(7, entity(name), 0)).unwrap();
            let mut want = Writer::new();
            // yaw comes before pitch, and there is no data
            want.varint(7).uuid(9).varint(old_entity(shown)).f64(1.5).f64(64.0).f64(-2.5).u8(20).u8(10).u8(30).i16(1).i16(2).i16(3);
            assert_eq!(spawned.spawn, Spawn::Mob(want.into_inner()), "{name}");
            assert_eq!((spawned.entity, spawned.ty), (7, entity(name)));
        }
    }

    #[test]
    fn other_entities_keep_the_object_packet_with_int_data() {
        let spawned = add_entity(&spawn(7, entity("arrow"), 300)).unwrap();
        let mut want = Writer::new();
        want.varint(7).uuid(9).varint(old_entity("arrow")).f64(1.5).f64(64.0).f64(-2.5).u8(10).u8(20).i32(300).i16(1).i16(2).i16(3);
        assert_eq!(spawned.spawn, Spawn::Object(want.into_inner()));

        let state = (1..d::BLOCK_STATES.len() as i32).find(|s| block_state(*s) != *s).unwrap();
        let Spawn::Object(falling) = add_entity(&spawn(8, entity("falling_block"), state)).unwrap().spawn else { panic!("not an object") };
        let mut r = Reader::new(&falling[falling.len() - 10..]);
        assert_eq!(r.i32().unwrap(), block_state(state));

        let Spawn::Object(boat) = add_entity(&spawn(9, entity("chest_boat"), 0)).unwrap().spawn else { panic!("not an object") };
        let mut r = Reader::new(&boat);
        r.varint().unwrap();
        r.uuid().unwrap();
        assert_eq!(r.varint().unwrap(), old_entity("boat"));
        assert!(add_entity(&spawn(7, 1, 0)[..20]).is_err());
    }

    #[test]
    fn paintings_wait_for_their_motive() {
        let mut w = Writer::new();
        w.varint(4).uuid(11).varint(entity("painting")).f64(-3.0).f64(70.0).f64(12.0).u8(0).u8(0).u8(0).varint(4).i16(0).i16(0).i16(0);
        let spawned = add_entity(&w.buf).unwrap();
        let painting = Painting { uuid: 11, position: pack_position(-3, 70, 12), direction: 1 };
        assert_eq!(spawned.spawn, Spawn::Painting(painting.clone()));
        let mut want = Writer::new();
        want.varint(4).uuid(11).varint(pos(&d::OLD_PAINTING_NAMES, "donkey_kong")).i64(pack_position(-3, 70, 12)).u8(1);
        assert_eq!(add_painting(4, &painting, 29), want.into_inner());
        // on a floor: 1.18.2 can't hang it
        assert_eq!(add_entity(&spawn(4, entity("painting"), 1)).unwrap().spawn, Spawn::Nothing);
        // the position unpacks the way the game does it
        let packed = pack_position(-3, 70, 12);
        assert_eq!((packed >> 38, (packed << 26) >> 38, (packed << 52) >> 52), (-3, 12, 70));
    }

    pub fn registries() -> Tag {
        let dimension = |name: &str, height: i32| Tag::Compound(vec![("name".into(), Tag::String(name.into())), ("id".into(), Tag::Int(0)), ("element".into(), Tag::Compound(vec![("height".into(), Tag::Int(height)), ("infiniburn".into(), Tag::String("#minecraft:infiniburn_overworld".into()))]))]);
        let biome = Tag::Compound(vec![("name".into(), Tag::String("minecraft:plains".into())), ("id".into(), Tag::Int(0)), ("element".into(), Tag::Compound(vec![("downfall".into(), Tag::Float(0.4))]))]);
        let registry = |ty: &str, entries: Vec<Tag>| Tag::Compound(vec![("type".into(), Tag::String(ty.into())), ("value".into(), Tag::List(entries))]);
        Tag::Compound(vec![
            ("minecraft:dimension_type".into(), registry("minecraft:dimension_type", vec![dimension("minecraft:overworld", 384), dimension("minecraft:the_nether", 256)])),
            ("minecraft:worldgen/biome".into(), registry("minecraft:worldgen/biome", vec![biome])),
            ("minecraft:chat_type".into(), registry("minecraft:chat_type", vec![])),
        ])
    }

    pub fn server_login(dimension: &str) -> Vec<u8> {
        let mut w = Writer::new();
        w.i32(42).bool(false).u8(1).i8(-1).varint(1).string("minecraft:overworld");
        nbt::write_named(&mut w, &registries());
        w.string(dimension).string("minecraft:overworld").i64(99).varint(20).varint(10).varint(8).bool(false).bool(true).bool(false).bool(true);
        w.bool(true).string("minecraft:overworld").i64(5); // last death
        w.into_inner()
    }

    #[test]
    fn login_and_respawn_carry_the_whole_dimension_type() {
        let mut dimensions = Dimensions::default();
        let out = login(&server_login("minecraft:the_nether"), &mut dimensions).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.i32().unwrap(), 42);
        r.bytes(3).unwrap();
        r.list(|r| r.string().map(|_| ())).unwrap();
        let sent = nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert!(sent.get("minecraft:chat_type").is_none());
        assert!(sent.get("minecraft:dimension_type").is_some());
        let biome = &sent.get("minecraft:worldgen/biome").unwrap().get("value").unwrap().as_list().unwrap()[0];
        assert_eq!(biome.get("element").unwrap().get("category"), Some(&Tag::String("none".into())));
        let dimension = nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert_eq!(dimension.get("height"), Some(&Tag::Int(256)));
        assert_eq!(r.string().unwrap(), "minecraft:overworld");
        assert_eq!(r.i64().unwrap(), 99);
        assert_eq!((r.varint().unwrap(), r.varint().unwrap(), r.varint().unwrap()), (20, 10, 8));
        assert_eq!(r.bytes(4).unwrap(), [0, 1, 0, 1]);
        assert!(r.is_empty(), "no last death location");

        let mut w = Writer::new();
        w.string("minecraft:overworld").string("minecraft:overworld").i64(7).u8(0).i8(1).bool(false).bool(false).bool(true).bool(false);
        let out = respawn(&w.buf, &dimensions).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(nbt::read_named_optional(&mut r).unwrap().unwrap().get("height"), Some(&Tag::Int(384)));
        assert_eq!(r.string().unwrap(), "minecraft:overworld");
        assert_eq!(r.remaining(), [0, 0, 0, 0, 0, 0, 0, 7, 0, 1, 0, 0, 1]);

        // a dimension the server never listed can't be shown
        let mut w = Writer::new();
        w.string("minecraft:the_end").string("minecraft:the_end").i64(7).u8(0).i8(1).bool(false).bool(false).bool(true).bool(false);
        assert!(respawn(&w.buf, &dimensions).is_err());
        assert!(login(&server_login("minecraft:the_end"), &mut dimensions).is_err());
    }

    #[test]
    fn sounds_lose_the_seed() {
        let moved = (0..d::SOUNDS.len() as i32).find(|i| d::SOUNDS[*i as usize] as i32 != *i).unwrap();
        let mut w = Writer::new();
        w.varint(moved).varint(2).i32(8).i32(16).i32(24).f32(1.0).f32(1.0);
        let mut want = Writer::new();
        want.varint(d::SOUNDS[moved as usize] as i32).raw(&w.buf[crate::proto::buf::varint_len(moved)..]);
        w.i64(12345);
        assert_eq!(sound(&w.buf).unwrap(), want.buf);
        assert!(sound(&[1, 2, 3]).is_err());

        let mut w = Writer::new();
        w.string("minecraft:entity.warden.roar").varint(2).i32(8).i32(16).i32(24).f32(1.0).f32(1.0).i64(1);
        let out = custom_sound(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        let name = r.string().unwrap();
        assert!(d::OLD_SOUND_NAMES.contains(&name.strip_prefix("minecraft:").unwrap()), "{name}");
        assert_eq!(r.remaining().len(), 1 + 12 + 8);
        let mut w = Writer::new();
        w.string("mypack:boom").varint(2).i32(8).i32(16).i32(24).f32(1.0).f32(1.0).i64(1);
        assert_eq!(Reader::new(&custom_sound(&w.buf).unwrap()).string().unwrap(), "mypack:boom");
    }

    #[test]
    fn player_info_and_effects_lose_what_1_19_added() {
        let entry = |w: &mut Writer| {
            w.uuid(3).string("Alex").varint(1).string("textures").string("v").bool(false).varint(0).varint(12).bool(true).string("\"A\"");
        };
        let mut w = Writer::new();
        w.varint(0).varint(2);
        entry(&mut w);
        w.bool(true).i64(1).byte_array(&[1, 2, 3]).byte_array(&[4; 8]);
        entry(&mut w);
        w.bool(false);
        let mut want = Writer::new();
        want.varint(0).varint(2);
        entry(&mut want);
        entry(&mut want);
        assert_eq!(player_info(&w.buf).unwrap(), want.buf);
        // other actions are the same in both versions
        let mut w = Writer::new();
        w.varint(2).varint(1).uuid(3).varint(40);
        assert_eq!(player_info(&w.buf).unwrap(), w.buf);

        let mut w = Writer::new();
        w.varint(5).varint(33).i8(1).varint(600).u8(3);
        let want = w.buf.clone();
        w.bool(true);
        nbt::write_named(&mut w, &Tag::Compound(vec![("padding_duration".into(), Tag::Int(22))]));
        assert_eq!(update_mob_effect(&w.buf).unwrap(), want);
    }

    #[test]
    fn tags_fit_what_the_client_knows() {
        let mud = pos(&d::NEW_BLOCK_NAMES, "mud");
        let stone = pos(&d::NEW_BLOCK_NAMES, "stone");
        let frog = entity("frog");
        let mut w = Writer::new();
        w.varint(4);
        w.string("minecraft:block").varint(2);
        w.string("minecraft:wool_carpets").varint(2).varint(mud).varint(stone);
        w.string("minecraft:dirt").varint(0);
        w.string("minecraft:entity_type").varint(1).string("minecraft:frog_food").varint(2).varint(frog).varint(entity("slime"));
        w.string("minecraft:game_event").varint(2).string("minecraft:vibrations").varint(2).varint(40).varint(41).string("minecraft:warden_can_listen").varint(1).varint(3);
        w.string("minecraft:instrument").varint(1).string("minecraft:goat_horns").varint(1).varint(0);
        let mut want = Writer::new();
        want.varint(3);
        want.string("minecraft:block").varint(3);
        want.string("minecraft:carpets").varint(1).varint(pos(&d::OLD_BLOCK_NAMES, "stone"));
        want.string("minecraft:dirt").varint(0);
        want.string("minecraft:polar_bears_spawnable_on_in_frozen_ocean").varint(0);
        want.string("minecraft:entity_type").varint(1).string("minecraft:frog_food").varint(1).varint(old_entity("slime"));
        want.string("minecraft:game_event").varint(1).string("minecraft:vibrations").varint(0);
        assert_eq!(update_tags(&w.buf).unwrap(), want.buf);
    }

    #[test]
    fn command_argument_types_are_named() {
        let ty = |name: &str| pos(&d::NEW_ARGUMENT_TYPE_NAMES, name);
        let mut w = Writer::new();
        w.varint(6);
        w.u8(0).varint(5).varint(1).varint(2).varint(3).varint(4).varint(5); // root
        w.u8(1).varint(0).string("say"); // literal
        w.u8(2 | 0x04).varint(0).string("count").varint(ty("brigadier:integer")).u8(3).i32(1).i32(64);
        w.u8(2 | 0x10).varint(0).string("target").varint(ty("entity")).u8(1).string("minecraft:ask_server");
        w.u8(2).varint(0).string("mirror").varint(ty("template_mirror"));
        w.u8(2).varint(0).string("biome").varint(ty("resource_or_tag")).string("minecraft:worldgen/biome");
        w.varint(0);
        let mut want = Writer::new();
        want.varint(6);
        want.u8(0).varint(5).varint(1).varint(2).varint(3).varint(4).varint(5);
        want.u8(1).varint(0).string("say");
        want.u8(2 | 0x04).varint(0).string("count").string("brigadier:integer").u8(3).i32(1).i32(64);
        want.u8(2 | 0x10).varint(0).string("target").string("minecraft:entity").u8(1).string("minecraft:ask_server");
        want.u8(2).varint(0).string("mirror").string("brigadier:string").varint(0);
        want.u8(2).varint(0).string("biome").string("minecraft:resource_or_tag").string("minecraft:worldgen/biome");
        want.varint(0);
        assert_eq!(commands(&w.buf).unwrap(), want.buf);
        // an argument type 1.19 doesn't have
        let mut w = Writer::new();
        w.varint(1).u8(2).varint(0).string("x").varint(200).varint(0);
        assert!(commands(&w.buf).is_err());
    }

    #[test]
    fn chunk_states_and_block_packets_use_old_ids() {
        let state = (1..d::BLOCK_STATES.len() as i32).find(|s| block_state(*s) != *s).unwrap();
        let mut section = Writer::new();
        section.i16(4096).u8(0).varint(state).varint(0); // one block state everywhere
        section.u8(0).varint(3).varint(0); // one biome
        let mut w = Writer::new();
        w.i32(1).i32(2);
        nbt::write_named(&mut w, &Tag::compound());
        w.byte_array(&section.buf);
        w.varint(2);
        w.u8(0x12).i16(70).varint(pos(&d::NEW_BLOCK_ENTITY_NAMES, "chest"));
        nbt::write_named(&mut w, &Tag::compound());
        w.u8(0x34).i16(71).varint(pos(&d::NEW_BLOCK_ENTITY_NAMES, "sculk_shrieker"));
        nbt::write_named(&mut w, &Tag::compound());
        w.raw(&[1, 2, 3]); // light
        let mut old_section = Writer::new();
        old_section.i16(4096).u8(0).varint(block_state(state)).varint(0);
        old_section.u8(0).varint(3).varint(0);
        let mut want = Writer::new();
        want.i32(1).i32(2);
        nbt::write_named(&mut want, &Tag::compound());
        want.byte_array(&old_section.buf);
        want.varint(1);
        want.u8(0x12).i16(70).varint(pos(&d::OLD_BLOCK_ENTITY_NAMES, "chest"));
        nbt::write_named(&mut want, &Tag::compound());
        want.raw(&[1, 2, 3]);
        assert_eq!(level_chunk_with_light(&w.buf).unwrap(), want.buf);

        let mut w = Writer::new();
        w.i64(77).varint(state);
        let mut want = Writer::new();
        want.i64(77).varint(block_state(state));
        assert_eq!(block_update(&w.buf).unwrap(), want.buf);

        let mut w = Writer::new();
        w.i64(77).bool(false).varint(1).varlong((i64::from(state) << 12) | 0x123);
        let mut want = Writer::new();
        want.i64(77).bool(false).varint(1).varlong((i64::from(block_state(state)) << 12) | 0x123);
        assert_eq!(section_blocks_update(&w.buf).unwrap(), want.buf);

        let mut w = Writer::new();
        w.i64(77).varint(pos(&d::NEW_BLOCK_ENTITY_NAMES, "sculk_catalyst")).u8(0);
        assert_eq!(block_entity_data(&w.buf).unwrap(), None);
    }
}

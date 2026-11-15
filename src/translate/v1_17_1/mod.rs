//! 1.18 / 1.18.1 (protocol 757) ↔ 1.17.1 (protocol 756).
//!
//! Both versions have the same blocks and entities. What 1.18 changed:
//! - A chunk and its light arrive in one packet, each section carries its own biomes, and a
//!   section of one block is sent as that one block. 1.17.1 gets the light packet and then the
//!   chunk, with the biomes in one list and a mask saying which sections are there.
//! - Block entities are sent by type number and without their position and name.
//! - One new item (the "otherside" disc), a few new sounds, and one particle ("block marker")
//!   in place of two (barrier and light).
//! - The simulation distance: its own packet and a field in the login packet.
//! - The client says whether it may be listed in the server list.

use super::data_1_17_1 as d;
use super::{Packet, State, TResult, Translator};
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};
use crate::proto::version::Version;

// Clientbound play packets of 1.18 this step looks at.
const AWARD_STATS: i32 = 7;
const BLOCK_ENTITY_DATA: i32 = 10;
const CONTAINER_SET_CONTENT: i32 = 20;
const CONTAINER_SET_SLOT: i32 = 22;
const COOLDOWN: i32 = 23;
const LEVEL_CHUNK_WITH_LIGHT: i32 = 34;
const LEVEL_EVENT: i32 = 35;
const LEVEL_PARTICLES: i32 = 36;
const LIGHT_UPDATE: i32 = 37;
const LOGIN: i32 = 38;
const MERCHANT_OFFERS: i32 = 40;
const SET_DISPLAY_OBJECTIVE: i32 = 76;
const SET_ENTITY_DATA: i32 = 77;
const SET_EQUIPMENT: i32 = 80;
const SET_OBJECTIVE: i32 = 83;
const SET_PLAYER_TEAM: i32 = 85;
const SET_SCORE: i32 = 86;
const SET_SIMULATION_DISTANCE: i32 = 87;
const SOUND_ENTITY: i32 = 92;
const SOUND: i32 = 93;
const UPDATE_ADVANCEMENTS: i32 = 99;
const UPDATE_RECIPES: i32 = 102;
const UPDATE_TAGS: i32 = 103;
/// 1.17.1 has the same packets in the same order, less the simulation distance.
const LEVEL_CHUNK_OLD: i32 = 34;

// Serverbound play packets (the same numbers in both versions).
const CLIENT_INFORMATION: i32 = 5;
const CONTAINER_CLICK: i32 = 8;
const SET_CREATIVE_MODE_SLOT: i32 = 40;

/// The one item 1.17.1 lacks, and what stands in for it (the "ward" disc).
const OTHERSIDE_DISC: i32 = 1027;
const WARD_DISC: i32 = 1024;

// 1.18 particles that carry data, and the two 1.17.1 has in place of "block marker".
const PARTICLE_BLOCK: i32 = 2;
const PARTICLE_BLOCK_MARKER: i32 = 3;
const PARTICLE_DUST: i32 = 14;
const PARTICLE_DUST_TRANSITION: i32 = 15;
const PARTICLE_FALLING_DUST: i32 = 24;
const PARTICLE_ITEM: i32 = 35;
const PARTICLE_VIBRATION: i32 = 36;
const OLD_PARTICLE_BARRIER: i32 = 2;
const OLD_PARTICLE_LIGHT: i32 = 3;
/// The block states of the light block, which a block marker shows as the light particle.
const LIGHT_STATES: std::ops::RangeInclusive<i32> = 7755..=7786;

const BLOCKS_PER_SECTION: usize = 4096;
const BIOMES_PER_SECTION: usize = 64;

fn err(msg: impl Into<String>) -> DecodeError {
    DecodeError::Other(msg.into())
}

/// 1.17.1 item id for a 1.18 one.
pub fn item_id(id: i32) -> i32 {
    match id {
        OTHERSIDE_DISC => WARD_DISC,
        id if id > OTHERSIDE_DISC => id - 1,
        id => id,
    }
}

/// 1.18 item id for a 1.17.1 one.
pub fn item_id_to_server(id: i32) -> i32 {
    if id >= OTHERSIDE_DISC { id + 1 } else { id }
}

fn sound_id(id: i32) -> i32 {
    usize::try_from(id).ok().and_then(|i| d::SOUNDS.get(i)).map_or(0, |v| *v as i32)
}

fn nbt_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    r.copy_to(w, |r| nbt::read_named_optional(r).map(|_| ()))
}

fn varint_copy(r: &mut Reader, w: &mut Writer) -> Result<i32> {
    let v = r.varint()?;
    w.varint(v);
    Ok(v)
}

fn string_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    w.string(r.string()?);
    Ok(())
}

/// An item stack with its id changed by `map`: present, id, count, data.
fn item_with(r: &mut Reader, w: &mut Writer, map: fn(i32) -> i32) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if present {
        w.varint(map(r.varint()?));
        w.i8(r.i8()?);
        nbt_copy(r, w)?;
    }
    Ok(())
}

fn item(r: &mut Reader, w: &mut Writer) -> Result<()> {
    item_with(r, w, item_id)
}

/// A particle's data, with its 1.17.1 id handed to `write_id` (which puts it where the packet
/// has it).
fn particle(id: i32, r: &mut Reader, w: &mut Writer, write_id: impl FnOnce(&mut Writer, i32)) -> Result<()> {
    match id {
        PARTICLE_BLOCK_MARKER => {
            let state = r.varint()?;
            write_id(w, if LIGHT_STATES.contains(&state) { OLD_PARTICLE_LIGHT } else { OLD_PARTICLE_BARRIER });
        }
        // the two that took its place sit before everything after "block marker"
        0 | 1 => write_id(w, id),
        PARTICLE_BLOCK => {
            write_id(w, 4);
            varint_copy(r, w)?;
        }
        _ => {
            write_id(w, id + 1);
            match id {
                PARTICLE_FALLING_DUST => {
                    varint_copy(r, w)?;
                }
                PARTICLE_DUST => {
                    w.raw(r.bytes(16)?);
                }
                PARTICLE_DUST_TRANSITION => {
                    w.raw(r.bytes(28)?);
                }
                PARTICLE_ITEM => item(r, w)?,
                PARTICLE_VIBRATION => {
                    w.raw(r.bytes(8)?); // where it starts
                    let source = r.string()?;
                    w.string(source);
                    match source.strip_prefix("minecraft:").unwrap_or(source) {
                        "block" => {
                            w.raw(r.bytes(8)?);
                        }
                        "entity" => {
                            varint_copy(r, w)?;
                        }
                        other => return Err(err(format!("vibration towards a {other}"))),
                    }
                    varint_copy(r, w)?; // ticks
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// LEVEL_PARTICLES: the id is a full number here, then where and how many, then the data.
fn level_particles(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    let id = r.i32()?;
    let head = r.bytes(1 + 24 + 12 + 4 + 4)?; // far, position, spread, speed, count
    let mut data = Writer::new();
    let mut old_id = id;
    particle(id, &mut r, &mut data, |_, new| old_id = new)?;
    w.i32(old_id).raw(head).raw(&data.buf);
    Ok(w.into_inner())
}

/// SET_ENTITY_DATA: items and particles among an entity's values.
fn entity_data(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    varint_copy(&mut r, &mut w)?;
    loop {
        let index = r.u8()?;
        w.u8(index);
        if index == 0xff {
            break;
        }
        let ty = varint_copy(&mut r, &mut w)?;
        match ty {
            0 | 7 => {
                w.raw(r.bytes(1)?);
            }
            1 | 11 | 13 | 17 | 18 => {
                varint_copy(&mut r, &mut w)?;
            }
            2 => {
                w.raw(r.bytes(4)?);
            }
            3 | 4 => string_copy(&mut r, &mut w)?,
            5 => {
                let present = r.bool()?;
                w.bool(present);
                if present {
                    string_copy(&mut r, &mut w)?;
                }
            }
            6 => item(&mut r, &mut w)?,
            8 => {
                w.raw(r.bytes(12)?);
            }
            9 => {
                w.raw(r.bytes(8)?);
            }
            10 => {
                let present = r.bool()?;
                w.bool(present);
                if present {
                    w.raw(r.bytes(8)?);
                }
            }
            12 => {
                let present = r.bool()?;
                w.bool(present);
                if present {
                    w.raw(r.bytes(16)?);
                }
            }
            14 => nbt_copy(&mut r, &mut w)?,
            15 => {
                let id = r.varint()?;
                particle(id, &mut r, &mut w, |w, new| {
                    w.varint(new);
                })?;
            }
            16 => {
                for _ in 0..3 {
                    varint_copy(&mut r, &mut w)?;
                }
            }
            other => return Err(err(format!("entity value of type {other}"))),
        }
    }
    Ok(w.into_inner())
}

fn set_equipment(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    varint_copy(&mut r, &mut w)?;
    loop {
        let slot = r.u8()?;
        w.u8(slot);
        item(&mut r, &mut w)?;
        if slot & 0x80 == 0 {
            break;
        }
    }
    Ok(w.into_inner())
}

fn container_set_content(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.u8(r.u8()?);
    varint_copy(&mut r, &mut w)?; // state
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        item(&mut r, &mut w)?;
    }
    item(&mut r, &mut w)?; // on the cursor
    Ok(w.into_inner())
}

fn container_set_slot(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.i8(r.i8()?);
    varint_copy(&mut r, &mut w)?;
    w.i16(r.i16()?);
    item(&mut r, &mut w)?;
    Ok(w.into_inner())
}

fn cooldown(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.varint(item_id(r.varint()?));
    w.raw(r.remaining());
    Ok(w.into_inner())
}

fn merchant_offers(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    varint_copy(&mut r, &mut w)?;
    let n = r.u8()?;
    w.u8(n);
    for _ in 0..n {
        item(&mut r, &mut w)?; // first price
        item(&mut r, &mut w)?; // what is sold
        let second = r.bool()?;
        w.bool(second);
        if second {
            item(&mut r, &mut w)?;
        }
        w.raw(r.bytes(1 + 4 + 4 + 4 + 4 + 4 + 4)?); // out of stock, uses, limit, xp, special price, multiplier, demand
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

fn strings_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let n = varint_copy(r, w)?;
    for _ in 0..n.max(0) {
        string_copy(r, w)?;
    }
    Ok(())
}

/// UPDATE_ADVANCEMENTS: each advancement shown in the tree has an item as its icon.
fn update_advancements(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.bool(r.bool()?);
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        string_copy(&mut r, &mut w)?; // id
        let parent = r.bool()?;
        w.bool(parent);
        if parent {
            string_copy(&mut r, &mut w)?;
        }
        let display = r.bool()?;
        w.bool(display);
        if display {
            string_copy(&mut r, &mut w)?; // title
            string_copy(&mut r, &mut w)?; // description
            item(&mut r, &mut w)?;
            varint_copy(&mut r, &mut w)?; // frame
            let flags = r.i32()?;
            w.i32(flags);
            if flags & 1 != 0 {
                string_copy(&mut r, &mut w)?; // background
            }
            w.raw(r.bytes(8)?); // position in the tree
        }
        strings_copy(&mut r, &mut w)?; // criteria
        let groups = varint_copy(&mut r, &mut w)?;
        for _ in 0..groups.max(0) {
            strings_copy(&mut r, &mut w)?;
        }
    }
    w.raw(r.remaining()); // removed ones and progress: no items
    Ok(w.into_inner())
}

fn ingredient(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let n = varint_copy(r, w)?;
    for _ in 0..n.max(0) {
        item(r, w)?;
    }
    Ok(())
}

fn update_recipes(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        let kind = r.string()?;
        w.string(kind);
        string_copy(&mut r, &mut w)?; // id
        match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "crafting_shapeless" => {
                string_copy(&mut r, &mut w)?; // group
                let n = varint_copy(&mut r, &mut w)?;
                for _ in 0..n.max(0) {
                    ingredient(&mut r, &mut w)?;
                }
                item(&mut r, &mut w)?;
            }
            "crafting_shaped" => {
                let width = varint_copy(&mut r, &mut w)?;
                let height = varint_copy(&mut r, &mut w)?;
                string_copy(&mut r, &mut w)?;
                for _ in 0..width.max(0).saturating_mul(height.max(0)) {
                    ingredient(&mut r, &mut w)?;
                }
                item(&mut r, &mut w)?;
            }
            "smelting" | "blasting" | "smoking" | "campfire_cooking" => {
                string_copy(&mut r, &mut w)?;
                ingredient(&mut r, &mut w)?;
                item(&mut r, &mut w)?;
                w.raw(r.bytes(4)?); // experience
                varint_copy(&mut r, &mut w)?; // time
            }
            "stonecutting" => {
                string_copy(&mut r, &mut w)?;
                ingredient(&mut r, &mut w)?;
                item(&mut r, &mut w)?;
            }
            "smithing" => {
                ingredient(&mut r, &mut w)?;
                ingredient(&mut r, &mut w)?;
                item(&mut r, &mut w)?;
            }
            _ => {}
        }
    }
    Ok(w.into_inner())
}

/// AWARD_STATS: the statistics counted per item name the item by id.
fn award_stats(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        let category = varint_copy(&mut r, &mut w)?;
        let id = r.varint()?;
        // crafted, used, broken, picked up, dropped
        w.varint(if (1..=5).contains(&category) { item_id(id) } else { id });
        varint_copy(&mut r, &mut w)?;
    }
    Ok(w.into_inner())
}

/// UPDATE_TAGS: item tags list items by id, and 1.17.1 will not join without one block tag
/// 1.18 no longer has.
fn update_tags(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 48);
    let registries = varint_copy(&mut r, &mut w)?;
    for _ in 0..registries.max(0) {
        let registry = r.string()?;
        w.string(registry);
        let registry = registry.strip_prefix("minecraft:").unwrap_or(registry);
        let tags = r.varint()?;
        let mut body = Writer::new();
        let mut has_pool_tag = false;
        for _ in 0..tags.max(0) {
            let name = r.string()?;
            body.string(name);
            has_pool_tag |= name == "minecraft:lava_pool_stone_replaceables";
            let n = varint_copy(&mut r, &mut body)?;
            for _ in 0..n.max(0) {
                let id = r.varint()?;
                body.varint(if registry == "item" { item_id(id) } else { id });
            }
        }
        if registry == "block" && !has_pool_tag {
            w.varint(tags.max(0) + 1);
            w.raw(&body.buf);
            w.string("minecraft:lava_pool_stone_replaceables").varint(0);
        } else {
            w.varint(tags);
            w.raw(&body.buf);
        }
    }
    Ok(w.into_inner())
}

fn sound(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.varint(sound_id(r.varint()?));
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// LEVEL_EVENT 1010 plays the record whose item id it carries.
fn level_event(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    let event = r.i32()?;
    w.i32(event);
    w.raw(r.bytes(8)?);
    let data = r.i32()?;
    w.i32(if event == 1010 { item_id(data) } else { data });
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// The first `max` characters of a name: 1.17.1 refuses a longer one.
fn cut(name: &str, max: usize) -> &str {
    name.char_indices().nth(max).map_or(name, |(at, _)| &name[..at])
}

/// A packet that starts with a scoreboard objective's or team's name.
fn name_first(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.string(cut(r.string()?, 16));
    w.raw(r.remaining());
    Ok(w.into_inner())
}

fn set_display_objective(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.i8(r.i8()?);
    w.string(cut(r.string()?, 16));
    Ok(w.into_inner())
}

fn set_score(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.string(cut(r.string()?, 40)); // whose score
    varint_copy(&mut r, &mut w)?; // set or remove
    w.string(cut(r.string()?, 16)); // objective
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// The names of 1.18's block entity types, by type number.
const BLOCK_ENTITIES: [&str; 34] = [
    "furnace", "chest", "trapped_chest", "ender_chest", "jukebox", "dispenser", "dropper", "sign", "mob_spawner", "piston", "brewing_stand", "enchanting_table", "end_portal", "beacon",
    "skull", "daylight_detector", "hopper", "comparator", "banner", "structure_block", "end_gateway", "command_block", "shulker_box", "bed", "conduit", "barrel", "smoker", "blast_furnace",
    "lectern", "bell", "jigsaw", "campfire", "beehive", "sculk_sensor",
];
const SPAWNER: i32 = 8;

/// The number 1.17.1's block entity packet uses for a 1.18 type; it only knows these.
fn block_entity_action(ty: i32) -> Option<u8> {
    Some(match ty {
        8 => 1,   // spawner
        21 => 2,  // command block
        13 => 3,  // beacon
        14 => 4,  // skull
        24 => 5,  // conduit
        18 => 6,  // banner
        19 => 7,  // structure block
        20 => 8,  // end gateway
        7 => 9,   // sign
        22 => 10, // shulker box
        23 => 11, // bed
        30 => 12, // jigsaw
        31 => 13, // campfire
        _ => return None,
    })
}

/// A block entity's data as 1.17.1 wants it: with its name and position inside. Without them
/// signs, banners and heads are invisible there.
fn block_entity_tag(ty: i32, tag: Option<Tag>, x: i32, y: i32, z: i32) -> Option<Tag> {
    let name = BLOCK_ENTITIES.get(usize::try_from(ty).ok()?)?;
    let mut tag = match tag {
        Some(tag @ Tag::Compound(_)) => tag,
        _ => Tag::compound(),
    };
    if ty == SPAWNER
        && let Some(mut spawn) = tag.remove("SpawnData")
    {
        // 1.18 wraps what is spawned in "entity"
        let inner = spawn.remove("entity").unwrap_or(spawn);
        tag.insert("SpawnData", inner);
    }
    tag.insert("id", Tag::String(format!("minecraft:{name}")));
    tag.insert("x", Tag::Int(x));
    tag.insert("y", Tag::Int(y));
    tag.insert("z", Tag::Int(z));
    Some(tag)
}

/// BLOCK_ENTITY_DATA. `None` for a kind 1.17.1 has no update for.
fn block_entity_data(p: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(p);
    let position = r.i64()?;
    let ty = r.varint()?;
    let tag = nbt::read_named_optional(&mut r)?;
    let Some(action) = block_entity_action(ty) else { return Ok(None) };
    let (x, y, z) = ((position >> 38) as i32, (position << 52 >> 52) as i32, (position << 26 >> 38) as i32);
    let Some(tag) = block_entity_tag(ty, tag, x, y, z) else { return Ok(None) };
    let mut w = Writer::with_capacity(p.len() + 48);
    w.i64(position).u8(action);
    nbt::write_named(&mut w, &tag);
    Ok(Some(w.into_inner()))
}

fn bits_for(count: u32) -> u32 {
    if count <= 1 { 0 } else { 32 - (count - 1).leading_zeros() }
}

/// The `count` values packed `bits` wide in `longs`, none of them across two longs.
fn unpack(longs: &[i64], bits: u32, count: usize) -> Vec<u32> {
    let per_long = 64 / (bits as usize).clamp(1, 64);
    let mask = if bits >= 32 { u64::from(u32::MAX) } else { (1u64 << bits) - 1 };
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

/// One section's blocks, written the 1.17.1 way. `false` when 1.17.1 is sent nothing for it.
fn section_blocks(r: &mut Reader, w: &mut Writer) -> Result<bool> {
    let non_air = r.i16()?;
    let bits = r.u8()?;
    let mut body = Writer::new();
    match bits {
        0 => {
            // one block throughout: a palette of one, and every place pointing at it
            let state = r.varint()?;
            r.long_array_raw()?;
            body.u8(4).varint(1).varint(state);
            let longs = BLOCKS_PER_SECTION * 4 / 64;
            body.varint(longs as i32);
            body.buf.resize(body.buf.len() + longs * 8, 0);
        }
        1..=8 => {
            body.u8(bits);
            let n = varint_copy(r, &mut body)?;
            for _ in 0..n.max(0) {
                varint_copy(r, &mut body)?;
            }
            body.long_array_raw(r.long_array_raw()?);
        }
        _ => {
            body.u8(bits);
            body.long_array_raw(r.long_array_raw()?);
        }
    }
    if non_air == 0 {
        return Ok(false);
    }
    w.i16(non_air).raw(&body.buf);
    Ok(true)
}

/// One section's 64 biomes, added to the chunk's list.
fn section_biomes(r: &mut Reader, biome_bits: u32, out: &mut Vec<i32>) -> Result<()> {
    let bits = u32::from(r.u8()?);
    if bits == 0 {
        let biome = r.varint()?;
        r.long_array_raw()?;
        out.extend(std::iter::repeat_n(biome, BIOMES_PER_SECTION));
        return Ok(());
    }
    let palette = if bits <= 3 { Some(r.list(|r| r.varint())?) } else { None };
    let width = if palette.is_some() { bits } else { biome_bits.max(bits.min(32)) };
    if width == 0 || width > 32 {
        return Err(err(format!("biomes {width} bits wide")));
    }
    let values = unpack(&r.long_array()?, width, BIOMES_PER_SECTION);
    out.extend(values.into_iter().map(|v| match &palette {
        Some(palette) => palette.get(v as usize).copied().unwrap_or(0),
        None => v as i32,
    }));
    Ok(())
}

/// LEVEL_CHUNK_WITH_LIGHT → the light packet and the chunk packet, in that order.
fn level_chunk(p: &[u8], biome_bits: u32) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut r = Reader::new(p);
    let (x, z) = (r.i32()?, r.i32()?);
    let mut heightmaps = Writer::new();
    nbt_copy(&mut r, &mut heightmaps)?;
    let mut sr = Reader::new(r.byte_array()?);
    let mut sections = Writer::with_capacity(sr.len() + 64);
    let mut mask: Vec<u64> = Vec::new();
    let mut biomes: Vec<i32> = Vec::new();
    let mut index = 0usize;
    while !sr.is_empty() {
        if section_blocks(&mut sr, &mut sections)? {
            if mask.len() <= index / 64 {
                mask.resize(index / 64 + 1, 0);
            }
            mask[index / 64] |= 1 << (index % 64);
        }
        section_biomes(&mut sr, biome_bits, &mut biomes)?;
        index += 1;
    }
    let mut w = Writer::with_capacity(p.len() + biomes.len() + 64);
    w.i32(x).i32(z);
    w.varint(mask.len() as i32);
    for word in &mask {
        w.i64(*word as i64);
    }
    w.raw(&heightmaps.buf);
    w.varint(biomes.len() as i32);
    for biome in &biomes {
        w.varint(*biome);
    }
    w.byte_array(&sections.buf);
    let n = r.varint()?;
    let mut entities = Writer::new();
    let mut kept = 0;
    for _ in 0..n.max(0) {
        let xz = r.u8()?;
        let y = r.i16()?;
        let ty = r.varint()?;
        let tag = nbt::read_named_optional(&mut r)?;
        if let Some(tag) = block_entity_tag(ty, tag, (x << 4) + i32::from(xz >> 4), i32::from(y), (z << 4) + i32::from(xz & 15)) {
            nbt::write_named(&mut entities, &tag);
            kept += 1;
        }
    }
    w.varint(kept).raw(&entities.buf);
    // what is left is the light, laid out as the light packet has it
    let mut light = Writer::with_capacity(r.len() + 10);
    light.varint(x).varint(z).raw(r.remaining());
    Ok((light.into_inner(), w.into_inner()))
}

/// LOGIN without the simulation distance, with what 1.17.1 expects of a biome. Returns the
/// packet and how many bits name a biome when a section lists them without a palette.
fn login(p: &[u8]) -> Result<(Vec<u8>, u32)> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    r.copy_to(&mut w, |r| {
        r.i32()?; // entity id
        r.bool()?; // hardcore
        r.u8()?; // game mode
        r.i8()?; // previous game mode
        r.list(|r| r.string().map(|_| ()))?; // worlds
        Ok(())
    })?;
    let mut registries = nbt::read_named_optional(&mut r)?.ok_or_else(|| err("login has no registries"))?;
    let mut biome_count = 0u32;
    let biomes = registries.get_mut("minecraft:worldgen/biome").and_then(|registry| registry.get_mut("value")).and_then(Tag::as_list_mut);
    for entry in biomes.into_iter().flatten() {
        biome_count += 1;
        if let Some(biome) = entry.get_mut("element") {
            if biome.get("category").and_then(Tag::as_str) == Some("mountain") {
                biome.insert("category", Tag::String("extreme_hills".into()));
            }
            // 1.17.1 reads these two and 1.18 no longer sends them
            biome.insert("depth", Tag::Float(0.125));
            biome.insert("scale", Tag::Float(0.05));
        }
    }
    nbt::write_named(&mut w, &registries);
    nbt_copy(&mut r, &mut w)?; // the dimension the player is in
    string_copy(&mut r, &mut w)?; // world
    w.raw(r.bytes(8)?); // seed
    varint_copy(&mut r, &mut w)?; // max players
    varint_copy(&mut r, &mut w)?; // view distance
    r.varint()?; // simulation distance
    w.raw(r.remaining());
    Ok((w.into_inner(), bits_for(biome_count)))
}

/// CLIENT_INFORMATION: 1.18 also asks whether the player may be shown in the server list.
fn client_information(p: &[u8]) -> Vec<u8> {
    let mut out = p.to_vec();
    out.push(1);
    out
}

fn set_creative_slot(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.i16(r.i16()?);
    item_with(&mut r, &mut w, item_id_to_server)?;
    Ok(w.into_inner())
}

fn container_click(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.u8(r.u8()?); // window
    varint_copy(&mut r, &mut w)?; // state
    w.i16(r.i16()?); // slot
    w.i8(r.i8()?); // button
    varint_copy(&mut r, &mut w)?; // mode
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        w.i16(r.i16()?);
        item_with(&mut r, &mut w, item_id_to_server)?;
    }
    item_with(&mut r, &mut w, item_id_to_server)?; // on the cursor
    Ok(w.into_inner())
}

#[derive(Default)]
pub struct V1_18To1_17_1 {
    /// Bits per biome where a section lists its biomes without a palette (from the login).
    biome_bits: u32,
}

impl V1_18To1_17_1 {
    pub fn new() -> Self {
        Self::default()
    }

    fn play_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        // the same packets in the same order, less one
        let id = if pkt.id > SET_SIMULATION_DISTANCE { pkt.id - 1 } else { pkt.id };
        let payload = match pkt.id {
            SET_SIMULATION_DISTANCE => return Ok(()),
            LEVEL_CHUNK_WITH_LIGHT => {
                let (light, chunk) = level_chunk(p, self.biome_bits).map_err(e)?;
                out.push(Packet::new(LIGHT_UPDATE, light));
                out.push(Packet::new(LEVEL_CHUNK_OLD, chunk));
                return Ok(());
            }
            LOGIN => {
                let (payload, biome_bits) = login(p).map_err(e)?;
                self.biome_bits = biome_bits;
                payload
            }
            BLOCK_ENTITY_DATA => match block_entity_data(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            AWARD_STATS => award_stats(p).map_err(e)?,
            CONTAINER_SET_CONTENT => container_set_content(p).map_err(e)?,
            CONTAINER_SET_SLOT => container_set_slot(p).map_err(e)?,
            COOLDOWN => cooldown(p).map_err(e)?,
            LEVEL_EVENT => level_event(p).map_err(e)?,
            LEVEL_PARTICLES => level_particles(p).map_err(e)?,
            MERCHANT_OFFERS => merchant_offers(p).map_err(e)?,
            SET_ENTITY_DATA => entity_data(p).map_err(e)?,
            SET_EQUIPMENT => set_equipment(p).map_err(e)?,
            SET_OBJECTIVE | SET_PLAYER_TEAM => name_first(p).map_err(e)?,
            SET_DISPLAY_OBJECTIVE => set_display_objective(p).map_err(e)?,
            SET_SCORE => set_score(p).map_err(e)?,
            SOUND | SOUND_ENTITY => sound(p).map_err(e)?,
            UPDATE_ADVANCEMENTS => update_advancements(p).map_err(e)?,
            UPDATE_RECIPES => update_recipes(p).map_err(e)?,
            UPDATE_TAGS => update_tags(p).map_err(e)?,
            _ => p.to_vec(),
        };
        out.push(Packet::new(id, payload));
        Ok(())
    }

    fn play_serverbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: DecodeError| format!("serverbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            CLIENT_INFORMATION => client_information(p),
            CONTAINER_CLICK => container_click(p).map_err(e)?,
            SET_CREATIVE_MODE_SLOT => set_creative_slot(p).map_err(e)?,
            _ => p.to_vec(),
        };
        out.push(Packet::new(pkt.id, payload));
        Ok(())
    }
}

impl Translator for V1_18To1_17_1 {
    fn from(&self) -> Version {
        Version { protocol: 757 }
    }
    fn to(&self) -> Version {
        Version { protocol: 756 }
    }
    fn clientbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_clientbound(pkt, out),
            // neither version has a configuration phase: a newer step stands in for it
            State::Configuration => {
                out.push(pkt.clone());
                Ok(())
            }
        }
    }
    fn serverbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_serverbound(pkt, out),
            State::Configuration => {
                out.push(pkt.clone());
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(w: &mut Writer, id: i32) {
        w.bool(true).varint(id).i8(1).u8(0); // no data
    }

    fn clientbound(step: &mut V1_18To1_17_1, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        step.clientbound(State::Play, &Packet::new(id, payload), &mut out).unwrap();
        out
    }

    #[test]
    fn items_move_down_past_the_new_disc_and_back() {
        assert_eq!((item_id(1), item_id(1026), item_id(OTHERSIDE_DISC), item_id(1028), item_id(1100)), (1, 1026, WARD_DISC, 1027, 1099));
        // every 1.17.1 item comes back as itself
        for old in 0..1100 {
            assert_eq!(item_id(item_id_to_server(old)), old);
        }
        let mut step = V1_18To1_17_1::new();
        let mut w = Writer::new();
        w.i8(0).varint(7).i16(36);
        stack(&mut w, 1029);
        let out = clientbound(&mut step, CONTAINER_SET_SLOT, w.into_inner());
        let mut want = Writer::new();
        want.i8(0).varint(7).i16(36);
        stack(&mut want, 1028);
        assert_eq!(out[0].payload, want.buf);
        // and a click sends the server's id again
        let mut w = Writer::new();
        w.u8(0).varint(7).i16(36).i8(0).varint(0).varint(1).i16(36);
        stack(&mut w, 1028);
        w.bool(false);
        let mut sent = Vec::new();
        step.serverbound(State::Play, &Packet::new(CONTAINER_CLICK, w.into_inner()), &mut sent).unwrap();
        let mut want = Writer::new();
        want.u8(0).varint(7).i16(36).i8(0).varint(0).varint(1).i16(36);
        stack(&mut want, 1029);
        want.bool(false);
        assert_eq!(sent[0].payload, want.buf);
    }

    #[test]
    fn packets_after_the_simulation_distance_move_down_one() {
        let mut step = V1_18To1_17_1::new();
        assert!(clientbound(&mut step, SET_SIMULATION_DISTANCE, vec![10]).is_empty());
        assert_eq!(clientbound(&mut step, 86, vec![0, 1, 0])[0].id, 86);
        assert_eq!(clientbound(&mut step, 89, vec![1, 2])[0].id, 88);
        assert_eq!(clientbound(&mut step, 0x30, vec![1, 2])[0].id, 0x30);
        // the client's settings gain the server-list answer
        let mut sent = Vec::new();
        step.serverbound(State::Play, &Packet::new(CLIENT_INFORMATION, vec![5, 5]), &mut sent).unwrap();
        assert_eq!(sent[0].payload, [5, 5, 1]);
    }

    #[test]
    fn a_block_marker_becomes_a_barrier_or_a_light() {
        let level = |id: i32, data: &[u8]| {
            let mut w = Writer::new();
            w.i32(id).raw(&[0; 45]).raw(data);
            let out = level_particles(&w.buf).unwrap();
            (i32::from_be_bytes(out[..4].try_into().unwrap()), out[49..].to_vec())
        };
        assert_eq!(level(PARTICLE_BLOCK_MARKER, &[1]), (OLD_PARTICLE_BARRIER, vec![]));
        let mut light = Writer::new();
        light.varint(7760);
        assert_eq!(level(PARTICLE_BLOCK_MARKER, &light.buf), (OLD_PARTICLE_LIGHT, vec![]));
        assert_eq!(level(PARTICLE_BLOCK, &[9]), (4, vec![9]));
        assert_eq!(level(1, &[]), (1, vec![]));
        assert_eq!(level(20, &[]), (21, vec![]));
        assert_eq!(level(PARTICLE_DUST, &[7; 16]), (15, vec![7; 16]));
        // among an entity's values: an item, then a particle
        let mut w = Writer::new();
        w.varint(5).u8(8).varint(6);
        stack(&mut w, 1030);
        w.u8(10).varint(15).varint(PARTICLE_ITEM);
        stack(&mut w, OTHERSIDE_DISC);
        w.u8(0xff);
        let mut want = Writer::new();
        want.varint(5).u8(8).varint(6);
        stack(&mut want, 1029);
        want.u8(10).varint(15).varint(PARTICLE_ITEM + 1);
        stack(&mut want, WARD_DISC);
        want.u8(0xff);
        assert_eq!(entity_data(&w.buf).unwrap(), want.buf);
    }

    #[test]
    fn block_entities_get_their_name_and_place() {
        // a sign at (-3, -60, 20)
        let position = ((-3i64 & 0x3ff_ffff) << 38) | ((20i64 & 0x3ff_ffff) << 12) | (-60i64 & 0xfff);
        let mut tag = Tag::compound();
        tag.insert("Text1", Tag::String("hi".into()));
        let mut w = Writer::new();
        w.i64(position).varint(7);
        nbt::write_named(&mut w, &tag);
        let out = block_entity_data(&w.buf).unwrap().unwrap();
        let mut r = Reader::new(&out);
        assert_eq!((r.i64().unwrap(), r.u8().unwrap()), (position, 9));
        let got = nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert_eq!(got.get("id").and_then(Tag::as_str), Some("minecraft:sign"));
        assert_eq!((got.get("x").and_then(Tag::as_i32), got.get("y").and_then(Tag::as_i32), got.get("z").and_then(Tag::as_i32)), (Some(-3), Some(-60), Some(20)));
        assert_eq!(got.get("Text1").and_then(Tag::as_str), Some("hi"));
        // a chest has no update in 1.17.1; a bed has no data of its own in 1.18
        let mut w = Writer::new();
        w.i64(position).varint(1).u8(0);
        assert_eq!(block_entity_data(&w.buf).unwrap(), None);
        let mut w = Writer::new();
        w.i64(position).varint(23).u8(0);
        assert!(block_entity_data(&w.buf).unwrap().is_some());
        // what a spawner spawns is no longer wrapped
        let mut entity = Tag::compound();
        entity.insert("id", Tag::String("minecraft:pig".into()));
        let mut data = Tag::compound();
        data.insert("entity", entity);
        let mut spawner = Tag::compound();
        spawner.insert("SpawnData", data);
        let got = block_entity_tag(SPAWNER, Some(spawner), 0, 0, 0).unwrap();
        assert_eq!(got.get("SpawnData").and_then(|s| s.get("id")).and_then(Tag::as_str), Some("minecraft:pig"));
    }

    /// A 1.18 chunk of three sections: all air, all stone, and a mix; with one sign.
    fn chunk_1_18() -> Vec<u8> {
        let mut sections = Writer::new();
        // air throughout, one biome (4)
        sections.i16(0).u8(0).varint(0).varint(0);
        sections.u8(0).varint(4).varint(0);
        // stone (1) throughout, biomes from a palette of two (7, 9), all the second
        sections.i16(4096).u8(0).varint(1).varint(0);
        sections.u8(1).varint(2).varint(7).varint(9).varint(1).i64(-1);
        // a palette of [air, dirt(10)], every block dirt; biomes listed by number, 6 bits each
        sections.i16(4096).u8(4).varint(2).varint(0).varint(10).varint(256);
        for _ in 0..256 {
            sections.i64(0x1111_1111_1111_1111);
        }
        let ids: Vec<u32> = (0..64).collect();
        sections.u8(6).varint(7);
        for longs in ids.chunks(10) {
            sections.i64(longs.iter().enumerate().fold(0u64, |acc, (i, v)| acc | (u64::from(*v) << (i * 6))) as i64);
        }
        let mut w = Writer::new();
        w.i32(2).i32(-1);
        nbt::write_named(&mut w, &Tag::compound());
        w.byte_array(&sections.buf);
        w.varint(1).u8(0x5a).i16(70).varint(7).u8(0);
        w.raw(&[1, 0, 0, 0, 0, 0, 0]); // light: trust edges, four empty masks, no arrays
        w.into_inner()
    }

    #[test]
    fn a_chunk_becomes_light_then_chunk() {
        let mut step = V1_18To1_17_1 { biome_bits: 6 };
        let out = clientbound(&mut step, LEVEL_CHUNK_WITH_LIGHT, chunk_1_18());
        assert_eq!(out.iter().map(|p| p.id).collect::<Vec<_>>(), [LIGHT_UPDATE, LEVEL_CHUNK_OLD]);
        assert_eq!(out[0].payload, [2, 0xff, 0xff, 0xff, 0xff, 0x0f, 1, 0, 0, 0, 0, 0, 0]);
        let mut r = Reader::new(&out[1].payload);
        assert_eq!((r.i32().unwrap(), r.i32().unwrap()), (2, -1));
        // the air section is not sent: sections 1 and 2 are
        assert_eq!(r.long_array().unwrap(), [0b110]);
        nbt::read_named_optional(&mut r).unwrap();
        let biomes = r.list(|r| r.varint()).unwrap();
        assert_eq!(biomes.len(), 3 * 64);
        assert!(biomes[..64].iter().all(|b| *b == 4) && biomes[64..128].iter().all(|b| *b == 9));
        assert_eq!(biomes[128..], (0..64).collect::<Vec<i32>>());
        let mut sr = Reader::new(r.byte_array().unwrap());
        // the stone section: a palette of one and 4096 places pointing at it
        assert_eq!((sr.i16().unwrap(), sr.u8().unwrap(), sr.list(|r| r.varint()).unwrap()), (4096, 4, vec![1]));
        assert_eq!(sr.long_array().unwrap(), vec![0i64; 256]);
        assert_eq!((sr.i16().unwrap(), sr.u8().unwrap(), sr.list(|r| r.varint()).unwrap()), (4096, 4, vec![0, 10]));
        assert_eq!(sr.long_array().unwrap(), vec![0x1111_1111_1111_1111i64; 256]);
        assert!(sr.is_empty());
        // the sign: local (5, 10) in chunk (2, -1)
        assert_eq!(r.varint().unwrap(), 1);
        let sign = nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert_eq!((sign.get("x").and_then(Tag::as_i32), sign.get("y").and_then(Tag::as_i32), sign.get("z").and_then(Tag::as_i32)), (Some(37), Some(70), Some(-6)));
        assert_eq!(sign.get("id").and_then(Tag::as_str), Some("minecraft:sign"));
        assert!(r.is_empty());
        // a cut-off chunk is an error, not a panic
        let whole = chunk_1_18();
        for end in 0..whole.len() {
            let _ = level_chunk(&whole[..end], 6);
        }
    }

    #[test]
    fn login_loses_the_simulation_distance_and_biomes_get_their_old_fields() {
        let mut biome = Tag::compound();
        biome.insert("category", Tag::String("mountain".into()));
        let mut entry = Tag::compound();
        entry.insert("element", biome);
        let mut registry = Tag::compound();
        registry.insert("value", Tag::List(vec![entry.clone(), entry.clone(), entry]));
        let mut registries = Tag::compound();
        registries.insert("minecraft:worldgen/biome", registry);
        let mut w = Writer::new();
        w.i32(9).bool(false).u8(0).i8(-1).varint(1).string("minecraft:overworld");
        nbt::write_named(&mut w, &registries);
        nbt::write_named(&mut w, &Tag::compound());
        w.string("minecraft:overworld").i64(5).varint(20).varint(10).varint(8).raw(&[0, 1, 0, 0]);
        let (out, biome_bits) = login(&w.buf).unwrap();
        assert_eq!(biome_bits, 2);
        let mut r = Reader::new(&out);
        r.bytes(7).unwrap();
        r.list(|r| r.string().map(|_| ())).unwrap();
        let got = nbt::read_named_optional(&mut r).unwrap().unwrap();
        let first = &got.get("minecraft:worldgen/biome").and_then(|b| b.get("value")).and_then(Tag::as_list).unwrap()[0];
        let element = first.get("element").unwrap();
        assert_eq!(element.get("category").and_then(Tag::as_str), Some("extreme_hills"));
        assert!(element.get("depth").is_some() && element.get("scale").is_some());
        nbt::read_named_optional(&mut r).unwrap();
        assert_eq!(r.string().unwrap(), "minecraft:overworld");
        r.i64().unwrap();
        assert_eq!((r.varint().unwrap(), r.varint().unwrap(), r.remaining()), (20, 10, &[0u8, 1, 0, 0][..]));
    }

    #[test]
    fn tags_names_and_sounds() {
        // item tags are renumbered, and the block tag 1.17.1 insists on is added once
        let mut w = Writer::new();
        w.varint(2).string("minecraft:item").varint(1).string("minecraft:music_discs").varint(2).varint(OTHERSIDE_DISC).varint(1028);
        w.string("minecraft:block").varint(1).string("minecraft:dirt").varint(1).varint(1028);
        let out = update_tags(&w.buf).unwrap();
        let mut want = Writer::new();
        want.varint(2).string("minecraft:item").varint(1).string("minecraft:music_discs").varint(2).varint(WARD_DISC).varint(1027);
        want.string("minecraft:block").varint(2).string("minecraft:dirt").varint(1).varint(1028).string("minecraft:lava_pool_stone_replaceables").varint(0);
        assert_eq!(out, want.buf);
        let again = update_tags(&out).unwrap();
        let name = b"lava_pool_stone_replaceables";
        assert_eq!(again.windows(name.len()).filter(|w| w == name).count(), 1, "it is not added twice");
        // names 1.17.1 would refuse are cut
        let mut w = Writer::new();
        w.string("a_very_long_objective_name").u8(1);
        let mut want = Writer::new();
        want.string("a_very_long_obje").u8(1);
        assert_eq!(name_first(&w.buf).unwrap(), want.buf);
        assert_eq!(cut("äöüß", 2), "äö");
        // a 1.18 sound 1.17.1 lacks plays one it has; the ones both have keep their name's place
        assert_eq!(d::SOUNDS.len(), 1203);
        assert!(d::SOUNDS.iter().all(|old| *old < 1190));
        assert_eq!(sound(&[0, 9]).unwrap(), [0, 9]);
    }
}

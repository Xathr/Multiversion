//! World data 1.20.3 → 1.20.2: chunks, block updates, block entities, block and level events,
//! tags, enabled features.
//!
//! The chunk layout is the same in both versions; block state ids, block entity types and the
//! JSON in block entity names differ.

use super::data_1_20_2 as d;
use super::items::{self, copy_nbt_with, copy_varint, err, fix_json_field};
use crate::proto::buf::{Reader, Result, Writer};
use crate::proto::nbt::Tag;

const BLOCKS_PER_SECTION: usize = 4096;

fn bits_for(count: u32) -> u32 {
    32 - (count - 1).leading_zeros()
}

/// Unpacks a vanilla "no values span longs" compact array.
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
        w.varint(items::block_state(r.varint()?));
        w.long_array(&r.long_array()?);
        return Ok(());
    }
    if bits <= 8 {
        let n = r.varint()?;
        if n < 0 {
            return Err(err("negative palette length"));
        }
        w.u8(bits as u8);
        w.varint(n);
        for _ in 0..n {
            w.varint(items::block_state(r.varint()?));
        }
        w.long_array(&r.long_array()?);
        return Ok(());
    }
    // Direct palette: both versions use 15 bits, so only the values change.
    let longs = r.long_array()?;
    let mapped: Vec<u32> = unpack(&longs, bits, BLOCKS_PER_SECTION).iter().map(|v| items::block_state(*v as i32) as u32).collect();
    let obits = bits.max(bits_for(d::OLD_BLOCK_STATE_COUNT));
    w.u8(obits as u8);
    w.long_array(&pack(&mapped, obits));
    Ok(())
}

fn copy_biomes(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let bits = r.u8()?;
    w.u8(bits);
    if bits == 0 {
        copy_varint(r, w)?;
    } else if bits <= 3 {
        let n = copy_varint(r, w)?;
        for _ in 0..n.max(0) {
            copy_varint(r, w)?;
        }
    }
    w.long_array(&r.long_array()?);
    Ok(())
}

fn fix_block_entity(tag: &mut Tag) -> bool {
    fix_json_field(tag, "CustomName")
}

pub fn level_chunk_with_light(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.raw(r.bytes(8)?); // chunk x, z
    items::copy_nbt(&mut r, &mut w)?; // heightmaps
    let data = r.byte_array()?;
    let mut sr = Reader::new(data);
    let mut sections = Writer::with_capacity(data.len());
    while !sr.is_empty() {
        sections.i16(sr.i16()?);
        block_palette(&mut sr, &mut sections)?;
        copy_biomes(&mut sr, &mut sections)?;
    }
    w.byte_array(&sections.buf);
    let n = r.varint()?;
    let mut entities = Writer::new();
    let mut kept = 0;
    for _ in 0..n.max(0) {
        let xz = r.u8()?;
        let y = r.i16()?;
        let ty = r.varint()?;
        let mut nbt = Writer::new();
        copy_nbt_with(&mut r, &mut nbt, fix_block_entity)?;
        // crafters and trial spawners have no 1.20.2 block entity
        if let Some(t) = items::block_entity(ty) {
            entities.u8(xz).i16(y).varint(t).raw(&nbt.buf);
            kept += 1;
        }
    }
    w.varint(kept);
    w.raw(&entities.buf);
    w.raw(r.remaining()); // light
    Ok(w.into_inner())
}

pub fn block_update(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.i64(r.i64()?);
    w.varint(items::block_state(r.varint()?));
    Ok(w.into_inner())
}

pub fn section_blocks_update(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.i64(r.i64()?);
    let n = copy_varint(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        let v = r.varlong()?;
        w.varlong((i64::from(items::block_state((v >> 12) as i32)) << 12) | (v & 0xfff));
    }
    Ok(w.into_inner())
}

/// BLOCK_ENTITY_DATA: `None` = a block entity 1.20.2 doesn't have.
pub fn block_entity_data(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.i64(r.i64()?);
    let Some(ty) = items::block_entity(r.varint()?) else { return Ok(None) };
    w.varint(ty);
    copy_nbt_with(&mut r, &mut w, fix_block_entity)?;
    Ok(Some(w.into_inner()))
}

pub fn block_event(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.i64(r.i64()?);
    w.raw(r.bytes(2)?);
    w.varint(items::block(r.varint()?));
    Ok(w.into_inner())
}

/// Level events new in 1.20.3 (crafter, trial spawner): 1.20.2 doesn't know them.
const NEW_LEVEL_EVENTS: &[i32] = &[1049, 1050, 2010, 3011, 3012, 3013];

/// LEVEL_EVENT: `None` = dropped.
pub fn level_event(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    let event = r.i32()?;
    if NEW_LEVEL_EVENTS.contains(&event) {
        return Ok(None);
    }
    w.i32(event);
    w.i64(r.i64()?);
    let data = r.i32()?;
    w.i32(match event {
        // block break / brushing finished: block state; jukebox: record item
        2001 | 3008 => items::block_state(data),
        1010 => items::item(data),
        _ => data,
    });
    w.raw(r.remaining());
    Ok(Some(w.into_inner()))
}

/// UPDATE_TAGS (play and configuration): remap ids; blocks 1.20.2 lacks are left out.
pub fn update_tags(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let n = copy_varint(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        let registry = r.string()?;
        w.string(registry);
        let key = registry.strip_prefix("minecraft:").unwrap_or(registry);
        let count = copy_varint(&mut r, &mut w)?;
        for _ in 0..count.max(0) {
            items::copy_string(&mut r, &mut w)?;
            let ids = r.list(|r| r.varint())?;
            let mapped: Vec<i32> = match key {
                "block" => ids.into_iter().filter(|id| items::block_known(*id)).map(items::block).collect(),
                "item" => ids.into_iter().map(items::item).collect(),
                "entity_type" => ids.into_iter().map(items::entity).collect(),
                _ => ids,
            };
            w.varint(mapped.len() as i32);
            for id in mapped {
                w.varint(id);
            }
        }
    }
    Ok(w.into_inner())
}

/// UPDATE_ENABLED_FEATURES: 1.20.2 has no update_1_21 experiment.
pub fn enabled_features(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let features = r.list(|r| r.string())?;
    let kept: Vec<&str> = features.into_iter().filter(|f| *f != "minecraft:update_1_21").collect();
    let mut w = Writer::new();
    w.varint(kept.len() as i32);
    for f in kept {
        w.string(f);
    }
    Ok(w.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::nbt;

    #[test]
    fn global_bits_are_15() {
        assert_eq!(bits_for(d::NEW_BLOCK_STATE_COUNT), 15);
        assert_eq!(bits_for(d::OLD_BLOCK_STATE_COUNT), 15);
    }

    #[test]
    fn block_state_table_is_sane() {
        assert_eq!(d::BLOCK_STATES.len(), d::NEW_BLOCK_STATE_COUNT as usize);
        assert!(d::BLOCK_STATES.iter().all(|s| *s < d::OLD_BLOCK_STATE_COUNT));
        // air, stone and the start of the table are unchanged
        assert_eq!(items::block_state(0), 0);
        assert_eq!(items::block_state(1), 1);
        // the last 1.20.3 states (new blocks) still land on real 1.20.2 states, and none on stone
        let stone: Vec<i32> = (2..d::NEW_BLOCK_STATE_COUNT as i32).filter(|s| items::block_state(*s) == 1).collect();
        assert!(stone.is_empty(), "states shown as stone: {stone:?}");
        // reverse table agrees for states both versions have
        for old in [0u32, 1, 100, 5000, d::WHITE_CARPET_STATE_OLD, d::OLD_BLOCK_STATE_COUNT - 1] {
            let new = d::BLOCK_STATES_REVERSE[old as usize];
            assert_eq!(d::BLOCK_STATES[new as usize], old, "1.20.2 state {old}");
        }
    }

    #[test]
    fn chunk_palettes_and_block_entities() {
        let last = d::NEW_BLOCK_STATE_COUNT - 1;
        let mut values = vec![1u32; BLOCKS_PER_SECTION];
        values[0] = last;
        let mut s = Writer::new();
        s.i16(4096);
        s.u8(15);
        s.long_array(&pack(&values, 15));
        s.u8(0).varint(3).varint(0); // biomes
        s.i16(1);
        s.u8(4).varint(2).varint(0).varint(last as i32);
        s.long_array(&pack(&[1; BLOCKS_PER_SECTION][..], 4));
        s.u8(0).varint(3).varint(0);
        let crafter = d::NEW_BLOCK_ENTITY_NAMES.iter().position(|n| *n == "crafter").unwrap() as i32;
        let chest = d::NEW_BLOCK_ENTITY_NAMES.iter().position(|n| *n == "chest").unwrap() as i32;
        let mut w = Writer::new();
        w.i32(1).i32(2);
        nbt::write(&mut w, &Tag::compound());
        w.byte_array(&s.buf);
        w.varint(2);
        w.u8(0).i16(5).varint(crafter);
        nbt::write(&mut w, &Tag::compound());
        w.u8(1).i16(6).varint(chest);
        nbt::write(&mut w, &Tag::Compound(vec![("CustomName".into(), Tag::String(r#"{"text":"Loot"}"#.into()))]));
        w.raw(&[9, 9]); // light, untouched
        let out = level_chunk_with_light(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!((r.i32().unwrap(), r.i32().unwrap()), (1, 2));
        nbt::read(&mut r).unwrap();
        let data = r.byte_array().unwrap().to_vec();
        let mut sr = Reader::new(&data);
        sr.i16().unwrap();
        assert_eq!(sr.u8().unwrap(), 15);
        let got = unpack(&sr.long_array().unwrap(), 15, BLOCKS_PER_SECTION);
        assert_eq!((got[0], got[1]), (d::BLOCK_STATES[last as usize], 1));
        sr.bytes(3).unwrap();
        sr.i16().unwrap();
        assert_eq!(sr.u8().unwrap(), 4);
        assert_eq!(sr.varint().unwrap(), 2);
        assert_eq!(sr.varint().unwrap(), 0);
        assert_eq!(sr.varint().unwrap(), items::block_state(last as i32));
        assert_eq!(r.varint().unwrap(), 1, "crafter dropped");
        assert_eq!((r.u8().unwrap(), r.i16().unwrap()), (1, 6));
        assert_eq!(r.varint().unwrap(), d::OLD_BLOCK_ENTITY_NAMES.iter().position(|n| *n == "chest").unwrap() as i32);
        nbt::read(&mut r).unwrap();
        assert_eq!(r.remaining(), &[9, 9]);
    }

    #[test]
    fn tags_leave_out_unknown_blocks() {
        let tuff_wall = d::NEW_BLOCK_NAMES.iter().position(|n| *n == "tuff_wall").unwrap() as i32;
        let cobble_wall = d::NEW_BLOCK_NAMES.iter().position(|n| *n == "cobblestone_wall").unwrap() as i32;
        let mut w = Writer::new();
        w.varint(1).string("minecraft:block").varint(1).string("minecraft:walls").varint(2).varint(tuff_wall).varint(cobble_wall);
        let out = update_tags(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        r.varint().unwrap();
        r.string().unwrap();
        r.varint().unwrap();
        r.string().unwrap();
        assert_eq!(r.list(|r| r.varint()).unwrap(), vec![items::block(cobble_wall)]);
    }

    #[test]
    fn level_events() {
        let mut w = Writer::new();
        w.i32(2001).i64(0).i32(d::NEW_BLOCK_STATE_COUNT as i32 - 1).bool(false);
        let out = level_event(&w.buf).unwrap().unwrap();
        let mut r = Reader::new(&out);
        r.i32().unwrap();
        r.i64().unwrap();
        assert_eq!(r.i32().unwrap(), items::block_state(d::NEW_BLOCK_STATE_COUNT as i32 - 1));
        let mut w = Writer::new();
        w.i32(3012).i64(0).i32(0).bool(false);
        assert!(level_event(&w.buf).unwrap().is_none());
    }

    #[test]
    fn update_1_21_feature_is_removed() {
        let mut w = Writer::new();
        w.varint(2).string("minecraft:vanilla").string("minecraft:update_1_21");
        let out = enabled_features(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.list(|r| r.string().map(str::to_string)).unwrap(), vec!["minecraft:vanilla".to_string()]);
    }
}

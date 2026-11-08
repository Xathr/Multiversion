//! World and player packets 1.20.2 → 1.20.1: chunks, block entities, block states, sounds, the
//! LOGIN / RESPAWN layouts and player spawns.

use super::data_1_20 as d;
use super::items::{self, block_state, effect_number, nbt_to_client};
use crate::proto::buf::{Reader, Result, Writer};
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

/// Block entity NBT 1.20.2 → 1.20.1: beacon effects by number, skull owners with a uuid.
fn block_entity_tag(r: &mut Reader, w: &mut Writer) -> Result<()> {
    match nbt::read_optional(r)? {
        Some(mut t) => {
            for (new, old) in [("primary_effect", "Primary"), ("secondary_effect", "Secondary")] {
                if let Some(Tag::String(key)) = t.get(new).cloned() {
                    t.remove(new);
                    t.insert(old, Tag::Int(effect_number(&key)));
                }
            }
            items::skull_owner_id(&mut t);
            nbt::write_named(w, &t);
        }
        None => {
            w.u8(0);
        }
    }
    Ok(())
}

pub fn level_chunk_with_light(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 64);
    w.raw(r.bytes(8)?); // chunk x, z
    nbt_to_client(&mut r, &mut w)?; // heightmaps
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
    w.varint(n);
    for _ in 0..n.max(0) {
        w.u8(r.u8()?); // packed xz
        w.i16(r.i16()?); // y
        w.varint(r.varint()?); // type (same registry)
        block_entity_tag(&mut r, &mut w)?;
    }
    w.raw(r.remaining()); // light data: same layout
    Ok(w.into_inner())
}

pub fn block_entity_data(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 2);
    w.i64(r.i64()?);
    w.varint(r.varint()?);
    block_entity_tag(&mut r, &mut w)?;
    Ok(w.into_inner())
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
    let n = r.varint()?;
    w.varint(n);
    for _ in 0..n.max(0) {
        let v = r.varlong()?;
        w.varlong((i64::from(block_state((v >> 12) as i32)) << 12) | (v & 0xfff));
    }
    Ok(w.into_inner())
}

pub fn level_event(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    let event = r.i32()?;
    w.i32(event);
    w.i64(r.i64()?);
    let data = r.i32()?;
    w.i32(if event == 2001 { block_state(data) } else { data }); // block break particles
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// SOUND / SOUND_ENTITY: `None` for a sound 1.20.1 doesn't have (sponge absorbing water).
pub fn sound(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let holder = r.varint()?;
    if holder == 0 {
        return Ok(Some(payload.to_vec())); // inline sound name
    }
    let id = holder - 1;
    let mapped = d::SOUNDS.get(id as usize).map_or(0, |v| *v as i32);
    if mapped == 0 && id != 0 {
        return Ok(None);
    }
    w.varint(mapped + 1);
    w.raw(r.remaining());
    Ok(Some(w.into_inner()))
}

/// 1.20.2 packs the chunk position into one long (x low, z high); 1.20.1 sends two ints.
pub fn forget_level_chunk(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let pos = r.i64()?;
    let mut w = Writer::new();
    w.i32(pos as i32);
    w.i32((pos >> 32) as i32);
    Ok(w.into_inner())
}

pub fn tag_query(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 2);
    w.varint(r.varint()?);
    nbt_to_client(&mut r, &mut w)?;
    Ok(w.into_inner())
}

/// 1.20.1 mob effect ids start at 1.
pub fn update_mob_effect(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 3);
    w.varint(r.varint()?); // entity
    w.varint(r.varint()? + 1);
    w.i8(r.i8()?); // amplifier
    w.varint(r.varint()?); // duration
    w.i8(r.i8()?); // flags
    let has_factor = r.bool()?;
    w.bool(has_factor);
    if has_factor {
        nbt_to_client(&mut r, &mut w)?;
    }
    Ok(w.into_inner())
}

pub fn remove_mob_effect(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.varint(r.varint()?);
    w.varint(r.varint()? + 1);
    Ok(w.into_inner())
}

/// 1.20.1 sends the display slot as a byte.
pub fn set_display_objective(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.i8(r.varint()? as i8);
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// Map decorations 1.20.1 doesn't have become the red marker.
pub fn map_item_data(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.varint(r.varint()?); // map id
    w.i8(r.i8()?); // scale
    w.bool(r.bool()?); // locked
    let has_icons = r.bool()?;
    w.bool(has_icons);
    if has_icons {
        let n = r.varint()?;
        w.varint(n);
        for _ in 0..n.max(0) {
            let ty = r.varint()?;
            w.varint(if ty < 27 { ty } else { 2 });
            w.raw(r.bytes(3)?); // x, z, rotation
            let named = r.bool()?;
            w.bool(named);
            if named {
                w.string(r.string()?);
            }
        }
    }
    w.raw(r.remaining()); // colour patch
    Ok(w.into_inner())
}

fn death_location(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let some = r.bool()?;
    w.bool(some);
    if some {
        w.string(r.string()?);
        w.i64(r.i64()?);
    }
    Ok(())
}

/// LOGIN 1.20.2 → 1.20.1: the game modes move to the front, the registries (sent in
/// configuration since 1.20.2) come back as a named compound, and do_limited_crafting goes.
pub fn login(payload: &[u8], registries: &Tag) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let entity = r.i32()?;
    let hardcore = r.bool()?;
    let dimensions = r.list(|r| r.string().map(str::to_string))?;
    let max_players = r.varint()?;
    let view = r.varint()?;
    let simulation = r.varint()?;
    let reduced_debug = r.bool()?;
    let respawn_screen = r.bool()?;
    r.bool()?; // do limited crafting
    let dimension_type = r.string()?;
    let dimension = r.string()?;
    let seed = r.i64()?;
    let game_mode = r.u8()?;
    let previous = r.i8()?;
    let mut w = Writer::with_capacity(payload.len() + 16 * 1024);
    w.i32(entity).bool(hardcore).u8(game_mode).i8(previous);
    w.varint(dimensions.len() as i32);
    for d in &dimensions {
        w.string(d);
    }
    nbt::write_named(&mut w, registries);
    w.string(dimension_type).string(dimension).i64(seed);
    w.varint(max_players).varint(view).varint(simulation).bool(reduced_debug).bool(respawn_screen);
    w.raw(r.remaining()); // debug, flat, death location, portal cooldown
    Ok(w.into_inner())
}

/// RESPAWN: 1.20.1 has the "data kept" flags before the death location and portal cooldown.
pub fn respawn(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.string(r.string()?); // dimension type
    w.string(r.string()?); // dimension
    w.i64(r.i64()?); // seed
    w.u8(r.u8()?); // game mode
    w.i8(r.i8()?); // previous
    w.bool(r.bool()?); // debug
    w.bool(r.bool()?); // flat
    let mut tail = Writer::new();
    death_location(&mut r, &mut tail)?;
    tail.varint(r.varint()?); // portal cooldown
    w.i8(r.i8()?); // data kept
    w.raw(&tail.buf);
    Ok(w.into_inner())
}

/// What ADD_ENTITY became: the packets for the client, and the entity's id and type to track.
pub struct Spawn {
    pub packets: Vec<(i32, Vec<u8>)>,
    pub entity: i32,
    pub ty: i32,
}

/// ADD_ENTITY: players become ADD_PLAYER (+ head rotation and motion, which it lacks); falling
/// blocks carry a block state.
pub fn add_entity(payload: &[u8]) -> Result<Spawn> {
    use super::ids::old;
    let mut r = Reader::new(payload);
    let entity = r.varint()?;
    let uuid = r.uuid()?;
    let ty = r.varint()?;
    let name = d::NEW_ENTITY_NAMES.get(ty as usize).copied().unwrap_or("");
    let mapped = d::ENTITIES.get(ty as usize).map_or(0, |v| *v as i32);
    let (x, y, z) = (r.f64()?, r.f64()?, r.f64()?);
    let (pitch, yaw, head) = (r.i8()?, r.i8()?, r.i8()?);
    let data = r.varint()?;
    let (vx, vy, vz) = (r.i16()?, r.i16()?, r.i16()?);
    let mut packets = Vec::new();
    if name == "player" {
        let mut w = Writer::new();
        w.varint(entity).uuid(uuid).f64(x).f64(y).f64(z).i8(yaw).i8(pitch);
        packets.push((old::ADD_PLAYER, w.into_inner()));
        let mut w = Writer::new();
        w.varint(entity).i8(head);
        packets.push((old::ROTATE_HEAD, w.into_inner()));
        if (vx, vy, vz) != (0, 0, 0) {
            let mut w = Writer::new();
            w.varint(entity).i16(vx).i16(vy).i16(vz);
            packets.push((old::SET_ENTITY_MOTION, w.into_inner()));
        }
    } else {
        let data = if name == "falling_block" { block_state(data) } else { data };
        let mut w = Writer::with_capacity(payload.len());
        w.varint(entity).uuid(uuid).varint(mapped).f64(x).f64(y).f64(z).i8(pitch).i8(yaw).i8(head).varint(data);
        w.i16(vx).i16(vy).i16(vz);
        packets.push((super::ids::play_clientbound(super::ids::new::ADD_ENTITY).unwrap_or(1), w.into_inner()));
    }
    Ok(Spawn { packets, entity, ty })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(names: &[&str], name: &str) -> i32 {
        names.iter().position(|n| *n == name).unwrap() as i32
    }

    #[test]
    fn chunk_nbt_gets_root_names_and_states_are_mapped() {
        // first powered player head state in 1.20.2: 1.20.1 has no "powered"
        let head = d::BLOCK_STATES.iter().enumerate().find(|(i, v)| **v as usize != *i).map(|(i, _)| i as i32).unwrap();
        let mut sec = Writer::new();
        sec.i16(1);
        sec.u8(0).varint(head).varint(0); // single value
        sec.u8(0).varint(0).varint(0);
        let mut w = Writer::new();
        w.i32(1).i32(2);
        nbt::write(&mut w, &Tag::Compound(vec![("MOTION_BLOCKING".into(), Tag::LongArray(vec![0; 37]))]));
        w.byte_array(&sec.buf);
        w.varint(1);
        w.u8(0x11).i16(64).varint(14); // beacon
        nbt::write(&mut w, &Tag::Compound(vec![("primary_effect".into(), Tag::String("minecraft:haste".into()))]));
        w.raw(&[9, 9, 9]); // light: copied
        let out = level_chunk_with_light(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!((r.i32().unwrap(), r.i32().unwrap()), (1, 2));
        let hm = nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert!(hm.get("MOTION_BLOCKING").is_some());
        let data = r.byte_array().unwrap().to_vec();
        let mut s = Reader::new(&data);
        s.i16().unwrap();
        assert_eq!(s.u8().unwrap(), 0);
        assert_eq!(s.varint().unwrap(), block_state(head));
        assert_ne!(block_state(head), head);
        assert_eq!(r.varint().unwrap(), 1);
        r.u8().unwrap();
        r.i16().unwrap();
        r.varint().unwrap();
        let be = nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert_eq!(be.get("Primary"), Some(&Tag::Int(3)));
        assert!(be.get("primary_effect").is_none());
        assert_eq!(r.remaining(), &[9, 9, 9]);
    }

    /// Barriers gained "waterlogged" and heads "powered" in 1.20.2: both fold into the 1.20.1
    /// state with the other properties kept.
    #[test]
    fn new_block_properties_fold_into_the_old_state() {
        for (new, old) in [(10365, 10225), (10366, 10225), (8967, 8891)] {
            let mut w = Writer::new();
            w.i64(0).varint(new);
            let out = block_update(&w.buf).unwrap();
            let mut r = Reader::new(&out);
            r.i64().unwrap();
            assert_eq!(r.varint().unwrap(), old, "1.20.2 state {new}");
        }
    }

    #[test]
    fn player_spawn_becomes_add_player() {
        let player = state(&d::NEW_ENTITY_NAMES, "player");
        let mut w = Writer::new();
        w.varint(7).uuid(42).varint(player).f64(1.0).f64(2.0).f64(3.0).i8(10).i8(20).i8(30).varint(0).i16(0).i16(5).i16(0);
        let spawn = add_entity(&w.buf).unwrap();
        assert_eq!(spawn.packets.len(), 3);
        let (id, p) = &spawn.packets[0];
        assert_eq!(*id, super::super::ids::old::ADD_PLAYER);
        let mut want = Writer::new();
        want.varint(7).uuid(42).f64(1.0).f64(2.0).f64(3.0).i8(20).i8(10); // yaw, then pitch
        assert_eq!(*p, want.buf);
        let mut head = Writer::new();
        head.varint(7).i8(30);
        assert_eq!(spawn.packets[1], (super::super::ids::old::ROTATE_HEAD, head.buf));
        let mut motion = Writer::new();
        motion.varint(7).i16(0).i16(5).i16(0);
        assert_eq!(spawn.packets[2], (super::super::ids::old::SET_ENTITY_MOTION, motion.buf));
        // other entities stay ADD_ENTITY, same layout
        let pig = state(&d::NEW_ENTITY_NAMES, "pig");
        let mut w = Writer::new();
        w.varint(8).uuid(1).varint(pig).f64(0.0).f64(0.0).f64(0.0).i8(1).i8(2).i8(3).varint(0).i16(0).i16(0).i16(0);
        let spawn = add_entity(&w.buf).unwrap();
        assert_eq!(spawn.packets, vec![(1, w.buf.clone())]);
    }

    #[test]
    fn respawn_moves_data_kept() {
        let mut w = Writer::new();
        w.string("minecraft:overworld").string("minecraft:overworld").i64(5).u8(1).i8(-1).bool(false).bool(true);
        w.bool(false).varint(3).i8(0x03);
        let out = respawn(&w.buf).unwrap();
        let mut want = Writer::new();
        want.string("minecraft:overworld").string("minecraft:overworld").i64(5).u8(1).i8(-1).bool(false).bool(true);
        want.i8(0x03).bool(false).varint(3);
        assert_eq!(out, want.buf);
    }

    #[test]
    fn forget_chunk_splits_the_position() {
        let mut w = Writer::new();
        w.i64((i64::from(-3i32) << 32) | i64::from(5u32));
        let out = forget_level_chunk(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!((r.i32().unwrap(), r.i32().unwrap()), (5, -3));
    }

    #[test]
    fn sponge_absorb_is_dropped() {
        let absorb = state(&d::NEW_SOUND_NAMES, "block.sponge.absorb");
        let mut w = Writer::new();
        w.varint(absorb + 1).varint(0);
        assert_eq!(sound(&w.buf).unwrap(), None);
        let mut w = Writer::new();
        w.varint(1).varint(0); // id 0 maps to 0
        assert_eq!(sound(&w.buf).unwrap(), Some(w.buf.clone()));
    }
}

//! World and player packets 1.20 → 1.19.4: chunks and light (1.19.4 has a "trust edges" flag),
//! signs (1.20 gave them two sides), block states, sounds, tags, statistics and the LOGIN /
//! RESPAWN layouts (1.20 added a portal cooldown).

use super::super::data_1_19_4 as d;
use super::super::table::SameName;
use super::items::{block_state, string_copy, varint_copy, SAME_BLOCKS, SAME_ITEMS};
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

const BLOCKS_PER_SECTION: usize = 4096;
/// Smithing templates of the trim patterns 1.20 added. 1.19.4 has no such items, and reads the
/// trim pattern registry by item name, so they are pointed at one it has.
const NEW_TRIM_TEMPLATES: [&str; 5] = [
    "host_armor_trim_smithing_template",
    "raiser_armor_trim_smithing_template",
    "silence_armor_trim_smithing_template",
    "shaper_armor_trim_smithing_template",
    "wayfinder_armor_trim_smithing_template",
];

fn plain(key: &str) -> &str {
    key.strip_prefix("minecraft:").unwrap_or(key)
}

fn bits_for(count: u32) -> u32 {
    32 - (count - 1).leading_zeros()
}

fn unpack(longs: &[i64], bits: u32, count: usize) -> Vec<u32> {
    let per_long = 64 / (bits as usize).clamp(1, 64);
    let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
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
        return Err(DecodeError::Other(format!("block palette of {bits} bits")));
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

/// A 1.20 sign has a front and a back, each with its own text, colour and glow. A 1.19.4 sign
/// has one face: it gets the front.
pub fn sign_to_client(tag: &mut Tag) {
    let front = tag.remove("front_text");
    tag.remove("back_text");
    tag.remove("is_waxed");
    let Some(mut front) = front else { return };
    for (list, prefix) in [("messages", "Text"), ("filtered_messages", "FilteredText")] {
        if let Some(Tag::List(lines)) = front.remove(list) {
            for (i, line) in lines.into_iter().take(4).enumerate() {
                tag.insert(&format!("{prefix}{}", i + 1), line);
            }
        }
    }
    if let Some(color) = front.remove("color") {
        tag.insert("Color", color);
    }
    if let Some(glow) = front.remove("has_glowing_text") {
        tag.insert("GlowingText", glow);
    }
}

/// A block entity's type and data for 1.19.4, or `None` for a type 1.19.4 does not have.
fn block_entity(ty: i32, r: &mut Reader) -> Result<Option<(i32, Option<Tag>)>> {
    let mut tag = nbt::read_named_optional(r)?;
    let name = d::NEW_BLOCK_ENTITY_NAMES.get(ty as usize).copied().unwrap_or("");
    // 1.20 renamed the suspicious sand block entity when gravel joined it.
    let old_name = if name == "brushable_block" { "suspicious_sand" } else { name };
    let Some(old) = d::OLD_BLOCK_ENTITY_NAMES.iter().position(|n| *n == old_name) else { return Ok(None) };
    if (name == "sign" || name == "hanging_sign")
        && let Some(t) = tag.as_mut()
    {
        sign_to_client(t);
    }
    Ok(Some((old as i32, tag)))
}

fn write_tag(w: &mut Writer, tag: Option<&Tag>) {
    match tag {
        Some(t) => nbt::write_named(w, t),
        None => {
            w.u8(0);
        }
    }
}

pub fn level_chunk_with_light(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 64);
    w.raw(r.bytes(8)?); // chunk x, z
    let heightmaps = nbt::read_named_optional(&mut r)?;
    write_tag(&mut w, heightmaps.as_ref());
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
    if n < 0 || n as usize > r.len() {
        return Err(DecodeError::Other("more block entities than the packet holds".into()));
    }
    let mut entities = Writer::new();
    let mut kept = 0;
    for _ in 0..n {
        let xz = r.u8()?;
        let y = r.i16()?;
        let ty = r.varint()?;
        if let Some((old, tag)) = block_entity(ty, &mut r)? {
            entities.u8(xz).i16(y).varint(old);
            write_tag(&mut entities, tag.as_ref());
            kept += 1;
        }
    }
    w.varint(kept);
    w.raw(&entities.buf);
    w.bool(true); // trust edges: 1.20 dropped the flag and always behaves as if it were set
    w.raw(r.remaining()); // light masks and arrays: same layout
    Ok(w.into_inner())
}

pub fn light_update(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 1);
    varint_copy(&mut r, &mut w)?; // chunk x
    varint_copy(&mut r, &mut w)?; // chunk z
    w.bool(true); // trust edges
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// `None` when 1.19.4 has no such block entity.
pub fn block_entity_data(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 2);
    w.i64(r.i64()?);
    let ty = r.varint()?;
    let Some((old, tag)) = block_entity(ty, &mut r)? else { return Ok(None) };
    w.varint(old);
    write_tag(&mut w, tag.as_ref());
    Ok(Some(w.into_inner()))
}

pub fn block_update(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.i64(r.i64()?);
    w.varint(block_state(r.varint()?));
    Ok(w.into_inner())
}

pub fn block_event(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.i64(r.i64()?);
    w.raw(r.bytes(2)?); // action id and parameter
    let block = r.varint()?;
    w.varint(d::BLOCKS.get(block as usize).map_or(0, |v| *v as i32));
    Ok(w.into_inner())
}

pub fn section_blocks_update(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 1);
    w.i64(r.i64()?);
    w.bool(false); // suppress light updates: 1.20 dropped the flag
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
    w.i32(match event {
        1010 => d::ITEMS.get(data as usize).map_or(0, |v| *v as i32), // jukebox: the record's item id
        2001 => block_state(data),                                     // block break particles
        _ => data,
    });
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// SOUND / SOUND_ENTITY: the sound is "0 and a name" or a registry id plus one.
pub fn sound(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let holder = r.varint()?;
    if holder == 0 {
        return Ok(payload.to_vec());
    }
    let mut w = Writer::with_capacity(payload.len());
    w.varint(d::SOUNDS.get((holder - 1) as usize).map_or(0, |v| *v as i32) + 1);
    w.raw(r.remaining());
    Ok(w.into_inner())
}

pub fn add_entity(payload: &[u8]) -> Result<(i32, i32, Vec<u8>)> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let entity = varint_copy(&mut r, &mut w)?;
    w.raw(r.bytes(16)?); // uuid
    let ty = varint_copy(&mut r, &mut w)?; // entity types have the same ids
    w.raw(r.bytes(24 + 3)?); // position, pitch, yaw, head yaw
    let data = r.varint()?;
    let falling_block = d::NEW_ENTITY_NAMES.get(ty as usize).is_some_and(|n| *n == "falling_block");
    w.varint(if falling_block { block_state(data) } else { data });
    w.raw(r.remaining()); // velocity
    Ok((entity, ty, w.into_inner()))
}

fn death_location(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let some = r.bool()?;
    w.bool(some);
    if some {
        string_copy(r, w)?;
        w.i64(r.i64()?);
    }
    Ok(())
}

pub fn login(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.raw(r.bytes(4 + 1 + 1 + 1)?); // entity id, hardcore, game mode, previous game mode
    let worlds = varint_copy(&mut r, &mut w)?;
    for _ in 0..worlds.max(0) {
        string_copy(&mut r, &mut w)?;
    }
    let mut registries = nbt::read_named_optional(&mut r)?;
    if let Some(patterns) = registries.as_mut().and_then(|t| t.get_mut("minecraft:trim_pattern")).and_then(|t| t.get_mut("value")).and_then(Tag::as_list_mut) {
        for entry in patterns {
            if let Some(element) = entry.get_mut("element") {
                let is_new = element.get("template_item").and_then(Tag::as_str).is_some_and(|item| NEW_TRIM_TEMPLATES.contains(&plain(item)));
                if is_new {
                    element.insert("template_item", Tag::String("minecraft:spire_armor_trim_smithing_template".into()));
                }
            }
        }
    }
    write_tag(&mut w, registries.as_ref());
    string_copy(&mut r, &mut w)?; // dimension type
    string_copy(&mut r, &mut w)?; // dimension
    w.i64(r.i64()?); // hashed seed
    for _ in 0..3 {
        varint_copy(&mut r, &mut w)?; // max players, view distance, simulation distance
    }
    w.raw(r.bytes(4)?); // reduced debug info, respawn screen, debug, flat
    death_location(&mut r, &mut w)?;
    r.varint()?; // portal cooldown: new in 1.20
    Ok(w.into_inner())
}

pub fn respawn(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    string_copy(&mut r, &mut w)?; // dimension type
    string_copy(&mut r, &mut w)?; // dimension
    w.raw(r.bytes(8 + 1 + 1 + 1 + 1 + 1)?); // seed, game mode, previous, debug, flat, data kept
    death_location(&mut r, &mut w)?;
    r.varint()?; // portal cooldown: new in 1.20
    Ok(w.into_inner())
}

/// COMBAT_END: 1.19.4 still carries the (unused) id of the entity that was fought.
pub fn combat_end(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    varint_copy(&mut r, &mut w)?; // duration
    w.i32(-1);
    Ok(w.into_inner())
}

/// COMBAT_KILL: likewise the killer's entity id, between the player and the message.
pub fn combat_kill(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 4);
    varint_copy(&mut r, &mut w)?; // player id
    w.i32(-1);
    w.raw(r.remaining()); // message
    Ok(w.into_inner())
}

/// The 1.20 features are a feature flag in 1.19.4: without it the client hides the 1.20 blocks
/// and items it does have (cherry, bamboo wood, hanging signs, ...).
pub fn enabled_features(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let features = r.list(|r| r.string().map(str::to_string))?;
    let mut w = Writer::with_capacity(payload.len() + 24);
    let extra = "minecraft:update_1_20";
    let has = features.iter().any(|f| f == extra);
    w.varint(features.len() as i32 + i32::from(!has));
    for f in &features {
        w.string(f);
    }
    if !has {
        w.string(extra);
    }
    Ok(w.into_inner())
}

pub fn open_screen(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    varint_copy(&mut r, &mut w)?; // container id
    let menu = r.varint()?;
    w.varint(d::MENUS.get(menu as usize).map_or(0, |v| *v as i32));
    w.raw(r.remaining()); // title
    Ok(w.into_inner())
}

/// Ids of things 1.19.4 also has, as 1.19.4 numbers them. Tags must not gain the stand-ins:
/// a lilac standing in for a pitcher plant is not suddenly a sniffer's food.
fn shared_ids(ids: &[i32], same: &SameName) -> Vec<i32> {
    ids.iter().filter_map(|id| same.get(*id)).collect()
}

pub fn update_tags(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 32);
    let registries = varint_copy(&mut r, &mut w)?;
    for _ in 0..registries.max(0) {
        let registry = r.string()?;
        w.string(registry);
        let kind = plain(registry);
        let n = r.varint()?;
        if n < 0 || n as usize > r.len() {
            return Err(DecodeError::Other("more tags than the packet holds".into()));
        }
        let mut body = Writer::new();
        let mut count = n;
        let mut has_replaceable_plants = false;
        for _ in 0..n {
            let name = r.string()?;
            body.string(name);
            has_replaceable_plants |= plain(name) == "replaceable_plants";
            let ids = r.list(Reader::varint)?;
            let ids = match kind {
                "block" => shared_ids(&ids, &SAME_BLOCKS),
                "item" => shared_ids(&ids, &SAME_ITEMS),
                _ => ids, // entity types, fluids, game events...: the same in both versions
            };
            body.varint(ids.len() as i32);
            for id in ids {
                body.varint(id);
            }
        }
        // 1.20 removed this tag; a 1.19.4 client refuses to join without it.
        if kind == "block" && !has_replaceable_plants {
            body.string("minecraft:replaceable_plants").varint(0);
            count += 1;
        }
        w.varint(count);
        w.raw(&body.buf);
    }
    Ok(w.into_inner())
}

/// AWARD_STATS: block, item and entity statistics are counted per id.
pub fn award_stats(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        let category = varint_copy(&mut r, &mut w)?;
        let id = r.varint()?;
        w.varint(match category {
            0 => d::BLOCKS.get(id as usize).map_or(0, |v| *v as i32),      // mined
            1..=5 => d::ITEMS.get(id as usize).map_or(0, |v| *v as i32),   // crafted, used, broken, picked up, dropped
            _ => id,                                                       // killed, killed by (same entity ids), custom
        });
        varint_copy(&mut r, &mut w)?; // value
    }
    Ok(w.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(names: &[&str], name: &str) -> i32 {
        names.iter().position(|n| *n == name).unwrap() as i32
    }

    #[test]
    fn a_two_sided_sign_shows_its_front() {
        let line = |s: &str| Tag::String(format!("{{\"text\":\"{s}\"}}"));
        let mut tag = Tag::Compound(vec![
            ("front_text".into(), Tag::Compound(vec![
                ("messages".into(), Tag::List(vec![line("a"), line("b"), line("c"), line("d")])),
                ("color".into(), Tag::String("red".into())),
                ("has_glowing_text".into(), Tag::Byte(1)),
            ])),
            ("back_text".into(), Tag::Compound(vec![("messages".into(), Tag::List(vec![line("back")]))])),
            ("is_waxed".into(), Tag::Byte(0)),
        ]);
        sign_to_client(&mut tag);
        assert_eq!(tag.get("Text1"), Some(&line("a")));
        assert_eq!(tag.get("Text4"), Some(&line("d")));
        assert_eq!(tag.get("Color"), Some(&Tag::String("red".into())));
        assert_eq!(tag.get("GlowingText"), Some(&Tag::Byte(1)));
        assert!(tag.get("front_text").is_none() && tag.get("back_text").is_none() && tag.get("is_waxed").is_none());
    }

    #[test]
    fn chunk_gets_trust_edges_and_old_ids() {
        let state_new = id(&d::NEW_BLOCK_NAMES, "stone"); // blocks before any change share state ids
        let mut sections = Writer::new();
        sections.i16(4096);
        sections.u8(0).varint(state_new).varint(0); // one block everywhere
        sections.u8(0).varint(3).varint(0); // one biome
        let mut p = Writer::new();
        p.i32(1).i32(2);
        nbt::write_named(&mut p, &Tag::Compound(vec![("MOTION_BLOCKING".into(), Tag::LongArray(vec![0; 37]))]));
        p.byte_array(&sections.buf);
        p.varint(2);
        p.u8(0x12).i16(64).varint(id(&d::NEW_BLOCK_ENTITY_NAMES, "sign"));
        nbt::write_named(&mut p, &Tag::Compound(vec![("front_text".into(), Tag::Compound(vec![("messages".into(), Tag::List(vec![Tag::String("\"x\"".into())]))]))]));
        p.u8(0x34).i16(70).varint(id(&d::NEW_BLOCK_ENTITY_NAMES, "brushable_block"));
        p.u8(0);
        let light = [0u8, 0, 0, 0, 0, 0]; // empty masks and arrays
        p.raw(&light);

        let out = level_chunk_with_light(&p.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!((r.i32().unwrap(), r.i32().unwrap()), (1, 2));
        nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert_eq!(r.byte_array().unwrap(), &sections.buf[..]);
        assert_eq!(r.varint().unwrap(), 2);
        assert_eq!((r.u8().unwrap(), r.i16().unwrap()), (0x12, 64));
        assert_eq!(r.varint().unwrap(), id(&d::OLD_BLOCK_ENTITY_NAMES, "sign"));
        let sign = nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert_eq!(sign.get("Text1"), Some(&Tag::String("\"x\"".into())));
        assert_eq!((r.u8().unwrap(), r.i16().unwrap()), (0x34, 70));
        assert_eq!(r.varint().unwrap(), id(&d::OLD_BLOCK_ENTITY_NAMES, "suspicious_sand"));
        assert_eq!(r.u8().unwrap(), 0);
        assert!(r.bool().unwrap(), "trust edges");
        assert_eq!(r.remaining(), light);
    }

    #[test]
    fn new_blocks_fold_into_their_stand_ins() {
        // every 1.20 state maps to a state 1.19.4 has
        assert!(d::BLOCK_STATES.iter().all(|s| *s < d::OLD_BLOCK_STATE_COUNT));
    }

    #[test]
    fn multi_block_change_gets_the_light_flag() {
        let mut p = Writer::new();
        p.i64(77).varint(1).varlong((5 << 12) | 0x123);
        let out = section_blocks_update(&p.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.i64().unwrap(), 77);
        assert!(!r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), 1);
        assert_eq!(r.varlong().unwrap(), (i64::from(block_state(5)) << 12) | 0x123);
    }

    #[test]
    fn login_and_respawn_lose_the_portal_cooldown() {
        let mut p = Writer::new();
        p.i32(9).bool(false).u8(0).i8(-1);
        p.varint(1).string("minecraft:overworld");
        nbt::write_named(&mut p, &Tag::Compound(vec![("minecraft:trim_pattern".into(), Tag::Compound(vec![("value".into(), Tag::List(vec![
            Tag::Compound(vec![("element".into(), Tag::Compound(vec![("template_item".into(), Tag::String("minecraft:host_armor_trim_smithing_template".into()))]))]),
            Tag::Compound(vec![("element".into(), Tag::Compound(vec![("template_item".into(), Tag::String("minecraft:coast_armor_trim_smithing_template".into()))]))]),
        ]))]))]));
        p.string("minecraft:overworld").string("minecraft:overworld").i64(5);
        p.varint(20).varint(10).varint(8);
        p.bool(false).bool(true).bool(false).bool(false);
        p.bool(false);
        let without_cooldown = p.buf.len();
        p.varint(40);
        let out = login(&p.buf).unwrap();
        // same length apart from the cooldown and the renamed template
        let mut r = Reader::new(&out);
        r.bytes(7).unwrap();
        assert_eq!(r.varint().unwrap(), 1);
        r.string().unwrap();
        let reg = nbt::read_named_optional(&mut r).unwrap().unwrap();
        let items: Vec<&str> = reg.get("minecraft:trim_pattern").unwrap().get("value").unwrap().as_list().unwrap().iter().map(|e| e.get("element").unwrap().get("template_item").unwrap().as_str().unwrap()).collect();
        assert_eq!(items, ["minecraft:spire_armor_trim_smithing_template", "minecraft:coast_armor_trim_smithing_template"]);
        assert_eq!(&out[out.len() - 9..], &p.buf[without_cooldown - 9..without_cooldown], "ends with the death location, not the cooldown");

        let mut p = Writer::new();
        p.string("a").string("b").i64(1).u8(0).i8(0).bool(false).bool(false).i8(1).bool(false);
        let want = p.buf.clone();
        p.varint(300);
        assert_eq!(respawn(&p.buf).unwrap(), want);
    }

    #[test]
    fn tags_keep_only_shared_blocks_and_gain_replaceable_plants() {
        let stone = id(&d::NEW_BLOCK_NAMES, "stone");
        let pitcher = id(&d::NEW_BLOCK_NAMES, "pitcher_plant");
        let mut p = Writer::new();
        p.varint(1).string("minecraft:block").varint(1);
        p.string("minecraft:flowers").varint(2).varint(stone).varint(pitcher);
        let out = update_tags(&p.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.varint().unwrap(), 1);
        assert_eq!(r.string().unwrap(), "minecraft:block");
        assert_eq!(r.varint().unwrap(), 2);
        assert_eq!(r.string().unwrap(), "minecraft:flowers");
        assert_eq!(r.list(Reader::varint).unwrap(), vec![id(&d::OLD_BLOCK_NAMES, "stone")]);
        assert_eq!(r.string().unwrap(), "minecraft:replaceable_plants");
        assert_eq!(r.varint().unwrap(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn combat_packets_get_the_old_entity_field() {
        let mut p = Writer::new();
        p.varint(12);
        assert_eq!(combat_end(&p.buf).unwrap(), [12, 0xff, 0xff, 0xff, 0xff]);
        let mut p = Writer::new();
        p.varint(3).string("died");
        let out = combat_kill(&p.buf).unwrap();
        assert_eq!(&out[..5], [3, 0xff, 0xff, 0xff, 0xff]);
        assert_eq!(Reader::new(&out[5..]).string().unwrap(), "died");
    }

    #[test]
    fn features_gain_the_1_20_flag_once() {
        let mut p = Writer::new();
        p.varint(1).string("minecraft:vanilla");
        let out = enabled_features(&p.buf).unwrap();
        let again = enabled_features(&out).unwrap();
        assert_eq!(out, again);
        assert_eq!(Reader::new(&out).list(|r| r.string().map(str::to_string)).unwrap(), ["minecraft:vanilla", "minecraft:update_1_20"]);
    }
}

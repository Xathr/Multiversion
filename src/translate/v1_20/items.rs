//! Items and the packets that carry them, 1.20.2 ↔ 1.20.1.
//!
//! Item stacks are still NBT based in both versions; what changed is that 1.20.2 network NBT has
//! no root name (1.20.1 sends an empty one) and that custom potion effects are keyed by name
//! (`custom_potion_effects` / `id: "minecraft:speed"`) instead of by number.

use super::data_1_20 as d;
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

/// Mob effects in registry order (1.20.2 ids start at 0, 1.20.1 ids at 1).
pub const EFFECTS: [&str; 33] = [
    "speed", "slowness", "haste", "mining_fatigue", "strength", "instant_health", "instant_damage", "jump_boost", "nausea", "regeneration", "resistance",
    "fire_resistance", "water_breathing", "invisibility", "blindness", "night_vision", "hunger", "weakness", "poison", "wither", "health_boost", "absorption",
    "saturation", "glowing", "levitation", "luck", "unluck", "slow_falling", "conduit_power", "dolphins_grace", "bad_omen", "hero_of_the_village", "darkness",
];

/// 1.20.1 effect number for a key ("minecraft:speed" → 1); 0 = none.
pub fn effect_number(key: &str) -> i32 {
    let key = key.strip_prefix("minecraft:").unwrap_or(key);
    EFFECTS.iter().position(|e| *e == key).map_or(0, |i| i as i32 + 1)
}

/// The key for a 1.20.1 effect number, or `None` for 0 / unknown.
pub fn effect_key(number: i32) -> Option<String> {
    let i = usize::try_from(number - 1).ok()?;
    EFFECTS.get(i).map(|e| format!("minecraft:{e}"))
}

const EFFECT_FIELDS: [(&str, &str); 7] = [
    ("amplifier", "Amplifier"),
    ("duration", "Duration"),
    ("ambient", "Ambient"),
    ("show_particles", "ShowParticles"),
    ("show_icon", "ShowIcon"),
    ("hidden_effect", "HiddenEffect"),
    ("factor_calculation_data", "FactorCalculationData"),
];

fn rename(tag: &mut Tag, from: &str, to: &str) {
    if let Some(v) = tag.remove(from) {
        tag.insert(to, v);
    }
}

/// Item NBT 1.20.2 → 1.20.1.
pub fn tag_to_client(tag: &mut Tag) {
    if let Some(mut list) = tag.remove("custom_potion_effects") {
        for e in list.as_list_mut().into_iter().flatten() {
            if let Some(Tag::String(id)) = e.remove("id") {
                e.insert("Id", Tag::Int(effect_number(&id)));
            }
            for (new, old) in EFFECT_FIELDS {
                rename(e, new, old);
            }
        }
        tag.insert("CustomPotionEffects", list);
    }
    skull_owner_id(tag);
}

/// Item NBT 1.20.1 → 1.20.2.
pub fn tag_to_server(tag: &mut Tag) {
    if let Some(mut list) = tag.remove("CustomPotionEffects") {
        for e in list.as_list_mut().into_iter().flatten() {
            if let Some(n) = e.remove("Id").and_then(|t| t.as_i32())
                && let Some(key) = effect_key(n)
            {
                e.insert("id", Tag::String(key));
            }
            for (new, old) in EFFECT_FIELDS {
                rename(e, old, new);
            }
        }
        tag.insert("custom_potion_effects", list);
    }
}

/// 1.20.2 allows a skull owner with textures but no uuid; 1.20.1 needs one.
pub fn skull_owner_id(tag: &mut Tag) {
    if let Some(owner) = tag.get_mut("SkullOwner")
        && matches!(owner, Tag::Compound(_))
        && owner.get("Id").is_none()
        && owner.get("Properties").is_some()
    {
        owner.insert("Id", Tag::IntArray(vec![0, 0, 0, 0]));
    }
}

/// Copies nameless (1.20.2) optional NBT as named (1.20.1) NBT.
pub fn nbt_to_client(r: &mut Reader, w: &mut Writer) -> Result<()> {
    match nbt::read_optional(r)? {
        Some(t) => nbt::write_named(w, &t),
        None => {
            w.u8(0);
        }
    }
    Ok(())
}

/// An item stack, 1.20.2 → 1.20.1.
pub fn item(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if !present {
        return Ok(());
    }
    let id = r.varint()?;
    w.varint(d::ITEMS.get(id as usize).map_or(0, |v| *v as i32));
    w.i8(r.i8()?);
    match nbt::read_optional(r)? {
        Some(mut t) => {
            tag_to_client(&mut t);
            nbt::write_named(w, &t);
        }
        None => {
            w.u8(0);
        }
    }
    Ok(())
}

/// An item stack, 1.20.1 → 1.20.2.
pub fn item_to_server(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if !present {
        return Ok(());
    }
    let id = r.varint()?;
    w.varint(d::ITEMS_REVERSE.get(id as usize).map_or(0, |v| *v as i32));
    w.i8(r.i8()?);
    match nbt::read_named_optional(r)? {
        Some(mut t) => {
            tag_to_server(&mut t);
            nbt::write(w, &t);
        }
        None => {
            w.u8(0);
        }
    }
    Ok(())
}

pub fn block_state(id: i32) -> i32 {
    d::BLOCK_STATES.get(id as usize).map_or(1, |v| *v as i32)
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

/// Particle id + data. Ids are the same; block particles carry a state, the item one a stack.
pub fn particle(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let id = varint_copy(r, w)?;
    particle_data(id, r, w)
}

fn particle_data(id: i32, r: &mut Reader, w: &mut Writer) -> Result<()> {
    match d::NEW_PARTICLE_NAMES.get(id as usize).copied().unwrap_or("") {
        "block" | "block_marker" | "falling_dust" => {
            w.varint(block_state(r.varint()?));
        }
        "item" => item(r, w)?,
        "dust" => {
            w.raw(r.bytes(16)?);
        }
        "dust_color_transition" => {
            w.raw(r.bytes(28)?);
        }
        "sculk_charge" => {
            w.raw(r.bytes(4)?);
        }
        "shriek" => {
            varint_copy(r, w)?;
        }
        "vibration" => {
            let source = r.string()?;
            w.string(source);
            match source.strip_prefix("minecraft:").unwrap_or(source) {
                "block" => {
                    w.raw(r.bytes(8)?);
                }
                "entity" => {
                    varint_copy(r, w)?;
                    w.raw(r.bytes(4)?);
                }
                other => return Err(DecodeError::Other(format!("unknown vibration source {other}"))),
            }
            varint_copy(r, w)?;
        }
        _ => {}
    }
    Ok(())
}

/// What the step remembers about a spawned entity, for its entity data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Block, item and text displays: 1.20.2 added a field at index 10.
    Display,
    /// Minecarts: index 11 is the displayed block state.
    Minecart,
}

pub fn entity_kind(ty: i32) -> Option<Kind> {
    let name = d::NEW_ENTITY_NAMES.get(ty as usize).copied()?;
    if name.ends_with("_display") {
        Some(Kind::Display)
    } else if name.ends_with("minecart") {
        Some(Kind::Minecart)
    } else {
        None
    }
}

/// SET_ENTITY_DATA body (after the entity id).
pub fn entity_data(kind: Option<Kind>, r: &mut Reader, w: &mut Writer) -> Result<()> {
    loop {
        let index = r.u8()?;
        if index == 0xff {
            w.u8(0xff);
            return Ok(());
        }
        let ty = r.varint()?;
        let mut v = Writer::new();
        match ty {
            0 | 8 => {
                v.raw(r.bytes(1)?);
            }
            1 | 12 | 19..=22 | 24 | 25 => {
                varint_copy(r, &mut v)?;
            }
            2 => {
                v.varlong(r.varlong()?);
            }
            3 => {
                v.raw(r.bytes(4)?);
            }
            4 | 5 => string_copy(r, &mut v)?,
            6 => {
                if r.bool()? {
                    v.bool(true);
                    string_copy(r, &mut v)?;
                } else {
                    v.bool(false);
                }
            }
            7 => item(r, &mut v)?,
            9 | 26 => {
                v.raw(r.bytes(12)?);
            }
            10 => {
                v.raw(r.bytes(8)?);
            }
            11 => {
                let some = r.bool()?;
                v.bool(some);
                if some {
                    v.raw(r.bytes(8)?);
                }
            }
            13 => {
                let some = r.bool()?;
                v.bool(some);
                if some {
                    v.raw(r.bytes(16)?);
                }
            }
            14 => {
                v.varint(block_state(r.varint()?));
            }
            15 => {
                // 0 = no block
                let s = r.varint()?;
                v.varint(if s == 0 { 0 } else { block_state(s) });
            }
            16 => nbt_to_client(r, &mut v)?,
            17 => particle(r, &mut v)?,
            18 => {
                for _ in 0..3 {
                    varint_copy(r, &mut v)?;
                }
            }
            23 => {
                let some = r.bool()?;
                v.bool(some);
                if some {
                    string_copy(r, &mut v)?;
                    v.raw(r.bytes(8)?);
                }
            }
            27 => {
                v.raw(r.bytes(16)?);
            }
            other => return Err(DecodeError::Other(format!("unknown entity data type {other}"))),
        }
        let mut index = index;
        match kind {
            // position/rotation interpolation duration: new in 1.20.2, later fields move down
            Some(Kind::Display) if index == 10 => continue,
            Some(Kind::Display) if index > 10 => index -= 1,
            Some(Kind::Minecart) if index == 11 && ty == 1 => {
                let mut vr = Reader::new(&v.buf);
                let s = vr.varint()?;
                v = Writer::new();
                v.varint(block_state(s));
            }
            _ => {}
        }
        w.u8(index);
        w.varint(ty);
        w.raw(&v.buf);
    }
}

pub fn set_equipment(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 8);
    varint_copy(&mut r, &mut w)?;
    loop {
        let slot = r.u8()?;
        w.u8(slot);
        item(&mut r, &mut w)?;
        if slot & 0x80 == 0 {
            return Ok(w.into_inner());
        }
    }
}

pub fn container_set_content(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 64);
    w.u8(r.u8()?); // container id
    varint_copy(&mut r, &mut w)?; // state id
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        item(&mut r, &mut w)?;
    }
    item(&mut r, &mut w)?; // carried
    Ok(w.into_inner())
}

pub fn container_set_slot(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 4);
    w.u8(r.u8()?);
    varint_copy(&mut r, &mut w)?;
    w.i16(r.i16()?);
    item(&mut r, &mut w)?;
    Ok(w.into_inner())
}

pub fn merchant_offers(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 32);
    varint_copy(&mut r, &mut w)?; // container id
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        item(&mut r, &mut w)?; // cost a
        item(&mut r, &mut w)?; // result
        item(&mut r, &mut w)?; // cost b
        w.raw(r.bytes(1 + 4 * 4 + 4 + 4)?); // out of stock, uses, max uses, xp, special price, multiplier, demand
    }
    w.raw(r.remaining()); // villager level, xp, show progress, can restock
    Ok(w.into_inner())
}

pub fn level_particles(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 4);
    // the particle id comes first, its data last
    let id = varint_copy(&mut r, &mut w)?;
    w.raw(r.bytes(1 + 3 * 8 + 4 * 4 + 4)?); // long distance, position, offsets, speed, count
    particle_data(id, &mut r, &mut w)?;
    Ok(w.into_inner())
}

pub fn update_advancements(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 256);
    w.bool(r.bool()?); // reset
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        string_copy(&mut r, &mut w)?; // id
        if r.bool()? {
            w.bool(true);
            string_copy(&mut r, &mut w)?; // parent
        } else {
            w.bool(false);
        }
        if r.bool()? {
            w.bool(true);
            string_copy(&mut r, &mut w)?; // title
            string_copy(&mut r, &mut w)?; // description
            item(&mut r, &mut w)?; // icon
            varint_copy(&mut r, &mut w)?; // frame
            let flags = r.i32()?;
            w.i32(flags);
            if flags & 1 != 0 {
                string_copy(&mut r, &mut w)?; // background
            }
            w.raw(r.bytes(8)?); // x, y
        } else {
            w.bool(false);
        }
        // 1.20.2 dropped the criteria list; 1.20.1 needs every criterion the requirements name
        let groups = r.varint()?;
        let mut requirements: Vec<Vec<String>> = Vec::new();
        for _ in 0..groups.max(0) {
            requirements.push(r.list(|r| r.string().map(str::to_string))?);
        }
        let mut criteria: Vec<&str> = Vec::new();
        for c in requirements.iter().flatten() {
            if !criteria.contains(&c.as_str()) {
                criteria.push(c);
            }
        }
        w.varint(criteria.len() as i32);
        for c in &criteria {
            w.string(c);
        }
        w.varint(requirements.len() as i32);
        for g in &requirements {
            w.varint(g.len() as i32);
            for c in g {
                w.string(c);
            }
        }
        w.bool(r.bool()?); // telemetry
    }
    w.raw(r.remaining()); // removed ids, progress: unchanged
    Ok(w.into_inner())
}

fn ingredient(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let n = varint_copy(r, w)?;
    for _ in 0..n.max(0) {
        item(r, w)?;
    }
    Ok(())
}

pub fn update_recipes(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let n = r.varint()?;
    let mut body = Writer::with_capacity(p.len() + 1024);
    let mut kept = 0;
    for _ in 0..n.max(0) {
        let ty = r.string()?;
        let id = r.string()?;
        let mut w = Writer::new();
        w.string(ty).string(id);
        match ty.strip_prefix("minecraft:").unwrap_or(ty) {
            "crafting_shapeless" => {
                string_copy(&mut r, &mut w)?; // group
                varint_copy(&mut r, &mut w)?; // category
                let k = varint_copy(&mut r, &mut w)?;
                for _ in 0..k.max(0) {
                    ingredient(&mut r, &mut w)?;
                }
                item(&mut r, &mut w)?;
            }
            "crafting_shaped" => {
                let width = varint_copy(&mut r, &mut w)?;
                let height = varint_copy(&mut r, &mut w)?;
                string_copy(&mut r, &mut w)?;
                varint_copy(&mut r, &mut w)?;
                for _ in 0..(width.max(0) * height.max(0)) {
                    ingredient(&mut r, &mut w)?;
                }
                item(&mut r, &mut w)?;
                w.bool(r.bool()?); // show notification
            }
            "smelting" | "blasting" | "smoking" | "campfire_cooking" => {
                string_copy(&mut r, &mut w)?;
                varint_copy(&mut r, &mut w)?;
                ingredient(&mut r, &mut w)?;
                item(&mut r, &mut w)?;
                w.raw(r.bytes(4)?); // xp
                varint_copy(&mut r, &mut w)?; // cooking time
            }
            "stonecutting" => {
                string_copy(&mut r, &mut w)?;
                ingredient(&mut r, &mut w)?;
                item(&mut r, &mut w)?;
            }
            "smithing_transform" => {
                for _ in 0..3 {
                    ingredient(&mut r, &mut w)?;
                }
                item(&mut r, &mut w)?;
            }
            "smithing_trim" => {
                for _ in 0..3 {
                    ingredient(&mut r, &mut w)?;
                }
            }
            s if s.starts_with("crafting_special_") || s == "crafting_decorated_pot" => {
                varint_copy(&mut r, &mut w)?; // category
            }
            // a type 1.20.1 can't parse: the rest of the list can't be read either
            _ => break,
        }
        body.raw(&w.buf);
        kept += 1;
    }
    let mut w = Writer::with_capacity(body.buf.len() + 5);
    w.varint(kept);
    w.raw(&body.buf);
    Ok(w.into_inner())
}

/// SET_CREATIVE_MODE_SLOT 1.20.1 → 1.20.2.
pub fn set_creative_slot(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.i16(r.i16()?);
    item_to_server(&mut r, &mut w)?;
    Ok(w.into_inner())
}

/// CONTAINER_CLICK 1.20.1 → 1.20.2.
pub fn container_click(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.u8(r.u8()?); // container id
    varint_copy(&mut r, &mut w)?; // state id
    w.i16(r.i16()?); // slot
    w.i8(r.i8()?); // button
    varint_copy(&mut r, &mut w)?; // mode
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        w.i16(r.i16()?);
        item_to_server(&mut r, &mut w)?;
    }
    item_to_server(&mut r, &mut w)?; // carried
    Ok(w.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(tag: Option<&Tag>) -> Vec<u8> {
        let mut w = Writer::new();
        w.bool(true).varint(5).i8(3);
        nbt::write_optional(&mut w, tag);
        w.into_inner()
    }

    #[test]
    fn item_nbt_gets_a_root_name_and_old_effects() {
        let effect = Tag::Compound(vec![("id".into(), Tag::String("minecraft:speed".into())), ("amplifier".into(), Tag::Byte(1)), ("duration".into(), Tag::Int(200))]);
        let tag = Tag::Compound(vec![("custom_potion_effects".into(), Tag::List(vec![effect]))]);
        let mut w = Writer::new();
        item(&mut Reader::new(&stack(Some(&tag))), &mut w).unwrap();
        let mut r = Reader::new(&w.buf);
        assert!(r.bool().unwrap());
        assert_eq!((r.varint().unwrap(), r.i8().unwrap()), (5, 3));
        assert_eq!(r.u8().unwrap(), 10);
        assert_eq!(r.u16().unwrap(), 0, "empty root name");
        r.set_position(r.position() - 3);
        let back = nbt::read_named_optional(&mut r).unwrap().unwrap();
        let e = &back.get("CustomPotionEffects").unwrap().as_list().unwrap()[0];
        assert_eq!(e.get("Id"), Some(&Tag::Int(1)));
        assert_eq!(e.get("Amplifier"), Some(&Tag::Byte(1)));
        assert_eq!(e.get("Duration"), Some(&Tag::Int(200)));
        assert!(r.is_empty());
        // and back to the server as the 1.20.2 form
        let mut s = Writer::new();
        item_to_server(&mut Reader::new(&w.buf), &mut s).unwrap();
        assert_eq!(s.buf, stack(Some(&tag)));
    }

    #[test]
    fn items_without_nbt_and_empty_slots() {
        let mut w = Writer::new();
        item(&mut Reader::new(&stack(None)), &mut w).unwrap();
        assert_eq!(w.buf, stack(None));
        let mut w = Writer::new();
        item(&mut Reader::new(&[0]), &mut w).unwrap();
        assert_eq!(w.buf, vec![0]);
    }

    #[test]
    fn display_entity_data_loses_the_new_field() {
        let mut p = Writer::new();
        p.u8(9).varint(1).varint(7); // interpolation duration
        p.u8(10).varint(1).varint(3); // teleport duration (1.20.2)
        p.u8(11).varint(26).f32(1.0).f32(2.0).f32(3.0); // translation
        p.u8(0xff);
        let mut w = Writer::new();
        entity_data(Some(Kind::Display), &mut Reader::new(&p.buf), &mut w).unwrap();
        let mut want = Writer::new();
        want.u8(9).varint(1).varint(7);
        want.u8(10).varint(26).f32(1.0).f32(2.0).f32(3.0);
        want.u8(0xff);
        assert_eq!(w.buf, want.buf);
    }

    #[test]
    fn entity_data_nbt_gets_a_root_name() {
        let mut p = Writer::new();
        p.u8(5).varint(16);
        nbt::write(&mut p, &Tag::Compound(vec![("a".into(), Tag::Int(1))]));
        p.u8(0xff);
        let mut w = Writer::new();
        entity_data(None, &mut Reader::new(&p.buf), &mut w).unwrap();
        let mut r = Reader::new(&w.buf);
        assert_eq!((r.u8().unwrap(), r.varint().unwrap()), (5, 16));
        assert_eq!(nbt::read_named_optional(&mut r).unwrap(), Some(Tag::Compound(vec![("a".into(), Tag::Int(1))])));
        assert_eq!(r.u8().unwrap(), 0xff);
    }

    #[test]
    fn advancements_get_criteria_from_requirements() {
        let mut p = Writer::new();
        p.bool(true).varint(1);
        p.string("minecraft:story/root").bool(false).bool(false);
        p.varint(2);
        p.varint(2).string("a").string("b");
        p.varint(1).string("a");
        p.bool(false);
        p.varint(0).varint(0);
        let out = update_advancements(&p.buf).unwrap();
        let mut r = Reader::new(&out);
        r.bool().unwrap();
        r.varint().unwrap();
        r.string().unwrap();
        r.bool().unwrap();
        r.bool().unwrap();
        assert_eq!(r.list(|r| r.string().map(str::to_string)).unwrap(), vec!["a", "b"]);
    }
}

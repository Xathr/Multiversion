//! Items, particles, entity data and the packets that carry them, 1.20 ↔ 1.19.4. The layouts are
//! the same in both versions; what changes are the ids, and armour trims 1.19.4 has no pattern for.

use super::super::data_1_19_4 as d;
use super::super::table::SameName;
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

/// Where an item's 1.20 identity is kept in the client's copy when 1.19.4 can only show a
/// stand-in, so it comes back unchanged from the creative inventory or a container click.
pub const BACKUP_KEY: &str = "multiversion:v1_19_4";
/// Trim patterns added in 1.20. A 1.19.4 client fails to read an item that names one.
const NEW_TRIM_PATTERNS: [&str; 5] = ["host", "raiser", "shaper", "silence", "wayfinder"];

fn plain(key: &str) -> &str {
    key.strip_prefix("minecraft:").unwrap_or(key)
}

/// 1.20 items and blocks that 1.19.4 has under the same name, by id.
pub static SAME_ITEMS: SameName = SameName::new(&d::NEW_ITEM_NAMES, &d::OLD_ITEM_NAMES);
pub static SAME_BLOCKS: SameName = SameName::new(&d::NEW_BLOCK_NAMES, &d::OLD_BLOCK_NAMES);

/// Whether 1.19.4 has this 1.20 item under the same name (otherwise it gets a stand-in).
fn exists_in_old(new_id: i32) -> bool {
    SAME_ITEMS.get(new_id).is_some()
}

/// An item stack, 1.20 → 1.19.4.
pub fn item(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if !present {
        return Ok(());
    }
    let id = r.varint()?;
    w.varint(d::ITEMS.get(id as usize).map_or(0, |v| *v as i32));
    w.i8(r.i8()?);
    let mut tag = nbt::read_named_optional(r)?;
    let mut backup = Vec::new();
    if !exists_in_old(id) {
        backup.push(("id".to_string(), Tag::Int(id)));
    }
    if let Some(t) = tag.as_mut() {
        let new_pattern = t.get("Trim").and_then(|trim| trim.get("pattern")).and_then(Tag::as_str).is_some_and(|p| NEW_TRIM_PATTERNS.contains(&plain(p)));
        if new_pattern && let Some(trim) = t.remove("Trim") {
            backup.push(("Trim".to_string(), trim));
        }
    }
    if !backup.is_empty() {
        tag.get_or_insert_with(Tag::compound).insert(BACKUP_KEY, Tag::Compound(backup));
    }
    match &tag {
        Some(t) => nbt::write_named(w, t),
        None => {
            w.u8(0);
        }
    }
    Ok(())
}

/// An item stack, 1.19.4 → 1.20.
pub fn item_to_server(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if !present {
        return Ok(());
    }
    let id = r.varint()?;
    let count = r.i8()?;
    let mut tag = nbt::read_named_optional(r)?;
    let mut new_id = d::ITEMS_REVERSE.get(id as usize).map_or(0, |v| *v as i32);
    if let Some(t) = tag.as_mut() {
        if let Some(backup) = t.remove(BACKUP_KEY) {
            // Only trust the kept id if it still stands for the item the client holds.
            if let Some(kept) = backup.get("id").and_then(Tag::as_i32)
                && d::ITEMS.get(kept as usize).is_some_and(|mapped| *mapped as i32 == id)
            {
                new_id = kept;
            }
            if let Some(trim) = backup.get("Trim") {
                t.insert("Trim", trim.clone());
            }
        }
        if matches!(t, Tag::Compound(entries) if entries.is_empty()) {
            tag = None;
        }
    }
    w.varint(new_id);
    w.i8(count);
    match &tag {
        Some(t) => nbt::write_named(w, t),
        None => {
            w.u8(0);
        }
    }
    Ok(())
}

pub fn block_state(id: i32) -> i32 {
    d::BLOCK_STATES.get(id as usize).map_or(1, |v| *v as i32)
}

pub fn varint_copy(r: &mut Reader, w: &mut Writer) -> Result<i32> {
    let v = r.varint()?;
    w.varint(v);
    Ok(v)
}

pub fn string_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    w.string(r.string()?);
    Ok(())
}

fn nbt_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    match nbt::read_named_optional(r)? {
        Some(t) => nbt::write_named(w, &t),
        None => {
            w.u8(0);
        }
    }
    Ok(())
}

/// Particle id + data. Block particles carry a state, the item one a stack.
pub fn particle(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let id = r.varint()?;
    w.varint(d::PARTICLES.get(id as usize).map_or(0, |v| *v as i32));
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
            match plain(source) {
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
    /// Minecarts: index 11 is the displayed block state.
    Minecart,
    /// Item displays: 1.20 turned the model to face the other way.
    ItemDisplay,
    FallingBlock,
}

pub fn entity_kind(ty: i32) -> Option<Kind> {
    let name = d::NEW_ENTITY_NAMES.get(ty as usize).copied()?;
    if name == "item_display" {
        Some(Kind::ItemDisplay)
    } else if name == "falling_block" {
        Some(Kind::FallingBlock)
    } else if name.ends_with("minecart") {
        Some(Kind::Minecart)
    } else {
        None
    }
}

/// Index of an item display's left rotation, and the entity data type of a quaternion.
const ITEM_DISPLAY_ROTATION: u8 = 12;
const QUATERNION: i32 = 27;

/// A quaternion turned half a turn around the Y axis: 1.20 flipped which way an item display's
/// model faces, so for 1.19.4 it is turned back.
fn rotate_y180(q: [f32; 4]) -> [f32; 4] {
    let [x, y, z, w] = q;
    [-z, w, x, -y]
}

/// SET_ENTITY_DATA body (after the entity id). `first` says this is the first data sent for the
/// entity, when an item display that sets no rotation still needs the half turn.
pub fn entity_data(kind: Option<Kind>, first: bool, r: &mut Reader, w: &mut Writer) -> Result<()> {
    let mut rotation_seen = false;
    loop {
        let index = r.u8()?;
        if index == 0xff {
            if first && !rotation_seen && kind == Some(Kind::ItemDisplay) {
                w.u8(ITEM_DISPLAY_ROTATION);
                w.varint(QUATERNION);
                for v in rotate_y180([0.0, 0.0, 0.0, 1.0]) {
                    w.f32(v);
                }
            }
            w.u8(0xff);
            return Ok(());
        }
        let ty = r.varint()?;
        w.u8(index);
        w.varint(ty);
        match ty {
            0 | 8 => {
                w.raw(r.bytes(1)?);
            }
            1 if index == 11 && kind == Some(Kind::Minecart) => {
                w.varint(block_state(r.varint()?));
            }
            1 | 12 | 19..=22 | 24 | 25 => {
                varint_copy(r, w)?;
            }
            2 => {
                w.varlong(r.varlong()?);
            }
            3 => {
                w.raw(r.bytes(4)?);
            }
            4 | 5 => string_copy(r, w)?,
            6 => {
                if r.bool()? {
                    w.bool(true);
                    string_copy(r, w)?;
                } else {
                    w.bool(false);
                }
            }
            7 => item(r, w)?,
            9 | 26 => {
                w.raw(r.bytes(12)?);
            }
            10 => {
                w.raw(r.bytes(8)?);
            }
            11 => {
                let some = r.bool()?;
                w.bool(some);
                if some {
                    w.raw(r.bytes(8)?);
                }
            }
            13 => {
                let some = r.bool()?;
                w.bool(some);
                if some {
                    w.raw(r.bytes(16)?);
                }
            }
            14 => {
                w.varint(block_state(r.varint()?));
            }
            15 => {
                // 0 = no block
                let s = r.varint()?;
                w.varint(if s == 0 { 0 } else { block_state(s) });
            }
            16 => nbt_copy(r, w)?,
            17 => particle(r, w)?,
            18 => {
                for _ in 0..3 {
                    varint_copy(r, w)?;
                }
            }
            23 => {
                let some = r.bool()?;
                w.bool(some);
                if some {
                    string_copy(r, w)?;
                    w.raw(r.bytes(8)?);
                }
            }
            QUATERNION => {
                let mut q = [0f32; 4];
                for v in &mut q {
                    *v = r.f32()?;
                }
                if index == ITEM_DISPLAY_ROTATION && kind == Some(Kind::ItemDisplay) {
                    rotation_seen = true;
                    q = rotate_y180(q);
                }
                for v in q {
                    w.f32(v);
                }
            }
            other => return Err(DecodeError::Other(format!("unknown entity data type {other}"))),
        }
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

pub fn cooldown(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::new();
    let id = r.varint()?;
    w.varint(d::ITEMS.get(id as usize).map_or(0, |v| *v as i32));
    w.raw(r.remaining());
    Ok(w.into_inner())
}

pub fn level_particles(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 4);
    // the particle id comes first, its data last
    let id = r.varint()?;
    w.varint(d::PARTICLES.get(id as usize).map_or(0, |v| *v as i32));
    w.raw(r.bytes(1 + 3 * 8 + 4 * 4 + 4)?); // long distance, position, offsets, speed, count
    particle_data(id, &mut r, &mut w)?;
    Ok(w.into_inner())
}

pub fn update_advancements(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
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
        let criteria = varint_copy(&mut r, &mut w)?;
        for _ in 0..criteria.max(0) {
            string_copy(&mut r, &mut w)?;
        }
        let groups = varint_copy(&mut r, &mut w)?;
        for _ in 0..groups.max(0) {
            let k = varint_copy(&mut r, &mut w)?;
            for _ in 0..k.max(0) {
                string_copy(&mut r, &mut w)?;
            }
        }
        r.bool()?; // sends telemetry: new in 1.20
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
    let mut body = Writer::with_capacity(p.len());
    let mut kept = 0;
    for _ in 0..n.max(0) {
        let ty = r.string()?;
        let id = r.string()?;
        let mut w = Writer::new();
        w.string(ty).string(id);
        match plain(ty) {
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
                for _ in 0..width.max(0).saturating_mul(height.max(0)) {
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
            // a type this step can't read: the rest of the list can't be read either
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

/// SET_CREATIVE_MODE_SLOT 1.19.4 → 1.20.
pub fn set_creative_slot(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.i16(r.i16()?);
    item_to_server(&mut r, &mut w)?;
    Ok(w.into_inner())
}

/// CONTAINER_CLICK 1.19.4 → 1.20.
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

    fn id(names: &[&str], name: &str) -> i32 {
        names.iter().position(|n| *n == name).unwrap() as i32
    }

    fn stack(item_id: i32, tag: Option<&Tag>) -> Vec<u8> {
        let mut w = Writer::new();
        w.bool(true).varint(item_id).i8(3);
        match tag {
            Some(t) => nbt::write_named(&mut w, t),
            None => {
                w.u8(0);
            }
        }
        w.into_inner()
    }

    fn convert(f: fn(&mut Reader, &mut Writer) -> Result<()>, bytes: &[u8]) -> Vec<u8> {
        let mut w = Writer::new();
        let mut r = Reader::new(bytes);
        f(&mut r, &mut w).unwrap();
        assert!(r.is_empty());
        w.into_inner()
    }

    #[test]
    fn shared_items_only_change_id() {
        let new = id(&d::NEW_ITEM_NAMES, "diamond_sword");
        let old = id(&d::OLD_ITEM_NAMES, "diamond_sword");
        let tag = Tag::Compound(vec![("Damage".into(), Tag::Int(5))]);
        assert_eq!(convert(item, &stack(new, Some(&tag))), stack(old, Some(&tag)));
        assert_eq!(convert(item_to_server, &stack(old, Some(&tag))), stack(new, Some(&tag)));
        assert_eq!(convert(item, &[0]), [0], "an empty slot stays empty");
    }

    #[test]
    fn new_items_come_back_as_themselves() {
        let new = id(&d::NEW_ITEM_NAMES, "pitcher_plant");
        let shown = convert(item, &stack(new, None));
        let mut r = Reader::new(&shown);
        assert!(r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), id(&d::OLD_ITEM_NAMES, "lilac"), "shown as its stand-in");
        r.i8().unwrap();
        let tag = nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert_eq!(tag.get(BACKUP_KEY).and_then(|b| b.get("id")).and_then(Tag::as_i32), Some(new));
        assert_eq!(convert(item_to_server, &shown), stack(new, None), "and restored exactly");
        // A real lilac from the client stays a lilac.
        let lilac_old = id(&d::OLD_ITEM_NAMES, "lilac");
        assert_eq!(convert(item_to_server, &stack(lilac_old, None)), stack(id(&d::NEW_ITEM_NAMES, "lilac"), None));
    }

    #[test]
    fn a_forged_backup_id_is_ignored() {
        // The client claims its dirt is really a netherite block.
        let dirt_old = id(&d::OLD_ITEM_NAMES, "dirt");
        let forged = Tag::Compound(vec![(BACKUP_KEY.into(), Tag::Compound(vec![("id".into(), Tag::Int(id(&d::NEW_ITEM_NAMES, "netherite_block")))]))]);
        assert_eq!(convert(item_to_server, &stack(dirt_old, Some(&forged))), stack(id(&d::NEW_ITEM_NAMES, "dirt"), None));
    }

    #[test]
    fn new_trim_patterns_are_hidden_and_restored() {
        let chest = id(&d::NEW_ITEM_NAMES, "iron_chestplate");
        let trim = |pattern: &str| Tag::Compound(vec![("Trim".into(), Tag::Compound(vec![("material".into(), Tag::String("minecraft:gold".into())), ("pattern".into(), Tag::String(pattern.into()))]))]);
        let shown = convert(item, &stack(chest, Some(&trim("minecraft:silence"))));
        let mut r = Reader::new(&shown);
        r.bool().unwrap();
        r.varint().unwrap();
        r.i8().unwrap();
        let tag = nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert!(tag.get("Trim").is_none(), "1.19.4 cannot read this pattern");
        assert_eq!(convert(item_to_server, &shown), stack(chest, Some(&trim("minecraft:silence"))));
        // A pattern 1.19.4 knows is left alone.
        let old_chest = id(&d::OLD_ITEM_NAMES, "iron_chestplate");
        assert_eq!(convert(item, &stack(chest, Some(&trim("minecraft:coast")))), stack(old_chest, Some(&trim("minecraft:coast"))));
    }

    #[test]
    fn item_display_is_turned_around() {
        // first data without a rotation: one is added
        let mut w = Writer::new();
        let mut r = Reader::new(&[0xff]);
        entity_data(Some(Kind::ItemDisplay), true, &mut r, &mut w).unwrap();
        let mut out = Reader::new(&w.buf);
        assert_eq!((out.u8().unwrap(), out.varint().unwrap()), (ITEM_DISPLAY_ROTATION, QUATERNION));
        assert_eq!([out.f32().unwrap(), out.f32().unwrap(), out.f32().unwrap(), out.f32().unwrap()], [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(out.u8().unwrap(), 0xff);
        // later data without a rotation: nothing added
        let mut w = Writer::new();
        entity_data(Some(Kind::ItemDisplay), false, &mut Reader::new(&[0xff]), &mut w).unwrap();
        assert_eq!(w.buf, [0xff]);
        // other entities are untouched
        let mut w = Writer::new();
        entity_data(None, true, &mut Reader::new(&[0xff]), &mut w).unwrap();
        assert_eq!(w.buf, [0xff]);
    }

    #[test]
    fn advancements_lose_the_telemetry_flag() {
        let mut p = Writer::new();
        p.bool(false).varint(1);
        p.string("a:b").bool(false).bool(false);
        p.varint(1).string("c").varint(1).varint(1).string("c");
        p.bool(true); // telemetry
        p.varint(0).varint(0);
        let out = update_advancements(&p.buf).unwrap();
        let mut want = Writer::new();
        want.bool(false).varint(1);
        want.string("a:b").bool(false).bool(false);
        want.varint(1).string("c").varint(1).varint(1).string("c");
        want.varint(0).varint(0);
        assert_eq!(out, want.buf);
    }
}

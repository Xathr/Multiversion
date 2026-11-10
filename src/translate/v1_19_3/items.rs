//! Items, particles, entity data and the packets that carry them, 1.19.4 ↔ 1.19.3.
//!
//! Item stacks have the same layout in both versions (present, id, count, named NBT); only the
//! ids differ. Items 1.19.3 doesn't have (the cherry and bamboo sets, smithing templates, ...)
//! become the substitutes the id table names. Entity data types were renumbered when 1.19.4
//! added the optional block state, and the display, interaction and sniffer entities don't
//! exist in 1.19.3: they are shown as armor stands and a ravager.

use crate::translate::data_1_19_3 as d;
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt;

/// 1.19.4 item id → 1.19.3. Ids outside the registry become air.
pub fn item_id(id: i32) -> i32 {
    usize::try_from(id).ok().and_then(|i| d::ITEMS.get(i)).map_or(0, |v| *v as i32)
}

/// 1.19.3 item id → 1.19.4.
pub fn item_id_to_server(id: i32) -> i32 {
    usize::try_from(id).ok().and_then(|i| d::ITEMS_REVERSE.get(i)).map_or(0, |v| *v as i32)
}

/// 1.19.4 block state → 1.19.3. Ids outside the registry become stone.
pub fn block_state(id: i32) -> i32 {
    usize::try_from(id).ok().and_then(|i| d::BLOCK_STATES.get(i)).map_or(1, |v| *v as i32)
}

/// 1.19.4 block → 1.19.3, `None` for the blocks 1.19.3 has nothing like (the table marks
/// those with stone).
pub fn block(id: i32) -> Option<i32> {
    let mapped = *usize::try_from(id).ok().and_then(|i| d::BLOCKS.get(i))? as i32;
    if mapped == 1 && id != 1 { None } else { Some(mapped) }
}

pub fn entity_type(id: i32) -> i32 {
    usize::try_from(id).ok().and_then(|i| d::ENTITIES.get(i)).map_or(id, |v| *v as i32)
}

/// Copies named optional NBT as it is: the bytes are the same in both versions, they are only
/// read to find where the value ends.
pub fn nbt_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    r.copy_to(w, |r| nbt::read_named_optional(r).map(|_| ()))
}

fn stack(r: &mut Reader, w: &mut Writer, map: fn(i32) -> i32) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if !present {
        return Ok(());
    }
    w.varint(map(r.varint()?));
    w.i8(r.i8()?);
    nbt_copy(r, w)
}

/// An item stack, 1.19.4 → 1.19.3.
pub fn item(r: &mut Reader, w: &mut Writer) -> Result<()> {
    stack(r, w, item_id)
}

/// An item stack, 1.19.3 → 1.19.4.
pub fn item_to_server(r: &mut Reader, w: &mut Writer) -> Result<()> {
    stack(r, w, item_id_to_server)
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

/// Particle id + data. The cherry leaf particles are new; block particles carry a state, the
/// item one a stack.
pub fn particle(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let id = r.varint()?;
    w.varint(particle_id(id));
    particle_data(id, r, w)
}

fn particle_id(id: i32) -> i32 {
    usize::try_from(id).ok().and_then(|i| d::PARTICLES.get(i)).map_or(id, |v| *v as i32)
}

/// The data that follows a particle, by its 1.19.4 id.
fn particle_data(id: i32, r: &mut Reader, w: &mut Writer) -> Result<()> {
    let name = usize::try_from(id).ok().and_then(|i| d::NEW_PARTICLE_NAMES.get(i)).copied().unwrap_or("");
    match name {
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
    /// Block and item displays, shown as invisible armor stands: none of their own fields fit.
    Display,
    /// Text displays: the text becomes the armor stand's name.
    TextDisplay,
    /// Interaction entities, also armor stands.
    Interaction,
    /// Sniffers, shown as ravagers.
    Sniffer,
    /// Horses and their relatives: 1.19.3 still has the owner at index 18.
    Horse,
    /// Minecarts: index 11 is the displayed block state.
    Minecart,
    /// Boats: index 11 is the wood type, and cherry was added in the middle of the list.
    Boat,
}

pub fn entity_kind(ty: i32) -> Option<Kind> {
    let name = usize::try_from(ty).ok().and_then(|i| d::NEW_ENTITY_NAMES.get(i)).copied()?;
    Some(match name {
        "block_display" | "item_display" => Kind::Display,
        "text_display" => Kind::TextDisplay,
        "interaction" => Kind::Interaction,
        "sniffer" => Kind::Sniffer,
        "horse" | "donkey" | "mule" | "llama" | "trader_llama" | "skeleton_horse" | "zombie_horse" | "camel" => Kind::Horse,
        "boat" | "chest_boat" => Kind::Boat,
        n if n.ends_with("minecart") => Kind::Minecart,
        _ => return None,
    })
}

pub fn is_falling_block(ty: i32) -> bool {
    usize::try_from(ty).ok().and_then(|i| d::NEW_ENTITY_NAMES.get(i)).copied() == Some("falling_block")
}

/// Entity data types as 1.19.3 numbers them (the ones this step writes itself).
mod old_type {
    pub const BYTE: i32 = 0;
    pub const OPTIONAL_COMPONENT: i32 = 6;
    pub const BOOLEAN: i32 = 8;
}

/// Entity data sent right after the spawn of an entity that stands in for one 1.19.3 doesn't
/// have (SET_ENTITY_DATA body after the entity id), or `None` when the spawn needs nothing.
pub fn spawn_data(kind: Option<Kind>) -> Option<Vec<u8>> {
    let mut w = Writer::new();
    match kind? {
        Kind::Display | Kind::TextDisplay | Kind::Interaction => {
            // an armor stand nobody can see or bump into: invisible, no gravity, small marker
            w.u8(0).varint(old_type::BYTE).u8(0x20);
            w.u8(5).varint(old_type::BOOLEAN).bool(true);
            w.u8(15).varint(old_type::BYTE).u8(0x01 | 0x10);
        }
        Kind::Sniffer => {
            // the ravager says what it really is when looked at
            w.u8(2).varint(old_type::OPTIONAL_COMPONENT).bool(true).string("{\"text\":\"Sniffer\"}");
        }
        Kind::Horse | Kind::Minecart | Kind::Boat => return None,
    }
    w.u8(0xff);
    Some(w.into_inner())
}

/// SET_ENTITY_DATA body (after the entity id).
pub fn entity_data(kind: Option<Kind>, r: &mut Reader, w: &mut Writer) -> Result<()> {
    loop {
        let mut index = r.u8()?;
        if index == 0xff {
            w.u8(0xff);
            return Ok(());
        }
        let ty = r.varint()?;
        // 1.19.4 put the optional block state at 15: it becomes the plain block state (14), and
        // later types are one lower in 1.19.3
        let old_ty = if ty >= 15 { ty - 1 } else { ty };
        let mut v = Writer::new();
        let mut int = None;
        let mut text = None;
        match ty {
            0 | 8 => {
                v.raw(r.bytes(1)?);
            }
            1 => int = Some(varint_copy(r, &mut v)?),
            12 | 19..=22 | 24 => {
                varint_copy(r, &mut v)?;
            }
            2 => {
                v.varlong(r.varlong()?);
            }
            3 => {
                v.raw(r.bytes(4)?);
            }
            4 => string_copy(r, &mut v)?,
            5 => {
                let s = r.string()?;
                v.string(s);
                text = Some(s);
            }
            6 => {
                if r.bool()? {
                    v.bool(true);
                    string_copy(r, &mut v)?;
                } else {
                    v.bool(false);
                }
            }
            7 => item(r, &mut v)?,
            9 => {
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
                // 0 = no block, which the plain block state shows as air
                let s = r.varint()?;
                v.varint(if s == 0 { 0 } else { block_state(s) });
            }
            16 => nbt_copy(r, &mut v)?,
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
            // sniffer state, vector and quaternion: 1.19.3 has no such types, and they are only
            // used by fields it doesn't have either
            25 => {
                r.varint()?;
                continue;
            }
            26 => {
                r.bytes(12)?;
                continue;
            }
            27 => {
                r.bytes(16)?;
                continue;
            }
            other => return Err(DecodeError::Other(format!("unknown entity data type {other}"))),
        }
        match kind {
            Some(Kind::TextDisplay) if index == 22 => {
                // the text, shown as the armor stand's always visible name
                let Some(text) = text else { continue };
                w.u8(2).varint(old_type::OPTIONAL_COMPONENT).bool(true).string(text);
                w.u8(3).varint(old_type::BOOLEAN).bool(true);
                continue;
            }
            // past the fields every entity has, displays share nothing with an armor stand
            Some(Kind::Display | Kind::TextDisplay) if index > 7 => continue,
            // width, height, response
            Some(Kind::Interaction) if (8..=10).contains(&index) => continue,
            // state, drop seed at tick
            Some(Kind::Sniffer) if index == 17 || index == 18 => continue,
            // 1.19.4 dropped the owner uuid at 18, later fields moved down
            Some(Kind::Horse) if index >= 18 => {
                if index == 0xfe {
                    continue; // one further would be the end marker
                }
                index += 1;
            }
            Some(Kind::Minecart) if index == 11 => {
                if let Some(state) = int {
                    v = Writer::new();
                    v.varint(block_state(state));
                }
            }
            Some(Kind::Boat) if index == 11 => {
                if let Some(wood) = int.filter(|w| *w > 4) {
                    v = Writer::new();
                    v.varint(wood - 1); // cherry becomes acacia, the rest keep their wood
                }
            }
            _ => {}
        }
        w.u8(index);
        w.varint(old_ty);
        w.raw(&v.buf);
    }
}

pub fn set_equipment(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
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
    let mut w = Writer::with_capacity(p.len());
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
    let mut w = Writer::with_capacity(p.len());
    w.u8(r.u8()?);
    varint_copy(&mut r, &mut w)?;
    w.i16(r.i16()?);
    item(&mut r, &mut w)?;
    Ok(w.into_inner())
}

pub fn cooldown(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.varint(item_id(r.varint()?));
    w.raw(r.remaining()); // ticks
    Ok(w.into_inner())
}

pub fn merchant_offers(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
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
    let mut w = Writer::with_capacity(p.len());
    // the particle id comes first, its data last
    let id = r.varint()?;
    w.varint(particle_id(id));
    w.raw(r.bytes(1 + 3 * 8 + 4 * 4 + 4)?); // long distance, position, offsets, speed, count
    particle_data(id, &mut r, &mut w)?;
    Ok(w.into_inner())
}

/// Only the icons change: both versions list criteria and requirements the same way.
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

/// UPDATE_RECIPES: 1.19.3 knows neither the new smithing recipes nor the decorated pot, and its
/// shaped recipes have no "show notification" flag.
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
                for _ in 0..i64::from(width.max(0)) * i64::from(height.max(0)) {
                    ingredient(&mut r, &mut w)?;
                }
                item(&mut r, &mut w)?;
                r.bool()?; // show notification
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
            "smithing" => {
                ingredient(&mut r, &mut w)?;
                ingredient(&mut r, &mut w)?;
                item(&mut r, &mut w)?;
            }
            // read past, not sent: the scratch writer is thrown away
            kind @ ("smithing_transform" | "smithing_trim") => {
                for _ in 0..3 {
                    ingredient(&mut r, &mut w)?;
                }
                if kind == "smithing_transform" {
                    item(&mut r, &mut w)?;
                }
                continue;
            }
            "crafting_decorated_pot" => {
                r.varint()?; // category
                continue;
            }
            s if s.starts_with("crafting_special_") => {
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

/// SET_CREATIVE_MODE_SLOT 1.19.3 → 1.19.4.
pub fn set_creative_slot(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.i16(r.i16()?);
    item_to_server(&mut r, &mut w)?;
    Ok(w.into_inner())
}

/// CONTAINER_CLICK 1.19.3 → 1.19.4.
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
    use crate::proto::nbt::Tag;

    fn index(names: &[&str], name: &str) -> i32 {
        names.iter().position(|n| *n == name).unwrap_or_else(|| panic!("{name}")) as i32
    }
    fn new_item(name: &str) -> i32 {
        index(&d::NEW_ITEM_NAMES, name)
    }
    fn old_item(name: &str) -> i32 {
        index(&d::OLD_ITEM_NAMES, name)
    }

    fn stack(id: i32, tag: Option<&Tag>) -> Vec<u8> {
        let mut w = Writer::new();
        w.bool(true).varint(id).i8(3);
        match tag {
            Some(t) => nbt::write_named(&mut w, t),
            None => {
                w.u8(0);
            }
        }
        w.into_inner()
    }

    #[test]
    fn items_change_id_and_keep_their_nbt() {
        let tag = Tag::Compound(vec![("Damage".into(), Tag::Int(7))]);
        let mut w = Writer::new();
        item(&mut Reader::new(&stack(new_item("diamond_sword"), Some(&tag))), &mut w).unwrap();
        assert_eq!(w.buf, stack(old_item("diamond_sword"), Some(&tag)));
        assert_ne!(new_item("diamond_sword"), old_item("diamond_sword"), "the test item does move");
        // and back
        let mut s = Writer::new();
        item_to_server(&mut Reader::new(&w.buf), &mut s).unwrap();
        assert_eq!(s.buf, stack(new_item("diamond_sword"), Some(&tag)));
    }

    #[test]
    fn items_without_nbt_and_empty_slots() {
        let mut w = Writer::new();
        item(&mut Reader::new(&stack(1, None)), &mut w).unwrap();
        assert_eq!(w.buf, stack(1, None));
        let mut w = Writer::new();
        item(&mut Reader::new(&[0]), &mut w).unwrap();
        assert_eq!(w.buf, vec![0]);
    }

    #[test]
    fn new_items_become_something_1_19_3_has() {
        let cherry = new_item("cherry_planks");
        let mapped = item_id(cherry);
        assert!(mapped > 0 && (mapped as usize) < d::OLD_ITEM_NAMES.len());
        assert!(d::OLD_ITEM_NAMES[mapped as usize].ends_with("_planks"));
        assert_eq!(item_id(-1), 0);
        assert_eq!(item_id(i32::MAX), 0);
        assert_eq!(item_id_to_server(i32::MAX), 0);
    }

    #[test]
    fn every_old_item_round_trips() {
        for (old, name) in d::OLD_ITEM_NAMES.iter().enumerate() {
            let new = item_id_to_server(old as i32);
            assert_eq!(d::NEW_ITEM_NAMES[new as usize], *name);
            assert_eq!(item_id(new), old as i32, "{name}");
        }
    }

    #[test]
    fn truncated_items_are_errors() {
        let full = stack(5, Some(&Tag::Compound(vec![("a".into(), Tag::Int(1))])));
        for cut in 1..full.len() {
            let mut w = Writer::new();
            assert!(item(&mut Reader::new(&full[..cut]), &mut w).is_err(), "cut at {cut}");
            let mut w = Writer::new();
            assert!(item_to_server(&mut Reader::new(&full[..cut]), &mut w).is_err(), "cut at {cut}");
        }
    }

    #[test]
    fn entity_data_types_are_renumbered() {
        let stone = d::NEW_BLOCK_STATE_COUNT as i32 - 1; // a state that moves
        let mut p = Writer::new();
        p.u8(1).varint(1).varint(300); // int: same number
        p.u8(6).varint(20).varint(2); // pose: 20 → 19
        p.u8(8).varint(15).varint(stone); // optional block state → block state
        p.u8(9).varint(15).varint(0); // none → air
        p.u8(10).varint(24).varint(4); // painting variant: 24 → 23
        p.u8(11).varint(16); // compound tag: 16 → 15
        nbt::write_named(&mut p, &Tag::Compound(vec![("a".into(), Tag::Int(1))]));
        p.u8(12).varint(7).raw(&stack(new_item("diamond_sword"), None));
        p.u8(0xff);
        let mut w = Writer::new();
        entity_data(None, &mut Reader::new(&p.buf), &mut w).unwrap();
        let mut want = Writer::new();
        want.u8(1).varint(1).varint(300);
        want.u8(6).varint(19).varint(2);
        want.u8(8).varint(14).varint(block_state(stone));
        want.u8(9).varint(14).varint(0);
        want.u8(10).varint(23).varint(4);
        want.u8(11).varint(15);
        nbt::write_named(&mut want, &Tag::Compound(vec![("a".into(), Tag::Int(1))]));
        want.u8(12).varint(7).raw(&stack(old_item("diamond_sword"), None));
        want.u8(0xff);
        assert_eq!(w.buf, want.buf);
    }

    #[test]
    fn new_entity_data_types_are_dropped() {
        let mut p = Writer::new();
        p.u8(17).varint(25).varint(3); // sniffer state
        p.u8(10).varint(26).f32(1.0).f32(2.0).f32(3.0);
        p.u8(12).varint(27).f32(0.0).f32(0.0).f32(0.0).f32(1.0);
        p.u8(0).varint(0).u8(0x40);
        p.u8(0xff);
        let mut w = Writer::new();
        entity_data(None, &mut Reader::new(&p.buf), &mut w).unwrap();
        assert_eq!(w.buf, vec![0, 0, 0x40, 0xff]);
        let mut w = Writer::new();
        assert!(entity_data(None, &mut Reader::new(&[3, 28, 0, 0xff]), &mut w).is_err(), "unknown type");
        let mut w = Writer::new();
        assert!(entity_data(None, &mut Reader::new(&[3, 1, 0]), &mut w).is_err(), "no terminator");
    }

    #[test]
    fn text_display_text_becomes_a_visible_name() {
        let mut p = Writer::new();
        p.u8(0).varint(0).u8(0x40); // glowing: kept
        p.u8(11).varint(27).f32(0.0).f32(0.0).f32(0.0).f32(1.0); // rotation: gone
        p.u8(22).varint(5).string("{\"text\":\"hi\"}");
        p.u8(23).varint(1).varint(200); // line width: gone
        p.u8(0xff);
        let mut w = Writer::new();
        entity_data(Some(Kind::TextDisplay), &mut Reader::new(&p.buf), &mut w).unwrap();
        let mut want = Writer::new();
        want.u8(0).varint(0).u8(0x40);
        want.u8(2).varint(6).bool(true).string("{\"text\":\"hi\"}");
        want.u8(3).varint(8).bool(true);
        want.u8(0xff);
        assert_eq!(w.buf, want.buf);
        // an item display's index 22 is just another display field
        let mut w = Writer::new();
        entity_data(Some(Kind::Display), &mut Reader::new(&p.buf), &mut w).unwrap();
        assert_eq!(w.buf, vec![0, 0, 0x40, 0xff]);
    }

    #[test]
    fn interaction_and_sniffer_lose_their_own_fields() {
        let mut p = Writer::new();
        p.u8(8).varint(3).f32(1.0).u8(9).varint(3).f32(1.0).u8(10).varint(8).bool(true).u8(1).varint(1).varint(5).u8(0xff);
        let mut w = Writer::new();
        entity_data(Some(Kind::Interaction), &mut Reader::new(&p.buf), &mut w).unwrap();
        assert_eq!(w.buf, vec![1, 1, 5, 0xff]);
        let mut p = Writer::new();
        p.u8(16).varint(8).bool(true).u8(17).varint(25).varint(1).u8(18).varint(1).varint(40).u8(0xff);
        let mut w = Writer::new();
        entity_data(Some(Kind::Sniffer), &mut Reader::new(&p.buf), &mut w).unwrap();
        assert_eq!(w.buf, vec![16, 8, 1, 0xff]);
    }

    #[test]
    fn horse_fields_move_past_the_owner() {
        let mut p = Writer::new();
        p.u8(17).varint(0).u8(4).u8(18).varint(1).varint(3).u8(19).varint(8).bool(true).u8(0xff);
        let mut w = Writer::new();
        entity_data(Some(Kind::Horse), &mut Reader::new(&p.buf), &mut w).unwrap();
        assert_eq!(w.buf, vec![17, 0, 4, 19, 1, 3, 20, 8, 1, 0xff]);
    }

    #[test]
    fn minecart_block_and_boat_wood() {
        let state = d::NEW_BLOCK_STATE_COUNT as i32 - 1;
        let mut p = Writer::new();
        p.u8(11).varint(1).varint(state).u8(0xff);
        let mut w = Writer::new();
        entity_data(Some(Kind::Minecart), &mut Reader::new(&p.buf), &mut w).unwrap();
        let mut want = Writer::new();
        want.u8(11).varint(1).varint(block_state(state)).u8(0xff);
        assert_eq!(w.buf, want.buf);
        assert_ne!(block_state(state), state);
        // oak .. acacia stay, cherry (5) becomes acacia, dark oak (6) is 5 in 1.19.3
        for (new, old) in [(0, 0), (4, 4), (5, 4), (6, 5), (8, 7)] {
            let mut p = Writer::new();
            p.u8(11).varint(1).varint(new).u8(0xff);
            let mut w = Writer::new();
            entity_data(Some(Kind::Boat), &mut Reader::new(&p.buf), &mut w).unwrap();
            assert_eq!(w.buf, vec![11, 1, old as u8, 0xff]);
        }
    }

    #[test]
    fn entity_kinds_by_type() {
        let ty = |n: &str| index(&d::NEW_ENTITY_NAMES, n);
        assert_eq!(entity_kind(ty("text_display")), Some(Kind::TextDisplay));
        assert_eq!(entity_kind(ty("block_display")), Some(Kind::Display));
        assert_eq!(entity_kind(ty("item_display")), Some(Kind::Display));
        assert_eq!(entity_kind(ty("interaction")), Some(Kind::Interaction));
        assert_eq!(entity_kind(ty("sniffer")), Some(Kind::Sniffer));
        assert_eq!(entity_kind(ty("camel")), Some(Kind::Horse));
        assert_eq!(entity_kind(ty("tnt_minecart")), Some(Kind::Minecart));
        assert_eq!(entity_kind(ty("minecart")), Some(Kind::Minecart));
        assert_eq!(entity_kind(ty("chest_boat")), Some(Kind::Boat));
        assert_eq!(entity_kind(ty("pig")), None);
        assert_eq!(entity_kind(-1), None);
        assert_eq!(entity_kind(9999), None);
        let old = |n: &str| index(&d::OLD_ENTITY_NAMES, n);
        for n in ["text_display", "block_display", "item_display", "interaction"] {
            assert_eq!(entity_type(ty(n)), old("armor_stand"), "{n}");
        }
        assert_eq!(entity_type(ty("sniffer")), old("ravager"));
        assert_eq!(entity_type(ty("pig")), old("pig"));
        assert!(is_falling_block(ty("falling_block")) && !is_falling_block(ty("pig")));
    }

    #[test]
    fn particles_change_id_and_keep_their_data() {
        let id = |n: &str| index(&d::NEW_PARTICLE_NAMES, n);
        let old = |n: &str| index(&d::OLD_PARTICLE_NAMES, n);
        let mut p = Writer::new();
        p.varint(id("item")).raw(&stack(new_item("diamond_sword"), None));
        let mut w = Writer::new();
        particle(&mut Reader::new(&p.buf), &mut w).unwrap();
        let mut want = Writer::new();
        want.varint(old("item")).raw(&stack(old_item("diamond_sword"), None));
        assert_eq!(w.buf, want.buf);

        let mut p = Writer::new();
        p.varint(id("shriek")).bool(true).f64(1.0).f64(2.0).f64(3.0).f32(0.0).f32(0.0).f32(0.0).f32(1.0).i32(4).varint(20);
        let out = level_particles(&p.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.varint().unwrap(), old("shriek"));
        assert_eq!(r.remaining(), &p.buf[1..]);

        let mut p = Writer::new();
        p.varint(id("vibration")).string("minecraft:entity").varint(4).f32(1.5).varint(10);
        let mut w = Writer::new();
        particle(&mut Reader::new(&p.buf), &mut w).unwrap();
        assert_eq!(w.buf[0] as i32, old("vibration"));
        assert_eq!(&w.buf[1..], &p.buf[1..]);

        // the cherry leaves have a stand-in without data
        let mut w = Writer::new();
        particle(&mut Reader::new(&[id("falling_cherry_leaves") as u8]), &mut w).unwrap();
        assert!((w.buf[0] as usize) < d::OLD_PARTICLE_NAMES.len());
    }

    #[test]
    fn item_packets() {
        let new = new_item("diamond_sword");
        let old = old_item("diamond_sword");
        let mut p = Writer::new();
        p.varint(9).u8(0x80).raw(&stack(new, None)).u8(5).raw(&stack(new, None));
        let mut want = Writer::new();
        want.varint(9).u8(0x80).raw(&stack(old, None)).u8(5).raw(&stack(old, None));
        assert_eq!(set_equipment(&p.buf).unwrap(), want.buf);

        let mut p = Writer::new();
        p.u8(1).varint(7).varint(2).raw(&stack(new, None)).u8(0).raw(&stack(new, None));
        let mut want = Writer::new();
        want.u8(1).varint(7).varint(2).raw(&stack(old, None)).u8(0).raw(&stack(old, None));
        assert_eq!(container_set_content(&p.buf).unwrap(), want.buf);
        assert!(container_set_content(&p.buf[..p.buf.len() - 1]).is_err());

        let mut p = Writer::new();
        p.u8(1).varint(7).i16(36).raw(&stack(new, None));
        let mut want = Writer::new();
        want.u8(1).varint(7).i16(36).raw(&stack(old, None));
        assert_eq!(container_set_slot(&p.buf).unwrap(), want.buf);

        let mut p = Writer::new();
        p.varint(new).varint(20);
        let mut want = Writer::new();
        want.varint(old).varint(20);
        assert_eq!(cooldown(&p.buf).unwrap(), want.buf);
    }

    #[test]
    fn merchant_offers_map_all_three_stacks() {
        let new = new_item("diamond_sword");
        let old = old_item("diamond_sword");
        let build = |id: i32| {
            let mut p = Writer::new();
            p.varint(3).varint(1);
            p.raw(&stack(id, None)).raw(&stack(id, None)).raw(&stack(id, None));
            p.bool(false).i32(1).i32(12).i32(2).i32(0).f32(0.05).i32(0);
            p.varint(2).varint(10).bool(true).bool(false);
            p.into_inner()
        };
        assert_eq!(merchant_offers(&build(new)).unwrap(), build(old));
    }

    #[test]
    fn advancement_icons_are_mapped() {
        let build = |id: i32| {
            let mut p = Writer::new();
            p.bool(true).varint(1);
            p.string("minecraft:story/root").bool(false).bool(true);
            p.string("{\"text\":\"t\"}").string("{\"text\":\"d\"}").raw(&stack(id, None)).varint(0).i32(1).string("bg").f32(0.0).f32(1.0);
            p.varint(1).string("a");
            p.varint(1).varint(1).string("a");
            p.varint(0).varint(0);
            p.into_inner()
        };
        assert_eq!(update_advancements(&build(new_item("diamond_sword"))).unwrap(), build(old_item("diamond_sword")));
    }

    #[test]
    fn recipes_lose_what_1_19_3_cannot_read() {
        let new = new_item("diamond_sword");
        let old = old_item("diamond_sword");
        let ing = |w: &mut Writer, id: i32| {
            w.varint(1).raw(&stack(id, None));
        };
        let mut p = Writer::new();
        p.varint(6);
        p.string("minecraft:crafting_shaped").string("a").varint(1).varint(2).string("g").varint(0);
        ing(&mut p, new);
        ing(&mut p, new);
        p.raw(&stack(new, None)).bool(true);
        p.string("minecraft:smithing_transform").string("b");
        for _ in 0..3 {
            ing(&mut p, new);
        }
        p.raw(&stack(new, None));
        p.string("minecraft:smithing_trim").string("c");
        for _ in 0..3 {
            ing(&mut p, new);
        }
        p.string("minecraft:crafting_decorated_pot").string("d").varint(2);
        p.string("minecraft:smithing").string("e");
        ing(&mut p, new);
        ing(&mut p, new);
        p.raw(&stack(new, None));
        p.string("minecraft:smelting").string("f").string("").varint(1);
        ing(&mut p, new);
        p.raw(&stack(new, None)).f32(0.1).varint(200);

        let mut want = Writer::new();
        want.varint(3);
        want.string("minecraft:crafting_shaped").string("a").varint(1).varint(2).string("g").varint(0);
        ing(&mut want, old);
        ing(&mut want, old);
        want.raw(&stack(old, None));
        want.string("minecraft:smithing").string("e");
        ing(&mut want, old);
        ing(&mut want, old);
        want.raw(&stack(old, None));
        want.string("minecraft:smelting").string("f").string("").varint(1);
        ing(&mut want, old);
        want.raw(&stack(old, None)).f32(0.1).varint(200);
        assert_eq!(update_recipes(&p.buf).unwrap(), want.buf);
        // a huge shaped recipe that isn't there is an error, not a long loop
        let mut p = Writer::new();
        p.varint(1).string("crafting_shaped").string("a").varint(i32::MAX).varint(i32::MAX).string("").varint(0);
        assert!(update_recipes(&p.buf).is_err());
    }

    #[test]
    fn creative_slot_and_container_click_go_to_the_server() {
        let new = new_item("diamond_sword");
        let old = old_item("diamond_sword");
        let mut p = Writer::new();
        p.i16(36).raw(&stack(old, None));
        let mut want = Writer::new();
        want.i16(36).raw(&stack(new, None));
        assert_eq!(set_creative_slot(&p.buf).unwrap(), want.buf);
        assert!(set_creative_slot(&p.buf[..3]).is_err());

        let build = |id: i32| {
            let mut p = Writer::new();
            p.u8(1).varint(4).i16(10).i8(0).varint(0).varint(1).i16(10).raw(&stack(id, None)).raw(&stack(id, None));
            p.into_inner()
        };
        assert_eq!(container_click(&build(old)).unwrap(), build(new));
        // a slot count with nothing behind it
        let mut p = Writer::new();
        p.u8(1).varint(4).i16(10).i8(0).varint(0).varint(i32::MAX);
        assert!(container_click(&p.buf).is_err());
    }
}

//! Items and the packets that carry them, 1.19.3 ↔ 1.19.1.
//!
//! The item stack layout is the same in both versions (present, id, count, named NBT); only the
//! ids differ. Items 1.19.1 lacks (bamboo wood, hanging signs, a few spawn eggs, the piglin
//! head) are shown as a similar item carrying the real name. For those the server's id is kept
//! under [`BACKUP_KEY`] in the client's NBT and restored when the client sends the item back
//! (creative inventory, container clicks), so nothing turns into its stand-in on the server.

use super::data_1_19_1 as d;
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

/// Root NBT key holding the server's (1.19.3) item: `{id: int, name: byte}`; `name` is set when
/// the display name was added by this step.
pub const BACKUP_KEY: &str = "multiversion:v1_19_3";

fn name_at(names: &'static [&'static str], id: i32) -> Option<&'static str> {
    names.get(usize::try_from(id).ok()?).copied()
}

fn lookup(table: &[u32], id: i32) -> Option<i32> {
    table.get(usize::try_from(id).ok()?).map(|v| *v as i32)
}

pub fn strip_ns(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}

/// 1.19.1 id for a 1.19.3 item (air for ids 1.19.3 doesn't have).
pub fn item_id(id: i32) -> i32 {
    lookup(&d::ITEMS, id).unwrap_or(0)
}

/// The item exists in 1.19.1 only as a stand-in.
pub fn substituted(id: i32) -> bool {
    match name_at(&d::NEW_ITEM_NAMES, id) {
        Some(name) => name_at(&d::OLD_ITEM_NAMES, item_id(id)) != Some(name),
        None => false,
    }
}

/// 1.19.1 item id for a 1.19.3 one, or `None` when 1.19.1 has no such item (for lists that
/// should not count a stand-in: tags, statistics).
pub fn item_exact(id: i32) -> Option<i32> {
    (name_at(&d::NEW_ITEM_NAMES, id).is_some() && !substituted(id)).then(|| item_id(id))
}

/// 1.19.1 block id for a 1.19.3 one, or `None` when 1.19.1 has no such block.
pub fn block_exact(id: i32) -> Option<i32> {
    let mapped = lookup(&d::BLOCKS, id)?;
    (name_at(&d::NEW_BLOCK_NAMES, id) == name_at(&d::OLD_BLOCK_NAMES, mapped)).then_some(mapped)
}

/// 1.19.1 entity type for a 1.19.3 one, or `None` when 1.19.1 has no such entity.
pub fn entity_exact(id: i32) -> Option<i32> {
    let mapped = lookup(&d::ENTITIES, id)?;
    (name_at(&d::NEW_ENTITY_NAMES, id) == name_at(&d::OLD_ENTITY_NAMES, mapped)).then_some(mapped)
}

pub fn block_state(id: i32) -> i32 {
    lookup(&d::BLOCK_STATES, id).unwrap_or(1)
}

/// "bamboo_planks" → "Bamboo Planks".
fn pretty_name(id: &str) -> String {
    id.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// NBT for a stand-in: the server's id as a backup and, unless the item has a custom name, the
/// real item's name (not italic, like a normal item name).
fn stand_in_tag(id: i32, tag: Option<Tag>) -> Tag {
    let mut t = match tag {
        Some(t @ Tag::Compound(_)) => t,
        _ => Tag::compound(),
    };
    let mut backup = Tag::compound();
    backup.insert("id", Tag::Int(id));
    if !matches!(t.get("display"), Some(Tag::Compound(_))) {
        t.insert("display", Tag::compound());
    }
    if let Some(display) = t.get_mut("display")
        && display.get("Name").is_none()
    {
        let name = pretty_name(name_at(&d::NEW_ITEM_NAMES, id).unwrap_or("item"));
        display.insert("Name", Tag::String(serde_json::json!({"text": name, "italic": false}).to_string()));
        backup.insert("name", Tag::Byte(1));
    }
    t.insert(BACKUP_KEY, backup);
    t
}

/// Skips one named NBT value and copies its bytes unchanged.
pub fn nbt_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    r.copy_to(w, |r| nbt::read_named_optional(r).map(|_| ()))
}

/// An item stack, 1.19.3 → 1.19.1.
pub fn item(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if !present {
        return Ok(());
    }
    let id = r.varint()?;
    w.varint(item_id(id));
    w.i8(r.i8()?);
    if substituted(id) {
        let tag = nbt::read_named_optional(r)?;
        nbt::write_named(w, &stand_in_tag(id, tag));
        Ok(())
    } else {
        nbt_copy(r, w)
    }
}

/// An item stack, 1.19.1 → 1.19.3: a stand-in with a backup becomes the server's item again.
pub fn item_to_server(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if !present {
        return Ok(());
    }
    let old_id = r.varint()?;
    let count = r.i8()?;
    let start = r.position();
    let tag = nbt::read_named_optional(r)?;
    let raw_len = r.position() - start;
    let mut id = lookup(&d::ITEMS_REVERSE, old_id).unwrap_or(0);
    let mut restored = None;
    if let Some(mut t) = tag
        && let Some(backup) = t.remove(BACKUP_KEY)
    {
        // only a backup that belongs to this stand-in counts: the key is in the client's hands
        if let Some(orig) = backup.get("id").and_then(Tag::as_i32)
            && substituted(orig)
            && item_id(orig) == old_id
        {
            id = orig;
            if backup.get("name").is_some() {
                if let Some(display) = t.get_mut("display") {
                    display.remove("Name");
                }
                if matches!(t.get("display"), Some(Tag::Compound(e)) if e.is_empty()) {
                    t.remove("display");
                }
            }
        }
        restored = Some(t);
    }
    w.varint(id);
    w.i8(count);
    match restored {
        Some(Tag::Compound(e)) if e.is_empty() => {
            w.u8(0);
        }
        Some(t) => nbt::write_named(w, &t),
        None => {
            r.set_position(start);
            w.raw(r.bytes(raw_len)?);
        }
    }
    Ok(())
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

/// Particle id + data. The ids are the same in both versions; block particles carry a state,
/// the item one a stack.
pub fn particle(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let id = varint_copy(r, w)?;
    particle_data(id, r, w)
}

fn particle_data(id: i32, r: &mut Reader, w: &mut Writer) -> Result<()> {
    match name_at(&d::NEW_PARTICLE_NAMES, id).unwrap_or("") {
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
            match strip_ns(source) {
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
    /// Shown as a donkey named "Camel"; its own two fields have no place on a donkey.
    Camel,
}

pub fn entity_kind(ty: i32) -> Option<Kind> {
    let name = name_at(&d::NEW_ENTITY_NAMES, ty)?;
    if name == "camel" {
        Some(Kind::Camel)
    } else if name.ends_with("minecart") {
        Some(Kind::Minecart)
    } else {
        None
    }
}

/// Entity data index of the custom name (an optional text component).
const CUSTOM_NAME: u8 = 2;
/// 1.19.3 entity data type ids this step looks at. 1.19.3 added the long at 2; every later type
/// is one lower in 1.19.1.
const TYPE_VARINT: i32 = 1;
const TYPE_LONG: i32 = 2;
const TYPE_OPT_COMPONENT: i32 = 6;
const TYPE_POSE: i32 = 19;
/// The sitting pose, new in 1.19.3 (camels); later poses are one lower in 1.19.1.
const POSE_SITTING: i32 = 10;

fn camel_name() -> String {
    r#"{"text":"Camel"}"#.to_string()
}

/// SET_ENTITY_DATA body (after the entity id). `first` = nothing was sent for this entity yet,
/// which is when a stand-in gets its name unless the server gives it one.
pub fn entity_data(kind: Option<Kind>, first: bool, r: &mut Reader, w: &mut Writer) -> Result<()> {
    let camel = kind == Some(Kind::Camel);
    let mut named = false;
    loop {
        let index = r.u8()?;
        if index == 0xff {
            if camel && first && !named {
                w.u8(CUSTOM_NAME).varint(TYPE_OPT_COMPONENT - 1).bool(true).string(&camel_name());
            }
            w.u8(0xff);
            return Ok(());
        }
        let ty = r.varint()?;
        let mut v = Writer::new();
        match ty {
            0 | 8 => {
                v.raw(r.bytes(1)?);
            }
            1 | 12 | 20 | 21 | 23 => {
                varint_copy(r, &mut v)?;
            }
            TYPE_LONG => {
                // 1.19.1 has no long type; only the camel uses it, for a field dropped below
                r.varlong()?;
                continue;
            }
            3 => {
                v.raw(r.bytes(4)?);
            }
            4 | 5 => string_copy(r, &mut v)?,
            TYPE_OPT_COMPONENT => {
                if r.bool()? {
                    v.bool(true);
                    string_copy(r, &mut v)?;
                } else if camel && index == CUSTOM_NAME {
                    // the server took the name away: back to the stand-in's
                    v.bool(true).string(&camel_name());
                } else {
                    v.bool(false);
                }
                named |= index == CUSTOM_NAME;
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
                // 0 = no block
                let s = r.varint()?;
                v.varint(if s == 0 { 0 } else { block_state(s) });
            }
            15 => nbt_copy(r, &mut v)?,
            16 => particle(r, &mut v)?,
            17 => {
                for _ in 0..3 {
                    varint_copy(r, &mut v)?;
                }
            }
            18 => {
                varint_copy(r, &mut v)?;
            }
            TYPE_POSE => {
                let pose = r.varint()?;
                v.varint(match pose {
                    POSE_SITTING => 0, // standing
                    p if p > POSE_SITTING => p - 1,
                    p => p,
                });
            }
            22 => {
                let some = r.bool()?;
                v.bool(some);
                if some {
                    string_copy(r, &mut v)?;
                    v.raw(r.bytes(8)?);
                }
            }
            other => return Err(DecodeError::Other(format!("unknown entity data type {other}"))),
        }
        match kind {
            // dashing and the last pose change: a donkey has "has chest" at 19 and nothing at 20
            Some(Kind::Camel) if index == 19 || index == 20 => continue,
            Some(Kind::Minecart) if index == 11 && ty == TYPE_VARINT => {
                let s = Reader::new(&v.buf).varint()?;
                v = Writer::new();
                v.varint(block_state(s));
            }
            _ => {}
        }
        w.u8(index);
        w.varint(if ty > TYPE_LONG { ty - 1 } else { ty });
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

pub fn cooldown(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::new();
    w.varint(item_id(r.varint()?));
    w.raw(r.remaining()); // ticks
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

/// Copies a list of strings.
fn strings_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let n = varint_copy(r, w)?;
    for _ in 0..n.max(0) {
        string_copy(r, w)?;
    }
    Ok(())
}

/// Same layout in both versions; only the icon is an item.
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
        strings_copy(&mut r, &mut w)?; // criteria
        let groups = varint_copy(&mut r, &mut w)?;
        for _ in 0..groups.max(0) {
            strings_copy(&mut r, &mut w)?; // requirements
        }
    }
    w.raw(r.remaining()); // removed ids, progress: no items
    Ok(w.into_inner())
}

fn ingredient(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let n = varint_copy(r, w)?;
    for _ in 0..n.max(0) {
        item(r, w)?;
    }
    Ok(())
}

/// UPDATE_RECIPES: 1.19.3 added the recipe book category to crafting and cooking recipes.
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
        match strip_ns(ty) {
            "crafting_shapeless" => {
                string_copy(&mut r, &mut w)?; // group
                r.varint()?; // category
                let k = varint_copy(&mut r, &mut w)?;
                for _ in 0..k.max(0) {
                    ingredient(&mut r, &mut w)?;
                }
                item(&mut r, &mut w)?;
            }
            "crafting_shaped" => {
                let width = varint_copy(&mut r, &mut w)?;
                let height = varint_copy(&mut r, &mut w)?;
                string_copy(&mut r, &mut w)?; // group
                r.varint()?; // category
                for _ in 0..i64::from(width.max(0)) * i64::from(height.max(0)) {
                    ingredient(&mut r, &mut w)?;
                }
                item(&mut r, &mut w)?;
            }
            "smelting" | "blasting" | "smoking" | "campfire_cooking" => {
                string_copy(&mut r, &mut w)?; // group
                r.varint()?; // category
                ingredient(&mut r, &mut w)?;
                item(&mut r, &mut w)?;
                w.raw(r.bytes(4)?); // xp
                varint_copy(&mut r, &mut w)?; // cooking time
            }
            "stonecutting" => {
                string_copy(&mut r, &mut w)?; // group
                ingredient(&mut r, &mut w)?;
                item(&mut r, &mut w)?;
            }
            "smithing" => {
                ingredient(&mut r, &mut w)?; // base
                ingredient(&mut r, &mut w)?; // addition
                item(&mut r, &mut w)?;
            }
            s if s.starts_with("crafting_special_") => {
                r.varint()?; // category
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

/// SET_CREATIVE_MODE_SLOT 1.19.1 → 1.19.3.
pub fn set_creative_slot(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.i16(r.i16()?);
    item_to_server(&mut r, &mut w)?;
    Ok(w.into_inner())
}

/// CONTAINER_CLICK 1.19.1 → 1.19.3.
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
pub(crate) mod tests {
    use super::*;

    pub fn new_item(name: &str) -> i32 {
        d::NEW_ITEM_NAMES.iter().position(|n| *n == name).unwrap() as i32
    }

    pub fn old_item(name: &str) -> i32 {
        d::OLD_ITEM_NAMES.iter().position(|n| *n == name).unwrap() as i32
    }

    /// A stack as either version sends it.
    pub fn stack(id: i32, count: i8, tag: Option<&Tag>) -> Vec<u8> {
        let mut w = Writer::new();
        w.bool(true).varint(id).i8(count);
        match tag {
            Some(t) => nbt::write_named(&mut w, t),
            None => {
                w.u8(0);
            }
        }
        w.into_inner()
    }

    fn to_client(bytes: &[u8]) -> Vec<u8> {
        let mut w = Writer::new();
        let mut r = Reader::new(bytes);
        item(&mut r, &mut w).unwrap();
        assert!(r.is_empty());
        w.into_inner()
    }

    fn to_server(bytes: &[u8]) -> Vec<u8> {
        let mut w = Writer::new();
        let mut r = Reader::new(bytes);
        item_to_server(&mut r, &mut w).unwrap();
        assert!(r.is_empty());
        w.into_inner()
    }

    fn tag_of(stack: &[u8]) -> Option<Tag> {
        let mut r = Reader::new(stack);
        r.bool().unwrap();
        r.varint().unwrap();
        r.i8().unwrap();
        nbt::read_named_optional(&mut r).unwrap()
    }

    #[test]
    fn items_both_versions_have_only_change_id() {
        let tag = Tag::Compound(vec![("Damage".into(), Tag::Int(3))]);
        // shears sit after the bamboo blocks, so their id moves
        let (new, old) = (new_item("shears"), old_item("shears"));
        assert_ne!(new, old);
        let out = to_client(&stack(new, 1, Some(&tag)));
        assert_eq!(out, stack(old, 1, Some(&tag)));
        assert_eq!(to_server(&out), stack(new, 1, Some(&tag)));
        assert_eq!(to_client(&stack(new, 64, None)), stack(old, 64, None));
        assert_eq!(to_client(&[0]), vec![0]);
        assert_eq!(to_server(&[0]), vec![0]);
    }

    #[test]
    fn items_the_client_lacks_become_named_stand_ins_and_come_back() {
        let input = stack(new_item("bamboo_planks"), 5, None);
        let out = to_client(&input);
        let mut r = Reader::new(&out);
        assert!(r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), old_item("birch_planks"));
        assert_eq!(r.i8().unwrap(), 5);
        let tag = tag_of(&out).unwrap();
        let name = tag.get("display").unwrap().get("Name").unwrap().as_str().unwrap();
        let name: serde_json::Value = serde_json::from_str(name).unwrap();
        assert_eq!(name, serde_json::json!({"text": "Bamboo Planks", "italic": false}));
        assert_eq!(tag.get(BACKUP_KEY).unwrap().get("id"), Some(&Tag::Int(new_item("bamboo_planks"))));
        assert_eq!(to_server(&out), input, "the server gets its own item back");
    }

    #[test]
    fn a_stand_in_keeps_the_servers_custom_name() {
        let display = Tag::Compound(vec![("Name".into(), Tag::String("\"Raft of doom\"".into()))]);
        let tag = Tag::Compound(vec![("display".into(), display), ("x".into(), Tag::Int(1))]);
        let input = stack(new_item("bamboo_raft"), 1, Some(&tag));
        let out = to_client(&input);
        let got = tag_of(&out).unwrap();
        assert_eq!(got.get("display").unwrap().get("Name"), Some(&Tag::String("\"Raft of doom\"".into())));
        assert!(got.get(BACKUP_KEY).unwrap().get("name").is_none());
        assert_eq!(to_server(&out), input);
    }

    #[test]
    fn a_backup_for_another_item_is_not_trusted() {
        // a client claims its birch planks are really a wither spawn egg
        let mut backup = Tag::compound();
        backup.insert("id", Tag::Int(new_item("wither_spawn_egg")));
        let tag = Tag::Compound(vec![(BACKUP_KEY.into(), backup), ("x".into(), Tag::Int(1))]);
        let out = to_server(&stack(old_item("birch_planks"), 1, Some(&tag)));
        let kept = Tag::Compound(vec![("x".into(), Tag::Int(1))]);
        assert_eq!(out, stack(new_item("birch_planks"), 1, Some(&kept)));
        // and an id far outside the registry is air, not a panic
        assert_eq!(to_server(&stack(i32::MAX, 1, None)), stack(0, 1, None));
        assert_eq!(to_server(&stack(-5, 1, None)), stack(0, 1, None));
    }

    #[test]
    fn entity_data_types_move_down_and_longs_go() {
        let mut p = Writer::new();
        p.u8(0).varint(0).u8(0x20); // byte: same type id
        p.u8(1).varint(1).varint(300); // varint
        p.u8(40).varint(2).varlong(1 << 40); // long: 1.19.1 has none
        p.u8(9).varint(3).f32(20.0); // float: 3 → 2
        p.u8(2).varint(6).bool(true).string("\"Bob\""); // optional component: 6 → 5
        p.u8(8).varint(7).raw(&stack(new_item("shears"), 1, None)); // item
        p.u8(6).varint(19).varint(10); // pose sitting → standing
        p.u8(7).varint(19).varint(12); // a later pose moves down
        p.u8(16).varint(14).varint(0); // no block
        p.u8(17).varint(15).raw(&[10, 0, 0, 0]); // empty named compound
        p.u8(0xff);
        let mut w = Writer::new();
        entity_data(None, true, &mut Reader::new(&p.buf), &mut w).unwrap();
        let mut want = Writer::new();
        want.u8(0).varint(0).u8(0x20);
        want.u8(1).varint(1).varint(300);
        want.u8(9).varint(2).f32(20.0);
        want.u8(2).varint(5).bool(true).string("\"Bob\"");
        want.u8(8).varint(6).raw(&stack(old_item("shears"), 1, None));
        want.u8(6).varint(18).varint(0);
        want.u8(7).varint(18).varint(11);
        want.u8(16).varint(13).varint(0);
        want.u8(17).varint(14).raw(&[10, 0, 0, 0]);
        want.u8(0xff);
        assert_eq!(w.buf, want.buf);
        assert!(entity_data(None, true, &mut Reader::new(&[1, 24, 0, 0xff]), &mut Writer::new()).is_err(), "unknown type");
    }

    #[test]
    fn camels_are_named_donkeys_without_camel_fields() {
        let mut p = Writer::new();
        p.u8(19).varint(8).bool(true); // dashing
        p.u8(20).varint(2).varlong(77); // last pose change
        p.u8(16).varint(8).bool(true); // baby: kept
        p.u8(0xff);
        let mut w = Writer::new();
        entity_data(Some(Kind::Camel), true, &mut Reader::new(&p.buf), &mut w).unwrap();
        let mut want = Writer::new();
        want.u8(16).varint(7).bool(true);
        want.u8(2).varint(5).bool(true).string(r#"{"text":"Camel"}"#);
        want.u8(0xff);
        assert_eq!(w.buf, want.buf);
        // later packets leave the name alone
        let mut w = Writer::new();
        entity_data(Some(Kind::Camel), false, &mut Reader::new(&p.buf), &mut w).unwrap();
        assert_eq!(w.buf, vec![16, 7, 1, 0xff]);
        // a name from the server wins; taking it away brings the stand-in's back
        let mut named = Writer::new();
        named.u8(2).varint(6).bool(true).string("\"Humphrey\"").u8(0xff);
        let mut w = Writer::new();
        entity_data(Some(Kind::Camel), true, &mut Reader::new(&named.buf), &mut w).unwrap();
        let mut want = Writer::new();
        want.u8(2).varint(5).bool(true).string("\"Humphrey\"").u8(0xff);
        assert_eq!(w.buf, want.buf);
        let mut w = Writer::new();
        entity_data(Some(Kind::Camel), false, &mut Reader::new(&[2, 6, 0, 0xff]), &mut w).unwrap();
        let mut want = Writer::new();
        want.u8(2).varint(5).bool(true).string(r#"{"text":"Camel"}"#).u8(0xff);
        assert_eq!(w.buf, want.buf);
    }

    #[test]
    fn minecart_blocks_and_particles_use_old_states() {
        let (state, mapped) = d::BLOCK_STATES.iter().enumerate().find(|(i, v)| **v as usize != *i).map(|(i, v)| (i as i32, *v as i32)).unwrap();
        let mut p = Writer::new();
        p.u8(11).varint(1).varint(state).u8(0xff);
        let mut w = Writer::new();
        entity_data(Some(Kind::Minecart), false, &mut Reader::new(&p.buf), &mut w).unwrap();
        let mut want = Writer::new();
        want.u8(11).varint(1).varint(mapped).u8(0xff);
        assert_eq!(w.buf, want.buf);
        // other entities keep a varint at index 11 as it is
        let mut w = Writer::new();
        entity_data(None, false, &mut Reader::new(&p.buf), &mut w).unwrap();
        assert_eq!(w.buf, p.buf);

        let block = d::NEW_PARTICLE_NAMES.iter().position(|n| *n == "block").unwrap() as i32;
        let mut p = Writer::new();
        p.varint(block).bool(false).f64(1.0).f64(2.0).f64(3.0).f32(0.0).f32(0.0).f32(0.0).f32(1.0).i32(8).varint(state);
        let out = level_particles(&p.buf).unwrap();
        let mut want = p.buf[..p.buf.len() - crate::proto::buf::varint_len(state)].to_vec();
        Writer::new().varint(mapped).buf.iter().for_each(|b| want.push(*b));
        assert_eq!(out, want);
    }

    #[test]
    fn recipes_lose_the_book_category() {
        let (new, old) = (new_item("shears"), old_item("shears"));
        let mut p = Writer::new();
        p.varint(5);
        p.string("minecraft:crafting_shapeless").string("a").string("g").varint(2).varint(1).varint(1).raw(&stack(new, 1, None)).raw(&stack(new, 2, None));
        p.string("minecraft:crafting_shaped").string("b").varint(1).varint(2).string("").varint(0);
        p.varint(1).raw(&stack(new, 1, None)).varint(0).raw(&stack(new, 1, None));
        p.string("minecraft:smelting").string("c").string("").varint(1).varint(1).raw(&stack(new, 1, None)).raw(&stack(new, 1, None)).f32(0.5).varint(200);
        p.string("minecraft:crafting_special_mapcloning").string("d").varint(3);
        p.string("minecraft:smithing").string("e").varint(0).varint(0).raw(&stack(new, 1, None));
        let out = update_recipes(&p.buf).unwrap();
        let mut want = Writer::new();
        want.varint(5);
        want.string("minecraft:crafting_shapeless").string("a").string("g").varint(1).varint(1).raw(&stack(old, 1, None)).raw(&stack(old, 2, None));
        want.string("minecraft:crafting_shaped").string("b").varint(1).varint(2).string("");
        want.varint(1).raw(&stack(old, 1, None)).varint(0).raw(&stack(old, 1, None));
        want.string("minecraft:smelting").string("c").string("").varint(1).raw(&stack(old, 1, None)).raw(&stack(old, 1, None)).f32(0.5).varint(200);
        want.string("minecraft:crafting_special_mapcloning").string("d");
        want.string("minecraft:smithing").string("e").varint(0).varint(0).raw(&stack(old, 1, None));
        assert_eq!(out, want.buf);
        // a huge shaped recipe is refused when the bytes run out, not looped over
        let mut p = Writer::new();
        p.varint(1).string("crafting_shaped").string("x").varint(i32::MAX).varint(i32::MAX).string("").varint(0);
        assert!(update_recipes(&p.buf).is_err());
    }

    #[test]
    fn item_packets_map_every_stack() {
        let (new, old) = (new_item("shears"), old_item("shears"));
        let (s_new, s_old) = (stack(new, 1, None), stack(old, 1, None));

        let mut p = Writer::new();
        p.u8(1).varint(7).varint(2).raw(&s_new).u8(0).raw(&s_new);
        let mut want = Writer::new();
        want.u8(1).varint(7).varint(2).raw(&s_old).u8(0).raw(&s_old);
        assert_eq!(container_set_content(&p.buf).unwrap(), want.buf);

        let mut p = Writer::new();
        p.u8(1).varint(7).i16(36).raw(&s_new);
        let mut want = Writer::new();
        want.u8(1).varint(7).i16(36).raw(&s_old);
        assert_eq!(container_set_slot(&p.buf).unwrap(), want.buf);

        let mut p = Writer::new();
        p.varint(9).u8(0x80).raw(&s_new).u8(5).u8(0);
        let mut want = Writer::new();
        want.varint(9).u8(0x80).raw(&s_old).u8(5).u8(0);
        assert_eq!(set_equipment(&p.buf).unwrap(), want.buf);

        let mut p = Writer::new();
        p.varint(new).varint(40);
        let mut want = Writer::new();
        want.varint(old).varint(40);
        assert_eq!(cooldown(&p.buf).unwrap(), want.buf);

        let tail = [0u8; 25];
        let mut p = Writer::new();
        p.varint(3).varint(1).raw(&s_new).raw(&s_new).u8(0).raw(&tail).varint(2).varint(10).bool(true).bool(false);
        let mut want = Writer::new();
        want.varint(3).varint(1).raw(&s_old).raw(&s_old).u8(0).raw(&tail).varint(2).varint(10).bool(true).bool(false);
        assert_eq!(merchant_offers(&p.buf).unwrap(), want.buf);

        let adv = |s: &[u8]| {
            let mut p = Writer::new();
            p.bool(false).varint(2);
            p.string("a").bool(true).string("root").bool(true).string("\"t\"").string("\"d\"").raw(s).varint(0).i32(1).string("bg").f32(1.0).f32(2.0);
            p.varint(1).string("c").varint(1).varint(1).string("c");
            p.string("b").bool(false).bool(false).varint(0).varint(0);
            p.varint(1).string("gone").varint(0);
            p.into_inner()
        };
        assert_eq!(update_advancements(&adv(&s_new)).unwrap(), adv(&s_old));
    }

    #[test]
    fn serverbound_item_packets_map_every_stack() {
        let (new, old) = (new_item("shears"), old_item("shears"));
        let (s_new, s_old) = (stack(new, 1, None), stack(old, 1, None));
        let mut p = Writer::new();
        p.i16(36).raw(&s_old);
        let mut want = Writer::new();
        want.i16(36).raw(&s_new);
        assert_eq!(set_creative_slot(&p.buf).unwrap(), want.buf);

        let mut p = Writer::new();
        p.u8(0).varint(4).i16(10).i8(0).varint(0).varint(1).i16(10).raw(&s_old).u8(0);
        let mut want = Writer::new();
        want.u8(0).varint(4).i16(10).i8(0).varint(0).varint(1).i16(10).raw(&s_new).u8(0);
        assert_eq!(container_click(&p.buf).unwrap(), want.buf);
        // a count the packet can't hold is an error, not an allocation
        let mut p = Writer::new();
        p.u8(0).varint(4).i16(10).i8(0).varint(0).varint(i32::MAX);
        assert!(container_click(&p.buf).is_err());
    }
}

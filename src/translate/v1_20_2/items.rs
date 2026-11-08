//! Registry ids 1.20.3 ↔ 1.20.2 and item stacks.
//!
//! Both versions send items the same way (present flag, item id, count, NBT); only the ids and
//! JSON written by 1.20.3 into display names and lore differ.

use super::data_1_20_2 as d;
use super::text::fix_json;
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

pub fn err(msg: &str) -> DecodeError {
    DecodeError::Other(msg.to_string())
}

fn lookup(table: &[u32], id: i32, fallback: i32) -> i32 {
    usize::try_from(id).ok().and_then(|i| table.get(i)).map_or(fallback, |v| *v as i32)
}

fn name_index(names: &[&str], name: &str) -> Option<i32> {
    names.iter().position(|n| *n == name).map(|i| i as i32)
}

pub fn item(id: i32) -> i32 {
    lookup(&d::ITEMS, id, 0)
}
pub fn item_to_server(id: i32) -> i32 {
    lookup(&d::ITEMS_REVERSE, id, 0)
}
pub fn block_state(id: i32) -> i32 {
    lookup(&d::BLOCK_STATES, id, 1)
}
pub fn block(id: i32) -> i32 {
    lookup(&d::BLOCKS, id, 1)
}
/// Blocks without a 1.20.2 counterpart map to stone; tags must not list them.
pub fn block_known(id: i32) -> bool {
    id == 1 || block(id) != 1
}
pub fn entity(id: i32) -> i32 {
    lookup(&d::ENTITIES, id, d::TEXT_DISPLAY_ENTITY_OLD as i32)
}
pub fn particle(id: i32) -> i32 {
    lookup(&d::PARTICLES, id, 0)
}
/// `None` = a 1.20.3 sound 1.20.2 has no stand-in for (breeze, trial spawner).
pub fn sound(id: i32) -> Option<i32> {
    let v = lookup(&d::SOUNDS, id, 0);
    (v != 0 || id == 0).then_some(v)
}
/// `None` = crafter and trial spawner.
pub fn block_entity(id: i32) -> Option<i32> {
    let v = lookup(&d::BLOCK_ENTITIES, id, 0);
    (v != 0 || id == 0).then_some(v)
}
/// `None` = the crafter menu.
pub fn menu(id: i32) -> Option<i32> {
    let name = d::NEW_MENU_NAMES.get(usize::try_from(id).ok()?)?;
    name_index(&d::OLD_MENU_NAMES, name)
}
pub fn entity_name(id: i32) -> &'static str {
    usize::try_from(id).ok().and_then(|i| d::NEW_ENTITY_NAMES.get(i)).copied().unwrap_or("")
}

pub fn copy_varint(r: &mut Reader, w: &mut Writer) -> Result<i32> {
    let v = r.varint()?;
    w.varint(v);
    Ok(v)
}
pub fn copy_bool(r: &mut Reader, w: &mut Writer) -> Result<bool> {
    let b = r.bool()?;
    w.bool(b);
    Ok(b)
}
pub fn copy_string(r: &mut Reader, w: &mut Writer) -> Result<()> {
    w.string(r.string()?);
    Ok(())
}
pub fn copy_bytes(r: &mut Reader, w: &mut Writer, n: usize) -> Result<()> {
    w.raw(r.bytes(n)?);
    Ok(())
}
pub fn copy_opt_string(r: &mut Reader, w: &mut Writer) -> Result<()> {
    if copy_bool(r, w)? {
        copy_string(r, w)?;
    }
    Ok(())
}

/// Reads one NBT value and writes it back, re-encoded only if `fix` changed it (so odd
/// strings keep their exact bytes).
pub fn copy_nbt_with(r: &mut Reader, w: &mut Writer, fix: impl FnOnce(&mut Tag) -> bool) -> Result<()> {
    let start = r.position();
    let mut tag = nbt::read(r)?;
    if fix(&mut tag) {
        nbt::write(w, &tag);
    } else {
        let end = r.position();
        r.set_position(start);
        w.raw(r.bytes(end - start)?);
    }
    Ok(())
}

pub fn copy_nbt(r: &mut Reader, w: &mut Writer) -> Result<()> {
    copy_nbt_with(r, w, |_| false)
}

/// Rewrites a JSON string field in place; true if it changed.
pub fn fix_json_field(tag: &mut Tag, key: &str) -> bool {
    match tag.get_mut(key) {
        Some(Tag::String(s)) => match fix_json(s) {
            Some(f) => {
                *s = f;
                true
            }
            None => false,
        },
        _ => false,
    }
}

fn fix_item_tag(tag: &mut Tag) -> bool {
    let Some(display) = tag.get_mut("display") else { return false };
    let mut changed = fix_json_field(display, "Name");
    if let Some(Tag::List(lore)) = display.get_mut("Lore") {
        for line in lore {
            if let Tag::String(s) = line {
                if let Some(f) = fix_json(s) {
                    *s = f;
                    changed = true;
                }
            }
        }
    }
    changed
}

/// One item stack, server → client.
pub fn item_stack(r: &mut Reader, w: &mut Writer) -> Result<()> {
    if !copy_bool(r, w)? {
        return Ok(());
    }
    w.varint(item(r.varint()?));
    w.i8(r.i8()?);
    copy_nbt_with(r, w, fix_item_tag)
}

/// One item stack, client → server.
pub fn item_stack_to_server(r: &mut Reader, w: &mut Writer) -> Result<()> {
    if !copy_bool(r, w)? {
        return Ok(());
    }
    w.varint(item_to_server(r.varint()?));
    w.i8(r.i8()?);
    copy_nbt(r, w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_item(name: &str) -> i32 {
        name_index(&d::NEW_ITEM_NAMES, name).unwrap()
    }
    fn old_item(name: &str) -> i32 {
        name_index(&d::OLD_ITEM_NAMES, name).unwrap()
    }

    #[test]
    fn new_items_get_stand_ins_and_known_ones_round_trip() {
        assert_eq!(item(new_item("crafter")), old_item("crafting_table"));
        assert_eq!(item(new_item("diamond")), old_item("diamond"));
        assert_eq!(item_to_server(old_item("diamond")), new_item("diamond"));
        // 1.20.3 renamed grass to short_grass
        assert_eq!(item(new_item("short_grass")), old_item("grass"));
    }

    #[test]
    fn unknown_registry_entries_are_reported() {
        let new_sound = name_index(&d::NEW_SOUND_NAMES, "entity.breeze.shoot").unwrap();
        assert_eq!(sound(new_sound), None);
        assert_eq!(sound(0), Some(0));
        let crafter = name_index(&d::NEW_BLOCK_ENTITY_NAMES, "crafter").unwrap();
        assert_eq!(block_entity(crafter), None);
        assert_eq!(block_entity(0), Some(0));
        let crafter_menu = name_index(&d::NEW_MENU_NAMES, "crafter_3x3").unwrap();
        assert_eq!(menu(crafter_menu), None);
        let anvil = name_index(&d::NEW_MENU_NAMES, "anvil").unwrap();
        assert_eq!(menu(anvil), name_index(&d::OLD_MENU_NAMES, "anvil"));
        let tuff_wall = name_index(&d::NEW_BLOCK_NAMES, "tuff_wall").unwrap();
        assert!(!block_known(tuff_wall));
        assert!(block_known(1));
        let breeze = name_index(&d::NEW_ENTITY_NAMES, "breeze").unwrap();
        assert_eq!(entity(breeze), name_index(&d::OLD_ENTITY_NAMES, "blaze").unwrap());
    }

    #[test]
    fn item_stack_keeps_nbt_and_fixes_names() {
        let display = Tag::Compound(vec![(
            "display".into(),
            Tag::Compound(vec![("Name".into(), Tag::String(r#"{"text":"x","hoverEvent":{"action":"show_entity","contents":{"type":"minecraft:pig","id":[0,0,0,2]}}}"#.into()))]),
        )]);
        let mut w = Writer::new();
        w.bool(true).varint(new_item("crafter")).i8(3);
        nbt::write(&mut w, &display);
        let mut out = Writer::new();
        item_stack(&mut Reader::new(&w.buf), &mut out).unwrap();
        let mut r = Reader::new(&out.buf);
        assert!(r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), old_item("crafting_table"));
        assert_eq!(r.i8().unwrap(), 3);
        let tag = nbt::read(&mut r).unwrap();
        let name = tag.get("display").and_then(|d| d.get("Name")).and_then(Tag::as_str).unwrap();
        assert!(name.contains("00000000-0000-0000-0000-000000000002"), "{name}");
        // empty slot and no NBT
        let mut out = Writer::new();
        item_stack(&mut Reader::new(&[0]), &mut out).unwrap();
        assert_eq!(out.buf, vec![0]);
        let mut w = Writer::new();
        w.bool(true).varint(old_item("diamond")).i8(1).u8(0);
        let mut out = Writer::new();
        item_stack_to_server(&mut Reader::new(&w.buf), &mut out).unwrap();
        let mut expect = Writer::new();
        expect.bool(true).varint(new_item("diamond")).i8(1).u8(0);
        assert_eq!(out.buf, expect.buf);
    }
}

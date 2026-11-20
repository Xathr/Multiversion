//! Text components 26.3 → 26.2.
//!
//! The text format itself is unchanged, but hover events name items and entities: a `show_item`
//! of an item 26.2 doesn't have (poplar planks, ...) makes the 26.2 client fail to decode the whole
//! packet and disconnect. Item and entity ids are mapped like everywhere else in this step, and item
//! components 26.2 doesn't know are dropped from the hover (as ViaBackwards' `ComponentRewriter26_3`).

use super::data_26_2 as d;
use super::items::Items;
use crate::proto::buf::{Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};
use crate::translate::table::position;

/// Hover-item components 26.2 can't read: new in 26.3, or changed shape (ComponentRewriter26_3,
/// plus the ones ViaVersion drops from every hover item).
const DROPPED_HOVER_COMPONENTS: &[&str] = &[
    "provides_trim_material", "provides_pottery_pattern", "block_transformer", "compostable", "interact_animation",
    "attack_animation", "trim", "cooking_fuel", "brewing_fuel", "villager_food", "mob_visibility", "sign_text_back",
    "sign_text_front", "waxed", "cushion/color", "map_decorations", "pot_decorations",
    "lock", "debug_stick_state", "equippable", "consumable", "blocks_attacks", "break_sound", "note_block_sound", "tool",
];

fn strip_ns(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}

/// `name` mapped through `new_names` → id → `map` → `old_names`, keeping its namespace style.
/// Unknown names are left alone.
fn map_name(name: &str, new_names: &'static [&'static str], old_names: &'static [&'static str], map: impl Fn(i32) -> i32) -> Option<String> {
    let id = position(new_names, strip_ns(name))?;
    let old = old_names.get(usize::try_from(map(id)).ok()?)?;
    Some(if name.starts_with("minecraft:") { format!("minecraft:{old}") } else { (*old).to_string() })
}

/// Rewrites a text component in place.
pub fn downgrade(items: &Items, tag: &mut Tag) {
    match tag {
        Tag::List(list) => list.iter_mut().for_each(|t| downgrade(items, t)),
        Tag::Compound(_) => {
            for key in ["with", "extra"] {
                if let Some(child) = tag.get_mut(key) {
                    downgrade(items, child);
                }
            }
            if let Some(hover) = tag.get_mut("hover_event") {
                downgrade_hover(items, hover);
            }
        }
        _ => {}
    }
}

fn downgrade_hover(items: &Items, hover: &mut Tag) {
    let Some(action) = hover.get("action").and_then(Tag::as_str).map(|a| strip_ns(a).to_string()) else { return };
    match action.as_str() {
        "show_text" => {
            if let Some(value) = hover.get_mut("value") {
                downgrade(items, value);
            }
        }
        "show_entity" => {
            if let Some(name) = hover.get_mut("name") {
                downgrade(items, name);
            }
            if let Some(Tag::String(id)) = hover.get_mut("id") {
                // as in ADD_ENTITY: the cushion is shown as a falling block
                let mapped = map_name(id, &d::NEW_ENTITY_NAMES, &d::OLD_ENTITY_NAMES, |t| {
                    if t == d::CUSHION_ENTITY_NEW as i32 { d::FALLING_BLOCK_ENTITY_OLD as i32 } else { items.entity(t) }
                });
                if let Some(mapped) = mapped {
                    *id = mapped;
                }
            }
        }
        "show_item" => {
            if let Some(Tag::String(id)) = hover.get_mut("id") {
                if let Some(mapped) = map_name(id, &d::NEW_ITEM_NAMES, &d::OLD_ITEM_NAMES, |i| items.item(i)) {
                    *id = mapped;
                }
            }
            if let Some(Tag::Compound(components)) = hover.get_mut("components") {
                components.retain(|(key, _)| {
                    let name = strip_ns(key.strip_prefix('!').unwrap_or(key));
                    !DROPPED_HOVER_COMPONENTS.contains(&name)
                });
                for (key, value) in components.iter_mut() {
                    if matches!(strip_ns(key), "custom_name" | "item_name" | "lore") {
                        downgrade(items, value);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Copies one text component (network NBT), downgraded.
pub fn copy_text(items: &Items, r: &mut Reader, w: &mut Writer) -> Result<()> {
    let mut tag = nbt::read(r)?;
    downgrade(items, &mut tag);
    nbt::write(w, &tag);
    Ok(())
}

/// Copies an optional text component (boolean, then the component).
pub fn copy_opt_text(items: &Items, r: &mut Reader, w: &mut Writer) -> Result<()> {
    if Items::copy_bool(r, w)? {
        copy_text(items, r, w)?;
    }
    Ok(())
}

/// Packets whose payload starts with a text component (SYSTEM_CHAT, titles, action bar,
/// DISCONNECT, SERVER_DATA): the rest is copied as is.
pub fn leading_text(items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_text(items, &mut r, &mut w)?;
    w.raw(r.remaining());
    Ok(w.into_inner())
}

pub fn boss_event(items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.uuid(r.uuid()?);
    let action = Items::copy_varint(&mut r, &mut w)?;
    if action == 0 || action == 3 {
        copy_text(items, &mut r, &mut w)?; // title (add / update title)
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

pub fn tab_list(items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_text(items, &mut r, &mut w)?; // header
    copy_text(items, &mut r, &mut w)?; // footer
    w.raw(r.remaining());
    Ok(w.into_inner())
}

pub fn set_objective(items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    Items::copy_string(&mut r, &mut w)?;
    let mode = r.i8()?;
    w.i8(mode);
    if mode == 0 || mode == 2 {
        copy_text(items, &mut r, &mut w)?; // display name
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

pub fn set_score(items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    Items::copy_string(&mut r, &mut w)?; // owner
    Items::copy_string(&mut r, &mut w)?; // objective
    Items::copy_varint(&mut r, &mut w)?; // score
    copy_opt_text(items, &mut r, &mut w)?; // display name
    if Items::copy_bool(&mut r, &mut w)? {
        let format = Items::copy_varint(&mut r, &mut w)?;
        if format == 1 || format == 2 {
            copy_text(items, &mut r, &mut w)?; // styled / fixed
        }
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

pub fn set_player_team(items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    Items::copy_string(&mut r, &mut w)?;
    let mode = r.i8()?;
    w.i8(mode);
    if mode == 0 || mode == 2 {
        for _ in 0..3 {
            copy_text(items, &mut r, &mut w)?; // display name, prefix, suffix
        }
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// PLAYER_INFO_UPDATE: only the display names (action bit 5) are text.
pub fn player_info_update(items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let actions = r.u8()?;
    w.u8(actions);
    let has = |bit: u8| actions & (1 << bit) != 0;
    if !has(5) {
        w.raw(r.remaining());
        return Ok(w.into_inner());
    }
    let entries = Items::copy_varint(&mut r, &mut w)?;
    for _ in 0..entries.max(0) {
        w.uuid(r.uuid()?);
        if has(0) {
            Items::copy_string(&mut r, &mut w)?; // name
            let properties = Items::copy_varint(&mut r, &mut w)?;
            for _ in 0..properties.max(0) {
                Items::copy_string(&mut r, &mut w)?;
                Items::copy_string(&mut r, &mut w)?;
                Items::copy_opt_string(&mut r, &mut w)?; // signature
            }
        }
        if has(1) && Items::copy_bool(&mut r, &mut w)? {
            w.uuid(r.uuid()?); // chat session
            w.i64(r.i64()?); // key expiry
            w.byte_array(r.byte_array()?);
            w.byte_array(r.byte_array()?);
        }
        if has(2) {
            Items::copy_varint(&mut r, &mut w)?; // game mode
        }
        if has(3) {
            Items::copy_bool(&mut r, &mut w)?; // listed
        }
        if has(4) {
            Items::copy_varint(&mut r, &mut w)?; // latency
        }
        copy_opt_text(items, &mut r, &mut w)?; // display name
        if has(6) {
            Items::copy_varint(&mut r, &mut w)?; // list order
        }
        if has(7) {
            Items::copy_bool(&mut r, &mut w)?; // show hat
        }
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

pub fn player_combat_kill(items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    Items::copy_varint(&mut r, &mut w)?; // player id
    copy_text(items, &mut r, &mut w)?;
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// Message, chat type, sender name and optional target name; a chat type sent inline (holder 0)
/// is left as is together with the names after it.
pub fn disguised_chat(items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_text(items, &mut r, &mut w)?;
    if Items::copy_varint(&mut r, &mut w)? != 0 {
        copy_text(items, &mut r, &mut w)?;
        copy_opt_text(items, &mut r, &mut w)?;
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translate::v26_2::items::Dir;

    fn s(v: &str) -> Tag {
        Tag::String(v.into())
    }
    fn compound(entries: Vec<(&str, Tag)>) -> Tag {
        Tag::Compound(entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
    fn write(tag: &Tag) -> Vec<u8> {
        let mut w = Writer::new();
        nbt::write(&mut w, tag);
        w.into_inner()
    }

    /// The SYSTEM_CHAT Pumpkin sends after `/give @s minecraft:poplar_planks` (NBT lists hold one
    /// tag type, so plain strings among compounds are `{text: ...}` compounds on the wire).
    fn give_message(item: &str, key: &str) -> Tag {
        compound(vec![
            ("translate", s("commands.give.success.single")),
            ("with", Tag::List(vec![
                compound(vec![("text", s("1"))]),
                compound(vec![
                    ("extra", Tag::List(vec![compound(vec![("translate", s(key))]), compound(vec![("text", s("]"))])])),
                    ("hover_event", compound(vec![("action", s("show_item")), ("count", Tag::Int(1)), ("id", s(item))])),
                    ("text", s("[")),
                ]),
            ])),
        ])
    }

    #[test]
    fn system_chat_with_a_26_3_only_item_hover_gets_the_26_2_item() {
        let items = Items::new(Dir::ToClient);
        let mut payload = write(&give_message("poplar_planks", "block.minecraft.poplar_planks"));
        payload.push(0); // overlay = false
        let out = leading_text(&items, &payload).unwrap();
        let mut expected = write(&give_message("birch_planks", "block.minecraft.poplar_planks"));
        expected.push(0);
        assert_eq!(out, expected);
    }

    #[test]
    fn known_items_and_namespaces_are_kept() {
        let items = Items::new(Dir::ToClient);
        let mut tag = compound(vec![("hover_event", compound(vec![("action", s("show_item")), ("id", s("minecraft:diamond_sword"))]))]);
        downgrade(&items, &mut tag);
        assert_eq!(tag.get("hover_event").and_then(|h| h.get("id")).and_then(Tag::as_str), Some("minecraft:diamond_sword"));
        let mut poplar = compound(vec![("hover_event", compound(vec![("action", s("show_item")), ("id", s("minecraft:poplar_planks"))]))]);
        downgrade(&items, &mut poplar);
        assert_eq!(poplar.get("hover_event").and_then(|h| h.get("id")).and_then(Tag::as_str), Some("minecraft:birch_planks"));
    }

    #[test]
    fn hover_components_26_2_cannot_read_are_dropped() {
        let items = Items::new(Dir::ToClient);
        let mut tag = compound(vec![(
            "hover_event",
            compound(vec![
                ("action", s("show_item")),
                ("id", s("minecraft:stone")),
                ("components", compound(vec![("minecraft:waxed", compound(vec![])), ("minecraft:custom_name", s("Stone")), ("!minecraft:attack_animation", compound(vec![]))])),
            ]),
        )]);
        downgrade(&items, &mut tag);
        let components = tag.get("hover_event").and_then(|h| h.get("components")).unwrap();
        assert_eq!(components, &compound(vec![("minecraft:custom_name", s("Stone"))]));
    }

    #[test]
    fn show_entity_cushion_becomes_a_falling_block() {
        let items = Items::new(Dir::ToClient);
        let mut tag = compound(vec![("hover_event", compound(vec![("action", s("show_entity")), ("id", s("minecraft:cushion"))]))]);
        downgrade(&items, &mut tag);
        assert_eq!(tag.get("hover_event").and_then(|h| h.get("id")).and_then(Tag::as_str), Some("minecraft:falling_block"));
    }

    #[test]
    fn team_and_player_list_texts_are_walked() {
        let items = Items::new(Dir::ToClient);
        let old = write(&give_message("poplar_planks", "k"));
        let new = write(&give_message("birch_planks", "k"));
        let mut team = Writer::new();
        team.string("t").i8(0).raw(&old).raw(&old).raw(&old).raw(&[0, 0, 0, 0]);
        let mut expected = Writer::new();
        expected.string("t").i8(0).raw(&new).raw(&new).raw(&new).raw(&[0, 0, 0, 0]);
        assert_eq!(set_player_team(&items, &team.into_inner()).unwrap(), expected.into_inner());

        // actions: listed (3) and display name (5)
        let mut info = Writer::new();
        info.u8(0b0010_1000).varint(1).uuid(7).bool(true).bool(true).raw(&old);
        let mut expected = Writer::new();
        expected.u8(0b0010_1000).varint(1).uuid(7).bool(true).bool(true).raw(&new);
        assert_eq!(player_info_update(&items, &info.into_inner()).unwrap(), expected.into_inner());
    }
}

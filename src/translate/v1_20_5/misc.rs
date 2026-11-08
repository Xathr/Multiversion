//! Everything else 1.21 ↔ 1.20.5: particles, sounds, explosions, containers, merchants,
//! advancements, recipes, attributes, chat and item hovers in text.

use std::cell::Cell;

use super::data_1_20_5 as d;
use super::items::{attribute_id_to_name, attribute_id_to_uuid, strip_ns, Items};
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

fn err(msg: &str) -> DecodeError {
    DecodeError::Other(msg.to_string())
}

/// The enchanting table menu id (1.20.5 and 1.21).
const ENCHANTMENT_MENU: i32 = 13;

// ---- particles ---------------------------------------------------------------------------------

/// One particle (id + data). Ids and layouts are identical; only item particles change.
pub fn particle(items: &Items, r: &mut Reader, w: &mut Writer) -> Result<()> {
    let id = Items::copy_varint(r, w)?;
    match d::NEW_PARTICLE_NAMES.get(id.max(0) as usize).copied().unwrap_or("") {
        "block" | "block_marker" | "falling_dust" | "dust_pillar" | "shriek" => {
            Items::copy_varint(r, w)?;
        }
        "item" => items.item_stack(r, w)?,
        "dust" => Items::copy_bytes(r, w, 16)?,
        "dust_color_transition" => Items::copy_bytes(r, w, 28)?,
        "vibration" => {
            match Items::copy_varint(r, w)? {
                0 => Items::copy_bytes(r, w, 8)?,
                1 => {
                    Items::copy_varint(r, w)?;
                    Items::copy_bytes(r, w, 4)?;
                }
                _ => return Err(err("unknown vibration source")),
            }
            Items::copy_varint(r, w)?;
        }
        "sculk_charge" | "entity_effect" => Items::copy_bytes(r, w, 4)?,
        _ => {}
    }
    Ok(())
}

pub struct Misc {
    menu: Cell<i32>,
    /// Last rotation the client reported (1.21 servers read it from USE_ITEM).
    yaw: Cell<f32>,
    pitch: Cell<f32>,
}

impl Default for Misc {
    fn default() -> Self {
        Self::new()
    }
}

impl Misc {
    pub fn new() -> Self {
        Self { menu: Cell::new(-1), yaw: Cell::new(0.0), pitch: Cell::new(0.0) }
    }

    /// LEVEL_PARTICLES: same header in both versions, particle last.
    pub fn level_particles(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.raw(r.bytes(1 + 24 + 12 + 4 + 4)?);
        particle(items, &mut r, &mut w)?;
        Ok(w.into_inner())
    }

    /// SOUND / SOUND_ENTITY: sound event first.
    pub fn sound(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        items.sound_event(&mut r, &mut w)?;
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    /// EXPLODE: center, power, affected blocks, knockback, interaction, two particles, sound.
    pub fn explode(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.raw(r.bytes(28)?);
        let n = Items::copy_varint(&mut r, &mut w)?;
        w.raw(r.bytes(n.max(0) as usize * 3)?);
        w.raw(r.bytes(12)?);
        Items::copy_varint(&mut r, &mut w)?;
        particle(items, &mut r, &mut w)?;
        particle(items, &mut r, &mut w)?;
        items.sound_event(&mut r, &mut w)?;
        Ok(w.into_inner())
    }

    // ---- containers ------------------------------------------------------------------------

    pub fn open_screen(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        r.varint()?;
        self.menu.set(r.varint()?);
        Ok(payload.to_vec())
    }

    pub fn container_close(&self) {
        self.menu.set(-1);
    }

    /// CONTAINER_SET_DATA: the enchanting table's hint properties (4-6) name enchantments.
    pub fn container_set_data(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let container = r.u8()?;
        let property = r.i16()?;
        let value = r.i16()?;
        if self.menu.get() != ENCHANTMENT_MENU || !(4..=6).contains(&property) || value < 0 {
            return Ok(payload.to_vec());
        }
        let mut w = Writer::new();
        w.u8(container);
        w.i16(property);
        w.i16(items.enchantment(i32::from(value)).map_or(-1, |v| v as i16));
        Ok(w.into_inner())
    }

    /// HORSE_SCREEN_OPEN: 1.21 sends inventory columns, 1.20.5 the slot count.
    pub fn horse_screen_open(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.u8(r.u8()?);
        w.varint(r.varint()? * 3 + 1);
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    pub fn container_set_content(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.u8(r.u8()?);
        Items::copy_varint(&mut r, &mut w)?; // state id
        let n = Items::copy_varint(&mut r, &mut w)?;
        for _ in 0..n.max(0) {
            items.item_stack(&mut r, &mut w)?;
        }
        items.item_stack(&mut r, &mut w)?; // carried
        Ok(w.into_inner())
    }

    pub fn container_set_slot(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.u8(r.u8()?);
        Items::copy_varint(&mut r, &mut w)?;
        w.i16(r.i16()?);
        items.item_stack(&mut r, &mut w)?;
        Ok(w.into_inner())
    }

    pub fn merchant_offers(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.varint(r.varint()?);
        let n = Items::copy_varint(&mut r, &mut w)?;
        for _ in 0..n.max(0) {
            items.item_cost(&mut r, &mut w)?;
            items.item_stack(&mut r, &mut w)?;
            items.opt_item_cost(&mut r, &mut w)?;
            w.raw(r.bytes(1 + 4 + 4 + 4 + 4 + 4 + 4)?);
        }
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    /// COOLDOWN: item id + ticks.
    pub fn cooldown(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.varint(items.item(r.varint()?));
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    /// AWARD_STATS: (stat type, stat id, value); item stats name items by id.
    pub fn award_stats(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        let n = Items::copy_varint(&mut r, &mut w)?;
        for _ in 0..n.max(0) {
            let category = Items::copy_varint(&mut r, &mut w)?;
            let id = r.varint()?;
            // crafted, used, broken, picked up, dropped; blocks and entities are unchanged
            w.varint(if (1..=5).contains(&category) { items.item(id) } else { id });
            Items::copy_varint(&mut r, &mut w)?;
        }
        Ok(w.into_inner())
    }

    pub fn update_advancements(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.bool(r.bool()?);
        let n = Items::copy_varint(&mut r, &mut w)?;
        for _ in 0..n.max(0) {
            w.string(r.string()?);
            Items::copy_opt_string(&mut r, &mut w)?;
            if Items::copy_bool(&mut r, &mut w)? {
                Items::copy_nbt(&mut r, &mut w)?; // title
                Items::copy_nbt(&mut r, &mut w)?; // description
                items.item_stack(&mut r, &mut w)?; // icon
                Items::copy_varint(&mut r, &mut w)?; // frame
                let flags = r.i32()?;
                w.i32(flags);
                if flags & 1 != 0 {
                    w.string(r.string()?);
                }
                w.raw(r.bytes(8)?); // x, y
            }
            let reqs = Items::copy_varint(&mut r, &mut w)?;
            for _ in 0..reqs.max(0) {
                let m = Items::copy_varint(&mut r, &mut w)?;
                for _ in 0..m.max(0) {
                    w.string(r.string()?);
                }
            }
            w.bool(r.bool()?); // telemetry
        }
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    /// UPDATE_RECIPES: same layout in both versions; only the item stacks change.
    pub fn update_recipes(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        let ingredient = |r: &mut Reader, w: &mut Writer| -> Result<()> {
            let n = Items::copy_varint(r, w)?;
            for _ in 0..n.max(0) {
                items.item_stack(r, w)?;
            }
            Ok(())
        };
        let n = Items::copy_varint(&mut r, &mut w)?;
        for _ in 0..n.max(0) {
            w.string(r.string()?);
            let serializer = Items::copy_varint(&mut r, &mut w)?;
            match d::NEW_RECIPE_SERIALIZER_NAMES.get(serializer.max(0) as usize).copied().unwrap_or("") {
                "crafting_shaped" => {
                    w.string(r.string()?); // group
                    Items::copy_varint(&mut r, &mut w)?; // category
                    let width = Items::copy_varint(&mut r, &mut w)?;
                    let height = Items::copy_varint(&mut r, &mut w)?;
                    for _ in 0..(width.max(0) * height.max(0)) {
                        ingredient(&mut r, &mut w)?;
                    }
                    items.item_stack(&mut r, &mut w)?;
                    w.bool(r.bool()?); // show notification
                }
                "crafting_shapeless" => {
                    w.string(r.string()?);
                    Items::copy_varint(&mut r, &mut w)?;
                    let k = Items::copy_varint(&mut r, &mut w)?;
                    for _ in 0..k.max(0) {
                        ingredient(&mut r, &mut w)?;
                    }
                    items.item_stack(&mut r, &mut w)?;
                }
                "smelting" | "blasting" | "smoking" | "campfire_cooking" => {
                    w.string(r.string()?);
                    Items::copy_varint(&mut r, &mut w)?;
                    ingredient(&mut r, &mut w)?;
                    items.item_stack(&mut r, &mut w)?;
                    w.raw(r.bytes(4)?); // experience
                    Items::copy_varint(&mut r, &mut w)?; // cooking time
                }
                "stonecutting" => {
                    w.string(r.string()?);
                    ingredient(&mut r, &mut w)?;
                    items.item_stack(&mut r, &mut w)?;
                }
                "smithing_transform" => {
                    for _ in 0..3 {
                        ingredient(&mut r, &mut w)?;
                    }
                    items.item_stack(&mut r, &mut w)?;
                }
                "smithing_trim" => {
                    for _ in 0..3 {
                        ingredient(&mut r, &mut w)?;
                    }
                }
                name if name.starts_with("crafting_special_") || name == "crafting_decorated_pot" => {
                    Items::copy_varint(&mut r, &mut w)?; // category
                }
                other => return Err(err(&format!("unknown recipe serializer {other}"))),
            }
        }
        Ok(w.into_inner())
    }

    // ---- serverbound -----------------------------------------------------------------------

    /// CONTAINER_CLICK: byte container, state, slot, button, mode, changed slots, carried item.
    pub fn container_click(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.u8(r.u8()?);
        Items::copy_varint(&mut r, &mut w)?;
        w.raw(r.bytes(3)?);
        Items::copy_varint(&mut r, &mut w)?;
        let n = Items::copy_varint(&mut r, &mut w)?;
        for _ in 0..n.clamp(0, 128) {
            w.raw(r.bytes(2)?);
            items.item_stack(&mut r, &mut w)?;
        }
        items.item_stack(&mut r, &mut w)?;
        Ok(w.into_inner())
    }

    pub fn set_creative_slot(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.i16(r.i16()?);
        items.item_stack(&mut r, &mut w)?;
        Ok(w.into_inner())
    }

    /// Serverbound movement with a rotation (yaw and pitch after `skip` bytes of position).
    pub fn track_rotation(&self, payload: &[u8], skip: usize) -> Result<()> {
        let mut r = Reader::new(payload);
        r.bytes(skip)?;
        self.yaw.set(r.f32()?);
        self.pitch.set(r.f32()?);
        Ok(())
    }

    /// PLAYER_POSITION (clientbound): an absolute rotation the client will take on.
    pub fn player_position(&self, payload: &[u8]) -> Result<()> {
        let mut r = Reader::new(payload);
        r.bytes(24)?;
        let (yaw, pitch) = (r.f32()?, r.f32()?);
        let relative = r.u8()?;
        self.yaw.set(if relative & 0x08 != 0 { self.yaw.get() + yaw } else { yaw });
        self.pitch.set(if relative & 0x10 != 0 { self.pitch.get() + pitch } else { pitch });
        Ok(())
    }

    /// USE_ITEM: 1.21 appends the player's rotation (used to aim and place).
    pub fn use_item(&self, payload: &[u8]) -> Vec<u8> {
        let mut w = Writer::with_capacity(payload.len() + 8);
        w.raw(payload);
        w.f32(self.yaw.get());
        w.f32(self.pitch.get());
        w.into_inner()
    }

    // ---- attributes, chat and events -------------------------------------------------------

    /// UPDATE_ATTRIBUTES: remaps attribute ids, drops attributes the client lacks and turns
    /// modifier ids into the UUIDs 1.20.5 uses.
    pub fn update_attributes(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.varint(r.varint()?);
        let n = r.varint()?;
        let mut body = Writer::new();
        let mut kept = 0;
        for _ in 0..n.max(0) {
            let attr = r.varint()?;
            let mut value = Writer::new();
            value.f64(r.f64()?);
            let m = Items::copy_varint(&mut r, &mut value)?;
            for _ in 0..m.max(0) {
                value.uuid(attribute_id_to_uuid(r.string()?));
                value.raw(r.bytes(8)?);
                value.u8(r.u8()?);
            }
            if Items::attribute_unknown(attr) {
                continue;
            }
            body.varint(items.attribute(attr));
            body.raw(&value.buf);
            kept += 1;
        }
        w.varint(kept);
        w.raw(&body.buf);
        Ok(w.into_inner())
    }

    /// GAME_EVENT: older clients skip the end poem for a "win game" value of 0, which servers
    /// send to players that saw it before; 1.21 clients always show it, so match that.
    pub fn game_event(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let event = r.u8()?;
        if event != 4 {
            return Ok(payload.to_vec());
        }
        let mut w = Writer::new();
        w.u8(event);
        w.f32(1.0);
        Ok(w.into_inner())
    }

    /// Packets that start with one text component (system chat, action bar, titles).
    pub fn leading_text(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        copy_text(&mut r, &mut w)?;
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    /// DISGUISED_CHAT: message, chat type, sender name, optional target name.
    pub fn disguised_chat(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        copy_text(&mut r, &mut w)?;
        chat_type(&mut r, &mut w)?;
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    pub fn player_chat(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.uuid(r.uuid()?);
        Items::copy_varint(&mut r, &mut w)?; // index
        if Items::copy_bool(&mut r, &mut w)? {
            w.raw(r.bytes(256)?);
        }
        w.string(r.string()?);
        w.raw(r.bytes(16)?); // timestamp, salt
        let n = Items::copy_varint(&mut r, &mut w)?;
        for _ in 0..n.max(0) {
            if Items::copy_varint(&mut r, &mut w)? == 0 {
                w.raw(r.bytes(256)?);
            }
        }
        if Items::copy_bool(&mut r, &mut w)? {
            copy_text(&mut r, &mut w)?; // unsigned content
        }
        if Items::copy_varint(&mut r, &mut w)? == 2 {
            w.long_array(&r.long_array()?);
        }
        chat_type(&mut r, &mut w)?;
        w.raw(r.remaining());
        Ok(w.into_inner())
    }
}

/// Chat type: a holder in 1.21, a plain registry id in 1.20.5. An inline chat type has no id;
/// it is shown with the first registered one (vanilla's plain chat).
fn chat_type(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let v = r.varint()?;
    if v > 0 {
        w.varint(v - 1);
        return Ok(());
    }
    for _ in 0..2 {
        // decoration: translation key, parameters, style
        r.string()?;
        let n = r.varint()?;
        for _ in 0..n.max(0) {
            r.varint()?;
        }
        nbt::skip(r)?;
    }
    w.varint(0);
    Ok(())
}

/// One text component with its item hovers made readable for 1.20.5.
fn copy_text(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let mut tag = nbt::read(r)?;
    fix_text(&mut tag);
    nbt::write(w, &tag);
    Ok(())
}

/// Item hovers in text: jukebox_playable is unknown to 1.20.5 and attribute modifiers need their
/// UUID and name back; modifiers of attributes 1.20.5 lacks are removed.
pub fn fix_text(tag: &mut Tag) {
    match tag {
        Tag::Compound(entries) => {
            let is_show_item = entries.iter().any(|(k, v)| k == "action" && v.as_str().map(strip_ns) == Some("show_item"));
            if is_show_item {
                for (k, v) in entries.iter_mut() {
                    if k == "contents" || k == "value" {
                        if let Some(components) = v.get_mut("components") {
                            fix_hover_components(components);
                        }
                    }
                }
            }
            for (_, v) in entries.iter_mut() {
                fix_text(v);
            }
        }
        Tag::List(list) => list.iter_mut().for_each(fix_text),
        _ => {}
    }
}

fn fix_hover_components(components: &mut Tag) {
    let Tag::Compound(entries) = components else { return };
    entries.retain(|(k, _)| strip_ns(k) != "jukebox_playable");
    let Some((_, modifiers)) = entries.iter_mut().find(|(k, _)| strip_ns(k) == "attribute_modifiers") else { return };
    // either a plain list or {modifiers: [...], show_in_tooltip}
    let list = match modifiers {
        Tag::List(list) => list,
        other => match other.get_mut("modifiers").and_then(Tag::as_list_mut) {
            Some(list) => list,
            None => return,
        },
    };
    list.retain(|m| m.get("type").and_then(Tag::as_str).is_none_or(|t| d::OLD_ATTRIBUTE_NAMES.contains(&strip_ns(t))));
    for m in list.iter_mut() {
        let Some(id) = m.get("id").and_then(Tag::as_str).map(str::to_string) else { continue };
        let uuid = attribute_id_to_uuid(&id);
        let ints = (0..4).map(|i| (uuid >> (96 - 32 * i)) as u32 as i32).collect();
        m.insert("uuid", Tag::IntArray(ints));
        m.insert("name", Tag::String(attribute_id_to_name(&id)));
    }
}

#[cfg(test)]
mod tests {
    use super::super::items::Dir;
    use super::*;

    #[test]
    fn chat_type_holder_becomes_registry_id() {
        let m = Misc::new();
        let mut p = Writer::new();
        nbt::write(&mut p, &Tag::String("hi".into()));
        p.varint(3); // registry id 2
        nbt::write(&mut p, &Tag::String("Steve".into()));
        p.bool(false);
        let out = m.disguised_chat(&p.buf).unwrap();
        let mut r = Reader::new(&out);
        nbt::skip(&mut r).unwrap();
        assert_eq!(r.varint().unwrap(), 2);
        assert_eq!(nbt::read(&mut r).unwrap(), Tag::String("Steve".into()));
        assert!(!r.bool().unwrap());
        assert!(r.is_empty());
        // inline chat type: skipped, shown as the first registered one
        let mut p = Writer::new();
        nbt::write(&mut p, &Tag::String("hi".into()));
        p.varint(0);
        for _ in 0..2 {
            p.string("chat.type.text").varint(2).varint(0).varint(2);
            nbt::write(&mut p, &Tag::compound());
        }
        nbt::write(&mut p, &Tag::String("Steve".into()));
        p.bool(false);
        let out = m.disguised_chat(&p.buf).unwrap();
        let mut r = Reader::new(&out);
        nbt::skip(&mut r).unwrap();
        assert_eq!(r.varint().unwrap(), 0);
        assert_eq!(nbt::read(&mut r).unwrap(), Tag::String("Steve".into()));
    }

    #[test]
    fn player_chat_keeps_layout() {
        let m = Misc::new();
        let mut p = Writer::new();
        p.uuid(9).varint(0).bool(false).string("hello").i64(1).i64(2);
        p.varint(1).varint(0).raw(&[7u8; 256]);
        p.bool(false);
        p.varint(0); // filter: pass through
        p.varint(1); // chat type id 0
        nbt::write(&mut p, &Tag::String("Alex".into()));
        p.bool(false);
        let out = m.player_chat(&p.buf).unwrap();
        let mut expected = p.buf.clone();
        let at = expected.len() - 1 - nbt_len(&Tag::String("Alex".into())) - 1;
        expected[at] = 0;
        assert_eq!(out, expected);
    }

    fn nbt_len(t: &Tag) -> usize {
        let mut w = Writer::new();
        nbt::write(&mut w, t);
        w.buf.len()
    }

    #[test]
    fn attribute_modifiers_get_uuids() {
        let items = Items::new(Dir::ToClient);
        let damage = d::NEW_ATTRIBUTE_NAMES.iter().position(|n| *n == "generic.attack_damage").unwrap() as i32;
        let oxygen = d::NEW_ATTRIBUTE_NAMES.iter().position(|n| *n == "generic.oxygen_bonus").unwrap() as i32;
        let mut p = Writer::new();
        p.varint(3).varint(2);
        p.varint(oxygen).f64(0.0).varint(0);
        p.varint(damage).f64(1.0).varint(1).string("minecraft:effect.strength").f64(3.0).u8(0);
        let out = Misc::new().update_attributes(&items, &p.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.varint().unwrap(), 3);
        assert_eq!(r.varint().unwrap(), 1, "oxygen_bonus is left out");
        assert_eq!(r.varint().unwrap(), d::OLD_ATTRIBUTE_NAMES.iter().position(|n| *n == "generic.attack_damage").unwrap() as i32);
        assert_eq!(r.f64().unwrap(), 1.0);
        assert_eq!(r.varint().unwrap(), 1);
        assert_eq!(r.uuid().unwrap(), attribute_id_to_uuid("effect.strength"));
        assert_eq!(r.f64().unwrap(), 3.0);
        assert_eq!(r.u8().unwrap(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn horse_columns_win_game_and_use_item() {
        let m = Misc::new();
        let mut p = Writer::new();
        p.u8(2).varint(5).i32(77);
        let out = m.horse_screen_open(&p.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!((r.u8().unwrap(), r.varint().unwrap(), r.i32().unwrap()), (2, 16, 77));
        let mut g = Writer::new();
        g.u8(4).f32(0.0);
        assert_eq!(m.game_event(&g.buf).unwrap(), vec![4, 0x3f, 0x80, 0, 0]);
        let mut mv = Writer::new();
        mv.f64(0.0).f64(0.0).f64(0.0).f32(90.0).f32(-10.0).bool(true);
        m.track_rotation(&mv.buf, 24).unwrap();
        let mut u = Writer::new();
        u.varint(0).varint(12);
        let out = m.use_item(&u.buf);
        let mut r = Reader::new(&out);
        assert_eq!((r.varint().unwrap(), r.varint().unwrap(), r.f32().unwrap(), r.f32().unwrap()), (0, 12, 90.0, -10.0));
    }

    #[test]
    fn enchanting_hints_follow_the_registry() {
        let mut items = Items::new(Dir::ToClient);
        items.set_enchantments(&["minecraft:efficiency".to_string()]);
        let m = Misc::new();
        let mut o = Writer::new();
        o.varint(1).varint(ENCHANTMENT_MENU);
        nbt::write(&mut o, &Tag::String("Enchant".into()));
        m.open_screen(&o.buf).unwrap();
        let mut p = Writer::new();
        p.u8(1).i16(4).i16(0);
        let out = m.container_set_data(&items, &p.buf).unwrap();
        assert_eq!(Reader::new(&out[3..]).i16().unwrap(), d::OLD_ENCHANTMENT_NAMES.iter().position(|n| *n == "efficiency").unwrap() as i16);
    }

    #[test]
    fn hover_components_are_fixed() {
        let mut modifier = Tag::compound();
        modifier.insert("type", Tag::String("minecraft:generic.armor".into()));
        modifier.insert("id", Tag::String("minecraft:armor.helmet".into()));
        let mut gone = Tag::compound();
        gone.insert("type", Tag::String("minecraft:generic.burning_time".into()));
        gone.insert("id", Tag::String("minecraft:x".into()));
        let mut components = Tag::compound();
        components.insert("minecraft:jukebox_playable", Tag::compound());
        components.insert("minecraft:attribute_modifiers", Tag::List(vec![modifier, gone]));
        let mut contents = Tag::compound();
        contents.insert("id", Tag::String("minecraft:diamond_helmet".into()));
        contents.insert("components", components);
        let mut hover = Tag::compound();
        hover.insert("action", Tag::String("show_item".into()));
        hover.insert("contents", contents);
        let mut text = Tag::compound();
        text.insert("hoverEvent", hover);
        fix_text(&mut text);
        let c = text.get("hoverEvent").unwrap().get("contents").unwrap().get("components").unwrap();
        assert!(c.get("minecraft:jukebox_playable").is_none());
        let list = c.get("minecraft:attribute_modifiers").unwrap().as_list().unwrap();
        assert_eq!(list.len(), 1);
        let Some(Tag::IntArray(uuid)) = list[0].get("uuid") else { panic!("no uuid") };
        let u = attribute_id_to_uuid("armor.helmet");
        assert_eq!(uuid[0], (u >> 96) as u32 as i32);
        assert_eq!(uuid[3], u as u32 as i32);
    }
}

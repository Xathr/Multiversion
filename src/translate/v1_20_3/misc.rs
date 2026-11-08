//! Everything else 1.20.5 → 1.20.3: particles, sounds, explosions, containers, merchants,
//! advancements, recipes, commands, statistics, maps and text.

use std::cell::Cell;

use super::data_1_20_3 as d;
use super::items::Items;
use super::text;
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt;

fn err(msg: &str) -> DecodeError {
    DecodeError::Other(msg.to_string())
}

/// The enchanting table menu id (same in both versions).
const ENCHANTMENT_MENU: i32 = 13;

// ---- particles ---------------------------------------------------------------------------------

/// A 1.20.5 particle read apart: the 1.20.3 id, its 1.20.3 data, and the entity effect color
/// (1.20.3 takes that from elsewhere: offsets, or an entity data value).
pub struct Particle {
    pub id: i32,
    pub data: Vec<u8>,
    pub color: Option<i32>,
}

pub fn read_particle(items: &Items, r: &mut Reader) -> Result<Particle> {
    let id = r.varint()?;
    let mut w = Writer::new();
    let mut color = None;
    match d::NEW_PARTICLE_NAMES.get(id.max(0) as usize).copied().unwrap_or("") {
        "block" | "block_marker" | "falling_dust" | "dust_pillar" => {
            w.varint(Items::block_state(r.varint()?));
        }
        "item" => items.to_client(r, &mut w)?,
        "dust" => {
            w.raw(r.bytes(16)?);
        }
        "dust_color_transition" => {
            // from, to, scale → from, scale, to
            let from = r.bytes(12)?;
            let to = r.bytes(12)?;
            let scale = r.bytes(4)?;
            w.raw(from).raw(scale).raw(to);
        }
        "entity_effect" => color = Some(r.i32()?),
        "vibration" => {
            let source = r.varint()?;
            w.varint(source);
            match source {
                0 => {
                    w.i64(r.i64()?);
                }
                1 => {
                    w.varint(r.varint()?);
                    w.raw(r.bytes(4)?);
                }
                _ => return Err(err("unknown vibration source")),
            }
            w.varint(r.varint()?);
        }
        "sculk_charge" => {
            w.raw(r.bytes(4)?);
        }
        "shriek" => {
            w.varint(r.varint()?);
        }
        _ => {}
    }
    let mapped = d::PARTICLES.get(id.max(0) as usize).map_or(id, |v| *v as i32);
    Ok(Particle { id: mapped, data: w.into_inner(), color })
}

/// A particle as 1.20.3 writes it inside other packets (id + data).
pub fn particle(items: &Items, r: &mut Reader, w: &mut Writer) -> Result<()> {
    let p = read_particle(items, r)?;
    w.varint(p.id);
    w.raw(&p.data);
    Ok(())
}

/// Sound event holder; `None` when 1.20.3 has no such sound.
fn sound_event(r: &mut Reader, w: &mut Writer) -> Result<Option<()>> {
    let v = r.varint()?;
    if v == 0 {
        w.varint(0);
        w.string(r.string()?);
        copy_opt_f32(r, w)?;
        return Ok(Some(()));
    }
    match Items::sound(v - 1) {
        Some(id) => {
            w.varint(id + 1);
            Ok(Some(()))
        }
        None => Ok(None),
    }
}

fn copy_opt_f32(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if present {
        w.raw(r.bytes(4)?);
    }
    Ok(())
}

pub struct Misc {
    menu: Cell<i32>,
}

impl Default for Misc {
    fn default() -> Self {
        Self::new()
    }
}

impl Misc {
    pub fn new() -> Self {
        Self { menu: Cell::new(-1) }
    }

    /// LEVEL_PARTICLES: 1.20.3 puts the particle id first and its data last. An entity effect
    /// color goes into the offsets (which 1.20.3 reads as the color when the count is 0).
    pub fn level_particles(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let long_distance = r.bool()?;
        let pos = r.bytes(24)?;
        let mut offsets = [r.f32()?, r.f32()?, r.f32()?];
        let mut speed = r.f32()?;
        let count = r.i32()?;
        let p = read_particle(items, &mut r)?;
        if let (Some(argb), 0) = (p.color, count) {
            offsets = [(argb >> 16 & 0xff) as f32 / 255.0, (argb >> 8 & 0xff) as f32 / 255.0, (argb & 0xff) as f32 / 255.0];
            speed = 1.0;
        }
        let mut w = Writer::with_capacity(payload.len());
        w.varint(p.id);
        w.bool(long_distance);
        w.raw(pos);
        for o in offsets {
            w.f32(o);
        }
        w.f32(speed);
        w.i32(count);
        w.raw(&p.data);
        Ok(w.into_inner())
    }

    /// SOUND / SOUND_ENTITY: sound event first; `None` for sounds 1.20.3 lacks.
    pub fn sound(&self, payload: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        if sound_event(&mut r, &mut w)?.is_none() {
            return Ok(None);
        }
        w.raw(r.remaining());
        Ok(Some(w.into_inner()))
    }

    /// EXPLODE: the sound is a holder in 1.20.5, a name and optional range in 1.20.3.
    pub fn explode(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.raw(r.bytes(28)?);
        let n = r.varint()?;
        w.varint(n);
        w.raw(r.bytes(n.max(0) as usize * 3)?);
        w.raw(r.bytes(12)?);
        w.varint(r.varint()?); // block interaction
        particle(items, &mut r, &mut w)?;
        particle(items, &mut r, &mut w)?;
        let v = r.varint()?;
        if v == 0 {
            w.string(r.string()?);
            copy_opt_f32(&mut r, &mut w)?;
        } else {
            let name = Items::sound(v - 1).and_then(|id| d::OLD_SOUND_NAMES.get(id as usize)).copied().unwrap_or("entity.generic.explode");
            w.string(&format!("minecraft:{name}"));
            w.bool(false);
        }
        Ok(w.into_inner())
    }

    // ---- containers ------------------------------------------------------------------------

    pub fn open_screen(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        r.varint()?;
        self.menu.set(r.varint()?);
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.varint(r.varint()?);
        w.varint(r.varint()?);
        let mut title = nbt::read(&mut r)?;
        text::fix_text(&mut title);
        nbt::write(&mut w, &title);
        Ok(w.into_inner())
    }

    pub fn container_close(&self) {
        self.menu.set(-1);
    }

    /// CONTAINER_SET_DATA: the enchanting table's hint properties (4-6) name enchantments.
    pub fn container_set_data(&self, payload: &[u8]) -> Result<Vec<u8>> {
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
        w.i16(Items::old_enchantment(i32::from(value)).map_or(-1, |v| v as i16));
        Ok(w.into_inner())
    }

    /// HORSE_SCREEN_OPEN: 1.20.3 counts the armor slot that 1.20.5 moved to the equipment.
    pub fn horse_screen_open(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.u8(r.u8()?);
        w.varint(r.varint()? + 1);
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    pub fn container_set_content(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.u8(r.u8()?);
        w.varint(r.varint()?); // state id
        let n = r.varint()?;
        w.varint(n);
        for _ in 0..n.max(0) {
            items.to_client(&mut r, &mut w)?;
        }
        items.to_client(&mut r, &mut w)?; // carried
        Ok(w.into_inner())
    }

    pub fn container_set_slot(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.u8(r.u8()?);
        w.varint(r.varint()?);
        w.i16(r.i16()?);
        items.to_client(&mut r, &mut w)?;
        Ok(w.into_inner())
    }

    pub fn merchant_offers(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.varint(r.varint()?);
        let n = r.varint()?;
        w.varint(n);
        for _ in 0..n.max(0) {
            items.item_cost(&mut r, &mut w)?;
            items.to_client(&mut r, &mut w)?;
            items.opt_item_cost(&mut r, &mut w)?;
            w.raw(r.bytes(1 + 4 + 4 + 4 + 4 + 4 + 4)?);
        }
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    /// COOLDOWN: item id + ticks.
    pub fn cooldown(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.varint(Items::item(r.varint()?));
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    /// AWARD_STATS: (stat type, stat id, value); stats name blocks, items or entities by id.
    pub fn award_stats(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let n = r.varint()?;
        let mut body = Writer::with_capacity(payload.len());
        let mut kept = 0;
        for _ in 0..n.max(0) {
            let category = r.varint()?;
            let id = r.varint()?;
            let value = r.varint()?;
            let mapped = match category {
                0 if Items::block_unknown(id) => continue,
                0 => Items::block(id),
                1..=5 => Items::item(id),
                6 | 7 => Items::entity(id),
                _ => id,
            };
            body.varint(category).varint(mapped).varint(value);
            kept += 1;
        }
        let mut w = Writer::new();
        w.varint(kept);
        w.raw(&body.buf);
        Ok(w.into_inner())
    }

    /// MAP_ITEM_DATA: the trial chambers marker is new; it is shown as the jungle temple one.
    pub fn map_item_data(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.varint(r.varint()?);
        w.raw(r.bytes(2)?); // scale, locked
        let has_icons = r.bool()?;
        w.bool(has_icons);
        if has_icons {
            let n = r.varint()?;
            w.varint(n);
            for _ in 0..n.max(0) {
                let ty = r.varint()?;
                w.varint(if ty == 34 { 32 } else { ty });
                w.raw(r.bytes(3)?);
                if r.bool()? {
                    w.bool(true);
                    let mut name = nbt::read(&mut r)?;
                    text::fix_text(&mut name);
                    nbt::write(&mut w, &name);
                } else {
                    w.bool(false);
                }
            }
        }
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    pub fn update_advancements(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.bool(r.bool()?);
        let n = r.varint()?;
        w.varint(n);
        for _ in 0..n.max(0) {
            w.string(r.string()?);
            if r.bool()? {
                w.bool(true);
                w.string(r.string()?);
            } else {
                w.bool(false);
            }
            let display = r.bool()?;
            w.bool(display);
            if display {
                for _ in 0..2 {
                    let mut t = nbt::read(&mut r)?;
                    text::fix_text(&mut t);
                    nbt::write(&mut w, &t);
                }
                items.to_client(&mut r, &mut w)?; // icon
                w.varint(r.varint()?); // frame
                let flags = r.i32()?;
                w.i32(flags);
                if flags & 1 != 0 {
                    w.string(r.string()?);
                }
                w.raw(r.bytes(8)?); // x, y
            }
            let reqs = r.varint()?;
            w.varint(reqs);
            for _ in 0..reqs.max(0) {
                let m = r.varint()?;
                w.varint(m);
                for _ in 0..m.max(0) {
                    w.string(r.string()?);
                }
            }
            w.bool(r.bool()?); // telemetry
        }
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    /// UPDATE_RECIPES: 1.20.3 names the serializer before the recipe id, and its items are
    /// legacy stacks.
    pub fn update_recipes(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        let ingredient = |r: &mut Reader, w: &mut Writer| -> Result<()> {
            let n = r.varint()?;
            w.varint(n);
            for _ in 0..n.max(0) {
                items.to_client(r, w)?;
            }
            Ok(())
        };
        let n = r.varint()?;
        w.varint(n);
        for _ in 0..n.max(0) {
            let id = r.string()?;
            let serializer = r.varint()?;
            let name = d::NEW_RECIPE_SERIALIZER_NAMES.get(serializer.max(0) as usize).copied().ok_or_else(|| err("unknown recipe serializer"))?;
            w.string(&format!("minecraft:{name}"));
            w.string(id);
            match name {
                "crafting_shaped" => {
                    w.string(r.string()?); // group
                    w.varint(r.varint()?); // category
                    let width = r.varint()?;
                    let height = r.varint()?;
                    w.varint(width).varint(height);
                    for _ in 0..(width.max(0) * height.max(0)) {
                        ingredient(&mut r, &mut w)?;
                    }
                    items.to_client(&mut r, &mut w)?;
                    w.bool(r.bool()?); // show notification
                }
                "crafting_shapeless" => {
                    w.string(r.string()?);
                    w.varint(r.varint()?);
                    let k = r.varint()?;
                    w.varint(k);
                    for _ in 0..k.max(0) {
                        ingredient(&mut r, &mut w)?;
                    }
                    items.to_client(&mut r, &mut w)?;
                }
                "smelting" | "blasting" | "smoking" | "campfire_cooking" => {
                    w.string(r.string()?);
                    w.varint(r.varint()?);
                    ingredient(&mut r, &mut w)?;
                    items.to_client(&mut r, &mut w)?;
                    w.raw(r.bytes(4)?); // experience
                    w.varint(r.varint()?); // cooking time
                }
                "stonecutting" => {
                    w.string(r.string()?);
                    ingredient(&mut r, &mut w)?;
                    items.to_client(&mut r, &mut w)?;
                }
                "smithing_transform" => {
                    for _ in 0..3 {
                        ingredient(&mut r, &mut w)?;
                    }
                    items.to_client(&mut r, &mut w)?;
                }
                "smithing_trim" => {
                    for _ in 0..3 {
                        ingredient(&mut r, &mut w)?;
                    }
                }
                name if name.starts_with("crafting_special_") || name == "crafting_decorated_pot" => {
                    w.varint(r.varint()?); // category
                }
                other => return Err(err(&format!("unknown recipe serializer {other}"))),
            }
        }
        Ok(w.into_inner())
    }

    // ---- commands --------------------------------------------------------------------------

    /// COMMANDS: remap parser ids; the loot table, predicate and modifier parsers are new and
    /// become single-word strings.
    pub fn commands(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        let n = r.varint()?;
        w.varint(n);
        let string_parser = d::OLD_ARGUMENT_TYPE_NAMES.iter().position(|n| *n == "brigadier:string").unwrap_or(5) as i32;
        for _ in 0..n.max(0) {
            let flags = r.u8()?;
            w.u8(flags);
            let children = r.list(|r| r.varint())?;
            w.varint(children.len() as i32);
            for c in children {
                w.varint(c);
            }
            if flags & 0x08 != 0 {
                w.varint(r.varint()?);
            }
            let node_type = flags & 0x03;
            if node_type == 1 || node_type == 2 {
                w.string(r.string()?);
            }
            if node_type == 2 {
                let parser = r.varint()?;
                let name = d::NEW_ARGUMENT_TYPE_NAMES.get(parser.max(0) as usize).copied().unwrap_or("");
                if matches!(name, "loot_table" | "loot_predicate" | "loot_modifier") {
                    w.varint(string_parser);
                    w.varint(0); // single word
                } else {
                    w.varint(d::ARGUMENT_TYPES.get(parser.max(0) as usize).map_or(string_parser, |m| *m as i32));
                    parser_properties(name, &mut r, &mut w)?;
                }
            }
            if flags & 0x10 != 0 {
                w.string(r.string()?);
            }
        }
        w.varint(r.varint()?); // root index
        Ok(w.into_inner())
    }

    // ---- serverbound -----------------------------------------------------------------------

    /// CONTAINER_CLICK: byte container, state, slot, button, mode, changed slots, carried item.
    pub fn container_click(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.u8(r.u8()?);
        w.varint(r.varint()?);
        w.raw(r.bytes(3)?);
        w.varint(r.varint()?);
        let n = r.varint()?;
        w.varint(n);
        for _ in 0..n.clamp(0, 128) {
            w.raw(r.bytes(2)?);
            items.to_server(&mut r, &mut w)?;
        }
        items.to_server(&mut r, &mut w)?;
        Ok(w.into_inner())
    }

    pub fn set_creative_slot(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        w.i16(r.i16()?);
        items.to_server(&mut r, &mut w)?;
        Ok(w.into_inner())
    }

    /// CONTAINER_BUTTON_CLICK: container and button are bytes in 1.20.3, varints in 1.20.5.
    pub fn container_button_click(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::new();
        w.varint(i32::from(r.u8()?));
        w.varint(i32::from(r.u8()?));
        Ok(w.into_inner())
    }

    // ---- text ------------------------------------------------------------------------------

    /// Packets that start with one text component (system chat, action bar, titles).
    pub fn leading_text(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        let mut t = nbt::read(&mut r)?;
        text::fix_text(&mut t);
        nbt::write(&mut w, &t);
        w.raw(r.remaining());
        Ok(w.into_inner())
    }
}

fn parser_properties(name: &str, r: &mut Reader, w: &mut Writer) -> Result<()> {
    match name {
        "brigadier:float" | "brigadier:integer" | "brigadier:double" | "brigadier:long" => {
            let size = if name == "brigadier:float" || name == "brigadier:integer" { 4 } else { 8 };
            let flags = r.u8()?;
            w.u8(flags);
            if flags & 1 != 0 {
                w.raw(r.bytes(size)?);
            }
            if flags & 2 != 0 {
                w.raw(r.bytes(size)?);
            }
        }
        "brigadier:string" => {
            w.varint(r.varint()?);
        }
        "entity" | "score_holder" => {
            w.u8(r.u8()?);
        }
        "time" => {
            w.raw(r.bytes(4)?);
        }
        "resource" | "resource_key" | "resource_or_tag" | "resource_or_tag_key" => {
            w.string(r.string()?);
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn particle_id(name: &str) -> i32 {
        d::NEW_PARTICLE_NAMES.iter().position(|n| *n == name).unwrap() as i32
    }

    #[test]
    fn particles_move_their_id_and_color() {
        let m = Misc::new();
        let items = Items::new();
        let mut p = Writer::new();
        p.bool(false).raw(&[0; 24]).f32(0.0).f32(0.0).f32(0.0).f32(0.5).i32(0);
        p.varint(particle_id("entity_effect")).i32(0xff20_40ff_u32 as i32);
        let out = m.level_particles(&items, &p.buf).unwrap();
        let mut r = Reader::new(&out);
        let old = d::OLD_PARTICLE_NAMES.iter().position(|n| *n == "entity_effect").unwrap() as i32;
        assert_eq!(r.varint().unwrap(), old);
        r.bool().unwrap();
        r.bytes(24).unwrap();
        assert_eq!(r.f32().unwrap(), 0x20 as f32 / 255.0);
        assert_eq!(r.f32().unwrap(), 0x40 as f32 / 255.0);
        assert_eq!(r.f32().unwrap(), 1.0);
        assert_eq!(r.f32().unwrap(), 1.0);
        assert_eq!(r.i32().unwrap(), 0);
        assert!(r.is_empty(), "entity_effect has no data in 1.20.3");

        // dust color transition reorders its floats
        let mut p = Writer::new();
        p.varint(particle_id("dust_color_transition"));
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
            p.f32(v);
        }
        let mut w = Writer::new();
        particle(&items, &mut Reader::new(&p.buf), &mut w).unwrap();
        let mut r = Reader::new(&w.buf);
        r.varint().unwrap();
        let v: Vec<f32> = (0..7).map(|_| r.f32().unwrap()).collect();
        assert_eq!(v, vec![1.0, 2.0, 3.0, 7.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn explosion_sound_becomes_a_name() {
        let m = Misc::new();
        let items = Items::new();
        let sound = d::NEW_SOUND_NAMES.iter().position(|n| *n == "entity.generic.explode").unwrap() as i32;
        let mut p = Writer::new();
        p.raw(&[0; 28]).varint(0).raw(&[0; 12]).varint(1);
        p.varint(particle_id("explosion")).varint(particle_id("explosion_emitter"));
        p.varint(sound + 1);
        let out = m.explode(&items, &p.buf).unwrap();
        let mut r = Reader::new(&out);
        r.bytes(28 + 1 + 12 + 1 + 2).unwrap();
        assert_eq!(r.string().unwrap(), "minecraft:entity.generic.explode");
        assert!(!r.bool().unwrap());
        assert!(r.is_empty());
    }

    #[test]
    fn recipes_name_the_serializer_first() {
        let m = Misc::new();
        let items = Items::new();
        let stone = d::NEW_ITEM_NAMES.iter().position(|n| *n == "stone").unwrap() as i32;
        let cutting = d::NEW_RECIPE_SERIALIZER_NAMES.iter().position(|n| *n == "stonecutting").unwrap() as i32;
        let mut p = Writer::new();
        p.varint(1).string("minecraft:stone_slab").varint(cutting).string("");
        p.varint(1).varint(1).varint(stone).varint(0).varint(0); // ingredient
        p.varint(2).varint(stone).varint(0).varint(0); // result
        let out = m.update_recipes(&items, &p.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.varint().unwrap(), 1);
        assert_eq!(r.string().unwrap(), "minecraft:stonecutting");
        assert_eq!(r.string().unwrap(), "minecraft:stone_slab");
        r.string().unwrap();
        assert_eq!(r.varint().unwrap(), 1);
        assert!(r.bool().unwrap());
        let old_stone = d::OLD_ITEM_NAMES.iter().position(|n| *n == "stone").unwrap() as i32;
        assert_eq!(r.varint().unwrap(), old_stone);
        assert_eq!(r.i8().unwrap(), 1);
        assert_eq!(r.u8().unwrap(), 0, "no NBT");
        assert!(r.bool().unwrap());
        r.varint().unwrap();
        assert_eq!(r.i8().unwrap(), 2);
    }

    #[test]
    fn loot_parsers_become_strings() {
        let m = Misc::new();
        let loot = d::NEW_ARGUMENT_TYPE_NAMES.iter().position(|n| *n == "loot_table").unwrap() as i32;
        let mut p = Writer::new();
        p.varint(2);
        p.u8(0).varint(1).varint(1); // root
        p.u8(2).varint(0).string("table").varint(loot);
        p.varint(0);
        let out = m.commands(&p.buf).unwrap();
        let mut r = Reader::new(&out);
        r.varint().unwrap();
        r.u8().unwrap();
        r.list(|r| r.varint()).unwrap();
        r.u8().unwrap();
        r.list(|r| r.varint()).unwrap();
        r.string().unwrap();
        assert_eq!(d::OLD_ARGUMENT_TYPE_NAMES[r.varint().unwrap() as usize], "brigadier:string");
        assert_eq!(r.varint().unwrap(), 0);
        assert_eq!(r.varint().unwrap(), 0);
        assert!(r.is_empty());
    }
}

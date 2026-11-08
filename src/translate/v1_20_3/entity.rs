//! Entities 1.20.5 → 1.20.3: spawning, entity data, equipment, attributes, mob effects, and the
//! login / respawn packets (which name the dimension type differently).
//!
//! Entity data changes: three value types are new in 1.20.5 (particle lists, wolf variants,
//! armadillo states); living entities show their effect color as particles instead of a color;
//! area effect clouds lost their color index and llamas their carpet index (the carpet is body
//! equipment now); 1.20.5-only mobs are shown as similar 1.20.3 ones and lose the indexes those
//! lack.

use std::collections::HashMap;

use super::data_1_20_3 as d;
use super::ids::old;
use super::items::{Items, DYE_COLORS, OLD_EFFECT_COUNT};
use super::misc::read_particle;
use super::world::Registries;
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

fn err(msg: &str) -> DecodeError {
    DecodeError::Other(msg.to_string())
}

/// Entity data value types, 1.20.5 ids.
mod ty {
    pub const BYTE: i32 = 0;
    pub const VARINT: i32 = 1;
    pub const VARLONG: i32 = 2;
    pub const FLOAT: i32 = 3;
    pub const STRING: i32 = 4;
    pub const COMPONENT: i32 = 5;
    pub const OPT_COMPONENT: i32 = 6;
    pub const ITEM: i32 = 7;
    pub const BOOL: i32 = 8;
    pub const ROTATIONS: i32 = 9;
    pub const POSITION: i32 = 10;
    pub const OPT_POSITION: i32 = 11;
    pub const DIRECTION: i32 = 12;
    pub const OPT_UUID: i32 = 13;
    pub const BLOCK_STATE: i32 = 14;
    pub const OPT_BLOCK_STATE: i32 = 15;
    pub const COMPOUND: i32 = 16;
    pub const PARTICLE: i32 = 17;
    pub const PARTICLES: i32 = 18;
    pub const VILLAGER_DATA: i32 = 19;
    pub const OPT_VARINT: i32 = 20;
    pub const POSE: i32 = 21;
    pub const CAT_VARIANT: i32 = 22;
    pub const WOLF_VARIANT: i32 = 23;
    pub const FROG_VARIANT: i32 = 24;
    pub const OPT_GLOBAL_POSITION: i32 = 25;
    pub const PAINTING_VARIANT: i32 = 26;
    pub const SNIFFER_STATE: i32 = 27;
    pub const ARMADILLO_STATE: i32 = 28;
    pub const VECTOR3F: i32 = 29;
    pub const QUATERNION: i32 = 30;
}

/// 1.20.3 value type id of a 1.20.5 one (particle lists are handled before).
fn old_type(t: i32) -> i32 {
    match t {
        0..=17 => t,
        19..=22 => t - 1,
        24..=27 => t - 2,
        _ => t - 3,
    }
}

/// Entity families whose data layout changed (1.20.5 entity type names).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Other,
    Llama,
    AreaEffectCloud,
    Minecart,
    /// Shown as another entity: indexes from this one on do not exist there.
    StandIn(u8),
    Horse,
}

fn kind_of(ty: i32) -> Kind {
    match d::NEW_ENTITY_NAMES.get(ty.max(0) as usize).copied().unwrap_or("") {
        "llama" | "trader_llama" => Kind::Llama,
        "area_effect_cloud" => Kind::AreaEffectCloud,
        "minecart" | "chest_minecart" | "command_block_minecart" | "furnace_minecart" | "hopper_minecart" | "spawner_minecart" | "tnt_minecart" => Kind::Minecart,
        // bogged → stray (no sheared flag), armadillo → cow (no state), ominous item spawner →
        // text display (no item)
        "bogged" => Kind::StandIn(16),
        "armadillo" => Kind::StandIn(17),
        "ominous_item_spawner" => Kind::StandIn(8),
        "horse" | "donkey" | "mule" | "skeleton_horse" | "zombie_horse" | "camel" => Kind::Horse,
        _ => Kind::Other,
    }
}

/// Mobs shown as another mob get their own name, so players can tell them apart.
fn stand_in_name(ty: i32) -> Option<&'static str> {
    match d::NEW_ENTITY_NAMES.get(ty.max(0) as usize).copied()? {
        "armadillo" => Some("Armadillo"),
        "bogged" => Some("Bogged"),
        _ => None,
    }
}

#[derive(Default)]
pub struct Entities {
    /// entity id → 1.20.5 entity type id
    types: HashMap<i32, i32>,
    /// From LOGIN; 1.20.3 reads it from SERVER_DATA.
    pub enforces_secure_chat: bool,
}

impl Entities {
    pub fn clear(&mut self) {
        self.types.clear();
    }

    fn kind(&self, entity: i32) -> Kind {
        self.types.get(&entity).map_or(Kind::Other, |t| kind_of(*t))
    }

    /// ADD_ENTITY: remaps the type (and a falling block's state).
    pub fn add_entity(&mut self, payload: &[u8], out: &mut Vec<(i32, Vec<u8>)>) -> Result<()> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        let entity_id = r.varint()?;
        w.varint(entity_id);
        w.uuid(r.uuid()?);
        let ty = r.varint()?;
        self.types.insert(entity_id, ty);
        w.varint(Items::entity(ty));
        w.raw(r.bytes(24 + 3)?); // x y z, pitch yaw head yaw
        let data = r.varint()?;
        let falling = d::NEW_ENTITY_NAMES.get(ty.max(0) as usize) == Some(&"falling_block");
        w.varint(if falling { Items::block_state(data) } else { data });
        w.raw(r.remaining()); // velocity
        out.push((super::ids::play_clientbound(super::ids::new::ADD_ENTITY).unwrap_or(1), w.into_inner()));
        if let Some(name) = stand_in_name(ty) {
            let mut m = Writer::new();
            m.varint(entity_id);
            m.u8(2); // custom name
            m.varint(ty::OPT_COMPONENT);
            m.bool(true);
            nbt::write(&mut m, &Tag::String(name.into()));
            m.u8(0xff);
            out.push((old::SET_ENTITY_DATA, m.into_inner()));
        }
        Ok(())
    }

    pub fn remove_entities(&mut self, payload: &[u8]) -> Result<()> {
        let mut r = Reader::new(payload);
        for id in r.list(|r| r.varint())? {
            self.types.remove(&id);
        }
        Ok(())
    }

    /// SET_ENTITY_DATA: converts values and applies the per-type index changes.
    pub fn set_entity_data(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        let entity_id = r.varint()?;
        w.varint(entity_id);
        let kind = self.kind(entity_id);
        loop {
            let index = r.u8()?;
            if index == 0xff {
                break;
            }
            let t = r.varint()?;
            let mut value = Writer::new();
            let Some((t, color)) = entity_data_value(items, t, &mut r, &mut value)? else { continue };
            let index = match kind {
                Kind::StandIn(limit) if index >= limit => continue,
                Kind::Llama if index >= 20 => index + 1,
                Kind::AreaEffectCloud if index >= 9 => index + 1,
                _ => index,
            };
            let mut value = value.into_inner();
            if kind == Kind::Minecart && index == 11 && t == ty::VARINT {
                // the display block is a block state in a plain int
                let state = Reader::new(&value).varint()?;
                let mut v = Writer::new();
                v.varint(Items::block_state(state));
                value = v.into_inner();
            }
            if kind == Kind::AreaEffectCloud && index == 11 {
                if let Some(argb) = color {
                    // 1.20.3 keeps the effect color in its own index
                    w.u8(9);
                    w.varint(ty::VARINT);
                    w.varint(argb & 0xff_ffff);
                }
            }
            w.u8(index);
            w.varint(old_type(t));
            w.raw(&value);
        }
        w.u8(0xff);
        Ok(w.into_inner())
    }

    /// SET_EQUIPMENT: the body slot (6) is the chest slot (4) on 1.20.3; a llama's carpet is
    /// entity data there.
    pub fn set_equipment(&self, items: &Items, payload: &[u8], out: &mut Vec<(i32, Vec<u8>)>) -> Result<()> {
        let mut r = Reader::new(payload);
        let entity_id = r.varint()?;
        let llama = self.kind(entity_id) == Kind::Llama;
        let mut slots: Vec<(i8, Vec<u8>)> = Vec::new();
        loop {
            let raw = r.i8()?;
            let start = r.position();
            let mut item = Writer::new();
            items.to_client(&mut r, &mut item)?;
            let slot = raw & 0x7f;
            if slot == 6 && llama {
                let mut sub = Reader::new(&payload[start..r.position()]);
                let color = if sub.varint()? > 0 {
                    let id = sub.varint()?;
                    let name = d::NEW_ITEM_NAMES.get(id.max(0) as usize).copied().unwrap_or("");
                    name.strip_suffix("_carpet").and_then(|c| DYE_COLORS.iter().position(|x| *x == c)).map_or(-1, |c| c as i32)
                } else {
                    -1
                };
                let mut m = Writer::new();
                m.varint(entity_id).u8(20).varint(ty::VARINT).varint(color).u8(0xff);
                out.push((old::SET_ENTITY_DATA, m.into_inner()));
            } else {
                slots.push((if slot == 6 { 4 } else { slot }, item.into_inner()));
            }
            if raw >= 0 {
                break;
            }
        }
        if slots.is_empty() {
            return Ok(());
        }
        let mut w = Writer::with_capacity(payload.len());
        w.varint(entity_id);
        let last = slots.len() - 1;
        for (i, (slot, item)) in slots.into_iter().enumerate() {
            w.i8(if i == last { slot } else { slot | -0x80 });
            w.raw(&item);
        }
        out.push((old::SET_EQUIPMENT, w.into_inner()));
        Ok(())
    }

    /// UPDATE_ATTRIBUTES: attributes are named by key in 1.20.3; those it lacks are left out
    /// (jump strength only exists for horses there).
    pub fn update_attributes(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        let entity_id = r.varint()?;
        w.varint(entity_id);
        let horse = matches!(self.kind(entity_id), Kind::Horse | Kind::Llama);
        let n = r.varint()?;
        let mut body = Writer::new();
        let mut kept = 0;
        for _ in 0..n.max(0) {
            let attr = r.varint()?;
            let start = r.position();
            r.f64()?;
            let m = r.varint()?;
            r.bytes(m.max(0) as usize * (16 + 8 + 1))?;
            let Some(key) = Items::old_attribute_key(attr, horse) else { continue };
            body.string(&key);
            body.raw(&payload[start..r.position()]);
            kept += 1;
        }
        w.varint(kept);
        w.raw(&body.buf);
        Ok(w.into_inner())
    }

    /// UPDATE_MOB_EFFECT: the amplifier is a byte in 1.20.3, which also expects darkness's fade
    /// data; `None` for effects 1.20.3 lacks.
    pub fn update_mob_effect(&self, payload: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len() + 8);
        w.varint(r.varint()?);
        let effect = r.varint()?;
        if !(0..OLD_EFFECT_COUNT).contains(&effect) {
            return Ok(None);
        }
        w.varint(effect);
        w.i8(r.varint()?.clamp(-128, 127) as i8);
        w.varint(r.varint()?); // duration
        w.u8(r.u8()?); // flags
        if effect == 32 {
            // darkness: a steady fade
            let mut f = Tag::compound();
            f.insert("padding_duration", Tag::Int(22));
            f.insert("had_effect_last_tick", Tag::Byte(1));
            f.insert("factor_previous_frame", Tag::Float(0.0));
            f.insert("factor_start", Tag::Float(1.0));
            f.insert("factor_target", Tag::Float(1.0));
            f.insert("factor_current", Tag::Float(1.0));
            w.bool(true);
            nbt::write(&mut w, &f);
        } else {
            w.bool(false);
        }
        Ok(Some(w.into_inner()))
    }

    /// REMOVE_MOB_EFFECT; `None` for effects 1.20.3 lacks.
    pub fn remove_mob_effect(payload: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut r = Reader::new(payload);
        r.varint()?;
        let effect = r.varint()?;
        Ok((0..OLD_EFFECT_COUNT).contains(&effect).then(|| payload.to_vec()))
    }

    /// LOGIN: the dimension type is a key in 1.20.3, and "enforces secure chat" moves to
    /// SERVER_DATA.
    pub fn login(&mut self, regs: &Registries, payload: &[u8]) -> Result<Vec<u8>> {
        self.clear();
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len() + 16);
        w.i32(r.i32()?); // entity id
        w.bool(r.bool()?); // hardcore
        let worlds = r.list(|r| r.string().map(str::to_string))?;
        w.varint(worlds.len() as i32);
        for name in &worlds {
            w.string(name);
        }
        for _ in 0..3 {
            w.varint(r.varint()?); // max players, view distance, simulation distance
        }
        w.raw(r.bytes(3)?); // reduced debug info, death screen, limited crafting
        w.string(&regs.dimension_type(r.varint()?));
        w.string(r.string()?); // dimension
        w.i64(r.i64()?); // seed
        w.raw(r.bytes(4)?); // game mode, previous, debug, flat
        if r.bool()? {
            w.bool(true);
            w.string(r.string()?);
            w.i64(r.i64()?);
        } else {
            w.bool(false);
        }
        w.varint(r.varint()?); // portal cooldown
        self.enforces_secure_chat = r.bool()?;
        Ok(w.into_inner())
    }

    pub fn respawn(&mut self, regs: &Registries, payload: &[u8]) -> Result<Vec<u8>> {
        self.clear();
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len() + 16);
        w.string(&regs.dimension_type(r.varint()?));
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    /// SERVER_DATA: 1.20.3 adds "enforces secure chat" (from LOGIN in 1.20.5).
    pub fn server_data(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len() + 1);
        let mut motd = nbt::read(&mut r)?;
        super::text::fix_text(&mut motd);
        nbt::write(&mut w, &motd);
        if r.bool()? {
            w.bool(true);
            w.byte_array(r.byte_array()?);
        } else {
            w.bool(false);
        }
        w.bool(self.enforces_secure_chat);
        Ok(w.into_inner())
    }
}

/// One entity data value. Returns the (1.20.5) type it is written as and, for particles, an
/// entity effect color; `None` when 1.20.3 has no such value.
fn entity_data_value(items: &Items, t: i32, r: &mut Reader, w: &mut Writer) -> Result<Option<(i32, Option<i32>)>> {
    let mut color = None;
    let mut out_type = t;
    match t {
        ty::BYTE => {
            w.u8(r.u8()?);
        }
        ty::VARINT | ty::DIRECTION | ty::OPT_VARINT | ty::POSE | ty::CAT_VARIANT | ty::FROG_VARIANT | ty::SNIFFER_STATE | ty::PAINTING_VARIANT => {
            w.varint(r.varint()?);
        }
        ty::BLOCK_STATE => {
            w.varint(Items::block_state(r.varint()?));
        }
        ty::OPT_BLOCK_STATE => {
            let v = r.varint()?;
            w.varint(if v == 0 { 0 } else { Items::block_state(v) });
        }
        ty::VARLONG => {
            w.varlong(r.varlong()?);
        }
        ty::FLOAT => {
            w.raw(r.bytes(4)?);
        }
        ty::STRING => {
            w.string(r.string()?);
        }
        ty::COMPONENT => {
            let mut t = nbt::read(r)?;
            super::text::fix_text(&mut t);
            nbt::write(w, &t);
        }
        ty::OPT_COMPONENT => {
            if r.bool()? {
                let mut t = nbt::read(r)?;
                super::text::fix_text(&mut t);
                w.bool(true);
                nbt::write(w, &t);
            } else {
                w.bool(false);
            }
        }
        ty::COMPOUND => {
            let t = nbt::read(r)?;
            nbt::write(w, &t);
        }
        ty::ITEM => items.to_client(r, w)?,
        ty::BOOL => {
            w.bool(r.bool()?);
        }
        ty::ROTATIONS | ty::VECTOR3F => {
            w.raw(r.bytes(12)?);
        }
        ty::QUATERNION => {
            w.raw(r.bytes(16)?);
        }
        ty::POSITION => {
            w.i64(r.i64()?);
        }
        ty::OPT_POSITION => {
            if r.bool()? {
                w.bool(true);
                w.i64(r.i64()?);
            } else {
                w.bool(false);
            }
        }
        ty::OPT_UUID => {
            if r.bool()? {
                w.bool(true);
                w.uuid(r.uuid()?);
            } else {
                w.bool(false);
            }
        }
        ty::PARTICLE => {
            let p = read_particle(items, r)?;
            color = p.color;
            w.varint(p.id);
            w.raw(&p.data);
        }
        ty::PARTICLES => {
            // a living entity's effect particles: 1.20.3 shows one effect color instead
            let n = r.varint()?;
            let mut rgb = 0;
            for _ in 0..n.max(0) {
                if let Some(c) = read_particle(items, r)?.color {
                    rgb = c & 0xff_ffff;
                }
            }
            w.varint(rgb);
            out_type = ty::VARINT;
        }
        ty::VILLAGER_DATA => {
            for _ in 0..3 {
                w.varint(r.varint()?);
            }
        }
        ty::OPT_GLOBAL_POSITION => {
            if r.bool()? {
                w.bool(true);
                w.string(r.string()?);
                w.i64(r.i64()?);
            } else {
                w.bool(false);
            }
        }
        ty::WOLF_VARIANT | ty::ARMADILLO_STATE => {
            r.varint()?;
            return Ok(None);
        }
        other => return Err(err(&format!("unknown entity data type {other}"))),
    }
    Ok(Some((out_type, color)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity_type(name: &str) -> i32 {
        d::NEW_ENTITY_NAMES.iter().position(|n| *n == name).unwrap() as i32
    }

    fn spawn(e: &mut Entities, id: i32, ty: i32) -> Vec<(i32, Vec<u8>)> {
        let mut p = Writer::new();
        p.varint(id).uuid(1).varint(ty).raw(&[0; 27]).varint(0).raw(&[0; 6]);
        let mut out = Vec::new();
        e.add_entity(&p.buf, &mut out).unwrap();
        out
    }

    #[test]
    fn llama_cloud_and_living_entity_indexes() {
        let items = Items::new();
        let mut e = Entities::default();
        spawn(&mut e, 1, entity_type("llama"));
        spawn(&mut e, 2, entity_type("area_effect_cloud"));
        spawn(&mut e, 3, entity_type("zombie"));
        // llama: variant moves from 20 to 21
        let mut p = Writer::new();
        p.varint(1).u8(20).varint(ty::VARINT).varint(2).u8(0xff);
        let out = e.set_entity_data(&items, &p.buf).unwrap();
        let mut x = Writer::new();
        x.varint(1).u8(21).varint(ty::VARINT).varint(2).u8(0xff);
        assert_eq!(out, x.buf);
        // cloud: entity_effect particle at 10 → color at 9, particle at 11
        let effect = d::NEW_PARTICLE_NAMES.iter().position(|n| *n == "entity_effect").unwrap() as i32;
        let mut p = Writer::new();
        p.varint(2).u8(10).varint(ty::PARTICLE).varint(effect).i32(0xff11_2233_u32 as i32).u8(0xff);
        let out = e.set_entity_data(&items, &p.buf).unwrap();
        let old_effect = d::OLD_PARTICLE_NAMES.iter().position(|n| *n == "entity_effect").unwrap() as i32;
        let mut x = Writer::new();
        x.varint(2).u8(9).varint(ty::VARINT).varint(0x11_2233).u8(11).varint(ty::PARTICLE).varint(old_effect).u8(0xff);
        assert_eq!(out, x.buf);
        // living entity: effect particles → color; later types shift down
        let mut p = Writer::new();
        p.varint(3).u8(10).varint(ty::PARTICLES).varint(1).varint(effect).i32(0xff00_ff00_u32 as i32);
        p.u8(6).varint(ty::POSE).varint(1);
        p.u8(17).varint(ty::WOLF_VARIANT).varint(1);
        p.u8(0xff);
        let out = e.set_entity_data(&items, &p.buf).unwrap();
        let mut x = Writer::new();
        x.varint(3).u8(10).varint(ty::VARINT).varint(0x00_ff00).u8(6).varint(20).varint(1).u8(0xff);
        assert_eq!(out, x.buf);
    }

    #[test]
    fn stand_ins_lose_their_own_indexes() {
        let items = Items::new();
        let mut e = Entities::default();
        let out = spawn(&mut e, 4, entity_type("bogged"));
        assert_eq!(out.len(), 2, "bogged is named");
        let mut r = Reader::new(&out[0].1);
        r.varint().unwrap();
        r.uuid().unwrap();
        assert_eq!(r.varint().unwrap() as u32, d::ENTITIES[entity_type("bogged") as usize]);
        let mut p = Writer::new();
        p.varint(4).u8(15).varint(ty::BYTE).u8(0).u8(16).varint(ty::BOOL).bool(true).u8(0xff);
        let out = e.set_entity_data(&items, &p.buf).unwrap();
        let mut x = Writer::new();
        x.varint(4).u8(15).varint(ty::BYTE).u8(0).u8(0xff);
        assert_eq!(out, x.buf);
    }

    #[test]
    fn llama_carpet_and_horse_armor() {
        let items = Items::new();
        let mut e = Entities::default();
        spawn(&mut e, 5, entity_type("llama"));
        spawn(&mut e, 6, entity_type("horse"));
        let carpet = d::NEW_ITEM_NAMES.iter().position(|n| *n == "red_carpet").unwrap() as i32;
        let mut p = Writer::new();
        p.varint(5).i8(6).varint(1).varint(carpet).varint(0).varint(0);
        let mut out = Vec::new();
        e.set_equipment(&items, &p.buf, &mut out).unwrap();
        let mut x = Writer::new();
        x.varint(5).u8(20).varint(ty::VARINT).varint(14).u8(0xff);
        assert_eq!(out, vec![(old::SET_ENTITY_DATA, x.buf)]);
        let armor = d::NEW_ITEM_NAMES.iter().position(|n| *n == "iron_horse_armor").unwrap() as i32;
        let mut p = Writer::new();
        p.varint(6).i8(-0x80).varint(0).i8(6).varint(1).varint(armor).varint(0).varint(0);
        let mut out = Vec::new();
        e.set_equipment(&items, &p.buf, &mut out).unwrap();
        let mut r = Reader::new(&out[0].1);
        r.varint().unwrap();
        assert_eq!(r.i8().unwrap(), -0x80);
        assert!(!r.bool().unwrap());
        assert_eq!(r.i8().unwrap(), 4, "body armor is the chest slot");
    }

    #[test]
    fn attributes_mob_effects_and_login() {
        let mut e = Entities::default();
        spawn(&mut e, 7, entity_type("horse"));
        let jump = d::NEW_ATTRIBUTE_NAMES.iter().position(|n| *n == "generic.jump_strength").unwrap() as i32;
        let scale = d::NEW_ATTRIBUTE_NAMES.iter().position(|n| *n == "generic.scale").unwrap() as i32;
        let speed = d::NEW_ATTRIBUTE_NAMES.iter().position(|n| *n == "generic.movement_speed").unwrap() as i32;
        let mut p = Writer::new();
        p.varint(7).varint(3);
        p.varint(jump).f64(0.7).varint(0);
        p.varint(scale).f64(1.0).varint(0);
        p.varint(speed).f64(0.2).varint(1).uuid(9).f64(0.1).u8(2);
        let out = e.update_attributes(&p.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.varint().unwrap(), 7);
        assert_eq!(r.varint().unwrap(), 2);
        assert_eq!(r.string().unwrap(), "minecraft:horse.jump_strength");
        r.f64().unwrap();
        r.varint().unwrap();
        assert_eq!(r.string().unwrap(), "minecraft:generic.movement_speed");
        // players have no jump strength in 1.20.3
        let mut p = Writer::new();
        p.varint(99).varint(1).varint(jump).f64(0.42).varint(0);
        assert_eq!(e.update_attributes(&p.buf).unwrap(), vec![99, 0]);

        let mut m = Writer::new();
        m.varint(1).varint(32).varint(0).varint(100).u8(0);
        let out = e.update_mob_effect(&m.buf).unwrap().unwrap();
        let mut r = Reader::new(&out);
        assert_eq!((r.varint().unwrap(), r.varint().unwrap(), r.i8().unwrap(), r.varint().unwrap(), r.u8().unwrap()), (1, 32, 0, 100, 0));
        assert!(r.bool().unwrap(), "darkness has fade data");
        let mut m = Writer::new();
        m.varint(1).varint(35).varint(0).varint(100).u8(0); // wind charged
        assert!(e.update_mob_effect(&m.buf).unwrap().is_none());

        let mut regs = Registries::default();
        let mut reg = Writer::new();
        reg.string("minecraft:dimension_type").varint(2).string("minecraft:overworld").bool(false).string("minecraft:the_end").bool(false);
        regs.registry_data(&reg.buf).unwrap();
        let mut l = Writer::new();
        l.i32(5).bool(false).varint(1).string("minecraft:the_end").varint(20).varint(10).varint(10).raw(&[0, 1, 0]);
        l.varint(1).string("minecraft:the_end").i64(42).raw(&[1, 255, 0, 0]).bool(false).varint(0).bool(true);
        let out = e.login(&regs, &l.buf).unwrap();
        let mut r = Reader::new(&out);
        r.bytes(5).unwrap();
        r.list(|r| r.string().map(|_| ())).unwrap();
        r.bytes(6).unwrap();
        assert_eq!(r.string().unwrap(), "minecraft:the_end", "dimension type key");
        assert_eq!(r.string().unwrap(), "minecraft:the_end");
        r.bytes(8 + 4 + 1 + 1).unwrap();
        assert!(r.is_empty(), "secure chat flag moved out");
        assert!(e.enforces_secure_chat);
        let mut s = Writer::new();
        nbt::write(&mut s, &Tag::String("motd".into()));
        s.bool(false);
        let out = e.server_data(&s.buf).unwrap();
        assert_eq!(*out.last().unwrap(), 1);
    }
}

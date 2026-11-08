//! Entities 1.21 → 1.20.5. Entity types and data indexes are identical; only some values changed:
//! wolf and painting variants are holders in 1.21 (plain registry ids in 1.20.5), and item values
//! carry the converted item stack.

use super::items::{Items, Reg};
use super::misc::particle;
use super::world::Registries;
use crate::proto::buf::{DecodeError, Reader, Result, Writer};

fn err(msg: &str) -> DecodeError {
    DecodeError::Other(msg.to_string())
}

/// Entity data value types (identical ids in 1.20.5 and 1.21).
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

/// SET_ENTITY_DATA: converts each value; entries the old client cannot represent are left out.
pub fn set_entity_data(items: &Items, regs: &Registries, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.varint(r.varint()?);
    loop {
        let index = r.u8()?;
        if index == 0xff {
            w.u8(0xff);
            break;
        }
        let t = r.varint()?;
        let mut value = Writer::new();
        if entity_data_value(items, regs, t, &mut r, &mut value)? {
            w.u8(index);
            w.varint(t);
            w.raw(&value.buf);
        }
    }
    Ok(w.into_inner())
}

/// One entity data value; returns false when it has to be left out.
fn entity_data_value(items: &Items, regs: &Registries, t: i32, r: &mut Reader, w: &mut Writer) -> Result<bool> {
    match t {
        ty::BYTE => {
            w.u8(r.u8()?);
        }
        ty::VARINT | ty::DIRECTION | ty::OPT_VARINT | ty::POSE | ty::CAT_VARIANT | ty::FROG_VARIANT | ty::SNIFFER_STATE | ty::ARMADILLO_STATE
        | ty::BLOCK_STATE | ty::OPT_BLOCK_STATE => {
            w.varint(r.varint()?);
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
        ty::COMPONENT | ty::COMPOUND => Items::copy_nbt(r, w)?,
        ty::OPT_COMPONENT => Items::copy_opt_nbt(r, w)?,
        ty::ITEM => items.item_stack(r, w)?,
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
            if Items::copy_bool(r, w)? {
                w.i64(r.i64()?);
            }
        }
        ty::OPT_UUID => {
            if Items::copy_bool(r, w)? {
                w.uuid(r.uuid()?);
            }
        }
        ty::PARTICLE => particle(items, r, w)?,
        ty::PARTICLES => {
            let n = Items::copy_varint(r, w)?;
            for _ in 0..n.max(0) {
                particle(items, r, w)?;
            }
        }
        ty::VILLAGER_DATA => {
            for _ in 0..3 {
                w.varint(r.varint()?);
            }
        }
        ty::WOLF_VARIANT => {
            let v = r.varint()?;
            if v == 0 {
                // an inline variant has no 1.20.5 id
                for _ in 0..3 {
                    r.string()?;
                }
                items.holder_set(r, &mut Writer::new(), Reg::Same)?;
                return Ok(false);
            }
            w.varint(v - 1);
        }
        ty::OPT_GLOBAL_POSITION => {
            if Items::copy_bool(r, w)? {
                w.string(r.string()?);
                w.i64(r.i64()?);
            }
        }
        ty::PAINTING_VARIANT => {
            let v = r.varint()?;
            let old = if v == 0 {
                let (width, height) = (r.varint()?, r.varint()?);
                super::world::old_painting(r.string()?, Some((width, height)))
            } else {
                regs.painting(v - 1)
            };
            w.varint(old);
        }
        other => return Err(err(&format!("unknown entity data type {other}"))),
    }
    Ok(true)
}

/// SET_EQUIPMENT: slot bytes (high bit = more follow) with item stacks.
pub fn set_equipment(items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.varint(r.varint()?);
    loop {
        let slot = r.i8()?;
        w.i8(slot);
        items.item_stack(&mut r, &mut w)?;
        if slot >= 0 {
            break;
        }
    }
    Ok(w.into_inner())
}

#[cfg(test)]
mod tests {
    use super::super::items::Dir;
    use super::*;

    #[test]
    fn variants_become_plain_ids() {
        let items = Items::new(Dir::ToClient);
        let regs = Registries { paintings: vec![3, 7], ..Default::default() };
        let mut p = Writer::new();
        p.varint(4);
        p.u8(8).varint(ty::PAINTING_VARIANT).varint(2); // registry id 1
        p.u8(9).varint(ty::PAINTING_VARIANT).varint(0).varint(4).varint(3).string("minecraft:skeleton");
        p.u8(17).varint(ty::WOLF_VARIANT).varint(5); // registry id 4
        p.u8(18).varint(ty::WOLF_VARIANT).varint(0).string("a").string("b").string("c").varint(1); // inline, empty set
        p.u8(0xff);
        let out = set_entity_data(&items, &regs, &p.buf).unwrap();
        let mut e = Writer::new();
        e.varint(4);
        e.u8(8).varint(ty::PAINTING_VARIANT).varint(7);
        e.u8(9).varint(ty::PAINTING_VARIANT).varint(24); // skeleton
        e.u8(17).varint(ty::WOLF_VARIANT).varint(4);
        e.u8(0xff);
        assert_eq!(out, e.buf);
    }
}

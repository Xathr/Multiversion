//! Entity data 1.19.1 → 1.19.
//!
//! The only difference is the allay: 1.19.1 gave it two more fields ("dancing" and "can
//! duplicate") that the 1.19 allay doesn't have, so they are taken out of its entity data.

use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt;

/// The allay's id in the entity type registry (the same in both versions).
pub const ALLAY: i32 = 0;

/// Allay fields added in 1.19.1: dancing, can duplicate.
const NEW_ALLAY_FIELDS: [u8; 2] = [16, 17];
/// Entity data type whose value this step can't measure.
const PARTICLE: i32 = 15;

/// Reads the start of ADD_ENTITY: the entity's id and type.
pub fn spawned(p: &[u8]) -> Result<(i32, i32)> {
    let mut r = Reader::new(p);
    let entity = r.varint()?;
    r.uuid()?;
    Ok((entity, r.varint()?))
}

/// Skips one entity data value of the given type (the 1.19 type list). `false` = a type whose
/// length isn't known here.
fn skip_value(r: &mut Reader, ty: i32) -> Result<bool> {
    match ty {
        0 => {
            r.i8()?;
        }
        // varint, direction, block state, pose, cat / frog / painting variant
        1 | 11 | 13 | 18 | 19 | 20 | 22 => {
            r.varint()?;
        }
        2 => {
            r.f32()?;
        }
        3 | 4 => {
            r.string_max(262_144)?;
        }
        5 => {
            r.option(|r| r.string_max(262_144).map(|_| ()))?;
        }
        6 => {
            // item: present, id, count, tag
            if r.bool()? {
                r.varint()?;
                r.i8()?;
                nbt::read_named_optional(r)?;
            }
        }
        7 => {
            r.bool()?;
        }
        8 => {
            r.bytes(12)?; // rotation: three floats
        }
        9 => {
            r.i64()?;
        }
        10 => {
            r.option(Reader::i64)?;
        }
        12 => {
            r.option(Reader::uuid)?;
        }
        14 => {
            nbt::read_named_optional(r)?;
        }
        16 => {
            // villager data: type, profession, level
            r.varint()?;
            r.varint()?;
            r.varint()?;
        }
        17 => {
            r.varint()?; // optional varint: 0 = none
        }
        21 => {
            // optional global position: dimension, position
            if r.bool()? {
                r.string()?;
                r.i64()?;
            }
        }
        PARTICLE => return Ok(false),
        _ => return Err(DecodeError::Other(format!("unknown entity data type {ty}"))),
    }
    Ok(true)
}

/// SET_ENTITY_DATA of an allay, without the fields 1.19 doesn't have.
pub fn allay_data(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.varint(r.varint()?);
    loop {
        let start = r.position();
        let index = r.u8()?;
        if index == 0xff {
            w.u8(0xff);
            break;
        }
        let ty = r.varint()?;
        if !skip_value(&mut r, ty)? {
            // Allays have no particle field. Should one turn up, the packet goes through as
            // it is: a vanilla client ignores fields its entity doesn't have.
            return Ok(p.to_vec());
        }
        if !NEW_ALLAY_FIELDS.contains(&index) {
            let field = p.get(start..r.position()).ok_or(DecodeError::Eof)?;
            w.raw(field);
        }
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_gives_id_and_type() {
        let mut w = Writer::new();
        w.varint(300).uuid(9).varint(ALLAY).f64(0.0).f64(64.0).f64(0.0);
        assert_eq!(spawned(&w.into_inner()).unwrap(), (300, ALLAY));
        assert!(spawned(&[1, 2, 3]).is_err());
    }

    #[test]
    fn new_allay_fields_are_removed() {
        let mut w = Writer::new();
        w.varint(5);
        w.u8(0).varint(0).i8(0x20); // flags
        w.u8(16).varint(7).bool(true); // dancing
        w.u8(2).varint(5).bool(true).string("{\"text\":\"Al\"}"); // custom name
        w.u8(8).varint(6).bool(true).varint(1).i8(1).u8(0); // an item without a tag
        w.u8(17).varint(7).bool(false); // can duplicate
        w.u8(9).varint(2).f32(20.0); // health
        w.u8(0xff);
        let mut want = Writer::new();
        want.varint(5);
        want.u8(0).varint(0).i8(0x20);
        want.u8(2).varint(5).bool(true).string("{\"text\":\"Al\"}");
        want.u8(8).varint(6).bool(true).varint(1).i8(1).u8(0);
        want.u8(9).varint(2).f32(20.0);
        want.u8(0xff);
        assert_eq!(allay_data(&w.into_inner()).unwrap(), want.into_inner());
    }

    #[test]
    fn every_fixed_type_is_measured() {
        let mut w = Writer::new();
        w.varint(5);
        w.u8(1).varint(1).varint(300);
        w.u8(2).varint(3).string("s");
        w.u8(3).varint(4).string("\"c\"");
        w.u8(4).varint(8).f32(1.0).f32(2.0).f32(3.0);
        w.u8(5).varint(9).i64(1);
        w.u8(6).varint(10).bool(true).i64(1);
        w.u8(7).varint(12).bool(true).uuid(1);
        w.u8(8).varint(14).u8(0);
        w.u8(9).varint(16).varint(1).varint(2).varint(3);
        w.u8(10).varint(17).varint(0);
        w.u8(11).varint(21).bool(true).string("minecraft:overworld").i64(1);
        w.u8(16).varint(7).bool(true);
        w.u8(0xff);
        let p = w.into_inner();
        let out = allay_data(&p).unwrap();
        assert_eq!(out.len(), p.len() - 3);
        // every way of cutting it short is an error, not a panic
        for len in 0..p.len() {
            assert!(allay_data(&p[..len]).is_err(), "cut at {len}");
        }
    }

    #[test]
    fn unmeasurable_and_unknown_types() {
        let mut w = Writer::new();
        w.varint(5).u8(16).varint(7).bool(true).u8(3).varint(PARTICLE).varint(2).varint(1).u8(0xff);
        let p = w.into_inner();
        assert_eq!(allay_data(&p).unwrap(), p, "a particle field leaves the packet alone");
        let mut w = Writer::new();
        w.varint(5).u8(3).varint(99).u8(0xff);
        assert!(allay_data(&w.into_inner()).is_err());
    }
}

//! Entities 1.20.3 → 1.20.2: spawn types, entity data and equipment.
//!
//! Entity data serializer ids are the same in both versions; components become JSON, items,
//! block states and particles get their 1.20.2 ids. Breezes and wind charges are shown as
//! blazes and llama spit (the type table's stand-ins).

use std::collections::HashMap;

use super::items::{self, copy_bool, copy_bytes, copy_varint, err, item_stack};
use super::misc::particle;
use super::text::copy_component;
use crate::proto::buf::{Reader, Result, Writer};

/// Entity id → 1.20.3 type, for data that depends on the entity type.
#[derive(Default)]
pub struct Entities {
    types: HashMap<i32, i32>,
}

/// Poses 1.20.3 added for the breeze (sliding, shooting, inhaling).
const FIRST_NEW_POSE: i32 = 15;
const MINECART_DISPLAY_BLOCK: u8 = 11;
const TNT_BLOCK_STATE: u8 = 9;

impl Entities {
    pub fn clear(&mut self) {
        self.types.clear();
    }

    /// ADD_ENTITY: type id, and falling blocks carry their block state in the data field.
    pub fn add_entity(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        let id = copy_varint(&mut r, &mut w)?;
        w.uuid(r.uuid()?);
        let ty = r.varint()?;
        self.types.insert(id, ty);
        w.varint(items::entity(ty));
        copy_bytes(&mut r, &mut w, 24 + 3)?; // position, pitch, yaw, head yaw
        let data = r.varint()?;
        w.varint(if items::entity_name(ty) == "falling_block" { items::block_state(data) } else { data });
        w.raw(r.remaining());
        Ok(w.into_inner())
    }

    pub fn remove_entities(&mut self, payload: &[u8]) -> Result<()> {
        let mut r = Reader::new(payload);
        for id in r.list(|r| r.varint())? {
            self.types.remove(&id);
        }
        Ok(())
    }

    pub fn set_entity_data(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        let id = copy_varint(&mut r, &mut w)?;
        let name = self.types.get(&id).map_or("", |t| items::entity_name(*t));
        loop {
            let index = r.u8()?;
            if index == 0xFF {
                w.u8(0xFF);
                break;
            }
            let ty = r.varint()?;
            let mut value = Writer::new();
            let keep = entry(name, index, ty, &mut r, &mut value)?;
            if keep {
                w.u8(index);
                w.varint(ty);
                w.raw(&value.buf);
            }
        }
        Ok(w.into_inner())
    }
}

/// One entity data value; `false` = leave the entry out.
fn entry(entity: &str, index: u8, ty: i32, r: &mut Reader, w: &mut Writer) -> Result<bool> {
    match ty {
        0 => copy_bytes(r, w, 1)?,
        1 => {
            let v = r.varint()?;
            // minecart display block is a plain int holding a block state
            let minecart = entity.ends_with("minecart") && index == MINECART_DISPLAY_BLOCK;
            w.varint(if minecart { items::block_state(v) } else { v });
        }
        2 => {
            w.varlong(r.varlong()?);
        }
        3 => copy_bytes(r, w, 4)?,
        4 => items::copy_string(r, w)?,
        5 => copy_component(r, w)?,
        6 => {
            if copy_bool(r, w)? {
                copy_component(r, w)?;
            }
        }
        7 => item_stack(r, w)?,
        8 => copy_bytes(r, w, 1)?,
        9 => copy_bytes(r, w, 12)?,
        10 => copy_bytes(r, w, 8)?,
        11 => {
            if copy_bool(r, w)? {
                copy_bytes(r, w, 8)?;
            }
        }
        12 | 18..=19 | 21..=22 | 24..=25 => {
            let n = if ty == 18 { 3 } else { 1 };
            for _ in 0..n {
                copy_varint(r, w)?;
            }
        }
        13 => {
            if copy_bool(r, w)? {
                copy_bytes(r, w, 16)?;
            }
        }
        14 => {
            let state = r.varint()?;
            // 1.20.3 lets primed TNT show any block; 1.20.2 has no such entry
            if entity == "tnt" && index == TNT_BLOCK_STATE {
                return Ok(false);
            }
            w.varint(items::block_state(state));
        }
        15 => {
            // the state id itself, 0 (air) meaning none
            w.varint(items::block_state(r.varint()?));
        }
        16 => items::copy_nbt(r, w)?,
        17 => particle(r, w)?,
        20 => {
            let pose = r.varint()?;
            if pose >= FIRST_NEW_POSE {
                return Ok(false);
            }
            w.varint(pose);
        }
        23 => {
            if copy_bool(r, w)? {
                items::copy_string(r, w)?;
                copy_bytes(r, w, 8)?;
            }
        }
        26 => copy_bytes(r, w, 12)?,
        27 => copy_bytes(r, w, 16)?,
        _ => return Err(err("unknown entity data type")),
    }
    Ok(true)
}

/// SET_EQUIPMENT: slots with a continuation bit, each followed by an item.
pub fn set_equipment(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_varint(&mut r, &mut w)?;
    loop {
        let slot = r.u8()?;
        w.u8(slot);
        item_stack(&mut r, &mut w)?;
        if slot & 0x80 == 0 {
            break;
        }
    }
    Ok(w.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::data_1_20_2 as d;
    use crate::proto::nbt::{self, Tag};

    fn new_entity(name: &str) -> i32 {
        d::NEW_ENTITY_NAMES.iter().position(|n| *n == name).unwrap() as i32
    }

    fn spawn(e: &mut Entities, id: i32, ty: i32, data: i32) -> Vec<u8> {
        let mut w = Writer::new();
        w.varint(id).uuid(7).varint(ty);
        w.raw(&[0; 27]);
        w.varint(data);
        w.raw(&[0; 6]);
        e.add_entity(&w.buf).unwrap()
    }

    #[test]
    fn spawn_types_and_falling_block_state() {
        let mut e = Entities::default();
        let last = d::NEW_BLOCK_STATE_COUNT as i32 - 1;
        let out = spawn(&mut e, 3, new_entity("falling_block"), last);
        let mut r = Reader::new(&out);
        r.varint().unwrap();
        r.uuid().unwrap();
        assert_eq!(r.varint().unwrap(), d::FALLING_BLOCK_ENTITY_OLD as i32);
        r.bytes(27).unwrap();
        assert_eq!(r.varint().unwrap(), items::block_state(last));
        let out = spawn(&mut e, 4, new_entity("wind_charge"), 0);
        let mut r = Reader::new(&out);
        r.varint().unwrap();
        r.uuid().unwrap();
        assert_eq!(d::OLD_ENTITY_NAMES[r.varint().unwrap() as usize], "llama_spit");
    }

    #[test]
    fn entity_data_components_items_and_tnt() {
        let mut e = Entities::default();
        spawn(&mut e, 5, new_entity("tnt"), 0);
        let mut w = Writer::new();
        w.varint(5);
        w.u8(2).varint(6).bool(true); // custom name (optional component)
        nbt::write(&mut w, &Tag::Compound(vec![("text".into(), Tag::String("Boom".into())), ("bold".into(), Tag::Byte(1))]));
        w.u8(9).varint(14).varint(10); // block state: 1.20.2 tnt has none
        w.u8(6).varint(20).varint(16); // pose: shooting
        w.u8(8).varint(7).bool(true).varint(1).i8(1).u8(0); // an item
        w.u8(0xFF);
        let out = e.set_entity_data(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.varint().unwrap(), 5);
        assert_eq!((r.u8().unwrap(), r.varint().unwrap(), r.bool().unwrap()), (2, 6, true));
        let json: serde_json::Value = serde_json::from_str(r.string().unwrap()).unwrap();
        assert_eq!(json, serde_json::json!({"text": "Boom", "bold": true}));
        assert_eq!((r.u8().unwrap(), r.varint().unwrap()), (8, 7));
        assert!(r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), items::item(1));
        r.i8().unwrap();
        r.u8().unwrap();
        assert_eq!(r.u8().unwrap(), 0xFF);
        assert!(r.is_empty());
    }

    #[test]
    fn minecart_display_block() {
        let mut e = Entities::default();
        spawn(&mut e, 9, new_entity("chest_minecart"), 0);
        let last = d::NEW_BLOCK_STATE_COUNT as i32 - 1;
        let mut w = Writer::new();
        w.varint(9).u8(11).varint(1).varint(last).u8(0xFF);
        let out = e.set_entity_data(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        r.varint().unwrap();
        r.u8().unwrap();
        r.varint().unwrap();
        assert_eq!(r.varint().unwrap(), items::block_state(last));
    }
}

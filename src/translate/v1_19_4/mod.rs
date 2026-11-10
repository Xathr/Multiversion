//! 1.20 / 1.20.1 (protocol 763) ↔ 1.19.4 (protocol 762).
//!
//! The two versions number their packets identically and share most layouts. What 1.20 changed:
//! signs have two sides (a 1.19.4 client shows and edits the front), light data lost its "trust
//! edges" flag, LOGIN and RESPAWN gained a portal cooldown, the combat packets lost an unused
//! entity id, advancements gained a telemetry flag, and the 1.20 blocks, items, sounds and trim
//! patterns are new. In 1.19.4 the 1.20 features sit behind a feature flag, which is switched on
//! so the client shows what it already has (cherry, bamboo wood, hanging signs, camels).
//!
//! Not converted: translation keys new in 1.20 inside chat and titles show as the raw key.

pub mod ids;
pub mod items;
pub mod world;

use std::collections::{HashMap, HashSet};

use super::{Packet, State, TResult, Translator};
use crate::proto::buf::{Reader, Writer};
use crate::proto::version::Version;
use items::Kind;

#[derive(Default)]
pub struct V1_20To1_19_4 {
    /// Spawned entities whose entity data needs more than a type-by-type rewrite.
    kinds: HashMap<i32, Kind>,
    /// Item displays that have had entity data: the first one must carry the rotation.
    displays_seen: HashSet<i32>,
    /// The sign whose back the server opened for editing. A 1.19.4 client edits "the sign"; its
    /// answer is sent as the back's text when it is for this position.
    editing_back_of: Option<i64>,
}

impl V1_20To1_19_4 {
    pub fn new() -> Self {
        Self::default()
    }

    fn play_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        if !(0..=ids::LAST_CLIENTBOUND).contains(&pkt.id) {
            return Err(format!("no 1.19.4 equivalent for clientbound play packet 0x{:02x}", pkt.id));
        }
        let payload = match pkt.id {
            ids::ADD_ENTITY => {
                let (entity, ty, payload) = world::add_entity(p).map_err(e)?;
                self.displays_seen.remove(&entity);
                match items::entity_kind(ty) {
                    Some(k) => self.kinds.insert(entity, k),
                    None => self.kinds.remove(&entity),
                };
                payload
            }
            ids::REMOVE_ENTITIES => {
                let mut r = Reader::new(p);
                for id in r.list(Reader::varint).map_err(e)? {
                    self.kinds.remove(&id);
                    self.displays_seen.remove(&id);
                }
                p.to_vec()
            }
            ids::LOGIN => {
                self.kinds.clear();
                self.displays_seen.clear();
                self.editing_back_of = None;
                world::login(p).map_err(e)?
            }
            ids::RESPAWN => {
                self.kinds.clear();
                self.displays_seen.clear();
                world::respawn(p).map_err(e)?
            }
            ids::SET_ENTITY_DATA => {
                let mut r = Reader::new(p);
                let entity = r.varint().map_err(e)?;
                let kind = self.kinds.get(&entity).copied();
                let first = kind == Some(Kind::ItemDisplay) && self.displays_seen.insert(entity);
                let mut w = Writer::with_capacity(p.len() + 24);
                w.varint(entity);
                items::entity_data(kind, first, &mut r, &mut w).map_err(e)?;
                w.into_inner()
            }
            ids::OPEN_SIGN_EDITOR => {
                let mut r = Reader::new(p);
                let position = r.i64().map_err(e)?;
                let front = r.bool().map_err(e)?;
                self.editing_back_of = (!front).then_some(position);
                position.to_be_bytes().to_vec()
            }
            ids::LEVEL_CHUNK_WITH_LIGHT => world::level_chunk_with_light(p).map_err(e)?,
            ids::LIGHT_UPDATE => world::light_update(p).map_err(e)?,
            ids::BLOCK_ENTITY_DATA => match world::block_entity_data(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            ids::BLOCK_UPDATE => world::block_update(p).map_err(e)?,
            ids::BLOCK_EVENT => world::block_event(p).map_err(e)?,
            ids::SECTION_BLOCKS_UPDATE => world::section_blocks_update(p).map_err(e)?,
            ids::LEVEL_EVENT => world::level_event(p).map_err(e)?,
            ids::SOUND | ids::SOUND_ENTITY => world::sound(p).map_err(e)?,
            ids::PLAYER_COMBAT_END => world::combat_end(p).map_err(e)?,
            ids::PLAYER_COMBAT_KILL => world::combat_kill(p).map_err(e)?,
            ids::UPDATE_ENABLED_FEATURES => world::enabled_features(p).map_err(e)?,
            ids::OPEN_SCREEN => world::open_screen(p).map_err(e)?,
            ids::UPDATE_TAGS => world::update_tags(p).map_err(e)?,
            ids::AWARD_STATS => world::award_stats(p).map_err(e)?,
            ids::COOLDOWN => items::cooldown(p).map_err(e)?,
            ids::LEVEL_PARTICLES => items::level_particles(p).map_err(e)?,
            ids::SET_EQUIPMENT => items::set_equipment(p).map_err(e)?,
            ids::CONTAINER_SET_CONTENT => items::container_set_content(p).map_err(e)?,
            ids::CONTAINER_SET_SLOT => items::container_set_slot(p).map_err(e)?,
            ids::MERCHANT_OFFERS => items::merchant_offers(p).map_err(e)?,
            ids::UPDATE_ADVANCEMENTS => items::update_advancements(p).map_err(e)?,
            ids::UPDATE_RECIPES => items::update_recipes(p).map_err(e)?,
            _ => p.to_vec(),
        };
        out.push(Packet::new(pkt.id, payload));
        Ok(())
    }

    fn play_serverbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let e = |e: crate::proto::buf::DecodeError| format!("serverbound 0x{:02x}: {e}", pkt.id);
        let p = &pkt.payload[..];
        if !(0..=ids::LAST_SERVERBOUND).contains(&pkt.id) {
            return Err(format!("unknown 1.19.4 serverbound play packet 0x{:02x}", pkt.id));
        }
        let payload = match pkt.id {
            ids::S_SET_CREATIVE_MODE_SLOT => items::set_creative_slot(p).map_err(e)?,
            ids::S_CONTAINER_CLICK => items::container_click(p).map_err(e)?,
            ids::S_SIGN_UPDATE => {
                // 1.20 wants to know which side was written.
                let mut r = Reader::new(p);
                let position = r.i64().map_err(e)?;
                let front = self.editing_back_of.take() != Some(position);
                let mut w = Writer::with_capacity(p.len() + 1);
                w.i64(position).bool(front);
                w.raw(r.remaining());
                w.into_inner()
            }
            _ => p.to_vec(),
        };
        out.push(Packet::new(pkt.id, payload));
        Ok(())
    }
}

impl Translator for V1_20To1_19_4 {
    fn from(&self) -> Version {
        Version { protocol: 763 }
    }
    fn to(&self) -> Version {
        Version { protocol: 762 }
    }
    fn clientbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_clientbound(pkt, out),
            // Neither version has a configuration phase; the 1.20.2 step plays it for the client.
            State::Configuration => {
                out.push(pkt.clone());
                Ok(())
            }
        }
    }
    fn serverbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_serverbound(pkt, out),
            State::Configuration => {
                out.push(pkt.clone());
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cb(t: &mut V1_20To1_19_4, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        t.clientbound(State::Play, &Packet::new(id, payload), &mut out).unwrap();
        out
    }
    fn sb(t: &mut V1_20To1_19_4, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        t.serverbound(State::Play, &Packet::new(id, payload), &mut out).unwrap();
        out
    }

    #[test]
    fn unchanged_packets_pass_with_the_same_id() {
        let mut t = V1_20To1_19_4::new();
        let out = cb(&mut t, 35, vec![1, 2, 3, 4, 5, 6, 7, 8]); // keep alive
        assert_eq!((out[0].id, &out[0].payload[..]), (35, &[1u8, 2, 3, 4, 5, 6, 7, 8][..]));
        let out = sb(&mut t, 18, vec![8, 7, 6, 5, 4, 3, 2, 1]);
        assert_eq!((out[0].id, &out[0].payload[..]), (18, &[8u8, 7, 6, 5, 4, 3, 2, 1][..]));
        assert!(t.clientbound(State::Play, &Packet::new(200, vec![]), &mut Vec::new()).is_err());
        assert!(t.serverbound(State::Play, &Packet::new(-1, vec![]), &mut Vec::new()).is_err());
    }

    #[test]
    fn sign_edits_go_to_the_side_the_server_opened() {
        let mut t = V1_20To1_19_4::new();
        let lines = {
            let mut w = Writer::new();
            for l in ["a", "b", "c", "d"] {
                w.string(l);
            }
            w.into_inner()
        };
        let update = |pos: i64| [&pos.to_be_bytes()[..], &lines].concat();
        // the back of the sign at 42
        let mut open = Writer::new();
        open.i64(42).bool(false);
        let out = cb(&mut t, ids::OPEN_SIGN_EDITOR, open.into_inner());
        assert_eq!(out[0].payload, 42i64.to_be_bytes(), "1.19.4 gets only the position");
        let out = sb(&mut t, ids::S_SIGN_UPDATE, update(42));
        assert_eq!(out[0].payload[8], 0, "written to the back");
        assert_eq!(&out[0].payload[9..], &lines[..]);
        // the next edit of the same sign, with no editor opened for its back, is the front
        let out = sb(&mut t, ids::S_SIGN_UPDATE, update(42));
        assert_eq!(out[0].payload[8], 1);
        // the front, opened explicitly
        let mut open = Writer::new();
        open.i64(7).bool(true);
        cb(&mut t, ids::OPEN_SIGN_EDITOR, open.into_inner());
        assert_eq!(sb(&mut t, ids::S_SIGN_UPDATE, update(7))[0].payload[8], 1);
    }

    #[test]
    fn falling_blocks_and_item_displays_are_tracked() {
        use super::super::data_1_19_4 as d;
        let ty = |name: &str| d::NEW_ENTITY_NAMES.iter().position(|n| *n == name).unwrap() as i32;
        let spawn = |entity: i32, ty: i32, data: i32| {
            let mut w = Writer::new();
            w.varint(entity).raw(&[0; 16]).varint(ty).raw(&[0; 27]).varint(data).raw(&[0; 6]);
            w.into_inner()
        };
        let mut t = V1_20To1_19_4::new();
        let state = (d::NEW_BLOCK_STATE_COUNT - 1) as i32;
        let out = cb(&mut t, ids::ADD_ENTITY, spawn(5, ty("falling_block"), state));
        let mut r = Reader::new(&out[0].payload);
        r.varint().unwrap();
        r.bytes(16).unwrap();
        r.varint().unwrap();
        r.bytes(27).unwrap();
        assert_eq!(r.varint().unwrap(), items::block_state(state));

        cb(&mut t, ids::ADD_ENTITY, spawn(6, ty("item_display"), 0));
        let data = |entity: i32| {
            let mut w = Writer::new();
            w.varint(entity).u8(0xff);
            w.into_inner()
        };
        assert!(cb(&mut t, ids::SET_ENTITY_DATA, data(6))[0].payload.len() > 2, "first data carries the rotation");
        assert_eq!(cb(&mut t, ids::SET_ENTITY_DATA, data(6))[0].payload, data(6), "later data does not");
        // removed and spawned again: first data once more
        let mut gone = Writer::new();
        gone.varint(1).varint(6);
        cb(&mut t, ids::REMOVE_ENTITIES, gone.into_inner());
        cb(&mut t, ids::ADD_ENTITY, spawn(6, ty("item_display"), 0));
        assert!(cb(&mut t, ids::SET_ENTITY_DATA, data(6))[0].payload.len() > 2);
    }

    #[test]
    fn malformed_packets_are_errors_not_panics() {
        let mut state: u64 = 0x1234_5678_9abc_def1;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut t = V1_20To1_19_4::new();
        for _ in 0..40_000 {
            let len = (next() % 64) as usize;
            let payload: Vec<u8> = (0..len).map(|_| if next() % 3 == 0 { (next() % 4) as u8 } else { next() as u8 }).collect();
            let id = (next() % 112) as i32;
            let _ = t.clientbound(State::Play, &Packet::new(id, payload.clone()), &mut Vec::new());
            let _ = t.serverbound(State::Play, &Packet::new(id % 52, payload), &mut Vec::new());
        }
    }
}

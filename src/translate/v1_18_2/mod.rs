//! 1.19 (protocol 759) ↔ 1.18.2 (protocol 758).
//!
//! What 1.19 changed, and what this step does about it:
//! - Chat: 1.19 signs it. A 1.18.2 client has no key, so its messages and commands reach the
//!   server unsigned, and what the server says reaches it as plain chat (see `chat`).
//! - Spawning: 1.19 spawns everything with ADD_ENTITY. 1.18.2 has its own packets for living
//!   entities and for paintings, whose motive 1.19 only sends afterwards as entity data.
//! - Dimensions: LOGIN and RESPAWN name the dimension type; 1.18.2 wants the type in full, so
//!   the step remembers the ones LOGIN lists.
//! - Blocks are acknowledged by a sequence number in 1.19; the client's digging, placing and
//!   item use get sequence 0 and the server's acknowledgement is dropped (1.18.2 does not wait
//!   for one).
//! - Commands name argument types by id instead of by name, sounds carry a seed, merchant
//!   offers and beacon effects are laid out differently, player info carries chat keys.
//! - Everything 1.19 added to the registries: mud, mangrove wood, sculk, froglights, the
//!   frog (shown as a rabbit), tadpole (pufferfish), warden (iron golem), allay (vex), chest
//!   boats (boats), four paintings, and their sounds and particles.

pub mod chat;
pub mod ids;
pub mod items;
pub mod world;

use std::collections::HashMap;

use super::{data_1_18_2, Packet, State, TResult, Translator};
use crate::proto::buf::{Reader, Writer};
use crate::proto::version::Version;
use ids::{new, old};
use items::Kind;
use world::{Dimensions, Painting, Spawn};

pub struct V1_19To1_18_2 {
    /// Spawned entities whose entity data needs more than a type-by-type rewrite, and whether
    /// any entity data was sent for them yet.
    kinds: HashMap<i32, (Kind, bool)>,
    /// Paintings the server spawned that the client hasn't been told about: their motive
    /// comes with their entity data.
    paintings: HashMap<i32, Painting>,
    /// The server's dimension types (from LOGIN).
    dimensions: Dimensions,
}

impl V1_19To1_18_2 {
    pub fn new() -> Self {
        Self { kinds: HashMap::new(), paintings: HashMap::new(), dimensions: Dimensions::default() }
    }

    fn forget(&mut self, entity: i32) {
        self.kinds.remove(&entity);
        self.paintings.remove(&entity);
    }

    fn play_clientbound(&mut self, pkt: &mut Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            // 1.18.2 has no server data or chat previews, and doesn't wait for block changes to
            // be acknowledged by sequence.
            new::SERVER_DATA | new::CHAT_PREVIEW | new::SET_DISPLAY_CHAT_PREVIEW | new::BLOCK_CHANGED_ACK => return Ok(()),
            new::SYSTEM_CHAT => {
                out.push(Packet::new(old::CHAT, chat::system_chat(p).map_err(e)?));
                return Ok(());
            }
            new::PLAYER_CHAT => {
                out.push(Packet::new(old::CHAT, chat::player_chat(p).map_err(e)?));
                return Ok(());
            }
            new::LOGIN => {
                self.kinds.clear();
                self.paintings.clear();
                world::login(p, &mut self.dimensions).map_err(e)?
            }
            new::RESPAWN => {
                self.kinds.clear();
                self.paintings.clear();
                world::respawn(p, &self.dimensions).map_err(e)?
            }
            new::ADD_ENTITY => {
                let spawned = world::add_entity(p).map_err(e)?;
                self.forget(spawned.entity);
                if let Some(kind) = items::entity_kind(spawned.ty) {
                    self.kinds.insert(spawned.entity, (kind, false));
                }
                match spawned.spawn {
                    Spawn::Object(payload) => out.push(Packet::new(old::ADD_ENTITY, payload)),
                    Spawn::Mob(payload) => out.push(Packet::new(old::ADD_MOB, payload)),
                    Spawn::Painting(painting) => {
                        self.paintings.insert(spawned.entity, painting);
                    }
                    Spawn::Nothing => {}
                }
                return Ok(());
            }
            // an id that was tracked may come back as something else
            new::ADD_PLAYER | new::ADD_EXPERIENCE_ORB => {
                let id = Reader::new(p).varint().map_err(e)?;
                self.forget(id);
                p.to_vec()
            }
            new::REMOVE_ENTITIES => {
                for id in Reader::new(p).list(Reader::varint).map_err(e)? {
                    self.forget(id);
                }
                p.to_vec()
            }
            new::SET_ENTITY_DATA => {
                let mut r = Reader::new(p);
                let entity = r.varint().map_err(e)?;
                let mut w = Writer::with_capacity(p.len() + 24);
                w.varint(entity);
                let header = w.buf.len();
                let tracked = self.kinds.get_mut(&entity);
                let (kind, first) = tracked.as_ref().map_or((None, false), |(k, seen)| (Some(*k), !*seen));
                let motive = items::entity_data(kind, first, &mut r, &mut w).map_err(e)?;
                if let Some((_, seen)) = tracked {
                    *seen = true;
                }
                if let Some(painting) = self.paintings.get(&entity) {
                    // the client only learns of the painting now, or not yet
                    let Some(motive) = motive else { return Ok(()) };
                    out.push(Packet::new(old::ADD_PAINTING, world::add_painting(entity, painting, motive)));
                    self.paintings.remove(&entity);
                }
                // nothing left but the end marker
                if w.buf.len() == header + 1 {
                    return Ok(());
                }
                w.into_inner()
            }
            new::SOUND | new::SOUND_ENTITY => world::sound(p).map_err(e)?,
            new::CUSTOM_SOUND => world::custom_sound(p).map_err(e)?,
            new::STOP_SOUND => world::stop_sound(p).map_err(e)?,
            new::LEVEL_CHUNK_WITH_LIGHT => world::level_chunk_with_light(p).map_err(e)?,
            new::BLOCK_ENTITY_DATA => match world::block_entity_data(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::BLOCK_EVENT => match world::block_event(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::BLOCK_UPDATE => world::block_update(p).map_err(e)?,
            new::SECTION_BLOCKS_UPDATE => world::section_blocks_update(p).map_err(e)?,
            new::LEVEL_EVENT => world::level_event(p).map_err(e)?,
            new::PLAYER_INFO => world::player_info(p).map_err(e)?,
            new::UPDATE_MOB_EFFECT => world::update_mob_effect(p).map_err(e)?,
            new::UPDATE_TAGS => world::update_tags(p).map_err(e)?,
            new::AWARD_STATS => world::award_stats(p).map_err(e)?,
            new::COMMANDS => world::commands(p).map_err(e)?,
            new::LEVEL_PARTICLES => match items::level_particles(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::SET_EQUIPMENT => items::set_equipment(p).map_err(e)?,
            new::CONTAINER_SET_CONTENT => items::container_set_content(p).map_err(e)?,
            new::CONTAINER_SET_SLOT => items::container_set_slot(p).map_err(e)?,
            new::COOLDOWN => items::cooldown(p).map_err(e)?,
            new::MERCHANT_OFFERS => items::merchant_offers(p).map_err(e)?,
            new::UPDATE_ADVANCEMENTS => items::update_advancements(p).map_err(e)?,
            new::UPDATE_RECIPES => items::update_recipes(p).map_err(e)?,
            _ => std::mem::take(&mut pkt.payload),
        };
        let mapped = ids::play_clientbound(pkt.id).ok_or_else(|| format!("no 1.18.2 equivalent for clientbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }

    fn play_serverbound(&mut self, pkt: &mut Packet, out: &mut Vec<Packet>) -> TResult {
        let e = |e: crate::proto::buf::DecodeError| format!("serverbound 0x{:02x}: {e}", pkt.id);
        let p = &pkt.payload[..];
        let payload = match pkt.id {
            old::S_CHAT => {
                let (id, payload) = match chat::chat(p, crate::platform::now_millis()).map_err(e)? {
                    chat::Typed::Message(payload) => (new::S_CHAT, payload),
                    chat::Typed::Command(payload) => (new::S_CHAT_COMMAND, payload),
                };
                out.push(Packet::new(id, payload));
                return Ok(());
            }
            // 1.19 numbers these so the server can acknowledge them; the server only keeps
            // the highest number it saw, and the client isn't waiting for an answer
            old::S_PLAYER_ACTION | old::S_USE_ITEM_ON | old::S_USE_ITEM => {
                let mut w = Writer::with_capacity(p.len() + 1);
                w.raw(p).varint(0);
                w.into_inner()
            }
            old::S_SET_BEACON => set_beacon(p).map_err(e)?,
            old::S_SET_CREATIVE_MODE_SLOT => items::set_creative_slot(p).map_err(e)?,
            old::S_CONTAINER_CLICK => items::container_click(p).map_err(e)?,
            _ => std::mem::take(&mut pkt.payload),
        };
        let mapped = ids::play_serverbound(pkt.id).ok_or_else(|| format!("unknown 1.18.2 serverbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }
}

/// SET_BEACON: 1.18.2 sends each effect's id, with zero or less for "none"; 1.19 sends each as
/// an optional id.
fn set_beacon(p: &[u8]) -> crate::proto::buf::Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 2);
    for _ in 0..2 {
        let effect = r.varint()?;
        w.bool(effect > 0);
        if effect > 0 {
            w.varint(effect);
        }
    }
    Ok(w.into_inner())
}

impl Default for V1_19To1_18_2 {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator for V1_19To1_18_2 {
    fn from(&self) -> Version {
        Version { protocol: 759 }
    }
    fn to(&self) -> Version {
        Version { protocol: 758 }
    }
    fn clientbound(&mut self, state: State, pkt: &mut Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_clientbound(pkt, out),
            // neither version has a configuration phase: a newer step stands in for it
            State::Configuration => {
                out.push(Packet::new(pkt.id, std::mem::take(&mut pkt.payload)));
                Ok(())
            }
        }
    }
    fn serverbound(&mut self, state: State, pkt: &mut Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_serverbound(pkt, out),
            State::Configuration => {
                out.push(Packet::new(pkt.id, std::mem::take(&mut pkt.payload)));
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::world::tests::{server_login, spawn};
    use super::*;

    fn cb(t: &mut V1_19To1_18_2, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        t.clientbound(State::Play, &mut Packet::new(id, payload), &mut out).unwrap();
        out
    }

    fn sb(t: &mut V1_19To1_18_2, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        t.serverbound(State::Play, &mut Packet::new(id, payload), &mut out).unwrap();
        out
    }

    fn pairs(out: &[Packet]) -> Vec<(i32, Vec<u8>)> {
        out.iter().map(|p| (p.id, p.payload.clone())).collect()
    }

    fn entity(name: &str) -> i32 {
        data_1_18_2::NEW_ENTITY_NAMES.iter().position(|n| *n == name).unwrap() as i32
    }

    fn joined() -> V1_19To1_18_2 {
        let mut t = V1_19To1_18_2::new();
        cb(&mut t, new::LOGIN, server_login("minecraft:overworld"));
        t
    }

    #[test]
    fn versions() {
        let t = V1_19To1_18_2::new();
        assert_eq!((t.from().protocol, t.to().protocol), (759, 758));
        assert!(!t.emulates_configuration());
    }

    #[test]
    fn unchanged_packets_only_change_id() {
        let mut t = joined();
        for (from, to) in [(new::KEEP_ALIVE, old::KEEP_ALIVE), (new::SET_HEALTH, old::SET_HEALTH), (new::TELEPORT_ENTITY, old::TELEPORT_ENTITY), (new::EXPLODE, old::EXPLODE)] {
            assert_eq!(pairs(&cb(&mut t, from, vec![1, 2, 3])), [(to, vec![1, 2, 3])]);
        }
        for (from, to) in [(old::S_KEEP_ALIVE, new::S_KEEP_ALIVE), (old::S_MOVE_PLAYER_POS, new::S_MOVE_PLAYER_POS), (old::S_SWING, new::S_SWING)] {
            assert_eq!(pairs(&sb(&mut t, from, vec![1, 2, 3])), [(to, vec![1, 2, 3])]);
        }
        let mut out = Vec::new();
        assert!(t.clientbound(State::Play, &mut Packet::new(200, vec![]), &mut out).is_err());
        assert!(t.serverbound(State::Play, &mut Packet::new(200, vec![]), &mut out).is_err());
        t.clientbound(State::Configuration, &mut Packet::new(3, vec![9]), &mut out).unwrap();
        assert_eq!(pairs(&out), [(3, vec![9])]);
    }

    #[test]
    fn packets_the_client_cannot_use_are_dropped() {
        let mut t = joined();
        for id in [new::SERVER_DATA, new::CHAT_PREVIEW, new::SET_DISPLAY_CHAT_PREVIEW, new::BLOCK_CHANGED_ACK] {
            assert!(cb(&mut t, id, vec![1, 2, 3]).is_empty());
        }
    }

    #[test]
    fn chat_goes_both_ways() {
        let mut t = joined();
        let mut w = Writer::new();
        w.string("\"hi\"").varint(1);
        let out = cb(&mut t, new::SYSTEM_CHAT, w.into_inner());
        assert_eq!(out[0].id, old::CHAT);

        let mut w = Writer::new();
        w.string("hello");
        let out = sb(&mut t, old::S_CHAT, w.into_inner());
        assert_eq!(out[0].id, new::S_CHAT);
        let mut r = Reader::new(&out[0].payload);
        assert_eq!(r.string().unwrap(), "hello");
        assert!(r.i64().unwrap() > 0, "stamped with the time");
        assert_eq!((r.i64().unwrap(), r.byte_array().unwrap().len(), r.bool().unwrap()), (0, 0, false));

        let mut w = Writer::new();
        w.string("/time set day");
        let out = sb(&mut t, old::S_CHAT, w.into_inner());
        assert_eq!(out[0].id, new::S_CHAT_COMMAND);
        assert_eq!(Reader::new(&out[0].payload).string().unwrap(), "time set day");
    }

    #[test]
    fn block_actions_get_a_sequence_and_beacons_optional_effects() {
        let mut t = joined();
        for (from, to) in [(old::S_PLAYER_ACTION, new::S_PLAYER_ACTION), (old::S_USE_ITEM_ON, new::S_USE_ITEM_ON), (old::S_USE_ITEM, new::S_USE_ITEM)] {
            assert_eq!(pairs(&sb(&mut t, from, vec![0, 5, 6])), [(to, vec![0, 5, 6, 0])]);
        }
        assert_eq!(pairs(&sb(&mut t, old::S_SET_BEACON, vec![1, 0])), [(new::S_SET_BEACON, vec![1, 1, 0])]);
        assert_eq!(pairs(&sb(&mut t, old::S_SET_BEACON, vec![3, 10])), [(new::S_SET_BEACON, vec![1, 3, 1, 10])]);
        let mut out = Vec::new();
        assert!(t.serverbound(State::Play, &mut Packet::new(old::S_SET_BEACON, vec![1]), &mut out).is_err());
    }

    #[test]
    fn spawns_use_the_packet_1_18_2_has_for_them() {
        let mut t = joined();
        assert_eq!(cb(&mut t, new::ADD_ENTITY, spawn(5, entity("zombie"), 0))[0].id, old::ADD_MOB);
        assert_eq!(cb(&mut t, new::ADD_ENTITY, spawn(6, entity("item"), 0))[0].id, old::ADD_ENTITY);
        // a warden is an iron golem with a name, once
        assert_eq!(cb(&mut t, new::ADD_ENTITY, spawn(7, entity("warden"), 0))[0].id, old::ADD_MOB);
        let data = || {
            let mut w = Writer::new();
            w.varint(7).u8(16).varint(1).varint(40).u8(0xff);
            w.into_inner()
        };
        let out = cb(&mut t, new::SET_ENTITY_DATA, data());
        assert_eq!(out[0].id, old::SET_ENTITY_DATA);
        assert!(String::from_utf8_lossy(&out[0].payload).contains("Warden"));
        // nothing left of the second update: anger has no place on a golem
        assert!(cb(&mut t, new::SET_ENTITY_DATA, data()).is_empty());
        // removed and reused as an item: no name
        cb(&mut t, new::REMOVE_ENTITIES, vec![1, 7]);
        cb(&mut t, new::ADD_ENTITY, spawn(7, entity("item"), 0));
        let mut w = Writer::new();
        w.varint(7).u8(0).varint(0).i8(0).u8(0xff);
        let out = cb(&mut t, new::SET_ENTITY_DATA, w.buf.clone());
        assert_eq!(out[0].payload, w.buf);
    }

    #[test]
    fn paintings_appear_when_their_motive_arrives() {
        let mut t = joined();
        assert!(cb(&mut t, new::ADD_ENTITY, spawn(9, entity("painting"), 2)).is_empty());
        // data without a motive: the client doesn't know the painting yet
        let mut w = Writer::new();
        w.varint(9).u8(0).varint(0).i8(0).u8(0xff);
        assert!(cb(&mut t, new::SET_ENTITY_DATA, w.into_inner()).is_empty());
        let mut w = Writer::new();
        w.varint(9).u8(0).varint(0).i8(0).u8(8).varint(22).varint(3).u8(0xff);
        let out = cb(&mut t, new::SET_ENTITY_DATA, w.into_inner());
        assert_eq!(out.iter().map(|p| p.id).collect::<Vec<_>>(), [old::ADD_PAINTING, old::SET_ENTITY_DATA]);
        let mut r = Reader::new(&out[0].payload);
        assert_eq!((r.varint().unwrap(), r.uuid().unwrap(), r.varint().unwrap()), (9, 9, 3));
        r.i64().unwrap();
        assert_eq!(r.u8().unwrap(), 2);
        let mut want = Writer::new();
        want.varint(9).u8(0).varint(0).i8(0).u8(0xff);
        assert_eq!(out[1].payload, want.buf);
        // a second motive doesn't spawn it again
        let mut w = Writer::new();
        w.varint(9).u8(8).varint(22).varint(4).u8(0xff);
        assert!(cb(&mut t, new::SET_ENTITY_DATA, w.into_inner()).is_empty());
        // a new world forgets the ones still waiting
        cb(&mut t, new::ADD_ENTITY, spawn(10, entity("painting"), 2));
        cb(&mut t, new::LOGIN, server_login("minecraft:overworld"));
        let mut w = Writer::new();
        w.varint(10).u8(8).varint(22).varint(4).u8(0xff);
        assert!(cb(&mut t, new::SET_ENTITY_DATA, w.into_inner()).is_empty());
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
        /// Bytes that look like packet fields: small numbers, flags, long varints, and noise.
        fn byte(&mut self) -> u8 {
            match self.below(10) {
                0..=3 => self.below(4) as u8,
                4 => 0xff,
                5 => 0x80 | self.below(128) as u8,
                6 => self.below(64) as u8,
                7 => 0x7f,
                _ => self.next() as u8,
            }
        }
        fn payload(&mut self) -> Vec<u8> {
            let len = match self.below(8) {
                0 => 0,
                1..=4 => self.below(24),
                5 | 6 => self.below(200),
                _ => self.below(3000),
            };
            (0..len).map(|_| self.byte()).collect()
        }
        /// A well-formed packet, damaged: cut short, or with a few bytes changed.
        fn damage(&mut self, good: &[u8]) -> Vec<u8> {
            let mut p = good.to_vec();
            if self.below(2) == 0 {
                p.truncate(self.below(p.len() as u64 + 1) as usize);
            }
            for _ in 0..self.below(4) {
                if !p.is_empty() {
                    let at = self.below(p.len() as u64) as usize;
                    p[at] = self.byte();
                }
            }
            p
        }
    }

    /// Noise and damaged packets in both directions: an error is fine, a panic, a hang or a
    /// huge answer is not. A player controls the serverbound bytes.
    #[test]
    fn malformed_packets_never_panic() {
        let mut typed = Writer::new();
        typed.string("/msg Alex hi");
        let serverbound = [(old::S_CHAT, typed.into_inner()), (old::S_SET_BEACON, vec![1, 2]), (old::S_CONTAINER_CLICK, vec![0, 1, 0, 5, 0, 0, 1, 0, 5, 1, 3, 1, 0, 0]), (old::S_SET_CREATIVE_MODE_SLOT, vec![0, 36, 1, 3, 1, 0])];
        let mut system = Writer::new();
        system.string("{\"text\":\"saved\"}").varint(1);
        let mut data = Writer::new();
        data.varint(5).u8(0).varint(0).i8(0).u8(16).varint(7).bool(true).u8(8).varint(22).varint(1).u8(2).varint(5).bool(true).string("\"n\"").u8(0xff);
        let mut tags = Writer::new();
        tags.varint(2).string("minecraft:block").varint(1).string("minecraft:wool_carpets").varint(2).varint(1).varint(900).string("minecraft:game_event").varint(1).string("minecraft:vibrations").varint(1).varint(4);
        let mut commands = Writer::new();
        commands.varint(2).u8(0).varint(1).varint(1).u8(2).varint(0).string("n").varint(3).u8(3).i32(1).i32(9).varint(0);
        let clientbound = [
            (new::LOGIN, server_login("minecraft:overworld")),
            (new::SYSTEM_CHAT, system.into_inner()),
            (new::ADD_ENTITY, spawn(5, entity("painting"), 3)),
            (new::ADD_ENTITY, spawn(5, entity("frog"), 0)),
            (new::SET_ENTITY_DATA, data.into_inner()),
            (new::REMOVE_ENTITIES, vec![2, 5, 6]),
            (new::UPDATE_TAGS, tags.into_inner()),
            (new::COMMANDS, commands.into_inner()),
            (new::SOUND, vec![5, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]),
        ];
        let iterations: u64 = std::env::var("MULTIVERSION_FUZZ").ok().and_then(|v| v.parse().ok()).unwrap_or(20_000);
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ 758);
        let mut t = joined();
        for i in 0..iterations {
            let to_server = rng.below(2) == 0;
            let state = if rng.below(10) == 0 { State::Configuration } else { State::Play };
            let pkt = match (rng.below(3), to_server) {
                (0, true) => {
                    let (id, good) = &serverbound[rng.below(serverbound.len() as u64) as usize];
                    Packet::new(*id, rng.damage(good))
                }
                (0, false) => {
                    let (id, good) = &clientbound[rng.below(clientbound.len() as u64) as usize];
                    Packet::new(*id, rng.damage(good))
                }
                (1, _) => Packet::new(rng.next() as i32, rng.payload()),
                (_, true) => Packet::new(rng.below(50) as i32, rng.payload()),
                (_, false) => Packet::new(rng.below(107) as i32, rng.payload()),
            };
            let shown = format!("{} packet {:#x} {:?} in {state:?} (iteration {i})", if to_server { "serverbound" } else { "clientbound" }, pkt.id, pkt.payload);
            let started = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut out = Vec::new();
                let r = if to_server { t.serverbound(state, &mut pkt.clone(), &mut out) } else { t.clientbound(state, &mut pkt.clone(), &mut out) };
                (r, out)
            }));
            match result {
                Err(_) => panic!("translator panicked on {shown}"),
                Ok((Ok(()), out)) => {
                    let bytes: usize = out.iter().map(|p| p.payload.len()).sum();
                    assert!(bytes < 1 << 20, "{bytes} bytes sent on for {shown}");
                }
                // A refused packet ends that player's connection; start the next one fresh.
                Ok((Err(_), _)) => t = joined(),
            }
            assert!(started.elapsed().as_millis() < 2000, "took {:?} on {shown}", started.elapsed());
        }
    }
}

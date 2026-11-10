//! 1.19.3 (protocol 761) ↔ 1.19.1 / 1.19.2 (protocol 760).
//!
//! The big change between these versions is chat signing, which a proxy can't translate (see
//! `chat`): player chat reaches the client as system chat and the client's own messages and
//! commands reach the server unsigned. For the player that means messages can't be verified or
//! reported, the client's "hide messages from this player" does nothing, and a server that
//! enforces secure chat will refuse to let them talk.
//!
//! Other changes: the player list packet (1.19.3 split it into an update with a set of actions
//! and a remove packet), sounds (1.19.3 sends an id or an inline name in one packet; 1.19.1 has
//! a separate packet for names), the entity data types (1.19.3 added a long), the recipe book
//! category in recipes, explosion positions as doubles, feature flags (dropped), and everything
//! 1.19.3 added to the registries: bamboo wood, hanging signs, the chiseled bookshelf, the
//! camel (shown as a donkey) and their sounds.

pub mod chat;
pub mod ids;
pub mod items;
pub mod world;

use std::collections::HashMap;

use super::{data_1_19_1, Packet, State, TResult, Translator};
use crate::proto::buf::{Reader, Writer};
use crate::proto::version::Version;
use chat::ChatTypes;
use ids::{new, old};
use items::Kind;

pub struct V1_19_3To1_19_1 {
    /// Spawned entities whose entity data needs more than a type-by-type rewrite, and whether
    /// any entity data was sent for them yet.
    kinds: HashMap<i32, (Kind, bool)>,
    /// The server's chat types (from LOGIN), to show player chat the way the client would.
    chat_types: ChatTypes,
}

impl V1_19_3To1_19_1 {
    pub fn new() -> Self {
        Self { kinds: HashMap::new(), chat_types: ChatTypes::default() }
    }

    fn play_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            // The client has no feature flags, and can't be told to delete a message: what it
            // was shown is system chat, which has no signature to name it by.
            new::UPDATE_ENABLED_FEATURES | new::DELETE_CHAT => return Ok(()),
            new::LOGIN => {
                if let Some(registries) = chat::login_registries(p).map_err(e)? {
                    self.chat_types.load(&registries);
                }
                self.kinds.clear();
                p.to_vec()
            }
            new::PLAYER_CHAT | new::DISGUISED_CHAT => {
                let shown = if pkt.id == new::PLAYER_CHAT { chat::player_chat(&self.chat_types, p) } else { chat::disguised_chat(&self.chat_types, p) };
                out.extend(shown.map_err(e)?.map(|payload| Packet::new(old::SYSTEM_CHAT, payload)));
                return Ok(());
            }
            new::PLAYER_INFO_UPDATE => {
                out.extend(chat::player_info_update(p).map_err(e)?.into_iter().map(|payload| Packet::new(old::PLAYER_INFO, payload)));
                return Ok(());
            }
            new::PLAYER_INFO_REMOVE => chat::player_info_remove(p).map_err(e)?,
            new::SOUND | new::SOUND_ENTITY => {
                out.extend(world::sound(p, pkt.id == new::SOUND_ENTITY).map_err(e)?.map(|(id, payload)| Packet::new(id, payload)));
                return Ok(());
            }
            new::STOP_SOUND => world::stop_sound(p).map_err(e)?,
            new::ADD_ENTITY => {
                let spawn = world::add_entity(p).map_err(e)?;
                match items::entity_kind(spawn.ty) {
                    Some(k) => self.kinds.insert(spawn.entity, (k, false)),
                    None => self.kinds.remove(&spawn.entity),
                };
                spawn.payload
            }
            new::REMOVE_ENTITIES => {
                let mut r = Reader::new(p);
                for id in r.list(Reader::varint).map_err(e)? {
                    self.kinds.remove(&id);
                }
                p.to_vec()
            }
            new::RESPAWN => {
                self.kinds.clear();
                world::respawn(p).map_err(e)?
            }
            new::SET_ENTITY_DATA => {
                let mut r = Reader::new(p);
                let entity = r.varint().map_err(e)?;
                let mut w = Writer::with_capacity(p.len() + 8);
                w.varint(entity);
                let tracked = self.kinds.get_mut(&entity);
                let (kind, first) = tracked.as_ref().map_or((None, false), |(k, seen)| (Some(*k), !*seen));
                items::entity_data(kind, first, &mut r, &mut w).map_err(e)?;
                if let Some((_, seen)) = tracked {
                    *seen = true;
                }
                w.into_inner()
            }
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
            new::EXPLODE => world::explode(p).map_err(e)?,
            new::SERVER_DATA => world::server_data(p).map_err(e)?,
            new::UPDATE_TAGS => world::update_tags(p).map_err(e)?,
            new::AWARD_STATS => world::award_stats(p).map_err(e)?,
            new::COMMANDS => world::commands(p).map_err(e)?,
            new::LEVEL_PARTICLES => items::level_particles(p).map_err(e)?,
            new::SET_EQUIPMENT => items::set_equipment(p).map_err(e)?,
            new::CONTAINER_SET_CONTENT => items::container_set_content(p).map_err(e)?,
            new::CONTAINER_SET_SLOT => items::container_set_slot(p).map_err(e)?,
            new::COOLDOWN => items::cooldown(p).map_err(e)?,
            new::MERCHANT_OFFERS => items::merchant_offers(p).map_err(e)?,
            new::UPDATE_ADVANCEMENTS => items::update_advancements(p).map_err(e)?,
            new::UPDATE_RECIPES => items::update_recipes(p).map_err(e)?,
            _ => p.to_vec(),
        };
        let mapped = ids::play_clientbound(pkt.id).ok_or_else(|| format!("no 1.19.1 equivalent for clientbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }

    fn play_serverbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let e = |e: crate::proto::buf::DecodeError| format!("serverbound 0x{:02x}: {e}", pkt.id);
        let p = &pkt.payload[..];
        let payload = match pkt.id {
            // Chat previews are gone from 1.19.3, and the client's acknowledgements name
            // 1.19.1 signatures the server never sent.
            old::S_CHAT_PREVIEW | old::S_CHAT_ACK => return Ok(()),
            old::S_CHAT => chat::chat_to_server(p).map_err(e)?,
            old::S_CHAT_COMMAND => chat::chat_command_to_server(p).map_err(e)?,
            old::S_SET_CREATIVE_MODE_SLOT => items::set_creative_slot(p).map_err(e)?,
            old::S_CONTAINER_CLICK => items::container_click(p).map_err(e)?,
            _ => p.to_vec(),
        };
        let mapped = ids::play_serverbound(pkt.id).ok_or_else(|| format!("unknown 1.19.1 serverbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }
}

impl Default for V1_19_3To1_19_1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator for V1_19_3To1_19_1 {
    fn from(&self) -> Version {
        Version { protocol: 761 }
    }
    fn to(&self) -> Version {
        Version { protocol: 760 }
    }
    fn clientbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_clientbound(pkt, out),
            // neither version has a configuration phase; a newer step stands in for it
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
    use crate::proto::nbt;
    use super::data_1_19_1 as d;

    fn cb(t: &mut V1_19_3To1_19_1, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        t.clientbound(State::Play, &Packet::new(id, payload), &mut out).unwrap();
        out
    }

    fn sb(t: &mut V1_19_3To1_19_1, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        t.serverbound(State::Play, &Packet::new(id, payload), &mut out).unwrap();
        out
    }

    fn pairs(out: &[Packet]) -> Vec<(i32, Vec<u8>)> {
        out.iter().map(|p| (p.id, p.payload.clone())).collect()
    }

    /// A 1.19.3 LOGIN with the test chat types.
    fn login() -> Vec<u8> {
        let mut w = Writer::new();
        w.i32(1).bool(false).u8(0).i8(-1).varint(1).string("minecraft:overworld");
        nbt::write_named(&mut w, &chat::tests::registries());
        w.string("minecraft:overworld").string("minecraft:overworld").i64(0);
        w.varint(20).varint(10).varint(10).bool(false).bool(true).bool(false).bool(false).bool(false);
        w.into_inner()
    }

    fn entity(name: &str) -> i32 {
        d::NEW_ENTITY_NAMES.iter().position(|n| *n == name).unwrap() as i32
    }

    fn spawn(id: i32, ty: i32) -> Vec<u8> {
        let mut w = Writer::new();
        w.varint(id).uuid(1).varint(ty).f64(0.0).f64(0.0).f64(0.0).i8(0).i8(0).i8(0).varint(0).i16(0).i16(0).i16(0);
        w.into_inner()
    }

    #[test]
    fn versions() {
        let t = V1_19_3To1_19_1::new();
        assert_eq!((t.from().protocol, t.to().protocol), (761, 760));
        assert!(!t.emulates_configuration());
    }

    #[test]
    fn unchanged_packets_only_change_id() {
        let mut t = V1_19_3To1_19_1::new();
        // KEEP_ALIVE is 31 in 1.19.3 and 32 in 1.19.1; 17 and 18 serverbound
        assert_eq!(pairs(&cb(&mut t, 31, vec![1, 2, 3])), vec![(32, vec![1, 2, 3])]);
        assert_eq!(pairs(&sb(&mut t, 18, vec![1, 2, 3])), vec![(17, vec![1, 2, 3])]);
        // ids neither side has are refused
        let mut out = Vec::new();
        assert!(t.clientbound(State::Play, &Packet::new(107, vec![]), &mut out).is_err());
        assert!(t.serverbound(State::Play, &Packet::new(51, vec![]), &mut out).is_err());
        assert!(t.serverbound(State::Play, &Packet::new(-1, vec![]), &mut out).is_err());
        // configuration belongs to newer steps
        t.clientbound(State::Configuration, &Packet::new(3, vec![9]), &mut out).unwrap();
        t.serverbound(State::Configuration, &Packet::new(4, vec![8]), &mut out).unwrap();
        assert_eq!(pairs(&out), vec![(3, vec![9]), (4, vec![8])]);
    }

    #[test]
    fn player_chat_reaches_the_client_as_system_chat() {
        let mut t = V1_19_3To1_19_1::new();
        let login = login();
        assert_eq!(pairs(&cb(&mut t, new::LOGIN, login.clone())), vec![(37, login)], "LOGIN itself is unchanged");
        let out = cb(&mut t, new::PLAYER_CHAT, chat::tests::signed_chat("hello", None, 0, None));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, old::SYSTEM_CHAT);
        let mut r = Reader::new(&out[0].payload);
        let shown: serde_json::Value = serde_json::from_str(r.string().unwrap()).unwrap();
        assert_eq!(shown, serde_json::json!({"translate": "chat.type.text", "with": [{"text": "Alex"}, {"text": "hello"}]}));
        assert!(!r.bool().unwrap());
        // a chat type that isn't shown in chat produces nothing
        assert!(cb(&mut t, new::PLAYER_CHAT, chat::tests::signed_chat("x", None, 5, None)).is_empty());

        let mut w = Writer::new();
        w.string("\"hi\"").varint(0).string("\"Server\"").bool(false);
        let out = cb(&mut t, new::DISGUISED_CHAT, w.into_inner());
        assert_eq!(out[0].id, old::SYSTEM_CHAT);
        let shown: serde_json::Value = serde_json::from_str(Reader::new(&out[0].payload).string().unwrap()).unwrap();
        assert_eq!(shown, serde_json::json!({"translate": "chat.type.text", "with": ["Server", "hi"]}));
    }

    #[test]
    fn client_chat_reaches_the_server_unsigned() {
        let mut t = V1_19_3To1_19_1::new();
        let out = sb(&mut t, old::S_CHAT, chat::tests::old_chat("hello"));
        let mut want = Writer::new();
        want.string("hello").i64(1_700_000_000_000).i64(99).bool(false).varint(0).raw(&[0, 0, 0]);
        assert_eq!(pairs(&out), vec![(new::S_CHAT, want.buf)]);

        let mut w = Writer::new();
        w.string("say hi").i64(5).i64(6).varint(0).bool(false).varint(0).bool(false);
        let out = sb(&mut t, old::S_CHAT_COMMAND, w.into_inner());
        let mut want = Writer::new();
        want.string("say hi").i64(5).i64(6).varint(0).varint(0).raw(&[0, 0, 0]);
        assert_eq!(pairs(&out), vec![(new::S_CHAT_COMMAND, want.buf)]);

        // previews and acknowledgements stay here
        assert!(sb(&mut t, old::S_CHAT_PREVIEW, vec![0, 0, 0, 1, 2, b'h', b'i']).is_empty());
        assert!(sb(&mut t, old::S_CHAT_ACK, vec![0, 0]).is_empty());
    }

    #[test]
    fn packets_the_client_cannot_use_are_dropped() {
        let mut t = V1_19_3To1_19_1::new();
        let mut features = Writer::new();
        features.varint(1).string("minecraft:vanilla");
        assert!(cb(&mut t, new::UPDATE_ENABLED_FEATURES, features.into_inner()).is_empty());
        assert!(cb(&mut t, new::DELETE_CHAT, vec![4]).is_empty());
    }

    #[test]
    fn player_info_and_sounds_change_packet() {
        let mut t = V1_19_3To1_19_1::new();
        let mut p = Writer::new();
        p.u8(0x10 | 0x04).varint(1).uuid(9).varint(1).varint(50);
        let out = cb(&mut t, new::PLAYER_INFO_UPDATE, p.into_inner());
        let mut mode = Writer::new();
        mode.varint(1).varint(1).uuid(9).varint(1);
        let mut latency = Writer::new();
        latency.varint(2).varint(1).uuid(9).varint(50);
        assert_eq!(pairs(&out), vec![(old::PLAYER_INFO, mode.buf), (old::PLAYER_INFO, latency.buf)]);
        let mut p = Writer::new();
        p.varint(1).uuid(9);
        let mut want = Writer::new();
        want.varint(4).varint(1).uuid(9);
        assert_eq!(pairs(&cb(&mut t, new::PLAYER_INFO_REMOVE, p.into_inner())), vec![(old::PLAYER_INFO, want.buf)]);

        let mut p = Writer::new();
        p.varint(0).string("mypack:boom").bool(false).varint(0).i32(1).i32(2).i32(3).f32(1.0).f32(1.0).i64(4);
        let out = cb(&mut t, new::SOUND, p.into_inner());
        assert_eq!(out[0].id, old::CUSTOM_SOUND);
        let mut p = Writer::new();
        p.varint(1).varint(0).varint(7).f32(1.0).f32(1.0).i64(4);
        let out = cb(&mut t, new::SOUND_ENTITY, p.into_inner());
        assert_eq!(out[0].id, old::SOUND_ENTITY);
        assert_eq!(out[0].payload[0], 0, "the first sound, without the holder's offset");
    }

    #[test]
    fn camels_are_tracked_from_spawn_to_removal() {
        let mut t = V1_19_3To1_19_1::new();
        let out = cb(&mut t, new::ADD_ENTITY, spawn(5, entity("camel")));
        assert_eq!(out[0].id, 0);
        let donkey = d::OLD_ENTITY_NAMES.iter().position(|n| *n == "donkey").unwrap() as u8;
        assert_eq!(out[0].payload[17], donkey);
        let data = || {
            let mut w = Writer::new();
            w.varint(5).u8(19).varint(8).bool(true).u8(0xff);
            w.into_inner()
        };
        let mut named = Writer::new();
        named.varint(5).u8(2).varint(5).bool(true).string(r#"{"text":"Camel"}"#).u8(0xff);
        // the first entity data names the stand-in, later ones don't repeat it
        assert_eq!(pairs(&cb(&mut t, new::SET_ENTITY_DATA, data())), vec![(80, named.buf.clone())]);
        assert_eq!(pairs(&cb(&mut t, new::SET_ENTITY_DATA, data())), vec![(80, vec![5, 0xff])]);
        // once removed, the id is anyone's: a zombie keeps its index 19
        cb(&mut t, new::REMOVE_ENTITIES, vec![1, 5]);
        cb(&mut t, new::ADD_ENTITY, spawn(5, entity("zombie")));
        assert_eq!(pairs(&cb(&mut t, new::SET_ENTITY_DATA, data())), vec![(80, vec![5, 19, 7, 1, 0xff])]);
        // and a respawn forgets every entity
        cb(&mut t, new::ADD_ENTITY, spawn(6, entity("camel")));
        let mut respawn = Writer::new();
        respawn.string("minecraft:overworld").string("minecraft:overworld").i64(7).u8(1).i8(-1).bool(false).bool(false).u8(1).bool(false);
        assert_eq!(cb(&mut t, new::RESPAWN, respawn.into_inner())[0].id, 62);
        let mut w = Writer::new();
        w.varint(6).u8(19).varint(8).bool(true).u8(0xff);
        assert_eq!(pairs(&cb(&mut t, new::SET_ENTITY_DATA, w.into_inner())), vec![(80, vec![6, 19, 7, 1, 0xff])]);
    }

    #[test]
    fn item_packets_are_translated_both_ways() {
        let mut t = V1_19_3To1_19_1::new();
        let (new_id, old_id) = (items::tests::new_item("shears"), items::tests::old_item("shears"));
        let mut p = Writer::new();
        p.u8(0).varint(1).i16(36).raw(&items::tests::stack(new_id, 1, None));
        let mut want = Writer::new();
        want.u8(0).varint(1).i16(36).raw(&items::tests::stack(old_id, 1, None));
        assert_eq!(pairs(&cb(&mut t, new::CONTAINER_SET_SLOT, p.into_inner())), vec![(19, want.buf)]);
        let mut p = Writer::new();
        p.i16(36).raw(&items::tests::stack(old_id, 1, None));
        let mut want = Writer::new();
        want.i16(36).raw(&items::tests::stack(new_id, 1, None));
        assert_eq!(pairs(&sb(&mut t, old::S_SET_CREATIVE_MODE_SLOT, p.into_inner())), vec![(43, want.buf)]);
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
    }

    /// Runs one packet through the translator; a panic fails the test, an error is fine.
    fn survives(t: &mut V1_19_3To1_19_1, clientbound: bool, id: i32, payload: Vec<u8>) {
        let shown = format!("{} packet {id:#x} {payload:?}", if clientbound { "clientbound" } else { "serverbound" });
        let pkt = Packet::new(id, payload);
        let started = std::time::Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut out = Vec::new();
            let r = if clientbound { t.clientbound(State::Play, &pkt, &mut out) } else { t.serverbound(State::Play, &pkt, &mut out) };
            r.map(|()| out)
        }));
        match result {
            Err(_) => panic!("translator panicked on {shown}"),
            Ok(Ok(out)) => {
                let bytes: usize = out.iter().map(|p| p.payload.len()).sum();
                assert!(bytes < 1 << 20, "{bytes} bytes written for {shown}");
            }
            Ok(Err(_)) => {}
        }
        assert!(started.elapsed().as_millis() < 2000, "took {:?} on {shown}", started.elapsed());
    }

    #[test]
    fn random_packets_never_panic() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ 760);
        let mut t = V1_19_3To1_19_1::new();
        cb(&mut t, new::LOGIN, login());
        cb(&mut t, new::ADD_ENTITY, spawn(0, entity("camel")));
        cb(&mut t, new::ADD_ENTITY, spawn(1, entity("minecart")));
        let iterations = std::env::var("MULTIVERSION_FUZZ").ok().and_then(|v| v.parse().ok()).unwrap_or(60_000);
        for _ in 0..iterations {
            let clientbound = rng.below(2) == 0;
            let id = if rng.below(20) == 0 { rng.next() as i32 } else { rng.below(if clientbound { 108 } else { 52 }) as i32 };
            let payload = rng.payload();
            survives(&mut t, clientbound, id, payload);
        }
    }

    /// Well-formed packets cut short at every length and with single bytes changed: closer to
    /// the deep ends of the readers than random bytes get.
    #[test]
    fn damaged_packets_never_panic() {
        let mut rng = Rng(0x0123_4567_89AB_CDEF);
        let shears = items::tests::stack(items::tests::new_item("shears"), 1, None);
        let planks = items::tests::stack(items::tests::new_item("bamboo_planks"), 1, None);
        let mut clientbound: Vec<(i32, Vec<u8>)> = vec![(new::LOGIN, login()), (new::PLAYER_CHAT, chat::tests::signed_chat("hello", Some("\"hi\""), 3, Some("\"Steve\"")))];
        let mut w = Writer::new();
        w.u8(0x3f).varint(1).uuid(1).string("Alex").varint(1).string("a").string("b").bool(false).bool(false).varint(0).bool(true).varint(1).bool(false);
        clientbound.push((new::PLAYER_INFO_UPDATE, w.into_inner()));
        let mut w = Writer::new();
        w.u8(0).varint(1).varint(2).raw(&shears).raw(&planks).raw(&planks);
        clientbound.push((new::CONTAINER_SET_CONTENT, w.into_inner()));
        let mut w = Writer::new();
        w.varint(0).u8(8).varint(7).raw(&planks).u8(6).varint(19).varint(10).u8(17).varint(16).varint(2).varint(5).u8(0xff);
        clientbound.push((new::SET_ENTITY_DATA, w.into_inner()));
        let mut w = Writer::new();
        w.varint(2).string("minecraft:crafting_shaped").string("a").varint(1).varint(1).string("").varint(0).varint(1).raw(&planks).raw(&shears);
        w.string("minecraft:stonecutting").string("b").string("").varint(1).raw(&shears).raw(&planks);
        clientbound.push((new::UPDATE_RECIPES, w.into_inner()));
        let mut w = Writer::new();
        w.i32(0).i32(0).u8(0);
        let mut sec = Writer::new();
        sec.i16(1).u8(15).long_array(&[3; 1024]).u8(0).varint(0).varint(0);
        sec.i16(1).u8(4).varint(1).varint(9).long_array(&[0; 256]).u8(2).varint(1).varint(0).long_array(&[0]);
        w.byte_array(&sec.buf).varint(1).u8(0).i16(1).varint(8).raw(&[10, 0, 0, 0]).raw(&[1, 0, 0, 0, 0, 0, 0]);
        clientbound.push((new::LEVEL_CHUNK_WITH_LIGHT, w.into_inner()));
        let mut w = Writer::new();
        w.varint(2).u8(0).varint(1).varint(1).u8(2 | 0x10).varint(0).string("x").varint(1).u8(3).f32(0.0).f32(1.0).string("s").varint(0);
        clientbound.push((new::COMMANDS, w.into_inner()));
        let mut w = Writer::new();
        w.varint(1).string("minecraft:item").varint(1).string("t").varint(2).varint(3).varint(4);
        clientbound.push((new::UPDATE_TAGS, w.into_inner()));

        let mut serverbound: Vec<(i32, Vec<u8>)> = vec![(old::S_CHAT, chat::tests::old_chat("hello"))];
        let mut stand_in = Writer::new();
        items::item(&mut Reader::new(&planks), &mut stand_in).unwrap();
        let mut w = Writer::new();
        w.u8(0).varint(1).i16(5).i8(0).varint(0).varint(1).i16(5).raw(&stand_in.buf).raw(&stand_in.buf);
        serverbound.push((old::S_CONTAINER_CLICK, w.into_inner()));
        let mut w = Writer::new();
        w.i16(5).raw(&stand_in.buf);
        serverbound.push((old::S_SET_CREATIVE_MODE_SLOT, w.into_inner()));

        let mut t = V1_19_3To1_19_1::new();
        for (is_clientbound, packets) in [(true, &clientbound), (false, &serverbound)] {
            for (id, good) in packets {
                // the untouched packet is valid
                let mut out = Vec::new();
                let r = if is_clientbound { t.clientbound(State::Play, &Packet::new(*id, good.clone()), &mut out) } else { t.serverbound(State::Play, &Packet::new(*id, good.clone()), &mut out) };
                r.unwrap_or_else(|e| panic!("packet {id:#x} should translate: {e}"));
                for len in 0..good.len().min(600) {
                    survives(&mut t, is_clientbound, *id, good[..len].to_vec());
                }
                for _ in 0..3000 {
                    let mut bad = good.clone();
                    for _ in 0..=rng.below(3) {
                        let at = rng.below(bad.len() as u64) as usize;
                        bad[at] = rng.byte();
                    }
                    survives(&mut t, is_clientbound, *id, bad);
                }
            }
        }
    }
}

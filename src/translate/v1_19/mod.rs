//! 1.19.1 / 1.19.2 (protocol 760) ↔ 1.19 (protocol 759).
//!
//! The two versions share every registry; what changed is chat. 1.19.1 signs messages as a
//! chain (each one names the messages seen before it) where 1.19 signs them one by one, and
//! neither kind of signature can be made from the other. So, as ViaBackwards does without the
//! player's key: the client's chat and commands go to the server unsigned, player chat from the
//! server is shown as a system message decorated with the server's chat type, and this step
//! reports the "last seen" messages a 1.19.1 client would (in chat packets and, when the player
//! stays quiet, with a CHAT_ACK of its own through `take_replies`).
//!
//! Other changes: LOGIN carries the 1.19 chat type registry, SYSTEM_CHAT names a chat type
//! instead of an action bar flag, SERVER_DATA loses "enforces secure chat", the allay loses its
//! two new entity data fields, and the packets 1.19 lacks (custom chat completions, delete chat,
//! player chat header) are dropped.

pub mod chat;
pub mod entity;
pub mod ids;

use std::collections::HashSet;

use super::{Packet, State, TResult, Translator};
use crate::proto::buf::Reader;
use crate::proto::version::Version;
use ids::{new, old};

pub struct V1_19_1To1_19 {
    /// The server's chat types, from LOGIN.
    chat_types: chat::ChatTypes,
    /// Signed messages the client was shown, for the "last seen" lists.
    seen: chat::Seen,
    /// Spawned allays: their entity data has fields 1.19 lacks.
    allays: HashSet<i32>,
    /// Packets for the server, sent on the client's behalf.
    replies: Vec<(State, Packet)>,
}

impl V1_19_1To1_19 {
    pub fn new() -> Self {
        Self { chat_types: chat::ChatTypes::default(), seen: chat::Seen::default(), allays: HashSet::new(), replies: Vec::new() }
    }

    fn play_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            // Nothing in 1.19 can show these: completions and deletions belong to the 1.19.1
            // chat window, and headers only matter to a client that checks the signature chain.
            new::CUSTOM_CHAT_COMPLETIONS | new::DELETE_CHAT | new::PLAYER_CHAT_HEADER => return Ok(()),
            new::PLAYER_CHAT => {
                let converted = chat::player_chat(p, &self.chat_types, &mut self.seen).map_err(e)?;
                if let Some(ack) = converted.ack {
                    self.replies.push((State::Play, Packet::new(new::S_CHAT_ACK, ack)));
                }
                out.extend(converted.system_chat.map(|payload| Packet::new(old::SYSTEM_CHAT, payload)));
                return Ok(());
            }
            new::SYSTEM_CHAT => chat::system_chat(p).map_err(e)?,
            new::SERVER_DATA => chat::server_data(p).map_err(e)?,
            new::LOGIN => {
                self.allays.clear();
                chat::login(p, &mut self.chat_types).map_err(e)?
            }
            new::RESPAWN => {
                self.allays.clear();
                p.to_vec()
            }
            new::ADD_ENTITY => {
                let (id, ty) = entity::spawned(p).map_err(e)?;
                if ty == entity::ALLAY {
                    self.allays.insert(id);
                } else {
                    self.allays.remove(&id);
                }
                p.to_vec()
            }
            // an id that was an allay's may come back as something else
            new::ADD_PLAYER | new::ADD_EXPERIENCE_ORB => {
                let id = Reader::new(p).varint().map_err(e)?;
                self.allays.remove(&id);
                p.to_vec()
            }
            new::REMOVE_ENTITIES => {
                for id in Reader::new(p).list(Reader::varint).map_err(e)? {
                    self.allays.remove(&id);
                }
                p.to_vec()
            }
            new::SET_ENTITY_DATA => {
                let id = Reader::new(p).varint().map_err(e)?;
                if self.allays.contains(&id) { entity::allay_data(p).map_err(e)? } else { p.to_vec() }
            }
            _ => p.to_vec(),
        };
        let mapped = ids::play_clientbound(pkt.id).ok_or_else(|| format!("no 1.19 equivalent for clientbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }

    fn play_serverbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("serverbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            old::S_CHAT => chat::chat(p, &mut self.seen).map_err(e)?,
            old::S_CHAT_COMMAND => chat::chat_command(p, &mut self.seen).map_err(e)?,
            _ => p.to_vec(),
        };
        let mapped = ids::play_serverbound(pkt.id).ok_or_else(|| format!("unknown 1.19 serverbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }
}

impl Default for V1_19_1To1_19 {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator for V1_19_1To1_19 {
    fn from(&self) -> Version {
        Version { protocol: 760 }
    }
    fn to(&self) -> Version {
        Version { protocol: 759 }
    }
    fn clientbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_clientbound(pkt, out),
            // neither version has a configuration phase: a newer step stands in for it
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
    fn take_replies(&mut self) -> Vec<(State, Packet)> {
        std::mem::take(&mut self.replies)
    }
}

#[cfg(test)]
mod tests {
    use super::chat::tests::{player_chat_packet, registry_1_19_1, server_login, Chat};
    use super::*;
    use crate::proto::buf::Writer;
    use crate::proto::nbt;

    fn cb(t: &mut V1_19_1To1_19, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        t.clientbound(State::Play, &Packet::new(id, payload), &mut out).unwrap();
        out
    }

    fn sb(t: &mut V1_19_1To1_19, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        t.serverbound(State::Play, &Packet::new(id, payload), &mut out).unwrap();
        out
    }

    fn joined() -> V1_19_1To1_19 {
        let mut t = V1_19_1To1_19::new();
        cb(&mut t, new::LOGIN, server_login(&registry_1_19_1()));
        t
    }

    #[test]
    fn versions() {
        let t = V1_19_1To1_19::new();
        assert_eq!((t.from().protocol, t.to().protocol), (760, 759));
        assert!(!t.emulates_configuration());
    }

    #[test]
    fn login_carries_the_1_19_chat_types() {
        let mut t = V1_19_1To1_19::new();
        let out = cb(&mut t, new::LOGIN, server_login(&registry_1_19_1()));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, 35);
        let mut r = Reader::new(&out[0].payload);
        r.i32().unwrap();
        r.bytes(3).unwrap();
        r.list(|r| r.string().map(|_| ())).unwrap();
        let registries = nbt::read_named_optional(&mut r).unwrap().unwrap();
        assert_eq!(registries.get("minecraft:chat_type"), Some(&chat::registry_1_19()));
    }

    #[test]
    fn player_chat_becomes_system_chat() {
        let mut t = joined();
        let out = cb(&mut t, new::PLAYER_CHAT, player_chat_packet(&Chat { chat_type: 6, ..Chat::default() }));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, old::SYSTEM_CHAT);
        let mut r = Reader::new(&out[0].payload);
        let json: serde_json::Value = serde_json::from_str(r.string().unwrap()).unwrap();
        assert_eq!(json, serde_json::json!({"translate": "chat.type.emote", "with": [{"text": "Steve"}, {"text": "hi"}]}));
        assert_eq!(r.varint().unwrap(), chat::SYSTEM);
        assert!(r.is_empty());
        assert!(t.take_replies().is_empty());
        // a type with nothing to show in chat reaches nobody
        assert!(cb(&mut t, new::PLAYER_CHAT, player_chat_packet(&Chat { chat_type: 7, ..Chat::default() })).is_empty());
    }

    /// What the client was shown is reported to the server: with the client's next message, or
    /// by this step when the client says nothing for a long time.
    #[test]
    fn seen_messages_are_reported() {
        let mut t = joined();
        cb(&mut t, new::PLAYER_CHAT, player_chat_packet(&Chat { sender: 1, signature: &[1; 4], ..Chat::default() }));
        cb(&mut t, new::PLAYER_CHAT, player_chat_packet(&Chat { sender: 2, signature: &[2; 4], ..Chat::default() }));
        let mut w = Writer::new();
        w.string("hello").i64(1000).i64(5).byte_array(&[8; 256]).bool(false);
        let out = sb(&mut t, old::S_CHAT, w.into_inner());
        let mut want = Writer::new();
        want.string("hello").i64(1000).i64(5).varint(0).bool(false);
        want.varint(2).uuid(2).byte_array(&[2; 4]).uuid(1).byte_array(&[1; 4]).bool(false);
        assert_eq!((out[0].id, &out[0].payload), (new::S_CHAT, &want.buf));

        let mut acks = Vec::new();
        for _ in 0..200 {
            cb(&mut t, new::PLAYER_CHAT, player_chat_packet(&Chat { sender: 3, signature: &[3; 4], ..Chat::default() }));
            acks.extend(t.take_replies());
        }
        assert_eq!(acks.len(), 3);
        let mut want = Writer::new();
        want.varint(3).uuid(3).byte_array(&[3; 4]).uuid(2).byte_array(&[2; 4]).uuid(1).byte_array(&[1; 4]).bool(false);
        for (state, ack) in acks {
            assert_eq!((state, ack.id, &ack.payload), (State::Play, new::S_CHAT_ACK, &want.buf));
        }
    }

    #[test]
    fn commands_go_to_the_server_unsigned() {
        let mut t = joined();
        let mut w = Writer::new();
        w.string("msg Alex hi").i64(1000).i64(5).varint(1).string("message").byte_array(&[8; 256]).bool(false);
        let out = sb(&mut t, old::S_CHAT_COMMAND, w.into_inner());
        let mut want = Writer::new();
        want.string("msg Alex hi").i64(1000).i64(5).varint(1).string("message").varint(0).bool(false).varint(0).bool(false);
        assert_eq!((out[0].id, &out[0].payload), (new::S_CHAT_COMMAND, &want.buf));
    }

    #[test]
    fn system_chat_and_server_data() {
        let mut t = joined();
        let mut w = Writer::new();
        w.string("{\"text\":\"saved\"}").bool(true);
        let out = cb(&mut t, new::SYSTEM_CHAT, w.into_inner());
        let mut want = Writer::new();
        want.string("{\"text\":\"saved\"}").varint(chat::GAME_INFO);
        assert_eq!((out[0].id, &out[0].payload), (old::SYSTEM_CHAT, &want.buf));

        let mut w = Writer::new();
        w.bool(false).bool(false).bool(false);
        let want = w.buf.clone();
        w.bool(true);
        let out = cb(&mut t, new::SERVER_DATA, w.into_inner());
        assert_eq!((out[0].id, &out[0].payload), (63, &want));
    }

    #[test]
    fn packets_1_19_lacks_are_dropped() {
        let mut t = joined();
        for id in [new::CUSTOM_CHAT_COMPLETIONS, new::DELETE_CHAT, new::PLAYER_CHAT_HEADER] {
            assert!(cb(&mut t, id, vec![1, 2, 3]).is_empty());
        }
        assert!(t.take_replies().is_empty());
    }

    #[test]
    fn other_packets_only_change_id() {
        let mut t = joined();
        // KEEP_ALIVE both ways
        let out = cb(&mut t, 32, vec![0; 8]);
        assert_eq!((out[0].id, out[0].payload.len()), (30, 8));
        let out = sb(&mut t, 17, vec![0; 8]);
        assert_eq!((out[0].id, out[0].payload.len()), (18, 8));
        // ids neither version has are refused
        let mut out = Vec::new();
        assert!(t.clientbound(State::Play, &Packet::new(108, Vec::new()), &mut out).is_err());
        assert!(t.serverbound(State::Play, &Packet::new(50, Vec::new()), &mut out).is_err());
        assert!(out.is_empty());
        // there is no configuration phase to translate
        t.clientbound(State::Configuration, &Packet::new(new::PLAYER_CHAT, vec![1]), &mut out).unwrap();
        t.serverbound(State::Configuration, &Packet::new(old::S_CHAT, vec![1]), &mut out).unwrap();
        assert_eq!(out.iter().map(|p| (p.id, p.payload.clone())).collect::<Vec<_>>(), vec![(new::PLAYER_CHAT, vec![1]), (old::S_CHAT, vec![1])]);
    }

    fn spawn(entity: i32, ty: i32) -> Vec<u8> {
        let mut w = Writer::new();
        w.varint(entity).uuid(9).varint(ty).f64(0.0).f64(64.0).f64(0.0).i8(0).i8(0).i8(0).varint(0).i16(0).i16(0).i16(0);
        w.into_inner()
    }

    fn dancing(entity: i32) -> Vec<u8> {
        let mut w = Writer::new();
        w.varint(entity).u8(16).varint(7).bool(true).u8(0xff);
        w.into_inner()
    }

    #[test]
    fn only_allays_lose_their_new_fields() {
        let mut t = joined();
        let stripped = |entity: u8| vec![entity, 0xff];
        let out = cb(&mut t, new::ADD_ENTITY, spawn(5, entity::ALLAY));
        assert_eq!((out[0].id, &out[0].payload), (0, &spawn(5, entity::ALLAY)));
        cb(&mut t, new::ADD_ENTITY, spawn(6, 50));
        let out = cb(&mut t, new::SET_ENTITY_DATA, dancing(5));
        assert_eq!((out[0].id, &out[0].payload), (77, &stripped(5)));
        assert_eq!(cb(&mut t, new::SET_ENTITY_DATA, dancing(6))[0].payload, dancing(6));

        // removed, respawned or replaced entities are forgotten
        let mut w = Writer::new();
        w.varint(1).varint(5);
        cb(&mut t, new::REMOVE_ENTITIES, w.into_inner());
        assert_eq!(cb(&mut t, new::SET_ENTITY_DATA, dancing(5))[0].payload, dancing(5));
        cb(&mut t, new::ADD_ENTITY, spawn(5, entity::ALLAY));
        cb(&mut t, new::ADD_PLAYER, vec![5; 40]);
        assert_eq!(cb(&mut t, new::SET_ENTITY_DATA, dancing(5))[0].payload, dancing(5));
        cb(&mut t, new::ADD_ENTITY, spawn(5, entity::ALLAY));
        cb(&mut t, new::RESPAWN, vec![0; 30]);
        assert_eq!(cb(&mut t, new::SET_ENTITY_DATA, dancing(5))[0].payload, dancing(5));
        cb(&mut t, new::ADD_ENTITY, spawn(5, entity::ALLAY));
        cb(&mut t, new::ADD_ENTITY, spawn(5, 50));
        assert_eq!(cb(&mut t, new::SET_ENTITY_DATA, dancing(5))[0].payload, dancing(5));
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
        let mut chat_message = Writer::new();
        chat_message.string("hello").i64(1000).i64(5).byte_array(&[8; 64]).bool(false);
        let mut command = Writer::new();
        command.string("msg Alex hi").i64(1000).i64(5).varint(2).string("a").byte_array(&[8; 16]).string("b").byte_array(&[]).bool(false);
        let serverbound = [(old::S_CHAT, chat_message.into_inner()), (old::S_CHAT_COMMAND, command.into_inner())];

        let mut system = Writer::new();
        system.string("{\"text\":\"saved\"}").bool(false);
        let mut server_data = Writer::new();
        server_data.bool(true).string("\"motd\"").bool(true).string("icon").bool(false).bool(true);
        let mut data = Writer::new();
        data.varint(5).u8(0).varint(0).i8(0).u8(16).varint(7).bool(true).u8(8).varint(6).bool(true).varint(1).i8(1).u8(0).u8(0xff);
        let clientbound = [
            (new::LOGIN, server_login(&registry_1_19_1())),
            (new::PLAYER_CHAT, player_chat_packet(&Chat { decorated: Some("{\"text\":\"d\"}"), unsigned: Some("\"u\""), filter_mask: Some(&[1]), target_name: Some("\"t\""), ..Chat::default() })),
            (new::SYSTEM_CHAT, system.into_inner()),
            (new::SERVER_DATA, server_data.into_inner()),
            (new::ADD_ENTITY, spawn(5, entity::ALLAY)),
            (new::SET_ENTITY_DATA, data.into_inner()),
            (new::REMOVE_ENTITIES, vec![2, 5, 6]),
        ];

        let iterations: u64 = std::env::var("MULTIVERSION_FUZZ").ok().and_then(|v| v.parse().ok()).unwrap_or(20_000);
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ 759);
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
                // the ids of an allay's data and of the chat packets come up often this way
                (_, true) => Packet::new(rng.below(52) as i32, rng.payload()),
                (_, false) => Packet::new(rng.below(110) as i32, rng.payload()),
            };
            let shown = format!("{} packet {:#x} {:?} in {state:?} (iteration {i})", if to_server { "serverbound" } else { "clientbound" }, pkt.id, pkt.payload);
            let started = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut out = Vec::new();
                let r = if to_server { t.serverbound(state, &pkt, &mut out) } else { t.clientbound(state, &pkt, &mut out) };
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
            let replies: usize = t.take_replies().iter().map(|(_, p)| p.payload.len()).sum();
            assert!(replies < 1 << 16, "{replies} bytes of replies for {shown}");
            assert!(started.elapsed().as_millis() < 2000, "took {:?} on {shown}", started.elapsed());
        }
    }
}

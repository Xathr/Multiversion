//! Protocol translation between the client's version and Pumpkin's (26.3).
//!
//! A `Translator` converts packets one version step (e.g. 26.3 ↔ 26.2). A `Chain` stacks the
//! steps needed for a given client, newest first for clientbound and oldest first for
//! serverbound, one version step at a time.

pub mod table;
pub mod data_26_2;
pub mod data_26_1;
pub mod data_1_21_11;
pub mod data_1_21_9;
pub mod data_1_21_7;
pub mod data_1_21_6;
pub mod data_1_21_5;
pub mod data_1_21_4;
pub mod data_1_21_2;
pub mod data_1_21;
pub mod data_1_20_5;
pub mod data_1_20_3;
pub mod data_1_20_2;
pub mod data_1_20;
pub mod data_1_19_4;
pub mod data_1_19_3;
pub mod v26_2;
pub mod v26_1;
pub mod v1_21;
pub mod v1_21_4;
pub mod v1_21_5;
pub mod v1_21_7;
pub mod v1_21_2;
pub mod v1_21_9;
pub mod v1_21_11;
pub mod v1_21_6;
pub mod v1_20_5;
pub mod v1_20_3;
pub mod v1_20_2;
pub mod v1_20;
pub mod v1_19_4;
pub mod v1_19_3;
pub mod v1_19;
#[cfg(test)]
mod fuzz_tests;

use crate::proto::version::{Version, BACKEND, KNOWN};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Configuration,
    Play,
}

/// A packet in some protocol version: id + payload.
#[derive(Debug, Clone)]
pub struct Packet {
    pub id: i32,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn new(id: i32, payload: Vec<u8>) -> Self {
        Self { id, payload }
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut w = crate::proto::buf::Writer::with_capacity(self.payload.len() + 5);
        w.varint(self.id);
        w.raw(&self.payload);
        w.into_inner()
    }
}

pub type TResult = Result<(), String>;

/// One version step. `from` is the newer version (server side), `to` the older (client side).
pub trait Translator {
    fn from(&self) -> Version;
    fn to(&self) -> Version;
    /// Server → client: input is a `from`-version packet, output `to`-version packets (0..n).
    fn clientbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult;
    /// Client → server: input is a `to`-version packet, output `from`-version packets (0..n).
    fn serverbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult;
    /// Packets this step sends to the server by itself, on the client's behalf (`from`-version
    /// packets, each with the state it is sent in). The chain collects them after every call.
    fn take_replies(&mut self) -> Vec<(State, Packet)> {
        Vec::new()
    }
    /// The client has no configuration phase (1.20.1 and older): the proxy acknowledges login
    /// to the server itself and this step stands in for the client during configuration.
    fn emulates_configuration(&self) -> bool {
        false
    }
    /// The proxy acknowledged login on the client's behalf (steps that emulate configuration).
    fn login_acknowledged(&mut self) {}
}

/// All steps for one client, newest first.
pub struct Chain {
    steps: Vec<Box<dyn Translator>>,
    pub client: Version,
    /// Steps' own packets for the server, already translated to the backend version.
    replies: Vec<Packet>,
}

impl Chain {
    /// Builds the chain for `client`, or `None` if the version is not supported.
    pub fn for_client(client: Version) -> Option<Chain> {
        if client == BACKEND {
            return Some(Chain { steps: Vec::new(), client, replies: Vec::new() });
        }
        let mut steps: Vec<Box<dyn Translator>> = Vec::new();
        let mut current = BACKEND;
        while current != client {
            let step = make_step(current)?;
            current = step.to();
            steps.push(step);
        }
        Some(Chain { steps, client, replies: Vec::new() })
    }

    pub fn is_passthrough(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn clientbound(&mut self, state: State, pkt: Packet) -> Result<Vec<Packet>, String> {
        let mut current = vec![pkt];
        for i in 0..self.steps.len() {
            let mut next = Vec::with_capacity(current.len());
            for p in &current {
                self.steps[i].clientbound(state, p, &mut next)?;
            }
            current = next;
            self.collect_replies(i)?;
        }
        Ok(current)
    }

    pub fn serverbound(&mut self, state: State, pkt: Packet) -> Result<Vec<Packet>, String> {
        let mut current = vec![pkt];
        for step in self.steps.iter_mut().rev() {
            let mut next = Vec::with_capacity(current.len());
            for p in &current {
                step.serverbound(state, p, &mut next)?;
            }
            current = next;
        }
        Ok(current)
    }

    /// Takes step `i`'s own packets up through the newer steps, towards the server.
    fn collect_replies(&mut self, i: usize) -> TResult {
        for (state, pkt) in self.steps[i].take_replies() {
            let mut current = vec![pkt];
            for step in self.steps[..i].iter_mut().rev() {
                let mut next = Vec::with_capacity(current.len());
                for p in &current {
                    step.serverbound(state, p, &mut next)?;
                }
                current = next;
            }
            self.replies.append(&mut current);
        }
        Ok(())
    }

    /// Packets for the server that the steps sent on the client's behalf (backend version),
    /// in order; empty for clients that answer everything themselves.
    pub fn take_replies(&mut self) -> Vec<Packet> {
        std::mem::take(&mut self.replies)
    }

    /// The client has no configuration phase: the proxy must acknowledge login itself and call
    /// `login_acknowledged`.
    pub fn emulates_configuration(&self) -> bool {
        self.steps.iter().any(|s| s.emulates_configuration())
    }

    /// Login was acknowledged to the server on the client's behalf; returns what the steps send
    /// to start configuration (backend version).
    pub fn login_acknowledged(&mut self) -> Result<Vec<Packet>, String> {
        for i in 0..self.steps.len() {
            self.steps[i].login_acknowledged();
            self.collect_replies(i)?;
        }
        Ok(self.take_replies())
    }
}


/// The translator that takes `from` one step down, if implemented.
fn make_step(from: Version) -> Option<Box<dyn Translator>> {
    match from.protocol {
        777 => Some(Box::new(v26_2::V26_3To26_2::new())),
        776 => Some(Box::new(v26_1::V26_2To26_1::new())),
        768 => Some(Box::new(v1_21::V1_21_2To1_21::new())),
        770 => Some(Box::new(v1_21_4::V1_21_5To1_21_4::new())),
        771 => Some(Box::new(v1_21_5::V1_21_6To1_21_5::new())),
        773 => Some(Box::new(v1_21_7::V1_21_9To1_21_7::new())),
        769 => Some(Box::new(v1_21_2::V1_21_4To1_21_2::new())),
        774 => Some(Box::new(v1_21_9::V1_21_11To1_21_9::new())),
        775 => Some(Box::new(v1_21_11::V26_1To1_21_11::new())),
        772 => Some(Box::new(v1_21_6::V1_21_7To1_21_6::new())),
        767 => Some(Box::new(v1_20_5::V1_21To1_20_5::new())),
        766 => Some(Box::new(v1_20_3::V1_20_5To1_20_3::new())),
        765 => Some(Box::new(v1_20_2::V1_20_3To1_20_2::new())),
        764 => Some(Box::new(v1_20::V1_20_2To1_20::new())),
        763 => Some(Box::new(v1_19_4::V1_20To1_19_4::new())),
        762 => Some(Box::new(v1_19_3::V1_19_4To1_19_3::new())),
        _ => None,
    }
}

/// Every protocol `Chain::for_client` can build, newest first. A step registered below a gap
/// (one whose newer neighbour isn't implemented yet) is not reachable and not counted.
fn reachable() -> &'static [i32] {
    static REACHABLE: std::sync::OnceLock<Vec<i32>> = std::sync::OnceLock::new();
    REACHABLE.get_or_init(|| {
        let mut out = vec![BACKEND.protocol];
        let mut v = BACKEND;
        while let Some(step) = make_step(v) {
            v = step.to();
            out.push(v.protocol);
        }
        out
    })
}

/// Lowest client version a chain exists for.
pub fn lowest_supported() -> Version {
    Version { protocol: *reachable().last().expect("backend is always reachable") }
}

pub fn is_supported(client: Version) -> bool {
    KNOWN.iter().any(|k| k.protocol == client.protocol) && reachable().contains(&client.protocol)
}

pub fn supported_range() -> String {
    let low = lowest_supported();
    if low == BACKEND { BACKEND.name() } else { format!("{}-{}", low.name(), BACKEND.name()) }
}

/// Versions that both have a translator and pass the config's allow/deny lists, newest first.
pub fn allowed_versions(cfg: &crate::config::Versions) -> Vec<Version> {
    KNOWN.iter().map(|k| Version { protocol: k.protocol }).filter(|v| is_supported(*v) && cfg.permits(*v)).collect()
}

/// Human-readable list of the allowed versions, contiguous runs collapsed ("1.21-1.21.4, 26.2-26.3").
pub fn allowed_range(cfg: &crate::config::Versions) -> String {
    let mut vs = allowed_versions(cfg);
    vs.sort();
    let supported: Vec<Version> = {
        let mut s: Vec<Version> = KNOWN.iter().map(|k| Version { protocol: k.protocol }).filter(|v| is_supported(*v)).collect();
        s.sort();
        s
    };
    let mut runs: Vec<(Version, Version)> = Vec::new();
    for v in vs {
        match runs.last_mut() {
            Some((_, hi)) if supported.iter().position(|s| *s == v) == supported.iter().position(|s| s == hi).map(|i| i + 1) => *hi = v,
            _ => runs.push((v, v)),
        }
    }
    if runs.is_empty() {
        return "no version".into();
    }
    runs.iter().map(|(lo, hi)| if lo == hi { lo.name() } else { format!("{}-{}", lo.name(), hi.name()) }).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod chain_tests {
    use super::*;
    use crate::proto::buf::{Reader, Writer};
    use crate::proto::nbt::{self, Tag};

    /// Stands in for the steps above 1.20.2: serverbound ids + 100, clientbound unchanged.
    struct Shift;
    impl Translator for Shift {
        fn from(&self) -> Version {
            Version { protocol: 765 }
        }
        fn to(&self) -> Version {
            Version { protocol: 764 }
        }
        fn clientbound(&mut self, _: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
            out.push(pkt.clone());
            Ok(())
        }
        fn serverbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
            let shift = if state == State::Play { 100 } else { 200 };
            out.push(Packet::new(pkt.id + shift, pkt.payload.clone()));
            Ok(())
        }
    }

    /// What the 1.20.2 → 1.20 step answers on the client's behalf goes up through the newer
    /// steps, in the state it belongs to, and comes out of the chain for the proxy to send.
    #[test]
    fn replies_go_through_the_newer_steps() {
        let mut chain = Chain { steps: vec![Box::new(Shift), Box::new(v1_20::V1_20_2To1_20::new())], client: Version { protocol: 763 }, replies: Vec::new() };
        assert!(chain.emulates_configuration());
        let start = chain.login_acknowledged().unwrap();
        assert_eq!(start.iter().map(|p| p.id).collect::<Vec<_>>(), vec![200 + v1_20::ids::new::S_CONFIG_CLIENT_INFORMATION]);
        let out = chain.clientbound(State::Configuration, Packet::new(v1_20::ids::new::CONFIG_FINISH, Vec::new())).unwrap();
        assert!(out.is_empty());
        assert_eq!(chain.take_replies().iter().map(|p| p.id).collect::<Vec<_>>(), vec![200 + v1_20::ids::new::S_CONFIG_FINISH]);
        chain.clientbound(State::Play, Packet::new(v1_20::ids::new::CHUNK_BATCH_FINISHED, vec![1])).unwrap();
        assert_eq!(chain.take_replies().iter().map(|p| p.id).collect::<Vec<_>>(), vec![100 + v1_20::ids::new::S_CHUNK_BATCH_RECEIVED]);
        assert!(chain.take_replies().is_empty());
    }

    /// Clients with a configuration phase answer for themselves: no replies, no emulation.
    #[test]
    fn chains_with_configuration_have_no_replies() {
        let mut chain = Chain::for_client(Version { protocol: 766 }).unwrap();
        assert!(!chain.emulates_configuration());
        assert!(chain.login_acknowledged().unwrap().is_empty());
        let mut w = Writer::new();
        w.i64(0).varint(1);
        chain.clientbound(State::Play, Packet::new(v26_2::ids::new::BLOCK_UPDATE, w.into_inner())).unwrap();
        assert!(chain.take_replies().is_empty());
    }

    /// 1.20 / 1.20.1 is known, but only counts as supported once every step down to it exists.
    #[test]
    fn support_follows_the_chain() {
        let v1_20 = Version { protocol: 763 };
        assert_eq!(v1_20.name(), "1.20");
        assert_eq!(Chain::for_client(v1_20).is_some(), is_supported(v1_20));
        assert_eq!(Chain::for_client(Version { protocol: 764 }).is_some(), is_supported(Version { protocol: 764 }));
        let cfg = crate::config::Versions::default();
        for v in allowed_versions(&cfg) {
            assert!(Chain::for_client(v).is_some(), "{} allowed without a chain", v.name());
        }
    }

    /// The game profile 1.20.1 expects is uuid, name and properties, nothing after.
    #[test]
    fn login_finished_formats() {
        let props = [("textures", "abc", Some("sig"))];
        let base = {
            let mut w = Writer::new();
            w.uuid(7).string("Steve").varint(1).string("textures").string("abc").bool(true).string("sig");
            w.into_inner()
        };
        for protocol in [763, 764, 765, 768, 775] {
            assert_eq!(crate::proto::login::login_finished(protocol, 7, "Steve", &props, 9), base, "protocol {protocol}");
        }
        let mut strict = base.clone();
        strict.push(1);
        assert_eq!(crate::proto::login::login_finished(766, 7, "Steve", &props, 9), strict);
        let mut session = base.clone();
        session.extend_from_slice(&9u128.to_be_bytes());
        assert_eq!(crate::proto::login::login_finished(777, 7, "Steve", &props, 9), session);
    }

    /// A 26.3 block update for pink petals, through every step down to 1.21.4.
    #[test]
    fn pink_petals_block_update_reaches_1_21_4() {
        let mut chain = Chain::for_client(Version { protocol: 769 }).unwrap();
        let mut w = Writer::new();
        w.i64(0); // position
        w.varint(33664); // pink_petals[facing=north,flower_amount=1] in 26.3
        let out = chain.clientbound(State::Play, Packet::new(v26_2::ids::new::BLOCK_UPDATE, w.into_inner())).unwrap();
        assert_eq!(out.len(), 1);
        let mut r = Reader::new(&out[0].payload);
        r.i64().unwrap();
        assert_eq!(r.varint().unwrap(), 25840, "1.21.4 pink_petals[facing=north,flower_amount=1]");
    }

    fn pack(values: &[u32], bits: u32) -> Vec<i64> {
        let per = 64 / bits as usize;
        values.chunks(per).map(|c| c.iter().enumerate().fold(0u64, |v, (i, x)| v | (u64::from(*x) << (i as u32 * bits))) as i64).collect()
    }
    fn unpack(longs: &[i64], bits: u32, n: usize) -> Vec<u32> {
        let per = 64 / bits as usize;
        let mask = (1u64 << bits) - 1;
        (0..n).map(|i| ((longs[i / per] as u64 >> ((i % per) as u32 * bits)) & mask) as u32).collect()
    }

    /// A 26.3 chunk with one section of air and pink petals, through every step to 1.21.4.
    fn chunk_to_1_21_4(direct: bool) -> Vec<u32> {
        let mut values = vec![0u32; 4096];
        values[0] = 33664; // pink petals
        let mut sec = Writer::new();
        sec.i16(1); // non-air
        sec.i16(0); // fluids (26.1+)
        if direct {
            sec.u8(16);
            for l in pack(&values, 16) { sec.i64(l); }
        } else {
            sec.u8(4);
            sec.varint(2);
            sec.varint(0);
            sec.varint(33664);
            let idx: Vec<u32> = values.iter().map(|v| u32::from(*v != 0)).collect();
            for l in pack(&idx, 4) { sec.i64(l); }
        }
        sec.u8(0); // biomes: single value
        sec.varint(0);
        let mut w = Writer::new();
        w.i32(0);
        w.i32(0);
        w.varint(0); // heightmaps
        w.byte_array(&sec.buf);
        w.varint(0); // block entities
        for _ in 0..4 { w.varint(0); } // light masks
        w.varint(0);
        w.varint(0);
        let mut chain = Chain::for_client(Version { protocol: 769 }).unwrap();
        let out = chain.clientbound(State::Play, Packet::new(v26_2::ids::new::LEVEL_CHUNK_WITH_LIGHT, w.into_inner())).unwrap();
        let mut r = Reader::new(&out[0].payload);
        r.i32().unwrap();
        r.i32().unwrap();
        // 1.21.4 heightmaps: NBT compound
        crate::proto::nbt::read(&mut r).unwrap();
        let data = r.byte_array().unwrap().to_vec();
        let mut s = Reader::new(&data);
        s.i16().unwrap();
        let bits = u32::from(s.u8().unwrap());
        if bits == 0 {
            let v = s.varint().unwrap() as u32;
            return vec![v; 4096];
        }
        let palette: Vec<u32> = if bits <= 8 { (0..s.varint().unwrap()).map(|_| s.varint().unwrap() as u32).collect() } else { Vec::new() };
        let n = s.varint().unwrap() as usize; // 1.21.4 long array length
        let longs: Vec<i64> = (0..n).map(|_| s.i64().unwrap()).collect();
        let raw = unpack(&longs, bits.max(4), 4096);
        if palette.is_empty() { raw } else { raw.iter().map(|i| palette[*i as usize]).collect() }
    }

    #[test]
    fn pink_petals_section_update_reaches_1_21_4() {
        let mut chain = Chain::for_client(Version { protocol: 769 }).unwrap();
        let mut w = Writer::new();
        w.i64(0); // section position
        w.varint(1);
        w.varlong((33664i64 << 12) | 5);
        let out = chain.clientbound(State::Play, Packet::new(v26_2::ids::new::SECTION_BLOCKS_UPDATE, w.into_inner())).unwrap();
        let mut r = Reader::new(&out[0].payload);
        r.i64().unwrap();
        assert_eq!(r.varint().unwrap(), 1);
        assert_eq!(r.varlong().unwrap() >> 12, 25840);
    }

    #[test]
    fn pink_petals_in_chunk_palette_reach_1_21_4() {
        let v = chunk_to_1_21_4(false);
        assert_eq!((v[0], v[1]), (25840, 0));
    }

    #[test]
    fn pink_petals_in_chunk_direct_reach_1_21_4() {
        let v = chunk_to_1_21_4(true);
        assert_eq!((v[0], v[1]), (25840, 0));
    }

    /// A 1.21-only music disc in a 26.3 slot update reaches 1.20.5 as its stand-in, and the
    /// client's copy of it comes back to the server as the real disc.
    #[test]
    fn new_disc_round_trips_through_1_20_5() {
        let disc = data_26_2::NEW_ITEM_NAMES.iter().position(|n| *n == "music_disc_creator").unwrap() as i32;
        let mut chain = Chain::for_client(Version { protocol: 766 }).unwrap();
        let mut w = Writer::new();
        w.varint(0).varint(1).i16(36);
        w.varint(1).varint(disc).varint(0).varint(0);
        let out = chain.clientbound(State::Play, Packet::new(v26_2::ids::new::CONTAINER_SET_SLOT, w.into_inner())).unwrap();
        assert_eq!(out[0].id, 0x15, "1.20.5 container_set_slot");
        let mut r = Reader::new(&out[0].payload);
        r.u8().unwrap();
        r.varint().unwrap();
        r.i16().unwrap();
        assert_eq!(r.varint().unwrap(), 1);
        let far = data_1_20_5::OLD_ITEM_NAMES.iter().position(|n| *n == "music_disc_far").unwrap() as i32;
        assert_eq!(r.varint().unwrap(), far);
        // the client puts it in its creative inventory as is
        let mut c = Writer::new();
        c.i16(36);
        c.raw(&out[0].payload[r.position() - crate::proto::buf::varint_len(far) - 1..]);
        let back = chain.serverbound(State::Play, Packet::new(v1_20_5::ids::old::S_SET_CREATIVE_MODE_SLOT, c.into_inner())).unwrap();
        let last = back.last().unwrap();
        assert_eq!(last.id, v26_2::ids::new::S_SET_CREATIVE_MODE_SLOT);
        let mut r = Reader::new(&last.payload);
        assert_eq!(r.i16().unwrap(), 36);
        assert_eq!(r.varint().unwrap(), 1);
        assert_eq!(r.varint().unwrap(), disc);
    }

    /// 1.20.3 has no known-packs handshake: the chain answers the server's question for the
    /// client (in the server's version) and the client never sees it.
    #[test]
    fn known_packs_are_answered_for_1_20_3() {
        let mut chain = Chain::for_client(Version { protocol: 765 }).unwrap();
        let mut w = Writer::new();
        w.varint(1).string("minecraft").string("core").string("26.3");
        let out = chain.clientbound(State::Configuration, Packet::new(0x0F, w.into_inner())).unwrap(); // 26.3 select_known_packs
        assert!(out.is_empty());
        let replies = chain.take_replies();
        assert_eq!(replies.len(), 1);
        assert_eq!((replies[0].id, replies[0].payload.clone()), (0x07, vec![0]), "26.3 select_known_packs, no packs");
        assert!(chain.take_replies().is_empty());
    }

    /// A named, enchanted sword made in the 1.20.3 creative inventory (item NBT) reaches the
    /// server with its components, and a named item from the server comes back unchanged.
    #[test]
    fn creative_sword_round_trips_through_1_20_3() {
        let mut chain = Chain::for_client(Version { protocol: 765 }).unwrap();
        let mut reg = Writer::new();
        reg.string("minecraft:enchantment").varint(2).string("minecraft:protection").bool(false).string("minecraft:sharpness").bool(false);
        chain.clientbound(State::Configuration, Packet::new(v26_2::ids::new::CONFIG_REGISTRY_DATA, reg.into_inner())).unwrap();

        let old_sword = data_1_20_3::OLD_ITEM_NAMES.iter().position(|n| *n == "diamond_sword").unwrap() as i32;
        let mut ench = Tag::compound();
        ench.insert("id", Tag::String("minecraft:sharpness".into()));
        ench.insert("lvl", Tag::Short(5));
        let mut display = Tag::compound();
        display.insert("Name", Tag::String(r#"{"text":"Excalibur"}"#.into()));
        let mut tag = Tag::compound();
        tag.insert("Damage", Tag::Int(0));
        tag.insert("display", display);
        tag.insert("Enchantments", Tag::List(vec![ench]));
        let mut c = Writer::new();
        c.i16(36).bool(true).varint(old_sword).i8(1);
        nbt::write(&mut c, &tag);
        let out = chain.serverbound(State::Play, Packet::new(v1_20_3::ids::old::S_SET_CREATIVE_MODE_SLOT, c.into_inner())).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, v26_2::ids::new::S_SET_CREATIVE_MODE_SLOT);
        let mut r = Reader::new(&out[0].payload);
        assert_eq!(r.i16().unwrap(), 36);
        assert_eq!(r.varint().unwrap(), 1);
        let sword = data_26_2::NEW_ITEM_NAMES.iter().position(|n| *n == "diamond_sword").unwrap() as i32;
        assert_eq!(r.varint().unwrap(), sword);
        assert_eq!(r.varint().unwrap(), 2, "custom name and enchantments");
        assert_eq!(r.varint().unwrap(), 0);
        let mut found = Vec::new();
        for _ in 0..2 {
            let name = data_26_2::NEW_COMPONENT_NAMES[r.varint().unwrap() as usize];
            let value = r.byte_array().unwrap();
            found.push(name);
            if name == "enchantments" {
                assert_eq!(value, [1, 1, 5], "sharpness (registry id 1) level 5");
            } else {
                assert_eq!(nbt::read(&mut Reader::new(value)).unwrap().get("text"), Some(&Tag::String("Excalibur".into())));
            }
        }
        found.sort();
        assert_eq!(found, vec!["custom_name", "enchantments"]);

        // the server's named item reaches the client as item NBT and comes back as it was
        let mut name = Writer::new();
        nbt::write(&mut name, &Tag::String("Gift".into()));
        let custom_name = data_26_2::NEW_COMPONENT_NAMES.iter().position(|n| *n == "custom_name").unwrap() as i32;
        let mut s = Writer::new();
        s.varint(0).varint(1).i16(36);
        s.varint(1).varint(sword).varint(1).varint(0).varint(custom_name).raw(&name.buf);
        let out = chain.clientbound(State::Play, Packet::new(v26_2::ids::new::CONTAINER_SET_SLOT, s.into_inner())).unwrap();
        assert_eq!(out[0].id, 0x15, "1.20.3 container_set_slot");
        let mut r = Reader::new(&out[0].payload);
        r.u8().unwrap();
        r.varint().unwrap();
        r.i16().unwrap();
        let item = r.remaining().to_vec();
        assert!(r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), old_sword);
        r.i8().unwrap();
        let tag = nbt::read_optional(&mut r).unwrap().unwrap();
        assert_eq!(tag.get("display").unwrap().get("Name"), Some(&Tag::String("\"Gift\"".into())));
        let mut c = Writer::new();
        c.i16(36).raw(&item);
        let back = chain.serverbound(State::Play, Packet::new(v1_20_3::ids::old::S_SET_CREATIVE_MODE_SLOT, c.into_inner())).unwrap();
        let mut r = Reader::new(&back[0].payload);
        r.i16().unwrap();
        assert_eq!((r.varint().unwrap(), r.varint().unwrap(), r.varint().unwrap(), r.varint().unwrap()), (1, sword, 1, 0));
        assert_eq!(r.varint().unwrap(), custom_name);
        assert_eq!(r.byte_array().unwrap(), &name.buf[..]);
    }

    /// 1.21's data-driven registries never reach a 1.20.5 client.
    #[test]
    fn new_registries_do_not_reach_1_20_5() {
        let mut chain = Chain::for_client(Version { protocol: 766 }).unwrap();
        for name in ["minecraft:enchantment", "minecraft:painting_variant", "minecraft:jukebox_song"] {
            let mut w = Writer::new();
            w.string(name);
            w.varint(0);
            let out = chain.clientbound(State::Configuration, Packet::new(v26_2::ids::new::CONFIG_REGISTRY_DATA, w.into_inner())).unwrap();
            assert!(out.is_empty(), "{name} reached the client");
        }
    }
}

#[cfg(test)]
mod support_tests {
    use super::*;

    /// Only versions a chain can be built for are offered to clients.
    #[test]
    fn supported_matches_buildable_chains() {
        for k in KNOWN {
            let v = Version { protocol: k.protocol };
            assert_eq!(is_supported(v), Chain::for_client(v).is_some(), "{}", k.name);
        }
        assert!(is_supported(BACKEND));
        // The protocol below the oldest known version has no translator.
        let oldest = KNOWN.iter().map(|k| k.protocol).min().unwrap();
        assert!(!is_supported(Version { protocol: oldest - 1 }));
        let range = allowed_range(&crate::config::Versions::default());
        if !is_supported(Version { protocol: 765 }) {
            assert!(!is_supported(Version { protocol: 764 }), "1.20.2 needs the 1.20.3 step's newer neighbour");
            assert!(!range.contains("1.20.2"), "{range}");
        }
    }
}

#[cfg(test)]
mod block_state_coverage {
    use super::*;
    use crate::proto::buf::{Reader, Writer};

    /// No 26.3 block state should fall back to stone on the oldest client (e.g. leaf litter).
    #[test]
    fn no_state_falls_back_to_stone_on_1_21() {
        let mut stone = Vec::new();
        for s in 2..35723 {
            let mut chain = Chain::for_client(Version { protocol: 767 }).unwrap();
            let mut w = Writer::new();
            w.i64(0);
            w.varint(s);
            let out = chain.clientbound(State::Play, Packet::new(v26_2::ids::new::BLOCK_UPDATE, w.into_inner())).unwrap();
            let mut r = Reader::new(&out[0].payload);
            r.i64().unwrap();
            if r.varint().unwrap() == 1 {
                stone.push(s);
            }
        }
        assert!(stone.is_empty(), "states shown as stone: {stone:?}");
    }

    /// Every 26.3 block state reaches the 1.20.5 client as a valid state, and none falls back
    /// to stone unless it is stone itself.
    #[test]
    fn every_state_reaches_1_20_5() {
        let mut chain = Chain::for_client(Version { protocol: 766 }).unwrap();
        let mut stone = Vec::new();
        for s in 2..35723 {
            let mut w = Writer::new();
            w.i64(0);
            w.varint(s);
            let out = chain.clientbound(State::Play, Packet::new(v26_2::ids::new::BLOCK_UPDATE, w.into_inner())).unwrap();
            // some steps follow the update with block entity data; the update comes first
            assert_eq!(out[0].id, 0x09, "1.20.5 block_update");
            let mut r = Reader::new(&out[0].payload);
            r.i64().unwrap();
            let id = r.varint().unwrap();
            assert!((0..data_1_20_5::OLD_BLOCK_STATE_COUNT as i32).contains(&id), "state {s} became {id}");
            if id == 1 {
                stone.push(s);
            }
        }
        assert!(stone.is_empty(), "states shown as stone: {stone:?}");
        assert!(super::is_supported(Version { protocol: 766 }));
    }

    /// Every 26.3 block state reaches the 1.20.3 client as a valid state, and none falls back
    /// to stone unless it is stone itself.
    #[test]
    fn every_state_reaches_1_20_3() {
        let mut chain = Chain::for_client(Version { protocol: 765 }).unwrap();
        let mut stone = Vec::new();
        for s in 2..35723 {
            let mut w = Writer::new();
            w.i64(0);
            w.varint(s);
            let out = chain.clientbound(State::Play, Packet::new(v26_2::ids::new::BLOCK_UPDATE, w.into_inner())).unwrap();
            assert_eq!(out[0].id, v1_20_3::ids::old::BLOCK_UPDATE, "1.20.3 block_update");
            let mut r = Reader::new(&out[0].payload);
            r.i64().unwrap();
            let id = r.varint().unwrap();
            assert!((0..data_1_20_3::OLD_BLOCK_STATE_COUNT as i32).contains(&id), "state {s} became {id}");
            if id == 1 {
                stone.push(s);
            }
        }
        assert!(stone.is_empty(), "states shown as stone: {stone:?}");
        assert!(super::is_supported(Version { protocol: 765 }));
        let oldest = super::KNOWN.iter().min_by_key(|k| k.protocol).unwrap().name;
        assert_eq!(super::supported_range(), format!("{oldest}-26.3"));
        assert_eq!(super::allowed_range(&crate::config::Versions::default()), format!("{oldest}-26.3"));
    }
}
#[cfg(test)]
mod creative_slot_items {
    use super::*;
    use crate::proto::buf::{Reader, Writer};

    fn index(names: &[&str], name: &str) -> i32 {
        names.iter().position(|n| *n == name).unwrap() as i32
    }

    /// A creative-inventory bundle with a rarity, holding a stone that also has a rarity: the
    /// bundle's own components are length-prefixed from 1.21.5 on, the nested stone's never are.
    fn creative_bundle(items: &[&str], comps: &[&str], prefixed: bool) -> Vec<u8> {
        let mut rarity = Writer::new();
        rarity.varint(2);
        let mut stone = Writer::new();
        stone.varint(1);
        stone.varint(index(items, "stone"));
        stone.varint(1);
        stone.varint(0);
        stone.varint(index(comps, "rarity"));
        stone.raw(&rarity.buf);
        let mut contents = Writer::new();
        contents.varint(1);
        contents.raw(&stone.buf);
        let mut w = Writer::new();
        w.i16(36);
        w.varint(1);
        w.varint(index(items, "bundle"));
        w.varint(2);
        w.varint(0);
        for (name, value) in [("rarity", &rarity.buf), ("bundle_contents", &contents.buf)] {
            w.varint(index(comps, name));
            if prefixed {
                w.varint(value.len() as i32);
            }
            w.raw(value);
        }
        w.into_inner()
    }

    /// Bundles and other items with components, picked in the creative inventory, reach the
    /// server from every client version (1.21.9 clients and older used to be disconnected).
    #[test]
    fn creative_bundle_reaches_the_server_from_every_version() {
        let cases: [(i32, i32, &[&str], &[&str]); 10] = [
            (766, 0x32, &data_1_20_5::OLD_ITEM_NAMES, &data_1_20_5::OLD_COMPONENT_NAMES),
            (767, 0x32, &data_1_21::OLD_ITEM_NAMES, &data_1_21::OLD_COMPONENT_NAMES),
            (768, 0x34, &data_1_21_2::OLD_ITEM_NAMES, &data_1_21_2::OLD_COMPONENT_NAMES),
            (769, 0x36, &data_1_21_4::OLD_ITEM_NAMES, &data_1_21_4::OLD_COMPONENT_NAMES),
            (770, 0x36, &data_1_21_5::OLD_ITEM_NAMES, &data_1_21_5::OLD_COMPONENT_NAMES),
            (771, 0x37, &data_1_21_6::OLD_ITEM_NAMES, &data_1_21_6::OLD_COMPONENT_NAMES),
            (772, 0x37, &data_1_21_7::OLD_ITEM_NAMES, &data_1_21_7::OLD_COMPONENT_NAMES),
            (773, 0x37, &data_1_21_9::OLD_ITEM_NAMES, &data_1_21_9::OLD_COMPONENT_NAMES),
            (774, 0x37, &data_1_21_11::OLD_ITEM_NAMES, &data_1_21_11::OLD_COMPONENT_NAMES),
            (775, 0x38, &data_26_1::OLD_ITEM_NAMES, &data_26_1::OLD_COMPONENT_NAMES),
        ];
        for (protocol, id, items, comps) in cases {
            let payload = creative_bundle(items, comps, protocol >= 770);
            let mut chain = Chain::for_client(Version { protocol }).unwrap();
            let out = chain
                .serverbound(State::Play, Packet::new(id, payload))
                .unwrap_or_else(|e| panic!("protocol {protocol}: {e}"));
            assert_eq!(out.len(), 1, "protocol {protocol}");
            let mut r = Reader::new(&out[0].payload);
            assert_eq!(r.i16().unwrap(), 36, "protocol {protocol}");
            assert_eq!(r.varint().unwrap(), 1, "protocol {protocol}: count");
            r.varint().unwrap();
            assert_eq!(r.varint().unwrap(), 2, "protocol {protocol}: both components arrive");
        }
    }
}

#[cfg(test)]
mod loaded_crossbow {
    use super::*;
    use crate::proto::buf::{Reader, Writer};

    /// A 26.3 client is sent Pumpkin's packets as they are, so the same correction is made in
    /// the 26.3 bytes: everything else in the packet is untouched.
    fn native_client_gets_an_arrow_too(packet: &Packet) {
        let (items, comps) = (&data_26_2::NEW_ITEM_NAMES, &data_26_2::NEW_COMPONENT_NAMES);
        let mut step = v26_2::V26_3To26_2::new();
        let fixed = step.repair_native(packet).expect("the empty projectile is corrected");
        let mut want = Writer::new();
        want.varint(0).varint(5).i16(36).varint(1).varint(idx(items, "crossbow")).varint(1).varint(0);
        want.varint(idx(comps, "charged_projectiles")).varint(1);
        want.varint(idx(items, "arrow")).varint(1).varint(0).varint(0);
        assert_eq!(fixed, want.buf);
        // The corrected packet needs nothing more, and neither does an ordinary item.
        assert!(step.repair_native(&Packet::new(packet.id, fixed)).is_none());
        let mut plain = Writer::new();
        plain.varint(0).varint(5).i16(36).varint(3).varint(idx(items, "arrow")).varint(0).varint(0);
        assert!(step.repair_native(&Packet::new(packet.id, plain.into_inner())).is_none());
        // Other players see the crossbow through the equipment packet.
        let mut eq = Writer::new();
        eq.varint(77).i8(0).varint(1).varint(idx(items, "crossbow")).varint(1).varint(0);
        eq.varint(idx(comps, "charged_projectiles")).varint(2);
        for _ in 0..8 {
            eq.varint(0);
        }
        let fixed = step.repair_native(&Packet::new(v26_2::ids::new::SET_EQUIPMENT, eq.into_inner())).expect("equipment is corrected");
        let mut r = Reader::new(&fixed);
        assert_eq!((r.varint().unwrap(), r.i8().unwrap()), (77, 0));
        for want in [1, idx(items, "crossbow"), 1, 0, idx(comps, "charged_projectiles"), 1, idx(items, "arrow"), 1, 0, 0] {
            assert_eq!(r.varint().unwrap(), want);
        }
        assert!(r.is_empty());
        // Packets without items are not looked at.
        assert!(step.repair_native(&Packet::new(v26_2::ids::new::LEVEL_CHUNK_WITH_LIGHT, vec![1, 2, 3])).is_none());
    }

    fn idx(names: &[&str], n: &str) -> i32 {
        names.iter().position(|x| *x == n).unwrap() as i32
    }

    /// Pumpkin writes a loaded crossbow's projectile as an empty (air) template. Clients that
    /// take item stacks there (1.21.11 and older) reject an empty one and disconnect, so they get
    /// a valid crossbow loaded with an arrow instead.
    #[test]
    fn loaded_crossbow_reaches_older_clients() {
        let (items, comps) = (&data_26_2::NEW_ITEM_NAMES, &data_26_2::NEW_COMPONENT_NAMES);
        let mut w = Writer::new();
        w.varint(0);
        w.varint(5);
        w.i16(36);
        w.varint(1);
        w.varint(idx(items, "crossbow"));
        w.varint(1);
        w.varint(0);
        w.varint(idx(comps, "charged_projectiles"));
        w.varint(1);
        for _ in 0..4 {
            w.varint(0); // id, count, added, removed: what Pumpkin sends
        }
        let packet = Packet::new(v26_2::ids::new::CONTAINER_SET_SLOT, w.into_inner());
        native_client_gets_an_arrow_too(&packet);
        for (protocol, old_items, old_comps) in [
            (776, &data_26_2::OLD_ITEM_NAMES[..], &data_26_2::OLD_COMPONENT_NAMES[..]),
            (775, &data_26_1::OLD_ITEM_NAMES[..], &data_26_1::OLD_COMPONENT_NAMES[..]),
            (774, &data_1_21_11::OLD_ITEM_NAMES[..], &data_1_21_11::OLD_COMPONENT_NAMES[..]),
            (767, &data_1_21::OLD_ITEM_NAMES[..], &data_1_21::OLD_COMPONENT_NAMES[..]),
            (766, &data_1_20_5::OLD_ITEM_NAMES[..], &data_1_20_5::OLD_COMPONENT_NAMES[..]),
        ] {
            let mut chain = Chain::for_client(Version { protocol }).unwrap();
            let out = chain.clientbound(State::Play, packet.clone()).unwrap();
            let mut r = Reader::new(&out[0].payload);
            r.varint().unwrap();
            r.varint().unwrap();
            r.i16().unwrap();
            assert_eq!(r.varint().unwrap(), 1, "protocol {protocol}: count");
            assert_eq!(r.varint().unwrap(), idx(old_items, "crossbow"), "protocol {protocol}");
            let added = r.varint().unwrap();
            r.varint().unwrap();
            assert!(added >= 1, "protocol {protocol}");
            assert_eq!(r.varint().unwrap(), idx(old_comps, "charged_projectiles"), "protocol {protocol}");
            assert_eq!(r.varint().unwrap(), 1, "protocol {protocol}: one projectile");
            // 26.1 and newer list item templates (id, then count); older ones item stacks (count, then id).
            let (first, second) = (r.varint().unwrap(), r.varint().unwrap());
            let (count, id) = if protocol >= 775 { (second, first) } else { (first, second) };
            assert_eq!(count, 1, "protocol {protocol}: projectile count");
            assert_eq!(id, idx(old_items, "arrow"), "protocol {protocol}: an arrow, not air");
        }
    }
}

#[cfg(test)]
mod placed_painting {
    use super::*;
    use crate::proto::buf::{Reader, Writer};

    /// Pumpkin sends a painting's variant as a plain registry id; clients expect the id plus one
    /// (0 means a definition follows). Its first painting therefore disconnected every client.
    #[test]
    fn painting_variant_reaches_every_client_as_id_plus_one() {
        for raw in [0, 5] {
            let mut w = Writer::new();
            w.varint(321); // entity id
            w.u8(8).varint(v26_2::entity_type_painting_variant()).varint(raw);
            w.u8(0xff);
            let packet = Packet::new(v26_2::ids::new::SET_ENTITY_DATA, w.into_inner());

            // 26.3: Pumpkin's packet with only that value changed.
            let fixed = v26_2::V26_3To26_2::new().repair_native(&packet).expect("corrected for 26.3 clients");
            let mut r = Reader::new(&fixed);
            assert_eq!(r.varint().unwrap(), 321);
            assert_eq!(r.u8().unwrap(), 8);
            r.varint().unwrap();
            assert_eq!(r.varint().unwrap(), raw + 1);
            assert_eq!(r.u8().unwrap(), 0xff);
            assert!(r.is_empty());

            // Translated clients: the packet survives the whole chain and carries id + 1.
            for protocol in [776, 774, 769, 767, 765, 763] {
                let mut chain = Chain::for_client(Version { protocol }).unwrap();
                let out = chain.clientbound(State::Play, packet.clone()).unwrap_or_else(|e| panic!("protocol {protocol}: {e}"));
                assert_eq!(out.len(), 1, "protocol {protocol}");
                let mut r = Reader::new(&out[0].payload);
                assert_eq!(r.varint().unwrap(), 321, "protocol {protocol}");
                assert_eq!(r.u8().unwrap(), 8, "protocol {protocol}");
                r.varint().unwrap();
                // 1.20.5 and newer read "id plus one"; older versions read the plain id, which
                // their own step converts to.
                if protocol >= 766 {
                    assert_eq!(r.varint().unwrap(), raw + 1, "protocol {protocol}: variant");
                } else if raw == 0 {
                    assert_eq!(r.varint().unwrap(), 0, "protocol {protocol}: variant");
                }
            }
        }
    }
}

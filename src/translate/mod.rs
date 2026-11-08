//! Protocol translation between the client's version and Pumpkin's (26.3).
//!
//! A `Translator` converts packets one version step (e.g. 26.3 ↔ 26.2). A `Chain` stacks the
//! steps needed for a given client, newest first for clientbound and oldest first for
//! serverbound, one version step at a time.

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
}

/// All steps for one client, newest first.
pub struct Chain {
    steps: Vec<Box<dyn Translator>>,
    pub client: Version,
}

impl Chain {
    /// Builds the chain for `client`, or `None` if the version is not supported.
    pub fn for_client(client: Version) -> Option<Chain> {
        if client == BACKEND {
            return Some(Chain { steps: Vec::new(), client });
        }
        let mut steps: Vec<Box<dyn Translator>> = Vec::new();
        let mut current = BACKEND;
        while current != client {
            let step = make_step(current)?;
            current = step.to();
            steps.push(step);
        }
        Some(Chain { steps, client })
    }

    pub fn is_passthrough(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn clientbound(&mut self, state: State, pkt: Packet) -> Result<Vec<Packet>, String> {
        let mut current = vec![pkt];
        for step in &mut self.steps {
            let mut next = Vec::with_capacity(current.len());
            for p in &current {
                step.clientbound(state, p, &mut next)?;
            }
            current = next;
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
        _ => None,
    }
}

/// Lowest client version a chain exists for.
pub fn lowest_supported() -> Version {
    let mut v = BACKEND;
    while let Some(step) = make_step(v) {
        v = step.to();
    }
    v
}

pub fn is_supported(client: Version) -> bool {
    KNOWN.iter().any(|k| k.protocol == client.protocol)
        && client.protocol <= BACKEND.protocol
        && client.protocol >= lowest_supported().protocol
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
}

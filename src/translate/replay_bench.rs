//! Replays a recorded stream of server packets through every translator chain and reports how
//! long each takes, to find what is worth making faster. Not run by default:
//!
//! ```text
//! MULTIVERSION_REPLAY=stream.bin cargo test --release replay -- --ignored --nocapture
//! ```
//!
//! The file is a list of records: one byte of state (0 configuration, 1 play), the packet id and
//! the payload length as big-endian i32, then the payload, as a 26.3 client receives them.

use super::{Chain, Packet, State};
use crate::proto::version::{Version, BACKEND, KNOWN};
use std::collections::HashMap;
use std::time::{Duration, Instant};

fn records(data: &[u8]) -> Vec<(State, Packet)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 9 <= data.len() {
        let state = if data[i] == 0 { State::Configuration } else { State::Play };
        let id = i32::from_be_bytes(data[i + 1..i + 5].try_into().unwrap());
        let len = i32::from_be_bytes(data[i + 5..i + 9].try_into().unwrap()) as usize;
        i += 9;
        out.push((state, Packet::new(id, data[i..i + len].to_vec())));
        i += len;
    }
    out
}

#[test]
#[ignore = "needs a recorded stream in MULTIVERSION_REPLAY"]
fn replay() {
    let Ok(path) = std::env::var("MULTIVERSION_REPLAY") else { return };
    let stream = records(&std::fs::read(path).unwrap());
    let rounds: u32 = std::env::var("MULTIVERSION_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
    for info in KNOWN.iter().filter(|v| v.protocol != BACKEND.protocol) {
        let mut total = Duration::ZERO;
        let mut by_id: HashMap<i32, (Duration, u32)> = HashMap::new();
        let mut errors = 0;
        for _ in 0..rounds {
            let mut chain = Chain::for_client(Version { protocol: info.protocol }).unwrap();
            if chain.emulates_configuration() {
                chain.login_acknowledged().unwrap();
            }
            for (state, pkt) in &stream {
                let started = Instant::now();
                let result = chain.clientbound(*state, pkt.clone());
                let took = started.elapsed();
                chain.take_replies();
                errors += u32::from(result.is_err());
                total += took;
                if *state == State::Play {
                    let e = by_id.entry(pkt.id).or_default();
                    e.0 += took;
                    e.1 += 1;
                }
            }
        }
        let mut top: Vec<_> = by_id.into_iter().collect();
        top.sort_by_key(|(_, (t, _))| std::cmp::Reverse(*t));
        let top: Vec<String> = top.iter().take(4).map(|(id, (t, n))| format!("{id}:{:.1}ms/{}", t.as_secs_f64() * 1000.0 / f64::from(rounds), n / rounds)).collect();
        println!("{:>8}  {:7.1} ms per stream  errors {}  top {}", info.name, total.as_secs_f64() * 1000.0 / f64::from(rounds), errors / rounds, top.join("  "));
    }
}

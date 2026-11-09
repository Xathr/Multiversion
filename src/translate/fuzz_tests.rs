//! Throws malformed serverbound packets at every translator chain. A player controls these bytes,
//! and a panic in the plugin would disconnect everyone, so translators must answer bad input
//! with an error, never a panic, a hang or a huge allocation.

use super::{Chain, Packet, State};
use crate::proto::version::{Version, BACKEND, KNOWN};

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

fn iterations() -> u64 {
    std::env::var("MULTIVERSION_FUZZ").ok().and_then(|v| v.parse().ok()).unwrap_or(20_000)
}

#[test]
fn malformed_serverbound_packets_never_panic() {
    let per_version = iterations();
    for info in KNOWN.iter().filter(|v| v.protocol != BACKEND.protocol) {
        let client = Version { protocol: info.protocol };
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ info.protocol as u64);
        let mut chain = Chain::for_client(client).expect("a chain for every known version");
        for i in 0..per_version {
            let state = if rng.below(4) == 0 { State::Configuration } else { State::Play };
            let id = if rng.below(20) == 0 { rng.next() as i32 } else { rng.below(96) as i32 };
            let pkt = Packet::new(id, rng.payload());
            let shown = format!("{} packet {id:#x} {:?} in {state:?} (iteration {i})", info.name, pkt.payload);
            let started = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| chain.serverbound(state, pkt)));
            match result {
                Err(_) => panic!("translator panicked on {shown}"),
                Ok(Ok(out)) => {
                    let bytes: usize = out.iter().map(|p| p.payload.len()).sum();
                    assert!(bytes < 1 << 20, "{bytes} bytes sent to the server for {shown}");
                }
                Ok(Err(_)) => {
                    // A refused packet ends that player's connection; start the next one fresh.
                    chain = Chain::for_client(client).unwrap();
                }
            }
            assert!(started.elapsed().as_millis() < 2000, "took {:?} on {shown}", started.elapsed());
        }
    }
}

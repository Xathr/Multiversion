//! Remembers the result of converting one large packet, so the same packet for the next player
//! is not converted again.
//!
//! The recipe book is the same for every player and is the largest single thing sent at login:
//! about a third of all translation for an older client. Each step that converts it without
//! looking at anything but the packet asks here first. What is kept is the converted bytes and
//! a fingerprint of the input, not the input itself.

use crate::proto::buf::Result;
use std::cell::RefCell;

/// Packets smaller than this are converted every time: they are quick, and there are many
/// different ones.
const MIN_BYTES: usize = 16 * 1024;
/// The most results kept, and the most bytes they may take together. A full recipe book is a
/// few hundred kilobytes per step; the oldest results go first.
const MAX_ENTRIES: usize = 64;
const MAX_BYTES: usize = 12 * 1024 * 1024;

struct Entry {
    site: u32,
    len: usize,
    print: (u64, u64),
    output: Vec<u8>,
}

thread_local! {
    static KEPT: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

/// Two independent 64-bit fingerprints of `data`, taken eight bytes at a time. With the length
/// they identify the input; two different packets sharing all three is not a practical concern.
fn fingerprint(data: &[u8]) -> (u64, u64) {
    let (mut a, mut b) = (0x9e37_79b9_7f4a_7c15u64, 0xc2b2_ae3d_27d4_eb4fu64);
    let mut chunks = data.chunks_exact(8);
    for chunk in &mut chunks {
        let word = u64::from_le_bytes(chunk.try_into().expect("eight bytes"));
        a = (a ^ word).wrapping_mul(0x0000_0100_0000_01b3).rotate_left(29);
        b = (b.rotate_left(31) ^ word).wrapping_mul(0xff51_afd7_ed55_8ccd);
    }
    for byte in chunks.remainder() {
        a = (a ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        b = (b.rotate_left(7) ^ u64::from(*byte)).wrapping_mul(0xff51_afd7_ed55_8ccd);
    }
    (a ^ (a >> 32), b ^ (b >> 29))
}

/// `convert()`'s result for `input`, from an earlier call if there was one. `site` says which
/// conversion this is (one number per step and packet), since the same bytes mean different
/// things to different steps. `convert` must depend on `input` alone. Errors are not kept.
pub fn cached(site: u32, input: &[u8], convert: impl FnOnce() -> Result<Vec<u8>>) -> Result<Vec<u8>> {
    if input.len() < MIN_BYTES {
        return convert();
    }
    let print = fingerprint(input);
    let found = KEPT.with(|kept| kept.borrow().iter().find(|e| e.site == site && e.len == input.len() && e.print == print).map(|e| e.output.clone()));
    if let Some(output) = found {
        return Ok(output);
    }
    let output = convert()?;
    KEPT.with(|kept| {
        let mut kept = kept.borrow_mut();
        kept.push(Entry { site, len: input.len(), print, output: output.clone() });
        let mut bytes: usize = kept.iter().map(|e| e.output.len()).sum();
        while kept.len() > MAX_ENTRIES || (bytes > MAX_BYTES && kept.len() > 1) {
            bytes -= kept.remove(0).output.len();
        }
    });
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn big(seed: u8) -> Vec<u8> {
        (0..MIN_BYTES + 100).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
    }

    #[test]
    fn a_large_packet_is_converted_once_per_step() {
        let calls = Cell::new(0);
        let convert = |input: &[u8]| {
            calls.set(calls.get() + 1);
            Ok(input.iter().rev().copied().collect::<Vec<u8>>())
        };
        let input = big(1);
        let first = cached(9001, &input, || convert(&input)).unwrap();
        let again = cached(9001, &input, || convert(&input)).unwrap();
        assert_eq!((first.clone(), calls.get()), (again, 1));
        // another step, another packet, a packet one byte different: each is converted itself
        cached(9002, &input, || convert(&input)).unwrap();
        let other = big(2);
        assert_eq!(cached(9001, &other, || convert(&other)).unwrap(), other.iter().rev().copied().collect::<Vec<u8>>());
        let mut near = input.clone();
        *near.last_mut().unwrap() ^= 1;
        assert_ne!(cached(9001, &near, || convert(&near)).unwrap(), first);
        assert_eq!(calls.get(), 4);
        // small packets are never kept
        let small = vec![7u8; 100];
        cached(9001, &small, || convert(&small)).unwrap();
        cached(9001, &small, || convert(&small)).unwrap();
        assert_eq!(calls.get(), 6);
    }

    #[test]
    fn errors_are_not_kept_and_old_results_make_room() {
        let input = big(3);
        let failing = cached(9100, &input, || Err(crate::proto::buf::DecodeError::Eof));
        assert!(failing.is_err());
        assert_eq!(cached(9100, &input, || Ok(vec![1])).unwrap(), [1]);
        for site in 0..(MAX_ENTRIES as u32 + 10) {
            cached(20_000 + site, &input, || Ok(vec![0; 10])).unwrap();
        }
        assert!(KEPT.with(|k| k.borrow().len()) <= MAX_ENTRIES);
        // the first of those was dropped to make room, so it is converted again
        let again = Cell::new(false);
        cached(20_000, &input, || {
            again.set(true);
            Ok(vec![0; 10])
        })
        .unwrap();
        assert!(again.get());
    }
}

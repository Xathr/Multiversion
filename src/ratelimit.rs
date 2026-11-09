//! Counts new connections per address in fixed windows, so one address cannot keep the proxy busy
//! with a stream of short connections (each login costs an RSA decryption).

use std::collections::HashMap;
use std::net::IpAddr;

/// Length of one counting window.
pub const WINDOW_MS: u64 = 10_000;

#[derive(Default)]
pub struct ConnectionRate {
    /// address -> (start of its window in ms, connections in that window)
    seen: HashMap<IpAddr, (u64, u32)>,
}

impl ConnectionRate {
    /// Records a connection from `ip` at `now_ms`. False if it is over `limit` for the current
    /// window; `limit` 0 means no limit.
    pub fn allow(&mut self, ip: IpAddr, now_ms: u64, limit: u32) -> bool {
        if limit == 0 {
            return true;
        }
        let entry = self.seen.entry(ip).or_insert((now_ms, 0));
        if now_ms.saturating_sub(entry.0) >= WINDOW_MS {
            *entry = (now_ms, 0);
        }
        entry.1 = entry.1.saturating_add(1);
        entry.1 <= limit
    }

    /// Forgets addresses whose window has passed. Call now and then so the table stays small.
    pub fn prune(&mut self, now_ms: u64) {
        self.seen.retain(|_, (start, _)| now_ms.saturating_sub(*start) < WINDOW_MS);
    }

    pub fn tracked(&self) -> usize {
        self.seen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_per_address_and_window() {
        let a: IpAddr = "203.0.113.7".parse().unwrap();
        let b: IpAddr = "203.0.113.8".parse().unwrap();
        let mut rate = ConnectionRate::default();
        assert!((0..3).all(|i| rate.allow(a, i, 3)));
        assert!(!rate.allow(a, 5, 3), "the fourth connection in the window is refused");
        assert!(rate.allow(b, 5, 3), "another address is counted separately");
        assert!(!rate.allow(a, WINDOW_MS - 1, 3));
        assert!(rate.allow(a, WINDOW_MS, 3), "a new window starts fresh");
        assert!((0..1000).all(|i| rate.allow(a, i, 0)), "0 turns the limit off");
    }

    #[test]
    fn prune_forgets_old_addresses() {
        let mut rate = ConnectionRate::default();
        for i in 0..50u8 {
            rate.allow(IpAddr::from([10, 0, 0, i]), 0, 5);
        }
        rate.allow(IpAddr::from([10, 0, 1, 1]), WINDOW_MS, 5);
        rate.prune(WINDOW_MS + 1);
        assert_eq!(rate.tracked(), 1);
    }
}

//! Compact id-mapping tables.
//!
//! Most mappings between versions count up one at a time (id 500 maps to 497, 501 to 498, ...),
//! so the generated tables store runs of `(first value, length)` instead of every entry. That
//! keeps the plugin file small; each table is expanded into a plain list in memory the first time
//! it is used, so lookups cost the same as indexing an array.

use std::ops::Deref;
use std::sync::OnceLock;

pub struct Table {
    len: usize,
    /// Consecutive runs covering indexes 0..len in order: `(value at the run's first index, length)`.
    runs: &'static [(u32, u32)],
    expanded: OnceLock<Vec<u32>>,
}

impl Table {
    pub const fn new(len: usize, runs: &'static [(u32, u32)]) -> Self {
        Self { len, runs, expanded: OnceLock::new() }
    }

    fn expand(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.len);
        for &(start, n) in self.runs {
            out.extend((0..n).map(|i| start.wrapping_add(i)));
        }
        debug_assert_eq!(out.len(), self.len, "table runs don't add up to its length");
        out
    }
}

impl Deref for Table {
    type Target = [u32];
    fn deref(&self) -> &[u32] {
        self.expanded.get_or_init(|| self.expand())
    }
}

/// Splits values into the runs `Table::new` takes (used by tests; the generator does the same).
#[cfg(test)]
pub fn runs(values: &[u32]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (i, &v) in values.iter().enumerate() {
        match out.last_mut() {
            Some((_, n)) if i > 0 && values[i - 1].wrapping_add(1) == v => *n += 1,
            _ => out.push((v, 1)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_runs() {
        static T: Table = Table::new(7, &[(0, 3), (10, 1), (5, 2), (0xFFFF_FFFF, 1)]);
        assert_eq!(&*T, &[0, 1, 2, 10, 5, 6, 0xFFFF_FFFF]);
        assert_eq!(T.len(), 7);
        assert_eq!(T[3], 10);
        assert_eq!(T.get(7), None);
    }

    #[test]
    fn runs_round_trip() {
        let values = [3, 4, 5, 5, 5, 9, 1, 2, 0xFFFF_FFFF];
        let r: &'static [(u32, u32)] = Box::leak(runs(&values).into_boxed_slice());
        let t = Table::new(values.len(), r);
        assert_eq!(&*t, &values);
    }
}

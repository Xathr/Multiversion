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

/// A list of names (items, blocks, sounds, ... of one version), stored as runs of positions in
/// the shared pool of names (`data_names::POOL`) instead of as a list of strings: versions share
/// nearly all their names, and a list of strings costs 8 bytes per entry on top of the text. It
/// is laid out as an ordinary list the first time it is used, and reads like one (`get`, `iter`,
/// `len`, indexing).
pub struct Names {
    len: usize,
    runs: &'static [(u32, u32)],
    expanded: OnceLock<Vec<&'static str>>,
}

impl Names {
    pub const fn new(len: usize, runs: &'static [(u32, u32)]) -> Self {
        Self { len, runs, expanded: OnceLock::new() }
    }
}

impl Deref for Names {
    type Target = [&'static str];
    fn deref(&self) -> &[&'static str] {
        self.expanded.get_or_init(|| {
            let pool = &super::data_names::POOL;
            let mut out = Vec::with_capacity(self.len);
            for &(start, n) in self.runs {
                out.extend((0..n).map(|i| pool.get((start + i) as usize).copied().unwrap_or("")));
            }
            debug_assert_eq!(out.len(), self.len, "name runs don't add up to the list's length");
            out
        })
    }
}

/// For every id of a newer version, the id the thing of the same name has in an older version.
/// Worked out once, on first use, so looking a name up never means searching the whole list.
pub struct SameName {
    new: &'static Names,
    old: &'static Names,
    ids: OnceLock<Vec<i32>>,
}

impl SameName {
    pub const fn new(new: &'static Names, old: &'static Names) -> Self {
        Self { new, old, ids: OnceLock::new() }
    }

    /// The older version's id for `new_id`, or `None` if the older version has nothing by that name.
    pub fn get(&self, new_id: i32) -> Option<i32> {
        let ids = self.ids.get_or_init(|| {
            let by_name: std::collections::HashMap<&str, i32> = self.old.iter().enumerate().map(|(i, n)| (*n, i as i32)).collect();
            self.new.iter().map(|n| by_name.get(n).copied().unwrap_or(-1)).collect()
        });
        usize::try_from(new_id).ok().and_then(|i| ids.get(i)).copied().filter(|id| *id >= 0)
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

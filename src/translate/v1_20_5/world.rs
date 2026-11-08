//! Registries, tags and world events 1.21 → 1.20.5.
//!
//! 1.21 made enchantments, painting variants and jukebox songs data-driven and syncs them in the
//! configuration phase; the 1.20.5 client has those built in (jukebox songs not at all) and must
//! not receive them. Their entries are kept here so ids can be mapped to the client's built-in
//! ones. Block states, blocks and block entities are identical in both versions, so chunks and
//! block updates pass through unchanged.

use super::data_1_20_5 as d;
use super::items::{strip_ns, Items};
use crate::proto::buf::{Reader, Result, Writer};
use crate::proto::nbt;

/// 1.20.5 built-in painting variants, in id order: (asset id, width, height).
const OLD_PAINTINGS: [(&str, i32, i32); 30] = [
    ("kebab", 1, 1),
    ("aztec", 1, 1),
    ("alban", 1, 1),
    ("aztec2", 1, 1),
    ("bomb", 1, 1),
    ("plant", 1, 1),
    ("wasteland", 1, 1),
    ("pool", 2, 1),
    ("courbet", 2, 1),
    ("sea", 2, 1),
    ("sunset", 2, 1),
    ("creebet", 2, 1),
    ("wanderer", 1, 2),
    ("graham", 1, 2),
    ("match", 2, 2),
    ("bust", 2, 2),
    ("stage", 2, 2),
    ("void", 2, 2),
    ("skull_and_roses", 2, 2),
    ("wither", 2, 2),
    ("fighters", 4, 2),
    ("pointer", 4, 4),
    ("pigscene", 4, 4),
    ("burning_skull", 4, 4),
    ("skeleton", 4, 3),
    ("earth", 2, 2),
    ("wind", 2, 2),
    ("water", 2, 2),
    ("fire", 2, 2),
    ("donkey_kong", 4, 3),
];

/// The 1.20.5 painting for a 1.21 one: by name, else the first of the same size, else kebab.
pub fn old_painting(asset: &str, size: Option<(i32, i32)>) -> i32 {
    let name = strip_ns(asset);
    debug_assert_eq!(d::OLD_PAINTING_NAMES.len(), OLD_PAINTINGS.len());
    if let Some(i) = OLD_PAINTINGS.iter().position(|(n, _, _)| *n == name) {
        return i as i32;
    }
    size.and_then(|(w, h)| OLD_PAINTINGS.iter().position(|(_, pw, ph)| *pw == w && *ph == h)).map_or(0, |i| i as i32)
}

/// What the server's data-driven registries contained, by 1.21 id.
#[derive(Default)]
pub struct Registries {
    pub enchantments: Vec<String>,
    /// 1.21 painting variant id → 1.20.5 painting id
    pub paintings: Vec<i32>,
    /// 1.21 jukebox song id → 1.20.5 music disc item id (-1 = none)
    pub jukebox_songs: Vec<i32>,
}

/// Result of a REGISTRY_DATA packet.
pub enum RegistryData {
    /// Pass on to the client.
    Keep,
    /// Recorded here, not for the client; `true` when the enchantment list changed.
    Dropped(bool),
}

impl Registries {
    pub fn painting(&self, id: i32) -> i32 {
        usize::try_from(id).ok().and_then(|i| self.paintings.get(i)).copied().unwrap_or(0)
    }

    pub fn jukebox_song_item(&self, id: i32) -> i32 {
        usize::try_from(id).ok().and_then(|i| self.jukebox_songs.get(i)).copied().unwrap_or(-1)
    }

    /// REGISTRY_DATA: records (and drops) the registries the 1.20.5 client has built in.
    pub fn registry_data(&mut self, payload: &[u8]) -> Result<RegistryData> {
        let mut r = Reader::new(payload);
        let registry = strip_ns(r.string()?).to_string();
        if !matches!(registry.as_str(), "enchantment" | "painting_variant" | "jukebox_song") {
            return Ok(RegistryData::Keep);
        }
        let entries = r.list(|r| {
            let key = r.string()?.to_string();
            let data = if r.bool()? { Some(nbt::read(r)?) } else { None };
            Ok((key, data))
        })?;
        match registry.as_str() {
            "enchantment" => {
                self.enchantments = entries.into_iter().map(|(k, _)| k).collect();
                return Ok(RegistryData::Dropped(true));
            }
            "painting_variant" => {
                self.paintings = entries
                    .iter()
                    .map(|(key, data)| {
                        let size = data.as_ref().and_then(|t| Some((t.get("width")?.as_i32()?, t.get("height")?.as_i32()?)));
                        old_painting(key, size)
                    })
                    .collect();
            }
            _ => {
                self.jukebox_songs = entries
                    .iter()
                    .map(|(key, _)| Items::new_item_by_name(&format!("music_disc_{}", strip_ns(key))).map_or(-1, |i| d::ITEMS[i as usize] as i32))
                    .collect();
            }
        }
        Ok(RegistryData::Dropped(false))
    }

    /// UPDATE_TAGS: remaps item, enchantment and painting ids; jukebox song tags are dropped.
    pub fn update_tags(&self, items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let n = r.varint()?;
        let mut kept = 0;
        let mut body = Writer::with_capacity(payload.len());
        for _ in 0..n.max(0) {
            let registry = r.string()?.to_string();
            let key = strip_ns(&registry).to_string();
            let tags = r.list(|r| Ok((r.string()?.to_string(), r.list(|r| r.varint())?)))?;
            if key == "jukebox_song" {
                continue;
            }
            body.string(&registry);
            body.varint(tags.len() as i32);
            for (name, ids) in tags {
                body.string(&name);
                let mapped: Vec<i32> = ids
                    .iter()
                    .filter_map(|id| match key.as_str() {
                        "item" => Some(items.item(*id)),
                        "enchantment" => items.enchantment(*id),
                        "painting_variant" => Some(self.painting(*id)),
                        _ => Some(*id),
                    })
                    .collect();
                body.varint(mapped.len() as i32);
                for id in mapped {
                    body.varint(id);
                }
            }
            kept += 1;
        }
        let mut w = Writer::with_capacity(body.buf.len() + 2);
        w.varint(kept);
        w.raw(&body.buf);
        Ok(w.into_inner())
    }

    /// LEVEL_EVENT: the jukebox event names a song in 1.21 and a disc item in 1.20.5; `None` when
    /// the song has no disc.
    pub fn level_event(&self, payload: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut r = Reader::new(payload);
        let event = r.i32()?;
        if event != 1010 {
            return Ok(Some(payload.to_vec()));
        }
        let pos = r.i64()?;
        let item = self.jukebox_song_item(r.i32()?);
        if item < 0 {
            return Ok(None);
        }
        let mut w = Writer::with_capacity(payload.len());
        w.i32(event);
        w.i64(pos);
        w.i32(item);
        w.raw(r.remaining());
        Ok(Some(w.into_inner()))
    }
}

/// UPDATE_ENABLED_FEATURES: the 1.21 content is behind a feature flag in 1.20.5.
pub fn enabled_features(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut flags: Vec<String> = r.list(|r| r.string().map(str::to_string))?;
    if !flags.iter().any(|f| strip_ns(f) == "update_1_21") {
        flags.push("minecraft:update_1_21".into());
    }
    let mut w = Writer::new();
    w.varint(flags.len() as i32);
    for f in &flags {
        w.string(f);
    }
    Ok(w.into_inner())
}

/// Tags by registry and tag name (test helper).
#[cfg(test)]
fn tag_map(payload: &[u8]) -> std::collections::HashMap<String, std::collections::HashMap<String, Vec<i32>>> {
    let mut r = Reader::new(payload);
    let mut out = std::collections::HashMap::new();
    for _ in 0..r.varint().unwrap() {
        let reg = r.string().unwrap().to_string();
        let tags = r.list(|r| Ok((r.string()?.to_string(), r.list(|r| r.varint())?))).unwrap();
        out.insert(reg, tags.into_iter().collect());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::items::Dir;
    use super::*;
    use crate::proto::nbt::Tag;

    fn registry(name: &str, entries: &[(&str, Option<Tag>)]) -> Vec<u8> {
        let mut w = Writer::new();
        w.string(name);
        w.varint(entries.len() as i32);
        for (k, t) in entries {
            w.string(k);
            w.bool(t.is_some());
            if let Some(t) = t {
                nbt::write(&mut w, t);
            }
        }
        w.into_inner()
    }

    fn size(w: i32, h: i32) -> Option<Tag> {
        let mut t = Tag::compound();
        t.insert("width", Tag::Int(w));
        t.insert("height", Tag::Int(h));
        t.insert("asset_id", Tag::String("x".into()));
        Some(t)
    }

    #[test]
    fn new_registries_are_dropped_and_recorded() {
        let mut regs = Registries::default();
        let p = registry("minecraft:painting_variant", &[("minecraft:alban", size(1, 1)), ("minecraft:backyard", size(3, 4)), ("minecraft:meditative", size(1, 1))]);
        assert!(matches!(regs.registry_data(&p).unwrap(), RegistryData::Dropped(false)));
        assert_eq!(regs.paintings, vec![2, 0, 0]); // alban by name, the others by size or kebab
        let e = registry("minecraft:enchantment", &[("minecraft:mending", None)]);
        assert!(matches!(regs.registry_data(&e).unwrap(), RegistryData::Dropped(true)));
        assert_eq!(regs.enchantments, vec!["minecraft:mending".to_string()]);
        let j = registry("minecraft:jukebox_song", &[("minecraft:cat", None), ("minecraft:creator", None), ("minecraft:unknown", None)]);
        assert!(matches!(regs.registry_data(&j).unwrap(), RegistryData::Dropped(false)));
        let item = |n: &str| d::OLD_ITEM_NAMES.iter().position(|x| *x == n).unwrap() as i32;
        assert_eq!(regs.jukebox_songs, vec![item("music_disc_cat"), item("music_disc_far"), -1]);
        let b = registry("minecraft:worldgen/biome", &[("minecraft:plains", None)]);
        assert!(matches!(regs.registry_data(&b).unwrap(), RegistryData::Keep));
    }

    #[test]
    fn tags_are_remapped_and_filtered() {
        let mut items = Items::new(Dir::ToClient);
        items.set_enchantments(&["minecraft:mending".to_string(), "minecraft:custom".to_string()]);
        let regs = Registries { paintings: vec![5, 9], ..Default::default() };
        let creator = Items::new_item_by_name("music_disc_creator").unwrap();
        let mut w = Writer::new();
        w.varint(4);
        w.string("minecraft:item").varint(1).string("minecraft:music_discs").varint(1).varint(creator);
        w.string("minecraft:enchantment").varint(1).string("minecraft:treasure").varint(2).varint(0).varint(1);
        w.string("minecraft:painting_variant").varint(1).string("minecraft:placeable").varint(1).varint(1);
        w.string("minecraft:jukebox_song").varint(0);
        let tags = tag_map(&regs.update_tags(&items, &w.buf).unwrap());
        assert_eq!(tags.len(), 3);
        assert_eq!(tags["minecraft:item"]["minecraft:music_discs"], vec![d::ITEMS[creator as usize] as i32]);
        assert_eq!(tags["minecraft:enchantment"]["minecraft:treasure"], vec![d::OLD_ENCHANTMENT_NAMES.iter().position(|n| *n == "mending").unwrap() as i32]);
        assert_eq!(tags["minecraft:painting_variant"]["minecraft:placeable"], vec![9]);
    }

    #[test]
    fn jukebox_event_plays_the_disc_item() {
        let regs = Registries { jukebox_songs: vec![42, -1], ..Default::default() };
        let mut w = Writer::new();
        w.i32(1010).i64(7).i32(0).bool(false);
        let out = regs.level_event(&w.buf).unwrap().unwrap();
        let mut r = Reader::new(&out);
        assert_eq!((r.i32().unwrap(), r.i64().unwrap(), r.i32().unwrap()), (1010, 7, 42));
        let mut w = Writer::new();
        w.i32(1010).i64(7).i32(1).bool(false);
        assert!(regs.level_event(&w.buf).unwrap().is_none());
    }

    #[test]
    fn update_1_21_feature_is_enabled() {
        let mut f = Writer::new();
        f.varint(1);
        f.string("minecraft:vanilla");
        let out = enabled_features(&f.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.varint().unwrap(), 2);
        r.string().unwrap();
        assert_eq!(r.string().unwrap(), "minecraft:update_1_21");
    }
}

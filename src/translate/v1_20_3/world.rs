//! World data 1.20.5 → 1.20.3: chunks, block updates, block entities, tags and registries.
//!
//! Both versions use the 1.20.2 chunk format; block states and block entity types are remapped.
//! 1.20.5 sends each synced registry in its own packet (with a known-packs handshake first),
//! 1.20.3 takes all of them at once as one registry codec, so the registries are collected here
//! and sent together before the tags (or the end of configuration).

use super::data_1_20_3 as d;
use super::items::{strip_ns, Items, BANNER_PATTERNS, DYE_COLORS};
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

const BLOCKS_PER_SECTION: usize = 4096;

/// Synced registries the 1.20.3 client reads from its registry codec.
const OLD_SYNCED: [&str; 6] = ["worldgen/biome", "chat_type", "trim_pattern", "trim_material", "dimension_type", "damage_type"];

fn err(msg: &str) -> DecodeError {
    DecodeError::Other(msg.to_string())
}

fn bits_for(count: u32) -> u32 {
    32 - (count - 1).leading_zeros()
}

fn unpack(longs: &[i64], bits: u32, count: usize) -> Vec<u32> {
    let per_long = 64 / (bits as usize).clamp(1, 64);
    let mask = (1u64 << bits) - 1;
    let mut out = Vec::with_capacity(count);
    'outer: for l in longs {
        let v = *l as u64;
        for i in 0..per_long {
            if out.len() == count {
                break 'outer;
            }
            out.push(((v >> (i as u32 * bits)) & mask) as u32);
        }
    }
    out.resize(count, 0);
    out
}

fn pack(values: &[u32], bits: u32) -> Vec<i64> {
    let per_long = 64 / (bits as usize).clamp(1, 64);
    let mut out = Vec::with_capacity(values.len().div_ceil(per_long));
    for chunk in values.chunks(per_long) {
        let mut v: u64 = 0;
        for (i, x) in chunk.iter().enumerate() {
            v |= (u64::from(*x) & ((1u64 << bits) - 1)) << (i as u32 * bits);
        }
        out.push(v as i64);
    }
    out
}

/// Block entity type on 1.20.3, `None` for types it lacks (the vault).
pub fn block_entity(ty: i32) -> Option<i32> {
    let mapped = d::BLOCK_ENTITIES.get(usize::try_from(ty).ok()?).map(|v| *v as i32)?;
    let new = d::NEW_BLOCK_ENTITY_NAMES.get(ty as usize)?;
    (d::OLD_BLOCK_ENTITY_NAMES.get(mapped as usize) == Some(new)).then_some(mapped)
}

// ---- chunks and blocks --------------------------------------------------------------------------

fn section(r: &mut Reader, w: &mut Writer) -> Result<()> {
    w.i16(r.i16()?); // non-air block count
    let bits = u32::from(r.u8()?);
    if bits == 0 {
        w.u8(0);
        w.varint(Items::block_state(r.varint()?));
        w.long_array(&r.long_array()?);
    } else if bits <= 8 {
        w.u8(bits as u8);
        let n = r.varint()?;
        if n < 0 {
            return Err(err("negative palette length"));
        }
        w.varint(n);
        for _ in 0..n {
            w.varint(Items::block_state(r.varint()?));
        }
        w.long_array(&r.long_array()?);
    } else {
        // direct palette: global ids, repacked in the client's width
        let values = unpack(&r.long_array()?, bits, BLOCKS_PER_SECTION);
        let mapped: Vec<u32> = values.iter().map(|v| Items::block_state(*v as i32).max(0) as u32).collect();
        let obits = bits_for(d::OLD_BLOCK_STATE_COUNT);
        w.u8(obits as u8);
        w.long_array(&pack(&mapped, obits));
    }
    // biomes: same ids (the registry is passed on)
    let bits = r.u8()?;
    w.u8(bits);
    if bits == 0 {
        w.varint(r.varint()?);
    } else if bits <= 3 {
        let n = r.varint()?;
        w.varint(n);
        for _ in 0..n.max(0) {
            w.varint(r.varint()?);
        }
    }
    w.long_array(&r.long_array()?);
    Ok(())
}

pub fn level_chunk_with_light(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.raw(r.bytes(8)?); // chunk x, z
    let start = r.position();
    nbt::skip(&mut r)?; // heightmaps: same
    w.raw(&payload[start..r.position()]);
    let data = r.byte_array()?;
    let mut sr = Reader::new(data);
    let mut sections = Writer::with_capacity(data.len());
    while !sr.is_empty() {
        section(&mut sr, &mut sections)?;
    }
    w.byte_array(&sections.buf);
    let n = r.varint()?;
    let mut body = Writer::new();
    let mut kept = 0;
    for _ in 0..n.max(0) {
        let xz = r.u8()?;
        let y = r.i16()?;
        let ty = r.varint()?;
        let mut tag = nbt::read(&mut r)?;
        let Some(old) = block_entity(ty) else { continue };
        fix_block_entity_tag(&mut tag);
        body.u8(xz);
        body.i16(y);
        body.varint(old);
        nbt::write(&mut body, &tag);
        kept += 1;
    }
    w.varint(kept);
    w.raw(&body.buf);
    w.raw(r.remaining()); // light: same layout
    Ok(w.into_inner())
}

pub fn block_update(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.i64(r.i64()?);
    w.varint(Items::block_state(r.varint()?));
    Ok(w.into_inner())
}

pub fn section_blocks_update(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.i64(r.i64()?);
    let n = r.varint()?;
    w.varint(n);
    for _ in 0..n.max(0) {
        let v = r.varlong()?;
        w.varlong((i64::from(Items::block_state((v >> 12) as i32)) << 12) | (v & 0xfff));
    }
    Ok(w.into_inner())
}

/// BLOCK_EVENT: block id last; `None` for blocks 1.20.3 lacks.
pub fn block_event(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::new();
    w.raw(r.bytes(10)?); // position, action, param
    let block = r.varint()?;
    if Items::block_unknown(block) {
        return Ok(None);
    }
    w.varint(Items::block(block));
    Ok(Some(w.into_inner()))
}

/// LEVEL_EVENT: block breaking names a block state, the jukebox a record item.
pub fn level_event(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let event = r.i32()?;
    w.i32(event);
    w.i64(r.i64()?);
    let data = r.i32()?;
    w.i32(match event {
        2001 => Items::block_state(data),
        1010 => Items::item(data),
        _ => data,
    });
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// BLOCK_ENTITY_DATA; `None` for block entity types 1.20.3 lacks.
pub fn block_entity_data(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.i64(r.i64()?);
    let Some(ty) = block_entity(r.varint()?) else { return Ok(None) };
    w.varint(ty);
    let mut tag = nbt::read(&mut r)?;
    fix_block_entity_tag(&mut tag);
    nbt::write(&mut w, &tag);
    Ok(Some(w.into_inner()))
}

/// Block entity NBT the client renders from: banner patterns and skull owners use the 1.20.3 keys.
pub fn fix_block_entity_tag(tag: &mut Tag) {
    if let Some(profile) = tag.remove("profile") {
        let owner = match profile {
            Tag::String(_) => profile,
            p => {
                let mut owner = Tag::compound();
                if let Some(n) = p.get("name") {
                    owner.insert("Name", n.clone());
                }
                if let Some(id) = p.get("id") {
                    owner.insert("Id", id.clone());
                }
                let mut props = Tag::compound();
                for prop in p.get("properties").and_then(Tag::as_list).into_iter().flatten() {
                    let name = prop.get("name").and_then(Tag::as_str).unwrap_or("").to_string();
                    let mut v = Tag::compound();
                    v.insert("Value", Tag::String(prop.get("value").and_then(Tag::as_str).unwrap_or("").to_string()));
                    if let Some(s) = prop.get("signature") {
                        v.insert("Signature", s.clone());
                    }
                    match props.get_mut(&name) {
                        Some(Tag::List(l)) => l.push(v),
                        _ => props.insert(&name, Tag::List(vec![v])),
                    }
                }
                owner.insert("Properties", props);
                owner
            }
        };
        tag.insert("SkullOwner", owner);
    }
    if let Some(Tag::List(patterns)) = tag.remove("patterns") {
        let list = patterns
            .iter()
            .filter_map(|p| {
                let name = strip_ns(p.get("pattern")?.as_str()?);
                let code = BANNER_PATTERNS.iter().find(|(n, c)| *n == name && !c.is_empty())?.1;
                let color = DYE_COLORS.iter().position(|c| Some(*c) == p.get("color").and_then(Tag::as_str)).unwrap_or(0);
                let mut t = Tag::compound();
                t.insert("Pattern", Tag::String(code.to_string()));
                t.insert("Color", Tag::Int(color as i32));
                Some(t)
            })
            .collect();
        tag.insert("Patterns", Tag::List(list));
    }
}

// ---- registries ---------------------------------------------------------------------------------

/// The server's synced registries, collected for the 1.20.3 registry codec.
#[derive(Default)]
pub struct Registries {
    /// registry key (no namespace) → `{type, value: [{name, id, element}]}`
    codec: Vec<(String, Tag)>,
    sent: bool,
    /// dimension type keys by id (LOGIN and RESPAWN name the dimension type)
    pub dimension_types: Vec<String>,
}

/// What a REGISTRY_DATA packet updated (for the item converter).
pub enum Recorded {
    None,
    BannerPatterns(Vec<String>),
    TrimPatterns(Vec<String>),
    TrimMaterials(Vec<String>),
}

impl Registries {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// REGISTRY_DATA (1.20.5): records the registry; nothing is sent to the client yet.
    pub fn registry_data(&mut self, payload: &[u8]) -> Result<Recorded> {
        let mut r = Reader::new(payload);
        let key = strip_ns(r.string()?).to_string();
        let entries = r.list(|r| {
            let name = r.string()?.to_string();
            let data = if r.bool()? { Some(nbt::read(r)?) } else { None };
            Ok((name, data))
        })?;
        let keys: Vec<String> = entries.iter().map(|(k, _)| k.clone()).collect();
        let recorded = match key.as_str() {
            "banner_pattern" => return Ok(Recorded::BannerPatterns(keys)),
            "trim_pattern" => Recorded::TrimPatterns(keys),
            "trim_material" => Recorded::TrimMaterials(keys),
            "dimension_type" => {
                self.dimension_types = keys;
                Recorded::None
            }
            _ => Recorded::None,
        };
        if !OLD_SYNCED.contains(&key.as_str()) {
            return Ok(recorded);
        }
        let mut value = Vec::with_capacity(entries.len());
        for (id, (name, data)) in entries.into_iter().enumerate() {
            let mut element = data.unwrap_or_else(Tag::compound);
            match key.as_str() {
                "worldgen/biome" => fix_biome(&mut element),
                "dimension_type" => fix_dimension_type(&mut element),
                // patterns of templates 1.20.3 lacks would fail its codec
                "trim_pattern" => {
                    let template = element.get("template_item").and_then(Tag::as_str).unwrap_or("");
                    if !d::OLD_ITEM_NAMES.contains(&strip_ns(template)) {
                        continue;
                    }
                }
                _ => {}
            }
            let mut e = Tag::compound();
            e.insert("name", Tag::String(name));
            e.insert("id", Tag::Int(id as i32));
            e.insert("element", element);
            value.push(e);
        }
        let mut reg = Tag::compound();
        reg.insert("type", Tag::String(key.clone()));
        reg.insert("value", Tag::List(value));
        match self.codec.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = reg,
            None => self.codec.push((key, reg)),
        }
        Ok(recorded)
    }

    /// The 1.20.3 REGISTRY_DATA payload (all registries in one compound), once per configuration.
    pub fn take_codec(&mut self) -> Option<Vec<u8>> {
        if self.sent || self.codec.is_empty() {
            return None;
        }
        self.sent = true;
        let mut w = Writer::new();
        nbt::write(&mut w, &Tag::Compound(self.codec.clone()));
        Some(w.into_inner())
    }

    pub fn dimension_type(&self, id: i32) -> String {
        usize::try_from(id).ok().and_then(|i| self.dimension_types.get(i)).cloned().unwrap_or_else(|| "minecraft:overworld".into())
    }
}

/// Biome particle options use the 1.20.3 keys.
fn fix_biome(biome: &mut Tag) {
    let Some(options) = biome.get_mut("effects").and_then(|e| e.get_mut("particle")).and_then(|p| p.get_mut("options")) else { return };
    let ty = options.get("type").and_then(Tag::as_str).map(|t| strip_ns(t).to_string()).unwrap_or_default();
    let rename = |o: &mut Tag, from: &str, to: &str| {
        if let Some(v) = o.remove(from) {
            o.insert(to, v);
        }
    };
    match ty.as_str() {
        "block" | "block_marker" | "falling_dust" | "dust_pillar" => {
            if let Some(state) = options.remove("block_state") {
                let state = match state {
                    Tag::String(name) => Tag::Compound(vec![("Name".into(), Tag::String(name))]),
                    other => other,
                };
                options.insert("value", state);
            }
        }
        "item" => {
            if let Some(item) = options.remove("item") {
                let item = match item {
                    Tag::String(id) => Tag::Compound(vec![("id".into(), Tag::String(id))]),
                    other => other,
                };
                options.insert("value", item);
            }
        }
        "dust_color_transition" => {
            rename(options, "from_color", "fromColor");
            rename(options, "to_color", "toColor");
        }
        "entity_effect" => {
            if let Some(color) = options.remove("color") {
                let color = match color {
                    Tag::List(parts) => {
                        let c: Vec<f64> = parts
                            .iter()
                            .map(|p| match p {
                                Tag::Float(v) => f64::from(*v),
                                Tag::Double(v) => *v,
                                _ => 0.0,
                            })
                            .collect();
                        let part = |i: usize| ((c.get(i).copied().unwrap_or(0.0) * 255.0).floor() as i32) & 0xff;
                        Tag::Float((part(0) << 24 | part(1) << 16 | part(2) << 8 | part(3)) as f32)
                    }
                    other => other,
                };
                options.insert("value", color);
            }
        }
        _ => {}
    }
}

/// 1.20.3 reads a monster spawn light range wrapped in `value`.
fn fix_dimension_type(dim: &mut Tag) {
    let Some(level @ Tag::Compound(_)) = dim.get_mut("monster_spawn_light_level") else { return };
    let mut value = Tag::compound();
    for k in ["min_inclusive", "max_inclusive"] {
        value.insert(k, Tag::Int(level.get(k).and_then(Tag::as_i32).unwrap_or(0)));
    }
    level.insert("value", value);
}

// ---- tags ---------------------------------------------------------------------------------------

/// UPDATE_TAGS: remaps item, block, entity, enchantment and banner pattern ids; registries 1.20.3
/// does not sync (wolf variants) are left out.
pub fn update_tags(items: &Items, payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let n = r.varint()?;
    let mut kept = 0;
    let mut body = Writer::with_capacity(payload.len());
    for _ in 0..n.max(0) {
        let registry = r.string()?.to_string();
        let key = strip_ns(&registry).to_string();
        let tags = r.list(|r| Ok((r.string()?.to_string(), r.list(|r| r.varint())?)))?;
        if key == "wolf_variant" {
            continue;
        }
        body.string(&registry);
        body.varint(tags.len() as i32);
        for (name, ids) in tags {
            let name = if key == "item" && strip_ns(&name) == "axolotl_food" { "minecraft:axolotl_tempt_items".to_string() } else { name };
            body.string(&name);
            let mapped: Vec<i32> = ids
                .iter()
                .filter_map(|id| match key.as_str() {
                    "item" => Some(Items::item(*id)),
                    "block" => (!Items::block_unknown(*id)).then(|| Items::block(*id)),
                    "entity_type" => Some(Items::entity(*id)),
                    "enchantment" => Items::old_enchantment(*id),
                    "banner_pattern" => items.old_banner_pattern(*id),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn registry(name: &str, entries: &[(&str, Tag)]) -> Vec<u8> {
        let mut w = Writer::new();
        w.string(name);
        w.varint(entries.len() as i32);
        for (k, t) in entries {
            w.string(k);
            w.bool(true);
            nbt::write(&mut w, t);
        }
        w.into_inner()
    }

    #[test]
    fn registries_become_one_codec() {
        let mut regs = Registries::default();
        let mut light = Tag::compound();
        light.insert("type", Tag::String("minecraft:uniform".into()));
        light.insert("min_inclusive", Tag::Int(0));
        light.insert("max_inclusive", Tag::Int(7));
        let mut overworld = Tag::compound();
        overworld.insert("monster_spawn_light_level", light);
        let dims = registry("minecraft:dimension_type", &[("minecraft:overworld", overworld), ("minecraft:the_nether", Tag::compound())]);
        assert!(matches!(regs.registry_data(&dims).unwrap(), Recorded::None));
        let mut flow = Tag::compound();
        flow.insert("template_item", Tag::String("minecraft:flow_armor_trim_smithing_template".into()));
        let mut coast = Tag::compound();
        coast.insert("template_item", Tag::String("minecraft:coast_armor_trim_smithing_template".into()));
        assert!(matches!(regs.registry_data(&registry("minecraft:trim_pattern", &[("minecraft:flow", flow), ("minecraft:coast", coast)])).unwrap(), Recorded::TrimPatterns(k) if k.len() == 2));
        // registries 1.20.3 does not sync are recorded but left out
        assert!(matches!(regs.registry_data(&registry("minecraft:banner_pattern", &[("minecraft:base", Tag::compound())])).unwrap(), Recorded::BannerPatterns(_)));
        regs.registry_data(&registry("minecraft:wolf_variant", &[("minecraft:pale", Tag::compound())])).unwrap();
        assert_eq!(regs.dimension_type(1), "minecraft:the_nether");

        let codec = regs.take_codec().unwrap();
        assert!(regs.take_codec().is_none(), "sent once");
        let tag = nbt::read(&mut Reader::new(&codec)).unwrap();
        let Tag::Compound(top) = &tag else { panic!() };
        assert_eq!(top.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), vec!["dimension_type", "trim_pattern"]);
        let dims = tag.get("dimension_type").unwrap();
        assert_eq!(dims.get("type"), Some(&Tag::String("dimension_type".into())));
        let first = &dims.get("value").unwrap().as_list().unwrap()[0];
        assert_eq!(first.get("name"), Some(&Tag::String("minecraft:overworld".into())));
        assert_eq!(first.get("id"), Some(&Tag::Int(0)));
        let level = first.get("element").unwrap().get("monster_spawn_light_level").unwrap();
        assert_eq!(level.get("value").unwrap().get("max_inclusive"), Some(&Tag::Int(7)));
        // the flow pattern is left out, coast keeps its id
        let patterns = tag.get("trim_pattern").unwrap().get("value").unwrap().as_list().unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].get("id"), Some(&Tag::Int(1)));
    }

    #[test]
    fn block_entities_and_tags() {
        let vault = d::NEW_BLOCK_ENTITY_NAMES.iter().position(|n| *n == "vault").unwrap() as i32;
        assert_eq!(block_entity(vault), None);
        let chest = d::NEW_BLOCK_ENTITY_NAMES.iter().position(|n| *n == "chest").unwrap() as i32;
        assert_eq!(block_entity(chest), Some(d::OLD_BLOCK_ENTITY_NAMES.iter().position(|n| *n == "chest").unwrap() as i32));
        let mut p = Tag::compound();
        p.insert("pattern", Tag::String("minecraft:creeper".into()));
        p.insert("color", Tag::String("lime".into()));
        let mut banner = Tag::compound();
        banner.insert("patterns", Tag::List(vec![p]));
        fix_block_entity_tag(&mut banner);
        let pat = &banner.get("Patterns").unwrap().as_list().unwrap()[0];
        assert_eq!((pat.get("Pattern"), pat.get("Color")), (Some(&Tag::String("cre".into())), Some(&Tag::Int(5))));

        let heavy_core = d::NEW_BLOCK_NAMES.iter().position(|n| *n == "heavy_core").unwrap() as i32;
        let stone = d::NEW_BLOCK_NAMES.iter().position(|n| *n == "stone").unwrap() as i32;
        let mut w = Writer::new();
        w.varint(1).string("minecraft:block").varint(1).string("minecraft:test").varint(2).varint(heavy_core).varint(stone);
        let out = update_tags(&Items::new(), &w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.varint().unwrap(), 1);
        r.string().unwrap();
        r.varint().unwrap();
        r.string().unwrap();
        assert_eq!(r.list(|r| r.varint()).unwrap(), vec![1], "heavy core is left out");
    }
}

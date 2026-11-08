//! Item stacks 1.20.5 ↔ 1.20.3.
//!
//! 1.20.5 replaced item NBT with typed data components. A 1.20.3 item is `present, id, count,
//! optional NBT`, so every component is written into the NBT keys 1.20.3 reads (display name and
//! lore, enchantments, damage, potions, block entity data, ...), and NBT coming from the client is
//! turned back into components.
//!
//! When an item does not come back from its 1.20.3 form exactly as the server sent it (components
//! 1.20.3 has no NBT for, removed components, items 1.20.3 lacks), the server's original id and
//! component patch are kept under [`BACKUP_KEY`] in the client's NBT and restored when the client
//! sends the item back (creative inventory, container clicks). Items that convert both ways without
//! loss carry no backup, so they stay equal to the client's own items.

use super::data_1_20_3 as d;
use super::text;
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

/// Root NBT key holding the server's (1.20.5) item: `{id: int, c: byte array (component patch)}`.
pub const BACKUP_KEY: &str = "mineversed:v1_20_5";

fn err(msg: &str) -> DecodeError {
    DecodeError::Other(msg.to_string())
}

pub fn strip_ns(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}

fn ns(s: &str) -> String {
    if s.contains(':') { s.to_string() } else { format!("minecraft:{s}") }
}

/// Bytes consumed since `start`.
fn raw_since(r: &mut Reader, start: usize) -> Result<Vec<u8>> {
    let end = r.position();
    r.set_position(start);
    Ok(r.bytes(end - start)?.to_vec())
}

/// "wolf_armor" → "Wolf Armor".
fn pretty_name(id: &str) -> String {
    strip_ns(id)
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---- built-in registries (1.20.5 ids; 1.20.3 uses the names) ------------------------------------

/// Potions, 1.20.5 order (1.20.3 lacks the last four).
pub const POTIONS: [&str; 46] = [
    "water", "mundane", "thick", "awkward", "night_vision", "long_night_vision", "invisibility", "long_invisibility", "leaping",
    "long_leaping", "strong_leaping", "fire_resistance", "long_fire_resistance", "swiftness", "long_swiftness", "strong_swiftness",
    "slowness", "long_slowness", "strong_slowness", "turtle_master", "long_turtle_master", "strong_turtle_master", "water_breathing",
    "long_water_breathing", "healing", "strong_healing", "harming", "strong_harming", "poison", "long_poison", "strong_poison",
    "regeneration", "long_regeneration", "strong_regeneration", "strength", "long_strength", "strong_strength", "weakness",
    "long_weakness", "luck", "slow_falling", "long_slow_falling", "wind_charged", "weaving", "oozing", "infested",
];

/// Mob effects, 1.20.5 order. 1.20.3 has the first [`OLD_EFFECT_COUNT`] with the same ids.
pub const EFFECTS: [&str; 39] = [
    "speed", "slowness", "haste", "mining_fatigue", "strength", "instant_health", "instant_damage", "jump_boost", "nausea",
    "regeneration", "resistance", "fire_resistance", "water_breathing", "invisibility", "blindness", "night_vision", "hunger",
    "weakness", "poison", "wither", "health_boost", "absorption", "saturation", "glowing", "levitation", "luck", "unluck",
    "slow_falling", "conduit_power", "dolphins_grace", "bad_omen", "hero_of_the_village", "darkness", "trial_omen", "raid_omen",
    "wind_charged", "weaving", "oozing", "infested",
];
pub const OLD_EFFECT_COUNT: i32 = 33;

const INSTRUMENTS: [&str; 8] =
    ["ponder_goat_horn", "sing_goat_horn", "seek_goat_horn", "feel_goat_horn", "admire_goat_horn", "call_goat_horn", "yearn_goat_horn", "dream_goat_horn"];

/// Built-in banner patterns (1.20.5 order; 1.20.3 has all but flow and guster, same order) and
/// the short codes 1.20.3 NBT uses for them.
pub const BANNER_PATTERNS: [(&str, &str); 43] = [
    ("base", "b"), ("square_bottom_left", "bl"), ("square_bottom_right", "br"), ("square_top_left", "tl"), ("square_top_right", "tr"),
    ("stripe_bottom", "bs"), ("stripe_top", "ts"), ("stripe_left", "ls"), ("stripe_right", "rs"), ("stripe_center", "cs"),
    ("stripe_middle", "ms"), ("stripe_downright", "drs"), ("stripe_downleft", "dls"), ("small_stripes", "ss"), ("cross", "cr"),
    ("straight_cross", "sc"), ("triangle_bottom", "bt"), ("triangle_top", "tt"), ("triangles_bottom", "bts"), ("triangles_top", "tts"),
    ("diagonal_left", "ld"), ("diagonal_up_right", "rd"), ("diagonal_up_left", "lud"), ("diagonal_right", "rud"), ("circle", "mc"),
    ("rhombus", "mr"), ("half_vertical", "vh"), ("half_horizontal", "hh"), ("half_vertical_right", "vhr"),
    ("half_horizontal_bottom", "hhb"), ("border", "bo"), ("curly_border", "cbo"), ("gradient", "gra"), ("gradient_up", "gru"),
    ("bricks", "bri"), ("globe", "glb"), ("creeper", "cre"), ("skull", "sku"), ("flower", "flo"), ("mojang", "moj"), ("piglin", "pig"),
    ("flow", ""), ("guster", ""),
];

/// Equipment slot groups of attribute modifiers (1.20.5 ids).
const SLOT_GROUPS: [&str; 10] = ["any", "mainhand", "offhand", "hand", "feet", "legs", "chest", "head", "armor", "body"];

const MAP_DECORATIONS: [&str; 35] = [
    "player", "frame", "red_marker", "blue_marker", "target_x", "target_point", "player_off_map", "player_off_limits", "mansion",
    "monument", "banner_white", "banner_orange", "banner_magenta", "banner_light_blue", "banner_yellow", "banner_lime", "banner_pink",
    "banner_gray", "banner_light_gray", "banner_cyan", "banner_purple", "banner_blue", "banner_brown", "banner_green", "banner_red",
    "banner_black", "red_x", "village_desert", "village_plains", "village_savanna", "village_snowy", "village_taiga", "jungle_temple",
    "swamp_hut", "trial_chambers",
];

pub const DYE_COLORS: [&str; 16] =
    ["white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray", "cyan", "purple", "blue", "brown", "green", "red", "black"];

/// Root NBT keys 1.20.3 bucket items keep their mob's data in.
const MOB_TAGS: [&str; 10] = ["NoAI", "Silent", "NoGravity", "Glowing", "Invulnerable", "Health", "Age", "Variant", "HuntingCooldown", "BucketVariantTag"];

// hide flags (1.20.3 HideFlags bits)
const HIDE_ENCHANTMENTS: i32 = 1;
const HIDE_ATTRIBUTES: i32 = 1 << 1;
const HIDE_UNBREAKABLE: i32 = 1 << 2;
const HIDE_CAN_DESTROY: i32 = 1 << 3;
const HIDE_CAN_PLACE_ON: i32 = 1 << 4;
const HIDE_ADDITIONAL: i32 = 1 << 5;
const HIDE_DYE: i32 = 1 << 6;
const HIDE_TRIM: i32 = 1 << 7;

fn position(names: &[&str], name: &str) -> Option<i32> {
    let n = strip_ns(name);
    names.iter().position(|x| *x == n).map(|i| i as i32)
}

fn name_at<'a>(names: &[&'a str], id: i32) -> Option<&'a str> {
    usize::try_from(id).ok().and_then(|i| names.get(i)).copied()
}

/// Block position packed into a long (x 26 bits, z 26 bits, y 12 bits).
fn pack_pos(x: i32, y: i32, z: i32) -> i64 {
    ((i64::from(x) & 0x3ff_ffff) << 38) | ((i64::from(z) & 0x3ff_ffff) << 12) | (i64::from(y) & 0xfff)
}

fn unpack_pos(v: i64) -> (i32, i32, i32) {
    ((v >> 38) as i32, (v << 52 >> 52) as i32, (v << 26 >> 38) as i32)
}

pub fn uuid_to_ints(u: u128) -> Tag {
    Tag::IntArray((0..4).map(|i| (u >> (96 - 32 * i)) as u32 as i32).collect())
}

fn uuid_from_ints(t: &Tag) -> Option<u128> {
    match t {
        Tag::IntArray(v) if v.len() == 4 => Some(v.iter().fold(0u128, |acc, x| (acc << 32) | u128::from(*x as u32))),
        _ => None,
    }
}

fn component_id(name: &str) -> i32 {
    position(&d::NEW_COMPONENT_NAMES, name).expect("1.20.5 component")
}

fn take_i32(tag: &mut Tag, key: &str) -> Option<i32> {
    match tag.get(key) {
        Some(t) if t.as_i32().is_some() => tag.remove(key).and_then(|t| t.as_i32()),
        _ => None,
    }
}

fn take_string(tag: &mut Tag, key: &str) -> Option<String> {
    match tag.get(key) {
        Some(Tag::String(_)) => tag.remove(key).and_then(|t| match t {
            Tag::String(s) => Some(s),
            _ => None,
        }),
        _ => None,
    }
}

fn take_list(tag: &mut Tag, key: &str) -> Option<Vec<Tag>> {
    match tag.get(key) {
        Some(Tag::List(_)) => tag.remove(key).and_then(|t| match t {
            Tag::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

fn take_compound(tag: &mut Tag, key: &str) -> Option<Tag> {
    match tag.get(key) {
        Some(Tag::Compound(_)) => tag.remove(key),
        _ => None,
    }
}

fn is_empty_compound(t: &Tag) -> bool {
    matches!(t, Tag::Compound(e) if e.is_empty())
}

fn f64_of(t: Option<&Tag>) -> Option<f64> {
    match t? {
        Tag::Float(v) => Some(f64::from(*v)),
        Tag::Double(v) => Some(*v),
        other => other.as_i32().map(f64::from),
    }
}

/// 1.20.3 NBT being built from one item's components.
#[derive(Default)]
struct Legacy {
    tag: Vec<(String, Tag)>,
    custom_data: Option<Tag>,
    hide: i32,
    item_name: Option<Tag>,
}

impl Legacy {
    fn root(&mut self) -> &mut Vec<(String, Tag)> {
        &mut self.tag
    }
    fn put(&mut self, key: &str, value: Tag) {
        if let Some(slot) = self.tag.iter_mut().find(|(k, _)| k == key) {
            slot.1 = value;
        } else {
            self.tag.push((key.to_string(), value));
        }
    }
    /// A compound under the root, created when missing.
    fn sub(&mut self, key: &str) -> &mut Tag {
        if !self.tag.iter().any(|(k, v)| k == key && matches!(v, Tag::Compound(_))) {
            self.put(key, Tag::compound());
        }
        &mut self.tag.iter_mut().find(|(k, _)| k == key).expect("just inserted").1
    }
    /// BlockEntityTag with the block entity id it needs when components create it.
    fn block_entity(&mut self, id: Option<&str>) -> &mut Tag {
        let be = self.sub("BlockEntityTag");
        if let Some(id) = id {
            if be.get("id").is_none() {
                be.insert("id", Tag::String(id.to_string()));
            }
        }
        be
    }
}

pub struct Items {
    /// The server's banner pattern registry (1.20.5 id → key); built-in order until it arrives.
    pub banner_patterns: Vec<String>,
    pub trim_patterns: Vec<String>,
    pub trim_materials: Vec<String>,
}

impl Default for Items {
    fn default() -> Self {
        Self::new()
    }
}

impl Items {
    pub fn new() -> Self {
        Self { banner_patterns: BANNER_PATTERNS.iter().map(|(n, _)| ns(n)).collect(), trim_patterns: Vec::new(), trim_materials: Vec::new() }
    }

    // ---- id remaps -------------------------------------------------------------------------

    pub fn item(id: i32) -> i32 {
        if id < 0 {
            return id;
        }
        d::ITEMS.get(id as usize).map_or(id, |v| *v as i32)
    }
    pub fn item_to_server(id: i32) -> i32 {
        if id < 0 {
            return id;
        }
        d::ITEMS_REVERSE.get(id as usize).map_or(id, |v| *v as i32)
    }
    /// 1.20.5 item that is shown as a different item on 1.20.3 (mace, wolf armor, ...).
    fn substituted(id: i32) -> bool {
        name_at(&d::NEW_ITEM_NAMES, id) != name_at(&d::OLD_ITEM_NAMES, Self::item(id))
    }
    fn new_item_name(id: i32) -> &'static str {
        name_at(&d::NEW_ITEM_NAMES, id).unwrap_or("air")
    }
    /// "minecraft:<1.20.3 name>" of a 1.20.5 item.
    pub fn old_item_key(id: i32) -> String {
        ns(name_at(&d::OLD_ITEM_NAMES, Self::item(id)).unwrap_or("air"))
    }
    /// 1.20.5 item id for a 1.20.3 item key.
    fn item_from_old_key(key: &str) -> Option<i32> {
        position(&d::OLD_ITEM_NAMES, key).map(Self::item_to_server)
    }
    pub fn block(id: i32) -> i32 {
        if id < 0 {
            return id;
        }
        d::BLOCKS.get(id as usize).map_or(id, |v| *v as i32)
    }
    /// Blocks 1.20.3 has no equivalent for (the table falls back to stone for them).
    pub fn block_unknown(id: i32) -> bool {
        Self::block(id) == 1 && name_at(&d::NEW_BLOCK_NAMES, id) != Some("stone")
    }
    pub fn entity(id: i32) -> i32 {
        if id < 0 {
            return id;
        }
        d::ENTITIES.get(id as usize).map_or(id, |v| *v as i32)
    }
    pub fn block_state(id: i32) -> i32 {
        if id < 0 {
            return id;
        }
        d::BLOCK_STATES.get(id as usize).map_or(id, |v| *v as i32)
    }
    /// Sound id on 1.20.3, `None` when 1.20.3 has no sound for it.
    pub fn sound(id: i32) -> Option<i32> {
        let mapped = d::SOUNDS.get(usize::try_from(id).ok()?).map(|v| *v as i32)?;
        if mapped == 0 && name_at(&d::NEW_SOUND_NAMES, id) != name_at(&d::OLD_SOUND_NAMES, 0) {
            return None;
        }
        Some(mapped)
    }
    /// 1.20.3 attribute key ("minecraft:generic.armor") of a 1.20.5 attribute, if 1.20.3 has it.
    /// `horse` decides whether jump strength exists (1.20.3 only has it for horses).
    pub fn old_attribute_key(id: i32, horse: bool) -> Option<String> {
        let name = name_at(&d::NEW_ATTRIBUTE_NAMES, id)?;
        if name == "generic.jump_strength" {
            return horse.then(|| "minecraft:horse.jump_strength".to_string());
        }
        let mapped = d::ATTRIBUTES.get(id as usize).map(|v| *v as i32)?;
        if mapped == 0 && id != 0 {
            return None;
        }
        name_at(&d::OLD_ATTRIBUTE_NAMES, mapped).map(ns)
    }
    fn attribute_from_old_key(key: &str) -> Option<i32> {
        let k = strip_ns(key);
        let k = if k == "horse.jump_strength" { "generic.jump_strength" } else { k };
        position(&d::NEW_ATTRIBUTE_NAMES, k)
    }
    /// 1.20.3 enchantment key of a 1.20.5 enchantment id.
    pub fn old_enchantment_key(id: i32) -> Option<String> {
        let name = name_at(&d::NEW_ENCHANTMENT_NAMES, id)?;
        let name = if name == "sweeping_edge" { "sweeping" } else { name };
        position(&d::OLD_ENCHANTMENT_NAMES, name).map(|_| ns(name))
    }
    /// 1.20.3 enchantment registry id of a 1.20.5 one (enchanting table hints).
    pub fn old_enchantment(id: i32) -> Option<i32> {
        Self::old_enchantment_key(id).and_then(|k| position(&d::OLD_ENCHANTMENT_NAMES, &k))
    }
    fn enchantment_from_old_key(key: &str) -> Option<i32> {
        let k = strip_ns(key);
        let k = if k == "sweeping" { "sweeping_edge" } else { k };
        position(&d::NEW_ENCHANTMENT_NAMES, k)
    }
    fn banner_pattern_key(&self, id: i32) -> Option<&str> {
        usize::try_from(id).ok().and_then(|i| self.banner_patterns.get(i)).map(|s| strip_ns(s))
    }
    /// Banner pattern id on 1.20.3 (built in) of a server registry id.
    pub fn old_banner_pattern(&self, id: i32) -> Option<i32> {
        let key = self.banner_pattern_key(id)?;
        BANNER_PATTERNS.iter().position(|(n, c)| *n == key && !c.is_empty()).map(|i| i as i32)
    }

    // ---- 1.20.5 → 1.20.3 -------------------------------------------------------------------

    /// One item stack (1.20.5 wire format, count first) as a 1.20.3 one (present flag first).
    pub fn to_client(&self, r: &mut Reader, w: &mut Writer) -> Result<()> {
        match self.read_stack(r, true)? {
            None => {
                w.bool(false);
            }
            Some((id, count, tag)) => {
                w.bool(true);
                w.varint(Self::item(id));
                w.i8(count.clamp(1, 127) as i8);
                nbt::write_optional(w, tag.as_ref());
            }
        }
        Ok(())
    }

    /// Reads a 1.20.5 stack; returns its 1.20.5 id, count and 1.20.3 NBT (with a backup when the
    /// item would not come back as it is and `backup` is set).
    fn read_stack(&self, r: &mut Reader, backup: bool) -> Result<Option<(i32, i32, Option<Tag>)>> {
        let count = r.varint()?;
        if count <= 0 {
            return Ok(None);
        }
        let id = r.varint()?;
        let patch_start = r.position();
        let added = r.varint()?;
        let removed = r.varint()?;
        if added < 0 || removed < 0 || added + removed > 512 {
            return Err(err("bad component counts"));
        }
        let mut lg = Legacy::default();
        let mut comps: Vec<(i32, Vec<u8>)> = Vec::with_capacity(added as usize);
        for _ in 0..added {
            let ty = r.varint()?;
            let name = name_at(&d::NEW_COMPONENT_NAMES, ty).ok_or_else(|| err(&format!("unknown data component type {ty}")))?;
            let start = r.position();
            self.component(name, r, &mut lg)?;
            comps.push((ty, raw_since(r, start)?));
        }
        for _ in 0..removed {
            r.varint()?;
        }
        let patch = raw_since(r, patch_start)?;
        let mut tag = self.finish(id, lg);
        if backup {
            comps.sort_by_key(|(t, _)| *t);
            let back = self.to_components(Self::item(id), tag.clone());
            if removed > 0 || back.0 != id || back.1 != comps {
                let mut b = Tag::compound();
                b.insert("id", Tag::Int(id));
                b.insert("c", Tag::ByteArray(patch.iter().map(|x| *x as i8).collect()));
                let mut t = tag.unwrap_or_else(Tag::compound);
                t.insert(BACKUP_KEY, b);
                tag = Some(t);
            }
        }
        Ok(Some((id, count, tag)))
    }

    /// A nested item (bundle, container, crossbow) as 1.20.3 saves it: `{id, Count, tag}`.
    fn nested_to_tag(&self, r: &mut Reader) -> Result<Tag> {
        let mut t = Tag::compound();
        match self.read_stack(r, true)? {
            None => {
                t.insert("id", Tag::String("minecraft:air".into()));
                t.insert("Count", Tag::Byte(0));
            }
            Some((id, count, tag)) => {
                t.insert("id", Tag::String(Self::old_item_key(id)));
                t.insert("Count", Tag::Byte(count.clamp(1, 127) as i8));
                if let Some(tag) = tag {
                    t.insert("tag", tag);
                }
            }
        }
        Ok(t)
    }

    /// Merchant item cost: id, count, exact components; 1.20.3 gets a plain item without backup so
    /// the client can match its own items against it.
    pub fn item_cost(&self, r: &mut Reader, w: &mut Writer) -> Result<()> {
        let id = r.varint()?;
        let count = r.varint()?;
        let n = r.varint()?;
        let mut lg = Legacy::default();
        for _ in 0..n.max(0) {
            let ty = r.varint()?;
            let name = name_at(&d::NEW_COMPONENT_NAMES, ty).ok_or_else(|| err(&format!("unknown data component type {ty}")))?;
            self.component(name, r, &mut lg)?;
        }
        let mut tag = self.finish(id, lg);
        // the stand-in name would stop the client's matching; a cost is only a template
        if Self::substituted(id) {
            if let Some(t) = tag.as_mut() {
                t.remove("display");
            }
            tag = tag.filter(|t| !is_empty_compound(t));
        }
        w.bool(true);
        w.varint(Self::item(id));
        w.i8(count.clamp(1, 127) as i8);
        nbt::write_optional(w, tag.as_ref());
        Ok(())
    }

    pub fn opt_item_cost(&self, r: &mut Reader, w: &mut Writer) -> Result<()> {
        if r.bool()? {
            self.item_cost(r, w)
        } else {
            w.bool(false);
            Ok(())
        }
    }

    /// The finished 1.20.3 NBT: custom data first, the converted keys on top.
    fn finish(&self, id: i32, mut lg: Legacy) -> Option<Tag> {
        if let Some(name) = lg.item_name.take() {
            let display = lg.sub("display");
            if display.get("Name").is_none() {
                let mut wrapper = Tag::compound();
                wrapper.insert("italic", Tag::Byte(0));
                wrapper.insert("text", Tag::String(String::new()));
                wrapper.insert("extra", Tag::List(vec![name]));
                display.insert("Name", Tag::String(text::to_json(&wrapper)));
            }
        }
        if Self::substituted(id) && !lg.tag.iter().any(|(k, v)| k == "display" && v.get("Name").is_some()) {
            let mut name = Tag::compound();
            name.insert("text", Tag::String(pretty_name(Self::new_item_name(id))));
            name.insert("italic", Tag::Byte(0));
            lg.sub("display").insert("Name", Tag::String(text::to_json(&name)));
        }
        if lg.hide != 0 {
            lg.put("HideFlags", Tag::Int(lg.hide));
        }
        let mut root = match lg.custom_data.take() {
            Some(Tag::Compound(e)) => Tag::Compound(e),
            _ => Tag::compound(),
        };
        for (k, v) in std::mem::take(lg.root()) {
            match (root.get_mut(&k), v) {
                (Some(Tag::Compound(existing)), Tag::Compound(add)) => {
                    for (ak, av) in add {
                        if let Some(slot) = existing.iter_mut().find(|(ek, _)| *ek == ak) {
                            slot.1 = av;
                        } else {
                            existing.push((ak, av));
                        }
                    }
                }
                (_, v) => root.insert(&k, v),
            }
        }
        if is_empty_compound(&root) { None } else { Some(root) }
    }

    fn component(&self, name: &str, r: &mut Reader, lg: &mut Legacy) -> Result<()> {
        match name {
            "custom_data" => lg.custom_data = Some(nbt::read(r)?),
            "damage" => {
                let v = r.varint()?;
                lg.put("Damage", Tag::Int(v));
            }
            "unbreakable" => {
                lg.put("Unbreakable", Tag::Byte(1));
                if !r.bool()? {
                    lg.hide |= HIDE_UNBREAKABLE;
                }
            }
            "custom_name" => {
                let t = nbt::read(r)?;
                lg.sub("display").insert("Name", Tag::String(text::to_json(&t)));
            }
            "item_name" => lg.item_name = Some(nbt::read(r)?),
            "lore" => {
                let lines = r.list(|r| Ok(Tag::String(text::to_json(&nbt::read(r)?))))?;
                lg.sub("display").insert("Lore", Tag::List(lines));
            }
            "enchantments" | "stored_enchantments" => {
                let n = r.varint()?;
                let mut list = Vec::new();
                for _ in 0..n.max(0) {
                    let id = r.varint()?;
                    let level = r.varint()?;
                    if let Some(key) = Self::old_enchantment_key(id) {
                        let mut e = Tag::compound();
                        e.insert("id", Tag::String(key));
                        e.insert("lvl", Tag::Short(level.clamp(i16::MIN.into(), i16::MAX.into()) as i16));
                        list.push(e);
                    }
                }
                let stored = name == "stored_enchantments";
                if !r.bool()? {
                    lg.hide |= if stored { HIDE_ADDITIONAL } else { HIDE_ENCHANTMENTS };
                }
                lg.put(if stored { "StoredEnchantments" } else { "Enchantments" }, Tag::List(list));
            }
            "can_place_on" | "can_break" => {
                let n = r.varint()?;
                let mut list = Vec::new();
                for _ in 0..n.max(0) {
                    self.block_predicate(r, &mut list)?;
                }
                let place = name == "can_place_on";
                if !r.bool()? {
                    lg.hide |= if place { HIDE_CAN_PLACE_ON } else { HIDE_CAN_DESTROY };
                }
                lg.put(if place { "CanPlaceOn" } else { "CanDestroy" }, Tag::List(list));
            }
            "attribute_modifiers" => {
                let n = r.varint()?;
                let mut list = Vec::new();
                for _ in 0..n.max(0) {
                    let attr = r.varint()?;
                    let uuid = r.uuid()?;
                    let mod_name = r.string()?.to_string();
                    let amount = r.f64()?;
                    let op = r.varint()?;
                    let slot = r.varint()?;
                    let Some(key) = Self::old_attribute_key(attr, true) else { continue };
                    let mut m = Tag::compound();
                    m.insert("UUID", uuid_to_ints(uuid));
                    m.insert("AttributeName", Tag::String(key));
                    m.insert("Name", Tag::String(mod_name));
                    m.insert("Amount", Tag::Double(amount));
                    if slot != 0 {
                        m.insert("Slot", Tag::String(name_at(&SLOT_GROUPS, slot).unwrap_or("any").to_string()));
                    }
                    m.insert("Operation", Tag::Int(op));
                    list.push(m);
                }
                if !r.bool()? {
                    lg.hide |= HIDE_ATTRIBUTES;
                }
                lg.put("AttributeModifiers", Tag::List(list));
            }
            "custom_model_data" => {
                let v = r.varint()?;
                lg.put("CustomModelData", Tag::Int(v));
            }
            "repair_cost" => {
                let v = r.varint()?;
                lg.put("RepairCost", Tag::Int(v));
            }
            "hide_additional_tooltip" => lg.hide |= HIDE_ADDITIONAL,
            // hides everything 1.20.3 can hide
            "hide_tooltip" => lg.hide |= 0xff,
            "enchantment_glint_override" => {
                // 1.20.3 has no glint switch; an invalid enchantment makes an item glow
                if r.bool()? {
                    let has = lg.tag.iter().any(|(k, v)| k == "Enchantments" && v.as_list().is_some_and(|l| !l.is_empty()));
                    if !has {
                        let mut e = Tag::compound();
                        e.insert("id", Tag::String(String::new()));
                        lg.put("Enchantments", Tag::List(vec![e]));
                    }
                }
            }
            "dyed_color" => {
                let rgb = r.i32()?;
                lg.sub("display").insert("color", Tag::Int(rgb));
                if !r.bool()? {
                    lg.hide |= HIDE_DYE;
                }
            }
            "map_color" => {
                let rgb = r.i32()?;
                lg.sub("display").insert("MapColor", Tag::Int(rgb));
            }
            "map_id" => {
                let v = r.varint()?;
                lg.put("map", Tag::Int(v));
            }
            "map_decorations" => {
                let t = nbt::read(r)?;
                let mut list = Vec::new();
                if let Tag::Compound(entries) = t {
                    for (key, deco) in entries {
                        let Some(ty) = deco.get("type").and_then(Tag::as_str).and_then(|n| position(&MAP_DECORATIONS, n)) else { continue };
                        let mut m = Tag::compound();
                        m.insert("id", Tag::String(key));
                        m.insert("type", Tag::Byte(ty as i8));
                        m.insert("x", Tag::Double(f64_of(deco.get("x")).unwrap_or(0.0)));
                        m.insert("z", Tag::Double(f64_of(deco.get("z")).unwrap_or(0.0)));
                        m.insert("rot", Tag::Double(f64_of(deco.get("rotation")).unwrap_or(0.0)));
                        list.push(m);
                    }
                }
                lg.put("Decorations", Tag::List(list));
            }
            "charged_projectiles" => {
                let list = r.list(|r| self.nested_to_tag(r))?;
                if !list.is_empty() {
                    lg.put("Charged", Tag::Byte(1));
                }
                lg.put("ChargedProjectiles", Tag::List(list));
            }
            "bundle_contents" => {
                let list = r.list(|r| self.nested_to_tag(r))?;
                lg.put("Items", Tag::List(list));
            }
            "container" => {
                let n = r.varint()?;
                let mut list = Vec::new();
                for slot in 0..n.max(0) {
                    let mut t = self.nested_to_tag(r)?;
                    if t.get("Count").and_then(Tag::as_i32) == Some(0) {
                        continue;
                    }
                    if let Tag::Compound(e) = &mut t {
                        e.insert(0, ("Slot".into(), Tag::Byte(slot as i8)));
                    }
                    list.push(t);
                }
                lg.block_entity(None).insert("Items", Tag::List(list));
            }
            "potion_contents" => {
                if r.bool()? {
                    let p = r.varint()?;
                    lg.put("Potion", Tag::String(ns(name_at(&POTIONS, p).unwrap_or("water"))));
                }
                if r.bool()? {
                    let c = r.i32()?;
                    lg.put("CustomPotionColor", Tag::Int(c));
                }
                let n = r.varint()?;
                let mut list = Vec::new();
                for _ in 0..n.max(0) {
                    let effect = r.varint()?;
                    let mut e = Tag::compound();
                    e.insert("id", Tag::String(ns(name_at(&EFFECTS, effect).unwrap_or("luck"))));
                    Self::effect_details(r, &mut e)?;
                    list.push(e);
                }
                if !list.is_empty() {
                    lg.put("custom_potion_effects", Tag::List(list));
                }
            }
            "suspicious_stew_effects" => {
                let list = r.list(|r| {
                    let mut e = Tag::compound();
                    e.insert("id", Tag::String(ns(name_at(&EFFECTS, r.varint()?).unwrap_or("luck"))));
                    e.insert("duration", Tag::Int(r.varint()?));
                    Ok(e)
                })?;
                lg.put("effects", Tag::List(list));
            }
            "writable_book_content" => {
                let n = r.varint()?;
                let mut pages = Vec::new();
                let mut filtered = Tag::compound();
                for i in 0..n.max(0) {
                    pages.push(Tag::String(r.string()?.to_string()));
                    if r.bool()? {
                        filtered.insert(&i.to_string(), Tag::String(r.string()?.to_string()));
                    }
                }
                lg.put("pages", Tag::List(pages));
                lg.put("filtered_pages", filtered);
            }
            "written_book_content" => {
                let title = r.string()?.to_string();
                let filtered_title = r.option(|r| r.string().map(str::to_string))?;
                let author = r.string()?.to_string();
                let generation = r.varint()?;
                let n = r.varint()?;
                let mut pages = Vec::new();
                let mut filtered = Tag::compound();
                for i in 0..n.max(0) {
                    pages.push(Tag::String(text::to_json(&nbt::read(r)?)));
                    if r.bool()? {
                        filtered.insert(&i.to_string(), Tag::String(text::to_json(&nbt::read(r)?)));
                    }
                }
                let resolved = r.bool()?;
                lg.put("pages", Tag::List(pages));
                lg.put("filtered_pages", filtered);
                lg.put("author", Tag::String(author));
                lg.put("generation", Tag::Int(generation));
                lg.put("resolved", Tag::Byte(i8::from(resolved)));
                lg.put("title", Tag::String(title));
                if let Some(f) = filtered_title {
                    lg.put("filtered_title", Tag::String(f));
                }
            }
            "trim" => {
                let material = Self::holder(r, Self::skip_trim_material)?;
                let pattern = Self::holder(r, Self::skip_trim_pattern)?;
                if !r.bool()? {
                    lg.hide |= HIDE_TRIM;
                }
                let key = |keys: &[String], id: Option<i32>| id.and_then(|i| keys.get(usize::try_from(i).ok()?)).cloned();
                let material = key(&self.trim_materials, material);
                // 1.20.3 lacks the flow and bolt patterns
                let pattern = key(&self.trim_patterns, pattern).filter(|p| !matches!(strip_ns(p), "flow" | "bolt"));
                if let (Some(m), Some(p)) = (material, pattern) {
                    let mut t = Tag::compound();
                    t.insert("material", Tag::String(m));
                    t.insert("pattern", Tag::String(p));
                    lg.put("Trim", t);
                }
            }
            "debug_stick_state" => lg.put("DebugProperty", nbt::read(r)?),
            "entity_data" => lg.put("EntityTag", nbt::read(r)?),
            "bucket_entity_data" => {
                if let Tag::Compound(entries) = nbt::read(r)? {
                    for (k, v) in entries {
                        if MOB_TAGS.contains(&k.as_str()) {
                            lg.put(&k, v);
                        }
                    }
                }
            }
            "block_entity_data" => {
                if let Tag::Compound(entries) = nbt::read(r)? {
                    let be = lg.sub("BlockEntityTag");
                    for (k, v) in entries {
                        be.insert(&k, v);
                    }
                }
            }
            "instrument" => {
                if let Some(id) = Self::holder(r, |r| {
                    Self::skip_sound_event(r)?;
                    r.varint()?;
                    r.f32().map(|_| ())
                })? {
                    if let Some(n) = name_at(&INSTRUMENTS, id) {
                        lg.put("instrument", Tag::String(ns(n)));
                    }
                }
            }
            "recipes" => lg.put("Recipes", nbt::read(r)?),
            "lodestone_tracker" => {
                let target = r.option(|r| Ok((r.string()?.to_string(), r.i64()?)))?;
                let tracked = r.bool()?;
                lg.put("LodestoneTracked", Tag::Byte(i8::from(tracked)));
                if let Some((dimension, pos)) = target {
                    let (x, y, z) = unpack_pos(pos);
                    let mut p = Tag::compound();
                    p.insert("X", Tag::Int(x));
                    p.insert("Y", Tag::Int(y));
                    p.insert("Z", Tag::Int(z));
                    lg.put("LodestonePos", p);
                    lg.put("LodestoneDimension", Tag::String(dimension));
                }
            }
            "firework_explosion" => {
                let e = Self::explosion(r)?;
                lg.put("Explosion", e);
            }
            "fireworks" => {
                let flight = r.varint()?;
                let explosions = r.list(Self::explosion)?;
                let f = lg.sub("Fireworks");
                f.insert("Flight", Tag::Byte(flight as i8));
                if !explosions.is_empty() {
                    f.insert("Explosions", Tag::List(explosions));
                }
            }
            "profile" => {
                let name = r.option(|r| r.string().map(str::to_string))?;
                let id = r.option(|r| r.uuid())?;
                let props = r.list(|r| Ok((r.string()?.to_string(), r.string()?.to_string(), r.option(|r| r.string().map(str::to_string))?)))?;
                if let (Some(n), None, true) = (&name, id, props.is_empty()) {
                    lg.put("SkullOwner", Tag::String(n.clone()));
                } else {
                    let mut owner = Tag::compound();
                    if let Some(n) = name {
                        owner.insert("Name", Tag::String(n));
                    }
                    if let Some(u) = id {
                        owner.insert("Id", uuid_to_ints(u));
                    }
                    let mut properties = Tag::compound();
                    for (pname, value, signature) in props {
                        let mut p = Tag::compound();
                        p.insert("Value", Tag::String(value));
                        if let Some(s) = signature {
                            p.insert("Signature", Tag::String(s));
                        }
                        match properties.get_mut(&pname) {
                            Some(Tag::List(l)) => l.push(p),
                            _ => properties.insert(&pname, Tag::List(vec![p])),
                        }
                    }
                    owner.insert("Properties", properties);
                    lg.put("SkullOwner", owner);
                }
            }
            "note_block_sound" => {
                let s = r.string()?.to_string();
                lg.block_entity(Some("player_head")).insert("note_block_sound", Tag::String(s));
            }
            "banner_patterns" => {
                let n = r.varint()?;
                let mut list = Vec::new();
                for _ in 0..n.max(0) {
                    let pattern = Self::holder(r, |r| {
                        r.string()?;
                        r.string().map(|_| ())
                    })?;
                    let color = r.varint()?;
                    let code = pattern.and_then(|p| self.old_banner_pattern(p)).map(|i| BANNER_PATTERNS[i as usize].1);
                    if let Some(code) = code {
                        let mut p = Tag::compound();
                        p.insert("Pattern", Tag::String(code.to_string()));
                        p.insert("Color", Tag::Int(color));
                        list.push(p);
                    }
                }
                lg.block_entity(Some("banner")).insert("Patterns", Tag::List(list));
            }
            "base_color" => {
                let c = r.varint()?;
                lg.block_entity(None).insert("Base", Tag::Int(c));
            }
            "pot_decorations" => {
                let sherds = r.list(|r| Ok(Tag::String(Self::old_item_key(r.varint()?))))?;
                lg.block_entity(Some("decorated_pot")).insert("sherds", Tag::List(sherds));
            }
            "block_state" => {
                let props = r.list(|r| Ok((r.string()?.to_string(), r.string()?.to_string())))?;
                let mut t = Tag::compound();
                for (k, v) in props {
                    t.insert(&k, Tag::String(v));
                }
                lg.put("BlockStateTag", t);
            }
            "bees" => {
                let bees = r.list(|r| {
                    let mut b = Tag::compound();
                    b.insert("EntityData", nbt::read(r)?);
                    b.insert("TicksInHive", Tag::Int(r.varint()?));
                    b.insert("MinOccupationTicks", Tag::Int(r.varint()?));
                    Ok(b)
                })?;
                lg.block_entity(Some("beehive")).insert("Bees", Tag::List(bees));
            }
            "lock" => {
                let t = nbt::read(r)?;
                lg.block_entity(None).insert("Lock", t);
            }
            "container_loot" => {
                let t = nbt::read(r)?;
                let be = lg.block_entity(None);
                if let Some(v) = t.get("loot_table") {
                    be.insert("LootTable", v.clone());
                }
                if let Some(v) = t.get("seed") {
                    be.insert("LootTableSeed", v.clone());
                }
            }
            // no 1.20.3 equivalent: only the backup keeps these
            "max_stack_size" | "max_damage" | "rarity" | "map_post_processing" | "ominous_bottle_amplifier" => {
                r.varint()?;
            }
            "creative_slot_lock" | "fire_resistant" => {}
            "intangible_projectile" => nbt::skip(r)?,
            "food" => {
                r.varint()?;
                r.f32()?;
                r.bool()?;
                r.f32()?;
                let n = r.varint()?;
                for _ in 0..n.max(0) {
                    r.varint()?;
                    Self::effect_details(r, &mut Tag::compound())?;
                    r.f32()?;
                }
            }
            "tool" => {
                let n = r.varint()?;
                for _ in 0..n.max(0) {
                    Self::skip_holder_set(r)?;
                    r.option(|r| r.f32())?;
                    r.option(|r| r.bool())?;
                }
                r.f32()?;
                r.varint()?;
            }
            other => return Err(err(&format!("no codec for data component {other}"))),
        }
        Ok(())
    }

    /// Potion effect details into a 1.20.3 effect compound.
    fn effect_details(r: &mut Reader, e: &mut Tag) -> Result<()> {
        e.insert("amplifier", Tag::Byte(r.varint()? as i8));
        e.insert("duration", Tag::Int(r.varint()?));
        e.insert("ambient", Tag::Byte(i8::from(r.bool()?)));
        e.insert("show_particles", Tag::Byte(i8::from(r.bool()?)));
        e.insert("show_icon", Tag::Byte(i8::from(r.bool()?)));
        if r.bool()? {
            let mut hidden = Tag::compound();
            Self::effect_details(r, &mut hidden)?;
            e.insert("hidden_effect", hidden);
        }
        Ok(())
    }

    /// Registry holder: `Some(id)`, or `None` for an inline value (skipped with `direct`).
    fn holder(r: &mut Reader, direct: impl FnOnce(&mut Reader) -> Result<()>) -> Result<Option<i32>> {
        let v = r.varint()?;
        if v == 0 {
            direct(r)?;
            return Ok(None);
        }
        Ok(Some(v - 1))
    }

    fn skip_sound_event(r: &mut Reader) -> Result<()> {
        Self::holder(r, |r| {
            r.string()?;
            r.option(|r| r.f32()).map(|_| ())
        })
        .map(|_| ())
    }

    fn skip_holder_set(r: &mut Reader) -> Result<()> {
        let v = r.varint()?;
        if v == 0 {
            r.string()?;
        } else {
            for _ in 0..(v - 1).max(0) {
                r.varint()?;
            }
        }
        Ok(())
    }

    fn skip_trim_material(r: &mut Reader) -> Result<()> {
        r.string()?;
        r.varint()?;
        r.f32()?;
        let n = r.varint()?;
        for _ in 0..n.max(0) {
            r.varint()?;
            r.string()?;
        }
        nbt::skip(r)
    }

    fn skip_trim_pattern(r: &mut Reader) -> Result<()> {
        r.string()?;
        r.varint()?;
        nbt::skip(r)?;
        r.bool().map(|_| ())
    }

    fn explosion(r: &mut Reader) -> Result<Tag> {
        let mut e = Tag::compound();
        e.insert("Type", Tag::Byte(r.varint()? as i8));
        e.insert("Colors", Tag::IntArray(r.list(|r| r.i32())?));
        e.insert("FadeColors", Tag::IntArray(r.list(|r| r.i32())?));
        e.insert("Trail", Tag::Byte(i8::from(r.bool()?)));
        e.insert("Flicker", Tag::Byte(i8::from(r.bool()?)));
        Ok(e)
    }

    /// A block predicate as 1.20.3 strings ("minecraft:stone[axis=y]", "#minecraft:logs"); one per
    /// block. Predicates 1.20.3 cannot express (no blocks, value ranges, NBT) are left out.
    fn block_predicate(&self, r: &mut Reader, out: &mut Vec<Tag>) -> Result<()> {
        let blocks = r.option(|r| {
            let v = r.varint()?;
            if v == 0 {
                return Ok(vec![format!("#{}", r.string()?)]);
            }
            let ids = (0..(v - 1).max(0)).map(|_| r.varint()).collect::<Result<Vec<_>>>()?;
            Ok(ids.into_iter().filter(|id| !Self::block_unknown(*id)).filter_map(|id| name_at(&d::NEW_BLOCK_NAMES, id).map(ns)).collect())
        })?;
        let mut props = Some(Vec::new());
        if r.bool()? {
            let n = r.varint()?;
            for _ in 0..n.max(0) {
                let key = r.string()?.to_string();
                if r.bool()? {
                    let value = r.string()?.to_string();
                    if let Some(p) = props.as_mut() {
                        p.push(format!("{key}={value}"));
                    }
                } else {
                    r.option(|r| r.string().map(|_| ()))?;
                    r.option(|r| r.string().map(|_| ()))?;
                    props = None;
                }
            }
        }
        let has_nbt = r.bool()?;
        if has_nbt {
            nbt::skip(r)?;
        }
        let (Some(blocks), Some(props), false) = (blocks, props, has_nbt) else { return Ok(()) };
        for b in blocks {
            let s = if props.is_empty() { b } else { format!("{b}[{}]", props.join(",")) };
            out.push(Tag::String(s));
        }
        Ok(())
    }

    // ---- 1.20.3 → 1.20.5 -------------------------------------------------------------------

    /// One 1.20.3 item stack (present flag first) as a 1.20.5 one (count first).
    pub fn to_server(&self, r: &mut Reader, w: &mut Writer) -> Result<()> {
        if !r.bool()? {
            w.varint(0);
            return Ok(());
        }
        let id = r.varint()?;
        let count = i32::from(r.i8()?);
        let tag = nbt::read_optional(r)?;
        self.write_server_stack(id, count, tag, w);
        Ok(())
    }

    fn write_server_stack(&self, old_id: i32, count: i32, tag: Option<Tag>, w: &mut Writer) {
        if count <= 0 || old_id <= 0 {
            w.varint(0);
            return;
        }
        w.varint(count);
        if let Some((id, patch)) = tag.as_ref().and_then(|t| Self::backup(old_id, t)) {
            w.varint(id);
            w.raw(&patch);
            return;
        }
        let (id, comps) = self.to_components(old_id, tag);
        w.varint(id);
        w.varint(comps.len() as i32);
        w.varint(0);
        for (ty, raw) in comps {
            w.varint(ty);
            w.raw(&raw);
        }
    }

    /// The server's item kept in the NBT, if it belongs to this (1.20.3) item.
    fn backup(old_id: i32, tag: &Tag) -> Option<(i32, Vec<u8>)> {
        let b = tag.get(BACKUP_KEY)?;
        let id = b.get("id")?.as_i32()?;
        let Some(Tag::ByteArray(patch)) = b.get("c") else { return None };
        (Self::item(id) == old_id).then(|| (id, patch.iter().map(|x| *x as u8).collect()))
    }

    /// A nested 1.20.3 item (`{id, Count, tag}`) as 1.20.5 stack bytes.
    fn nested_from_tag(&self, t: &Tag, w: &mut Writer) {
        let id = t.get("id").and_then(Tag::as_str).and_then(|k| position(&d::OLD_ITEM_NAMES, k)).unwrap_or(0);
        let count = t.get("Count").and_then(Tag::as_i32).unwrap_or(0);
        self.write_server_stack(id, count, t.get("tag").cloned(), w);
    }

    /// 1.20.3 item (id and NBT) → 1.20.5 id and components, sorted by component id. What has no
    /// component becomes custom data, as the vanilla upgrade does.
    fn to_components(&self, old_id: i32, tag: Option<Tag>) -> (i32, Vec<(i32, Vec<u8>)>) {
        let id = Self::item_to_server(old_id);
        let item = Self::new_item_name(id);
        let mut out: Vec<(i32, Vec<u8>)> = Vec::new();
        let mut tag = match tag {
            Some(t @ Tag::Compound(_)) => t,
            _ => return (id, out),
        };
        macro_rules! push {
            ($name:expr, $w:expr) => {
                out.push((component_id($name), $w.into_inner()))
            };
        }
        let hide = take_i32(&mut tag, "HideFlags").unwrap_or(0);
        let shown = |bit: i32| hide & bit == 0;

        if let Some(damage) = take_i32(&mut tag, "Damage") {
            // 1.20.3 gives every damageable item an explicit Damage:0
            if damage != 0 {
                let mut w = Writer::new();
                w.varint(damage);
                push!("damage", w);
            }
        }
        if take_i32(&mut tag, "Unbreakable").is_some_and(|v| v != 0) {
            let mut w = Writer::new();
            w.bool(shown(HIDE_UNBREAKABLE));
            push!("unbreakable", w);
        }
        if let Some(mut display) = take_compound(&mut tag, "display") {
            if let Some(name) = take_string(&mut display, "Name") {
                let t = text::from_json(&name);
                let mut w = Writer::new();
                match Self::item_name_of(&t) {
                    Some(inner) => {
                        nbt::write(&mut w, inner);
                        push!("item_name", w);
                    }
                    None => {
                        nbt::write(&mut w, &t);
                        push!("custom_name", w);
                    }
                }
            }
            if let Some(lore) = take_list(&mut display, "Lore") {
                let mut w = Writer::new();
                w.varint(lore.len() as i32);
                for line in &lore {
                    nbt::write(&mut w, &text::from_json(line.as_str().unwrap_or("")));
                }
                push!("lore", w);
            }
            if let Some(rgb) = take_i32(&mut display, "color") {
                let mut w = Writer::new();
                w.i32(rgb).bool(shown(HIDE_DYE));
                push!("dyed_color", w);
            }
            if let Some(rgb) = take_i32(&mut display, "MapColor") {
                let mut w = Writer::new();
                w.i32(rgb);
                push!("map_color", w);
            }
            if !is_empty_compound(&display) {
                tag.insert("display", display);
            }
        }
        let mut glint = false;
        for (key, comp, bit) in [("Enchantments", "enchantments", HIDE_ENCHANTMENTS), ("StoredEnchantments", "stored_enchantments", HIDE_ADDITIONAL)] {
            let Some(list) = take_list(&mut tag, key) else { continue };
            let mut entries = Vec::new();
            let mut invalid = false;
            for e in &list {
                let level = e.get("lvl").and_then(Tag::as_i32);
                match (e.get("id").and_then(Tag::as_str).and_then(Self::enchantment_from_old_key), level) {
                    (Some(id), Some(level)) => entries.push((id, level)),
                    _ => invalid = true,
                }
            }
            // only an invalid enchantment: the glint stand-in written for 1.20.3
            if comp == "enchantments" && entries.is_empty() && invalid {
                glint = true;
                continue;
            }
            let mut w = Writer::new();
            w.varint(entries.len() as i32);
            for (id, level) in entries {
                w.varint(id).varint(level);
            }
            w.bool(shown(bit));
            push!(comp, w);
        }
        if glint {
            let mut w = Writer::new();
            w.bool(true);
            push!("enchantment_glint_override", w);
        }
        if hide & HIDE_ADDITIONAL != 0 && !out.iter().any(|(t, _)| *t == component_id("stored_enchantments")) {
            push!("hide_additional_tooltip", Writer::new());
        }
        for (key, comp, bit) in [("CanPlaceOn", "can_place_on", HIDE_CAN_PLACE_ON), ("CanDestroy", "can_break", HIDE_CAN_DESTROY)] {
            let Some(list) = take_list(&mut tag, key) else { continue };
            let preds: Vec<Writer> = list.iter().filter_map(|s| s.as_str()).filter_map(Self::block_predicate_from_string).collect();
            let mut w = Writer::new();
            w.varint(preds.len() as i32);
            for p in preds {
                w.raw(&p.buf);
            }
            w.bool(shown(bit));
            push!(comp, w);
        }
        if let Some(list) = take_list(&mut tag, "AttributeModifiers") {
            let mut body = Writer::new();
            let mut n = 0;
            for m in &list {
                let Some(attr) = m.get("AttributeName").and_then(Tag::as_str).and_then(Self::attribute_from_old_key) else { continue };
                let uuid = m.get("UUID").and_then(uuid_from_ints).unwrap_or(0);
                body.varint(attr);
                body.uuid(uuid);
                body.string(m.get("Name").and_then(Tag::as_str).unwrap_or(""));
                body.f64(f64_of(m.get("Amount")).unwrap_or(0.0));
                body.varint(m.get("Operation").and_then(Tag::as_i32).unwrap_or(0));
                body.varint(m.get("Slot").and_then(Tag::as_str).and_then(|s| position(&SLOT_GROUPS, s)).unwrap_or(0));
                n += 1;
            }
            let mut w = Writer::new();
            w.varint(n);
            w.raw(&body.buf);
            w.bool(shown(HIDE_ATTRIBUTES));
            push!("attribute_modifiers", w);
        }
        for (key, comp) in [("CustomModelData", "custom_model_data"), ("RepairCost", "repair_cost"), ("map", "map_id")] {
            if let Some(v) = take_i32(&mut tag, key) {
                let mut w = Writer::new();
                w.varint(v);
                push!(comp, w);
            }
        }
        if item == "crossbow" {
            if let Some(list) = take_list(&mut tag, "ChargedProjectiles") {
                take_i32(&mut tag, "Charged");
                let mut w = Writer::new();
                w.varint(list.len() as i32);
                for t in &list {
                    self.nested_from_tag(t, &mut w);
                }
                push!("charged_projectiles", w);
            }
        }
        if item == "bundle" {
            if let Some(list) = take_list(&mut tag, "Items") {
                let mut w = Writer::new();
                w.varint(list.len() as i32);
                for t in &list {
                    self.nested_from_tag(t, &mut w);
                }
                push!("bundle_contents", w);
            }
        }
        let potion = take_string(&mut tag, "Potion");
        let color = take_i32(&mut tag, "CustomPotionColor");
        let effects = take_list(&mut tag, "custom_potion_effects");
        if potion.is_some() || color.is_some() || effects.is_some() {
            let mut w = Writer::new();
            w.option(potion.as_deref().and_then(|p| position(&POTIONS, p)), |w, p| {
                w.varint(p);
            });
            w.option(color, |w, c| {
                w.i32(c);
            });
            let effects: Vec<&Tag> = effects.iter().flatten().filter(|e| e.get("id").and_then(Tag::as_str).and_then(|n| position(&EFFECTS, n)).is_some()).collect();
            w.varint(effects.len() as i32);
            for e in effects {
                w.varint(e.get("id").and_then(Tag::as_str).and_then(|n| position(&EFFECTS, n)).unwrap_or(0));
                Self::write_effect_details(e, &mut w);
            }
            push!("potion_contents", w);
        }
        if item == "suspicious_stew" {
            if let Some(list) = take_list(&mut tag, "effects") {
                let effects: Vec<(i32, i32)> = list
                    .iter()
                    .filter_map(|e| Some((e.get("id").and_then(Tag::as_str).and_then(|n| position(&EFFECTS, n))?, e.get("duration").and_then(Tag::as_i32).unwrap_or(160))))
                    .collect();
                let mut w = Writer::new();
                w.varint(effects.len() as i32);
                for (e, dur) in effects {
                    w.varint(e).varint(dur);
                }
                push!("suspicious_stew_effects", w);
            }
        }
        if item == "writable_book" || item == "written_book" {
            if let Some(pages) = take_list(&mut tag, "pages") {
                let filtered = take_compound(&mut tag, "filtered_pages").unwrap_or_else(Tag::compound);
                let mut w = Writer::new();
                if item == "written_book" {
                    let title = take_string(&mut tag, "title").unwrap_or_default();
                    w.string(&title);
                    w.option(take_string(&mut tag, "filtered_title"), |w, s| {
                        w.string(&s);
                    });
                    w.string(&take_string(&mut tag, "author").unwrap_or_default());
                    w.varint(take_i32(&mut tag, "generation").unwrap_or(0));
                }
                w.varint(pages.len() as i32);
                for (i, page) in pages.iter().enumerate() {
                    let raw = page.as_str().unwrap_or("");
                    let filtered_page = filtered.get(&i.to_string()).and_then(Tag::as_str);
                    if item == "written_book" {
                        nbt::write(&mut w, &text::from_json(raw));
                        w.option(filtered_page, |w, f| nbt::write(w, &text::from_json(f)));
                    } else {
                        w.string(raw);
                        w.option(filtered_page, |w, f| {
                            w.string(f);
                        });
                    }
                }
                if item == "written_book" {
                    w.bool(take_i32(&mut tag, "resolved").unwrap_or(0) != 0);
                    push!("written_book_content", w);
                } else {
                    push!("writable_book_content", w);
                }
            }
        }
        if let Some(trim) = take_compound(&mut tag, "Trim") {
            let index_of = |keys: &[String], key: Option<&str>| key.and_then(|k| keys.iter().position(|x| strip_ns(x) == strip_ns(k)));
            let material = index_of(&self.trim_materials, trim.get("material").and_then(Tag::as_str));
            let pattern = index_of(&self.trim_patterns, trim.get("pattern").and_then(Tag::as_str));
            if let (Some(m), Some(p)) = (material, pattern) {
                let mut w = Writer::new();
                w.varint(m as i32 + 1).varint(p as i32 + 1).bool(shown(HIDE_TRIM));
                push!("trim", w);
            } else {
                tag.insert("Trim", trim);
            }
        }
        for (key, comp) in [("DebugProperty", "debug_stick_state"), ("EntityTag", "entity_data"), ("Recipes", "recipes")] {
            if let Some(v) = tag.remove(key) {
                let mut w = Writer::new();
                nbt::write(&mut w, &v);
                push!(comp, w);
            }
        }
        if item.ends_with("_bucket") && MOB_TAGS.iter().any(|k| tag.get(k).is_some()) && !matches!(item, "water_bucket" | "lava_bucket" | "milk_bucket" | "powder_snow_bucket") {
            let mut data = Tag::compound();
            for k in MOB_TAGS {
                if let Some(v) = tag.remove(k) {
                    data.insert(k, v);
                }
            }
            let mut w = Writer::new();
            nbt::write(&mut w, &data);
            push!("bucket_entity_data", w);
        }
        if let Some(instrument) = take_string(&mut tag, "instrument") {
            match position(&INSTRUMENTS, &instrument) {
                Some(i) => {
                    let mut w = Writer::new();
                    w.varint(i + 1);
                    push!("instrument", w);
                }
                None => tag.insert("instrument", Tag::String(instrument)),
            }
        }
        if tag.get("LodestoneTracked").is_some() || tag.get("LodestonePos").is_some() {
            let tracked = take_i32(&mut tag, "LodestoneTracked").unwrap_or(0) != 0;
            let pos = take_compound(&mut tag, "LodestonePos");
            let dim = take_string(&mut tag, "LodestoneDimension");
            let mut w = Writer::new();
            match (pos, dim) {
                (Some(p), Some(dim)) => {
                    let c = |k: &str| p.get(k).and_then(Tag::as_i32).unwrap_or(0);
                    w.bool(true).string(&dim).i64(pack_pos(c("X"), c("Y"), c("Z")));
                }
                _ => {
                    w.bool(false);
                }
            }
            w.bool(tracked);
            push!("lodestone_tracker", w);
        }
        if let Some(e) = take_compound(&mut tag, "Explosion") {
            let mut w = Writer::new();
            Self::write_explosion(&e, &mut w);
            push!("firework_explosion", w);
        }
        if let Some(f) = take_compound(&mut tag, "Fireworks") {
            let mut w = Writer::new();
            w.varint(f.get("Flight").and_then(Tag::as_i32).unwrap_or(0));
            let explosions = f.get("Explosions").and_then(Tag::as_list).cloned().unwrap_or_default();
            w.varint(explosions.len() as i32);
            for e in &explosions {
                Self::write_explosion(e, &mut w);
            }
            push!("fireworks", w);
        }
        if let Some(owner) = tag.remove("SkullOwner") {
            let mut w = Writer::new();
            match &owner {
                Tag::String(name) => {
                    w.bool(true).string(name).bool(false).varint(0);
                }
                _ => {
                    w.option(owner.get("Name").and_then(Tag::as_str), |w, n| {
                        w.string(n);
                    });
                    w.option(owner.get("Id").and_then(uuid_from_ints), |w, u| {
                        w.uuid(u);
                    });
                    let mut props = Vec::new();
                    if let Some(Tag::Compound(p)) = owner.get("Properties") {
                        for (pname, values) in p {
                            for v in values.as_list().into_iter().flatten() {
                                props.push((pname.as_str(), v.get("Value").and_then(Tag::as_str).unwrap_or(""), v.get("Signature").and_then(Tag::as_str)));
                            }
                        }
                    }
                    w.varint(props.len() as i32);
                    for (n, v, s) in props {
                        w.string(n).string(v);
                        w.option(s, |w, s| {
                            w.string(s);
                        });
                    }
                }
            }
            push!("profile", w);
        }
        if let Some(state) = take_compound(&mut tag, "BlockStateTag") {
            let Tag::Compound(entries) = &state else { unreachable!() };
            let mut w = Writer::new();
            w.varint(entries.len() as i32);
            for (k, v) in entries {
                w.string(k);
                w.string(&match v {
                    Tag::String(s) => s.clone(),
                    other => other.as_i32().map(|i| i.to_string()).unwrap_or_default(),
                });
            }
            push!("block_state", w);
        }
        if let Some(mut be) = take_compound(&mut tag, "BlockEntityTag") {
            if let Some(patterns) = take_list(&mut be, "Patterns") {
                let layers: Vec<(i32, i32)> = patterns
                    .iter()
                    .filter_map(|p| {
                        let code = p.get("Pattern")?.as_str()?;
                        let key = BANNER_PATTERNS.iter().find(|(_, c)| *c == code)?.0;
                        let id = self.banner_patterns.iter().position(|k| strip_ns(k) == key)?;
                        Some((id as i32, p.get("Color").and_then(Tag::as_i32).unwrap_or(0)))
                    })
                    .collect();
                let mut w = Writer::new();
                w.varint(layers.len() as i32);
                for (p, c) in layers {
                    w.varint(p + 1).varint(c);
                }
                push!("banner_patterns", w);
            }
            if let Some(base) = take_i32(&mut be, "Base") {
                let mut w = Writer::new();
                w.varint(base);
                push!("base_color", w);
            }
            if let Some(items) = take_list(&mut be, "Items") {
                let mut slots: Vec<(usize, &Tag)> = items.iter().filter_map(|t| Some((usize::try_from(t.get("Slot")?.as_i32()?).ok()?, t))).filter(|(s, _)| *s < 256).collect();
                slots.sort_by_key(|(s, _)| *s);
                let len = slots.last().map_or(0, |(s, _)| s + 1);
                let mut w = Writer::new();
                w.varint(len as i32);
                for i in 0..len {
                    match slots.iter().find(|(s, _)| *s == i) {
                        Some((_, t)) => self.nested_from_tag(t, &mut w),
                        None => {
                            w.varint(0);
                        }
                    }
                }
                push!("container", w);
            }
            if let Some(sherds) = take_list(&mut be, "sherds") {
                let mut w = Writer::new();
                w.varint(sherds.len() as i32);
                for s in &sherds {
                    w.varint(s.as_str().and_then(Self::item_from_old_key).unwrap_or(Self::item_from_old_key("brick").unwrap_or(0)));
                }
                push!("pot_decorations", w);
            }
            if let Some(bees) = take_list(&mut be, "Bees") {
                let mut w = Writer::new();
                w.varint(bees.len() as i32);
                for b in &bees {
                    nbt::write(&mut w, b.get("EntityData").unwrap_or(&Tag::compound()));
                    w.varint(b.get("TicksInHive").and_then(Tag::as_i32).unwrap_or(0));
                    w.varint(b.get("MinOccupationTicks").and_then(Tag::as_i32).unwrap_or(0));
                }
                push!("bees", w);
            }
            if let Some(lock) = be.remove("Lock") {
                let mut w = Writer::new();
                nbt::write(&mut w, &lock);
                push!("lock", w);
            }
            let loot = be.remove("LootTable");
            let seed = be.remove("LootTableSeed");
            if loot.is_some() || seed.is_some() {
                let mut t = Tag::compound();
                if let Some(l) = loot {
                    t.insert("loot_table", l);
                }
                if let Some(s) = seed {
                    t.insert("seed", s);
                }
                let mut w = Writer::new();
                nbt::write(&mut w, &t);
                push!("container_loot", w);
            }
            if let Some(sound) = take_string(&mut be, "note_block_sound") {
                let mut w = Writer::new();
                w.string(&sound);
                push!("note_block_sound", w);
            }
            // the id alone was only there for the components above
            if matches!(&be, Tag::Compound(e) if e.len() == 1 && e[0].0 == "id") {
                be = Tag::compound();
            }
            if !is_empty_compound(&be) {
                let mut w = Writer::new();
                nbt::write(&mut w, &be);
                push!("block_entity_data", w);
            }
        }
        tag.remove(BACKUP_KEY);
        if !is_empty_compound(&tag) {
            let mut w = Writer::new();
            nbt::write(&mut w, &tag);
            push!("custom_data", w);
        }
        out.sort_by_key(|(t, _)| *t);
        (id, out)
    }

    /// The item name inside the wrapper `finish` puts around it, if `name` is one.
    fn item_name_of(name: &Tag) -> Option<&Tag> {
        let Tag::Compound(e) = name else { return None };
        match e.as_slice() {
            [(a, Tag::Byte(0)), (b, Tag::String(s)), (c, Tag::List(extra))] if a == "italic" && b == "text" && s.is_empty() && c == "extra" && extra.len() == 1 => {
                Some(&extra[0])
            }
            _ => None,
        }
    }

    fn write_effect_details(e: &Tag, w: &mut Writer) {
        let flag = |k: &str, default: bool| e.get(k).and_then(Tag::as_i32).map_or(default, |v| v != 0);
        w.varint(e.get("amplifier").and_then(Tag::as_i32).unwrap_or(0));
        w.varint(e.get("duration").and_then(Tag::as_i32).unwrap_or(0));
        w.bool(flag("ambient", false));
        w.bool(flag("show_particles", true));
        w.bool(flag("show_icon", true));
        match e.get("hidden_effect") {
            Some(h) => {
                w.bool(true);
                Self::write_effect_details(h, w);
            }
            None => {
                w.bool(false);
            }
        }
    }

    fn write_explosion(e: &Tag, w: &mut Writer) {
        w.varint(e.get("Type").and_then(Tag::as_i32).unwrap_or(0));
        for key in ["Colors", "FadeColors"] {
            let colors = match e.get(key) {
                Some(Tag::IntArray(v)) => v.clone(),
                _ => Vec::new(),
            };
            w.varint(colors.len() as i32);
            for c in colors {
                w.i32(c);
            }
        }
        w.bool(e.get("Trail").and_then(Tag::as_i32).unwrap_or(0) != 0);
        w.bool(e.get("Flicker").and_then(Tag::as_i32).unwrap_or(0) != 0);
    }

    /// "minecraft:stone[axis=y]" or "#minecraft:logs" as a 1.20.5 block predicate (NBT parts are
    /// not read; such a string is left out).
    fn block_predicate_from_string(s: &str) -> Option<Writer> {
        if s.contains('{') {
            return None;
        }
        let (block, props) = match s.split_once('[') {
            Some((b, p)) => (b, p.strip_suffix(']')?),
            None => (s, ""),
        };
        let mut w = Writer::new();
        w.bool(true);
        if let Some(tag) = block.strip_prefix('#') {
            w.varint(0).string(&ns(tag));
        } else {
            let id = position(&d::NEW_BLOCK_NAMES, block)?;
            w.varint(2).varint(id);
        }
        let props: Vec<(&str, &str)> = props.split(',').filter(|p| !p.is_empty()).filter_map(|p| p.split_once('=')).collect();
        if props.is_empty() {
            w.bool(false);
        } else {
            w.bool(true).varint(props.len() as i32);
            for (k, v) in props {
                w.string(k).bool(true).string(v);
            }
        }
        w.bool(false); // nbt
        Some(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp(name: &str) -> i32 {
        component_id(name)
    }

    fn item(name: &str) -> i32 {
        position(&d::NEW_ITEM_NAMES, name).unwrap()
    }

    fn stack(id: i32, comps: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut w = Writer::new();
        w.varint(1).varint(id).varint(comps.len() as i32).varint(0);
        for (n, v) in comps {
            w.varint(comp(n)).raw(v);
        }
        w.into_inner()
    }

    /// 1.20.5 stack → 1.20.3 (old id, NBT).
    fn to_client(items: &Items, bytes: &[u8]) -> (i32, Option<Tag>) {
        let mut out = Writer::new();
        let mut r = Reader::new(bytes);
        items.to_client(&mut r, &mut out).unwrap();
        assert!(r.is_empty(), "trailing bytes");
        let mut r = Reader::new(&out.buf);
        assert!(r.bool().unwrap());
        let id = r.varint().unwrap();
        r.i8().unwrap();
        let tag = nbt::read_optional(&mut r).unwrap();
        assert!(r.is_empty());
        (id, tag)
    }

    fn to_server(items: &Items, old_id: i32, tag: Option<&Tag>) -> Vec<u8> {
        let mut w = Writer::new();
        w.bool(true).varint(old_id).i8(1);
        nbt::write_optional(&mut w, tag);
        let mut r = Reader::new(&w.buf);
        let mut out = Writer::new();
        items.to_server(&mut r, &mut out).unwrap();
        assert!(r.is_empty());
        out.into_inner()
    }

    fn nbt_bytes(t: &Tag) -> Vec<u8> {
        let mut w = Writer::new();
        nbt::write(&mut w, t);
        w.into_inner()
    }

    fn text(s: &str) -> Tag {
        let mut t = Tag::compound();
        t.insert("text", Tag::String(s.into()));
        t.insert("color", Tag::String("aqua".into()));
        t
    }

    #[test]
    fn named_enchanted_sword_has_no_backup_and_round_trips() {
        let mut name = Writer::new();
        nbt::write(&mut name, &text("Excalibur"));
        let mut lore = Writer::new();
        lore.varint(2);
        nbt::write(&mut lore, &text("line one"));
        nbt::write(&mut lore, &Tag::String("line two".into()));
        let sharp = position(&d::NEW_ENCHANTMENT_NAMES, "sharpness").unwrap();
        let sweep = position(&d::NEW_ENCHANTMENT_NAMES, "sweeping_edge").unwrap();
        let mut ench = Writer::new();
        ench.varint(2).varint(sharp).varint(5).varint(sweep).varint(3).bool(true);
        let mut damage = Writer::new();
        damage.varint(12);
        let sword = item("diamond_sword");
        let input = stack(sword, &[("damage", damage.buf), ("custom_name", name.buf), ("lore", lore.buf), ("enchantments", ench.buf)]);
        let items = Items::new();
        let (old, tag) = to_client(&items, &input);
        assert_eq!(old, position(&d::OLD_ITEM_NAMES, "diamond_sword").unwrap());
        let tag = tag.unwrap();
        assert!(tag.get(BACKUP_KEY).is_none(), "lossless items carry no backup");
        assert_eq!(tag.get("Damage"), Some(&Tag::Int(12)));
        let display = tag.get("display").unwrap();
        assert_eq!(display.get("Name").and_then(Tag::as_str), Some(r#"{"text":"Excalibur","color":"aqua"}"#));
        assert_eq!(display.get("Lore").unwrap().as_list().unwrap()[1], Tag::String("\"line two\"".into()));
        let ench = tag.get("Enchantments").unwrap().as_list().unwrap();
        assert_eq!(ench[0].get("id"), Some(&Tag::String("minecraft:sharpness".into())));
        assert_eq!(ench[0].get("lvl"), Some(&Tag::Short(5)));
        assert_eq!(ench[1].get("id"), Some(&Tag::String("minecraft:sweeping".into())));
        assert_eq!(to_server(&items, old, Some(&tag)), input);
    }

    #[test]
    fn hidden_flags_potion_and_attributes_round_trip() {
        let items = Items::new();
        let mut potion = Writer::new();
        potion.bool(true).varint(position(&POTIONS, "strong_swiftness").unwrap()).bool(true).i32(0x123456);
        potion.varint(1).varint(position(&EFFECTS, "haste").unwrap()).varint(1).varint(600).bool(false).bool(true).bool(true).bool(false);
        let (old, tag) = to_client(&items, &stack(item("potion"), &[("potion_contents", potion.buf.clone())]));
        let tag = tag.unwrap();
        assert_eq!(tag.get("Potion"), Some(&Tag::String("minecraft:strong_swiftness".into())));
        assert_eq!(tag.get("CustomPotionColor"), Some(&Tag::Int(0x123456)));
        let e = &tag.get("custom_potion_effects").unwrap().as_list().unwrap()[0];
        assert_eq!(e.get("id"), Some(&Tag::String("minecraft:haste".into())));
        assert_eq!(e.get("amplifier"), Some(&Tag::Byte(1)));
        assert!(tag.get(BACKUP_KEY).is_none());
        assert_eq!(to_server(&items, old, Some(&tag)), stack(item("potion"), &[("potion_contents", potion.buf)]));

        let damage = position(&d::NEW_ATTRIBUTE_NAMES, "generic.attack_damage").unwrap();
        let mut attrs = Writer::new();
        attrs.varint(1).varint(damage).uuid(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10).string("Weapon modifier").f64(7.0).varint(0).varint(1).bool(false);
        let mut unbreakable = Writer::new();
        unbreakable.bool(false);
        let input = stack(item("netherite_sword"), &[("unbreakable", unbreakable.buf), ("attribute_modifiers", attrs.buf)]);
        let (old, tag) = to_client(&items, &input);
        let tag = tag.unwrap();
        assert_eq!(tag.get("HideFlags"), Some(&Tag::Int(HIDE_ATTRIBUTES | HIDE_UNBREAKABLE)));
        let m = &tag.get("AttributeModifiers").unwrap().as_list().unwrap()[0];
        assert_eq!(m.get("AttributeName"), Some(&Tag::String("minecraft:generic.attack_damage".into())));
        assert_eq!(m.get("Slot"), Some(&Tag::String("mainhand".into())));
        assert!(tag.get(BACKUP_KEY).is_none());
        assert_eq!(to_server(&items, old, Some(&tag)), input);
    }

    #[test]
    fn new_attributes_and_components_are_backed_up() {
        let items = Items::new();
        let scale = position(&d::NEW_ATTRIBUTE_NAMES, "generic.scale").unwrap();
        let mut attrs = Writer::new();
        attrs.varint(1).varint(scale).uuid(5).string("big").f64(2.0).varint(0).varint(0).bool(true);
        let mut rarity = Writer::new();
        rarity.varint(3);
        let input = stack(item("stick"), &[("rarity", rarity.buf), ("attribute_modifiers", attrs.buf)]);
        let (old, tag) = to_client(&items, &input);
        let tag = tag.unwrap();
        assert!(tag.get(BACKUP_KEY).is_some());
        assert_eq!(tag.get("AttributeModifiers").unwrap().as_list().unwrap().len(), 0, "1.20.3 has no scale attribute");
        assert_eq!(to_server(&items, old, Some(&tag)), input);
        // a 1.20.5-only item keeps its id
        let mace = item("mace");
        let (old, tag) = to_client(&items, &stack(mace, &[]));
        assert_eq!(old, position(&d::OLD_ITEM_NAMES, "netherite_axe").unwrap());
        let tag = tag.unwrap();
        assert!(tag.get("display").unwrap().get("Name").unwrap().as_str().unwrap().contains("Mace"));
        assert_eq!(to_server(&items, old, Some(&tag)), stack(mace, &[]));
        // a removed component cannot be written as NBT
        let mut w = Writer::new();
        w.varint(1).varint(item("stick")).varint(0).varint(1).varint(comp("max_stack_size"));
        let (old, tag) = to_client(&items, &w.buf);
        assert_eq!(to_server(&items, old, tag.as_ref()), w.buf);
    }

    #[test]
    fn nested_bundle_and_container_items() {
        let items = Items::new();
        let mut name = Writer::new();
        nbt::write(&mut name, &text("inner"));
        let inner = stack(item("diamond"), &[("custom_name", name.buf)]);
        let mut rarity = Writer::new();
        rarity.varint(1);
        let lossy_inner = stack(item("stone"), &[("rarity", rarity.buf)]);
        let mut bundle = Writer::new();
        bundle.varint(2).raw(&inner).raw(&lossy_inner);
        let input = stack(item("bundle"), &[("bundle_contents", bundle.buf)]);
        let (old, tag) = to_client(&items, &input);
        let tag = tag.unwrap();
        assert!(tag.get(BACKUP_KEY).is_none(), "the nested item carries its own backup");
        let list = tag.get("Items").unwrap().as_list().unwrap();
        assert_eq!(list[0].get("id"), Some(&Tag::String("minecraft:diamond".into())));
        assert!(list[1].get("tag").unwrap().get(BACKUP_KEY).is_some());
        assert_eq!(to_server(&items, old, Some(&tag)), input);

        // shulker box: slots with gaps
        let mut container = Writer::new();
        container.varint(3).raw(&stack(item("dirt"), &[])).varint(0).raw(&inner);
        let input = stack(item("shulker_box"), &[("container", container.buf)]);
        let (old, tag) = to_client(&items, &input);
        let tag = tag.unwrap();
        let be = tag.get("BlockEntityTag").unwrap();
        let slots: Vec<i32> = be.get("Items").unwrap().as_list().unwrap().iter().map(|t| t.get("Slot").unwrap().as_i32().unwrap()).collect();
        assert_eq!(slots, vec![0, 2]);
        assert!(tag.get(BACKUP_KEY).is_none());
        assert_eq!(to_server(&items, old, Some(&tag)), input);
    }

    #[test]
    fn client_made_items_become_components() {
        let items = Items::new();
        // creative tab enchanted book and a 1.20.3 tool with its implicit Damage:0
        let mut ench = Tag::compound();
        ench.insert("id", Tag::String("minecraft:mending".into()));
        ench.insert("lvl", Tag::Short(1));
        let mut book = Tag::compound();
        book.insert("StoredEnchantments", Tag::List(vec![ench]));
        let old_book = position(&d::OLD_ITEM_NAMES, "enchanted_book").unwrap();
        let mut e = Writer::new();
        e.varint(1).varint(position(&d::NEW_ENCHANTMENT_NAMES, "mending").unwrap()).varint(1).bool(true);
        assert_eq!(to_server(&items, old_book, Some(&book)), stack(item("enchanted_book"), &[("stored_enchantments", e.buf)]));
        let mut pick = Tag::compound();
        pick.insert("Damage", Tag::Int(0));
        let old_pick = position(&d::OLD_ITEM_NAMES, "iron_pickaxe").unwrap();
        assert_eq!(to_server(&items, old_pick, Some(&pick)), stack(item("iron_pickaxe"), &[]));
        // unknown keys become custom data
        let mut custom = Tag::compound();
        custom.insert("my_plugin", Tag::Int(3));
        let mut cd = Writer::new();
        cd.raw(&nbt_bytes(&custom));
        assert_eq!(to_server(&items, old_pick, Some(&custom)), stack(item("iron_pickaxe"), &[("custom_data", cd.buf)]));
        // ominous banner from the creative tab
        let mut pattern = Tag::compound();
        pattern.insert("Pattern", Tag::String("mr".into()));
        pattern.insert("Color", Tag::Int(9));
        let mut be = Tag::compound();
        be.insert("Patterns", Tag::List(vec![pattern]));
        let mut display = Tag::compound();
        display.insert("Name", Tag::String(r#"{"color":"gold","translate":"block.minecraft.ominous_banner"}"#.into()));
        let mut banner = Tag::compound();
        banner.insert("BlockEntityTag", be);
        banner.insert("HideFlags", Tag::Int(32));
        banner.insert("display", display);
        let out = to_server(&items, position(&d::OLD_ITEM_NAMES, "white_banner").unwrap(), Some(&banner));
        let mut r = Reader::new(&out);
        r.varint().unwrap();
        assert_eq!(r.varint().unwrap(), item("white_banner"));
        assert_eq!(r.varint().unwrap(), 3, "custom name, hide_additional_tooltip, banner patterns");
    }

    #[test]
    fn backup_is_ignored_for_another_item() {
        let items = Items::new();
        let mut rarity = Writer::new();
        rarity.varint(2);
        let (_, tag) = to_client(&items, &stack(item("stick"), &[("rarity", rarity.buf)]));
        let tag = tag.unwrap();
        // the client put the NBT on a different item: the backup is not for it
        let old_stone = position(&d::OLD_ITEM_NAMES, "stone").unwrap();
        let out = to_server(&items, old_stone, Some(&tag));
        let mut r = Reader::new(&out);
        r.varint().unwrap();
        assert_eq!(r.varint().unwrap(), item("stone"));
    }

    #[test]
    fn merchant_cost_has_no_backup() {
        let items = Items::new();
        let mut w = Writer::new();
        let mut r_comp = Writer::new();
        r_comp.varint(1);
        w.varint(item("emerald")).varint(5).varint(1).varint(comp("rarity")).raw(&r_comp.buf);
        let mut out = Writer::new();
        items.item_cost(&mut Reader::new(&w.buf), &mut out).unwrap();
        let mut r = Reader::new(&out.buf);
        assert!(r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), position(&d::OLD_ITEM_NAMES, "emerald").unwrap());
        assert_eq!(r.i8().unwrap(), 5);
        assert_eq!(nbt::read_optional(&mut r).unwrap(), None);
    }

    #[test]
    fn block_positions_pack() {
        for (x, y, z) in [(0, 0, 0), (-1, -64, 5), (30_000_000, 319, -30_000_000)] {
            assert_eq!(unpack_pos(pack_pos(x, y, z)), (x, y, z));
        }
    }
}

//! Item stacks and data components, 1.21 ↔ 1.20.5.
//!
//! The component wire formats are the same in both versions except for food (1.21 added
//! `using_converts_to`), attribute_modifiers (1.21 names a modifier by id, 1.20.5 by UUID + name),
//! jukebox_playable (1.21 only) and enchantment ids (1.21 numbers enchantments by its synced
//! registry, 1.20.5 by its built-in list). Item ids shift around the three new music discs.
//!
//! What the 1.20.5 client cannot keep is backed up, in its 1.21 encoding, inside `custom_data`
//! under [`BACKUP_KEY`]; when the item comes back (creative inventory, container clicks) the backup
//! is restored so nothing is lost.

use super::data_1_20_5 as d;
use crate::proto::buf::{md5, uuid_to_string, DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    /// 1.21 server → 1.20.5 client
    ToClient,
    /// 1.20.5 client → 1.21 server
    ToServer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Reg {
    Item,
    Sound,
    /// Synced via registry data or identical on both sides.
    Same,
}

/// custom_data key holding the 1.21 data a 1.20.5 client cannot keep.
pub const BACKUP_KEY: &str = "mineversed:v1_21";

fn err(msg: &str) -> DecodeError {
    DecodeError::Other(msg.to_string())
}

/// Bytes consumed since `start`.
fn raw_since(r: &mut Reader, start: usize) -> Result<Vec<u8>> {
    let end = r.position();
    r.set_position(start);
    Ok(r.bytes(end - start)?.to_vec())
}

pub fn strip_ns(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}

/// "music_disc_creator" → "Music Disc Creator".
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

// ---- attribute modifier ids ------------------------------------------------------------------

/// Vanilla modifiers whose 1.20.5 UUID was replaced by an id in 1.21 (most and least significant
/// halves). Where an id appears twice the later UUID wins, as in the reference mapping.
const MODIFIER_UUIDS: &[(i64, i64, &str)] = &[
    (-4483571535397864886, -5989644940537681742, "armor.body"),
    (8144722948526719024, -7778190119041365872, "effect.slowness"),
    (6967552254039378640, -9116175396973475259, "enchantment.aqua_affinity"),
    (5279725409867744698, -5150363631200102632, "attacking"),
    (148071211714102867, -7685811609035173472, "attacking"),
    (6196088217904236654, -7493791321850887290, "effect.minining_fatigue"),
    (-5084161844451524480, -8859020046251006329, "enchantment.soul_speed"),
    (-7907339078496465106, -8112074600724210224, "enchantment.swift_sneak"),
    (6688265815086220243, -6545541163342161890, "drinking"),
    (8315164243412860989, -6631520853640075966, "creative_mode_block_range"),
    (4389663563256579765, -4827163546944004714, "enchantment.efficiency"),
    (6732612758648800940, -5145707699103688244, "effect.health_boost"),
    (9079981369298536661, -6728494925450710401, "covered"),
    (-1521481167610687786, -8630419745734927691, "effect.absorption"),
    (-7473408062188862076, -5872005994337939597, "creative_mode_entity_range"),
    (-3721396875562958315, -5317020504214661337, "effect.unluck"),
    (-2861585646493481178, -6113244764726669811, "armor.leggings"),
    (6718535547217657911, -5386630269401489641, "enchantment.sweeping_edge"),
    (-7949229004988660584, -7828611303000832459, "effect.speed"),
    (-8650171790042118250, -5749650997644763080, "enchantment.soul_speed"),
    (551224288813600377, -8734740027710371860, "enchantment.respiration"),
    (-7046399332347654691, -6723081531683397260, "suffocating"),
    (7361814797886573596, -8641397326606817395, "sprinting"),
    (-6972338111383059132, -8978659762232839026, "armor.chestplate"),
    (-5371971015925809039, -6062243582569928137, "enchantment.fire_protection"),
    (7245570952092733273, -8449101711440750679, "effect.strength"),
    (-422425648963762075, -5756800103171642205, "base_attack_speed"),
    (-4607081316629330256, -7008565754814018370, "effect.jump_boost"),
    (271280981090454338, -8746077033958322898, "effect.luck"),
    (2211131075215181206, -5513857709499300658, "powder_snow"),
    (-8908768238899017377, -8313820693701227669, "armor.boots"),
    (-5797418877589107702, -6181652684028920077, "effect.haste"),
    (3086076556416732775, -5150312587563650736, "armor.helmet"),
    (-5082757096938257406, -4891139119377885130, "baby"),
    (2478452629826324956, -7247530463494186011, "effect.weakness"),
    (4659420831966187055, -5191473055587376048, "enchantment.blast_protection"),
    (7301951777949303281, -6753860660653972126, "evil"),
    (8533189226688352746, -8254757081029716377, "baby"),
    (1286946037536540352, -5768092872487507967, "enchantment.depth_strider"),
    (-3801225194067177672, -6586624321849018929, "base_attack_damage"),
];

/// 1.20.5 modifier names and the 1.21 ids they became.
const MODIFIER_NAMES: &[(&str, &str)] = &[
    ("Random spawn bonus", "random_spawn_bonus"),
    ("Random zombie-spawn bonus", "zombie_random_spawn_bonus"),
    ("Leader zombie bonus", "leader_zombie_bonus"),
    ("Zombie reinforcement callee charge", "reinforcement_callee_charge"),
    ("Zombie reinforcement caller charge", "reinforcement_caller_charge"),
];

fn modifier_uuid(msb: i64, lsb: i64) -> u128 {
    (u128::from(msb as u64) << 64) | u128::from(lsb as u64)
}

/// A 36-character UUID string (the only form the reference mapping accepts).
fn parse_uuid(s: &str) -> Option<u128> {
    let ok = s.len() == 36
        && s.char_indices().all(|(i, c)| if matches!(i, 8 | 13 | 18 | 23) { c == '-' } else { c.is_ascii_hexdigit() });
    if !ok {
        return None;
    }
    u128::from_str_radix(&s.replace('-', ""), 16).ok()
}

/// 1.21 modifier id → 1.20.5 UUID: the vanilla UUID if it had one, the id itself if it is a UUID,
/// else a name-based (v3) UUID of the id.
pub fn attribute_id_to_uuid(id: &str) -> u128 {
    let key = strip_ns(id);
    if let Some((msb, lsb, _)) = MODIFIER_UUIDS.iter().rev().find(|(_, _, n)| *n == key) {
        return modifier_uuid(*msb, *lsb);
    }
    let path = id.split_once(':').map_or(id, |(_, p)| p);
    if let Some(uuid) = parse_uuid(path) {
        return uuid;
    }
    let mut b = md5(id.as_bytes());
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    u128::from_be_bytes(b)
}

/// 1.20.5 modifier UUID (and name) → 1.21 id.
pub fn attribute_uuid_to_id(uuid: u128, name: Option<&str>) -> String {
    if let Some((_, _, id)) = MODIFIER_UUIDS.iter().find(|(m, l, _)| modifier_uuid(*m, *l) == uuid) {
        return (*id).to_string();
    }
    if let Some((_, id)) = name.and_then(|n| MODIFIER_NAMES.iter().find(|(old, _)| *old == n)) {
        return (*id).to_string();
    }
    uuid_to_string(uuid)
}

/// The 1.20.5 display name for a 1.21 modifier id.
pub fn attribute_id_to_name(id: &str) -> String {
    let key = strip_ns(id);
    MODIFIER_NAMES.iter().find(|(_, n)| *n == key).map_or_else(|| id.to_string(), |(old, _)| (*old).to_string())
}

/// Whether a modifier survives 1.21 → 1.20.5 → 1.21 unchanged (up to the default namespace).
fn attribute_id_round_trips(id: &str) -> bool {
    let back = attribute_uuid_to_id(attribute_id_to_uuid(id), Some(&attribute_id_to_name(id)));
    strip_ns(&back) == strip_ns(id) && !strip_ns(id).contains(':')
}

// ---- items -----------------------------------------------------------------------------------

/// Converted item patch, collected before writing.
#[derive(Default)]
struct Ctx {
    /// (target component name, target encoding)
    out: Vec<(&'static str, Vec<u8>)>,
    /// target component names
    removed: Vec<&'static str>,
    // ToClient: what goes into the backup
    backup: Vec<(&'static str, Vec<u8>)>,
    backup_removed: Vec<&'static str>,
    // ToServer
    restore: Option<Restore>,
}

#[derive(Default)]
struct Restore {
    id: Option<i32>,
    remove_custom_name: bool,
    comps: Vec<(&'static str, Vec<u8>)>,
    removed: Vec<&'static str>,
    custom_data_was_added: bool,
}

/// What happens to a parsed component.
enum Keep {
    /// Converted without loss.
    Yes,
    /// Converted, but the original is also backed up.
    Lossy,
    /// Dropped; the original is backed up.
    No,
}

pub struct Items {
    pub dir: Dir,
    /// Enchantment ids in this direction (index = source id, value = target id or -1). Built from
    /// the server's enchantment registry; empty until it arrives.
    enchantments: Vec<i32>,
}

impl Items {
    pub fn new(dir: Dir) -> Self {
        Self { dir, enchantments: Vec::new() }
    }

    /// Sets the enchantment remap from the server's enchantment registry (keys in id order).
    pub fn set_enchantments(&mut self, keys: &[String]) {
        let old = |name: &str| d::OLD_ENCHANTMENT_NAMES.iter().position(|n| *n == strip_ns(name)).map_or(-1, |i| i as i32);
        self.enchantments = match self.dir {
            Dir::ToClient => keys.iter().map(|k| old(k)).collect(),
            Dir::ToServer => d::OLD_ENCHANTMENT_NAMES.iter().map(|n| keys.iter().position(|k| strip_ns(k) == *n).map_or(-1, |i| i as i32)).collect(),
        };
    }

    /// Enchantment id in the target version, if it has one.
    pub fn enchantment(&self, id: i32) -> Option<i32> {
        usize::try_from(id).ok().and_then(|i| self.enchantments.get(i)).copied().filter(|v| *v >= 0)
    }

    // ---- id remaps -------------------------------------------------------------------------

    fn lookup(table: &[u32], reverse: &[u32], dir: Dir, id: i32) -> i32 {
        let t = if dir == Dir::ToClient { table } else { reverse };
        if id < 0 {
            return id;
        }
        t.get(id as usize).map_or(id, |v| *v as i32)
    }
    pub fn item(&self, id: i32) -> i32 {
        Self::lookup(&d::ITEMS, &d::ITEMS_REVERSE, self.dir, id)
    }
    pub fn sound(&self, id: i32) -> i32 {
        Self::lookup(&d::SOUNDS, &d::SOUNDS_REVERSE, self.dir, id)
    }
    pub fn attribute(&self, id: i32) -> i32 {
        Self::lookup(&d::ATTRIBUTES, &d::ATTRIBUTES_REVERSE, self.dir, id)
    }
    /// A 1.21 attribute the 1.20.5 client lacks (burning_time, oxygen_bonus, ...).
    pub fn attribute_unknown(id: i32) -> bool {
        id > 0 && d::ATTRIBUTES.get(id as usize) == Some(&0)
    }
    /// 1.21 item that only exists as a stand-in on the 1.20.5 client (the new music discs).
    fn substituted(id: i32) -> bool {
        let Some(&mapped) = d::ITEMS.get(id.max(0) as usize) else { return false };
        d::OLD_ITEM_NAMES.get(mapped as usize) != d::NEW_ITEM_NAMES.get(id.max(0) as usize)
    }
    /// Item id by registry name in the server's (1.21) registry.
    pub fn new_item_by_name(name: &str) -> Option<i32> {
        let n = strip_ns(name);
        d::NEW_ITEM_NAMES.iter().position(|x| *x == n).map(|i| i as i32)
    }
    fn source_component_name(&self, id: i32) -> Option<&'static str> {
        let names: &[&'static str] = if self.dir == Dir::ToClient { &d::NEW_COMPONENT_NAMES } else { &d::OLD_COMPONENT_NAMES };
        names.get(usize::try_from(id).ok()?).copied()
    }
    fn target_component_id(&self, name: &str) -> Option<i32> {
        let names: &[&str] = if self.dir == Dir::ToClient { &d::OLD_COMPONENT_NAMES } else { &d::NEW_COMPONENT_NAMES };
        names.iter().position(|n| *n == name).map(|i| i as i32)
    }
    fn new_component_name(name: &str) -> Option<&'static str> {
        d::NEW_COMPONENT_NAMES.iter().find(|n| **n == strip_ns(name)).copied()
    }
    fn reg_id(&self, reg: Reg, id: i32) -> i32 {
        match reg {
            Reg::Item => self.item(id),
            Reg::Sound => self.sound(id),
            Reg::Same => id,
        }
    }

    // ---- generic pieces --------------------------------------------------------------------

    pub fn copy_varint(r: &mut Reader, w: &mut Writer) -> Result<i32> {
        let v = r.varint()?;
        w.varint(v);
        Ok(v)
    }
    pub fn copy_string(r: &mut Reader, w: &mut Writer) -> Result<()> {
        w.string(r.string()?);
        Ok(())
    }
    pub fn copy_bool(r: &mut Reader, w: &mut Writer) -> Result<bool> {
        let b = r.bool()?;
        w.bool(b);
        Ok(b)
    }
    pub fn copy_bytes(r: &mut Reader, w: &mut Writer, n: usize) -> Result<()> {
        w.raw(r.bytes(n)?);
        Ok(())
    }
    /// One network NBT value.
    pub fn copy_nbt(r: &mut Reader, w: &mut Writer) -> Result<()> {
        let start = r.position();
        nbt::skip(r)?;
        w.raw(&raw_since(r, start)?);
        Ok(())
    }
    /// Bool-prefixed optional NBT.
    pub fn copy_opt_nbt(r: &mut Reader, w: &mut Writer) -> Result<()> {
        if Self::copy_bool(r, w)? {
            Self::copy_nbt(r, w)?;
        }
        Ok(())
    }
    pub fn copy_opt_string(r: &mut Reader, w: &mut Writer) -> Result<()> {
        if Self::copy_bool(r, w)? {
            Self::copy_string(r, w)?;
        }
        Ok(())
    }
    /// `Holder<T>`: varint id+1, or 0 followed by the inline value.
    pub fn holder(&self, r: &mut Reader, w: &mut Writer, reg: Reg, direct: impl FnOnce(&Self, &mut Reader, &mut Writer) -> Result<()>) -> Result<()> {
        let v = r.varint()?;
        if v == 0 {
            w.varint(0);
            direct(self, r, w)
        } else {
            w.varint(self.reg_id(reg, v - 1) + 1);
            Ok(())
        }
    }
    /// `HolderSet`: varint n+1 then n ids, or 0 then a tag key.
    pub fn holder_set(&self, r: &mut Reader, w: &mut Writer, reg: Reg) -> Result<()> {
        let v = r.varint()?;
        w.varint(v);
        if v == 0 {
            return Self::copy_string(r, w);
        }
        for _ in 0..(v - 1).max(0) {
            let id = r.varint()?;
            w.varint(self.reg_id(reg, id));
        }
        Ok(())
    }
    pub fn sound_event(&self, r: &mut Reader, w: &mut Writer) -> Result<()> {
        self.holder(r, w, Reg::Sound, |_, r, w| {
            Self::copy_string(r, w)?;
            if Self::copy_bool(r, w)? {
                Self::copy_bytes(r, w, 4)?;
            }
            Ok(())
        })
    }

    // ---- item stacks -----------------------------------------------------------------------

    /// Full item stack: count, then id + component patch when count > 0.
    pub fn item_stack(&self, r: &mut Reader, w: &mut Writer) -> Result<()> {
        let count = Self::copy_varint(r, w)?;
        if count <= 0 {
            return Ok(());
        }
        let id = r.varint()?;
        let mut ctx = Ctx::default();
        let added = r.varint()?;
        let removed = r.varint()?;
        if added < 0 || removed < 0 || added + removed > 512 {
            return Err(err("bad component counts"));
        }
        for _ in 0..added {
            self.component(r, &mut ctx)?;
        }
        for _ in 0..removed {
            let ty = r.varint()?;
            let name = self.source_component_name(ty).ok_or_else(|| err("unknown removed component"))?;
            if self.dir == Dir::ToClient && name == "jukebox_playable" {
                ctx.backup_removed.push(name);
            } else {
                ctx.removed.push(name);
            }
        }
        let id = self.finish(id, &mut ctx)?;
        w.varint(id);
        self.write_patch(&ctx, w);
        Ok(())
    }

    /// Merchant item cost: id, count, then exact components (no removals, no backup).
    pub fn item_cost(&self, r: &mut Reader, w: &mut Writer) -> Result<()> {
        let id = r.varint()?;
        w.varint(self.item(id));
        Self::copy_varint(r, w)?;
        let n = r.varint()?;
        let mut ctx = Ctx::default();
        for _ in 0..n.max(0) {
            self.component(r, &mut ctx)?;
        }
        let entries: Vec<(i32, &Vec<u8>)> = ctx.out.iter().filter_map(|(name, v)| self.target_component_id(name).map(|t| (t, v))).collect();
        w.varint(entries.len() as i32);
        for (t, v) in entries {
            w.varint(t);
            w.raw(v);
        }
        Ok(())
    }
    pub fn opt_item_cost(&self, r: &mut Reader, w: &mut Writer) -> Result<()> {
        if Self::copy_bool(r, w)? {
            self.item_cost(r, w)?;
        }
        Ok(())
    }

    fn write_patch(&self, ctx: &Ctx, w: &mut Writer) {
        let mut seen: Vec<&str> = Vec::new();
        let mut added: Vec<(i32, &Vec<u8>)> = Vec::new();
        // later entries win (restored backups are pushed last)
        for (name, value) in ctx.out.iter().rev() {
            if seen.contains(name) {
                continue;
            }
            seen.push(name);
            if let Some(t) = self.target_component_id(name) {
                added.push((t, value));
            }
        }
        added.reverse();
        let mut removed: Vec<i32> = Vec::new();
        for name in &ctx.removed {
            if seen.contains(name) {
                continue;
            }
            if let Some(t) = self.target_component_id(name) {
                if !removed.contains(&t) {
                    removed.push(t);
                }
            }
        }
        w.varint(added.len() as i32);
        w.varint(removed.len() as i32);
        for (t, v) in added {
            w.varint(t);
            w.raw(v);
        }
        for t in removed {
            w.varint(t);
        }
    }

    /// Applies the post-processing that needs the whole patch; returns the target item id.
    fn finish(&self, id: i32, ctx: &mut Ctx) -> Result<i32> {
        match self.dir {
            Dir::ToClient => {
                let substituted = Self::substituted(id);
                let has_name = ctx.out.iter().any(|(n, _)| *n == "custom_name");
                let mut added_name = false;
                if substituted && !has_name {
                    let name = d::NEW_ITEM_NAMES.get(id as usize).copied().unwrap_or("");
                    let mut text = Tag::compound();
                    text.insert("text", Tag::String(pretty_name(name)));
                    text.insert("italic", Tag::Byte(0));
                    let mut v = Writer::new();
                    nbt::write(&mut v, &text);
                    ctx.out.push(("custom_name", v.into_inner()));
                    added_name = true;
                }
                if substituted || !ctx.backup.is_empty() || !ctx.backup_removed.is_empty() {
                    let mut b = Tag::compound();
                    if substituted {
                        b.insert("id", Tag::Int(id));
                    }
                    if added_name {
                        b.insert("n", Tag::Byte(1));
                    }
                    if !ctx.backup.is_empty() {
                        let comps = ctx.backup.iter().map(|(n, raw)| (n.to_string(), Tag::ByteArray(raw.iter().map(|b| *b as i8).collect()))).collect();
                        b.insert("c", Tag::Compound(comps));
                    }
                    if !ctx.backup_removed.is_empty() {
                        b.insert("r", Tag::List(ctx.backup_removed.iter().map(|n| Tag::String(n.to_string())).collect()));
                    }
                    let pos = ctx.out.iter().position(|(n, _)| *n == "custom_data");
                    let mut data = match pos {
                        Some(p) => {
                            let bytes = ctx.out.remove(p).1;
                            nbt::read(&mut Reader::new(&bytes))?
                        }
                        None => {
                            b.insert("e", Tag::Byte(1));
                            Tag::compound()
                        }
                    };
                    if !matches!(data, Tag::Compound(_)) {
                        data = Tag::compound();
                    }
                    data.insert(BACKUP_KEY, b);
                    let mut v = Writer::new();
                    nbt::write(&mut v, &data);
                    ctx.out.push(("custom_data", v.into_inner()));
                }
                Ok(self.item(id))
            }
            Dir::ToServer => {
                let mut id = self.item(id);
                if let Some(restore) = ctx.restore.take() {
                    if let Some(orig) = restore.id {
                        id = orig;
                    }
                    if restore.remove_custom_name {
                        ctx.out.retain(|(n, _)| *n != "custom_name");
                    }
                    if restore.custom_data_was_added {
                        // the client only saw custom_data because of the backup; drop it when empty
                        if let Some(p) = ctx.out.iter().position(|(n, _)| *n == "custom_data") {
                            if ctx.out[p].1 == [10, 0] {
                                ctx.out.remove(p);
                            }
                        }
                    }
                    for name in &restore.removed {
                        ctx.out.retain(|(n, _)| n != name);
                        ctx.removed.push(name);
                    }
                    for (name, raw) in restore.comps {
                        ctx.out.retain(|(n, _)| *n != name);
                        ctx.removed.retain(|n| *n != name);
                        ctx.out.push((name, raw));
                    }
                }
                Ok(id)
            }
        }
    }

    /// Reads one component (type id + value) and records its converted form in `ctx`.
    fn component(&self, r: &mut Reader, ctx: &mut Ctx) -> Result<()> {
        let ty = r.varint()?;
        let name = self.source_component_name(ty).ok_or_else(|| err(&format!("unknown data component type {ty}")))?;
        let start = r.position();
        let mut w = Writer::new();
        let keep = self.component_value(name, r, &mut w, ctx)?;
        if self.dir == Dir::ToClient && matches!(keep, Keep::No | Keep::Lossy) {
            let raw = raw_since(r, start)?;
            ctx.backup.push((name, raw));
        }
        if matches!(keep, Keep::Yes | Keep::Lossy) {
            ctx.out.push((name, w.into_inner()));
        }
        Ok(())
    }

    /// Parses one component payload named `name` (source version), writing the target form.
    fn component_value(&self, name: &'static str, r: &mut Reader, w: &mut Writer, ctx: &mut Ctx) -> Result<Keep> {
        let to_client = self.dir == Dir::ToClient;
        macro_rules! varint { () => {{ Self::copy_varint(r, w)?; }}; }
        macro_rules! bool { () => {{ Self::copy_bool(r, w)?; }}; }
        macro_rules! string { () => {{ Self::copy_string(r, w)?; }}; }
        macro_rules! nbt { () => {{ Self::copy_nbt(r, w)?; }}; }
        match name {
            "max_stack_size" | "max_damage" | "damage" | "rarity" | "repair_cost" | "custom_model_data" | "map_id" | "map_post_processing"
            | "ominous_bottle_amplifier" | "base_color" => {
                varint!();
                Ok(Keep::Yes)
            }
            "unbreakable" | "enchantment_glint_override" => {
                bool!();
                Ok(Keep::Yes)
            }
            "hide_additional_tooltip" | "hide_tooltip" | "creative_slot_lock" | "fire_resistant" => Ok(Keep::Yes),
            "custom_name" | "item_name" | "map_decorations" | "debug_stick_state" | "entity_data" | "bucket_entity_data" | "block_entity_data"
            | "recipes" | "container_loot" | "intangible_projectile" | "lock" => {
                nbt!();
                Ok(Keep::Yes)
            }
            "custom_data" => {
                let tag = nbt::read(r)?;
                self.custom_data(tag, w, ctx);
                Ok(Keep::Yes)
            }
            "lore" => {
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    nbt!();
                }
                Ok(Keep::Yes)
            }
            "enchantments" | "stored_enchantments" => {
                let n = r.varint()?;
                let mut kept = Vec::new();
                for _ in 0..n.max(0) {
                    let id = r.varint()?;
                    let level = r.varint()?;
                    if let Some(mapped) = self.enchantment(id) {
                        kept.push((mapped, level));
                    }
                }
                let show = r.bool()?;
                w.varint(kept.len() as i32);
                for (id, level) in &kept {
                    w.varint(*id);
                    w.varint(*level);
                }
                w.bool(show);
                Ok(if (kept.len() as i32) < n { Keep::Lossy } else { Keep::Yes })
            }
            "can_place_on" | "can_break" => {
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    self.block_predicate(r, w)?;
                }
                bool!();
                Ok(Keep::Yes)
            }
            "attribute_modifiers" => self.attribute_modifiers(r, w),
            "food" => {
                varint!(); // nutrition
                Self::copy_bytes(r, w, 4)?; // saturation
                bool!(); // can always eat
                Self::copy_bytes(r, w, 4)?; // eat seconds
                let mut lossy = false;
                if to_client {
                    // 1.21 using_converts_to: the 1.20.5 food component has no remainder
                    if r.bool()? {
                        self.item_stack(r, &mut Writer::new())?;
                        lossy = true;
                    }
                } else {
                    w.bool(false);
                }
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    Self::copy_varint(r, w)?;
                    self.potion_effect_data(r, w)?;
                    Self::copy_bytes(r, w, 4)?; // probability
                }
                Ok(if lossy { Keep::Lossy } else { Keep::Yes })
            }
            "tool" => {
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    self.holder_set(r, w, Reg::Same)?;
                    if Self::copy_bool(r, w)? {
                        Self::copy_bytes(r, w, 4)?;
                    }
                    if Self::copy_bool(r, w)? {
                        bool!();
                    }
                }
                Self::copy_bytes(r, w, 4)?;
                varint!();
                Ok(Keep::Yes)
            }
            "dyed_color" => {
                Self::copy_bytes(r, w, 4)?;
                bool!();
                Ok(Keep::Yes)
            }
            "map_color" => {
                Self::copy_bytes(r, w, 4)?;
                Ok(Keep::Yes)
            }
            "charged_projectiles" | "bundle_contents" | "container" => {
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    self.item_stack(r, w)?;
                }
                Ok(Keep::Yes)
            }
            "potion_contents" => {
                if Self::copy_bool(r, w)? {
                    varint!();
                }
                if Self::copy_bool(r, w)? {
                    Self::copy_bytes(r, w, 4)?;
                }
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    Self::copy_varint(r, w)?;
                    self.potion_effect_data(r, w)?;
                }
                Ok(Keep::Yes)
            }
            "suspicious_stew_effects" => {
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    varint!();
                    varint!();
                }
                Ok(Keep::Yes)
            }
            "writable_book_content" => {
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    string!();
                    Self::copy_opt_string(r, w)?;
                }
                Ok(Keep::Yes)
            }
            "written_book_content" => {
                string!();
                Self::copy_opt_string(r, w)?;
                string!();
                varint!();
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    nbt!();
                    Self::copy_opt_nbt(r, w)?;
                }
                bool!();
                Ok(Keep::Yes)
            }
            "trim" => {
                self.holder(r, w, Reg::Same, |s, r, w| {
                    Self::copy_string(r, w)?; // asset name
                    w.varint(s.item(r.varint()?)); // ingredient
                    Self::copy_bytes(r, w, 4)?; // item model index
                    let n = Self::copy_varint(r, w)?; // armor material overrides
                    for _ in 0..n.max(0) {
                        Self::copy_varint(r, w)?;
                        Self::copy_string(r, w)?;
                    }
                    Self::copy_nbt(r, w)
                })?;
                self.holder(r, w, Reg::Same, |s, r, w| {
                    Self::copy_string(r, w)?; // asset id
                    w.varint(s.item(r.varint()?)); // template item
                    Self::copy_nbt(r, w)?;
                    Self::copy_bool(r, w).map(|_| ())
                })?;
                bool!();
                Ok(Keep::Yes)
            }
            "instrument" => {
                self.holder(r, w, Reg::Same, |s, r, w| {
                    s.sound_event(r, w)?;
                    Self::copy_varint(r, w)?; // use duration
                    Self::copy_bytes(r, w, 4) // range
                })?;
                Ok(Keep::Yes)
            }
            "jukebox_playable" => {
                // 1.21 only: the 1.20.5 client plays a disc by its item instead
                let mut scratch = Writer::new();
                if r.bool()? {
                    self.holder(r, &mut scratch, Reg::Same, |s, r, w| {
                        s.sound_event(r, w)?;
                        Self::copy_nbt(r, w)?;
                        Self::copy_bytes(r, w, 4)?;
                        Self::copy_varint(r, w).map(|_| ())
                    })?;
                } else {
                    r.string()?;
                }
                r.bool()?;
                Ok(Keep::No)
            }
            "lodestone_tracker" => {
                if Self::copy_bool(r, w)? {
                    string!();
                    Self::copy_bytes(r, w, 8)?;
                }
                bool!();
                Ok(Keep::Yes)
            }
            "firework_explosion" => {
                Self::firework_explosion(r, w)?;
                Ok(Keep::Yes)
            }
            "fireworks" => {
                varint!();
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    Self::firework_explosion(r, w)?;
                }
                Ok(Keep::Yes)
            }
            "profile" => {
                Self::copy_opt_string(r, w)?;
                if Self::copy_bool(r, w)? {
                    Self::copy_bytes(r, w, 16)?;
                }
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    string!();
                    string!();
                    Self::copy_opt_string(r, w)?;
                }
                Ok(Keep::Yes)
            }
            "note_block_sound" => {
                string!();
                Ok(Keep::Yes)
            }
            "banner_patterns" => {
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    self.holder(r, w, Reg::Same, |_, r, w| {
                        Self::copy_string(r, w)?;
                        Self::copy_string(r, w)
                    })?;
                    varint!();
                }
                Ok(Keep::Yes)
            }
            "pot_decorations" => {
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    let id = r.varint()?;
                    w.varint(self.item(id));
                }
                Ok(Keep::Yes)
            }
            "block_state" => {
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    string!();
                    string!();
                }
                Ok(Keep::Yes)
            }
            "bees" => {
                let n = Self::copy_varint(r, w)?;
                for _ in 0..n.max(0) {
                    nbt!();
                    varint!();
                    varint!();
                }
                Ok(Keep::Yes)
            }
            other => Err(err(&format!("no codec for data component {other}"))),
        }
    }

    /// attribute_modifiers: 1.21 (attribute, id, amount, operation, slot) ↔ 1.20.5 (attribute,
    /// UUID, name, amount, operation, slot). Attributes the 1.20.5 client lacks are left out.
    fn attribute_modifiers(&self, r: &mut Reader, w: &mut Writer) -> Result<Keep> {
        let n = r.varint()?;
        let mut body = Writer::new();
        let mut kept = 0;
        let mut lossy = false;
        for _ in 0..n.max(0) {
            let attr = r.varint()?;
            match self.dir {
                Dir::ToClient => {
                    let id = r.string()?;
                    let amount = r.f64()?;
                    let op = r.varint()?;
                    let slot = r.varint()?;
                    if Self::attribute_unknown(attr) {
                        lossy = true;
                        continue;
                    }
                    lossy |= !attribute_id_round_trips(id);
                    body.varint(self.attribute(attr));
                    body.uuid(attribute_id_to_uuid(id));
                    body.string(&attribute_id_to_name(id));
                    body.f64(amount);
                    body.varint(op);
                    body.varint(slot);
                }
                Dir::ToServer => {
                    let uuid = r.uuid()?;
                    let name = r.string()?;
                    body.varint(self.attribute(attr));
                    body.string(&attribute_uuid_to_id(uuid, Some(name)));
                    body.f64(r.f64()?);
                    body.varint(r.varint()?);
                    body.varint(r.varint()?);
                }
            }
            kept += 1;
        }
        w.varint(kept);
        w.raw(&body.buf);
        Self::copy_bool(r, w)?; // show in tooltip
        Ok(if lossy { Keep::Lossy } else { Keep::Yes })
    }

    /// custom_data: on the way to the server, pulls our backup out of it.
    fn custom_data(&self, mut tag: Tag, w: &mut Writer, ctx: &mut Ctx) {
        if self.dir == Dir::ToServer {
            if let Some(b) = tag.remove(BACKUP_KEY) {
                let mut restore = Restore {
                    id: b.get("id").and_then(Tag::as_i32),
                    remove_custom_name: b.get("n").is_some(),
                    custom_data_was_added: b.get("e").is_some(),
                    ..Restore::default()
                };
                if let Some(Tag::Compound(comps)) = b.get("c") {
                    for (name, value) in comps {
                        if let (Some(static_name), Tag::ByteArray(raw)) = (Self::new_component_name(name), value) {
                            restore.comps.push((static_name, raw.iter().map(|b| *b as u8).collect()));
                        }
                    }
                }
                if let Some(Tag::List(list)) = b.get("r") {
                    for name in list.iter().filter_map(Tag::as_str) {
                        if let Some(static_name) = Self::new_component_name(name) {
                            restore.removed.push(static_name);
                        }
                    }
                }
                ctx.restore = Some(restore);
            }
        }
        nbt::write(w, &tag);
    }

    fn potion_effect_data(&self, r: &mut Reader, w: &mut Writer) -> Result<()> {
        Self::copy_varint(r, w)?; // amplifier
        Self::copy_varint(r, w)?; // duration
        Self::copy_bytes(r, w, 3)?; // ambient, particles, icon
        if Self::copy_bool(r, w)? {
            self.potion_effect_data(r, w)?;
        }
        Ok(())
    }

    fn firework_explosion(r: &mut Reader, w: &mut Writer) -> Result<()> {
        Self::copy_varint(r, w)?;
        for _ in 0..2 {
            let n = Self::copy_varint(r, w)?;
            Self::copy_bytes(r, w, n.max(0) as usize * 4)?;
        }
        Self::copy_bytes(r, w, 2)
    }

    /// Block predicate: optional blocks, optional state properties, optional nbt.
    fn block_predicate(&self, r: &mut Reader, w: &mut Writer) -> Result<()> {
        if Self::copy_bool(r, w)? {
            self.holder_set(r, w, Reg::Same)?;
        }
        if Self::copy_bool(r, w)? {
            let n = Self::copy_varint(r, w)?;
            for _ in 0..n.max(0) {
                Self::copy_string(r, w)?;
                if Self::copy_bool(r, w)? {
                    Self::copy_string(r, w)?;
                } else {
                    Self::copy_opt_string(r, w)?;
                    Self::copy_opt_string(r, w)?;
                }
            }
        }
        Self::copy_opt_nbt(r, w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp(names: &[&str], name: &str) -> i32 {
        names.iter().position(|n| *n == name).unwrap() as i32
    }

    fn stack(id: i32, comps: &[(i32, Vec<u8>)], removed: &[i32]) -> Vec<u8> {
        let mut w = Writer::new();
        w.varint(1);
        w.varint(id);
        w.varint(comps.len() as i32);
        w.varint(removed.len() as i32);
        for (t, v) in comps {
            w.varint(*t);
            w.raw(v);
        }
        for t in removed {
            w.varint(*t);
        }
        w.into_inner()
    }

    fn convert(items: &Items, bytes: &[u8]) -> Vec<u8> {
        let mut out = Writer::new();
        let mut r = Reader::new(bytes);
        items.item_stack(&mut r, &mut out).unwrap();
        assert!(r.is_empty(), "trailing bytes");
        out.into_inner()
    }

    /// Reads a converted stack's components as (name, raw value) using the target's names.
    fn components(bytes: &[u8], names: &[&str], value_len: impl Fn(&str, &mut Reader)) -> Vec<String> {
        let mut r = Reader::new(bytes);
        r.varint().unwrap();
        r.varint().unwrap();
        let added = r.varint().unwrap();
        r.varint().unwrap();
        let mut found = Vec::new();
        for _ in 0..added {
            let name = names[r.varint().unwrap() as usize];
            value_len(name, &mut r);
            found.push(name.to_string());
        }
        found
    }

    #[test]
    fn music_disc_stand_in_round_trips() {
        let creator = Items::new_item_by_name("music_disc_creator").unwrap();
        let mut song = Writer::new();
        song.bool(false).string("minecraft:creator").bool(true);
        let input = stack(creator, &[(comp(&d::NEW_COMPONENT_NAMES, "jukebox_playable"), song.buf.clone())], &[]);
        let client = convert(&Items::new(Dir::ToClient), &input);
        let mut r = Reader::new(&client);
        r.varint().unwrap();
        let far = d::OLD_ITEM_NAMES.iter().position(|n| *n == "music_disc_far").unwrap() as i32;
        assert_eq!(r.varint().unwrap(), far);
        let found = components(&client, &d::OLD_COMPONENT_NAMES, |_, r| nbt::skip(r).unwrap());
        assert_eq!(found, vec!["custom_name", "custom_data"]);
        let back = convert(&Items::new(Dir::ToServer), &client);
        assert_eq!(back, input);
    }

    #[test]
    fn food_remainder_is_backed_up() {
        let stew = Items::new_item_by_name("mushroom_stew").unwrap();
        let bowl = Items::new_item_by_name("bowl").unwrap();
        let mut food = Writer::new();
        food.varint(6).f32(7.2).bool(false).f32(1.6);
        food.bool(true).varint(1).varint(bowl).varint(0).varint(0); // using_converts_to
        food.varint(0);
        let input = stack(stew, &[(comp(&d::NEW_COMPONENT_NAMES, "food"), food.buf.clone())], &[]);
        let client = convert(&Items::new(Dir::ToClient), &input);
        let mut r = Reader::new(&client);
        r.varint().unwrap();
        r.varint().unwrap();
        assert_eq!(r.varint().unwrap(), 2); // food + custom_data backup
        r.varint().unwrap();
        assert_eq!(r.varint().unwrap(), comp(&d::OLD_COMPONENT_NAMES, "food"));
        assert_eq!(r.varint().unwrap(), 6);
        r.f32().unwrap();
        r.bool().unwrap();
        assert_eq!(r.f32().unwrap(), 1.6);
        assert_eq!(r.varint().unwrap(), 0); // effects follow directly
        assert_eq!(convert(&Items::new(Dir::ToServer), &client), input);
        // a food component made by the client gets an empty remainder
        let mut plain = Writer::new();
        plain.varint(6).f32(7.2).bool(false).f32(1.6).varint(0);
        let from_client = stack(d::ITEMS[stew as usize] as i32, &[(comp(&d::OLD_COMPONENT_NAMES, "food"), plain.buf)], &[]);
        let mut expected = Writer::new();
        expected.varint(6).f32(7.2).bool(false).f32(1.6).bool(false).varint(0);
        assert_eq!(convert(&Items::new(Dir::ToServer), &from_client), stack(stew, &[(comp(&d::NEW_COMPONENT_NAMES, "food"), expected.buf)], &[]));
    }

    #[test]
    fn enchantments_follow_the_registry() {
        let keys: Vec<String> = ["minecraft:sharpness", "minecraft:custom_thing", "minecraft:protection"].iter().map(|s| s.to_string()).collect();
        let mut to_client = Items::new(Dir::ToClient);
        to_client.set_enchantments(&keys);
        let mut to_server = Items::new(Dir::ToServer);
        to_server.set_enchantments(&keys);
        let mut e = Writer::new();
        e.varint(2).varint(0).varint(5).varint(1).varint(1).bool(true);
        let input = stack(1, &[(comp(&d::NEW_COMPONENT_NAMES, "enchantments"), e.buf.clone())], &[]);
        let client = convert(&to_client, &input);
        let mut r = Reader::new(&client);
        r.varint().unwrap();
        r.varint().unwrap();
        r.varint().unwrap();
        r.varint().unwrap();
        assert_eq!(r.varint().unwrap(), comp(&d::OLD_COMPONENT_NAMES, "enchantments"));
        assert_eq!(r.varint().unwrap(), 1, "the unknown enchantment is left out");
        assert_eq!(r.varint().unwrap(), comp(&d::OLD_ENCHANTMENT_NAMES, "sharpness"));
        assert_eq!(r.varint().unwrap(), 5);
        // the original (with the unknown enchantment) comes back
        assert_eq!(convert(&to_server, &client), input);
        // client-made: protection (1.20.5 id 0) → registry id 2
        let mut c = Writer::new();
        c.varint(1).varint(0).varint(4).bool(true);
        let made = stack(1, &[(comp(&d::OLD_COMPONENT_NAMES, "enchantments"), c.buf)], &[]);
        let mut expected = Writer::new();
        expected.varint(1).varint(2).varint(4).bool(true);
        assert_eq!(convert(&to_server, &made), stack(1, &[(comp(&d::NEW_COMPONENT_NAMES, "enchantments"), expected.buf)], &[]));
    }

    #[test]
    fn attribute_modifier_ids_become_uuids() {
        // vanilla ids map to their old UUIDs (the later one where an id is listed twice)
        assert_eq!(attribute_id_to_uuid("minecraft:base_attack_damage"), modifier_uuid(-3801225194067177672, -6586624321849018929));
        assert_eq!(attribute_id_to_uuid("minecraft:baby"), modifier_uuid(8533189226688352746, -8254757081029716377));
        assert_eq!(attribute_uuid_to_id(modifier_uuid(-3801225194067177672, -6586624321849018929), None), "base_attack_damage");
        // an id that is a UUID keeps it
        let id = "minecraft:8d4e1c52-0d5c-4a8a-9f3e-0123456789ab";
        assert_eq!(uuid_to_string(attribute_id_to_uuid(id)), "8d4e1c52-0d5c-4a8a-9f3e-0123456789ab");
        // anything else: name-based UUID (version 3, like Java's UUID.nameUUIDFromBytes)
        let u = attribute_id_to_uuid("mymod:speed");
        assert_eq!((u >> 76) & 0xf, 3);
        assert_eq!(uuid_to_string(u), uuid_to_string(attribute_id_to_uuid("mymod:speed")));
        // old modifier names map to ids
        assert_eq!(attribute_id_to_name("minecraft:random_spawn_bonus"), "Random spawn bonus");
        assert_eq!(attribute_uuid_to_id(1, Some("Random spawn bonus")), "random_spawn_bonus");
        assert!(attribute_id_round_trips("minecraft:armor.helmet"));
        assert!(!attribute_id_round_trips("mymod:speed"));
    }

    #[test]
    fn attribute_modifiers_component_both_ways() {
        let damage = d::NEW_ATTRIBUTE_NAMES.iter().position(|n| *n == "generic.attack_damage").unwrap() as i32;
        let burning = d::NEW_ATTRIBUTE_NAMES.iter().position(|n| *n == "generic.burning_time").unwrap() as i32;
        let mut a = Writer::new();
        a.varint(2);
        a.varint(damage).string("minecraft:base_attack_damage").f64(5.0).varint(0).varint(1);
        a.varint(burning).string("minecraft:x").f64(1.0).varint(0).varint(0);
        a.bool(true);
        let input = stack(1, &[(comp(&d::NEW_COMPONENT_NAMES, "attribute_modifiers"), a.buf.clone())], &[]);
        let client = convert(&Items::new(Dir::ToClient), &input);
        let mut r = Reader::new(&client);
        r.bytes(4).unwrap();
        assert_eq!(r.varint().unwrap(), comp(&d::OLD_COMPONENT_NAMES, "attribute_modifiers"));
        assert_eq!(r.varint().unwrap(), 1);
        assert_eq!(r.varint().unwrap(), d::OLD_ATTRIBUTE_NAMES.iter().position(|n| *n == "generic.attack_damage").unwrap() as i32);
        assert_eq!(r.uuid().unwrap(), modifier_uuid(-3801225194067177672, -6586624321849018929));
        assert_eq!(r.string().unwrap(), "minecraft:base_attack_damage");
        assert_eq!(r.f64().unwrap(), 5.0);
        assert_eq!(convert(&Items::new(Dir::ToServer), &client), input);
    }
}

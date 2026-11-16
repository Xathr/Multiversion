//! Items, particles and entity data, 1.19 ↔ 1.18.2.
//!
//! The item stack layout is the same in both versions (present, id, count, named NBT); only the
//! ids differ. Items 1.18.2 lacks (mud, mangrove wood, sculk, froglights, chest boats, the goat
//! horn, ...) are shown as a similar item carrying the real name. For those the server's id is
//! kept under [`BACKUP_KEY`] in the client's NBT and restored when the client sends the item
//! back (creative inventory, container clicks), so nothing turns into its stand-in on the server.


use super::data_1_18_2 as d;
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};

/// Root NBT key holding the server's (1.19) item: `{id: int, name: byte}`; `name` is set when
/// the display name was added by this step.
pub const BACKUP_KEY: &str = "multiversion:v1_19";

fn name_at(names: &'static [&'static str], id: i32) -> Option<&'static str> {
    names.get(usize::try_from(id).ok()?).copied()
}

fn lookup(table: &[u32], id: i32) -> Option<i32> {
    table.get(usize::try_from(id).ok()?).map(|v| *v as i32)
}

pub fn strip_ns(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}

/// 1.18.2 id for a 1.19 item (air for ids 1.19 doesn't have).
pub fn item_id(id: i32) -> i32 {
    lookup(&d::ITEMS, id).unwrap_or(0)
}

/// The item exists in 1.18.2 only as a stand-in.
pub fn substituted(id: i32) -> bool {
    match name_at(&d::NEW_ITEM_NAMES, id) {
        Some(name) => name_at(&d::OLD_ITEM_NAMES, item_id(id)) != Some(name),
        None => false,
    }
}

/// 1.18.2 item id for a 1.19 one, or `None` when 1.18.2 has no such item (for lists that
/// should not count a stand-in: tags, statistics).
pub fn item_exact(id: i32) -> Option<i32> {
    (name_at(&d::NEW_ITEM_NAMES, id).is_some() && !substituted(id)).then(|| item_id(id))
}

/// 1.18.2 block id for a 1.19 one, or `None` when 1.18.2 has no such block.
pub fn block_exact(id: i32) -> Option<i32> {
    let mapped = lookup(&d::BLOCKS, id)?;
    (name_at(&d::NEW_BLOCK_NAMES, id) == name_at(&d::OLD_BLOCK_NAMES, mapped)).then_some(mapped)
}

/// 1.18.2 entity type for a 1.19 one, or `None` when 1.18.2 has no such entity.
pub fn entity_exact(id: i32) -> Option<i32> {
    let mapped = lookup(&d::ENTITIES, id)?;
    (name_at(&d::NEW_ENTITY_NAMES, id) == name_at(&d::OLD_ENTITY_NAMES, mapped)).then_some(mapped)
}

pub fn block_state(id: i32) -> i32 {
    lookup(&d::BLOCK_STATES, id).unwrap_or(1)
}

/// "mud_bricks" → "Mud Bricks".
fn pretty_name(id: &str) -> String {
    id.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// NBT for a stand-in: the server's id as a backup and, unless the item has a custom name, the
/// real item's name (not italic, like a normal item name).
fn stand_in_tag(id: i32, tag: Option<Tag>) -> Tag {
    let mut t = match tag {
        Some(t @ Tag::Compound(_)) => t,
        _ => Tag::compound(),
    };
    let mut backup = Tag::compound();
    backup.insert("id", Tag::Int(id));
    if !matches!(t.get("display"), Some(Tag::Compound(_))) {
        t.insert("display", Tag::compound());
    }
    if let Some(display) = t.get_mut("display")
        && display.get("Name").is_none()
    {
        let name = pretty_name(name_at(&d::NEW_ITEM_NAMES, id).unwrap_or("item"));
        display.insert("Name", Tag::String(serde_json::json!({"text": name, "italic": false}).to_string()));
        backup.insert("name", Tag::Byte(1));
    }
    t.insert(BACKUP_KEY, backup);
    t
}

/// Skips one named NBT value and copies its bytes unchanged.
pub fn nbt_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    r.copy_to(w, |r| nbt::skip_named_optional(r))
}

/// An item stack, 1.19 → 1.18.2.
pub fn item(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if !present {
        return Ok(());
    }
    let id = r.varint()?;
    w.varint(item_id(id));
    w.i8(r.i8()?);
    if substituted(id) {
        let tag = nbt::read_named_optional(r)?;
        nbt::write_named(w, &stand_in_tag(id, tag));
        Ok(())
    } else {
        nbt_copy(r, w)
    }
}

/// An item stack, 1.18.2 → 1.19: a stand-in with a backup becomes the server's item again.
pub fn item_to_server(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let present = r.bool()?;
    w.bool(present);
    if !present {
        return Ok(());
    }
    let old_id = r.varint()?;
    let count = r.i8()?;
    let start = r.position();
    let tag = nbt::read_named_optional(r)?;
    let raw_len = r.position() - start;
    let mut id = lookup(&d::ITEMS_REVERSE, old_id).unwrap_or(0);
    let mut restored = None;
    if let Some(mut t) = tag
        && let Some(backup) = t.remove(BACKUP_KEY)
    {
        // only a backup that belongs to this stand-in counts: the key is in the client's hands
        if let Some(orig) = backup.get("id").and_then(Tag::as_i32)
            && substituted(orig)
            && item_id(orig) == old_id
        {
            id = orig;
            if backup.get("name").is_some() {
                if let Some(display) = t.get_mut("display") {
                    display.remove("Name");
                }
                if matches!(t.get("display"), Some(Tag::Compound(e)) if e.is_empty()) {
                    t.remove("display");
                }
            }
        }
        restored = Some(t);
    }
    w.varint(id);
    w.i8(count);
    match restored {
        Some(Tag::Compound(e)) if e.is_empty() => {
            w.u8(0);
        }
        Some(t) => nbt::write_named(w, &t),
        None => {
            r.set_position(start);
            w.raw(r.bytes(raw_len)?);
        }
    }
    Ok(())
}

fn varint_copy(r: &mut Reader, w: &mut Writer) -> Result<i32> {
    let v = r.varint()?;
    w.varint(v);
    Ok(v)
}

/// Text components may be this long (in characters), longer than other strings.
const MAX_COMPONENT: usize = 262_144;

fn string_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    w.string(r.string_max(MAX_COMPONENT)?);
    Ok(())
}

/// 1.18.2 particle id for a 1.19 one.
fn particle_id(id: i32) -> i32 {
    lookup(&d::PARTICLES, id).unwrap_or(0)
}

/// Particle id + data. `false` = 1.18.2 has nothing to show for it (the sculk charge, the
/// shriek, and the vibration, whose 1.18.2 form needs a start position 1.19 doesn't send);
/// nothing is written then.
pub fn particle(r: &mut Reader, w: &mut Writer) -> Result<bool> {
    let id = r.varint()?;
    let mut data = Writer::new();
    if !particle_data(id, r, &mut data)? {
        return Ok(false);
    }
    w.varint(particle_id(id));
    w.raw(&data.buf);
    Ok(true)
}

/// The data that follows a 1.19 particle id, for 1.18.2; `false` as for [`particle`].
fn particle_data(id: i32, r: &mut Reader, w: &mut Writer) -> Result<bool> {
    match name_at(&d::NEW_PARTICLE_NAMES, id).unwrap_or("") {
        "block" | "block_marker" | "falling_dust" => {
            w.varint(block_state(r.varint()?));
        }
        "item" => item(r, w)?,
        "dust" => {
            w.raw(r.bytes(16)?);
        }
        "dust_color_transition" => {
            w.raw(r.bytes(28)?);
        }
        "sculk_charge" => {
            r.bytes(4)?;
            return Ok(false);
        }
        "shriek" => {
            r.varint()?;
            return Ok(false);
        }
        "vibration" => {
            match strip_ns(r.string()?) {
                "block" => {
                    r.bytes(8)?;
                }
                "entity" => {
                    r.varint()?;
                    r.bytes(4)?;
                }
                other => return Err(DecodeError::Other(format!("unknown vibration source {other}"))),
            }
            r.varint()?;
            return Ok(false);
        }
        _ => {}
    }
    Ok(true)
}

/// What the step remembers about a spawned entity, for its entity data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Minecarts: index 11 is the displayed block state.
    Minecart,
    /// Goats: 1.19 added a field for each horn.
    Goat,
    /// Paintings are spawned once their motive arrives as entity data.
    Painting,
    /// Shown as a rabbit named "Frog".
    Frog,
    /// Shown as a pufferfish named "Tadpole".
    Tadpole,
    /// Shown as an iron golem named "Warden".
    Warden,
    /// Shown as a vex named "Allay".
    Allay,
}

impl Kind {
    /// The name a stand-in carries, for entities 1.18.2 doesn't have.
    fn stand_in_name(self) -> Option<&'static str> {
        match self {
            Kind::Frog => Some("Frog"),
            Kind::Tadpole => Some("Tadpole"),
            Kind::Warden => Some("Warden"),
            Kind::Allay => Some("Allay"),
            _ => None,
        }
    }

    /// Entity data fields the stand-in (or the 1.18.2 entity) has no place for.
    fn drops(self, index: u8) -> bool {
        match self {
            Kind::Frog => (16..=18).contains(&index), // age, variant, tongue target
            Kind::Warden => index == 16,              // anger
            Kind::Goat => index == 18 || index == 19, // left horn, right horn
            _ => false,
        }
    }
}

pub fn entity_kind(ty: i32) -> Option<Kind> {
    let name = name_at(&d::NEW_ENTITY_NAMES, ty)?;
    match name {
        "goat" => Some(Kind::Goat),
        "painting" => Some(Kind::Painting),
        "frog" => Some(Kind::Frog),
        "tadpole" => Some(Kind::Tadpole),
        "warden" => Some(Kind::Warden),
        "allay" => Some(Kind::Allay),
        _ if name.ends_with("minecart") => Some(Kind::Minecart),
        _ => None,
    }
}

/// 1.19 entity types 1.18.2 spawns with its ADD_MOB packet: everything that is a living entity
/// (which includes the armor stand), except players, who have a packet of their own.
const LIVING: [&str; 78] = [
    "allay", "armor_stand", "axolotl", "bat", "bee", "blaze", "cat", "cave_spider", "chicken", "cod", "cow", "creeper", "dolphin", "donkey", "drowned",
    "elder_guardian", "ender_dragon", "enderman", "endermite", "evoker", "fox", "frog", "ghast", "giant", "glow_squid", "goat", "guardian", "hoglin",
    "horse", "husk", "illusioner", "iron_golem", "llama", "magma_cube", "mule", "mooshroom", "ocelot", "panda", "parrot", "phantom", "pig", "piglin",
    "piglin_brute", "pillager", "polar_bear", "pufferfish", "rabbit", "ravager", "salmon", "sheep", "shulker", "silverfish", "skeleton",
    "skeleton_horse", "slime", "snow_golem", "spider", "squid", "stray", "strider", "tadpole", "trader_llama", "tropical_fish", "turtle", "vex",
    "villager", "vindicator", "wandering_trader", "warden", "witch", "wither", "wither_skeleton", "wolf", "zoglin", "zombie", "zombie_horse",
    "zombie_villager", "zombified_piglin",
];

/// The 1.19 entity type is a living entity.
pub fn is_living(ty: i32) -> bool {
    name_at(&d::NEW_ENTITY_NAMES, ty).is_some_and(|name| LIVING.contains(&name))
}

pub fn is_falling_block(ty: i32) -> bool {
    name_at(&d::NEW_ENTITY_NAMES, ty) == Some("falling_block")
}

/// 1.18.2 entity type for a 1.19 one (a stand-in for the five 1.19 added).
pub fn entity_type(ty: i32) -> i32 {
    lookup(&d::ENTITIES, ty).unwrap_or(0)
}

/// Entity data index of the custom name (an optional text component).
const CUSTOM_NAME: u8 = 2;
/// Entity data index of a painting's motive.
const PAINTING_VARIANT: u8 = 8;
/// Entity data type ids. Both versions agree up to the pose (18); 1.19 added the four after it.
const TYPE_VARINT: i32 = 1;
const TYPE_OPT_COMPONENT: i32 = 5;
const TYPE_POSE: i32 = 18;
const TYPE_CAT_VARIANT: i32 = 19;
const TYPE_FROG_VARIANT: i32 = 20;
const TYPE_OPT_GLOBAL_POS: i32 = 21;
const TYPE_PAINTING_VARIANT: i32 = 22;
/// The last pose 1.18.2 knows (dying); 1.19 added croaking, using tongue, roaring, sniffing,
/// emerging and digging after it.
const LAST_POSE: i32 = 7;

fn stand_in_json(name: &str) -> String {
    serde_json::json!({ "text": name }).to_string()
}

/// 1.18.2 painting motive for a 1.19 one. The four 1.19 added sit before "donkey_kong", and
/// become an older painting of the same size.
pub fn painting_motive(variant: i32) -> i32 {
    match variant {
        29 => 25, // donkey_kong moved
        25 => 19, // earth → wither
        26 => 16, // wind → stage
        27 => 18, // water → skull_and_roses
        28 => 14, // fire → match
        v => v,
    }
}

/// SET_ENTITY_DATA body (after the entity id). `first` = nothing was sent for this entity yet,
/// which is when a stand-in gets its name unless the server gives it one. Returns the painting
/// motive (1.19 id) if the data carried one.
pub fn entity_data(kind: Option<Kind>, first: bool, r: &mut Reader, w: &mut Writer) -> Result<Option<i32>> {
    let stand_in = kind.and_then(Kind::stand_in_name);
    let mut named = false;
    let mut motive = None;
    loop {
        let index = r.u8()?;
        if index == 0xff {
            if let Some(name) = stand_in
                && first
                && !named
            {
                w.u8(CUSTOM_NAME).varint(TYPE_OPT_COMPONENT).bool(true).string(&stand_in_json(name));
            }
            w.u8(0xff);
            return Ok(motive);
        }
        let ty = r.varint()?;
        let mut out_ty = ty;
        let mut v = Writer::new();
        match ty {
            0 | 7 => {
                v.raw(r.bytes(1)?);
            }
            TYPE_VARINT | 11 | 17 => {
                varint_copy(r, &mut v)?;
            }
            2 => {
                v.raw(r.bytes(4)?);
            }
            3 | 4 => string_copy(r, &mut v)?,
            TYPE_OPT_COMPONENT => {
                if r.bool()? {
                    v.bool(true);
                    string_copy(r, &mut v)?;
                } else if let Some(name) = stand_in.filter(|_| index == CUSTOM_NAME) {
                    // the server took the name away: back to the stand-in's
                    v.bool(true).string(&stand_in_json(name));
                } else {
                    v.bool(false);
                }
                named |= index == CUSTOM_NAME;
            }
            6 => item(r, &mut v)?,
            8 => {
                v.raw(r.bytes(12)?);
            }
            9 => {
                v.raw(r.bytes(8)?);
            }
            10 => {
                let some = r.bool()?;
                v.bool(some);
                if some {
                    v.raw(r.bytes(8)?);
                }
            }
            12 => {
                let some = r.bool()?;
                v.bool(some);
                if some {
                    v.raw(r.bytes(16)?);
                }
            }
            13 => {
                // 0 = no block
                let s = r.varint()?;
                v.varint(if s == 0 { 0 } else { block_state(s) });
            }
            14 => nbt_copy(r, &mut v)?,
            15 => {
                if !particle(r, &mut v)? {
                    continue;
                }
            }
            16 => {
                for _ in 0..3 {
                    varint_copy(r, &mut v)?;
                }
            }
            TYPE_POSE => {
                let pose = r.varint()?;
                v.varint(if pose > LAST_POSE { 0 } else { pose });
            }
            TYPE_CAT_VARIANT => {
                // 1.18.2 sends the cat's coat as a plain number, in the same order
                varint_copy(r, &mut v)?;
                out_ty = TYPE_VARINT;
            }
            TYPE_FROG_VARIANT => {
                r.varint()?;
                continue;
            }
            TYPE_OPT_GLOBAL_POS => {
                if r.bool()? {
                    r.string()?;
                    r.i64()?;
                }
                continue;
            }
            TYPE_PAINTING_VARIANT => {
                let variant = r.varint()?;
                if index == PAINTING_VARIANT {
                    motive = Some(variant);
                }
                continue;
            }
            other => return Err(DecodeError::Other(format!("unknown entity data type {other}"))),
        }
        match kind {
            Some(k) if k.drops(index) => continue,
            Some(Kind::Minecart) if index == 11 && ty == TYPE_VARINT => {
                let s = Reader::new(&v.buf).varint()?;
                v = Writer::new();
                v.varint(block_state(s));
            }
            _ => {}
        }
        w.u8(index);
        w.varint(out_ty);
        w.raw(&v.buf);
    }
}

pub fn set_equipment(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 8);
    varint_copy(&mut r, &mut w)?;
    loop {
        let slot = r.u8()?;
        w.u8(slot);
        item(&mut r, &mut w)?;
        if slot & 0x80 == 0 {
            return Ok(w.into_inner());
        }
    }
}

pub fn container_set_content(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 64);
    w.u8(r.u8()?); // container id
    varint_copy(&mut r, &mut w)?; // state id
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        item(&mut r, &mut w)?;
    }
    item(&mut r, &mut w)?; // carried
    Ok(w.into_inner())
}

pub fn container_set_slot(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 4);
    w.u8(r.u8()?);
    varint_copy(&mut r, &mut w)?;
    w.i16(r.i16()?);
    item(&mut r, &mut w)?;
    Ok(w.into_inner())
}

pub fn cooldown(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::new();
    w.varint(item_id(r.varint()?));
    w.raw(r.remaining()); // ticks
    Ok(w.into_inner())
}

/// MERCHANT_OFFERS: 1.18.2 counts the offers in one byte and marks the second cost with a
/// "present" flag of its own, where 1.19 just sends an empty stack.
pub fn merchant_offers(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 32);
    varint_copy(&mut r, &mut w)?; // container id
    let n = r.varint()?.clamp(0, 255);
    w.u8(n as u8);
    for _ in 0..n {
        item(&mut r, &mut w)?; // cost a
        item(&mut r, &mut w)?; // result
        let mut second = Writer::new();
        item(&mut r, &mut second)?; // cost b
        let present = second.buf.first() == Some(&1);
        w.bool(present);
        if present {
            w.raw(&second.buf);
        }
        w.raw(r.bytes(1 + 4 * 4 + 4 + 4)?); // out of stock, uses, max uses, xp, special price, multiplier, demand
    }
    w.raw(r.remaining()); // villager level, xp, show progress, can restock
    Ok(w.into_inner())
}

/// LEVEL_PARTICLES: 1.18.2 reads the particle id as an int. `None` for a particle it can't show.
pub fn level_particles(p: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 4);
    // the particle id comes first, its data last
    let id = r.varint()?;
    w.i32(particle_id(id));
    w.raw(r.bytes(1 + 3 * 8 + 4 * 4 + 4)?); // long distance, position, offsets, speed, count
    Ok(particle_data(id, &mut r, &mut w)?.then(|| w.into_inner()))
}

/// Copies a list of strings.
fn strings_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let n = varint_copy(r, w)?;
    for _ in 0..n.max(0) {
        string_copy(r, w)?;
    }
    Ok(())
}

/// Same layout in both versions; only the icon is an item.
pub fn update_advancements(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 256);
    w.bool(r.bool()?); // reset
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        string_copy(&mut r, &mut w)?; // id
        if r.bool()? {
            w.bool(true);
            string_copy(&mut r, &mut w)?; // parent
        } else {
            w.bool(false);
        }
        if r.bool()? {
            w.bool(true);
            string_copy(&mut r, &mut w)?; // title
            string_copy(&mut r, &mut w)?; // description
            item(&mut r, &mut w)?; // icon
            varint_copy(&mut r, &mut w)?; // frame
            let flags = r.i32()?;
            w.i32(flags);
            if flags & 1 != 0 {
                string_copy(&mut r, &mut w)?; // background
            }
            w.raw(r.bytes(8)?); // x, y
        } else {
            w.bool(false);
        }
        strings_copy(&mut r, &mut w)?; // criteria
        let groups = varint_copy(&mut r, &mut w)?;
        for _ in 0..groups.max(0) {
            strings_copy(&mut r, &mut w)?; // requirements
        }
    }
    w.raw(r.remaining()); // removed ids, progress: no items
    Ok(w.into_inner())
}

fn ingredient(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let n = varint_copy(r, w)?;
    for _ in 0..n.max(0) {
        item(r, w)?;
    }
    Ok(())
}

/// UPDATE_RECIPES: the same layout in both versions; every ingredient and result is an item.
pub fn update_recipes(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let n = r.varint()?;
    let mut body = Writer::with_capacity(p.len() + 1024);
    let mut kept = 0;
    for _ in 0..n.max(0) {
        let ty = r.string()?;
        let id = r.string()?;
        let mut w = Writer::new();
        w.string(ty).string(id);
        match strip_ns(ty) {
            "crafting_shapeless" => {
                string_copy(&mut r, &mut w)?; // group
                let k = varint_copy(&mut r, &mut w)?;
                for _ in 0..k.max(0) {
                    ingredient(&mut r, &mut w)?;
                }
                item(&mut r, &mut w)?;
            }
            "crafting_shaped" => {
                let width = varint_copy(&mut r, &mut w)?;
                let height = varint_copy(&mut r, &mut w)?;
                string_copy(&mut r, &mut w)?; // group
                for _ in 0..i64::from(width.max(0)) * i64::from(height.max(0)) {
                    ingredient(&mut r, &mut w)?;
                }
                item(&mut r, &mut w)?;
            }
            "smelting" | "blasting" | "smoking" | "campfire_cooking" => {
                string_copy(&mut r, &mut w)?; // group
                ingredient(&mut r, &mut w)?;
                item(&mut r, &mut w)?;
                w.raw(r.bytes(4)?); // xp
                varint_copy(&mut r, &mut w)?; // cooking time
            }
            "stonecutting" => {
                string_copy(&mut r, &mut w)?; // group
                ingredient(&mut r, &mut w)?;
                item(&mut r, &mut w)?;
            }
            "smithing" => {
                ingredient(&mut r, &mut w)?; // base
                ingredient(&mut r, &mut w)?; // addition
                item(&mut r, &mut w)?;
            }
            s if s.starts_with("crafting_special_") => {}
            // a type this step can't read: the rest of the list can't be read either
            _ => break,
        }
        body.raw(&w.buf);
        kept += 1;
    }
    let mut w = Writer::with_capacity(body.buf.len() + 5);
    w.varint(kept);
    w.raw(&body.buf);
    Ok(w.into_inner())
}

/// SET_CREATIVE_MODE_SLOT 1.18.2 → 1.19.
pub fn set_creative_slot(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.i16(r.i16()?);
    item_to_server(&mut r, &mut w)?;
    Ok(w.into_inner())
}

/// CONTAINER_CLICK 1.18.2 → 1.19.
pub fn container_click(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.u8(r.u8()?); // container id
    varint_copy(&mut r, &mut w)?; // state id
    w.i16(r.i16()?); // slot
    w.i8(r.i8()?); // button
    varint_copy(&mut r, &mut w)?; // mode
    let n = varint_copy(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        w.i16(r.i16()?);
        item_to_server(&mut r, &mut w)?;
    }
    item_to_server(&mut r, &mut w)?; // carried
    Ok(w.into_inner())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn pos(names: &[&str], name: &str) -> i32 {
        names.iter().position(|n| *n == name).unwrap_or_else(|| panic!("{name}")) as i32
    }

    pub fn new_item(name: &str) -> i32 {
        pos(&d::NEW_ITEM_NAMES, name)
    }

    pub fn old_item(name: &str) -> i32 {
        pos(&d::OLD_ITEM_NAMES, name)
    }

    pub fn stack(id: i32, count: i8, tag: Option<&Tag>) -> Vec<u8> {
        let mut w = Writer::new();
        w.bool(true).varint(id).i8(count);
        match tag {
            Some(t) => nbt::write_named(&mut w, t),
            None => {
                w.u8(0);
            }
        }
        w.into_inner()
    }

    fn to_client(bytes: &[u8]) -> Vec<u8> {
        let mut r = Reader::new(bytes);
        let mut w = Writer::new();
        item(&mut r, &mut w).unwrap();
        assert!(r.is_empty());
        w.into_inner()
    }

    fn to_server(bytes: &[u8]) -> Vec<u8> {
        let mut r = Reader::new(bytes);
        let mut w = Writer::new();
        item_to_server(&mut r, &mut w).unwrap();
        assert!(r.is_empty());
        w.into_inner()
    }

    #[test]
    fn items_both_versions_have_only_change_id() {
        for name in ["stone", "diamond_sword", "oak_boat", "music_disc_13"] {
            let sent = stack(new_item(name), 3, None);
            let shown = to_client(&sent);
            assert_eq!(shown, stack(old_item(name), 3, None), "{name}");
            assert_eq!(to_server(&shown), sent, "{name}");
        }
        assert_eq!(to_client(&[0]), [0]);
        assert_eq!(to_server(&[0]), [0]);
    }

    #[test]
    fn items_the_client_lacks_become_named_stand_ins_and_come_back() {
        for (name, stand_in, shown_name) in [("mud", "coarse_dirt", "Mud"), ("oak_chest_boat", "oak_boat", "Oak Chest Boat"), ("echo_shard", "prismarine_shard", "Echo Shard")] {
            let sent = stack(new_item(name), 1, None);
            let shown = to_client(&sent);
            let mut r = Reader::new(&shown);
            assert!(r.bool().unwrap());
            assert_eq!(r.varint().unwrap(), old_item(stand_in), "{name}");
            r.i8().unwrap();
            let tag = nbt::read_named_optional(&mut r).unwrap().unwrap();
            let display = tag.get("display").and_then(|d| d.get("Name")).and_then(Tag::as_str).unwrap();
            assert!(display.contains(shown_name), "{display}");
            assert_eq!(to_server(&shown), sent, "{name}");
        }
    }

    fn data(fields: impl FnOnce(&mut Writer)) -> Vec<u8> {
        let mut w = Writer::new();
        fields(&mut w);
        w.u8(0xff);
        w.into_inner()
    }

    fn convert(kind: Option<Kind>, first: bool, bytes: &[u8]) -> (Vec<u8>, Option<i32>) {
        let mut r = Reader::new(bytes);
        let mut w = Writer::new();
        let motive = entity_data(kind, first, &mut r, &mut w).unwrap();
        assert!(r.is_empty());
        (w.into_inner(), motive)
    }

    #[test]
    fn entity_data_types_1_18_2_lacks_are_converted_or_dropped() {
        // a cat's coat becomes a plain number; a global position and a frog's kind go
        let sent = data(|w| {
            w.u8(19).varint(TYPE_CAT_VARIANT).varint(4);
            w.u8(20).varint(TYPE_OPT_GLOBAL_POS).bool(true).string("minecraft:overworld").i64(77);
            w.u8(17).varint(TYPE_FROG_VARIANT).varint(1);
            w.u8(6).varint(TYPE_POSE).varint(9);
            w.u8(7).varint(TYPE_POSE).varint(5);
            w.u8(1).varint(TYPE_VARINT).varint(300);
        });
        let want = data(|w| {
            w.u8(19).varint(TYPE_VARINT).varint(4);
            w.u8(6).varint(TYPE_POSE).varint(0);
            w.u8(7).varint(TYPE_POSE).varint(5);
            w.u8(1).varint(TYPE_VARINT).varint(300);
        });
        assert_eq!(convert(None, false, &sent), (want, None));
        assert!(entity_data(None, false, &mut Reader::new(&[3, 40, 0, 0xff]), &mut Writer::new()).is_err());
    }

    #[test]
    fn stand_ins_are_named_and_lose_fields_they_have_no_place_for() {
        let sent = data(|w| {
            w.u8(0).varint(0).i8(0);
            w.u8(16).varint(TYPE_VARINT).varint(80); // anger
        });
        let named = data(|w| {
            w.u8(0).varint(0).i8(0);
            w.u8(CUSTOM_NAME).varint(TYPE_OPT_COMPONENT).bool(true).string("{\"text\":\"Warden\"}");
        });
        assert_eq!(convert(Some(Kind::Warden), true, &sent).0, named);
        // later updates don't name it again, and the server's own name wins
        assert_eq!(convert(Some(Kind::Warden), false, &sent).0, data(|w| {
            w.u8(0).varint(0).i8(0);
        }));
        let own = data(|w| {
            w.u8(CUSTOM_NAME).varint(TYPE_OPT_COMPONENT).bool(true).string("\"Bob\"");
        });
        assert_eq!(convert(Some(Kind::Frog), true, &own).0, own);
        // goats keep everything but their horns
        let goat = data(|w| {
            w.u8(17).varint(7).bool(true);
            w.u8(18).varint(7).bool(true);
            w.u8(19).varint(7).bool(false);
        });
        assert_eq!(convert(Some(Kind::Goat), false, &goat).0, data(|w| {
            w.u8(17).varint(7).bool(true);
        }));
    }

    #[test]
    fn painting_motives_are_reported_and_renumbered() {
        let sent = data(|w| {
            w.u8(PAINTING_VARIANT).varint(TYPE_PAINTING_VARIANT).varint(29);
        });
        assert_eq!(convert(Some(Kind::Painting), true, &sent), (vec![0xff], Some(29)));
        assert_eq!(painting_motive(29), pos(&d::OLD_PAINTING_NAMES, "donkey_kong"));
        assert_eq!(painting_motive(3), 3);
        assert_eq!(painting_motive(25), pos(&d::OLD_PAINTING_NAMES, "wither"));
    }

    #[test]
    fn particles_and_minecart_blocks_use_old_ids() {
        // a 1.19 block state whose number differs in 1.18.2
        let state = (1..d::BLOCK_STATES.len() as i32).find(|s| block_state(*s) != *s).unwrap();
        let block = pos(&d::NEW_PARTICLE_NAMES, "block");
        let shriek = pos(&d::NEW_PARTICLE_NAMES, "shriek");
        let sent = data(|w| {
            w.u8(10).varint(15).varint(block).varint(state);
            w.u8(12).varint(15).varint(shriek).varint(20);
            w.u8(11).varint(TYPE_VARINT).varint(state);
        });
        let want = data(|w| {
            w.u8(10).varint(15).varint(pos(&d::OLD_PARTICLE_NAMES, "block")).varint(block_state(state));
            w.u8(11).varint(TYPE_VARINT).varint(block_state(state));
        });
        assert_eq!(convert(Some(Kind::Minecart), false, &sent).0, want);
    }

    #[test]
    fn level_particles_carry_an_int_id_or_are_dropped() {
        let packet = |id: i32, data: &[u8]| {
            let mut w = Writer::new();
            w.varint(id).bool(false).f64(1.0).f64(2.0).f64(3.0).f32(0.0).f32(0.0).f32(0.0).f32(1.0).i32(4).raw(data);
            w.into_inner()
        };
        let soul = pos(&d::NEW_PARTICLE_NAMES, "sculk_soul");
        let out = level_particles(&packet(soul, &[])).unwrap().unwrap();
        assert_eq!(Reader::new(&out).i32().unwrap(), pos(&d::OLD_PARTICLE_NAMES, "soul"));
        assert_eq!(out.len(), packet(soul, &[]).len() - crate::proto::buf::varint_len(soul) + 4);
        assert_eq!(level_particles(&packet(pos(&d::NEW_PARTICLE_NAMES, "sculk_charge"), &[0, 0, 0, 0])).unwrap(), None);
        let mut vibration = Writer::new();
        vibration.string("minecraft:block").i64(5).varint(20);
        assert_eq!(level_particles(&packet(pos(&d::NEW_PARTICLE_NAMES, "vibration"), &vibration.buf)).unwrap(), None);
        assert!(level_particles(&packet(soul, &[])[..10]).is_err());
    }

    #[test]
    fn merchant_offers_flag_the_second_cost() {
        let emerald = stack(new_item("emerald"), 2, None);
        let mud = stack(new_item("stone"), 1, None);
        let tail = [0u8; 1 + 16 + 4 + 4];
        let mut w = Writer::new();
        w.varint(3).varint(2);
        w.raw(&emerald).raw(&mud).u8(0).raw(&tail);
        w.raw(&emerald).raw(&mud).raw(&emerald).raw(&tail);
        w.varint(1).varint(0).bool(true).bool(false);
        let old_emerald = stack(old_item("emerald"), 2, None);
        let old_stone = stack(old_item("stone"), 1, None);
        let mut want = Writer::new();
        want.varint(3).u8(2);
        want.raw(&old_emerald).raw(&old_stone).bool(false).raw(&tail);
        want.raw(&old_emerald).raw(&old_stone).bool(true).raw(&old_emerald).raw(&tail);
        want.varint(1).varint(0).bool(true).bool(false);
        assert_eq!(merchant_offers(&w.buf).unwrap(), want.buf);
    }

    #[test]
    fn item_packets_map_every_stack() {
        let new = stack(new_item("diamond"), 1, None);
        let old = stack(old_item("diamond"), 1, None);
        let mut w = Writer::new();
        w.u8(1).varint(9).varint(2).raw(&new).u8(0).raw(&new);
        let mut want = Writer::new();
        want.u8(1).varint(9).varint(2).raw(&old).u8(0).raw(&old);
        assert_eq!(container_set_content(&w.buf).unwrap(), want.buf);

        let mut w = Writer::new();
        w.u8(1).varint(9).i16(4).raw(&new);
        let mut want = Writer::new();
        want.u8(1).varint(9).i16(4).raw(&old);
        assert_eq!(container_set_slot(&w.buf).unwrap(), want.buf);

        let mut w = Writer::new();
        w.varint(7).u8(0x80).raw(&new).u8(1).u8(0);
        let mut want = Writer::new();
        want.varint(7).u8(0x80).raw(&old).u8(1).u8(0);
        assert_eq!(set_equipment(&w.buf).unwrap(), want.buf);

        let mut w = Writer::new();
        w.varint(new_item("ender_pearl")).varint(20);
        let mut want = Writer::new();
        want.varint(old_item("ender_pearl")).varint(20);
        assert_eq!(cooldown(&w.buf).unwrap(), want.buf);

        // recipes: same layout, items mapped
        let mut w = Writer::new();
        w.varint(2);
        w.string("minecraft:crafting_shapeless").string("a").string("").varint(1).varint(1).raw(&new).raw(&new);
        w.string("minecraft:smelting").string("b").string("g").varint(1).raw(&new).raw(&new).f32(0.5).varint(200);
        let mut want = Writer::new();
        want.varint(2);
        want.string("minecraft:crafting_shapeless").string("a").string("").varint(1).varint(1).raw(&old).raw(&old);
        want.string("minecraft:smelting").string("b").string("g").varint(1).raw(&old).raw(&old).f32(0.5).varint(200);
        assert_eq!(update_recipes(&w.buf).unwrap(), want.buf);

        // serverbound
        let mut w = Writer::new();
        w.i16(36).raw(&old);
        let mut want = Writer::new();
        want.i16(36).raw(&new);
        assert_eq!(set_creative_slot(&w.buf).unwrap(), want.buf);
        let mut w = Writer::new();
        w.u8(0).varint(1).i16(5).i8(0).varint(0).varint(1).i16(5).raw(&old).raw(&old);
        let mut want = Writer::new();
        want.u8(0).varint(1).i16(5).i8(0).varint(0).varint(1).i16(5).raw(&new).raw(&new);
        assert_eq!(container_click(&w.buf).unwrap(), want.buf);
    }
}

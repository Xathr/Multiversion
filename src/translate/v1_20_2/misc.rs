//! Everything else 1.20.3 → 1.20.2: text-carrying packets, scoreboards, resource packs,
//! particles, sounds, explosions, containers, merchants, advancements, recipes, commands and the
//! loading screen.

use super::data_1_20_2 as d;
use super::items::{self, copy_bool, copy_bytes, copy_opt_string, copy_string, copy_varint, err, item_stack, item_stack_to_server};
use super::text::{copy_component, copy_opt_component};
use crate::proto::buf::{Reader, Result, Writer};

// ---- particles ---------------------------------------------------------------------------------

/// One particle (id + data). Only vibration's source type changed layout (id → name).
pub fn particle(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let id = r.varint()?;
    w.varint(items::particle(id));
    particle_data(id, r, w)
}

fn particle_data(id: i32, r: &mut Reader, w: &mut Writer) -> Result<()> {
    match usize::try_from(id).ok().and_then(|i| d::NEW_PARTICLE_NAMES.get(i)).copied().unwrap_or("") {
        "block" | "block_marker" | "falling_dust" => {
            w.varint(items::block_state(r.varint()?));
        }
        "item" => item_stack(r, w)?,
        "dust" => copy_bytes(r, w, 16)?,
        "dust_color_transition" => copy_bytes(r, w, 28)?,
        "vibration" => {
            match r.varint()? {
                0 => {
                    w.string("minecraft:block");
                    copy_bytes(r, w, 8)?;
                }
                1 => {
                    w.string("minecraft:entity");
                    copy_varint(r, w)?;
                    copy_bytes(r, w, 4)?;
                }
                _ => return Err(err("unknown vibration source")),
            }
            copy_varint(r, w)?; // arrival ticks
        }
        "sculk_charge" => copy_bytes(r, w, 4)?,
        "shriek" => {
            copy_varint(r, w)?;
        }
        _ => {}
    }
    Ok(())
}

/// LEVEL_PARTICLES: particle id first, its data last.
pub fn level_particles(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let id = r.varint()?;
    w.varint(items::particle(id));
    w.raw(r.bytes(1 + 24 + 12 + 4 + 4)?);
    particle_data(id, &mut r, &mut w)?;
    Ok(w.into_inner())
}

/// SOUND / SOUND_ENTITY: holder id (0 = inline name); `None` = no 1.20.2 sound for it.
pub fn sound(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let holder = r.varint()?;
    if holder == 0 {
        return Ok(Some(payload.to_vec()));
    }
    let Some(id) = items::sound(holder - 1) else { return Ok(None) };
    w.varint(id + 1);
    w.raw(r.remaining());
    Ok(Some(w.into_inner()))
}

/// EXPLODE: 1.20.2 always destroys the listed blocks and has no particle / sound fields.
pub fn explode(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.raw(r.bytes(28)?); // center, power
    let n = r.varint()?;
    let blocks = r.bytes(n.max(0) as usize * 3)?;
    let knockback = r.bytes(12)?;
    // 0 keep, 1 destroy, 2 destroy with decay, 3 trigger blocks
    let destroys = matches!(r.varint()?, 1 | 2);
    if destroys {
        w.varint(n);
        w.raw(blocks);
    } else {
        w.varint(0);
    }
    w.raw(knockback);
    Ok(w.into_inner())
}

// ---- containers --------------------------------------------------------------------------------

/// OPEN_SCREEN: `None` = the crafter, which 1.20.2 can't show.
pub fn open_screen(payload: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_varint(&mut r, &mut w)?;
    let Some(menu) = items::menu(r.varint()?) else { return Ok(None) };
    w.varint(menu);
    copy_component(&mut r, &mut w)?;
    Ok(Some(w.into_inner()))
}

pub fn container_set_content(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.u8(r.u8()?);
    copy_varint(&mut r, &mut w)?; // state id
    let n = copy_varint(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        item_stack(&mut r, &mut w)?;
    }
    item_stack(&mut r, &mut w)?; // carried
    Ok(w.into_inner())
}

pub fn container_set_slot(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.u8(r.u8()?);
    copy_varint(&mut r, &mut w)?;
    w.i16(r.i16()?);
    item_stack(&mut r, &mut w)?;
    Ok(w.into_inner())
}

pub fn merchant_offers(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_varint(&mut r, &mut w)?;
    let n = copy_varint(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        item_stack(&mut r, &mut w)?; // cost A
        item_stack(&mut r, &mut w)?; // result
        item_stack(&mut r, &mut w)?; // cost B
        // disabled, uses, max uses, xp, special price, price multiplier, demand
        copy_bytes(&mut r, &mut w, 1 + 4 * 6)?;
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// COOLDOWN: item id + ticks.
pub fn cooldown(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.varint(items::item(r.varint()?));
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// AWARD_STATS: (stat type, stat id, value); the id is a block, item or entity type.
pub fn award_stats(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let n = copy_varint(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        let category = copy_varint(&mut r, &mut w)?;
        let id = r.varint()?;
        w.varint(match category {
            0 => items::block(id),
            1..=5 => items::item(id),
            6 | 7 => items::entity(id),
            _ => id,
        });
        copy_varint(&mut r, &mut w)?;
    }
    Ok(w.into_inner())
}

pub fn update_advancements(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_bool(&mut r, &mut w)?; // reset
    let n = copy_varint(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        copy_string(&mut r, &mut w)?;
        copy_opt_string(&mut r, &mut w)?; // parent
        if copy_bool(&mut r, &mut w)? {
            copy_component(&mut r, &mut w)?; // title
            copy_component(&mut r, &mut w)?; // description
            item_stack(&mut r, &mut w)?; // icon
            copy_varint(&mut r, &mut w)?; // frame
            let flags = r.i32()?;
            w.i32(flags);
            if flags & 1 != 0 {
                copy_string(&mut r, &mut w)?; // background
            }
            copy_bytes(&mut r, &mut w, 8)?;
        }
        let reqs = copy_varint(&mut r, &mut w)?;
        for _ in 0..reqs.max(0) {
            let m = copy_varint(&mut r, &mut w)?;
            for _ in 0..m.max(0) {
                copy_string(&mut r, &mut w)?;
            }
        }
        copy_bool(&mut r, &mut w)?; // sends telemetry
    }
    w.raw(r.remaining()); // removed, progress
    Ok(w.into_inner())
}

// ---- recipes -----------------------------------------------------------------------------------

fn ingredient(r: &mut Reader, w: &mut Writer) -> Result<()> {
    let n = copy_varint(r, w)?;
    for _ in 0..n.max(0) {
        item_stack(r, w)?;
    }
    Ok(())
}

/// UPDATE_RECIPES: item ids, and shaped recipes put width/height before group and category
/// in 1.20.2.
pub fn update_recipes(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let n = copy_varint(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        let ty = r.string()?;
        w.string(ty);
        copy_string(&mut r, &mut w)?; // recipe id
        match ty.strip_prefix("minecraft:").unwrap_or(ty) {
            "crafting_shaped" => {
                let group = r.string()?;
                let category = r.varint()?;
                let width = r.varint()?;
                let height = r.varint()?;
                w.varint(width).varint(height).string(group).varint(category);
                for _ in 0..(width.max(0) * height.max(0)) {
                    ingredient(&mut r, &mut w)?;
                }
                item_stack(&mut r, &mut w)?;
                copy_bool(&mut r, &mut w)?; // show notification
            }
            "crafting_shapeless" => {
                copy_string(&mut r, &mut w)?;
                copy_varint(&mut r, &mut w)?;
                let k = copy_varint(&mut r, &mut w)?;
                for _ in 0..k.max(0) {
                    ingredient(&mut r, &mut w)?;
                }
                item_stack(&mut r, &mut w)?;
            }
            "smelting" | "blasting" | "smoking" | "campfire_cooking" => {
                copy_string(&mut r, &mut w)?;
                copy_varint(&mut r, &mut w)?;
                ingredient(&mut r, &mut w)?;
                item_stack(&mut r, &mut w)?;
                copy_bytes(&mut r, &mut w, 4)?; // experience
                copy_varint(&mut r, &mut w)?; // cooking time
            }
            "stonecutting" => {
                copy_string(&mut r, &mut w)?;
                ingredient(&mut r, &mut w)?;
                item_stack(&mut r, &mut w)?;
            }
            "smithing_transform" => {
                for _ in 0..3 {
                    ingredient(&mut r, &mut w)?;
                }
                item_stack(&mut r, &mut w)?;
            }
            "smithing_trim" => {
                for _ in 0..3 {
                    ingredient(&mut r, &mut w)?;
                }
            }
            t if t.starts_with("crafting_special_") || t == "crafting_decorated_pot" => {
                copy_varint(&mut r, &mut w)?; // category
            }
            _ => return Err(err("unknown recipe type")),
        }
    }
    Ok(w.into_inner())
}

// ---- commands ----------------------------------------------------------------------------------

/// COMMANDS: remap parser ids; `style` (new in 1.20.3) becomes a quotable string.
pub fn commands(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let n = copy_varint(&mut r, &mut w)?;
    let string_parser = d::OLD_ARGUMENT_TYPE_NAMES.iter().position(|n| *n == "brigadier:string").unwrap_or(5) as i32;
    for _ in 0..n.max(0) {
        let flags = r.u8()?;
        w.u8(flags);
        let children = copy_varint(&mut r, &mut w)?;
        for _ in 0..children.max(0) {
            copy_varint(&mut r, &mut w)?;
        }
        if flags & 0x08 != 0 {
            copy_varint(&mut r, &mut w)?; // redirect
        }
        let node_type = flags & 0x03;
        if node_type == 1 || node_type == 2 {
            copy_string(&mut r, &mut w)?;
        }
        if node_type == 2 {
            let parser = r.varint()?;
            let name = usize::try_from(parser).ok().and_then(|i| d::NEW_ARGUMENT_TYPE_NAMES.get(i)).copied().unwrap_or("");
            if name == "style" {
                w.varint(string_parser);
                w.varint(1); // quotable phrase
            } else {
                w.varint(usize::try_from(parser).ok().and_then(|i| d::ARGUMENT_TYPES.get(i)).map_or(string_parser, |m| *m as i32));
                parser_properties(name, &mut r, &mut w)?;
            }
        }
        if flags & 0x10 != 0 {
            copy_string(&mut r, &mut w)?; // suggestions type
        }
    }
    copy_varint(&mut r, &mut w)?; // root index
    Ok(w.into_inner())
}

fn parser_properties(name: &str, r: &mut Reader, w: &mut Writer) -> Result<()> {
    match name {
        "brigadier:float" | "brigadier:integer" | "brigadier:double" | "brigadier:long" => {
            let size = if name.ends_with("float") || name.ends_with("integer") { 4 } else { 8 };
            let flags = r.u8()?;
            w.u8(flags);
            if flags & 1 != 0 {
                copy_bytes(r, w, size)?;
            }
            if flags & 2 != 0 {
                copy_bytes(r, w, size)?;
            }
        }
        "brigadier:string" => {
            copy_varint(r, w)?;
        }
        "entity" | "score_holder" => copy_bytes(r, w, 1)?,
        "time" => copy_bytes(r, w, 4)?,
        "resource" | "resource_key" | "resource_or_tag" | "resource_or_tag_key" => copy_string(r, w)?,
        _ => {}
    }
    Ok(())
}

pub fn command_suggestions(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_varints(&mut r, &mut w, 3)?; // transaction id, start, length
    let n = copy_varint(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        copy_string(&mut r, &mut w)?;
        copy_opt_component(&mut r, &mut w)?; // tooltip
    }
    Ok(w.into_inner())
}

fn copy_varints(r: &mut Reader, w: &mut Writer, n: usize) -> Result<()> {
    for _ in 0..n {
        copy_varint(r, w)?;
    }
    Ok(())
}

// ---- chat and other text -----------------------------------------------------------------------

/// Packets that are a component followed by anything (system chat, titles, action bar,
/// disconnect, tab list header + footer with `count` = 2).
pub fn leading_components(payload: &[u8], count: usize) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    for _ in 0..count {
        copy_component(&mut r, &mut w)?;
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

pub fn disguised_chat(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_component(&mut r, &mut w)?;
    copy_varint(&mut r, &mut w)?; // chat type
    copy_component(&mut r, &mut w)?; // name
    copy_opt_component(&mut r, &mut w)?; // target
    Ok(w.into_inner())
}

pub fn player_chat(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_bytes(&mut r, &mut w, 16)?; // sender
    copy_varint(&mut r, &mut w)?; // index
    if copy_bool(&mut r, &mut w)? {
        copy_bytes(&mut r, &mut w, 256)?;
    }
    copy_string(&mut r, &mut w)?; // plain message
    copy_bytes(&mut r, &mut w, 16)?; // timestamp, salt
    let seen = copy_varint(&mut r, &mut w)?;
    for _ in 0..seen.max(0) {
        if copy_varint(&mut r, &mut w)? == 0 {
            copy_bytes(&mut r, &mut w, 256)?;
        }
    }
    copy_opt_component(&mut r, &mut w)?; // unsigned content
    if copy_varint(&mut r, &mut w)? == 2 {
        let longs = copy_varint(&mut r, &mut w)?;
        copy_bytes(&mut r, &mut w, longs.max(0) as usize * 8)?;
    }
    copy_varint(&mut r, &mut w)?; // chat type
    copy_component(&mut r, &mut w)?; // sender name
    copy_opt_component(&mut r, &mut w)?; // target
    Ok(w.into_inner())
}

pub fn boss_event(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_bytes(&mut r, &mut w, 16)?;
    let action = copy_varint(&mut r, &mut w)?;
    if action == 0 || action == 3 {
        copy_component(&mut r, &mut w)?;
    }
    w.raw(r.remaining());
    Ok(w.into_inner())
}

pub fn player_combat_kill(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_varint(&mut r, &mut w)?; // player id
    copy_component(&mut r, &mut w)?;
    Ok(w.into_inner())
}

pub fn player_info_update(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    let actions = r.u8()?;
    w.u8(actions);
    let n = copy_varint(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        copy_bytes(&mut r, &mut w, 16)?;
        if actions & 0x01 != 0 {
            copy_string(&mut r, &mut w)?; // name
            let props = copy_varint(&mut r, &mut w)?;
            for _ in 0..props.max(0) {
                copy_string(&mut r, &mut w)?;
                copy_string(&mut r, &mut w)?;
                copy_opt_string(&mut r, &mut w)?;
            }
        }
        if actions & 0x02 != 0 && copy_bool(&mut r, &mut w)? {
            copy_bytes(&mut r, &mut w, 16 + 8)?; // session id, expiry
            let key = r.byte_array()?;
            w.byte_array(key);
            let sig = r.byte_array()?;
            w.byte_array(sig);
        }
        if actions & 0x04 != 0 {
            copy_varint(&mut r, &mut w)?;
        }
        if actions & 0x08 != 0 {
            copy_bool(&mut r, &mut w)?;
        }
        if actions & 0x10 != 0 {
            copy_varint(&mut r, &mut w)?;
        }
        if actions & 0x20 != 0 {
            copy_opt_component(&mut r, &mut w)?;
        }
    }
    Ok(w.into_inner())
}

pub fn set_player_team(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_string(&mut r, &mut w)?;
    let mode = r.i8()?;
    w.i8(mode);
    if mode == 0 || mode == 2 {
        copy_component(&mut r, &mut w)?; // display name
        copy_bytes(&mut r, &mut w, 1)?; // flags
        copy_string(&mut r, &mut w)?; // name tag visibility
        copy_string(&mut r, &mut w)?; // collision rule
        copy_varint(&mut r, &mut w)?; // color
        copy_component(&mut r, &mut w)?; // prefix
        copy_component(&mut r, &mut w)?; // suffix
    }
    w.raw(r.remaining()); // members
    Ok(w.into_inner())
}

pub fn map_item_data(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_varint(&mut r, &mut w)?;
    copy_bytes(&mut r, &mut w, 2)?; // scale, locked
    if copy_bool(&mut r, &mut w)? {
        let n = copy_varint(&mut r, &mut w)?;
        for _ in 0..n.max(0) {
            copy_varint(&mut r, &mut w)?; // type
            copy_bytes(&mut r, &mut w, 3)?; // x, z, rotation
            copy_opt_component(&mut r, &mut w)?;
        }
    }
    w.raw(r.remaining()); // colors
    Ok(w.into_inner())
}

// ---- scoreboard --------------------------------------------------------------------------------

/// SET_OBJECTIVE: display name to JSON; the number format is new in 1.20.3.
pub fn set_objective(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_string(&mut r, &mut w)?;
    let mode = r.i8()?;
    w.i8(mode);
    if mode == 0 || mode == 2 {
        copy_component(&mut r, &mut w)?;
        copy_varint(&mut r, &mut w)?; // render type
    }
    Ok(w.into_inner())
}

/// SET_SCORE: 1.20.2 has an action field and no display name or number format.
pub fn set_score(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    copy_string(&mut r, &mut w)?; // owner
    w.varint(0); // change
    copy_string(&mut r, &mut w)?; // objective
    copy_varint(&mut r, &mut w)?; // score
    Ok(w.into_inner())
}

/// RESET_SCORE → SET_SCORE with the remove action (empty objective = every objective).
pub fn reset_score(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len() + 1);
    copy_string(&mut r, &mut w)?;
    w.varint(1);
    let objective = if r.bool()? { r.string()? } else { "" };
    w.string(objective);
    Ok(w.into_inner())
}

// ---- connection state --------------------------------------------------------------------------

/// Encoded block position (8, 64, 8): the 1.20.2 client's default spawn.
const DEFAULT_SPAWN: i64 = (8 << 38) | (8 << 12) | 64;

pub struct Misc {
    /// Last resource pack the server pushed; 1.20.3 wants its id in the client's answer.
    pack: u128,
    spawn: (i64, f32),
    dimension: Option<String>,
}

impl Default for Misc {
    fn default() -> Self {
        Self::new()
    }
}

impl Misc {
    pub fn new() -> Self {
        Self { pack: 0, spawn: (DEFAULT_SPAWN, 0.0), dimension: None }
    }

    /// RESOURCE_PACK_PUSH (play and configuration) → RESOURCE_PACK: no pack id.
    pub fn resource_pack_push(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut r = Reader::new(payload);
        let mut w = Writer::with_capacity(payload.len());
        self.pack = r.uuid()?;
        copy_string(&mut r, &mut w)?; // url
        copy_string(&mut r, &mut w)?; // hash
        copy_bool(&mut r, &mut w)?; // required
        copy_opt_component(&mut r, &mut w)?; // prompt
        Ok(w.into_inner())
    }

    /// Serverbound RESOURCE_PACK (play and configuration): 1.20.3 names the pack first.
    pub fn resource_pack_status(&self, payload: &[u8]) -> Vec<u8> {
        let mut w = Writer::with_capacity(payload.len() + 16);
        w.uuid(self.pack);
        w.raw(payload);
        w.into_inner()
    }

    pub fn set_default_spawn_position(&mut self, payload: &[u8]) -> Result<()> {
        let mut r = Reader::new(payload);
        self.spawn = (r.i64()?, r.f32()?);
        Ok(())
    }

    fn enter_dimension(&mut self, name: &str) {
        if self.dimension.as_deref() != Some(name) {
            self.dimension = Some(name.to_string());
            self.spawn = (DEFAULT_SPAWN, 0.0);
        }
    }

    /// LOGIN: tracks the dimension (layout unchanged).
    pub fn login(&mut self, payload: &[u8]) -> Result<()> {
        let mut r = Reader::new(payload);
        r.bytes(4 + 1)?; // entity id, hardcore
        r.list(|r| r.string())?; // levels
        for _ in 0..3 {
            r.varint()?; // max players, view and simulation distance
        }
        r.bytes(3)?; // reduced debug info, death screen, limited crafting
        r.string()?; // dimension type
        let name = r.string()?.to_string();
        self.enter_dimension(&name);
        Ok(())
    }

    /// RESPAWN: tracks the dimension (layout unchanged).
    pub fn respawn(&mut self, payload: &[u8]) -> Result<()> {
        let mut r = Reader::new(payload);
        r.string()?;
        let name = r.string()?.to_string();
        self.enter_dimension(&name);
        Ok(())
    }

    /// GAME_EVENT: "level chunks load start" (13) is new; 1.20.2 leaves the loading screen when
    /// it gets the spawn position, so that is sent instead. `Err` only on a short packet.
    pub fn game_event(&self, payload: &[u8]) -> Result<Option<(i32, Vec<u8>)>> {
        let mut r = Reader::new(payload);
        if r.u8()? != 13 {
            return Ok(None);
        }
        let mut w = Writer::new();
        w.i64(self.spawn.0);
        w.f32(self.spawn.1);
        Ok(Some((super::ids::old::SET_DEFAULT_SPAWN_POSITION, w.into_inner())))
    }
}

// ---- serverbound -------------------------------------------------------------------------------

pub fn container_click(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.u8(r.u8()?);
    copy_varint(&mut r, &mut w)?; // state id
    w.i16(r.i16()?);
    w.i8(r.i8()?);
    copy_varint(&mut r, &mut w)?; // mode
    let n = copy_varint(&mut r, &mut w)?;
    for _ in 0..n.max(0) {
        w.i16(r.i16()?);
        item_stack_to_server(&mut r, &mut w)?;
    }
    item_stack_to_server(&mut r, &mut w)?; // carried
    Ok(w.into_inner())
}

pub fn set_creative_slot(payload: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(payload);
    let mut w = Writer::with_capacity(payload.len());
    w.i16(r.i16()?);
    item_stack_to_server(&mut r, &mut w)?;
    Ok(w.into_inner())
}

/// SET_JIGSAW_BLOCK: 1.20.3 appends selection and placement priority.
pub fn set_jigsaw_block(payload: &[u8]) -> Vec<u8> {
    let mut w = Writer::with_capacity(payload.len() + 2);
    w.raw(payload);
    w.varint(0).varint(0);
    w.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::nbt::{self, Tag};
    use serde_json::{json, Value};

    fn text(v: &str) -> Tag {
        Tag::Compound(vec![("text".into(), Tag::String(v.into()))])
    }
    fn json_at(r: &mut Reader) -> Value {
        serde_json::from_str(r.string().unwrap()).unwrap()
    }
    fn particle_id(name: &str) -> i32 {
        d::NEW_PARTICLE_NAMES.iter().position(|n| *n == name).unwrap() as i32
    }
    fn old_particle(name: &str) -> i32 {
        d::OLD_PARTICLE_NAMES.iter().position(|n| *n == name).unwrap() as i32
    }

    #[test]
    fn system_chat_and_tab_list() {
        let mut w = Writer::new();
        nbt::write(&mut w, &text("hello"));
        w.bool(true); // overlay
        let out = leading_components(&w.buf, 1).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(json_at(&mut r), json!({"text": "hello"}));
        assert!(r.bool().unwrap());
        let mut w = Writer::new();
        nbt::write(&mut w, &Tag::String("head".into()));
        nbt::write(&mut w, &text("foot"));
        let out = leading_components(&w.buf, 2).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(json_at(&mut r), json!("head"));
        assert_eq!(json_at(&mut r), json!({"text": "foot"}));
        assert!(r.is_empty());
    }

    #[test]
    fn player_chat_layout() {
        let mut w = Writer::new();
        w.uuid(1).varint(0).bool(true).raw(&[7; 256]);
        w.string("hi").i64(2).i64(3);
        w.varint(2).varint(0).raw(&[8; 256]).varint(5);
        w.bool(true);
        nbt::write(&mut w, &text("unsigned"));
        w.varint(2).varint(1).i64(-1);
        w.varint(4);
        nbt::write(&mut w, &text("Steve"));
        w.bool(false);
        let out = player_chat(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        r.bytes(16).unwrap();
        r.varint().unwrap();
        assert!(r.bool().unwrap());
        r.bytes(256).unwrap();
        assert_eq!(r.string().unwrap(), "hi");
        r.bytes(16).unwrap();
        assert_eq!(r.varint().unwrap(), 2);
        assert_eq!(r.varint().unwrap(), 0);
        r.bytes(256).unwrap();
        assert_eq!(r.varint().unwrap(), 5);
        assert!(r.bool().unwrap());
        assert_eq!(json_at(&mut r), json!({"text": "unsigned"}));
        assert_eq!((r.varint().unwrap(), r.varint().unwrap(), r.i64().unwrap()), (2, 1, -1));
        assert_eq!(r.varint().unwrap(), 4);
        assert_eq!(json_at(&mut r), json!({"text": "Steve"}));
        assert!(!r.bool().unwrap());
        assert!(r.is_empty());
    }

    #[test]
    fn disguised_chat_layout() {
        let mut w = Writer::new();
        nbt::write(&mut w, &text("msg"));
        w.varint(1);
        nbt::write(&mut w, &text("Server"));
        w.bool(true);
        nbt::write(&mut w, &Tag::String("target".into()));
        let out = disguised_chat(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(json_at(&mut r), json!({"text": "msg"}));
        assert_eq!(r.varint().unwrap(), 1);
        assert_eq!(json_at(&mut r), json!({"text": "Server"}));
        assert!(r.bool().unwrap());
        assert_eq!(json_at(&mut r), json!("target"));
    }

    #[test]
    fn scoreboard_packets() {
        let mut w = Writer::new();
        w.string("obj").i8(0);
        nbt::write(&mut w, &text("Kills"));
        w.varint(0).bool(true).varint(0); // render type, blank number format
        let out = set_objective(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!((r.string().unwrap(), r.i8().unwrap()), ("obj", 0));
        assert_eq!(json_at(&mut r), json!({"text": "Kills"}));
        assert_eq!(r.varint().unwrap(), 0);
        assert!(r.is_empty());
        let mut w = Writer::new();
        w.string("Steve").string("obj").varint(12).bool(false).bool(false);
        let out = set_score(&w.buf).unwrap();
        let mut expect = Writer::new();
        expect.string("Steve").varint(0).string("obj").varint(12);
        assert_eq!(out, expect.buf);
        let mut w = Writer::new();
        w.string("Steve").bool(true).string("obj");
        let mut expect = Writer::new();
        expect.string("Steve").varint(1).string("obj");
        assert_eq!(reset_score(&w.buf).unwrap(), expect.buf);
        let mut w = Writer::new();
        w.string("Steve").bool(false);
        let mut expect = Writer::new();
        expect.string("Steve").varint(1).string("");
        assert_eq!(reset_score(&w.buf).unwrap(), expect.buf);
    }

    #[test]
    fn team_boss_bar_and_death_message() {
        let mut w = Writer::new();
        w.string("red").i8(0);
        nbt::write(&mut w, &text("Red"));
        w.u8(3).string("always").string("never").varint(12);
        nbt::write(&mut w, &text("["));
        nbt::write(&mut w, &text("]"));
        w.varint(1).string("Steve");
        let out = set_player_team(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        r.string().unwrap();
        r.i8().unwrap();
        assert_eq!(json_at(&mut r), json!({"text": "Red"}));
        r.u8().unwrap();
        r.string().unwrap();
        r.string().unwrap();
        r.varint().unwrap();
        assert_eq!(json_at(&mut r), json!({"text": "["}));
        assert_eq!(json_at(&mut r), json!({"text": "]"}));
        assert_eq!((r.varint().unwrap(), r.string().unwrap()), (1, "Steve"));

        let mut w = Writer::new();
        w.uuid(5).varint(0);
        nbt::write(&mut w, &text("Boss"));
        w.f32(1.0).varint(0).varint(0).u8(0);
        let out = boss_event(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        r.bytes(16).unwrap();
        r.varint().unwrap();
        assert_eq!(json_at(&mut r), json!({"text": "Boss"}));
        assert_eq!(r.remaining().len(), 4 + 3);

        let mut w = Writer::new();
        w.varint(3);
        nbt::write(&mut w, &text("You died"));
        let out = player_combat_kill(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.varint().unwrap(), 3);
        assert_eq!(json_at(&mut r), json!({"text": "You died"}));
    }

    #[test]
    fn player_info_display_name() {
        let mut w = Writer::new();
        w.u8(0x01 | 0x04 | 0x20).varint(1).uuid(9);
        w.string("Steve").varint(1).string("textures").string("v").bool(false);
        w.varint(1);
        w.bool(true);
        nbt::write(&mut w, &text("[Admin] Steve"));
        let out = player_info_update(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        r.u8().unwrap();
        r.varint().unwrap();
        r.uuid().unwrap();
        assert_eq!(r.string().unwrap(), "Steve");
        r.varint().unwrap();
        r.string().unwrap();
        r.string().unwrap();
        assert!(!r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), 1);
        assert!(r.bool().unwrap());
        assert_eq!(json_at(&mut r), json!({"text": "[Admin] Steve"}));
    }

    #[test]
    fn explosion_drops_new_fields() {
        let mut w = Writer::new();
        w.f64(1.0).f64(2.0).f64(3.0).f32(4.0);
        w.varint(1).raw(&[1, 2, 3]);
        w.f32(0.5).f32(0.0).f32(0.0);
        w.varint(1); // destroy
        w.varint(particle_id("explosion"));
        w.varint(particle_id("explosion_emitter"));
        w.string("minecraft:entity.generic.explode").bool(false);
        let out = explode(&w.buf).unwrap();
        let mut expect = Writer::new();
        expect.f64(1.0).f64(2.0).f64(3.0).f32(4.0).varint(1).raw(&[1, 2, 3]).f32(0.5).f32(0.0).f32(0.0);
        assert_eq!(out, expect.buf);
        // wind charges only push: the blocks stay
        let mut w = Writer::new();
        w.f64(0.0).f64(0.0).f64(0.0).f32(1.0).varint(1).raw(&[1, 1, 1]).f32(0.0).f32(0.0).f32(0.0).varint(3);
        w.varint(particle_id("gust")).varint(particle_id("gust_emitter")).string("minecraft:entity.generic.wind_burst").bool(false);
        let out = explode(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        r.bytes(28).unwrap();
        assert_eq!(r.varint().unwrap(), 0);
        assert_eq!(r.remaining().len(), 12);
    }

    #[test]
    fn particles() {
        let mut w = Writer::new();
        w.varint(particle_id("vibration")).raw(&[0; 45]).varint(1).varint(42).f32(1.5).varint(20);
        let out = level_particles(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.varint().unwrap(), old_particle("vibration"));
        r.bytes(45).unwrap();
        assert_eq!(r.string().unwrap(), "minecraft:entity");
        assert_eq!((r.varint().unwrap(), r.f32().unwrap(), r.varint().unwrap()), (42, 1.5, 20));
        let mut w = Writer::new();
        w.varint(particle_id("gust_emitter")).raw(&[0; 45]);
        let out = level_particles(&w.buf).unwrap();
        assert_eq!(Reader::new(&out).varint().unwrap(), old_particle("explosion_emitter"));
        let last = d::NEW_BLOCK_STATE_COUNT as i32 - 1;
        let mut w = Writer::new();
        w.varint(particle_id("block")).raw(&[0; 45]).varint(last);
        let out = level_particles(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        r.varint().unwrap();
        r.bytes(45).unwrap();
        assert_eq!(r.varint().unwrap(), items::block_state(last));
    }

    #[test]
    fn breeze_sounds_are_dropped() {
        let breeze = d::NEW_SOUND_NAMES.iter().position(|n| *n == "entity.breeze.shoot").unwrap() as i32;
        let mut w = Writer::new();
        w.varint(breeze + 1).varint(0).raw(&[0; 12 + 8 + 8]);
        assert!(sound(&w.buf).unwrap().is_none());
        let teleport = d::NEW_SOUND_NAMES.iter().position(|n| *n == "entity.player.teleport").unwrap() as i32;
        let mut w = Writer::new();
        w.varint(teleport + 1).varint(0);
        let out = sound(&w.buf).unwrap().unwrap();
        let old = d::OLD_SOUND_NAMES.iter().position(|n| *n == "entity.enderman.teleport").unwrap() as i32;
        assert_eq!(Reader::new(&out).varint().unwrap(), old + 1);
    }

    #[test]
    fn shaped_recipe_layout() {
        let diamond = d::NEW_ITEM_NAMES.iter().position(|n| *n == "diamond").unwrap() as i32;
        let crafter = d::NEW_ITEM_NAMES.iter().position(|n| *n == "crafter").unwrap() as i32;
        let mut w = Writer::new();
        w.varint(2);
        w.string("minecraft:crafting_shaped").string("minecraft:crafter").string("").varint(1).varint(1).varint(1);
        w.varint(1).bool(true).varint(diamond).i8(1).u8(0);
        w.bool(true).varint(crafter).i8(1).u8(0);
        w.bool(true);
        w.string("minecraft:crafting_special_armordye").string("minecraft:armor_dye").varint(3);
        let out = update_recipes(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.varint().unwrap(), 2);
        r.string().unwrap();
        r.string().unwrap();
        assert_eq!((r.varint().unwrap(), r.varint().unwrap(), r.string().unwrap(), r.varint().unwrap()), (1, 1, "", 1));
        assert_eq!(r.varint().unwrap(), 1);
        assert!(r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), items::item(diamond));
        r.i8().unwrap();
        r.u8().unwrap();
        assert!(r.bool().unwrap());
        assert_eq!(d::OLD_ITEM_NAMES[r.varint().unwrap() as usize], "crafting_table");
        r.i8().unwrap();
        r.u8().unwrap();
        assert!(r.bool().unwrap());
        r.string().unwrap();
        r.string().unwrap();
        assert_eq!(r.varint().unwrap(), 3);
        assert!(r.is_empty());
    }

    #[test]
    fn style_argument_becomes_string() {
        let style = d::NEW_ARGUMENT_TYPE_NAMES.iter().position(|n| *n == "style").unwrap() as i32;
        let int = d::NEW_ARGUMENT_TYPE_NAMES.iter().position(|n| *n == "brigadier:integer").unwrap() as i32;
        let mut w = Writer::new();
        w.varint(3);
        w.u8(0).varint(2).varint(1).varint(2); // root
        w.u8(2).varint(0).string("style").varint(style);
        w.u8(2).varint(0).string("n").varint(int).u8(1).i32(0);
        w.varint(0);
        let out = commands(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        r.varint().unwrap();
        r.u8().unwrap();
        r.list(|r| r.varint()).unwrap();
        r.u8().unwrap();
        r.varint().unwrap();
        r.string().unwrap();
        assert_eq!(d::OLD_ARGUMENT_TYPE_NAMES[r.varint().unwrap() as usize], "brigadier:string");
        assert_eq!(r.varint().unwrap(), 1);
        r.u8().unwrap();
        r.varint().unwrap();
        r.string().unwrap();
        assert_eq!(d::OLD_ARGUMENT_TYPE_NAMES[r.varint().unwrap() as usize], "brigadier:integer");
        assert_eq!((r.u8().unwrap(), r.i32().unwrap()), (1, 0));
        assert_eq!(r.varint().unwrap(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn crafter_screen_is_dropped() {
        let crafter = d::NEW_MENU_NAMES.iter().position(|n| *n == "crafter_3x3").unwrap() as i32;
        let mut w = Writer::new();
        w.varint(1).varint(crafter);
        nbt::write(&mut w, &text("Crafter"));
        assert!(open_screen(&w.buf).unwrap().is_none());
        let anvil = d::NEW_MENU_NAMES.iter().position(|n| *n == "anvil").unwrap() as i32;
        let mut w = Writer::new();
        w.varint(1).varint(anvil);
        nbt::write(&mut w, &text("Repair"));
        let out = open_screen(&w.buf).unwrap().unwrap();
        let mut r = Reader::new(&out);
        r.varint().unwrap();
        assert_eq!(d::OLD_MENU_NAMES[r.varint().unwrap() as usize], "anvil");
        assert_eq!(json_at(&mut r), json!({"text": "Repair"}));
    }

    #[test]
    fn resource_pack_ids_round_trip() {
        let mut m = Misc::new();
        let mut w = Writer::new();
        w.uuid(0xABCD).string("https://example.invalid/p.zip").string("hash").bool(true).bool(true);
        nbt::write(&mut w, &text("Please"));
        let out = m.resource_pack_push(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        assert_eq!(r.string().unwrap(), "https://example.invalid/p.zip");
        r.string().unwrap();
        assert!(r.bool().unwrap());
        assert!(r.bool().unwrap());
        assert_eq!(json_at(&mut r), json!({"text": "Please"}));
        let back = m.resource_pack_status(&[3]);
        let mut r = Reader::new(&back);
        assert_eq!((r.uuid().unwrap(), r.varint().unwrap()), (0xABCD, 3));
    }

    #[test]
    fn chunk_load_start_becomes_spawn_position() {
        let mut m = Misc::new();
        let mut login = Writer::new();
        login.i32(1).bool(false).varint(1).string("minecraft:overworld").varint(20).varint(10).varint(10);
        login.bool(false).bool(true).bool(false).string("minecraft:overworld").string("minecraft:overworld").raw(&[0; 10]);
        m.login(&login.buf).unwrap();
        let mut spawn = Writer::new();
        spawn.i64(12345).f32(90.0);
        m.set_default_spawn_position(&spawn.buf).unwrap();
        let mut w = Writer::new();
        w.u8(13).f32(0.0);
        let (id, payload) = m.game_event(&w.buf).unwrap().unwrap();
        assert_eq!(id, super::super::ids::old::SET_DEFAULT_SPAWN_POSITION);
        assert_eq!(payload, spawn.buf);
        // a new dimension starts from the default spawn
        let mut respawn = Writer::new();
        respawn.string("minecraft:the_nether").string("minecraft:the_nether");
        m.respawn(&respawn.buf).unwrap();
        let (_, payload) = m.game_event(&w.buf).unwrap().unwrap();
        let mut r = Reader::new(&payload);
        let pos = r.i64().unwrap();
        assert_eq!((pos >> 38, (pos << 26) >> 38, (pos << 52) >> 52), (8, 8, 64));
        // other events pass
        let mut w = Writer::new();
        w.u8(3).f32(1.0);
        assert!(m.game_event(&w.buf).unwrap().is_none());
    }

    #[test]
    fn serverbound_items_and_jigsaw() {
        let diamond_old = d::OLD_ITEM_NAMES.iter().position(|n| *n == "diamond").unwrap() as i32;
        let diamond_new = d::NEW_ITEM_NAMES.iter().position(|n| *n == "diamond").unwrap() as i32;
        let mut w = Writer::new();
        w.i16(36).bool(true).varint(diamond_old).i8(1).u8(0);
        let out = set_creative_slot(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        r.i16().unwrap();
        r.bool().unwrap();
        assert_eq!(r.varint().unwrap(), diamond_new);
        let mut w = Writer::new();
        w.u8(0).varint(1).i16(3).i8(0).varint(0).varint(1).i16(3).bool(true).varint(diamond_old).i8(2).u8(0).bool(false);
        let out = container_click(&w.buf).unwrap();
        let mut r = Reader::new(&out);
        r.bytes(1).unwrap();
        r.varint().unwrap();
        r.bytes(3).unwrap();
        r.varint().unwrap();
        r.varint().unwrap();
        r.i16().unwrap();
        r.bool().unwrap();
        assert_eq!(r.varint().unwrap(), diamond_new);
        let mut w = Writer::new();
        w.i64(0).string("a").string("b").string("c").string("d").string("rollable");
        let out = set_jigsaw_block(&w.buf);
        assert_eq!(&out[w.buf.len()..], &[0, 0]);
    }
}

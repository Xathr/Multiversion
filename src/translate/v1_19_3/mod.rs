//! 1.19.4 (protocol 762) ↔ 1.19.3 (protocol 761).
//!
//! 1.19.4 brought the first 1.20 content behind a feature flag (cherry and bamboo wood, the
//! sniffer, the decorated pot, armor trims), the display and interaction entities, damage
//! types, and four packets: BUNDLE_DELIMITER and CHUNKS_BIOMES are dropped, DAMAGE_EVENT and
//! HURT_ANIMATION become the entity event and animation 1.19.3 used for getting hurt (as
//! ViaBackwards does).
//!
//! Layout changes: PLAYER_POSITION lost its "dismount vehicle" flag, SERVER_DATA sends the icon
//! as bytes, shaped recipes have a notification flag, the `time` command argument has a
//! minimum, and entity data types were renumbered. The registries inside LOGIN lose what 1.19.3
//! can't read. Everything else is ids: items, block states, entities, particles, sounds, block
//! entities, menus, statistics and tags.
//!
//! Text displays and interaction entities are shown as invisible marker armor stands (a text
//! display's text is the stand's name), item displays as such a stand wearing the item, block
//! displays as a falling block that stays put, and sniffers as ravagers.

pub mod ids;
pub mod items;
pub mod world;

use std::collections::HashMap;

use super::{Packet, State, TResult, Translator};
use crate::proto::buf::{Reader, Writer};
use crate::proto::version::Version;
use crate::translate::data_1_19_3 as d;
use ids::{new, old};
use items::Kind;

pub struct V1_19_4To1_19_3 {
    /// Spawned entities whose entity data needs more than a type-by-type rewrite.
    kinds: HashMap<i32, Kind>,
    /// Block and item displays, which 1.19.3 does not have, and what stands in for each.
    displays: HashMap<i32, StandIn>,
    /// Packets for the server, sent on the client's behalf.
    replies: Vec<(State, Packet)>,
}

/// A block or item display as a 1.19.3 client sees it. An item display is an unseen armor stand
/// wearing the item on its head; a block display is a falling block that does not fall, made
/// once the block is known. Both are placed so that what is shown sits where the display is.
struct StandIn {
    uuid: [u8; 16],
    /// Where the display really is.
    at: [f64; 3],
    block: bool,
    /// The block state the falling block was last made with.
    state: Option<i32>,
}

/// How far below an item display its armor stand stands, so the item on its head is level
/// with the display.
const HEAD_HEIGHT: f64 = 1.7;

impl StandIn {
    /// Where the stand-in goes for a display at `at`.
    fn place(&self, at: [f64; 3]) -> [f64; 3] {
        if !self.block {
            [at[0], at[1] - HEAD_HEIGHT, at[2]]
        } else if self.state.is_some() {
            // a block display starts at its corner, a falling block at the middle of its base
            [at[0] + 0.5, at[1], at[2] + 0.5]
        } else {
            at
        }
    }
}

impl V1_19_4To1_19_3 {
    pub fn new() -> Self {
        Self { kinds: HashMap::new(), displays: HashMap::new(), replies: Vec::new() }
    }

    fn play_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            // bundles only group packets into one tick, and resending whole chunks for a biome
            // change isn't worth it
            new::BUNDLE_DELIMITER | new::CHUNKS_BIOMES => return Ok(()),
            new::DAMAGE_EVENT => {
                out.push(Packet::new(old::ENTITY_EVENT, world::damage_event(p).map_err(e)?));
                return Ok(());
            }
            new::HURT_ANIMATION => {
                out.push(Packet::new(old::ANIMATE, world::hurt_animation(p).map_err(e)?));
                return Ok(());
            }
            new::ADD_ENTITY => {
                let spawn = world::add_entity(p).map_err(e)?;
                let kind = items::entity_kind(spawn.ty);
                match kind {
                    Some(k) => self.kinds.insert(spawn.entity, k),
                    None => self.kinds.remove(&spawn.entity),
                };
                let mut payload = spawn.payload;
                self.displays.remove(&spawn.entity);
                if let Some(block) = items::display_shows_block(spawn.ty) {
                    // id, uuid, type, then x y z
                    let mut r = Reader::new(p);
                    r.varint().map_err(e)?;
                    let uuid: [u8; 16] = r.bytes(16).map_err(e)?.try_into().unwrap_or([0; 16]);
                    r.varint().map_err(e)?;
                    let at = [r.f64().map_err(e)?, r.f64().map_err(e)?, r.f64().map_err(e)?];
                    let stand = StandIn { uuid, at, block, state: None };
                    // the same fields in what goes out; only the type's length can differ
                    let mut o = Reader::new(&payload);
                    o.varint().map_err(e)?;
                    o.bytes(16).map_err(e)?;
                    o.varint().map_err(e)?;
                    let start = payload.len() - o.len();
                    for (i, v) in stand.place(at).iter().enumerate() {
                        payload[start + i * 8..start + i * 8 + 8].copy_from_slice(&v.to_be_bytes());
                    }
                    self.displays.insert(spawn.entity, stand);
                }
                out.push(Packet::new(old::ADD_ENTITY, payload));
                let item_display = items::display_shows_block(spawn.ty) == Some(false);
                if let Some(mut data) = items::spawn_data(kind) {
                    // full size for a stand that wears an item, so the item is not tiny
                    if let (true, [.., flags, 0xff]) = (item_display, data.as_mut_slice()) {
                        *flags &= !0x01;
                    }
                    let mut w = Writer::with_capacity(data.len() + 5);
                    w.varint(spawn.entity).raw(&data);
                    out.push(Packet::new(old::SET_ENTITY_DATA, w.into_inner()));
                }
                return Ok(());
            }
            new::REMOVE_ENTITIES => {
                let mut r = Reader::new(p);
                for id in r.list(Reader::varint).map_err(e)? {
                    self.kinds.remove(&id);
                    self.displays.remove(&id);
                }
                p.to_vec()
            }
            new::LOGIN => {
                self.kinds.clear();
                self.displays.clear();
                world::login(p).map_err(e)?
            }
            new::RESPAWN => {
                self.kinds.clear();
                self.displays.clear();
                p.to_vec()
            }
            new::SET_ENTITY_DATA => {
                let mut r = Reader::new(p);
                let entity = r.varint().map_err(e)?;
                let mut w = Writer::with_capacity(p.len() + 8);
                w.varint(entity);
                let shown = items::entity_data(self.kinds.get(&entity).copied(), &mut r, &mut w).map_err(e)?;
                match (shown, self.displays.get_mut(&entity)) {
                    (Some(items::Shown::Item(item)), Some(stand)) if !stand.block => {
                        // the armor stand wears the item on its head (slot 5)
                        let mut gear = Writer::with_capacity(item.len() + 6);
                        gear.varint(entity).u8(5).raw(&item);
                        out.push(Packet::new(old::SET_EQUIPMENT, gear.into_inner()));
                    }
                    (Some(items::Shown::Block(state)), Some(stand)) if stand.block && stand.state != Some(state) => {
                        // a falling block shows the block it was made with, so make it again
                        stand.state = Some(state);
                        let mut gone = Writer::new();
                        gone.varint(1).varint(entity);
                        out.push(Packet::new(old::REMOVE_ENTITIES, gone.into_inner()));
                        let at = stand.place(stand.at);
                        let falling = d::OLD_ENTITY_NAMES.iter().position(|n| *n == "falling_block").unwrap_or(0) as i32;
                        let mut spawn = Writer::new();
                        spawn.varint(entity).raw(&stand.uuid).varint(falling);
                        spawn.f64(at[0]).f64(at[1]).f64(at[2]);
                        spawn.u8(0).u8(0).u8(0).varint(state).i16(0).i16(0).i16(0);
                        out.push(Packet::new(old::ADD_ENTITY, spawn.into_inner()));
                        // it must not fall
                        let mut still = Writer::new();
                        still.varint(entity).u8(5).varint(8).bool(true).u8(0xff);
                        out.push(Packet::new(old::SET_ENTITY_DATA, still.into_inner()));
                        // the fields left over were for the armor stand that has just gone
                        return Ok(());
                    }
                    _ => {}
                }
                w.into_inner()
            }
            new::TELEPORT_ENTITY => {
                // a display that moves takes its stand-in with it
                let mut r = Reader::new(p);
                let entity = r.varint().map_err(e)?;
                match self.displays.get_mut(&entity) {
                    Some(stand) => {
                        stand.at = [r.f64().map_err(e)?, r.f64().map_err(e)?, r.f64().map_err(e)?];
                        let at = stand.place(stand.at);
                        let mut w = Writer::with_capacity(p.len());
                        w.varint(entity).f64(at[0]).f64(at[1]).f64(at[2]).raw(r.remaining());
                        w.into_inner()
                    }
                    None => p.to_vec(),
                }
            }
            new::OPEN_SCREEN => match world::open_screen(p).map_err(e)? {
                world::Screen::Open(v) => v,
                world::Screen::Unknown(container) => {
                    // the client never sees this menu: close it for the server too, so the
                    // player isn't left with a container open that they can't see
                    let mut w = Writer::new();
                    w.u8(container as u8);
                    self.replies.push((State::Play, Packet::new(new::S_CONTAINER_CLOSE, w.into_inner())));
                    return Ok(());
                }
            },
            new::LEVEL_CHUNK_WITH_LIGHT => world::level_chunk_with_light(p).map_err(e)?,
            new::BLOCK_ENTITY_DATA => match world::block_entity_data(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::BLOCK_EVENT => match world::block_event(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::BLOCK_UPDATE => world::block_update(p).map_err(e)?,
            new::SECTION_BLOCKS_UPDATE => world::section_blocks_update(p).map_err(e)?,
            new::LEVEL_EVENT => world::level_event(p).map_err(e)?,
            new::SOUND | new::SOUND_ENTITY => match world::sound(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::STOP_SOUND => world::stop_sound(p).map_err(e)?,
            new::PLAYER_POSITION => world::player_position(p).map_err(e)?,
            new::SERVER_DATA => world::server_data(p).map_err(e)?,
            new::AWARD_STATS => world::award_stats(p).map_err(e)?,
            new::UPDATE_TAGS => world::update_tags(p).map_err(e)?,
            new::COMMANDS => world::commands(p).map_err(e)?,
            new::LEVEL_PARTICLES => items::level_particles(p).map_err(e)?,
            new::SET_EQUIPMENT => items::set_equipment(p).map_err(e)?,
            new::CONTAINER_SET_CONTENT => items::container_set_content(p).map_err(e)?,
            new::CONTAINER_SET_SLOT => items::container_set_slot(p).map_err(e)?,
            new::COOLDOWN => items::cooldown(p).map_err(e)?,
            new::MERCHANT_OFFERS => items::merchant_offers(p).map_err(e)?,
            new::UPDATE_ADVANCEMENTS => items::update_advancements(p).map_err(e)?,
            new::UPDATE_RECIPES => items::update_recipes(p).map_err(e)?,
            _ => p.to_vec(),
        };
        let mapped = ids::play_clientbound(pkt.id).ok_or_else(|| format!("no 1.19.3 equivalent for clientbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }

    fn play_serverbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let e = |e: crate::proto::buf::DecodeError| format!("serverbound 0x{:02x}: {e}", pkt.id);
        let p = &pkt.payload[..];
        let payload = match pkt.id {
            old::S_SET_CREATIVE_MODE_SLOT => items::set_creative_slot(p).map_err(e)?,
            old::S_CONTAINER_CLICK => items::container_click(p).map_err(e)?,
            _ => p.to_vec(),
        };
        let mapped = ids::play_serverbound(pkt.id).ok_or_else(|| format!("unknown 1.19.3 serverbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }
}

impl Default for V1_19_4To1_19_3 {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator for V1_19_4To1_19_3 {
    fn from(&self) -> Version {
        Version { protocol: 762 }
    }
    fn to(&self) -> Version {
        Version { protocol: 761 }
    }
    fn clientbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_clientbound(pkt, out),
            // neither version has a configuration phase: whatever a newer step does with it
            // is none of this step's business
            State::Configuration => {
                out.push(pkt.clone());
                Ok(())
            }
        }
    }
    fn serverbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_serverbound(pkt, out),
            State::Configuration => {
                out.push(pkt.clone());
                Ok(())
            }
        }
    }
    fn take_replies(&mut self) -> Vec<(State, Packet)> {
        std::mem::take(&mut self.replies)
    }
}

#[cfg(test)]
mod tests {
    use super::super::data_1_19_3 as d;
    use super::*;

    fn cb(t: &mut V1_19_4To1_19_3, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        t.clientbound(State::Play, &Packet::new(id, payload), &mut out).unwrap();
        out
    }

    fn sb(t: &mut V1_19_4To1_19_3, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        t.serverbound(State::Play, &Packet::new(id, payload), &mut out).unwrap();
        out
    }

    fn entity(name: &str) -> i32 {
        d::NEW_ENTITY_NAMES.iter().position(|n| *n == name).unwrap() as i32
    }

    fn spawn(id: i32, ty: i32) -> Vec<u8> {
        let mut w = Writer::new();
        w.varint(id).uuid(1).varint(ty).f64(0.0).f64(0.0).f64(0.0).i8(0).i8(0).i8(0).varint(0).i16(0).i16(0).i16(0);
        w.into_inner()
    }

    #[test]
    fn versions() {
        let t = V1_19_4To1_19_3::new();
        assert_eq!((t.from().protocol, t.to().protocol), (762, 761));
        assert!(!t.emulates_configuration());
    }

    #[test]
    fn packets_only_1_19_4_has() {
        let mut t = V1_19_4To1_19_3::new();
        assert!(cb(&mut t, new::BUNDLE_DELIMITER, Vec::new()).is_empty());
        assert!(cb(&mut t, new::CHUNKS_BIOMES, vec![0]).is_empty());
        let mut w = Writer::new();
        w.varint(9).varint(0).varint(0).varint(0).bool(false);
        let out = cb(&mut t, new::DAMAGE_EVENT, w.into_inner());
        assert_eq!((out[0].id, out[0].payload.clone()), (old::ENTITY_EVENT, vec![0, 0, 0, 9, 2]));
        let mut w = Writer::new();
        w.varint(9).f32(10.0);
        let out = cb(&mut t, new::HURT_ANIMATION, w.into_inner());
        assert_eq!((out[0].id, out[0].payload.clone()), (old::ANIMATE, vec![9, 1]));
        assert!(t.take_replies().is_empty());
    }

    #[test]
    fn unchanged_packets_keep_their_payload_under_the_old_id() {
        let mut t = V1_19_4To1_19_3::new();
        // KEEP_ALIVE is 35 in 1.19.4 and 31 in 1.19.3
        let out = cb(&mut t, 35, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!((out[0].id, out[0].payload.clone()), (31, vec![1, 2, 3, 4, 5, 6, 7, 8]));
        let mut out = Vec::new();
        assert!(t.clientbound(State::Play, &Packet::new(111, Vec::new()), &mut out).is_err());
        // serverbound: KEEP_ALIVE 17 → 18, CHAT_SESSION_UPDATE 32 → 6
        let out = sb(&mut t, 17, vec![9; 8]);
        assert_eq!((out[0].id, out[0].payload.clone()), (18, vec![9; 8]));
        assert_eq!(sb(&mut t, 32, vec![1])[0].id, 6);
        let mut out = Vec::new();
        assert!(t.serverbound(State::Play, &Packet::new(51, Vec::new()), &mut out).is_err());
    }

    #[test]
    fn configuration_packets_pass_through() {
        let mut t = V1_19_4To1_19_3::new();
        let mut out = Vec::new();
        t.clientbound(State::Configuration, &Packet::new(5, vec![1, 2]), &mut out).unwrap();
        t.serverbound(State::Configuration, &Packet::new(2, vec![3]), &mut out).unwrap();
        assert_eq!(out.iter().map(|p| (p.id, p.payload.clone())).collect::<Vec<_>>(), vec![(5, vec![1, 2]), (2, vec![3])]);
    }

    /// The packets a real server sends for an item display holding a diamond and a block
    /// display showing a chest (taken from Pumpkin, as the 1.19.4 step hands them on).
    #[test]
    fn item_and_block_displays_get_stand_ins() {
        let ty = |n: &str| d::NEW_ENTITY_NAMES.iter().position(|x| *x == n).unwrap() as i32;
        let old_ty = |n: &str| d::OLD_ENTITY_NAMES.iter().position(|x| *x == n).unwrap() as i32;
        let spawn_at = |id: i32, ty: i32, at: [f64; 3]| {
            let mut w = Writer::new();
            w.varint(id).raw(&[7; 16]).varint(ty).f64(at[0]).f64(at[1]).f64(at[2]).u8(0).u8(0).u8(0).varint(0).i16(0).i16(0).i16(0);
            w.into_inner()
        };
        let mut t = V1_19_4To1_19_3::new();

        // an item display: an armor stand lower down, wearing the item
        let out = cb(&mut t, new::ADD_ENTITY, spawn_at(20, ty("item_display"), [0.5, 120.35, 0.5]));
        assert_eq!(out[0].payload, spawn_at(20, old_ty("armor_stand"), [0.5, 120.35 - HEAD_HEIGHT, 0.5]));
        assert_eq!(out[1].payload[out[1].payload.len() - 4..], [15, 0, 0x10, 0xff], "an unseen marker, full size");
        let mut data = Writer::new();
        data.varint(20).u8(5).varint(8).bool(true);
        data.u8(22).varint(7).bool(true).varint(1).u8(1).u8(0); // one of item 1, no tag
        data.u8(23).varint(0).u8(0).u8(0xff);
        let out = cb(&mut t, new::SET_ENTITY_DATA, data.into_inner());
        assert_eq!(out.iter().map(|p| p.id).collect::<Vec<_>>(), vec![old::SET_EQUIPMENT, old::SET_ENTITY_DATA]);
        assert_eq!(out[0].payload[..2], [20, 5]);
        assert_eq!(out[0].payload[2], 1, "the item is there");
        assert_eq!(out[1].payload, [20, 5, 8, 1, 0xff], "only the fields an armor stand has");
        // it moves with the display
        let mut tp = Writer::new();
        tp.varint(20).f64(3.0).f64(70.0).f64(4.0).u8(9).u8(8).bool(false);
        let out = cb(&mut t, new::TELEPORT_ENTITY, tp.into_inner());
        let mut want = Writer::new();
        want.varint(20).f64(3.0).f64(70.0 - HEAD_HEIGHT).f64(4.0).u8(9).u8(8).bool(false);
        assert_eq!(out[0].payload, want.into_inner());

        // a block display: a falling block that stays put, once the block is known
        let out = cb(&mut t, new::ADD_ENTITY, spawn_at(21, ty("block_display"), [8.0, 64.0, -3.0]));
        assert_eq!(out[0].payload, spawn_at(21, old_ty("armor_stand"), [8.0, 64.0, -3.0]));
        let state = 2955;
        let mut data = Writer::new();
        data.varint(21).u8(22).varint(14).varint(state).u8(0xff);
        let out = cb(&mut t, new::SET_ENTITY_DATA, data.buf.clone());
        assert_eq!(out.iter().map(|p| p.id).collect::<Vec<_>>(), vec![old::REMOVE_ENTITIES, old::ADD_ENTITY, old::SET_ENTITY_DATA]);
        assert_eq!(out[0].payload, [1, 21]);
        let mut want = Writer::new();
        want.varint(21).raw(&[7; 16]).varint(old_ty("falling_block")).f64(8.5).f64(64.0).f64(-2.5).u8(0).u8(0).u8(0).varint(items::block_state(state)).i16(0).i16(0).i16(0);
        assert_eq!(out[1].payload, want.into_inner());
        assert_eq!(out[2].payload, [21, 5, 8, 1, 0xff]);
        // the same block again changes nothing; the server repeats its entity data often
        let out = cb(&mut t, new::SET_ENTITY_DATA, data.into_inner());
        assert_eq!(out.iter().map(|p| p.id).collect::<Vec<_>>(), vec![old::SET_ENTITY_DATA]);
        // other entities are left alone
        let mut tp = Writer::new();
        tp.varint(99).f64(1.0).f64(2.0).f64(3.0).u8(0).u8(0).bool(true);
        assert_eq!(cb(&mut t, new::TELEPORT_ENTITY, tp.buf.clone())[0].payload, tp.buf);
        cb(&mut t, new::REMOVE_ENTITIES, vec![2, 20, 21]);
        assert!(t.displays.is_empty());
    }

    #[test]
    fn display_entities_become_marker_armor_stands() {
        let mut t = V1_19_4To1_19_3::new();
        let out = cb(&mut t, new::ADD_ENTITY, spawn(5, entity("text_display")));
        assert_eq!(out.iter().map(|p| p.id).collect::<Vec<_>>(), vec![old::ADD_ENTITY, old::SET_ENTITY_DATA]);
        assert_eq!(out[0].payload, spawn(5, d::OLD_ENTITY_NAMES.iter().position(|n| *n == "armor_stand").unwrap() as i32));
        let mut want = Writer::new();
        want.varint(5).u8(0).varint(0).u8(0x20).u8(5).varint(8).bool(true).u8(15).varint(0).u8(0x11).u8(0xff);
        assert_eq!(out[1].payload, want.buf);

        // its text arrives as the stand's name, the display fields don't arrive at all
        let mut data = Writer::new();
        data.varint(5).u8(22).varint(5).string("{\"text\":\"hi\"}").u8(23).varint(1).varint(200).u8(0xff);
        let out = cb(&mut t, new::SET_ENTITY_DATA, data.buf.clone());
        let mut want = Writer::new();
        want.varint(5).u8(2).varint(6).bool(true).string("{\"text\":\"hi\"}").u8(3).varint(8).bool(true).u8(0xff);
        assert_eq!((out[0].id, &out[0].payload), (old::SET_ENTITY_DATA, &want.buf));

        // once removed, the id is an ordinary entity again
        cb(&mut t, new::REMOVE_ENTITIES, vec![1, 5]);
        let out = cb(&mut t, new::SET_ENTITY_DATA, vec![5, 23, 1, 7, 0xff]);
        assert_eq!(out[0].payload, vec![5, 23, 1, 7, 0xff]);
    }

    #[test]
    fn sniffers_are_named_ravagers_and_respawn_forgets_entities() {
        let mut t = V1_19_4To1_19_3::new();
        let out = cb(&mut t, new::ADD_ENTITY, spawn(8, entity("sniffer")));
        assert_eq!(out.len(), 2);
        let mut want = Writer::new();
        want.varint(8).u8(2).varint(6).bool(true).string("{\"text\":\"Sniffer\"}").u8(0xff);
        assert_eq!(out[1].payload, want.buf);
        let out = cb(&mut t, new::SET_ENTITY_DATA, vec![8, 17, 25, 1, 0xff]);
        assert_eq!(out[0].payload, vec![8, 0xff]);
        // plain entities need nothing extra
        assert_eq!(cb(&mut t, new::ADD_ENTITY, spawn(9, entity("pig"))).len(), 1);
        // RESPAWN is the same in both versions, but the world's entities are gone
        let mut w = Writer::new();
        w.string("minecraft:overworld").string("minecraft:overworld").i64(5).u8(1).i8(-1).bool(false).bool(true).i8(3).bool(false);
        let out = cb(&mut t, new::RESPAWN, w.buf.clone());
        assert_eq!((out[0].id, &out[0].payload), (61, &w.buf));
        let out = cb(&mut t, new::SET_ENTITY_DATA, vec![8, 17, 1, 1, 0xff]);
        assert_eq!(out[0].payload, vec![8, 17, 1, 1, 0xff]);
    }

    #[test]
    fn the_new_smithing_menu_is_closed_for_the_server() {
        let mut t = V1_19_4To1_19_3::new();
        let mut w = Writer::new();
        w.varint(4).varint(21).string("{\"text\":\"Upgrade Gear\"}");
        assert!(cb(&mut t, new::OPEN_SCREEN, w.into_inner()).is_empty());
        let replies: Vec<_> = t.take_replies().into_iter().map(|(s, p)| (s, p.id, p.payload)).collect();
        assert_eq!(replies, vec![(State::Play, new::S_CONTAINER_CLOSE, vec![4])]);
        // any other menu opens
        let mut w = Writer::new();
        w.varint(4).varint(22).string("{}");
        let out = cb(&mut t, new::OPEN_SCREEN, w.into_inner());
        assert_eq!((out[0].id, out[0].payload[1]), (44, 21));
        assert!(t.take_replies().is_empty());
    }

    #[test]
    fn items_cross_in_both_directions() {
        let new_id = d::NEW_ITEM_NAMES.iter().position(|n| *n == "diamond_sword").unwrap() as i32;
        let old_id = d::OLD_ITEM_NAMES.iter().position(|n| *n == "diamond_sword").unwrap() as i32;
        let slot = |id: i32| {
            let mut w = Writer::new();
            w.u8(0).varint(1).i16(36).bool(true).varint(id).i8(1).u8(0);
            w.into_inner()
        };
        let mut t = V1_19_4To1_19_3::new();
        let out = cb(&mut t, new::CONTAINER_SET_SLOT, slot(new_id));
        assert_eq!((out[0].id, &out[0].payload), (18, &slot(old_id)));
        let creative = |id: i32| {
            let mut w = Writer::new();
            w.i16(36).bool(true).varint(id).i8(1).u8(0);
            w.into_inner()
        };
        let out = sb(&mut t, old::S_SET_CREATIVE_MODE_SLOT, creative(old_id));
        assert_eq!((out[0].id, &out[0].payload), (43, &creative(new_id)));
        let mut out = Vec::new();
        assert!(t.serverbound(State::Play, &Packet::new(old::S_CONTAINER_CLICK, vec![0, 0]), &mut out).is_err());
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
        /// Bytes that look like packet fields: small numbers, flags, long varints, and noise.
        fn byte(&mut self) -> u8 {
            match self.below(10) {
                0..=3 => self.below(4) as u8,
                4 => 0xff,
                5 => 0x80 | self.below(128) as u8,
                6 => self.below(64) as u8,
                7 => 0x7f,
                _ => self.next() as u8,
            }
        }
        fn payload(&mut self) -> Vec<u8> {
            let len = match self.below(8) {
                0 => 0,
                1..=4 => self.below(24),
                5 | 6 => self.below(200),
                _ => self.below(3000),
            };
            (0..len).map(|_| self.byte()).collect()
        }
    }

    /// A player controls serverbound bytes, and a broken or hostile server the clientbound
    /// ones: whatever arrives, the step answers with packets or an error, never a panic, a hang
    /// or a huge allocation.
    #[test]
    fn random_packets_never_panic() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ 761);
        let mut t = V1_19_4To1_19_3::new();
        for i in 0..60_000u32 {
            let clientbound = i % 2 == 0;
            let id = if rng.below(20) == 0 { rng.next() as i32 } else { rng.below(if clientbound { 112 } else { 52 }) as i32 };
            let pkt = Packet::new(id, rng.payload());
            let shown = format!("{} packet {id:#x} {:?} (iteration {i})", if clientbound { "clientbound" } else { "serverbound" }, pkt.payload);
            let started = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut out = Vec::new();
                let r = if clientbound { t.clientbound(State::Play, &pkt, &mut out) } else { t.serverbound(State::Play, &pkt, &mut out) };
                r.map(|()| out)
            }));
            match result {
                Err(_) => panic!("translator panicked on {shown}"),
                Ok(Ok(out)) => {
                    let bytes: usize = out.iter().map(|p| p.payload.len()).sum();
                    assert!(bytes < 1 << 20, "{bytes} bytes produced for {shown}");
                }
                // a refused packet ends that connection; the next one starts fresh
                Ok(Err(_)) => t = V1_19_4To1_19_3::new(),
            }
            t.take_replies();
            assert!(started.elapsed().as_millis() < 2000, "took {:?} on {shown}", started.elapsed());
        }
    }

    /// Valid packets cut short at every length must be errors or shorter valid packets, never
    /// a panic.
    #[test]
    fn truncated_packets_never_panic() {
        let mut login = Writer::new();
        login.i32(1).bool(false).u8(0).i8(-1).varint(1).string("minecraft:overworld");
        crate::proto::nbt::write_named(&mut login, &crate::proto::nbt::Tag::Compound(vec![("a".into(), crate::proto::nbt::Tag::Int(1))]));
        login.string("minecraft:overworld");
        let mut data = Writer::new();
        data.varint(5).u8(7).varint(7).bool(true).varint(3).i8(1).u8(0).u8(17).varint(17).varint(2).varint(9).u8(0xff);
        let mut tags = Writer::new();
        tags.varint(1).string("minecraft:item").varint(1).string("a").varint(2).varint(1).varint(2);
        for (id, full) in [(new::LOGIN, login.buf), (new::SET_ENTITY_DATA, data.buf), (new::UPDATE_TAGS, tags.buf), (new::ADD_ENTITY, spawn(1, 2))] {
            for cut in 0..=full.len() {
                let mut t = V1_19_4To1_19_3::new();
                let mut out = Vec::new();
                let _ = t.clientbound(State::Play, &Packet::new(id, full[..cut].to_vec()), &mut out);
            }
        }
    }
}

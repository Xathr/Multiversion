//! 1.21 / 1.21.1 (protocol 767) ↔ 1.20.5 / 1.20.6 (protocol 766).
//!
//! Packet layout differences follow the vanilla protocol changes between these versions; registry
//! id tables are generated from the game's registry mapping data (see `tools/gen_mappings.py`).
//! Block states, blocks, entities, particles and menus are identical, so chunks and most world
//! packets pass through. What changed: item ids (three new music discs), sounds, attributes, the
//! enchantment / painting / jukebox song registries becoming data-driven, chat types becoming
//! holders, attribute modifiers being named by id instead of UUID, and USE_ITEM carrying the
//! player's rotation.

pub mod entity;
pub mod ids;
pub mod items;
pub mod misc;
pub mod world;

use super::{data_1_20_5, Packet, State, TResult, Translator};
use crate::proto::version::Version;
use ids::{new, old};
use items::{Dir, Items};
use misc::Misc;
use world::{RegistryData, Registries};

pub struct V1_21To1_20_5 {
    to_client: Items,
    to_server: Items,
    registries: Registries,
    misc: Misc,
}

impl V1_21To1_20_5 {
    pub fn new() -> Self {
        Self { to_client: Items::new(Dir::ToClient), to_server: Items::new(Dir::ToServer), registries: Registries::default(), misc: Misc::new() }
    }

    fn play_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let items = &self.to_client;
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            // 1.20.5 takes the projectile power per axis, which one 1.21 value cannot be split back
            // into; the other two packets are new in 1.21
            new::PROJECTILE_POWER | new::CUSTOM_REPORT_DETAILS | new::SERVER_LINKS => return Ok(()),
            new::SET_ENTITY_DATA => entity::set_entity_data(items, &self.registries, p).map_err(e)?,
            new::SET_EQUIPMENT => entity::set_equipment(items, p).map_err(e)?,
            new::LEVEL_EVENT => match self.registries.level_event(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::UPDATE_TAGS => self.registries.update_tags(items, p).map_err(e)?,
            new::LEVEL_PARTICLES => self.misc.level_particles(items, p).map_err(e)?,
            new::SOUND | new::SOUND_ENTITY => self.misc.sound(items, p).map_err(e)?,
            new::EXPLODE => self.misc.explode(items, p).map_err(e)?,
            new::OPEN_SCREEN => self.misc.open_screen(p).map_err(e)?,
            new::CONTAINER_CLOSE => {
                self.misc.container_close();
                p.to_vec()
            }
            new::CONTAINER_SET_DATA => self.misc.container_set_data(items, p).map_err(e)?,
            new::HORSE_SCREEN_OPEN => self.misc.horse_screen_open(p).map_err(e)?,
            new::CONTAINER_SET_CONTENT => self.misc.container_set_content(items, p).map_err(e)?,
            new::CONTAINER_SET_SLOT => self.misc.container_set_slot(items, p).map_err(e)?,
            new::MERCHANT_OFFERS => self.misc.merchant_offers(items, p).map_err(e)?,
            new::COOLDOWN => self.misc.cooldown(items, p).map_err(e)?,
            new::AWARD_STATS => self.misc.award_stats(items, p).map_err(e)?,
            new::UPDATE_ADVANCEMENTS => self.misc.update_advancements(items, p).map_err(e)?,
            new::UPDATE_RECIPES => self.misc.update_recipes(items, p).map_err(e)?,
            new::UPDATE_ATTRIBUTES => self.misc.update_attributes(items, p).map_err(e)?,
            new::GAME_EVENT => self.misc.game_event(p).map_err(e)?,
            new::PLAYER_POSITION => {
                self.misc.player_position(p).map_err(e)?;
                p.to_vec()
            }
            new::DISGUISED_CHAT => self.misc.disguised_chat(p).map_err(e)?,
            new::PLAYER_CHAT => self.misc.player_chat(p).map_err(e)?,
            new::SYSTEM_CHAT | new::SET_ACTION_BAR_TEXT | new::SET_TITLE_TEXT | new::SET_SUBTITLE_TEXT => self.misc.leading_text(p).map_err(e)?,
            _ => p.to_vec(),
        };
        let mapped = ids::play_clientbound(pkt.id).ok_or_else(|| format!("no 1.20.5 equivalent for clientbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }

    fn config_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let e = |e: crate::proto::buf::DecodeError| format!("config 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            new::CONFIG_CUSTOM_REPORT_DETAILS | new::CONFIG_SERVER_LINKS => return Ok(()),
            new::CONFIG_REGISTRY_DATA => match self.registries.registry_data(&pkt.payload).map_err(e)? {
                RegistryData::Keep => pkt.payload.clone(),
                RegistryData::Dropped(enchantments_changed) => {
                    if enchantments_changed {
                        self.to_client.set_enchantments(&self.registries.enchantments);
                        self.to_server.set_enchantments(&self.registries.enchantments);
                    }
                    return Ok(());
                }
            },
            new::CONFIG_UPDATE_TAGS => self.registries.update_tags(&self.to_client, &pkt.payload).map_err(e)?,
            new::CONFIG_UPDATE_ENABLED_FEATURES => world::enabled_features(&pkt.payload).map_err(e)?,
            _ => pkt.payload.clone(),
        };
        let mapped = ids::config_clientbound(pkt.id).ok_or_else(|| format!("no 1.20.5 equivalent for clientbound config packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }

    fn play_serverbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let e = |e: crate::proto::buf::DecodeError| format!("serverbound 0x{:02x}: {e}", pkt.id);
        let items = &self.to_server;
        let p = &pkt.payload[..];
        // serverbound play ids are identical
        let payload = match pkt.id {
            old::S_CONTAINER_CLICK => self.misc.container_click(items, p).map_err(e)?,
            old::S_SET_CREATIVE_MODE_SLOT => self.misc.set_creative_slot(items, p).map_err(e)?,
            old::S_USE_ITEM => self.misc.use_item(p),
            old::S_MOVE_PLAYER_POS_ROT | old::S_MOVE_PLAYER_ROT => {
                self.misc.track_rotation(p, if pkt.id == old::S_MOVE_PLAYER_ROT { 0 } else { 24 }).map_err(e)?;
                p.to_vec()
            }
            _ => p.to_vec(),
        };
        out.push(Packet::new(pkt.id, payload));
        Ok(())
    }
}

impl Default for V1_21To1_20_5 {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator for V1_21To1_20_5 {
    fn from(&self) -> Version {
        Version { protocol: 767 }
    }
    fn to(&self) -> Version {
        Version { protocol: 766 }
    }
    fn clientbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_clientbound(pkt, out),
            State::Configuration => self.config_clientbound(pkt, out),
        }
    }
    fn serverbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_serverbound(pkt, out),
            State::Configuration => {
                // configuration serverbound packets are identical
                out.push(pkt.clone());
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::buf::{Reader, Writer};

    #[test]
    fn new_packets_are_dropped() {
        let mut t = V1_21To1_20_5::new();
        let mut out = Vec::new();
        for id in [new::PROJECTILE_POWER, new::CUSTOM_REPORT_DETAILS, new::SERVER_LINKS] {
            t.clientbound(State::Play, &Packet::new(id, vec![0]), &mut out).unwrap();
        }
        for id in [new::CONFIG_CUSTOM_REPORT_DETAILS, new::CONFIG_SERVER_LINKS] {
            t.clientbound(State::Configuration, &Packet::new(id, vec![0]), &mut out).unwrap();
        }
        assert!(out.is_empty());
        // everything else keeps its id
        t.clientbound(State::Play, &Packet::new(0x26, vec![1, 2, 3]), &mut out).unwrap();
        assert_eq!((out[0].id, out[0].payload.clone()), (0x26, vec![1, 2, 3]));
    }

    #[test]
    fn enchantment_registry_is_dropped_and_used_for_items() {
        let mut t = V1_21To1_20_5::new();
        let mut p = Writer::new();
        p.string("minecraft:enchantment");
        p.varint(2);
        p.string("minecraft:unbreaking").bool(false);
        p.string("minecraft:mending").bool(false);
        let mut out = Vec::new();
        t.clientbound(State::Configuration, &Packet::new(new::CONFIG_REGISTRY_DATA, p.buf), &mut out).unwrap();
        assert!(out.is_empty());
        let old = |n: &str| data_1_20_5::OLD_ENCHANTMENT_NAMES.iter().position(|x| *x == n).unwrap() as i32;
        assert_eq!(t.to_client.enchantment(1), Some(old("mending")));
        assert_eq!(t.to_server.enchantment(old("unbreaking")), Some(0));
        // other registries pass
        let mut p = Writer::new();
        p.string("minecraft:dimension_type");
        p.varint(0);
        t.clientbound(State::Configuration, &Packet::new(new::CONFIG_REGISTRY_DATA, p.buf.clone()), &mut out).unwrap();
        assert_eq!(out[0].payload, p.buf);
    }

    #[test]
    fn use_item_gets_rotation() {
        let mut t = V1_21To1_20_5::new();
        let mut mv = Writer::new();
        mv.f32(45.0).f32(30.0).bool(true);
        let mut out = Vec::new();
        t.serverbound(State::Play, &Packet::new(old::S_MOVE_PLAYER_ROT, mv.buf.clone()), &mut out).unwrap();
        assert_eq!(out[0].payload, mv.buf);
        let mut u = Writer::new();
        u.varint(1).varint(3);
        t.serverbound(State::Play, &Packet::new(old::S_USE_ITEM, u.buf), &mut out).unwrap();
        assert_eq!(out[1].id, old::S_USE_ITEM);
        let mut r = Reader::new(&out[1].payload);
        assert_eq!((r.varint().unwrap(), r.varint().unwrap(), r.f32().unwrap(), r.f32().unwrap()), (1, 3, 45.0, 30.0));
    }
}

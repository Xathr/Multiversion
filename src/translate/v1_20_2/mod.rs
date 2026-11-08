//! 1.20.3 / 1.20.4 (protocol 765) ↔ 1.20.2 (protocol 764).
//!
//! Packet layout differences follow the vanilla protocol changes between these versions; registry
//! id tables are generated from the game's registry mapping data (see `tools/gen_mappings.py`).
//! The big change: 1.20.3 sends text components as NBT, 1.20.2 as JSON strings. Besides that
//! 1.20.3 added scoreboard number formats and reset_score, resource pack ids (push/pop), tick
//! rate packets, explosion interaction / particle / sound fields, the "chunks loading" game
//! event, the style command argument, and new blocks, items, sounds and entities (crafter, tuff
//! and copper variants, trial spawner, breeze, wind charge).

pub mod entity;
pub mod ids;
pub mod items;
pub mod misc;
pub mod text;
pub mod world;

use super::{data_1_20_2, Packet, State, TResult, Translator};
use crate::proto::version::Version;
use entity::Entities;
use ids::{new, old};
use misc::Misc;

pub struct V1_20_3To1_20_2 {
    entities: Entities,
    misc: Misc,
}

impl V1_20_3To1_20_2 {
    pub fn new() -> Self {
        Self { entities: Entities::default(), misc: Misc::new() }
    }

    fn play_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            // tick rate control and resource pack removal have no 1.20.2 counterpart
            new::TICKING_STATE | new::TICKING_STEP | new::RESOURCE_PACK_POP => return Ok(()),
            new::RESET_SCORE => {
                out.push(Packet::new(old::SET_SCORE, misc::reset_score(p).map_err(e)?));
                return Ok(());
            }
            new::GAME_EVENT => {
                if let Some((id, spawn)) = self.misc.game_event(p).map_err(e)? {
                    out.push(Packet::new(id, spawn));
                    return Ok(());
                }
                p.to_vec()
            }
            new::LOGIN => {
                self.entities.clear();
                self.misc.login(p).map_err(e)?;
                p.to_vec()
            }
            new::RESPAWN => {
                self.entities.clear();
                self.misc.respawn(p).map_err(e)?;
                p.to_vec()
            }
            new::SET_DEFAULT_SPAWN_POSITION => {
                self.misc.set_default_spawn_position(p).map_err(e)?;
                p.to_vec()
            }
            new::ADD_ENTITY => self.entities.add_entity(p).map_err(e)?,
            new::REMOVE_ENTITIES => {
                self.entities.remove_entities(p).map_err(e)?;
                p.to_vec()
            }
            new::SET_ENTITY_DATA => self.entities.set_entity_data(p).map_err(e)?,
            new::SET_EQUIPMENT => entity::set_equipment(p).map_err(e)?,
            new::LEVEL_CHUNK_WITH_LIGHT => world::level_chunk_with_light(p).map_err(e)?,
            new::BLOCK_UPDATE => world::block_update(p).map_err(e)?,
            new::SECTION_BLOCKS_UPDATE => world::section_blocks_update(p).map_err(e)?,
            new::BLOCK_EVENT => world::block_event(p).map_err(e)?,
            new::BLOCK_ENTITY_DATA => match world::block_entity_data(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::LEVEL_EVENT => match world::level_event(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::UPDATE_TAGS => world::update_tags(p).map_err(e)?,
            new::LEVEL_PARTICLES => misc::level_particles(p).map_err(e)?,
            new::SOUND | new::SOUND_ENTITY => match misc::sound(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::EXPLODE => misc::explode(p).map_err(e)?,
            new::OPEN_SCREEN => match misc::open_screen(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::CONTAINER_SET_CONTENT => misc::container_set_content(p).map_err(e)?,
            new::CONTAINER_SET_SLOT => misc::container_set_slot(p).map_err(e)?,
            new::MERCHANT_OFFERS => misc::merchant_offers(p).map_err(e)?,
            new::COOLDOWN => misc::cooldown(p).map_err(e)?,
            new::AWARD_STATS => misc::award_stats(p).map_err(e)?,
            new::UPDATE_ADVANCEMENTS => misc::update_advancements(p).map_err(e)?,
            new::UPDATE_RECIPES => misc::update_recipes(p).map_err(e)?,
            new::COMMANDS => misc::commands(p).map_err(e)?,
            new::COMMAND_SUGGESTIONS => misc::command_suggestions(p).map_err(e)?,
            new::SYSTEM_CHAT | new::SET_ACTION_BAR_TEXT | new::SET_TITLE_TEXT | new::SET_SUBTITLE_TEXT | new::DISCONNECT | new::SERVER_DATA => {
                misc::leading_components(p, 1).map_err(e)?
            }
            new::TAB_LIST => misc::leading_components(p, 2).map_err(e)?,
            new::DISGUISED_CHAT => misc::disguised_chat(p).map_err(e)?,
            new::PLAYER_CHAT => misc::player_chat(p).map_err(e)?,
            new::BOSS_EVENT => misc::boss_event(p).map_err(e)?,
            new::PLAYER_COMBAT_KILL => misc::player_combat_kill(p).map_err(e)?,
            new::PLAYER_INFO_UPDATE => misc::player_info_update(p).map_err(e)?,
            new::SET_PLAYER_TEAM => misc::set_player_team(p).map_err(e)?,
            new::MAP_ITEM_DATA => misc::map_item_data(p).map_err(e)?,
            new::SET_OBJECTIVE => misc::set_objective(p).map_err(e)?,
            new::SET_SCORE => misc::set_score(p).map_err(e)?,
            new::RESOURCE_PACK_PUSH => self.misc.resource_pack_push(p).map_err(e)?,
            _ => p.to_vec(),
        };
        let mapped = ids::play_clientbound(pkt.id).ok_or_else(|| format!("no 1.20.2 equivalent for clientbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }

    fn config_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("config 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            new::CONFIG_RESOURCE_PACK_POP => return Ok(()),
            new::CONFIG_RESOURCE_PACK_PUSH => self.misc.resource_pack_push(p).map_err(e)?,
            new::CONFIG_DISCONNECT => misc::leading_components(p, 1).map_err(e)?,
            new::CONFIG_UPDATE_TAGS => world::update_tags(p).map_err(e)?,
            new::CONFIG_UPDATE_ENABLED_FEATURES => world::enabled_features(p).map_err(e)?,
            _ => p.to_vec(),
        };
        let mapped = ids::config_clientbound(pkt.id).ok_or_else(|| format!("no 1.20.2 equivalent for clientbound config packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }

    fn play_serverbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("serverbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            old::S_CONTAINER_CLICK => misc::container_click(p).map_err(e)?,
            old::S_SET_CREATIVE_MODE_SLOT => misc::set_creative_slot(p).map_err(e)?,
            old::S_RESOURCE_PACK => self.misc.resource_pack_status(p),
            old::S_SET_JIGSAW_BLOCK => misc::set_jigsaw_block(p),
            _ => p.to_vec(),
        };
        let mapped = ids::play_serverbound(pkt.id).ok_or_else(|| format!("no 1.20.3 equivalent for serverbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }
}

impl Default for V1_20_3To1_20_2 {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator for V1_20_3To1_20_2 {
    fn from(&self) -> Version {
        Version { protocol: 765 }
    }
    fn to(&self) -> Version {
        Version { protocol: 764 }
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
                // serverbound configuration ids are identical; only the pack answer changed
                let payload = if pkt.id == old::S_CONFIG_RESOURCE_PACK { self.misc.resource_pack_status(&pkt.payload) } else { pkt.payload.clone() };
                out.push(Packet::new(pkt.id, payload));
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::buf::{Reader, Writer};
    use crate::proto::nbt::{self, Tag};

    fn text(v: &str) -> Tag {
        Tag::Compound(vec![("text".into(), Tag::String(v.into()))])
    }

    #[test]
    fn new_packets_are_dropped() {
        let mut t = V1_20_3To1_20_2::new();
        let mut out = Vec::new();
        t.clientbound(State::Play, &Packet::new(new::TICKING_STATE, vec![0, 0, 0xA0, 0x41, 0]), &mut out).unwrap();
        t.clientbound(State::Play, &Packet::new(new::TICKING_STEP, vec![5]), &mut out).unwrap();
        t.clientbound(State::Play, &Packet::new(new::RESOURCE_PACK_POP, vec![0]), &mut out).unwrap();
        t.clientbound(State::Configuration, &Packet::new(new::CONFIG_RESOURCE_PACK_POP, vec![0]), &mut out).unwrap();
        assert!(out.is_empty());
        // everything else passes with its 1.20.2 id
        t.clientbound(State::Play, &Packet::new(0x6B, vec![1, 2, 3]), &mut out).unwrap();
        assert_eq!((out[0].id, out[0].payload.clone()), (0x69, vec![1, 2, 3]));
        t.clientbound(State::Configuration, &Packet::new(0x05, vec![4]), &mut out).unwrap();
        assert_eq!(out[1].id, 0x05);
    }

    #[test]
    fn disconnect_in_both_states() {
        let mut w = Writer::new();
        nbt::write(&mut w, &text("Bye"));
        let mut t = V1_20_3To1_20_2::new();
        let mut out = Vec::new();
        t.clientbound(State::Play, &Packet::new(new::DISCONNECT, w.buf.clone()), &mut out).unwrap();
        t.clientbound(State::Configuration, &Packet::new(new::CONFIG_DISCONNECT, w.buf), &mut out).unwrap();
        for (p, id) in out.iter().zip([0x1B, 0x01]) {
            assert_eq!(p.id, id);
            let v: serde_json::Value = serde_json::from_str(Reader::new(&p.payload).string().unwrap()).unwrap();
            assert_eq!(v, serde_json::json!({"text": "Bye"}));
        }
    }

    #[test]
    fn reset_score_becomes_set_score() {
        let mut w = Writer::new();
        w.string("Steve").bool(false);
        let mut t = V1_20_3To1_20_2::new();
        let mut out = Vec::new();
        t.clientbound(State::Play, &Packet::new(new::RESET_SCORE, w.buf), &mut out).unwrap();
        assert_eq!(out[0].id, old::SET_SCORE);
    }

    #[test]
    fn server_data_motd() {
        let mut w = Writer::new();
        nbt::write(&mut w, &Tag::String("A Pumpkin server".into()));
        w.bool(false).bool(true);
        let mut t = V1_20_3To1_20_2::new();
        let mut out = Vec::new();
        t.clientbound(State::Play, &Packet::new(new::SERVER_DATA, w.buf), &mut out).unwrap();
        assert_eq!(out[0].id, 0x47);
        let mut r = Reader::new(&out[0].payload);
        assert_eq!(r.string().unwrap(), r#""A Pumpkin server""#);
        assert_eq!((r.bool().unwrap(), r.bool().unwrap()), (false, true));
    }

    #[test]
    fn serverbound_ids_and_resource_pack_answers() {
        let mut t = V1_20_3To1_20_2::new();
        let mut push = Writer::new();
        push.uuid(77).string("u").string("h").bool(false).bool(false);
        let mut out = Vec::new();
        t.clientbound(State::Configuration, &Packet::new(new::CONFIG_RESOURCE_PACK_PUSH, push.buf), &mut out).unwrap();
        assert_eq!(out[0].id, old::CONFIG_RESOURCE_PACK);
        let mut back = Vec::new();
        t.serverbound(State::Configuration, &Packet::new(old::S_CONFIG_RESOURCE_PACK, vec![0]), &mut back).unwrap();
        t.serverbound(State::Play, &Packet::new(old::S_RESOURCE_PACK, vec![3]), &mut back).unwrap();
        t.serverbound(State::Play, &Packet::new(old::S_CUSTOM_PAYLOAD, vec![9]), &mut back).unwrap();
        t.serverbound(State::Play, &Packet::new(0x00, vec![1]), &mut back).unwrap();
        assert_eq!(back[0].id, old::S_CONFIG_RESOURCE_PACK);
        assert_eq!(Reader::new(&back[0].payload).uuid().unwrap(), 77);
        assert_eq!(back[1].id, new::S_RESOURCE_PACK);
        let mut r = Reader::new(&back[1].payload);
        assert_eq!((r.uuid().unwrap(), r.varint().unwrap()), (77, 3));
        assert_eq!((back[2].id, back[2].payload.clone()), (0x10, vec![9]));
        assert_eq!(back[3].id, 0x00);
    }

    #[test]
    fn step_versions() {
        let t = V1_20_3To1_20_2::new();
        assert_eq!((t.from().protocol, t.to().protocol), (765, 764));
    }
}

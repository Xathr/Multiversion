//! 1.20.5 / 1.20.6 (protocol 766) ↔ 1.20.3 / 1.20.4 (protocol 765).
//!
//! Packet layout differences follow the vanilla protocol changes between these versions; registry
//! id tables are generated from the game's registry mapping data (see `tools/gen_mappings.py`).
//! The big change is items: 1.20.5 replaced item NBT with data components (see `items`). Besides
//! that: registry data is sent as one codec, the known-packs handshake and cookies are answered
//! here for the client, a few blocks, items, entities, particles and sounds are new, particles
//! changed their encoding, entity data gained value types, attributes are named by id, the body
//! armor slot is new, and commands are split into signed and unsigned ones.

pub mod entity;
pub mod ids;
pub mod items;
pub mod misc;
pub mod text;
pub mod world;

use std::collections::HashMap;

use super::{data_1_20_3, Packet, State, TResult, Translator};
use crate::proto::buf::{Reader, Writer};
use crate::proto::version::Version;
use entity::Entities;
use ids::{new, old};
use items::Items;
use misc::Misc;
use world::{Recorded, Registries};

/// Most a server may store per cookie (as vanilla).
const MAX_COOKIE: usize = 5120;

pub struct V1_20_5To1_20_3 {
    items: Items,
    registries: Registries,
    entities: Entities,
    misc: Misc,
    /// Cookies the server stored; 1.20.3 clients cannot keep them, so they live here.
    cookies: HashMap<String, Vec<u8>>,
    /// Answers to the server produced while handling clientbound packets (1.20.5 ids).
    /// Answers to the server on the client's behalf, each with the state it is sent in.
    replies: Vec<(State, Packet)>,
}

impl V1_20_5To1_20_3 {
    pub fn new() -> Self {
        Self {
            items: Items::new(),
            registries: Registries::default(),
            entities: Entities::default(),
            misc: Misc::new(),
            cookies: HashMap::new(),
            replies: Vec::new(),
        }
    }

    /// STORE_COOKIE: kept for this connection.
    fn store_cookie(&mut self, payload: &[u8]) -> TResult {
        let mut r = Reader::new(payload);
        let key = r.string().map_err(|e| e.to_string())?.to_string();
        let data = r.byte_array().map_err(|e| e.to_string())?;
        if data.len() > MAX_COOKIE {
            return Err("cookie too large".into());
        }
        self.cookies.insert(key, data.to_vec());
        Ok(())
    }

    /// COOKIE_REQUEST: answered with what the server stored, if anything.
    fn answer_cookie(&mut self, payload: &[u8], response_id: i32, state: State) -> TResult {
        let mut r = Reader::new(payload);
        let key = r.string().map_err(|e| e.to_string())?;
        let mut w = Writer::new();
        w.string(key);
        w.option(self.cookies.get(key), |w, data| {
            w.byte_array(data);
        });
        self.replies.push((state, Packet::new(response_id, w.into_inner())));
        Ok(())
    }

    fn play_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let items = &self.items;
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        let mut produced: Vec<(i32, Vec<u8>)> = Vec::new();
        let payload = match pkt.id {
            // nothing on 1.20.3 for these (the transfer cannot be followed)
            new::PROJECTILE_POWER | new::DEBUG_SAMPLE | new::TRANSFER => return Ok(()),
            new::STORE_COOKIE => return self.store_cookie(p),
            new::COOKIE_REQUEST => return self.answer_cookie(p, new::S_COOKIE_RESPONSE, State::Play),
            new::ADD_ENTITY => {
                self.entities.add_entity(p, &mut produced).map_err(e)?;
                None
            }
            new::REMOVE_ENTITIES => {
                self.entities.remove_entities(p).map_err(e)?;
                Some(p.to_vec())
            }
            new::SET_ENTITY_DATA => Some(self.entities.set_entity_data(items, p).map_err(e)?),
            new::SET_EQUIPMENT => {
                self.entities.set_equipment(items, p, &mut produced).map_err(e)?;
                None
            }
            new::UPDATE_ATTRIBUTES => Some(self.entities.update_attributes(p).map_err(e)?),
            new::UPDATE_MOB_EFFECT => match self.entities.update_mob_effect(p).map_err(e)? {
                Some(v) => Some(v),
                None => return Ok(()),
            },
            new::REMOVE_MOB_EFFECT => match Entities::remove_mob_effect(p).map_err(e)? {
                Some(v) => Some(v),
                None => return Ok(()),
            },
            new::LOGIN => Some(self.entities.login(&self.registries, p).map_err(e)?),
            new::RESPAWN => Some(self.entities.respawn(&self.registries, p).map_err(e)?),
            new::SERVER_DATA => Some(self.entities.server_data(p).map_err(e)?),
            new::START_CONFIGURATION => {
                self.registries.clear();
                Some(p.to_vec())
            }
            new::LEVEL_CHUNK_WITH_LIGHT => Some(world::level_chunk_with_light(p).map_err(e)?),
            new::BLOCK_UPDATE => Some(world::block_update(p).map_err(e)?),
            new::SECTION_BLOCKS_UPDATE => Some(world::section_blocks_update(p).map_err(e)?),
            new::BLOCK_EVENT => match world::block_event(p).map_err(e)? {
                Some(v) => Some(v),
                None => return Ok(()),
            },
            new::LEVEL_EVENT => Some(world::level_event(p).map_err(e)?),
            new::BLOCK_ENTITY_DATA => match world::block_entity_data(p).map_err(e)? {
                Some(v) => Some(v),
                None => return Ok(()),
            },
            new::UPDATE_TAGS => Some(world::update_tags(items, p).map_err(e)?),
            new::LEVEL_PARTICLES => Some(self.misc.level_particles(items, p).map_err(e)?),
            new::SOUND | new::SOUND_ENTITY => match self.misc.sound(p).map_err(e)? {
                Some(v) => Some(v),
                None => return Ok(()),
            },
            new::EXPLODE => Some(self.misc.explode(items, p).map_err(e)?),
            new::OPEN_SCREEN => Some(self.misc.open_screen(p).map_err(e)?),
            new::CONTAINER_CLOSE => {
                self.misc.container_close();
                Some(p.to_vec())
            }
            new::CONTAINER_SET_DATA => Some(self.misc.container_set_data(p).map_err(e)?),
            new::HORSE_SCREEN_OPEN => Some(self.misc.horse_screen_open(p).map_err(e)?),
            new::CONTAINER_SET_CONTENT => Some(self.misc.container_set_content(items, p).map_err(e)?),
            new::CONTAINER_SET_SLOT => Some(self.misc.container_set_slot(items, p).map_err(e)?),
            new::MERCHANT_OFFERS => Some(self.misc.merchant_offers(items, p).map_err(e)?),
            new::COOLDOWN => Some(self.misc.cooldown(p).map_err(e)?),
            new::AWARD_STATS => Some(self.misc.award_stats(p).map_err(e)?),
            new::MAP_ITEM_DATA => Some(self.misc.map_item_data(p).map_err(e)?),
            new::UPDATE_ADVANCEMENTS => Some(self.misc.update_advancements(items, p).map_err(e)?),
            new::UPDATE_RECIPES => Some(self.misc.update_recipes(items, p).map_err(e)?),
            new::COMMANDS => Some(self.misc.commands(p).map_err(e)?),
            new::SYSTEM_CHAT | new::SET_ACTION_BAR_TEXT | new::SET_TITLE_TEXT | new::SET_SUBTITLE_TEXT | new::DISGUISED_CHAT => {
                Some(self.misc.leading_text(p).map_err(e)?)
            }
            _ => Some(p.to_vec()),
        };
        if let Some(payload) = payload {
            let mapped = ids::play_clientbound(pkt.id).ok_or_else(|| format!("no 1.20.3 equivalent for clientbound play packet 0x{:02x}", pkt.id))?;
            out.push(Packet::new(mapped, payload));
        }
        out.extend(produced.into_iter().map(|(id, payload)| Packet::new(id, payload)));
        Ok(())
    }

    /// The registry codec, once, before whatever needs the registries.
    fn send_codec(&mut self, out: &mut Vec<Packet>) {
        if let Some(codec) = self.registries.take_codec() {
            out.push(Packet::new(old::CONFIG_REGISTRY_DATA, codec));
        }
    }

    fn config_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let e = |e: crate::proto::buf::DecodeError| format!("config 0x{:02x}: {e}", pkt.id);
        let p = &pkt.payload[..];
        let payload = match pkt.id {
            // 1.20.3 clients reset the chat themselves when configuration starts
            new::CONFIG_RESET_CHAT | new::CONFIG_TRANSFER => return Ok(()),
            new::CONFIG_STORE_COOKIE => return self.store_cookie(p),
            new::CONFIG_COOKIE_REQUEST => return self.answer_cookie(p, new::S_CONFIG_COOKIE_RESPONSE, State::Configuration),
            new::CONFIG_SELECT_KNOWN_PACKS => {
                // the client knows no packs to share: the server then sends full registry data
                self.replies.push((State::Configuration, Packet::new(new::S_CONFIG_SELECT_KNOWN_PACKS, vec![0])));
                return Ok(());
            }
            new::CONFIG_REGISTRY_DATA => {
                match self.registries.registry_data(p).map_err(e)? {
                    Recorded::BannerPatterns(keys) => self.items.banner_patterns = keys,
                    Recorded::TrimPatterns(keys) => self.items.trim_patterns = keys,
                    Recorded::TrimMaterials(keys) => self.items.trim_materials = keys,
                    Recorded::None => {}
                }
                return Ok(());
            }
            new::CONFIG_UPDATE_TAGS => {
                self.send_codec(out);
                world::update_tags(&self.items, p).map_err(e)?
            }
            new::CONFIG_FINISH => {
                self.send_codec(out);
                p.to_vec()
            }
            _ => p.to_vec(),
        };
        let mapped = ids::config_clientbound(pkt.id).ok_or_else(|| format!("no 1.20.3 equivalent for clientbound config packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }

    fn play_serverbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let e = |e: crate::proto::buf::DecodeError| format!("serverbound 0x{:02x}: {e}", pkt.id);
        let p = &pkt.payload[..];
        let payload = match pkt.id {
            old::S_CONTAINER_CLICK => self.misc.container_click(&self.items, p).map_err(e)?,
            old::S_SET_CREATIVE_MODE_SLOT => self.misc.set_creative_slot(&self.items, p).map_err(e)?,
            old::S_CONTAINER_BUTTON_CLICK => self.misc.container_button_click(p).map_err(e)?,
            _ => p.to_vec(),
        };
        let mapped = ids::play_serverbound(pkt.id).ok_or_else(|| format!("unknown 1.20.3 serverbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }
}

impl Default for V1_20_5To1_20_3 {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator for V1_20_5To1_20_3 {
    fn from(&self) -> Version {
        Version { protocol: 766 }
    }
    fn to(&self) -> Version {
        Version { protocol: 765 }
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
                let mapped = ids::config_serverbound(pkt.id).ok_or_else(|| format!("unknown 1.20.3 serverbound config packet 0x{:02x}", pkt.id))?;
                out.push(Packet::new(mapped, pkt.payload.clone()));
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
    use super::*;
    use crate::proto::nbt;

    #[test]
    fn known_packs_and_cookies_are_answered_for_the_client() {
        let mut t = V1_20_5To1_20_3::new();
        let mut out = Vec::new();
        let mut packs = Writer::new();
        packs.varint(1).string("minecraft").string("core").string("1.20.6");
        t.clientbound(State::Configuration, &Packet::new(new::CONFIG_SELECT_KNOWN_PACKS, packs.buf), &mut out).unwrap();
        assert!(out.is_empty());
        let replies = t.take_replies();
        assert_eq!(replies.len(), 1);
        assert_eq!((replies[0].0, replies[0].1.id, replies[0].1.payload.clone()), (State::Configuration, new::S_CONFIG_SELECT_KNOWN_PACKS, vec![0]));
        assert!(t.take_replies().is_empty());

        let mut store = Writer::new();
        store.string("minecraft:session").byte_array(&[1, 2, 3]);
        t.clientbound(State::Play, &Packet::new(new::STORE_COOKIE, store.buf), &mut out).unwrap();
        let mut req = Writer::new();
        req.string("minecraft:session");
        t.clientbound(State::Play, &Packet::new(new::COOKIE_REQUEST, req.buf.clone()), &mut out).unwrap();
        let mut unknown = Writer::new();
        unknown.string("minecraft:other");
        t.clientbound(State::Configuration, &Packet::new(new::CONFIG_COOKIE_REQUEST, unknown.buf.clone()), &mut out).unwrap();
        assert!(out.is_empty());
        let replies = t.take_replies();
        let mut ok = req.buf.clone();
        ok.extend([1, 3, 1, 2, 3]);
        assert_eq!((replies[0].0, replies[0].1.id, replies[0].1.payload.clone()), (State::Play, new::S_COOKIE_RESPONSE, ok));
        let mut none = unknown.buf.clone();
        none.push(0);
        assert_eq!((replies[1].0, replies[1].1.id, replies[1].1.payload.clone()), (State::Configuration, new::S_CONFIG_COOKIE_RESPONSE, none));
    }

    #[test]
    fn registry_codec_is_sent_before_tags_and_finish() {
        let mut t = V1_20_5To1_20_3::new();
        let mut out = Vec::new();
        let mut reg = Writer::new();
        reg.string("minecraft:damage_type").varint(1).string("minecraft:generic").bool(true);
        nbt::write(&mut reg, &nbt::Tag::compound());
        t.clientbound(State::Configuration, &Packet::new(new::CONFIG_REGISTRY_DATA, reg.buf), &mut out).unwrap();
        assert!(out.is_empty(), "held until the codec is complete");
        let mut tags = Writer::new();
        tags.varint(0);
        t.clientbound(State::Configuration, &Packet::new(new::CONFIG_UPDATE_TAGS, tags.buf), &mut out).unwrap();
        assert_eq!(out.iter().map(|p| p.id).collect::<Vec<_>>(), vec![old::CONFIG_REGISTRY_DATA, old::CONFIG_UPDATE_TAGS]);
        t.clientbound(State::Configuration, &Packet::new(new::CONFIG_FINISH, Vec::new()), &mut out).unwrap();
        assert_eq!(out.last().unwrap().id, old::CONFIG_FINISH);
        assert_eq!(out.len(), 3, "the codec is sent once");
    }

    #[test]
    fn serverbound_ids_and_button_clicks() {
        let mut t = V1_20_5To1_20_3::new();
        let mut out = Vec::new();
        t.serverbound(State::Play, &Packet::new(old::S_CONTAINER_BUTTON_CLICK, vec![3, 200]), &mut out).unwrap();
        assert_eq!((out[0].id, out[0].payload.clone()), (new::S_CONTAINER_BUTTON_CLICK, vec![3, 0xc8, 0x01]));
        let mut cmd = Writer::new();
        cmd.string("say hi").i64(1).i64(0).varint(0).varint(0).raw(&[0, 0, 0]);
        t.serverbound(State::Play, &Packet::new(old::S_CHAT_COMMAND, cmd.buf.clone()), &mut out).unwrap();
        assert_eq!((out[1].id, out[1].payload.clone()), (new::S_CHAT_COMMAND_SIGNED, cmd.buf));
        t.serverbound(State::Configuration, &Packet::new(0x02, Vec::new()), &mut out).unwrap();
        assert_eq!(out[2].id, 0x03, "finish configuration");
    }
}

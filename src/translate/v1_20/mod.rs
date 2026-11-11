//! 1.20.2 (protocol 764) ↔ 1.20 / 1.20.1 (protocol 763).
//!
//! 1.20.1 has no configuration phase: login success takes it straight to play, and the
//! registries arrive inside the play LOGIN packet. The proxy acknowledges login to the server
//! itself (`emulates_configuration`), and this step plays the client's part in configuration
//! (as ViaBackwards does): it answers keep-alives, pings, resource packs and FINISH_CONFIGURATION
//! with packets of its own (`take_replies`), sends the client's settings, and keeps the
//! registries, enabled features, tags and plugin messages for the client. When the server's
//! LOGIN arrives it becomes a 1.20.1 LOGIN carrying the registries, followed by the features,
//! tags and messages. START_CONFIGURATION during play is acknowledged the same way; the LOGIN
//! that ends that configuration resets the client's world like a server switch.
//!
//! Other changes: chunk batches (answered here so the server keeps sending), players spawned with
//! ADD_ENTITY, network NBT without a root name, mob effect ids, the RESPAWN and LOGIN layouts,
//! potion effects in item NBT, and two block properties (barrier waterlogged, heads powered).

pub mod ids;
pub mod items;
pub mod world;

use std::collections::HashMap;

use super::{data_1_20, Packet, State, TResult, Translator};
use crate::proto::buf::{Reader, Writer};
use crate::proto::nbt::{self, Tag};
use crate::proto::version::Version;
use ids::{new, old};
use items::Kind;

/// Chunks per tick asked for after each batch (vanilla clients measure theirs; 1.20.1 can't).
const CHUNKS_PER_TICK: f32 = 20.0;

pub struct V1_20_2To1_20 {
    /// Spawned entities whose entity data needs more than a type-by-type rewrite.
    kinds: HashMap<i32, Kind>,
    /// Packets for the server, sent on the client's behalf.
    replies: Vec<(State, Packet)>,
    /// Registries from configuration, for the next LOGIN.
    registries: Option<Tag>,
    /// From configuration, sent to the client after LOGIN (1.20.1 ids).
    features: Option<Packet>,
    tags: Option<Packet>,
    messages: Vec<Packet>,
    resource_pack: Option<Packet>,
    /// The client's settings (1.20.1 CLIENT_INFORMATION = 1.20.2 layout).
    client_information: Option<Vec<u8>>,
    /// From configuration until LOGIN: the client is still waiting for LOGIN, so play packets
    /// the server sends before it wait here.
    awaiting_login: bool,
    held: Vec<Packet>,
}

impl V1_20_2To1_20 {
    pub fn new() -> Self {
        Self {
            kinds: HashMap::new(),
            replies: Vec::new(),
            registries: None,
            features: None,
            tags: None,
            messages: Vec::new(),
            resource_pack: None,
            client_information: None,
            awaiting_login: false,
            held: Vec::new(),
        }
    }

    fn reply(&mut self, state: State, id: i32, payload: Vec<u8>) {
        self.replies.push((state, Packet::new(id, payload)));
    }

    /// Defaults until the client sends its own (after LOGIN): what a fresh 1.20.1 client has.
    fn default_client_information() -> Vec<u8> {
        let mut w = Writer::new();
        w.string("en_us").i8(12).varint(0).bool(true).u8(0x7f).varint(1).bool(false).bool(true);
        w.into_inner()
    }

    fn start_configuration(&mut self) {
        self.awaiting_login = true;
        self.kinds.clear();
        let info = self.client_information.clone().unwrap_or_else(Self::default_client_information);
        self.reply(State::Configuration, new::S_CONFIG_CLIENT_INFORMATION, info);
    }

    /// The server's configuration packets, answered or kept for after LOGIN.
    fn configuration(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let e = |e: crate::proto::buf::DecodeError| format!("config 0x{:02x}: {e}", pkt.id);
        let p = &pkt.payload;
        match pkt.id {
            new::CONFIG_CUSTOM_PAYLOAD => self.messages.push(Packet::new(old::CUSTOM_PAYLOAD, p.clone())),
            new::CONFIG_DISCONNECT => out.push(Packet::new(old::DISCONNECT, p.clone())),
            new::CONFIG_FINISH => self.reply(State::Configuration, new::S_CONFIG_FINISH, Vec::new()),
            new::CONFIG_KEEP_ALIVE => self.reply(State::Configuration, new::S_CONFIG_KEEP_ALIVE, p.clone()),
            new::CONFIG_PING => self.reply(State::Configuration, new::S_CONFIG_PONG, p.clone()),
            new::CONFIG_REGISTRY_DATA => {
                let mut tag = nbt::read(&mut Reader::new(p)).map_err(e)?;
                canonical_registry_names(&mut tag);
                match (&mut self.registries, tag) {
                    (Some(Tag::Compound(have)), Tag::Compound(entries)) => {
                        for (k, v) in entries {
                            match have.iter_mut().find(|(n, _)| *n == k) {
                                Some(slot) => slot.1 = v,
                                None => have.push((k, v)),
                            }
                        }
                    }
                    (_, tag) => self.registries = Some(tag),
                }
            }
            new::CONFIG_RESOURCE_PACK => {
                // the server waits for the answer; the client gets the pack after LOGIN
                self.resource_pack = Some(Packet::new(old::RESOURCE_PACK, p.clone()));
                for result in [3, 0] {
                    let mut w = Writer::new();
                    w.varint(result); // accepted, then downloaded
                    self.reply(State::Configuration, new::S_CONFIG_RESOURCE_PACK, w.into_inner());
                }
            }
            new::CONFIG_UPDATE_ENABLED_FEATURES => self.features = Some(Packet::new(old::UPDATE_ENABLED_FEATURES, p.clone())),
            new::CONFIG_UPDATE_TAGS => self.tags = Some(Packet::new(old::UPDATE_TAGS, p.clone())),
            _ => {}
        }
        Ok(())
    }

    /// LOGIN with the registries, then what configuration left for the client, then what the
    /// server sent before LOGIN.
    fn login(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let registries = self.registries.as_ref().ok_or("LOGIN before any registry data")?;
        let payload = world::login(&pkt.payload, registries).map_err(|e| format!("clientbound login: {e}"))?;
        out.push(Packet::new(old::LOGIN, payload));
        out.extend(self.features.clone());
        // tags are kept: a later configuration that sends none still needs them after its LOGIN
        out.extend(self.tags.clone());
        out.append(&mut self.messages);
        out.extend(self.resource_pack.take());
        out.append(&mut self.held);
        self.awaiting_login = false;
        self.kinds.clear();
        Ok(())
    }

    fn play_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: crate::proto::buf::DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            new::CHUNK_BATCH_START | new::PONG_RESPONSE => return Ok(()),
            new::CHUNK_BATCH_FINISHED => {
                let mut w = Writer::new();
                w.f32(CHUNKS_PER_TICK);
                self.reply(State::Play, new::S_CHUNK_BATCH_RECEIVED, w.into_inner());
                return Ok(());
            }
            new::ADD_ENTITY => {
                let spawn = world::add_entity(p).map_err(e)?;
                match items::entity_kind(spawn.ty) {
                    Some(k) => self.kinds.insert(spawn.entity, k),
                    None => self.kinds.remove(&spawn.entity),
                };
                out.extend(spawn.packets.into_iter().map(|(id, payload)| Packet::new(id, payload)));
                return Ok(());
            }
            new::REMOVE_ENTITIES => {
                let mut r = Reader::new(p);
                for id in r.list(Reader::varint).map_err(e)? {
                    self.kinds.remove(&id);
                }
                p.to_vec()
            }
            new::RESPAWN => {
                self.kinds.clear();
                world::respawn(p).map_err(e)?
            }
            new::SET_ENTITY_DATA => {
                let mut r = Reader::new(p);
                let entity = r.varint().map_err(e)?;
                let mut w = Writer::with_capacity(p.len() + 8);
                w.varint(entity);
                items::entity_data(self.kinds.get(&entity).copied(), &mut r, &mut w).map_err(e)?;
                w.into_inner()
            }
            new::LEVEL_CHUNK_WITH_LIGHT => world::level_chunk_with_light(p).map_err(e)?,
            new::BLOCK_ENTITY_DATA => world::block_entity_data(p).map_err(e)?,
            new::BLOCK_UPDATE => world::block_update(p).map_err(e)?,
            new::SECTION_BLOCKS_UPDATE => world::section_blocks_update(p).map_err(e)?,
            new::LEVEL_EVENT => world::level_event(p).map_err(e)?,
            new::FORGET_LEVEL_CHUNK => world::forget_level_chunk(p).map_err(e)?,
            new::SOUND | new::SOUND_ENTITY => match world::sound(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            new::TAG_QUERY => world::tag_query(p).map_err(e)?,
            new::UPDATE_MOB_EFFECT => world::update_mob_effect(p).map_err(e)?,
            new::REMOVE_MOB_EFFECT => world::remove_mob_effect(p).map_err(e)?,
            new::SET_DISPLAY_OBJECTIVE => world::set_display_objective(p).map_err(e)?,
            new::MAP_ITEM_DATA => world::map_item_data(p).map_err(e)?,
            new::LEVEL_PARTICLES => items::level_particles(p).map_err(e)?,
            new::SET_EQUIPMENT => items::set_equipment(p).map_err(e)?,
            new::CONTAINER_SET_CONTENT => items::container_set_content(p).map_err(e)?,
            new::CONTAINER_SET_SLOT => items::container_set_slot(p).map_err(e)?,
            new::MERCHANT_OFFERS => items::merchant_offers(p).map_err(e)?,
            new::UPDATE_ADVANCEMENTS => items::update_advancements(p).map_err(e)?,
            new::UPDATE_RECIPES => items::update_recipes(p).map_err(e)?,
            _ => p.to_vec(),
        };
        let mapped = ids::play_clientbound(pkt.id).ok_or_else(|| format!("no 1.20.1 equivalent for clientbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }

    fn play_serverbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let e = |e: crate::proto::buf::DecodeError| format!("serverbound 0x{:02x}: {e}", pkt.id);
        let p = &pkt.payload[..];
        let payload = match pkt.id {
            old::S_CLIENT_INFORMATION => {
                self.client_information = Some(p.to_vec());
                p.to_vec()
            }
            old::S_SET_CREATIVE_MODE_SLOT => items::set_creative_slot(p).map_err(e)?,
            old::S_CONTAINER_CLICK => items::container_click(p).map_err(e)?,
            old::S_SET_BEACON => {
                // 1.20.2 effect ids start at 0
                let mut r = Reader::new(p);
                let mut w = Writer::new();
                for _ in 0..2 {
                    let some = r.bool().map_err(e)?;
                    w.bool(some);
                    if some {
                        w.varint(r.varint().map_err(e)? - 1);
                    }
                }
                w.into_inner()
            }
            _ => p.to_vec(),
        };
        let mapped = ids::play_serverbound(pkt.id).ok_or_else(|| format!("unknown 1.20.1 serverbound play packet 0x{:02x}", pkt.id))?;
        out.push(Packet::new(mapped, payload));
        Ok(())
    }

    /// The client is in play while the server configures: settings and plugin messages go to
    /// the server as their configuration counterparts, the rest can't be delivered (keep-alives
    /// and pings of configuration are answered by this step).
    fn configuration_serverbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) {
        match pkt.id {
            old::S_CLIENT_INFORMATION => {
                self.client_information = Some(pkt.payload.clone());
                out.push(Packet::new(new::S_CONFIG_CLIENT_INFORMATION, pkt.payload.clone()));
            }
            old::S_CUSTOM_PAYLOAD => out.push(Packet::new(new::S_CONFIG_CUSTOM_PAYLOAD, pkt.payload.clone())),
            _ => {}
        }
    }
}

/// Gives every registry its full name. A server may send "worldgen/biome" for
/// "minecraft:worldgen/biome"; clients read both the same, but the steps for older versions find
/// the registries they must change (biomes, chat types, trim patterns, damage types) by full name.
fn canonical_registry_names(registries: &mut Tag) {
    let full = |name: &str| if name.contains(':') { name.to_string() } else { format!("minecraft:{name}") };
    let Tag::Compound(entries) = registries else { return };
    for (name, registry) in entries.iter_mut() {
        *name = full(name);
        if let Some(Tag::String(ty)) = registry.get_mut("type") {
            *ty = full(ty);
        }
    }
}

impl Default for V1_20_2To1_20 {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator for V1_20_2To1_20 {
    fn from(&self) -> Version {
        Version { protocol: 764 }
    }
    fn to(&self) -> Version {
        Version { protocol: 763 }
    }
    fn clientbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Configuration => self.configuration(pkt, out),
            State::Play => match pkt.id {
                new::LOGIN => self.login(pkt, out),
                new::START_CONFIGURATION => {
                    self.reply(State::Play, new::S_CONFIGURATION_ACKNOWLEDGED, Vec::new());
                    self.start_configuration();
                    Ok(())
                }
                // a kick still reaches the client right away
                new::DISCONNECT => self.play_clientbound(pkt, out),
                _ if self.awaiting_login => {
                    let mut held = std::mem::take(&mut self.held);
                    let r = self.play_clientbound(pkt, &mut held);
                    self.held = held;
                    r
                }
                _ => self.play_clientbound(pkt, out),
            },
        }
    }
    fn serverbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_serverbound(pkt, out),
            State::Configuration => {
                self.configuration_serverbound(pkt, out);
                Ok(())
            }
        }
    }
    fn take_replies(&mut self) -> Vec<(State, Packet)> {
        std::mem::take(&mut self.replies)
    }
    fn emulates_configuration(&self) -> bool {
        true
    }
    fn login_acknowledged(&mut self) {
        self.start_configuration();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registries() -> Tag {
        Tag::Compound(vec![
            ("minecraft:dimension_type".into(), Tag::Compound(vec![("type".into(), Tag::String("minecraft:dimension_type".into())), ("value".into(), Tag::List(Vec::new()))])),
            ("minecraft:worldgen/biome".into(), Tag::Compound(vec![("type".into(), Tag::String("minecraft:worldgen/biome".into()))])),
        ])
    }

    /// A 1.20.2 LOGIN as the server sends it.
    fn server_login() -> Vec<u8> {
        let mut w = Writer::new();
        w.i32(42).bool(false);
        w.varint(1).string("minecraft:overworld");
        w.varint(20).varint(10).varint(8).bool(false).bool(true);
        w.bool(false); // limited crafting
        w.string("minecraft:overworld").string("minecraft:overworld").i64(1234);
        w.u8(1).i8(-1);
        w.bool(false).bool(true); // debug, flat
        w.bool(false); // no death location
        w.varint(0); // portal cooldown
        w.into_inner()
    }

    fn cb(t: &mut V1_20_2To1_20, state: State, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        t.clientbound(state, &Packet::new(id, payload), &mut out).unwrap();
        out
    }

    fn replies(t: &mut V1_20_2To1_20) -> Vec<(State, i32, Vec<u8>)> {
        t.take_replies().into_iter().map(|(s, p)| (s, p.id, p.payload)).collect()
    }

    /// A scripted 1.20.2 configuration from the server: the step answers it, the client sees
    /// nothing until LOGIN, then a 1.20.1 LOGIN with the registries and what configuration sent.
    #[test]
    fn configuration_is_emulated_for_1_20_1() {
        let mut t = V1_20_2To1_20::new();
        assert!(t.emulates_configuration());
        t.login_acknowledged();
        assert_eq!(replies(&mut t), vec![(State::Configuration, new::S_CONFIG_CLIENT_INFORMATION, V1_20_2To1_20::default_client_information())]);

        let mut brand = Writer::new();
        brand.string("minecraft:brand").string("Pumpkin");
        let mut features = Writer::new();
        features.varint(1).string("minecraft:vanilla");
        let mut reg = Writer::new();
        nbt::write(&mut reg, &registries());
        let mut tags = Writer::new();
        tags.varint(1).string("minecraft:block").varint(0);
        let mut keep_alive = Writer::new();
        keep_alive.i64(77);
        let mut ping = Writer::new();
        ping.i32(5);
        let mut seen = Vec::new();
        for (id, payload) in [
            (new::CONFIG_CUSTOM_PAYLOAD, brand.buf.clone()),
            (new::CONFIG_UPDATE_ENABLED_FEATURES, features.buf.clone()),
            (new::CONFIG_REGISTRY_DATA, reg.buf.clone()),
            (new::CONFIG_UPDATE_TAGS, tags.buf.clone()),
            (new::CONFIG_KEEP_ALIVE, keep_alive.buf.clone()),
            (new::CONFIG_PING, ping.buf.clone()),
            (new::CONFIG_FINISH, Vec::new()),
        ] {
            seen.extend(cb(&mut t, State::Configuration, id, payload));
        }
        assert!(seen.is_empty(), "nothing reaches the 1.20.1 client during configuration");
        assert_eq!(
            replies(&mut t),
            vec![
                (State::Configuration, new::S_CONFIG_KEEP_ALIVE, keep_alive.buf.clone()),
                (State::Configuration, new::S_CONFIG_PONG, ping.buf.clone()),
                (State::Configuration, new::S_CONFIG_FINISH, Vec::new()),
            ]
        );

        // a play packet that beats LOGIN waits for it
        let mut time = Writer::new();
        time.i64(1).i64(2);
        assert!(cb(&mut t, State::Play, 96, time.buf.clone()).is_empty());

        let out = cb(&mut t, State::Play, new::LOGIN, server_login());
        let order: Vec<i32> = out.iter().map(|p| p.id).collect();
        assert_eq!(order, vec![old::LOGIN, old::UPDATE_ENABLED_FEATURES, old::UPDATE_TAGS, old::CUSTOM_PAYLOAD, 94]);
        assert_eq!(out[1].payload, features.buf);
        assert_eq!(out[2].payload, tags.buf);
        assert_eq!(out[3].payload, brand.buf);
        assert_eq!(out[4].payload, time.buf);

        let mut r = Reader::new(&out[0].payload);
        assert_eq!(r.i32().unwrap(), 42);
        assert!(!r.bool().unwrap());
        assert_eq!((r.u8().unwrap(), r.i8().unwrap()), (1, -1), "game modes come first");
        assert_eq!(r.list(|r| r.string().map(str::to_string)).unwrap(), vec!["minecraft:overworld"]);
        assert_eq!((r.u8().unwrap(), r.u16().unwrap()), (10, 0), "named root compound");
        r.set_position(r.position() - 3);
        assert_eq!(nbt::read_named_optional(&mut r).unwrap(), Some(registries()));
        assert_eq!(r.string().unwrap(), "minecraft:overworld");
        assert_eq!(r.string().unwrap(), "minecraft:overworld");
        assert_eq!(r.i64().unwrap(), 1234);
        assert_eq!((r.varint().unwrap(), r.varint().unwrap(), r.varint().unwrap()), (20, 10, 8));
        assert_eq!((r.bool().unwrap(), r.bool().unwrap()), (false, true));
        assert_eq!((r.bool().unwrap(), r.bool().unwrap(), r.bool().unwrap(), r.varint().unwrap()), (false, true, false, 0));
        assert!(r.is_empty());

        // afterwards play packets flow straight through
        assert_eq!(cb(&mut t, State::Play, 96, time.buf.clone())[0].id, 94);
    }

    #[test]
    fn resource_pack_in_configuration_is_accepted_and_sent_after_login() {
        let mut t = V1_20_2To1_20::new();
        t.login_acknowledged();
        t.take_replies();
        let mut pack = Writer::new();
        pack.string("https://example.invalid/p.zip").string("").bool(false).bool(false);
        let mut reg = Writer::new();
        nbt::write(&mut reg, &registries());
        cb(&mut t, State::Configuration, new::CONFIG_REGISTRY_DATA, reg.buf);
        cb(&mut t, State::Configuration, new::CONFIG_RESOURCE_PACK, pack.buf.clone());
        assert_eq!(replies(&mut t), vec![(State::Configuration, new::S_CONFIG_RESOURCE_PACK, vec![3]), (State::Configuration, new::S_CONFIG_RESOURCE_PACK, vec![0])]);
        let out = cb(&mut t, State::Play, new::LOGIN, server_login());
        assert_eq!(out.last().map(|p| (p.id, p.payload.clone())), Some((old::RESOURCE_PACK, pack.buf)));
    }

    /// The server re-enters configuration during play: the step acknowledges, re-sends the
    /// client's own settings, absorbs the configuration and turns the next LOGIN into a
    /// 1.20.1 LOGIN (which resets the client's world), keeping the tags it had.
    #[test]
    fn start_configuration_mid_play() {
        let mut t = V1_20_2To1_20::new();
        t.login_acknowledged();
        let mut reg = Writer::new();
        nbt::write(&mut reg, &registries());
        cb(&mut t, State::Configuration, new::CONFIG_REGISTRY_DATA, reg.buf.clone());
        let mut tags = Writer::new();
        tags.varint(0);
        cb(&mut t, State::Configuration, new::CONFIG_UPDATE_TAGS, tags.buf.clone());
        cb(&mut t, State::Configuration, new::CONFIG_FINISH, Vec::new());
        cb(&mut t, State::Play, new::LOGIN, server_login());
        // the client's settings, sent in play
        let mut info = Writer::new();
        info.string("de_de").i8(6).varint(0).bool(true).u8(0x7f).varint(0).bool(false).bool(true);
        let mut out = Vec::new();
        t.serverbound(State::Play, &Packet::new(old::S_CLIENT_INFORMATION, info.buf.clone()), &mut out).unwrap();
        assert_eq!((out[0].id, &out[0].payload), (new::S_CLIENT_INFORMATION, &info.buf));
        t.take_replies();

        assert!(cb(&mut t, State::Play, new::START_CONFIGURATION, Vec::new()).is_empty());
        assert_eq!(
            replies(&mut t),
            vec![(State::Play, new::S_CONFIGURATION_ACKNOWLEDGED, Vec::new()), (State::Configuration, new::S_CONFIG_CLIENT_INFORMATION, info.buf.clone())]
        );
        // the client keeps moving meanwhile: nothing of that reaches the configuring server
        let mut out = Vec::new();
        t.serverbound(State::Configuration, &Packet::new(0x14, vec![0; 25]), &mut out).unwrap();
        assert!(out.is_empty());
        // its plugin messages do, as configuration packets
        t.serverbound(State::Configuration, &Packet::new(old::S_CUSTOM_PAYLOAD, vec![1, 2]), &mut out).unwrap();
        assert_eq!((out[0].id, out[0].payload.clone()), (new::S_CONFIG_CUSTOM_PAYLOAD, vec![1, 2]));

        cb(&mut t, State::Configuration, new::CONFIG_REGISTRY_DATA, reg.buf);
        cb(&mut t, State::Configuration, new::CONFIG_FINISH, Vec::new());
        assert_eq!(replies(&mut t), vec![(State::Configuration, new::S_CONFIG_FINISH, Vec::new())]);
        let out = cb(&mut t, State::Play, new::LOGIN, server_login());
        assert_eq!(out.iter().map(|p| p.id).collect::<Vec<_>>(), vec![old::LOGIN, old::UPDATE_TAGS]);
    }

    #[test]
    fn chunk_batches_are_answered_and_dropped() {
        let mut t = V1_20_2To1_20::new();
        assert!(cb(&mut t, State::Play, new::CHUNK_BATCH_START, Vec::new()).is_empty());
        assert!(t.take_replies().is_empty());
        let mut size = Writer::new();
        size.varint(9);
        assert!(cb(&mut t, State::Play, new::CHUNK_BATCH_FINISHED, size.buf).is_empty());
        let mut want = Writer::new();
        want.f32(CHUNKS_PER_TICK);
        assert_eq!(replies(&mut t), vec![(State::Play, new::S_CHUNK_BATCH_RECEIVED, want.buf)]);
        assert!(cb(&mut t, State::Play, new::PONG_RESPONSE, vec![0; 8]).is_empty());
    }

    #[test]
    fn player_spawn_is_converted() {
        let mut t = V1_20_2To1_20::new();
        let player = data_1_20::NEW_ENTITY_NAMES.iter().position(|n| *n == "player").unwrap() as i32;
        let mut w = Writer::new();
        w.varint(3).uuid(9).varint(player).f64(0.0).f64(64.0).f64(0.0).i8(0).i8(64).i8(64).varint(0).i16(0).i16(0).i16(0);
        let out = cb(&mut t, State::Play, new::ADD_ENTITY, w.buf);
        assert_eq!(out.iter().map(|p| p.id).collect::<Vec<_>>(), vec![old::ADD_PLAYER, old::ROTATE_HEAD]);
    }

    #[test]
    fn display_entities_are_tracked_for_their_data() {
        let mut t = V1_20_2To1_20::new();
        let display = data_1_20::NEW_ENTITY_NAMES.iter().position(|n| *n == "text_display").unwrap() as i32;
        let mut w = Writer::new();
        w.varint(5).uuid(1).varint(display).f64(0.0).f64(0.0).f64(0.0).i8(0).i8(0).i8(0).varint(0).i16(0).i16(0).i16(0);
        cb(&mut t, State::Play, new::ADD_ENTITY, w.buf);
        let mut d = Writer::new();
        d.varint(5).u8(10).varint(1).varint(3).u8(0xff);
        let out = cb(&mut t, State::Play, new::SET_ENTITY_DATA, d.buf.clone());
        assert_eq!(out[0].payload, vec![5, 0xff]);
        // other entities keep index 10
        let out = cb(&mut t, State::Play, new::SET_ENTITY_DATA, {
            let mut d = Writer::new();
            d.varint(6).u8(10).varint(1).varint(3).u8(0xff);
            d.buf
        });
        assert_eq!(out[0].payload, vec![6, 10, 1, 3, 0xff]);
    }

    #[test]
    fn mob_effects_and_beacons_shift_ids() {
        let mut t = V1_20_2To1_20::new();
        let mut w = Writer::new();
        w.varint(1).varint(0).i8(0).varint(100).i8(0).bool(false);
        let out = cb(&mut t, State::Play, new::UPDATE_MOB_EFFECT, w.buf);
        let mut r = Reader::new(&out[0].payload);
        r.varint().unwrap();
        assert_eq!(r.varint().unwrap(), 1, "speed is 1 in 1.20.1");
        let mut out = Vec::new();
        t.serverbound(State::Play, &Packet::new(old::S_SET_BEACON, vec![1, 1, 0]), &mut out).unwrap();
        assert_eq!((out[0].id, out[0].payload.clone()), (42, vec![1, 0, 0]));
    }

    #[test]
    fn config_disconnect_reaches_the_client_as_play_disconnect() {
        let mut t = V1_20_2To1_20::new();
        t.login_acknowledged();
        let mut w = Writer::new();
        w.string("{\"text\":\"bye\"}");
        let out = cb(&mut t, State::Configuration, new::CONFIG_DISCONNECT, w.buf.clone());
        assert_eq!((out[0].id, out[0].payload.clone()), (old::DISCONNECT, w.buf));
    }

    #[test]
    fn registry_names_get_their_namespace() {
        let mut tag = Tag::Compound(vec![
            ("worldgen/biome".into(), Tag::Compound(vec![("type".into(), Tag::String("worldgen/biome".into())), ("value".into(), Tag::List(Vec::new()))])),
            ("minecraft:chat_type".into(), Tag::Compound(vec![("type".into(), Tag::String("minecraft:chat_type".into()))])),
            ("mod:thing".into(), Tag::Compound(Vec::new())),
        ]);
        canonical_registry_names(&mut tag);
        let Tag::Compound(entries) = &tag else { unreachable!() };
        let names: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["minecraft:worldgen/biome", "minecraft:chat_type", "mod:thing"]);
        assert_eq!(tag.get("minecraft:worldgen/biome").and_then(|r| r.get("type")).and_then(Tag::as_str), Some("minecraft:worldgen/biome"));
    }
}

//! Packet ids for 1.20.3 / 1.20.4 (protocol 765, server side of this step) and 1.20.2
//! (protocol 764). Source: the vanilla packet lists for both versions (position = id).
//!
//! 1.20.3 inserted reset_score, resource_pack_pop and resource_pack_push (replacing
//! resource_pack) in clientbound play, ticking_state and ticking_step before
//! update_advancements, and resource_pack_pop/push in clientbound configuration. Serverbound play
//! gained container_slot_state_changed; serverbound configuration is unchanged.

pub mod new {
    // clientbound play (1.20.3)
    pub const ADD_ENTITY: i32 = 0x01;
    pub const AWARD_STATS: i32 = 0x04;
    pub const BLOCK_ENTITY_DATA: i32 = 0x07;
    pub const BLOCK_EVENT: i32 = 0x08;
    pub const BLOCK_UPDATE: i32 = 0x09;
    pub const BOSS_EVENT: i32 = 0x0A;
    pub const COMMAND_SUGGESTIONS: i32 = 0x10;
    pub const COMMANDS: i32 = 0x11;
    pub const CONTAINER_SET_CONTENT: i32 = 0x13;
    pub const CONTAINER_SET_SLOT: i32 = 0x15;
    pub const COOLDOWN: i32 = 0x16;
    pub const DISCONNECT: i32 = 0x1B;
    pub const DISGUISED_CHAT: i32 = 0x1C;
    pub const EXPLODE: i32 = 0x1E;
    pub const GAME_EVENT: i32 = 0x20;
    pub const LEVEL_CHUNK_WITH_LIGHT: i32 = 0x25;
    pub const LEVEL_EVENT: i32 = 0x26;
    pub const LEVEL_PARTICLES: i32 = 0x27;
    pub const LOGIN: i32 = 0x29;
    pub const MAP_ITEM_DATA: i32 = 0x2A;
    pub const MERCHANT_OFFERS: i32 = 0x2B;
    pub const OPEN_SCREEN: i32 = 0x31;
    pub const PLAYER_CHAT: i32 = 0x37;
    pub const PLAYER_COMBAT_KILL: i32 = 0x3A;
    pub const PLAYER_INFO_UPDATE: i32 = 0x3C;
    pub const REMOVE_ENTITIES: i32 = 0x40;
    pub const RESET_SCORE: i32 = 0x42;
    pub const RESOURCE_PACK_POP: i32 = 0x43;
    pub const RESOURCE_PACK_PUSH: i32 = 0x44;
    pub const RESPAWN: i32 = 0x45;
    pub const SECTION_BLOCKS_UPDATE: i32 = 0x47;
    pub const SERVER_DATA: i32 = 0x49;
    pub const SET_ACTION_BAR_TEXT: i32 = 0x4A;
    pub const SET_DEFAULT_SPAWN_POSITION: i32 = 0x54;
    pub const SET_ENTITY_DATA: i32 = 0x56;
    pub const SET_EQUIPMENT: i32 = 0x59;
    pub const SET_OBJECTIVE: i32 = 0x5C;
    pub const SET_PLAYER_TEAM: i32 = 0x5E;
    pub const SET_SCORE: i32 = 0x5F;
    pub const SET_SUBTITLE_TEXT: i32 = 0x61;
    pub const SET_TITLE_TEXT: i32 = 0x63;
    pub const SOUND_ENTITY: i32 = 0x65;
    pub const SOUND: i32 = 0x66;
    pub const SYSTEM_CHAT: i32 = 0x69;
    pub const TAB_LIST: i32 = 0x6A;
    pub const TICKING_STATE: i32 = 0x6E;
    pub const TICKING_STEP: i32 = 0x6F;
    pub const UPDATE_ADVANCEMENTS: i32 = 0x70;
    pub const UPDATE_RECIPES: i32 = 0x73;
    pub const UPDATE_TAGS: i32 = 0x74;
    // clientbound configuration (1.20.3)
    pub const CONFIG_DISCONNECT: i32 = 0x01;
    pub const CONFIG_REGISTRY_DATA: i32 = 0x05;
    pub const CONFIG_RESOURCE_PACK_POP: i32 = 0x06;
    pub const CONFIG_RESOURCE_PACK_PUSH: i32 = 0x07;
    pub const CONFIG_UPDATE_ENABLED_FEATURES: i32 = 0x08;
    pub const CONFIG_UPDATE_TAGS: i32 = 0x09;
    // serverbound play (1.20.3)
    pub const S_CONTAINER_SLOT_STATE_CHANGED: i32 = 0x0F;
    pub const S_RESOURCE_PACK: i32 = 0x28;
    pub const S_SET_CREATIVE_MODE_SLOT: i32 = 0x2F;
    pub const S_SET_JIGSAW_BLOCK: i32 = 0x30;
}

pub mod old {
    // clientbound play (1.20.2)
    pub const RESOURCE_PACK: i32 = 0x42;
    pub const SET_DEFAULT_SPAWN_POSITION: i32 = 0x52;
    pub const SET_SCORE: i32 = 0x5D;
    pub const UPDATE_TAGS: i32 = 0x70;
    // clientbound configuration (1.20.2)
    pub const CONFIG_RESOURCE_PACK: i32 = 0x06;
    pub const CONFIG_UPDATE_TAGS: i32 = 0x08;
    // serverbound play (1.20.2)
    pub const S_CONTAINER_CLICK: i32 = 0x0D;
    pub const S_CUSTOM_PAYLOAD: i32 = 0x0F;
    pub const S_RESOURCE_PACK: i32 = 0x27;
    pub const S_SET_CREATIVE_MODE_SLOT: i32 = 0x2E;
    pub const S_SET_JIGSAW_BLOCK: i32 = 0x2F;
    pub const S_USE_ITEM: i32 = 0x35;
    // serverbound configuration (1.20.2 = 1.20.3)
    pub const S_CONFIG_RESOURCE_PACK: i32 = 0x05;
}

/// Clientbound play id 1.20.3 → 1.20.2. `None` = the 1.20.2 client has no such packet
/// (reset_score is rewritten into set_score before this is asked).
pub fn play_clientbound(id: i32) -> Option<i32> {
    match id {
        0x00..=0x41 => Some(id),
        new::RESOURCE_PACK_PUSH => Some(old::RESOURCE_PACK),
        0x45..=0x6D => Some(id - 2),
        0x70..=0x74 => Some(id - 4),
        _ => None,
    }
}

/// Clientbound configuration id 1.20.3 → 1.20.2.
pub fn config_clientbound(id: i32) -> Option<i32> {
    match id {
        0x00..=0x05 => Some(id),
        new::CONFIG_RESOURCE_PACK_PUSH => Some(old::CONFIG_RESOURCE_PACK),
        0x08 | 0x09 => Some(id - 1),
        _ => None,
    }
}

/// Serverbound play id 1.20.2 → 1.20.3: everything from custom_payload on moved up by one.
pub fn play_serverbound(id: i32) -> Option<i32> {
    match id {
        0x00..=0x0E => Some(id),
        0x0F..=0x35 => Some(id + 1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_maps_are_consistent() {
        assert_eq!(play_clientbound(new::RESPAWN), Some(0x43));
        assert_eq!(play_clientbound(new::SET_SCORE), Some(old::SET_SCORE));
        assert_eq!(play_clientbound(new::SET_DEFAULT_SPAWN_POSITION), Some(old::SET_DEFAULT_SPAWN_POSITION));
        assert_eq!(play_clientbound(new::TAB_LIST), Some(0x68));
        assert_eq!(play_clientbound(new::UPDATE_ADVANCEMENTS), Some(0x6C));
        assert_eq!(play_clientbound(new::UPDATE_TAGS), Some(old::UPDATE_TAGS));
        assert_eq!(play_clientbound(new::RESOURCE_PACK_PUSH), Some(old::RESOURCE_PACK));
        for gone in [new::RESET_SCORE, new::RESOURCE_PACK_POP, new::TICKING_STATE, new::TICKING_STEP, 0x75] {
            assert_eq!(play_clientbound(gone), None);
        }
        assert_eq!(config_clientbound(new::CONFIG_REGISTRY_DATA), Some(0x05));
        assert_eq!(config_clientbound(new::CONFIG_RESOURCE_PACK_PUSH), Some(old::CONFIG_RESOURCE_PACK));
        assert_eq!(config_clientbound(new::CONFIG_UPDATE_TAGS), Some(old::CONFIG_UPDATE_TAGS));
        assert_eq!(config_clientbound(new::CONFIG_RESOURCE_PACK_POP), None);
        assert_eq!(play_serverbound(old::S_CONTAINER_CLICK), Some(old::S_CONTAINER_CLICK));
        assert_eq!(play_serverbound(old::S_CUSTOM_PAYLOAD), Some(0x10));
        assert_eq!(play_serverbound(old::S_RESOURCE_PACK), Some(new::S_RESOURCE_PACK));
        assert_eq!(play_serverbound(old::S_SET_CREATIVE_MODE_SLOT), Some(new::S_SET_CREATIVE_MODE_SLOT));
        assert_eq!(play_serverbound(old::S_SET_JIGSAW_BLOCK), Some(new::S_SET_JIGSAW_BLOCK));
        assert_eq!(play_serverbound(old::S_USE_ITEM), Some(0x36));
        // no 1.20.2 packet lands on the new slot-state packet
        assert!((0..=0x35).filter_map(play_serverbound).all(|id| id != new::S_CONTAINER_SLOT_STATE_CHANGED));
        assert_eq!(play_serverbound(0x36), None);
    }
}

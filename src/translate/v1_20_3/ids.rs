//! Packet ids for 1.20.5 / 1.20.6 (protocol 766, server side of this step) and 1.20.3 / 1.20.4
//! (protocol 765). Source: the vanilla packet lists for both versions (position = id).
//!
//! 1.20.5 inserted cookie_request, debug_sample, store_cookie, transfer and projectile_power into
//! clientbound play; cookie_request, reset_chat, store_cookie, transfer and select_known_packs into
//! clientbound configuration; chat_command_signed, cookie_response and debug_subscription_request
//! into serverbound play; cookie_response and select_known_packs into serverbound configuration.

pub mod new {
    // clientbound play (1.20.5)
    pub const ADD_ENTITY: i32 = 0x01;
    pub const AWARD_STATS: i32 = 0x04;
    pub const BLOCK_ENTITY_DATA: i32 = 0x07;
    pub const BLOCK_EVENT: i32 = 0x08;
    pub const BLOCK_UPDATE: i32 = 0x09;
    pub const COMMANDS: i32 = 0x11;
    pub const CONTAINER_CLOSE: i32 = 0x12;
    pub const CONTAINER_SET_CONTENT: i32 = 0x13;
    pub const CONTAINER_SET_DATA: i32 = 0x14;
    pub const CONTAINER_SET_SLOT: i32 = 0x15;
    pub const COOKIE_REQUEST: i32 = 0x16;
    pub const COOLDOWN: i32 = 0x17;
    pub const DEBUG_SAMPLE: i32 = 0x1B;
    pub const DISGUISED_CHAT: i32 = 0x1E;
    pub const EXPLODE: i32 = 0x20;
    pub const HORSE_SCREEN_OPEN: i32 = 0x23;
    pub const LEVEL_CHUNK_WITH_LIGHT: i32 = 0x27;
    pub const LEVEL_EVENT: i32 = 0x28;
    pub const LEVEL_PARTICLES: i32 = 0x29;
    pub const LOGIN: i32 = 0x2B;
    pub const MAP_ITEM_DATA: i32 = 0x2C;
    pub const MERCHANT_OFFERS: i32 = 0x2D;
    pub const OPEN_SCREEN: i32 = 0x33;
    pub const REMOVE_ENTITIES: i32 = 0x42;
    pub const REMOVE_MOB_EFFECT: i32 = 0x43;
    pub const RESPAWN: i32 = 0x47;
    pub const SECTION_BLOCKS_UPDATE: i32 = 0x49;
    pub const SERVER_DATA: i32 = 0x4B;
    pub const SET_ACTION_BAR_TEXT: i32 = 0x4C;
    pub const SET_ENTITY_DATA: i32 = 0x58;
    pub const SET_EQUIPMENT: i32 = 0x5B;
    pub const SET_SUBTITLE_TEXT: i32 = 0x63;
    pub const SET_TITLE_TEXT: i32 = 0x65;
    pub const SOUND_ENTITY: i32 = 0x67;
    pub const SOUND: i32 = 0x68;
    pub const START_CONFIGURATION: i32 = 0x69;
    pub const STORE_COOKIE: i32 = 0x6B;
    pub const SYSTEM_CHAT: i32 = 0x6C;
    pub const TRANSFER: i32 = 0x73;
    pub const UPDATE_ADVANCEMENTS: i32 = 0x74;
    pub const UPDATE_ATTRIBUTES: i32 = 0x75;
    pub const UPDATE_MOB_EFFECT: i32 = 0x76;
    pub const UPDATE_RECIPES: i32 = 0x77;
    pub const UPDATE_TAGS: i32 = 0x78;
    pub const PROJECTILE_POWER: i32 = 0x79;
    // clientbound configuration (1.20.5)
    pub const CONFIG_COOKIE_REQUEST: i32 = 0x00;
    pub const CONFIG_FINISH: i32 = 0x03;
    pub const CONFIG_RESET_CHAT: i32 = 0x06;
    pub const CONFIG_REGISTRY_DATA: i32 = 0x07;
    pub const CONFIG_STORE_COOKIE: i32 = 0x0A;
    pub const CONFIG_TRANSFER: i32 = 0x0B;
    pub const CONFIG_UPDATE_TAGS: i32 = 0x0D;
    pub const CONFIG_SELECT_KNOWN_PACKS: i32 = 0x0E;
    // serverbound play (1.20.5)
    pub const S_CHAT_COMMAND_SIGNED: i32 = 0x05;
    pub const S_CONTAINER_BUTTON_CLICK: i32 = 0x0D;
    pub const S_CONTAINER_CLICK: i32 = 0x0E;
    pub const S_COOKIE_RESPONSE: i32 = 0x11;
    pub const S_SET_CREATIVE_MODE_SLOT: i32 = 0x32;
    // serverbound configuration (1.20.5)
    pub const S_CONFIG_COOKIE_RESPONSE: i32 = 0x01;
    pub const S_CONFIG_SELECT_KNOWN_PACKS: i32 = 0x07;
}

pub mod old {
    // clientbound play (1.20.3)
    pub const SET_ENTITY_DATA: i32 = 0x56;
    pub const CONTAINER_SET_SLOT: i32 = 0x15;
    pub const LEVEL_PARTICLES: i32 = 0x27;
    pub const LOGIN: i32 = 0x29;
    pub const SET_EQUIPMENT: i32 = 0x59;
    pub const BLOCK_UPDATE: i32 = 0x09;
    // clientbound configuration (1.20.3)
    pub const CONFIG_FINISH: i32 = 0x02;
    pub const CONFIG_REGISTRY_DATA: i32 = 0x05;
    pub const CONFIG_UPDATE_TAGS: i32 = 0x09;
    // serverbound play (1.20.3)
    pub const S_CHAT_COMMAND: i32 = 0x04;
    pub const S_CONTAINER_BUTTON_CLICK: i32 = 0x0C;
    pub const S_CONTAINER_CLICK: i32 = 0x0D;
    pub const S_SET_CREATIVE_MODE_SLOT: i32 = 0x2F;
    pub const S_USE_ITEM: i32 = 0x36;
    // serverbound configuration (1.20.3)
    pub const S_CONFIG_CUSTOM_PAYLOAD: i32 = 0x01;
    pub const S_CONFIG_RESOURCE_PACK: i32 = 0x05;
}

/// Clientbound play id 1.20.5 → 1.20.3. `None` = the 1.20.3 client has no such packet.
pub fn play_clientbound(id: i32) -> Option<i32> {
    match id {
        0x00..=0x15 => Some(id),
        0x17..=0x1A => Some(id - 1),
        0x1C..=0x6A => Some(id - 2),
        0x6C..=0x72 => Some(id - 3),
        0x74..=0x78 => Some(id - 4),
        _ => None,
    }
}

/// Clientbound configuration id 1.20.5 → 1.20.3.
pub fn config_clientbound(id: i32) -> Option<i32> {
    match id {
        0x01..=0x05 => Some(id - 1),
        // registry data (0x07) is rebuilt as one registry codec, see `world::Registries`
        0x08..=0x09 => Some(id - 2),
        0x0C..=0x0D => Some(id - 4),
        _ => None,
    }
}

/// Serverbound play id 1.20.3 → 1.20.5. Commands always go out as signed commands so the
/// acknowledgement data the client sends with them reaches the server.
pub fn play_serverbound(id: i32) -> Option<i32> {
    match id {
        0x00..=0x03 => Some(id),
        0x04 => Some(new::S_CHAT_COMMAND_SIGNED),
        0x05..=0x0F => Some(id + 1),
        0x10 => Some(0x12),
        0x11..=0x36 => Some(id + 3),
        _ => None,
    }
}

/// Serverbound configuration id 1.20.3 → 1.20.5.
pub fn config_serverbound(id: i32) -> Option<i32> {
    match id {
        0x00 => Some(0x00),
        0x01..=0x05 => Some(id + 1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_maps_are_consistent() {
        // ids the 1.21 → 1.20.5 step uses for its 1.20.5 side
        use crate::translate::v1_20_5::ids as up;
        assert_eq!(new::CONTAINER_SET_SLOT, up::new::CONTAINER_SET_SLOT);
        assert_eq!(new::CONTAINER_SET_CONTENT, up::new::CONTAINER_SET_CONTENT);
        assert_eq!(new::SET_ENTITY_DATA, up::new::SET_ENTITY_DATA);
        assert_eq!(new::SET_EQUIPMENT, up::new::SET_EQUIPMENT);
        assert_eq!(new::EXPLODE, up::new::EXPLODE);
        assert_eq!(new::LEVEL_PARTICLES, up::new::LEVEL_PARTICLES);
        assert_eq!(new::LEVEL_EVENT, up::new::LEVEL_EVENT);
        assert_eq!(new::MERCHANT_OFFERS, up::new::MERCHANT_OFFERS);
        assert_eq!(new::UPDATE_ATTRIBUTES, up::new::UPDATE_ATTRIBUTES);
        assert_eq!(new::UPDATE_RECIPES, up::new::UPDATE_RECIPES);
        assert_eq!(new::UPDATE_TAGS, up::new::UPDATE_TAGS);
        assert_eq!(new::PROJECTILE_POWER, up::new::PROJECTILE_POWER);
        assert_eq!(new::SYSTEM_CHAT, up::new::SYSTEM_CHAT);
        assert_eq!(new::OPEN_SCREEN, up::new::OPEN_SCREEN);
        assert_eq!(new::HORSE_SCREEN_OPEN, up::new::HORSE_SCREEN_OPEN);
        assert_eq!(new::COOLDOWN, up::new::COOLDOWN);
        assert_eq!(new::AWARD_STATS, up::new::AWARD_STATS);
        assert_eq!(new::CONFIG_REGISTRY_DATA, up::new::CONFIG_REGISTRY_DATA);
        assert_eq!(new::CONFIG_UPDATE_TAGS, up::new::CONFIG_UPDATE_TAGS);
        assert_eq!(new::S_CONTAINER_CLICK, up::old::S_CONTAINER_CLICK);
        assert_eq!(new::S_SET_CREATIVE_MODE_SLOT, up::old::S_SET_CREATIVE_MODE_SLOT);
        // the 1.20.3 ids follow from the maps
        assert_eq!(play_clientbound(new::SET_ENTITY_DATA), Some(old::SET_ENTITY_DATA));
        assert_eq!(play_clientbound(new::CONTAINER_SET_SLOT), Some(old::CONTAINER_SET_SLOT));
        assert_eq!(play_clientbound(new::LEVEL_PARTICLES), Some(old::LEVEL_PARTICLES));
        assert_eq!(play_clientbound(new::LOGIN), Some(old::LOGIN));
        assert_eq!(play_clientbound(new::SET_EQUIPMENT), Some(old::SET_EQUIPMENT));
        assert_eq!(play_clientbound(new::BLOCK_UPDATE), Some(old::BLOCK_UPDATE));
        assert_eq!(play_clientbound(new::COOLDOWN), Some(0x16));
        assert_eq!(play_clientbound(new::SYSTEM_CHAT), Some(0x69));
        assert_eq!(play_clientbound(new::START_CONFIGURATION), Some(0x67));
        assert_eq!(play_clientbound(new::UPDATE_TAGS), Some(0x74));
        for gone in [new::COOKIE_REQUEST, new::DEBUG_SAMPLE, new::STORE_COOKIE, new::TRANSFER, new::PROJECTILE_POWER] {
            assert_eq!(play_clientbound(gone), None);
        }
        assert_eq!(config_clientbound(new::CONFIG_FINISH), Some(old::CONFIG_FINISH));
        assert_eq!(config_clientbound(new::CONFIG_UPDATE_TAGS), Some(old::CONFIG_UPDATE_TAGS));
        for gone in [
            new::CONFIG_COOKIE_REQUEST,
            new::CONFIG_RESET_CHAT,
            new::CONFIG_REGISTRY_DATA,
            new::CONFIG_STORE_COOKIE,
            new::CONFIG_TRANSFER,
            new::CONFIG_SELECT_KNOWN_PACKS,
        ] {
            assert_eq!(config_clientbound(gone), None);
        }
        assert_eq!(play_serverbound(old::S_CHAT_COMMAND), Some(new::S_CHAT_COMMAND_SIGNED));
        assert_eq!(play_serverbound(old::S_CONTAINER_BUTTON_CLICK), Some(new::S_CONTAINER_BUTTON_CLICK));
        assert_eq!(play_serverbound(old::S_CONTAINER_CLICK), Some(new::S_CONTAINER_CLICK));
        assert_eq!(play_serverbound(old::S_SET_CREATIVE_MODE_SLOT), Some(new::S_SET_CREATIVE_MODE_SLOT));
        assert_eq!(play_serverbound(0x10), Some(0x12)); // custom payload
        assert_eq!(play_serverbound(0x11), Some(0x14)); // edit book
        assert_eq!(play_serverbound(old::S_USE_ITEM), Some(crate::translate::v1_20_5::ids::old::S_USE_ITEM));
        assert_eq!(play_serverbound(0x37), None);
        assert_eq!(config_serverbound(old::S_CONFIG_CUSTOM_PAYLOAD), Some(0x02));
        assert_eq!(config_serverbound(old::S_CONFIG_RESOURCE_PACK), Some(0x06));
        assert_eq!(config_serverbound(0x02), Some(0x03)); // finish configuration
    }
}

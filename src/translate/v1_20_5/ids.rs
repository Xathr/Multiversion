//! Packet ids for 1.21 / 1.21.1 (protocol 767, server side of this step) and 1.20.5 / 1.20.6
//! (protocol 766). Source: the vanilla packet lists for both versions (position = id).
//!
//! 1.21 only appended packets: clientbound play gained custom_report_details and server_links
//! after projectile_power, clientbound configuration the same two after select_known_packs.
//! Every other id, and every serverbound id, is the same in both versions.

pub mod new {
    // clientbound play (1.21)
    pub const AWARD_STATS: i32 = 0x04;
    pub const CONTAINER_CLOSE: i32 = 0x12;
    pub const CONTAINER_SET_CONTENT: i32 = 0x13;
    pub const CONTAINER_SET_DATA: i32 = 0x14;
    pub const CONTAINER_SET_SLOT: i32 = 0x15;
    pub const COOLDOWN: i32 = 0x17;
    pub const DISGUISED_CHAT: i32 = 0x1E;
    pub const EXPLODE: i32 = 0x20;
    pub const GAME_EVENT: i32 = 0x22;
    pub const HORSE_SCREEN_OPEN: i32 = 0x23;
    pub const LEVEL_EVENT: i32 = 0x28;
    pub const LEVEL_PARTICLES: i32 = 0x29;
    pub const MERCHANT_OFFERS: i32 = 0x2D;
    pub const OPEN_SCREEN: i32 = 0x33;
    pub const PLAYER_CHAT: i32 = 0x39;
    pub const PLAYER_POSITION: i32 = 0x40;
    pub const SET_ACTION_BAR_TEXT: i32 = 0x4C;
    pub const SET_ENTITY_DATA: i32 = 0x58;
    pub const SET_EQUIPMENT: i32 = 0x5B;
    pub const SET_SUBTITLE_TEXT: i32 = 0x63;
    pub const SET_TITLE_TEXT: i32 = 0x65;
    pub const SOUND_ENTITY: i32 = 0x67;
    pub const SOUND: i32 = 0x68;
    pub const SYSTEM_CHAT: i32 = 0x6C;
    pub const UPDATE_ADVANCEMENTS: i32 = 0x74;
    pub const UPDATE_ATTRIBUTES: i32 = 0x75;
    pub const UPDATE_RECIPES: i32 = 0x77;
    pub const UPDATE_TAGS: i32 = 0x78;
    pub const PROJECTILE_POWER: i32 = 0x79;
    pub const CUSTOM_REPORT_DETAILS: i32 = 0x7A;
    pub const SERVER_LINKS: i32 = 0x7B;
    // clientbound configuration (1.21)
    pub const CONFIG_REGISTRY_DATA: i32 = 0x07;
    pub const CONFIG_UPDATE_ENABLED_FEATURES: i32 = 0x0C;
    pub const CONFIG_UPDATE_TAGS: i32 = 0x0D;
    pub const CONFIG_CUSTOM_REPORT_DETAILS: i32 = 0x0F;
    pub const CONFIG_SERVER_LINKS: i32 = 0x10;
}

pub mod old {
    // serverbound play (1.20.5 = 1.21)
    pub const S_CONTAINER_CLICK: i32 = 0x0E;
    pub const S_MOVE_PLAYER_POS_ROT: i32 = 0x1B;
    pub const S_MOVE_PLAYER_ROT: i32 = 0x1C;
    pub const S_SET_CREATIVE_MODE_SLOT: i32 = 0x32;
    pub const S_USE_ITEM: i32 = 0x39;
}

/// Clientbound play id 1.21 → 1.20.5. `None` = the 1.20.5 client has no such packet.
pub fn play_clientbound(id: i32) -> Option<i32> {
    match id {
        0x00..=0x79 => Some(id),
        _ => None,
    }
}

/// Clientbound configuration id 1.21 → 1.20.5.
pub fn config_clientbound(id: i32) -> Option<i32> {
    match id {
        0x00..=0x0E => Some(id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_maps_are_consistent() {
        // ids the 1.21 → 1.21.2 step uses for its 1.21 side
        assert_eq!(new::EXPLODE, crate::translate::v1_21::ids::old::EXPLODE);
        assert_eq!(new::UPDATE_ATTRIBUTES, crate::translate::v1_21::ids::old::UPDATE_ATTRIBUTES);
        assert_eq!(new::UPDATE_RECIPES, crate::translate::v1_21::ids::old::UPDATE_RECIPES);
        assert_eq!(new::SET_ENTITY_DATA, crate::translate::v1_21::ids::old::SET_ENTITY_DATA);
        assert_eq!(new::SET_EQUIPMENT, crate::translate::v1_21::ids::old::SET_EQUIPMENT);
        assert_eq!(new::CONTAINER_SET_SLOT, crate::translate::v1_21::ids::old::CONTAINER_SET_SLOT);
        assert_eq!(new::PLAYER_POSITION, crate::translate::v1_21::ids::old::PLAYER_POSITION);
        assert_eq!(old::S_MOVE_PLAYER_ROT, crate::translate::v1_21::ids::old::S_MOVE_PLAYER_ROT);
        assert_eq!(old::S_MOVE_PLAYER_POS_ROT, crate::translate::v1_21::ids::old::S_MOVE_PLAYER_POS_ROT);
        assert_eq!(old::S_CONTAINER_CLICK, crate::translate::v1_21::ids::old::S_CONTAINER_CLICK);
        assert_eq!(old::S_SET_CREATIVE_MODE_SLOT, crate::translate::v1_21::ids::old::S_SET_CREATIVE_MODE_SLOT);
        assert_eq!(play_clientbound(0x00), Some(0x00));
        assert_eq!(play_clientbound(new::UPDATE_TAGS), Some(0x78));
        assert_eq!(play_clientbound(new::PROJECTILE_POWER), Some(0x79));
        for gone in [new::CUSTOM_REPORT_DETAILS, new::SERVER_LINKS] {
            assert_eq!(play_clientbound(gone), None);
        }
        assert_eq!(config_clientbound(0x0E), Some(0x0E)); // select_known_packs
        for gone in [new::CONFIG_CUSTOM_REPORT_DETAILS, new::CONFIG_SERVER_LINKS] {
            assert_eq!(config_clientbound(gone), None);
        }
    }
}

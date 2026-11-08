//! Packet ids for 1.20.2 (protocol 764, server side of this step) and 1.20 / 1.20.1 (protocol
//! 763). Source: the vanilla packet lists for both versions (position = id).
//!
//! 1.20.2 added chunk batching, ping requests and the configuration phase (START_CONFIGURATION in
//! play), moved UPDATE_ENABLED_FEATURES into configuration and spawns players with ADD_ENTITY
//! (1.20.1 still has ADD_PLAYER).

pub mod new {
    // clientbound play (1.20.2)
    pub const ADD_ENTITY: i32 = 1;
    pub const BLOCK_ENTITY_DATA: i32 = 7;
    pub const BLOCK_UPDATE: i32 = 9;
    pub const CHUNK_BATCH_FINISHED: i32 = 12;
    pub const CHUNK_BATCH_START: i32 = 13;
    pub const CONTAINER_SET_CONTENT: i32 = 19;
    pub const CONTAINER_SET_SLOT: i32 = 21;
    pub const DISCONNECT: i32 = 27;
    pub const FORGET_LEVEL_CHUNK: i32 = 31;
    pub const LEVEL_CHUNK_WITH_LIGHT: i32 = 37;
    pub const LEVEL_EVENT: i32 = 38;
    pub const LEVEL_PARTICLES: i32 = 39;
    pub const LOGIN: i32 = 41;
    pub const MAP_ITEM_DATA: i32 = 42;
    pub const MERCHANT_OFFERS: i32 = 43;
    pub const PONG_RESPONSE: i32 = 52;
    pub const REMOVE_ENTITIES: i32 = 64;
    pub const REMOVE_MOB_EFFECT: i32 = 65;
    pub const RESPAWN: i32 = 67;
    pub const SECTION_BLOCKS_UPDATE: i32 = 69;
    pub const SET_DISPLAY_OBJECTIVE: i32 = 83;
    pub const SET_ENTITY_DATA: i32 = 84;
    pub const SET_EQUIPMENT: i32 = 87;
    pub const SOUND_ENTITY: i32 = 99;
    pub const SOUND: i32 = 100;
    pub const START_CONFIGURATION: i32 = 101;
    pub const TAG_QUERY: i32 = 105;
    pub const UPDATE_ADVANCEMENTS: i32 = 108;
    pub const UPDATE_MOB_EFFECT: i32 = 110;
    pub const UPDATE_RECIPES: i32 = 111;
    // clientbound configuration (1.20.2)
    pub const CONFIG_CUSTOM_PAYLOAD: i32 = 0;
    pub const CONFIG_DISCONNECT: i32 = 1;
    pub const CONFIG_FINISH: i32 = 2;
    pub const CONFIG_KEEP_ALIVE: i32 = 3;
    pub const CONFIG_PING: i32 = 4;
    pub const CONFIG_REGISTRY_DATA: i32 = 5;
    pub const CONFIG_RESOURCE_PACK: i32 = 6;
    pub const CONFIG_UPDATE_ENABLED_FEATURES: i32 = 7;
    pub const CONFIG_UPDATE_TAGS: i32 = 8;
    // serverbound play (1.20.2)
    pub const S_CHUNK_BATCH_RECEIVED: i32 = 7;
    pub const S_CLIENT_INFORMATION: i32 = 9;
    pub const S_CONFIGURATION_ACKNOWLEDGED: i32 = 11;
    // serverbound configuration (1.20.2)
    pub const S_CONFIG_CLIENT_INFORMATION: i32 = 0;
    pub const S_CONFIG_CUSTOM_PAYLOAD: i32 = 1;
    pub const S_CONFIG_FINISH: i32 = 2;
    pub const S_CONFIG_KEEP_ALIVE: i32 = 3;
    pub const S_CONFIG_PONG: i32 = 4;
    pub const S_CONFIG_RESOURCE_PACK: i32 = 5;
}

pub mod old {
    // clientbound play (1.20.1)
    pub const ADD_PLAYER: i32 = 3;
    pub const CUSTOM_PAYLOAD: i32 = 23;
    pub const DISCONNECT: i32 = 26;
    pub const LOGIN: i32 = 40;
    pub const RESOURCE_PACK: i32 = 64;
    pub const ROTATE_HEAD: i32 = 66;
    pub const SET_ENTITY_MOTION: i32 = 84;
    pub const UPDATE_ENABLED_FEATURES: i32 = 107;
    pub const UPDATE_TAGS: i32 = 110;
    // serverbound play (1.20.1)
    pub const S_CLIENT_INFORMATION: i32 = 8;
    pub const S_CONTAINER_CLICK: i32 = 11;
    pub const S_CUSTOM_PAYLOAD: i32 = 13;
    pub const S_SET_BEACON: i32 = 39;
    pub const S_SET_CREATIVE_MODE_SLOT: i32 = 43;
}

/// Clientbound play id 1.20.2 → 1.20.1. `None` = the 1.20.1 client has no such packet.
pub fn play_clientbound(id: i32) -> Option<i32> {
    match id {
        0..=2 => Some(id),
        3..=11 => Some(id + 1),    // ADD_PLAYER sits at 3 in 1.20.1
        14..=51 => Some(id - 1),   // after the two chunk batch packets
        53..=100 => Some(id - 2),  // after PONG_RESPONSE
        102..=109 => Some(id - 3), // after START_CONFIGURATION
        110..=112 => Some(id - 2), // UPDATE_ENABLED_FEATURES sits at 107 in 1.20.1
        _ => None,
    }
}

/// Serverbound play id 1.20.1 → 1.20.2.
pub fn play_serverbound(id: i32) -> Option<i32> {
    match id {
        0..=6 => Some(id),
        7..=9 => Some(id + 1),   // CHUNK_BATCH_RECEIVED inserted at 7
        10..=26 => Some(id + 2), // CONFIGURATION_ACKNOWLEDGED at 11
        27..=50 => Some(id + 3), // PING_REQUEST at 29
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Packet lists in id order (ViaVersion's ClientboundPackets1_19_4 / 1_20_2 and
    // ServerboundPackets1_19_4 / 1_20_2; 1.20 uses the 1.19.4 set).
    const C_OLD: &[&str] = &["BUNDLE_DELIMITER", "ADD_ENTITY", "ADD_EXPERIENCE_ORB", "ADD_PLAYER", "ANIMATE", "AWARD_STATS", "BLOCK_CHANGED_ACK", "BLOCK_DESTRUCTION", "BLOCK_ENTITY_DATA", "BLOCK_EVENT", "BLOCK_UPDATE", "BOSS_EVENT", "CHANGE_DIFFICULTY", "CHUNKS_BIOMES", "CLEAR_TITLES", "COMMAND_SUGGESTIONS", "COMMANDS", "CONTAINER_CLOSE", "CONTAINER_SET_CONTENT", "CONTAINER_SET_DATA", "CONTAINER_SET_SLOT", "COOLDOWN", "CUSTOM_CHAT_COMPLETIONS", "CUSTOM_PAYLOAD", "DAMAGE_EVENT", "DELETE_CHAT", "DISCONNECT", "DISGUISED_CHAT", "ENTITY_EVENT", "EXPLODE", "FORGET_LEVEL_CHUNK", "GAME_EVENT", "HORSE_SCREEN_OPEN", "HURT_ANIMATION", "INITIALIZE_BORDER", "KEEP_ALIVE", "LEVEL_CHUNK_WITH_LIGHT", "LEVEL_EVENT", "LEVEL_PARTICLES", "LIGHT_UPDATE", "LOGIN", "MAP_ITEM_DATA", "MERCHANT_OFFERS", "MOVE_ENTITY_POS", "MOVE_ENTITY_POS_ROT", "MOVE_ENTITY_ROT", "MOVE_VEHICLE", "OPEN_BOOK", "OPEN_SCREEN", "OPEN_SIGN_EDITOR", "PING", "PLACE_GHOST_RECIPE", "PLAYER_ABILITIES", "PLAYER_CHAT", "PLAYER_COMBAT_END", "PLAYER_COMBAT_ENTER", "PLAYER_COMBAT_KILL", "PLAYER_INFO_REMOVE", "PLAYER_INFO_UPDATE", "PLAYER_LOOK_AT", "PLAYER_POSITION", "RECIPE", "REMOVE_ENTITIES", "REMOVE_MOB_EFFECT", "RESOURCE_PACK", "RESPAWN", "ROTATE_HEAD", "SECTION_BLOCKS_UPDATE", "SELECT_ADVANCEMENTS_TAB", "SERVER_DATA", "SET_ACTION_BAR_TEXT", "SET_BORDER_CENTER", "SET_BORDER_LERP_SIZE", "SET_BORDER_SIZE", "SET_BORDER_WARNING_DELAY", "SET_BORDER_WARNING_DISTANCE", "SET_CAMERA", "SET_CARRIED_ITEM", "SET_CHUNK_CACHE_CENTER", "SET_CHUNK_CACHE_RADIUS", "SET_DEFAULT_SPAWN_POSITION", "SET_DISPLAY_OBJECTIVE", "SET_ENTITY_DATA", "SET_ENTITY_LINK", "SET_ENTITY_MOTION", "SET_EQUIPMENT", "SET_EXPERIENCE", "SET_HEALTH", "SET_OBJECTIVE", "SET_PASSENGERS", "SET_PLAYER_TEAM", "SET_SCORE", "SET_SIMULATION_DISTANCE", "SET_SUBTITLE_TEXT", "SET_TIME", "SET_TITLE_TEXT", "SET_TITLES_ANIMATION", "SOUND_ENTITY", "SOUND", "STOP_SOUND", "SYSTEM_CHAT", "TAB_LIST", "TAG_QUERY", "TAKE_ITEM_ENTITY", "TELEPORT_ENTITY", "UPDATE_ADVANCEMENTS", "UPDATE_ATTRIBUTES", "UPDATE_ENABLED_FEATURES", "UPDATE_MOB_EFFECT", "UPDATE_RECIPES", "UPDATE_TAGS"];
    const C_NEW: &[&str] = &["BUNDLE_DELIMITER", "ADD_ENTITY", "ADD_EXPERIENCE_ORB", "ANIMATE", "AWARD_STATS", "BLOCK_CHANGED_ACK", "BLOCK_DESTRUCTION", "BLOCK_ENTITY_DATA", "BLOCK_EVENT", "BLOCK_UPDATE", "BOSS_EVENT", "CHANGE_DIFFICULTY", "CHUNK_BATCH_FINISHED", "CHUNK_BATCH_START", "CHUNKS_BIOMES", "CLEAR_TITLES", "COMMAND_SUGGESTIONS", "COMMANDS", "CONTAINER_CLOSE", "CONTAINER_SET_CONTENT", "CONTAINER_SET_DATA", "CONTAINER_SET_SLOT", "COOLDOWN", "CUSTOM_CHAT_COMPLETIONS", "CUSTOM_PAYLOAD", "DAMAGE_EVENT", "DELETE_CHAT", "DISCONNECT", "DISGUISED_CHAT", "ENTITY_EVENT", "EXPLODE", "FORGET_LEVEL_CHUNK", "GAME_EVENT", "HORSE_SCREEN_OPEN", "HURT_ANIMATION", "INITIALIZE_BORDER", "KEEP_ALIVE", "LEVEL_CHUNK_WITH_LIGHT", "LEVEL_EVENT", "LEVEL_PARTICLES", "LIGHT_UPDATE", "LOGIN", "MAP_ITEM_DATA", "MERCHANT_OFFERS", "MOVE_ENTITY_POS", "MOVE_ENTITY_POS_ROT", "MOVE_ENTITY_ROT", "MOVE_VEHICLE", "OPEN_BOOK", "OPEN_SCREEN", "OPEN_SIGN_EDITOR", "PING", "PONG_RESPONSE", "PLACE_GHOST_RECIPE", "PLAYER_ABILITIES", "PLAYER_CHAT", "PLAYER_COMBAT_END", "PLAYER_COMBAT_ENTER", "PLAYER_COMBAT_KILL", "PLAYER_INFO_REMOVE", "PLAYER_INFO_UPDATE", "PLAYER_LOOK_AT", "PLAYER_POSITION", "RECIPE", "REMOVE_ENTITIES", "REMOVE_MOB_EFFECT", "RESOURCE_PACK", "RESPAWN", "ROTATE_HEAD", "SECTION_BLOCKS_UPDATE", "SELECT_ADVANCEMENTS_TAB", "SERVER_DATA", "SET_ACTION_BAR_TEXT", "SET_BORDER_CENTER", "SET_BORDER_LERP_SIZE", "SET_BORDER_SIZE", "SET_BORDER_WARNING_DELAY", "SET_BORDER_WARNING_DISTANCE", "SET_CAMERA", "SET_CARRIED_ITEM", "SET_CHUNK_CACHE_CENTER", "SET_CHUNK_CACHE_RADIUS", "SET_DEFAULT_SPAWN_POSITION", "SET_DISPLAY_OBJECTIVE", "SET_ENTITY_DATA", "SET_ENTITY_LINK", "SET_ENTITY_MOTION", "SET_EQUIPMENT", "SET_EXPERIENCE", "SET_HEALTH", "SET_OBJECTIVE", "SET_PASSENGERS", "SET_PLAYER_TEAM", "SET_SCORE", "SET_SIMULATION_DISTANCE", "SET_SUBTITLE_TEXT", "SET_TIME", "SET_TITLE_TEXT", "SET_TITLES_ANIMATION", "SOUND_ENTITY", "SOUND", "START_CONFIGURATION", "STOP_SOUND", "SYSTEM_CHAT", "TAB_LIST", "TAG_QUERY", "TAKE_ITEM_ENTITY", "TELEPORT_ENTITY", "UPDATE_ADVANCEMENTS", "UPDATE_ATTRIBUTES", "UPDATE_MOB_EFFECT", "UPDATE_RECIPES", "UPDATE_TAGS"];
    const S_OLD: &[&str] = &["ACCEPT_TELEPORTATION", "BLOCK_ENTITY_TAG_QUERY", "CHANGE_DIFFICULTY", "CHAT_ACK", "CHAT_COMMAND", "CHAT", "CHAT_SESSION_UPDATE", "CLIENT_COMMAND", "CLIENT_INFORMATION", "COMMAND_SUGGESTION", "CONTAINER_BUTTON_CLICK", "CONTAINER_CLICK", "CONTAINER_CLOSE", "CUSTOM_PAYLOAD", "EDIT_BOOK", "ENTITY_TAG_QUERY", "INTERACT", "JIGSAW_GENERATE", "KEEP_ALIVE", "LOCK_DIFFICULTY", "MOVE_PLAYER_POS", "MOVE_PLAYER_POS_ROT", "MOVE_PLAYER_ROT", "MOVE_PLAYER_STATUS_ONLY", "MOVE_VEHICLE", "PADDLE_BOAT", "PICK_ITEM", "PLACE_RECIPE", "PLAYER_ABILITIES", "PLAYER_ACTION", "PLAYER_COMMAND", "PLAYER_INPUT", "PONG", "RECIPE_BOOK_CHANGE_SETTINGS", "RECIPE_BOOK_SEEN_RECIPE", "RENAME_ITEM", "RESOURCE_PACK", "SEEN_ADVANCEMENTS", "SELECT_TRADE", "SET_BEACON", "SET_CARRIED_ITEM", "SET_COMMAND_BLOCK", "SET_COMMAND_MINECART", "SET_CREATIVE_MODE_SLOT", "SET_JIGSAW_BLOCK", "SET_STRUCTURE_BLOCK", "SIGN_UPDATE", "SWING", "TELEPORT_TO_ENTITY", "USE_ITEM_ON", "USE_ITEM"];
    const S_NEW: &[&str] = &["ACCEPT_TELEPORTATION", "BLOCK_ENTITY_TAG_QUERY", "CHANGE_DIFFICULTY", "CHAT_ACK", "CHAT_COMMAND", "CHAT", "CHAT_SESSION_UPDATE", "CHUNK_BATCH_RECEIVED", "CLIENT_COMMAND", "CLIENT_INFORMATION", "COMMAND_SUGGESTION", "CONFIGURATION_ACKNOWLEDGED", "CONTAINER_BUTTON_CLICK", "CONTAINER_CLICK", "CONTAINER_CLOSE", "CUSTOM_PAYLOAD", "EDIT_BOOK", "ENTITY_TAG_QUERY", "INTERACT", "JIGSAW_GENERATE", "KEEP_ALIVE", "LOCK_DIFFICULTY", "MOVE_PLAYER_POS", "MOVE_PLAYER_POS_ROT", "MOVE_PLAYER_ROT", "MOVE_PLAYER_STATUS_ONLY", "MOVE_VEHICLE", "PADDLE_BOAT", "PICK_ITEM", "PING_REQUEST", "PLACE_RECIPE", "PLAYER_ABILITIES", "PLAYER_ACTION", "PLAYER_COMMAND", "PLAYER_INPUT", "PONG", "RECIPE_BOOK_CHANGE_SETTINGS", "RECIPE_BOOK_SEEN_RECIPE", "RENAME_ITEM", "RESOURCE_PACK", "SEEN_ADVANCEMENTS", "SELECT_TRADE", "SET_BEACON", "SET_CARRIED_ITEM", "SET_COMMAND_BLOCK", "SET_COMMAND_MINECART", "SET_CREATIVE_MODE_SLOT", "SET_JIGSAW_BLOCK", "SET_STRUCTURE_BLOCK", "SIGN_UPDATE", "SWING", "TELEPORT_TO_ENTITY", "USE_ITEM_ON", "USE_ITEM"];

    fn id(list: &[&str], name: &str) -> i32 {
        list.iter().position(|n| *n == name).unwrap_or_else(|| panic!("{name}")) as i32
    }

    /// Every packet maps to the one with the same name, and only packets the other side lacks
    /// have no mapping.
    #[test]
    fn id_maps_follow_the_packet_lists() {
        for (i, name) in C_NEW.iter().enumerate() {
            assert_eq!(play_clientbound(i as i32), C_OLD.iter().position(|n| n == name).map(|p| p as i32), "clientbound {name}");
        }
        assert_eq!(play_clientbound(C_NEW.len() as i32), None);
        for (i, name) in S_OLD.iter().enumerate() {
            assert_eq!(play_serverbound(i as i32), Some(id(S_NEW, name)), "serverbound {name}");
        }
        assert_eq!(play_serverbound(S_OLD.len() as i32), None);
    }

    #[test]
    fn named_ids_match_the_packet_lists() {
        let c_new = [
            (new::ADD_ENTITY, "ADD_ENTITY"),
            (new::BLOCK_ENTITY_DATA, "BLOCK_ENTITY_DATA"),
            (new::BLOCK_UPDATE, "BLOCK_UPDATE"),
            (new::CHUNK_BATCH_FINISHED, "CHUNK_BATCH_FINISHED"),
            (new::CHUNK_BATCH_START, "CHUNK_BATCH_START"),
            (new::CONTAINER_SET_CONTENT, "CONTAINER_SET_CONTENT"),
            (new::CONTAINER_SET_SLOT, "CONTAINER_SET_SLOT"),
            (new::DISCONNECT, "DISCONNECT"),
            (new::FORGET_LEVEL_CHUNK, "FORGET_LEVEL_CHUNK"),
            (new::LEVEL_CHUNK_WITH_LIGHT, "LEVEL_CHUNK_WITH_LIGHT"),
            (new::LEVEL_EVENT, "LEVEL_EVENT"),
            (new::LEVEL_PARTICLES, "LEVEL_PARTICLES"),
            (new::LOGIN, "LOGIN"),
            (new::MAP_ITEM_DATA, "MAP_ITEM_DATA"),
            (new::MERCHANT_OFFERS, "MERCHANT_OFFERS"),
            (new::PONG_RESPONSE, "PONG_RESPONSE"),
            (new::REMOVE_ENTITIES, "REMOVE_ENTITIES"),
            (new::REMOVE_MOB_EFFECT, "REMOVE_MOB_EFFECT"),
            (new::RESPAWN, "RESPAWN"),
            (new::SECTION_BLOCKS_UPDATE, "SECTION_BLOCKS_UPDATE"),
            (new::SET_DISPLAY_OBJECTIVE, "SET_DISPLAY_OBJECTIVE"),
            (new::SET_ENTITY_DATA, "SET_ENTITY_DATA"),
            (new::SET_EQUIPMENT, "SET_EQUIPMENT"),
            (new::SOUND_ENTITY, "SOUND_ENTITY"),
            (new::SOUND, "SOUND"),
            (new::START_CONFIGURATION, "START_CONFIGURATION"),
            (new::TAG_QUERY, "TAG_QUERY"),
            (new::UPDATE_ADVANCEMENTS, "UPDATE_ADVANCEMENTS"),
            (new::UPDATE_MOB_EFFECT, "UPDATE_MOB_EFFECT"),
            (new::UPDATE_RECIPES, "UPDATE_RECIPES"),
        ];
        for (v, name) in c_new {
            assert_eq!(v, id(C_NEW, name), "{name}");
        }
        let c_old = [
            (old::ADD_PLAYER, "ADD_PLAYER"),
            (old::CUSTOM_PAYLOAD, "CUSTOM_PAYLOAD"),
            (old::DISCONNECT, "DISCONNECT"),
            (old::LOGIN, "LOGIN"),
            (old::RESOURCE_PACK, "RESOURCE_PACK"),
            (old::ROTATE_HEAD, "ROTATE_HEAD"),
            (old::SET_ENTITY_MOTION, "SET_ENTITY_MOTION"),
            (old::UPDATE_ENABLED_FEATURES, "UPDATE_ENABLED_FEATURES"),
            (old::UPDATE_TAGS, "UPDATE_TAGS"),
        ];
        for (v, name) in c_old {
            assert_eq!(v, id(C_OLD, name), "{name}");
        }
        for (v, name) in [(new::S_CHUNK_BATCH_RECEIVED, "CHUNK_BATCH_RECEIVED"), (new::S_CLIENT_INFORMATION, "CLIENT_INFORMATION"), (new::S_CONFIGURATION_ACKNOWLEDGED, "CONFIGURATION_ACKNOWLEDGED")] {
            assert_eq!(v, id(S_NEW, name), "{name}");
        }
        for (v, name) in [
            (old::S_CLIENT_INFORMATION, "CLIENT_INFORMATION"),
            (old::S_CONTAINER_CLICK, "CONTAINER_CLICK"),
            (old::S_CUSTOM_PAYLOAD, "CUSTOM_PAYLOAD"),
            (old::S_SET_BEACON, "SET_BEACON"),
            (old::S_SET_CREATIVE_MODE_SLOT, "SET_CREATIVE_MODE_SLOT"),
        ] {
            assert_eq!(v, id(S_OLD, name), "{name}");
        }
        // configuration (ClientboundConfigurationPackets1_20_2 / ServerboundConfigurationPackets1_20_2)
        let c_cfg = ["CUSTOM_PAYLOAD", "DISCONNECT", "FINISH_CONFIGURATION", "KEEP_ALIVE", "PING", "REGISTRY_DATA", "RESOURCE_PACK", "UPDATE_ENABLED_FEATURES", "UPDATE_TAGS"];
        let named = [
            new::CONFIG_CUSTOM_PAYLOAD,
            new::CONFIG_DISCONNECT,
            new::CONFIG_FINISH,
            new::CONFIG_KEEP_ALIVE,
            new::CONFIG_PING,
            new::CONFIG_REGISTRY_DATA,
            new::CONFIG_RESOURCE_PACK,
            new::CONFIG_UPDATE_ENABLED_FEATURES,
            new::CONFIG_UPDATE_TAGS,
        ];
        assert_eq!(named.len(), c_cfg.len());
        for (i, v) in named.iter().enumerate() {
            assert_eq!(*v, i as i32, "{}", c_cfg[i]);
        }
        let s_cfg = [new::S_CONFIG_CLIENT_INFORMATION, new::S_CONFIG_CUSTOM_PAYLOAD, new::S_CONFIG_FINISH, new::S_CONFIG_KEEP_ALIVE, new::S_CONFIG_PONG, new::S_CONFIG_RESOURCE_PACK];
        for (i, v) in s_cfg.iter().enumerate() {
            assert_eq!(*v, i as i32);
        }
    }
}

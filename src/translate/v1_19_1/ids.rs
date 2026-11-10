//! Packet ids for 1.19.3 (protocol 761, server side of this step) and 1.19.1 / 1.19.2 (protocol
//! 760). Source: the vanilla packet lists for both versions (position = id).
//!
//! 1.19.3 dropped chat previews (CHAT_PREVIEW, SET_DISPLAY_CHAT_PREVIEW), the separate chat
//! header and the named sound packet, split PLAYER_INFO into an update and a remove packet, and
//! added DISGUISED_CHAT, UPDATE_ENABLED_FEATURES and (serverbound) CHAT_SESSION_UPDATE.

pub mod new {
    // clientbound play (1.19.3)
    pub const ADD_ENTITY: i32 = 0;
    pub const AWARD_STATS: i32 = 4;
    pub const BLOCK_ENTITY_DATA: i32 = 7;
    pub const BLOCK_EVENT: i32 = 8;
    pub const BLOCK_UPDATE: i32 = 9;
    pub const COMMANDS: i32 = 14;
    pub const CONTAINER_SET_CONTENT: i32 = 16;
    pub const CONTAINER_SET_SLOT: i32 = 18;
    pub const COOLDOWN: i32 = 19;
    pub const DELETE_CHAT: i32 = 22;
    pub const DISGUISED_CHAT: i32 = 24;
    pub const EXPLODE: i32 = 26;
    pub const LEVEL_CHUNK_WITH_LIGHT: i32 = 32;
    pub const LEVEL_EVENT: i32 = 33;
    pub const LEVEL_PARTICLES: i32 = 34;
    pub const LOGIN: i32 = 36;
    pub const MERCHANT_OFFERS: i32 = 38;
    pub const PLAYER_CHAT: i32 = 49;
    pub const PLAYER_INFO_REMOVE: i32 = 53;
    pub const PLAYER_INFO_UPDATE: i32 = 54;
    pub const REMOVE_ENTITIES: i32 = 58;
    pub const RESPAWN: i32 = 61;
    pub const SECTION_BLOCKS_UPDATE: i32 = 63;
    pub const SERVER_DATA: i32 = 65;
    pub const SET_ENTITY_DATA: i32 = 78;
    pub const SET_EQUIPMENT: i32 = 81;
    pub const SOUND_ENTITY: i32 = 93;
    pub const SOUND: i32 = 94;
    pub const STOP_SOUND: i32 = 95;
    pub const UPDATE_ADVANCEMENTS: i32 = 101;
    pub const UPDATE_ENABLED_FEATURES: i32 = 103;
    pub const UPDATE_RECIPES: i32 = 105;
    pub const UPDATE_TAGS: i32 = 106;
    // serverbound play (1.19.3)
    pub const S_CHAT_COMMAND: i32 = 4;
    pub const S_CHAT: i32 = 5;
}

pub mod old {
    // clientbound play (1.19.1)
    pub const CUSTOM_SOUND: i32 = 23;
    pub const PLAYER_INFO: i32 = 55;
    pub const SOUND_ENTITY: i32 = 95;
    pub const SOUND: i32 = 96;
    pub const SYSTEM_CHAT: i32 = 98;
    // serverbound play (1.19.1)
    pub const S_CHAT_ACK: i32 = 3;
    pub const S_CHAT_COMMAND: i32 = 4;
    pub const S_CHAT: i32 = 5;
    pub const S_CHAT_PREVIEW: i32 = 6;
    pub const S_CONTAINER_CLICK: i32 = 11;
    pub const S_SET_CREATIVE_MODE_SLOT: i32 = 43;
}

/// Clientbound play id 1.19.3 → 1.19.1. `None` = the 1.19.1 client has no such packet.
pub fn play_clientbound(id: i32) -> Option<i32> {
    match id {
        0..=11 => Some(id),
        12..=21 => Some(id + 1),   // CHAT_PREVIEW sits at 12 in 1.19.1
        22..=23 => Some(id + 2),   // CUSTOM_SOUND at 23
        24 => None,                // DISGUISED_CHAT
        25..=48 => Some(id + 1),   // one new packet (DISGUISED_CHAT) for two old ones
        49..=52 => Some(id + 2),   // PLAYER_CHAT_HEADER at 50
        53 | 54 => Some(55),       // both halves of PLAYER_INFO
        55..=76 => Some(id + 1),
        77..=102 => Some(id + 2),  // SET_DISPLAY_CHAT_PREVIEW at 78
        103 => None,               // UPDATE_ENABLED_FEATURES
        104..=106 => Some(id + 1),
        _ => None,
    }
}

/// Serverbound play id 1.19.1 → 1.19.3. `None` = the 1.19.3 server has no such packet.
pub fn play_serverbound(id: i32) -> Option<i32> {
    match id {
        0..=5 => Some(id),
        6 => None,                // CHAT_PREVIEW
        7..=32 => Some(id - 1),
        33..=50 => Some(id),      // CHAT_SESSION_UPDATE inserted at 32
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Packet lists in id order (ViaVersion's ClientboundPackets1_19_1 / 1_19_3 and
    // ServerboundPackets1_19_1 / 1_19_3, under the vanilla names).
    const C_OLD: &[&str] = &["ADD_ENTITY", "ADD_EXPERIENCE_ORB", "ADD_PLAYER", "ANIMATE", "AWARD_STATS", "BLOCK_CHANGED_ACK", "BLOCK_DESTRUCTION", "BLOCK_ENTITY_DATA", "BLOCK_EVENT", "BLOCK_UPDATE", "BOSS_EVENT", "CHANGE_DIFFICULTY", "CHAT_PREVIEW", "CLEAR_TITLES", "COMMAND_SUGGESTIONS", "COMMANDS", "CONTAINER_CLOSE", "CONTAINER_SET_CONTENT", "CONTAINER_SET_DATA", "CONTAINER_SET_SLOT", "COOLDOWN", "CUSTOM_CHAT_COMPLETIONS", "CUSTOM_PAYLOAD", "CUSTOM_SOUND", "DELETE_CHAT", "DISCONNECT", "ENTITY_EVENT", "EXPLODE", "FORGET_LEVEL_CHUNK", "GAME_EVENT", "HORSE_SCREEN_OPEN", "INITIALIZE_BORDER", "KEEP_ALIVE", "LEVEL_CHUNK_WITH_LIGHT", "LEVEL_EVENT", "LEVEL_PARTICLES", "LIGHT_UPDATE", "LOGIN", "MAP_ITEM_DATA", "MERCHANT_OFFERS", "MOVE_ENTITY_POS", "MOVE_ENTITY_POS_ROT", "MOVE_ENTITY_ROT", "MOVE_VEHICLE", "OPEN_BOOK", "OPEN_SCREEN", "OPEN_SIGN_EDITOR", "PING", "PLACE_GHOST_RECIPE", "PLAYER_ABILITIES", "PLAYER_CHAT_HEADER", "PLAYER_CHAT", "PLAYER_COMBAT_END", "PLAYER_COMBAT_ENTER", "PLAYER_COMBAT_KILL", "PLAYER_INFO", "PLAYER_LOOK_AT", "PLAYER_POSITION", "RECIPE", "REMOVE_ENTITIES", "REMOVE_MOB_EFFECT", "RESOURCE_PACK", "RESPAWN", "ROTATE_HEAD", "SECTION_BLOCKS_UPDATE", "SELECT_ADVANCEMENTS_TAB", "SERVER_DATA", "SET_ACTION_BAR_TEXT", "SET_BORDER_CENTER", "SET_BORDER_LERP_SIZE", "SET_BORDER_SIZE", "SET_BORDER_WARNING_DELAY", "SET_BORDER_WARNING_DISTANCE", "SET_CAMERA", "SET_CARRIED_ITEM", "SET_CHUNK_CACHE_CENTER", "SET_CHUNK_CACHE_RADIUS", "SET_DEFAULT_SPAWN_POSITION", "SET_DISPLAY_CHAT_PREVIEW", "SET_DISPLAY_OBJECTIVE", "SET_ENTITY_DATA", "SET_ENTITY_LINK", "SET_ENTITY_MOTION", "SET_EQUIPMENT", "SET_EXPERIENCE", "SET_HEALTH", "SET_OBJECTIVE", "SET_PASSENGERS", "SET_PLAYER_TEAM", "SET_SCORE", "SET_SIMULATION_DISTANCE", "SET_SUBTITLE_TEXT", "SET_TIME", "SET_TITLE_TEXT", "SET_TITLES_ANIMATION", "SOUND_ENTITY", "SOUND", "STOP_SOUND", "SYSTEM_CHAT", "TAB_LIST", "TAG_QUERY", "TAKE_ITEM_ENTITY", "TELEPORT_ENTITY", "UPDATE_ADVANCEMENTS", "UPDATE_ATTRIBUTES", "UPDATE_MOB_EFFECT", "UPDATE_RECIPES", "UPDATE_TAGS"];
    const C_NEW: &[&str] = &["ADD_ENTITY", "ADD_EXPERIENCE_ORB", "ADD_PLAYER", "ANIMATE", "AWARD_STATS", "BLOCK_CHANGED_ACK", "BLOCK_DESTRUCTION", "BLOCK_ENTITY_DATA", "BLOCK_EVENT", "BLOCK_UPDATE", "BOSS_EVENT", "CHANGE_DIFFICULTY", "CLEAR_TITLES", "COMMAND_SUGGESTIONS", "COMMANDS", "CONTAINER_CLOSE", "CONTAINER_SET_CONTENT", "CONTAINER_SET_DATA", "CONTAINER_SET_SLOT", "COOLDOWN", "CUSTOM_CHAT_COMPLETIONS", "CUSTOM_PAYLOAD", "DELETE_CHAT", "DISCONNECT", "DISGUISED_CHAT", "ENTITY_EVENT", "EXPLODE", "FORGET_LEVEL_CHUNK", "GAME_EVENT", "HORSE_SCREEN_OPEN", "INITIALIZE_BORDER", "KEEP_ALIVE", "LEVEL_CHUNK_WITH_LIGHT", "LEVEL_EVENT", "LEVEL_PARTICLES", "LIGHT_UPDATE", "LOGIN", "MAP_ITEM_DATA", "MERCHANT_OFFERS", "MOVE_ENTITY_POS", "MOVE_ENTITY_POS_ROT", "MOVE_ENTITY_ROT", "MOVE_VEHICLE", "OPEN_BOOK", "OPEN_SCREEN", "OPEN_SIGN_EDITOR", "PING", "PLACE_GHOST_RECIPE", "PLAYER_ABILITIES", "PLAYER_CHAT", "PLAYER_COMBAT_END", "PLAYER_COMBAT_ENTER", "PLAYER_COMBAT_KILL", "PLAYER_INFO_REMOVE", "PLAYER_INFO_UPDATE", "PLAYER_LOOK_AT", "PLAYER_POSITION", "RECIPE", "REMOVE_ENTITIES", "REMOVE_MOB_EFFECT", "RESOURCE_PACK", "RESPAWN", "ROTATE_HEAD", "SECTION_BLOCKS_UPDATE", "SELECT_ADVANCEMENTS_TAB", "SERVER_DATA", "SET_ACTION_BAR_TEXT", "SET_BORDER_CENTER", "SET_BORDER_LERP_SIZE", "SET_BORDER_SIZE", "SET_BORDER_WARNING_DELAY", "SET_BORDER_WARNING_DISTANCE", "SET_CAMERA", "SET_CARRIED_ITEM", "SET_CHUNK_CACHE_CENTER", "SET_CHUNK_CACHE_RADIUS", "SET_DEFAULT_SPAWN_POSITION", "SET_DISPLAY_OBJECTIVE", "SET_ENTITY_DATA", "SET_ENTITY_LINK", "SET_ENTITY_MOTION", "SET_EQUIPMENT", "SET_EXPERIENCE", "SET_HEALTH", "SET_OBJECTIVE", "SET_PASSENGERS", "SET_PLAYER_TEAM", "SET_SCORE", "SET_SIMULATION_DISTANCE", "SET_SUBTITLE_TEXT", "SET_TIME", "SET_TITLE_TEXT", "SET_TITLES_ANIMATION", "SOUND_ENTITY", "SOUND", "STOP_SOUND", "SYSTEM_CHAT", "TAB_LIST", "TAG_QUERY", "TAKE_ITEM_ENTITY", "TELEPORT_ENTITY", "UPDATE_ADVANCEMENTS", "UPDATE_ATTRIBUTES", "UPDATE_ENABLED_FEATURES", "UPDATE_MOB_EFFECT", "UPDATE_RECIPES", "UPDATE_TAGS"];
    const S_OLD: &[&str] = &["ACCEPT_TELEPORTATION", "BLOCK_ENTITY_TAG_QUERY", "CHANGE_DIFFICULTY", "CHAT_ACK", "CHAT_COMMAND", "CHAT", "CHAT_PREVIEW", "CLIENT_COMMAND", "CLIENT_INFORMATION", "COMMAND_SUGGESTION", "CONTAINER_BUTTON_CLICK", "CONTAINER_CLICK", "CONTAINER_CLOSE", "CUSTOM_PAYLOAD", "EDIT_BOOK", "ENTITY_TAG_QUERY", "INTERACT", "JIGSAW_GENERATE", "KEEP_ALIVE", "LOCK_DIFFICULTY", "MOVE_PLAYER_POS", "MOVE_PLAYER_POS_ROT", "MOVE_PLAYER_ROT", "MOVE_PLAYER_STATUS_ONLY", "MOVE_VEHICLE", "PADDLE_BOAT", "PICK_ITEM", "PLACE_RECIPE", "PLAYER_ABILITIES", "PLAYER_ACTION", "PLAYER_COMMAND", "PLAYER_INPUT", "PONG", "RECIPE_BOOK_CHANGE_SETTINGS", "RECIPE_BOOK_SEEN_RECIPE", "RENAME_ITEM", "RESOURCE_PACK", "SEEN_ADVANCEMENTS", "SELECT_TRADE", "SET_BEACON", "SET_CARRIED_ITEM", "SET_COMMAND_BLOCK", "SET_COMMAND_MINECART", "SET_CREATIVE_MODE_SLOT", "SET_JIGSAW_BLOCK", "SET_STRUCTURE_BLOCK", "SIGN_UPDATE", "SWING", "TELEPORT_TO_ENTITY", "USE_ITEM_ON", "USE_ITEM"];
    const S_NEW: &[&str] = &["ACCEPT_TELEPORTATION", "BLOCK_ENTITY_TAG_QUERY", "CHANGE_DIFFICULTY", "CHAT_ACK", "CHAT_COMMAND", "CHAT", "CLIENT_COMMAND", "CLIENT_INFORMATION", "COMMAND_SUGGESTION", "CONTAINER_BUTTON_CLICK", "CONTAINER_CLICK", "CONTAINER_CLOSE", "CUSTOM_PAYLOAD", "EDIT_BOOK", "ENTITY_TAG_QUERY", "INTERACT", "JIGSAW_GENERATE", "KEEP_ALIVE", "LOCK_DIFFICULTY", "MOVE_PLAYER_POS", "MOVE_PLAYER_POS_ROT", "MOVE_PLAYER_ROT", "MOVE_PLAYER_STATUS_ONLY", "MOVE_VEHICLE", "PADDLE_BOAT", "PICK_ITEM", "PLACE_RECIPE", "PLAYER_ABILITIES", "PLAYER_ACTION", "PLAYER_COMMAND", "PLAYER_INPUT", "PONG", "CHAT_SESSION_UPDATE", "RECIPE_BOOK_CHANGE_SETTINGS", "RECIPE_BOOK_SEEN_RECIPE", "RENAME_ITEM", "RESOURCE_PACK", "SEEN_ADVANCEMENTS", "SELECT_TRADE", "SET_BEACON", "SET_CARRIED_ITEM", "SET_COMMAND_BLOCK", "SET_COMMAND_MINECART", "SET_CREATIVE_MODE_SLOT", "SET_JIGSAW_BLOCK", "SET_STRUCTURE_BLOCK", "SIGN_UPDATE", "SWING", "TELEPORT_TO_ENTITY", "USE_ITEM_ON", "USE_ITEM"];

    fn id(list: &[&str], name: &str) -> i32 {
        list.iter().position(|n| *n == name).unwrap_or_else(|| panic!("{name}")) as i32
    }

    /// Every packet maps to the one with the same name; the two halves of PLAYER_INFO map to
    /// it, and only packets the other side lacks have no mapping.
    #[test]
    fn id_maps_follow_the_packet_lists() {
        assert_eq!((C_OLD.len(), C_NEW.len(), S_OLD.len(), S_NEW.len()), (108, 107, 51, 51));
        for (i, name) in C_NEW.iter().enumerate() {
            let want = match *name {
                "PLAYER_INFO_REMOVE" | "PLAYER_INFO_UPDATE" => Some(id(C_OLD, "PLAYER_INFO")),
                _ => C_OLD.iter().position(|n| n == name).map(|p| p as i32),
            };
            assert_eq!(play_clientbound(i as i32), want, "clientbound {name}");
        }
        assert_eq!(play_clientbound(C_NEW.len() as i32), None);
        assert_eq!(play_clientbound(-1), None);
        for (i, name) in S_OLD.iter().enumerate() {
            assert_eq!(play_serverbound(i as i32), S_NEW.iter().position(|n| n == name).map(|p| p as i32), "serverbound {name}");
        }
        assert_eq!(play_serverbound(S_OLD.len() as i32), None);
        assert_eq!(play_serverbound(-1), None);
    }

    #[test]
    fn named_ids_match_the_packet_lists() {
        let c_new = [
            (new::ADD_ENTITY, "ADD_ENTITY"),
            (new::AWARD_STATS, "AWARD_STATS"),
            (new::BLOCK_ENTITY_DATA, "BLOCK_ENTITY_DATA"),
            (new::BLOCK_EVENT, "BLOCK_EVENT"),
            (new::BLOCK_UPDATE, "BLOCK_UPDATE"),
            (new::COMMANDS, "COMMANDS"),
            (new::CONTAINER_SET_CONTENT, "CONTAINER_SET_CONTENT"),
            (new::CONTAINER_SET_SLOT, "CONTAINER_SET_SLOT"),
            (new::COOLDOWN, "COOLDOWN"),
            (new::DELETE_CHAT, "DELETE_CHAT"),
            (new::DISGUISED_CHAT, "DISGUISED_CHAT"),
            (new::EXPLODE, "EXPLODE"),
            (new::LEVEL_CHUNK_WITH_LIGHT, "LEVEL_CHUNK_WITH_LIGHT"),
            (new::LEVEL_EVENT, "LEVEL_EVENT"),
            (new::LEVEL_PARTICLES, "LEVEL_PARTICLES"),
            (new::LOGIN, "LOGIN"),
            (new::MERCHANT_OFFERS, "MERCHANT_OFFERS"),
            (new::PLAYER_CHAT, "PLAYER_CHAT"),
            (new::PLAYER_INFO_REMOVE, "PLAYER_INFO_REMOVE"),
            (new::PLAYER_INFO_UPDATE, "PLAYER_INFO_UPDATE"),
            (new::REMOVE_ENTITIES, "REMOVE_ENTITIES"),
            (new::RESPAWN, "RESPAWN"),
            (new::SECTION_BLOCKS_UPDATE, "SECTION_BLOCKS_UPDATE"),
            (new::SERVER_DATA, "SERVER_DATA"),
            (new::SET_ENTITY_DATA, "SET_ENTITY_DATA"),
            (new::SET_EQUIPMENT, "SET_EQUIPMENT"),
            (new::SOUND_ENTITY, "SOUND_ENTITY"),
            (new::SOUND, "SOUND"),
            (new::STOP_SOUND, "STOP_SOUND"),
            (new::UPDATE_ADVANCEMENTS, "UPDATE_ADVANCEMENTS"),
            (new::UPDATE_ENABLED_FEATURES, "UPDATE_ENABLED_FEATURES"),
            (new::UPDATE_RECIPES, "UPDATE_RECIPES"),
            (new::UPDATE_TAGS, "UPDATE_TAGS"),
        ];
        for (v, name) in c_new {
            assert_eq!(v, id(C_NEW, name), "{name}");
        }
        let c_old = [(old::CUSTOM_SOUND, "CUSTOM_SOUND"), (old::PLAYER_INFO, "PLAYER_INFO"), (old::SOUND_ENTITY, "SOUND_ENTITY"), (old::SOUND, "SOUND"), (old::SYSTEM_CHAT, "SYSTEM_CHAT")];
        for (v, name) in c_old {
            assert_eq!(v, id(C_OLD, name), "{name}");
        }
        for (v, name) in [(new::S_CHAT_COMMAND, "CHAT_COMMAND"), (new::S_CHAT, "CHAT")] {
            assert_eq!(v, id(S_NEW, name), "{name}");
        }
        for (v, name) in [
            (old::S_CHAT_ACK, "CHAT_ACK"),
            (old::S_CHAT_COMMAND, "CHAT_COMMAND"),
            (old::S_CHAT, "CHAT"),
            (old::S_CHAT_PREVIEW, "CHAT_PREVIEW"),
            (old::S_CONTAINER_CLICK, "CONTAINER_CLICK"),
            (old::S_SET_CREATIVE_MODE_SLOT, "SET_CREATIVE_MODE_SLOT"),
        ] {
            assert_eq!(v, id(S_OLD, name), "{name}");
        }
    }
}

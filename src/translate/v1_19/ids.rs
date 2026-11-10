//! Packet ids for 1.19.1 / 1.19.2 (protocol 760, server side of this step) and 1.19 (protocol
//! 759). Source: the packet lists for both versions (position = id).
//!
//! 1.19.1 added three clientbound chat packets (custom chat completions, delete chat, player chat
//! header) and one serverbound (chat acknowledgement); everything else only moved.

pub mod new {
    // clientbound play (1.19.1)
    pub const ADD_ENTITY: i32 = 0;
    pub const ADD_EXPERIENCE_ORB: i32 = 1;
    pub const ADD_PLAYER: i32 = 2;
    pub const CUSTOM_CHAT_COMPLETIONS: i32 = 21;
    pub const DELETE_CHAT: i32 = 24;
    pub const LOGIN: i32 = 37;
    pub const PLAYER_CHAT_HEADER: i32 = 50;
    pub const PLAYER_CHAT: i32 = 51;
    pub const REMOVE_ENTITIES: i32 = 59;
    pub const RESPAWN: i32 = 62;
    pub const SERVER_DATA: i32 = 66;
    pub const SET_ENTITY_DATA: i32 = 80;
    pub const SYSTEM_CHAT: i32 = 98;
    // serverbound play (1.19.1)
    pub const S_CHAT_ACK: i32 = 3;
    pub const S_CHAT_COMMAND: i32 = 4;
    pub const S_CHAT: i32 = 5;
}

pub mod old {
    // clientbound play (1.19)
    pub const SYSTEM_CHAT: i32 = 95;
    // serverbound play (1.19)
    pub const S_CHAT_COMMAND: i32 = 3;
    pub const S_CHAT: i32 = 4;
}

/// Clientbound play id 1.19.1 → 1.19. `None` = the 1.19 client has no such packet.
pub fn play_clientbound(id: i32) -> Option<i32> {
    match id {
        0..=20 => Some(id),
        22..=23 => Some(id - 1),   // after CUSTOM_CHAT_COMPLETIONS
        25..=49 => Some(id - 2),   // after DELETE_CHAT
        51..=107 => Some(id - 3),  // after PLAYER_CHAT_HEADER
        _ => None,
    }
}

/// Serverbound play id 1.19 → 1.19.1.
pub fn play_serverbound(id: i32) -> Option<i32> {
    match id {
        0..=2 => Some(id),
        3..=49 => Some(id + 1), // CHAT_ACK inserted at 3
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Packet lists in id order (ViaVersion's ClientboundPackets1_19_1 / 1_19 and
    // ServerboundPackets1_19_1 / 1_19, under the names those lists use).
    const C_NEW: &[&str] = &["SPAWN_ENTITY", "SPAWN_EXPERIENCE_ORB", "SPAWN_PLAYER", "ENTITY_ANIMATION", "STATISTICS", "BLOCK_CHANGED_ACK", "BLOCK_BREAK_ANIMATION", "BLOCK_ENTITY_DATA", "BLOCK_ACTION", "BLOCK_CHANGE", "BOSSBAR", "SERVER_DIFFICULTY", "CHAT_PREVIEW", "CLEAR_TITLES", "TAB_COMPLETE", "DECLARE_COMMANDS", "CLOSE_WINDOW", "WINDOW_ITEMS", "WINDOW_PROPERTY", "SET_SLOT", "COOLDOWN", "CUSTOM_CHAT_COMPLETIONS", "PLUGIN_MESSAGE", "NAMED_SOUND", "DELETE_CHAT_MESSAGE", "DISCONNECT", "ENTITY_STATUS", "EXPLOSION", "UNLOAD_CHUNK", "GAME_EVENT", "OPEN_HORSE_WINDOW", "WORLD_BORDER_INIT", "KEEP_ALIVE", "CHUNK_DATA", "EFFECT", "SPAWN_PARTICLE", "UPDATE_LIGHT", "JOIN_GAME", "MAP_DATA", "TRADE_LIST", "ENTITY_POSITION", "ENTITY_POSITION_AND_ROTATION", "ENTITY_ROTATION", "VEHICLE_MOVE", "OPEN_BOOK", "OPEN_WINDOW", "OPEN_SIGN_EDITOR", "PING", "CRAFT_RECIPE_RESPONSE", "PLAYER_ABILITIES", "PLAYER_CHAT_HEADER", "PLAYER_CHAT", "COMBAT_END", "COMBAT_ENTER", "COMBAT_KILL", "PLAYER_INFO", "FACE_PLAYER", "PLAYER_POSITION", "UNLOCK_RECIPES", "REMOVE_ENTITIES", "REMOVE_ENTITY_EFFECT", "RESOURCE_PACK", "RESPAWN", "ENTITY_HEAD_LOOK", "MULTI_BLOCK_CHANGE", "SELECT_ADVANCEMENTS_TAB", "SERVER_DATA", "ACTIONBAR", "WORLD_BORDER_CENTER", "WORLD_BORDER_LERP_SIZE", "WORLD_BORDER_SIZE", "WORLD_BORDER_WARNING_DELAY", "WORLD_BORDER_WARNING_DISTANCE", "CAMERA", "HELD_ITEM_CHANGE", "UPDATE_VIEW_POSITION", "UPDATE_VIEW_DISTANCE", "SPAWN_POSITION", "SET_DISPLAY_CHAT_PREVIEW", "DISPLAY_SCOREBOARD", "ENTITY_METADATA", "ATTACH_ENTITY", "ENTITY_VELOCITY", "ENTITY_EQUIPMENT", "SET_EXPERIENCE", "UPDATE_HEALTH", "SCOREBOARD_OBJECTIVE", "SET_PASSENGERS", "TEAMS", "UPDATE_SCORE", "SET_SIMULATION_DISTANCE", "TITLE_SUBTITLE", "TIME_UPDATE", "TITLE_TEXT", "TITLE_TIMES", "ENTITY_SOUND", "SOUND", "STOP_SOUND", "SYSTEM_CHAT", "TAB_LIST", "NBT_QUERY", "COLLECT_ITEM", "ENTITY_TELEPORT", "ADVANCEMENTS", "ENTITY_PROPERTIES", "ENTITY_EFFECT", "DECLARE_RECIPES", "TAGS"];
    const C_OLD: &[&str] = &["SPAWN_ENTITY", "SPAWN_EXPERIENCE_ORB", "SPAWN_PLAYER", "ENTITY_ANIMATION", "STATISTICS", "BLOCK_CHANGED_ACK", "BLOCK_BREAK_ANIMATION", "BLOCK_ENTITY_DATA", "BLOCK_ACTION", "BLOCK_CHANGE", "BOSSBAR", "SERVER_DIFFICULTY", "CHAT_PREVIEW", "CLEAR_TITLES", "TAB_COMPLETE", "DECLARE_COMMANDS", "CLOSE_WINDOW", "WINDOW_ITEMS", "WINDOW_PROPERTY", "SET_SLOT", "COOLDOWN", "PLUGIN_MESSAGE", "NAMED_SOUND", "DISCONNECT", "ENTITY_STATUS", "EXPLOSION", "UNLOAD_CHUNK", "GAME_EVENT", "OPEN_HORSE_WINDOW", "WORLD_BORDER_INIT", "KEEP_ALIVE", "CHUNK_DATA", "EFFECT", "SPAWN_PARTICLE", "UPDATE_LIGHT", "JOIN_GAME", "MAP_DATA", "TRADE_LIST", "ENTITY_POSITION", "ENTITY_POSITION_AND_ROTATION", "ENTITY_ROTATION", "VEHICLE_MOVE", "OPEN_BOOK", "OPEN_WINDOW", "OPEN_SIGN_EDITOR", "PING", "CRAFT_RECIPE_RESPONSE", "PLAYER_ABILITIES", "PLAYER_CHAT", "COMBAT_END", "COMBAT_ENTER", "COMBAT_KILL", "PLAYER_INFO", "FACE_PLAYER", "PLAYER_POSITION", "UNLOCK_RECIPES", "REMOVE_ENTITIES", "REMOVE_ENTITY_EFFECT", "RESOURCE_PACK", "RESPAWN", "ENTITY_HEAD_LOOK", "MULTI_BLOCK_CHANGE", "SELECT_ADVANCEMENTS_TAB", "SERVER_DATA", "ACTIONBAR", "WORLD_BORDER_CENTER", "WORLD_BORDER_LERP_SIZE", "WORLD_BORDER_SIZE", "WORLD_BORDER_WARNING_DELAY", "WORLD_BORDER_WARNING_DISTANCE", "CAMERA", "HELD_ITEM_CHANGE", "UPDATE_VIEW_POSITION", "UPDATE_VIEW_DISTANCE", "SPAWN_POSITION", "SET_DISPLAY_CHAT_PREVIEW", "DISPLAY_SCOREBOARD", "ENTITY_METADATA", "ATTACH_ENTITY", "ENTITY_VELOCITY", "ENTITY_EQUIPMENT", "SET_EXPERIENCE", "UPDATE_HEALTH", "SCOREBOARD_OBJECTIVE", "SET_PASSENGERS", "TEAMS", "UPDATE_SCORE", "SET_SIMULATION_DISTANCE", "TITLE_SUBTITLE", "TIME_UPDATE", "TITLE_TEXT", "TITLE_TIMES", "ENTITY_SOUND", "SOUND", "STOP_SOUND", "SYSTEM_CHAT", "TAB_LIST", "NBT_QUERY", "COLLECT_ITEM", "ENTITY_TELEPORT", "ADVANCEMENTS", "ENTITY_PROPERTIES", "ENTITY_EFFECT", "DECLARE_RECIPES", "TAGS"];
    const S_NEW: &[&str] = &["TELEPORT_CONFIRM", "QUERY_BLOCK_NBT", "SET_DIFFICULTY", "CHAT_ACK", "CHAT_COMMAND", "CHAT_MESSAGE", "CHAT_PREVIEW", "CLIENT_STATUS", "CLIENT_SETTINGS", "TAB_COMPLETE", "CLICK_WINDOW_BUTTON", "CLICK_WINDOW", "CLOSE_WINDOW", "PLUGIN_MESSAGE", "EDIT_BOOK", "ENTITY_NBT_REQUEST", "INTERACT_ENTITY", "GENERATE_JIGSAW", "KEEP_ALIVE", "LOCK_DIFFICULTY", "PLAYER_POSITION", "PLAYER_POSITION_AND_ROTATION", "PLAYER_ROTATION", "PLAYER_MOVEMENT", "VEHICLE_MOVE", "STEER_BOAT", "PICK_ITEM", "CRAFT_RECIPE_REQUEST", "PLAYER_ABILITIES", "PLAYER_DIGGING", "ENTITY_ACTION", "STEER_VEHICLE", "PONG", "RECIPE_BOOK_DATA", "SEEN_RECIPE", "RENAME_ITEM", "RESOURCE_PACK_STATUS", "ADVANCEMENT_TAB", "SELECT_TRADE", "SET_BEACON_EFFECT", "HELD_ITEM_CHANGE", "UPDATE_COMMAND_BLOCK", "UPDATE_COMMAND_BLOCK_MINECART", "CREATIVE_INVENTORY_ACTION", "UPDATE_JIGSAW_BLOCK", "UPDATE_STRUCTURE_BLOCK", "UPDATE_SIGN", "ANIMATION", "SPECTATE", "PLAYER_BLOCK_PLACEMENT", "USE_ITEM"];
    const S_OLD: &[&str] = &["TELEPORT_CONFIRM", "QUERY_BLOCK_NBT", "SET_DIFFICULTY", "CHAT_COMMAND", "CHAT_MESSAGE", "CHAT_PREVIEW", "CLIENT_STATUS", "CLIENT_SETTINGS", "TAB_COMPLETE", "CLICK_WINDOW_BUTTON", "CLICK_WINDOW", "CLOSE_WINDOW", "PLUGIN_MESSAGE", "EDIT_BOOK", "ENTITY_NBT_REQUEST", "INTERACT_ENTITY", "GENERATE_JIGSAW", "KEEP_ALIVE", "LOCK_DIFFICULTY", "PLAYER_POSITION", "PLAYER_POSITION_AND_ROTATION", "PLAYER_ROTATION", "PLAYER_MOVEMENT", "VEHICLE_MOVE", "STEER_BOAT", "PICK_ITEM", "CRAFT_RECIPE_REQUEST", "PLAYER_ABILITIES", "PLAYER_DIGGING", "ENTITY_ACTION", "STEER_VEHICLE", "PONG", "RECIPE_BOOK_DATA", "SEEN_RECIPE", "RENAME_ITEM", "RESOURCE_PACK_STATUS", "ADVANCEMENT_TAB", "SELECT_TRADE", "SET_BEACON_EFFECT", "HELD_ITEM_CHANGE", "UPDATE_COMMAND_BLOCK", "UPDATE_COMMAND_BLOCK_MINECART", "CREATIVE_INVENTORY_ACTION", "UPDATE_JIGSAW_BLOCK", "UPDATE_STRUCTURE_BLOCK", "UPDATE_SIGN", "ANIMATION", "SPECTATE", "PLAYER_BLOCK_PLACEMENT", "USE_ITEM"];

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
        assert_eq!(play_clientbound(-1), None);
        for (i, name) in S_OLD.iter().enumerate() {
            assert_eq!(play_serverbound(i as i32), Some(id(S_NEW, name)), "serverbound {name}");
        }
        assert_eq!(play_serverbound(S_OLD.len() as i32), None);
        assert_eq!(play_serverbound(-1), None);
    }

    #[test]
    fn named_ids_match_the_packet_lists() {
        let c_new = [
            (new::ADD_ENTITY, "SPAWN_ENTITY"),
            (new::ADD_EXPERIENCE_ORB, "SPAWN_EXPERIENCE_ORB"),
            (new::ADD_PLAYER, "SPAWN_PLAYER"),
            (new::CUSTOM_CHAT_COMPLETIONS, "CUSTOM_CHAT_COMPLETIONS"),
            (new::DELETE_CHAT, "DELETE_CHAT_MESSAGE"),
            (new::LOGIN, "JOIN_GAME"),
            (new::PLAYER_CHAT_HEADER, "PLAYER_CHAT_HEADER"),
            (new::PLAYER_CHAT, "PLAYER_CHAT"),
            (new::REMOVE_ENTITIES, "REMOVE_ENTITIES"),
            (new::RESPAWN, "RESPAWN"),
            (new::SERVER_DATA, "SERVER_DATA"),
            (new::SET_ENTITY_DATA, "ENTITY_METADATA"),
            (new::SYSTEM_CHAT, "SYSTEM_CHAT"),
        ];
        for (v, name) in c_new {
            assert_eq!(v, id(C_NEW, name), "{name}");
        }
        assert_eq!(old::SYSTEM_CHAT, id(C_OLD, "SYSTEM_CHAT"));
        for (v, name) in [(new::S_CHAT_ACK, "CHAT_ACK"), (new::S_CHAT_COMMAND, "CHAT_COMMAND"), (new::S_CHAT, "CHAT_MESSAGE")] {
            assert_eq!(v, id(S_NEW, name), "{name}");
        }
        for (v, name) in [(old::S_CHAT_COMMAND, "CHAT_COMMAND"), (old::S_CHAT, "CHAT_MESSAGE")] {
            assert_eq!(v, id(S_OLD, name), "{name}");
        }
    }
}

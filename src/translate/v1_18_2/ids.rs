//! Packet ids for 1.19 (protocol 759, server side of this step) and 1.18.2 (protocol 758; 1.18
//! and 1.18.1 use the same ids). Source: the packet lists for both versions (position = id).
//!
//! 1.19 folded the mob and painting spawn packets into ADD_ENTITY, replaced CHAT by PLAYER_CHAT
//! and SYSTEM_CHAT, replaced the block break acknowledgement by a sequence number, and added
//! SERVER_DATA and the chat preview packets. Serverbound, CHAT split in two and CHAT_PREVIEW
//! is new.
#![allow(dead_code)]

/// 1.19
pub mod new {
    // clientbound play
    pub const ADD_ENTITY: i32 = 0;
    pub const ADD_EXPERIENCE_ORB: i32 = 1;
    pub const ADD_PLAYER: i32 = 2;
    pub const ANIMATE: i32 = 3;
    pub const AWARD_STATS: i32 = 4;
    pub const BLOCK_CHANGED_ACK: i32 = 5;
    pub const BLOCK_DESTRUCTION: i32 = 6;
    pub const BLOCK_ENTITY_DATA: i32 = 7;
    pub const BLOCK_EVENT: i32 = 8;
    pub const BLOCK_UPDATE: i32 = 9;
    pub const BOSS_EVENT: i32 = 10;
    pub const CHANGE_DIFFICULTY: i32 = 11;
    pub const CHAT_PREVIEW: i32 = 12;
    pub const CLEAR_TITLES: i32 = 13;
    pub const COMMAND_SUGGESTIONS: i32 = 14;
    pub const COMMANDS: i32 = 15;
    pub const CONTAINER_CLOSE: i32 = 16;
    pub const CONTAINER_SET_CONTENT: i32 = 17;
    pub const CONTAINER_SET_DATA: i32 = 18;
    pub const CONTAINER_SET_SLOT: i32 = 19;
    pub const COOLDOWN: i32 = 20;
    pub const CUSTOM_PAYLOAD: i32 = 21;
    pub const CUSTOM_SOUND: i32 = 22;
    pub const DISCONNECT: i32 = 23;
    pub const ENTITY_EVENT: i32 = 24;
    pub const EXPLODE: i32 = 25;
    pub const FORGET_LEVEL_CHUNK: i32 = 26;
    pub const GAME_EVENT: i32 = 27;
    pub const HORSE_SCREEN_OPEN: i32 = 28;
    pub const INITIALIZE_BORDER: i32 = 29;
    pub const KEEP_ALIVE: i32 = 30;
    pub const LEVEL_CHUNK_WITH_LIGHT: i32 = 31;
    pub const LEVEL_EVENT: i32 = 32;
    pub const LEVEL_PARTICLES: i32 = 33;
    pub const LIGHT_UPDATE: i32 = 34;
    pub const LOGIN: i32 = 35;
    pub const MAP_ITEM_DATA: i32 = 36;
    pub const MERCHANT_OFFERS: i32 = 37;
    pub const MOVE_ENTITY_POS: i32 = 38;
    pub const MOVE_ENTITY_POS_ROT: i32 = 39;
    pub const MOVE_ENTITY_ROT: i32 = 40;
    pub const MOVE_VEHICLE: i32 = 41;
    pub const OPEN_BOOK: i32 = 42;
    pub const OPEN_SCREEN: i32 = 43;
    pub const OPEN_SIGN_EDITOR: i32 = 44;
    pub const PING: i32 = 45;
    pub const PLACE_GHOST_RECIPE: i32 = 46;
    pub const PLAYER_ABILITIES: i32 = 47;
    pub const PLAYER_CHAT: i32 = 48;
    pub const PLAYER_COMBAT_END: i32 = 49;
    pub const PLAYER_COMBAT_ENTER: i32 = 50;
    pub const PLAYER_COMBAT_KILL: i32 = 51;
    pub const PLAYER_INFO: i32 = 52;
    pub const PLAYER_LOOK_AT: i32 = 53;
    pub const PLAYER_POSITION: i32 = 54;
    pub const RECIPE: i32 = 55;
    pub const REMOVE_ENTITIES: i32 = 56;
    pub const REMOVE_MOB_EFFECT: i32 = 57;
    pub const RESOURCE_PACK: i32 = 58;
    pub const RESPAWN: i32 = 59;
    pub const ROTATE_HEAD: i32 = 60;
    pub const SECTION_BLOCKS_UPDATE: i32 = 61;
    pub const SELECT_ADVANCEMENTS_TAB: i32 = 62;
    pub const SERVER_DATA: i32 = 63;
    pub const SET_ACTION_BAR_TEXT: i32 = 64;
    pub const SET_BORDER_CENTER: i32 = 65;
    pub const SET_BORDER_LERP_SIZE: i32 = 66;
    pub const SET_BORDER_SIZE: i32 = 67;
    pub const SET_BORDER_WARNING_DELAY: i32 = 68;
    pub const SET_BORDER_WARNING_DISTANCE: i32 = 69;
    pub const SET_CAMERA: i32 = 70;
    pub const SET_CARRIED_ITEM: i32 = 71;
    pub const SET_CHUNK_CACHE_CENTER: i32 = 72;
    pub const SET_CHUNK_CACHE_RADIUS: i32 = 73;
    pub const SET_DEFAULT_SPAWN_POSITION: i32 = 74;
    pub const SET_DISPLAY_CHAT_PREVIEW: i32 = 75;
    pub const SET_DISPLAY_OBJECTIVE: i32 = 76;
    pub const SET_ENTITY_DATA: i32 = 77;
    pub const SET_ENTITY_LINK: i32 = 78;
    pub const SET_ENTITY_MOTION: i32 = 79;
    pub const SET_EQUIPMENT: i32 = 80;
    pub const SET_EXPERIENCE: i32 = 81;
    pub const SET_HEALTH: i32 = 82;
    pub const SET_OBJECTIVE: i32 = 83;
    pub const SET_PASSENGERS: i32 = 84;
    pub const SET_PLAYER_TEAM: i32 = 85;
    pub const SET_SCORE: i32 = 86;
    pub const SET_SIMULATION_DISTANCE: i32 = 87;
    pub const SET_SUBTITLE_TEXT: i32 = 88;
    pub const SET_TIME: i32 = 89;
    pub const SET_TITLE_TEXT: i32 = 90;
    pub const SET_TITLES_ANIMATION: i32 = 91;
    pub const SOUND_ENTITY: i32 = 92;
    pub const SOUND: i32 = 93;
    pub const STOP_SOUND: i32 = 94;
    pub const SYSTEM_CHAT: i32 = 95;
    pub const TAB_LIST: i32 = 96;
    pub const TAG_QUERY: i32 = 97;
    pub const TAKE_ITEM_ENTITY: i32 = 98;
    pub const TELEPORT_ENTITY: i32 = 99;
    pub const UPDATE_ADVANCEMENTS: i32 = 100;
    pub const UPDATE_ATTRIBUTES: i32 = 101;
    pub const UPDATE_MOB_EFFECT: i32 = 102;
    pub const UPDATE_RECIPES: i32 = 103;
    pub const UPDATE_TAGS: i32 = 104;
    // serverbound play
    pub const S_ACCEPT_TELEPORTATION: i32 = 0;
    pub const S_BLOCK_ENTITY_TAG_QUERY: i32 = 1;
    pub const S_CHANGE_DIFFICULTY: i32 = 2;
    pub const S_CHAT_COMMAND: i32 = 3;
    pub const S_CHAT: i32 = 4;
    pub const S_CHAT_PREVIEW: i32 = 5;
    pub const S_CLIENT_COMMAND: i32 = 6;
    pub const S_CLIENT_INFORMATION: i32 = 7;
    pub const S_COMMAND_SUGGESTION: i32 = 8;
    pub const S_CONTAINER_BUTTON_CLICK: i32 = 9;
    pub const S_CONTAINER_CLICK: i32 = 10;
    pub const S_CONTAINER_CLOSE: i32 = 11;
    pub const S_CUSTOM_PAYLOAD: i32 = 12;
    pub const S_EDIT_BOOK: i32 = 13;
    pub const S_ENTITY_TAG_QUERY: i32 = 14;
    pub const S_INTERACT: i32 = 15;
    pub const S_JIGSAW_GENERATE: i32 = 16;
    pub const S_KEEP_ALIVE: i32 = 17;
    pub const S_LOCK_DIFFICULTY: i32 = 18;
    pub const S_MOVE_PLAYER_POS: i32 = 19;
    pub const S_MOVE_PLAYER_POS_ROT: i32 = 20;
    pub const S_MOVE_PLAYER_ROT: i32 = 21;
    pub const S_MOVE_PLAYER_STATUS_ONLY: i32 = 22;
    pub const S_MOVE_VEHICLE: i32 = 23;
    pub const S_PADDLE_BOAT: i32 = 24;
    pub const S_PICK_ITEM: i32 = 25;
    pub const S_PLACE_RECIPE: i32 = 26;
    pub const S_PLAYER_ABILITIES: i32 = 27;
    pub const S_PLAYER_ACTION: i32 = 28;
    pub const S_PLAYER_COMMAND: i32 = 29;
    pub const S_PLAYER_INPUT: i32 = 30;
    pub const S_PONG: i32 = 31;
    pub const S_RECIPE_BOOK_CHANGE_SETTINGS: i32 = 32;
    pub const S_RECIPE_BOOK_SEEN_RECIPE: i32 = 33;
    pub const S_RENAME_ITEM: i32 = 34;
    pub const S_RESOURCE_PACK: i32 = 35;
    pub const S_SEEN_ADVANCEMENTS: i32 = 36;
    pub const S_SELECT_TRADE: i32 = 37;
    pub const S_SET_BEACON: i32 = 38;
    pub const S_SET_CARRIED_ITEM: i32 = 39;
    pub const S_SET_COMMAND_BLOCK: i32 = 40;
    pub const S_SET_COMMAND_MINECART: i32 = 41;
    pub const S_SET_CREATIVE_MODE_SLOT: i32 = 42;
    pub const S_SET_JIGSAW_BLOCK: i32 = 43;
    pub const S_SET_STRUCTURE_BLOCK: i32 = 44;
    pub const S_SIGN_UPDATE: i32 = 45;
    pub const S_SWING: i32 = 46;
    pub const S_TELEPORT_TO_ENTITY: i32 = 47;
    pub const S_USE_ITEM_ON: i32 = 48;
    pub const S_USE_ITEM: i32 = 49;
}

/// 1.18.2
pub mod old {
    // clientbound play
    pub const ADD_ENTITY: i32 = 0;
    pub const ADD_EXPERIENCE_ORB: i32 = 1;
    pub const ADD_MOB: i32 = 2;
    pub const ADD_PAINTING: i32 = 3;
    pub const ADD_PLAYER: i32 = 4;
    pub const ADD_VIBRATION_SIGNAL: i32 = 5;
    pub const ANIMATE: i32 = 6;
    pub const AWARD_STATS: i32 = 7;
    pub const BLOCK_BREAK_ACK: i32 = 8;
    pub const BLOCK_DESTRUCTION: i32 = 9;
    pub const BLOCK_ENTITY_DATA: i32 = 10;
    pub const BLOCK_EVENT: i32 = 11;
    pub const BLOCK_UPDATE: i32 = 12;
    pub const BOSS_EVENT: i32 = 13;
    pub const CHANGE_DIFFICULTY: i32 = 14;
    pub const CHAT: i32 = 15;
    pub const CLEAR_TITLES: i32 = 16;
    pub const COMMAND_SUGGESTIONS: i32 = 17;
    pub const COMMANDS: i32 = 18;
    pub const CONTAINER_CLOSE: i32 = 19;
    pub const CONTAINER_SET_CONTENT: i32 = 20;
    pub const CONTAINER_SET_DATA: i32 = 21;
    pub const CONTAINER_SET_SLOT: i32 = 22;
    pub const COOLDOWN: i32 = 23;
    pub const CUSTOM_PAYLOAD: i32 = 24;
    pub const CUSTOM_SOUND: i32 = 25;
    pub const DISCONNECT: i32 = 26;
    pub const ENTITY_EVENT: i32 = 27;
    pub const EXPLODE: i32 = 28;
    pub const FORGET_LEVEL_CHUNK: i32 = 29;
    pub const GAME_EVENT: i32 = 30;
    pub const HORSE_SCREEN_OPEN: i32 = 31;
    pub const INITIALIZE_BORDER: i32 = 32;
    pub const KEEP_ALIVE: i32 = 33;
    pub const LEVEL_CHUNK_WITH_LIGHT: i32 = 34;
    pub const LEVEL_EVENT: i32 = 35;
    pub const LEVEL_PARTICLES: i32 = 36;
    pub const LIGHT_UPDATE: i32 = 37;
    pub const LOGIN: i32 = 38;
    pub const MAP_ITEM_DATA: i32 = 39;
    pub const MERCHANT_OFFERS: i32 = 40;
    pub const MOVE_ENTITY_POS: i32 = 41;
    pub const MOVE_ENTITY_POS_ROT: i32 = 42;
    pub const MOVE_ENTITY_ROT: i32 = 43;
    pub const MOVE_VEHICLE: i32 = 44;
    pub const OPEN_BOOK: i32 = 45;
    pub const OPEN_SCREEN: i32 = 46;
    pub const OPEN_SIGN_EDITOR: i32 = 47;
    pub const PING: i32 = 48;
    pub const PLACE_GHOST_RECIPE: i32 = 49;
    pub const PLAYER_ABILITIES: i32 = 50;
    pub const PLAYER_COMBAT_END: i32 = 51;
    pub const PLAYER_COMBAT_ENTER: i32 = 52;
    pub const PLAYER_COMBAT_KILL: i32 = 53;
    pub const PLAYER_INFO: i32 = 54;
    pub const PLAYER_LOOK_AT: i32 = 55;
    pub const PLAYER_POSITION: i32 = 56;
    pub const RECIPE: i32 = 57;
    pub const REMOVE_ENTITIES: i32 = 58;
    pub const REMOVE_MOB_EFFECT: i32 = 59;
    pub const RESOURCE_PACK: i32 = 60;
    pub const RESPAWN: i32 = 61;
    pub const ROTATE_HEAD: i32 = 62;
    pub const SECTION_BLOCKS_UPDATE: i32 = 63;
    pub const SELECT_ADVANCEMENTS_TAB: i32 = 64;
    pub const SET_ACTION_BAR_TEXT: i32 = 65;
    pub const SET_BORDER_CENTER: i32 = 66;
    pub const SET_BORDER_LERP_SIZE: i32 = 67;
    pub const SET_BORDER_SIZE: i32 = 68;
    pub const SET_BORDER_WARNING_DELAY: i32 = 69;
    pub const SET_BORDER_WARNING_DISTANCE: i32 = 70;
    pub const SET_CAMERA: i32 = 71;
    pub const SET_CARRIED_ITEM: i32 = 72;
    pub const SET_CHUNK_CACHE_CENTER: i32 = 73;
    pub const SET_CHUNK_CACHE_RADIUS: i32 = 74;
    pub const SET_DEFAULT_SPAWN_POSITION: i32 = 75;
    pub const SET_DISPLAY_OBJECTIVE: i32 = 76;
    pub const SET_ENTITY_DATA: i32 = 77;
    pub const SET_ENTITY_LINK: i32 = 78;
    pub const SET_ENTITY_MOTION: i32 = 79;
    pub const SET_EQUIPMENT: i32 = 80;
    pub const SET_EXPERIENCE: i32 = 81;
    pub const SET_HEALTH: i32 = 82;
    pub const SET_OBJECTIVE: i32 = 83;
    pub const SET_PASSENGERS: i32 = 84;
    pub const SET_PLAYER_TEAM: i32 = 85;
    pub const SET_SCORE: i32 = 86;
    pub const SET_SIMULATION_DISTANCE: i32 = 87;
    pub const SET_SUBTITLE_TEXT: i32 = 88;
    pub const SET_TIME: i32 = 89;
    pub const SET_TITLE_TEXT: i32 = 90;
    pub const SET_TITLES_ANIMATION: i32 = 91;
    pub const SOUND_ENTITY: i32 = 92;
    pub const SOUND: i32 = 93;
    pub const STOP_SOUND: i32 = 94;
    pub const TAB_LIST: i32 = 95;
    pub const TAG_QUERY: i32 = 96;
    pub const TAKE_ITEM_ENTITY: i32 = 97;
    pub const TELEPORT_ENTITY: i32 = 98;
    pub const UPDATE_ADVANCEMENTS: i32 = 99;
    pub const UPDATE_ATTRIBUTES: i32 = 100;
    pub const UPDATE_MOB_EFFECT: i32 = 101;
    pub const UPDATE_RECIPES: i32 = 102;
    pub const UPDATE_TAGS: i32 = 103;
    // serverbound play
    pub const S_ACCEPT_TELEPORTATION: i32 = 0;
    pub const S_BLOCK_ENTITY_TAG_QUERY: i32 = 1;
    pub const S_CHANGE_DIFFICULTY: i32 = 2;
    pub const S_CHAT: i32 = 3;
    pub const S_CLIENT_COMMAND: i32 = 4;
    pub const S_CLIENT_INFORMATION: i32 = 5;
    pub const S_COMMAND_SUGGESTION: i32 = 6;
    pub const S_CONTAINER_BUTTON_CLICK: i32 = 7;
    pub const S_CONTAINER_CLICK: i32 = 8;
    pub const S_CONTAINER_CLOSE: i32 = 9;
    pub const S_CUSTOM_PAYLOAD: i32 = 10;
    pub const S_EDIT_BOOK: i32 = 11;
    pub const S_ENTITY_TAG_QUERY: i32 = 12;
    pub const S_INTERACT: i32 = 13;
    pub const S_JIGSAW_GENERATE: i32 = 14;
    pub const S_KEEP_ALIVE: i32 = 15;
    pub const S_LOCK_DIFFICULTY: i32 = 16;
    pub const S_MOVE_PLAYER_POS: i32 = 17;
    pub const S_MOVE_PLAYER_POS_ROT: i32 = 18;
    pub const S_MOVE_PLAYER_ROT: i32 = 19;
    pub const S_MOVE_PLAYER_STATUS_ONLY: i32 = 20;
    pub const S_MOVE_VEHICLE: i32 = 21;
    pub const S_PADDLE_BOAT: i32 = 22;
    pub const S_PICK_ITEM: i32 = 23;
    pub const S_PLACE_RECIPE: i32 = 24;
    pub const S_PLAYER_ABILITIES: i32 = 25;
    pub const S_PLAYER_ACTION: i32 = 26;
    pub const S_PLAYER_COMMAND: i32 = 27;
    pub const S_PLAYER_INPUT: i32 = 28;
    pub const S_PONG: i32 = 29;
    pub const S_RECIPE_BOOK_CHANGE_SETTINGS: i32 = 30;
    pub const S_RECIPE_BOOK_SEEN_RECIPE: i32 = 31;
    pub const S_RENAME_ITEM: i32 = 32;
    pub const S_RESOURCE_PACK: i32 = 33;
    pub const S_SEEN_ADVANCEMENTS: i32 = 34;
    pub const S_SELECT_TRADE: i32 = 35;
    pub const S_SET_BEACON: i32 = 36;
    pub const S_SET_CARRIED_ITEM: i32 = 37;
    pub const S_SET_COMMAND_BLOCK: i32 = 38;
    pub const S_SET_COMMAND_MINECART: i32 = 39;
    pub const S_SET_CREATIVE_MODE_SLOT: i32 = 40;
    pub const S_SET_JIGSAW_BLOCK: i32 = 41;
    pub const S_SET_STRUCTURE_BLOCK: i32 = 42;
    pub const S_SIGN_UPDATE: i32 = 43;
    pub const S_SWING: i32 = 44;
    pub const S_TELEPORT_TO_ENTITY: i32 = 45;
    pub const S_USE_ITEM_ON: i32 = 46;
    pub const S_USE_ITEM: i32 = 47;
}

/// 1.18.2 id of each 1.19 clientbound play packet; -1 where 1.18.2 has no such packet.
const CLIENTBOUND: [i8; 105] = [0, 1, 4, 6, 7, -1, 9, 10, 11, 12, 13, 14, -1, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, -1, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, -1, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, -1, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, -1, 95, 96, 97, 98, 99, 100, 101, 102, 103];
/// 1.19 id of each 1.18.2 serverbound play packet.
const SERVERBOUND: [u8; 48] = [0, 1, 2, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49];

/// Clientbound play id 1.19 → 1.18.2. `None` = the 1.18.2 client has no such packet.
pub fn play_clientbound(id: i32) -> Option<i32> {
    let mapped = *CLIENTBOUND.get(usize::try_from(id).ok()?)?;
    (mapped >= 0).then_some(i32::from(mapped))
}

/// Serverbound play id 1.18.2 → 1.19.
pub fn play_serverbound(id: i32) -> Option<i32> {
    SERVERBOUND.get(usize::try_from(id).ok()?).map(|v| i32::from(*v))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // Packet lists in id order (ViaVersion's ClientboundPackets1_19 / 1_18 and
    // ServerboundPackets1_19 / 1_17, which 1.18 still uses).
    pub const C_NEW: &[&str] = &["ADD_ENTITY", "ADD_EXPERIENCE_ORB", "ADD_PLAYER", "ANIMATE", "AWARD_STATS", "BLOCK_CHANGED_ACK", "BLOCK_DESTRUCTION", "BLOCK_ENTITY_DATA", "BLOCK_EVENT", "BLOCK_UPDATE", "BOSS_EVENT", "CHANGE_DIFFICULTY", "CHAT_PREVIEW", "CLEAR_TITLES", "COMMAND_SUGGESTIONS", "COMMANDS", "CONTAINER_CLOSE", "CONTAINER_SET_CONTENT", "CONTAINER_SET_DATA", "CONTAINER_SET_SLOT", "COOLDOWN", "CUSTOM_PAYLOAD", "CUSTOM_SOUND", "DISCONNECT", "ENTITY_EVENT", "EXPLODE", "FORGET_LEVEL_CHUNK", "GAME_EVENT", "HORSE_SCREEN_OPEN", "INITIALIZE_BORDER", "KEEP_ALIVE", "LEVEL_CHUNK_WITH_LIGHT", "LEVEL_EVENT", "LEVEL_PARTICLES", "LIGHT_UPDATE", "LOGIN", "MAP_ITEM_DATA", "MERCHANT_OFFERS", "MOVE_ENTITY_POS", "MOVE_ENTITY_POS_ROT", "MOVE_ENTITY_ROT", "MOVE_VEHICLE", "OPEN_BOOK", "OPEN_SCREEN", "OPEN_SIGN_EDITOR", "PING", "PLACE_GHOST_RECIPE", "PLAYER_ABILITIES", "PLAYER_CHAT", "PLAYER_COMBAT_END", "PLAYER_COMBAT_ENTER", "PLAYER_COMBAT_KILL", "PLAYER_INFO", "PLAYER_LOOK_AT", "PLAYER_POSITION", "RECIPE", "REMOVE_ENTITIES", "REMOVE_MOB_EFFECT", "RESOURCE_PACK", "RESPAWN", "ROTATE_HEAD", "SECTION_BLOCKS_UPDATE", "SELECT_ADVANCEMENTS_TAB", "SERVER_DATA", "SET_ACTION_BAR_TEXT", "SET_BORDER_CENTER", "SET_BORDER_LERP_SIZE", "SET_BORDER_SIZE", "SET_BORDER_WARNING_DELAY", "SET_BORDER_WARNING_DISTANCE", "SET_CAMERA", "SET_CARRIED_ITEM", "SET_CHUNK_CACHE_CENTER", "SET_CHUNK_CACHE_RADIUS", "SET_DEFAULT_SPAWN_POSITION", "SET_DISPLAY_CHAT_PREVIEW", "SET_DISPLAY_OBJECTIVE", "SET_ENTITY_DATA", "SET_ENTITY_LINK", "SET_ENTITY_MOTION", "SET_EQUIPMENT", "SET_EXPERIENCE", "SET_HEALTH", "SET_OBJECTIVE", "SET_PASSENGERS", "SET_PLAYER_TEAM", "SET_SCORE", "SET_SIMULATION_DISTANCE", "SET_SUBTITLE_TEXT", "SET_TIME", "SET_TITLE_TEXT", "SET_TITLES_ANIMATION", "SOUND_ENTITY", "SOUND", "STOP_SOUND", "SYSTEM_CHAT", "TAB_LIST", "TAG_QUERY", "TAKE_ITEM_ENTITY", "TELEPORT_ENTITY", "UPDATE_ADVANCEMENTS", "UPDATE_ATTRIBUTES", "UPDATE_MOB_EFFECT", "UPDATE_RECIPES", "UPDATE_TAGS"];
    pub const C_OLD: &[&str] = &["ADD_ENTITY", "ADD_EXPERIENCE_ORB", "ADD_MOB", "ADD_PAINTING", "ADD_PLAYER", "ADD_VIBRATION_SIGNAL", "ANIMATE", "AWARD_STATS", "BLOCK_BREAK_ACK", "BLOCK_DESTRUCTION", "BLOCK_ENTITY_DATA", "BLOCK_EVENT", "BLOCK_UPDATE", "BOSS_EVENT", "CHANGE_DIFFICULTY", "CHAT", "CLEAR_TITLES", "COMMAND_SUGGESTIONS", "COMMANDS", "CONTAINER_CLOSE", "CONTAINER_SET_CONTENT", "CONTAINER_SET_DATA", "CONTAINER_SET_SLOT", "COOLDOWN", "CUSTOM_PAYLOAD", "CUSTOM_SOUND", "DISCONNECT", "ENTITY_EVENT", "EXPLODE", "FORGET_LEVEL_CHUNK", "GAME_EVENT", "HORSE_SCREEN_OPEN", "INITIALIZE_BORDER", "KEEP_ALIVE", "LEVEL_CHUNK_WITH_LIGHT", "LEVEL_EVENT", "LEVEL_PARTICLES", "LIGHT_UPDATE", "LOGIN", "MAP_ITEM_DATA", "MERCHANT_OFFERS", "MOVE_ENTITY_POS", "MOVE_ENTITY_POS_ROT", "MOVE_ENTITY_ROT", "MOVE_VEHICLE", "OPEN_BOOK", "OPEN_SCREEN", "OPEN_SIGN_EDITOR", "PING", "PLACE_GHOST_RECIPE", "PLAYER_ABILITIES", "PLAYER_COMBAT_END", "PLAYER_COMBAT_ENTER", "PLAYER_COMBAT_KILL", "PLAYER_INFO", "PLAYER_LOOK_AT", "PLAYER_POSITION", "RECIPE", "REMOVE_ENTITIES", "REMOVE_MOB_EFFECT", "RESOURCE_PACK", "RESPAWN", "ROTATE_HEAD", "SECTION_BLOCKS_UPDATE", "SELECT_ADVANCEMENTS_TAB", "SET_ACTION_BAR_TEXT", "SET_BORDER_CENTER", "SET_BORDER_LERP_SIZE", "SET_BORDER_SIZE", "SET_BORDER_WARNING_DELAY", "SET_BORDER_WARNING_DISTANCE", "SET_CAMERA", "SET_CARRIED_ITEM", "SET_CHUNK_CACHE_CENTER", "SET_CHUNK_CACHE_RADIUS", "SET_DEFAULT_SPAWN_POSITION", "SET_DISPLAY_OBJECTIVE", "SET_ENTITY_DATA", "SET_ENTITY_LINK", "SET_ENTITY_MOTION", "SET_EQUIPMENT", "SET_EXPERIENCE", "SET_HEALTH", "SET_OBJECTIVE", "SET_PASSENGERS", "SET_PLAYER_TEAM", "SET_SCORE", "SET_SIMULATION_DISTANCE", "SET_SUBTITLE_TEXT", "SET_TIME", "SET_TITLE_TEXT", "SET_TITLES_ANIMATION", "SOUND_ENTITY", "SOUND", "STOP_SOUND", "TAB_LIST", "TAG_QUERY", "TAKE_ITEM_ENTITY", "TELEPORT_ENTITY", "UPDATE_ADVANCEMENTS", "UPDATE_ATTRIBUTES", "UPDATE_MOB_EFFECT", "UPDATE_RECIPES", "UPDATE_TAGS"];
    pub const S_NEW: &[&str] = &["ACCEPT_TELEPORTATION", "BLOCK_ENTITY_TAG_QUERY", "CHANGE_DIFFICULTY", "CHAT_COMMAND", "CHAT", "CHAT_PREVIEW", "CLIENT_COMMAND", "CLIENT_INFORMATION", "COMMAND_SUGGESTION", "CONTAINER_BUTTON_CLICK", "CONTAINER_CLICK", "CONTAINER_CLOSE", "CUSTOM_PAYLOAD", "EDIT_BOOK", "ENTITY_TAG_QUERY", "INTERACT", "JIGSAW_GENERATE", "KEEP_ALIVE", "LOCK_DIFFICULTY", "MOVE_PLAYER_POS", "MOVE_PLAYER_POS_ROT", "MOVE_PLAYER_ROT", "MOVE_PLAYER_STATUS_ONLY", "MOVE_VEHICLE", "PADDLE_BOAT", "PICK_ITEM", "PLACE_RECIPE", "PLAYER_ABILITIES", "PLAYER_ACTION", "PLAYER_COMMAND", "PLAYER_INPUT", "PONG", "RECIPE_BOOK_CHANGE_SETTINGS", "RECIPE_BOOK_SEEN_RECIPE", "RENAME_ITEM", "RESOURCE_PACK", "SEEN_ADVANCEMENTS", "SELECT_TRADE", "SET_BEACON", "SET_CARRIED_ITEM", "SET_COMMAND_BLOCK", "SET_COMMAND_MINECART", "SET_CREATIVE_MODE_SLOT", "SET_JIGSAW_BLOCK", "SET_STRUCTURE_BLOCK", "SIGN_UPDATE", "SWING", "TELEPORT_TO_ENTITY", "USE_ITEM_ON", "USE_ITEM"];
    pub const S_OLD: &[&str] = &["ACCEPT_TELEPORTATION", "BLOCK_ENTITY_TAG_QUERY", "CHANGE_DIFFICULTY", "CHAT", "CLIENT_COMMAND", "CLIENT_INFORMATION", "COMMAND_SUGGESTION", "CONTAINER_BUTTON_CLICK", "CONTAINER_CLICK", "CONTAINER_CLOSE", "CUSTOM_PAYLOAD", "EDIT_BOOK", "ENTITY_TAG_QUERY", "INTERACT", "JIGSAW_GENERATE", "KEEP_ALIVE", "LOCK_DIFFICULTY", "MOVE_PLAYER_POS", "MOVE_PLAYER_POS_ROT", "MOVE_PLAYER_ROT", "MOVE_PLAYER_STATUS_ONLY", "MOVE_VEHICLE", "PADDLE_BOAT", "PICK_ITEM", "PLACE_RECIPE", "PLAYER_ABILITIES", "PLAYER_ACTION", "PLAYER_COMMAND", "PLAYER_INPUT", "PONG", "RECIPE_BOOK_CHANGE_SETTINGS", "RECIPE_BOOK_SEEN_RECIPE", "RENAME_ITEM", "RESOURCE_PACK", "SEEN_ADVANCEMENTS", "SELECT_TRADE", "SET_BEACON", "SET_CARRIED_ITEM", "SET_COMMAND_BLOCK", "SET_COMMAND_MINECART", "SET_CREATIVE_MODE_SLOT", "SET_JIGSAW_BLOCK", "SET_STRUCTURE_BLOCK", "SIGN_UPDATE", "SWING", "TELEPORT_TO_ENTITY", "USE_ITEM_ON", "USE_ITEM"];

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
            assert_eq!(play_serverbound(i as i32), S_NEW.iter().position(|n| n == name).map(|p| p as i32), "serverbound {name}");
        }
        assert_eq!(play_serverbound(S_OLD.len() as i32), None);
        assert_eq!(play_serverbound(-1), None);
    }

    #[test]
    fn named_ids_match_the_lists() {
        assert_eq!(C_NEW[new::LOGIN as usize], "LOGIN");
        assert_eq!(C_NEW[new::SYSTEM_CHAT as usize], "SYSTEM_CHAT");
        assert_eq!(C_OLD[old::ADD_MOB as usize], "ADD_MOB");
        assert_eq!(C_OLD[old::CHAT as usize], "CHAT");
        assert_eq!(S_NEW[new::S_CHAT_COMMAND as usize], "CHAT_COMMAND");
        assert_eq!(S_OLD[old::S_USE_ITEM as usize], "USE_ITEM");
    }
}

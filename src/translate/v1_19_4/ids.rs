//! Play packet ids. 1.20 and 1.19.4 number their packets identically, so one set of names
//! serves both and no id is remapped.

// clientbound
pub const ADD_ENTITY: i32 = 1;
pub const AWARD_STATS: i32 = 5;
pub const BLOCK_ENTITY_DATA: i32 = 8;
pub const BLOCK_EVENT: i32 = 9;
pub const BLOCK_UPDATE: i32 = 10;
pub const CONTAINER_SET_CONTENT: i32 = 18;
pub const CONTAINER_SET_SLOT: i32 = 20;
pub const COOLDOWN: i32 = 21;
pub const LEVEL_CHUNK_WITH_LIGHT: i32 = 36;
pub const LEVEL_EVENT: i32 = 37;
pub const LEVEL_PARTICLES: i32 = 38;
pub const LIGHT_UPDATE: i32 = 39;
pub const LOGIN: i32 = 40;
pub const MERCHANT_OFFERS: i32 = 42;
pub const OPEN_SCREEN: i32 = 48;
pub const OPEN_SIGN_EDITOR: i32 = 49;
pub const PLAYER_COMBAT_END: i32 = 54;
pub const PLAYER_COMBAT_KILL: i32 = 56;
pub const REMOVE_ENTITIES: i32 = 62;
pub const RESPAWN: i32 = 65;
pub const SECTION_BLOCKS_UPDATE: i32 = 67;
pub const SET_ENTITY_DATA: i32 = 82;
pub const SET_EQUIPMENT: i32 = 85;
pub const SOUND_ENTITY: i32 = 97;
pub const SOUND: i32 = 98;
pub const UPDATE_ADVANCEMENTS: i32 = 105;
pub const UPDATE_ENABLED_FEATURES: i32 = 107;
pub const UPDATE_RECIPES: i32 = 109;
pub const UPDATE_TAGS: i32 = 110;
/// The highest clientbound play id either version has.
pub const LAST_CLIENTBOUND: i32 = 110;

// serverbound
pub const S_CONTAINER_CLICK: i32 = 11;
pub const S_SET_CREATIVE_MODE_SLOT: i32 = 43;
pub const S_SIGN_UPDATE: i32 = 46;
/// The highest serverbound play id either version has.
pub const LAST_SERVERBOUND: i32 = 50;

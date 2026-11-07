//! Logs Bedrock players joining and leaving, in the same format the proxy uses for Java players.
//! Bedrock clients connect to Pumpkin directly (not through this plugin's listener), so the details
//! come from Pumpkin's join and leave events.

use pumpkin_plugin_api::events::{EventHandler, FromIntoEvent, PlayerJoinEvent, PlayerLeaveEvent};
use pumpkin_plugin_api::player::{BedrockDeviceOs, Player};
use pumpkin_plugin_api::Server;

/// Readable name for a Bedrock device.
pub fn device_name(os: BedrockDeviceOs) -> &'static str {
    match os {
        BedrockDeviceOs::Android => "Android",
        BedrockDeviceOs::Ios => "iOS",
        BedrockDeviceOs::Osx => "macOS",
        BedrockDeviceOs::Amazon => "Fire OS",
        BedrockDeviceOs::GearVr => "Gear VR",
        BedrockDeviceOs::HoloLens => "HoloLens",
        BedrockDeviceOs::Windows10 | BedrockDeviceOs::Win32 => "Windows",
        BedrockDeviceOs::Dedicated => "dedicated",
        BedrockDeviceOs::TvOs => "tvOS",
        BedrockDeviceOs::Playstation => "PlayStation",
        BedrockDeviceOs::Nintendo => "Switch",
        BedrockDeviceOs::Xbox => "Xbox",
        BedrockDeviceOs::WindowsPhone => "Windows Phone",
        BedrockDeviceOs::Linux => "Linux",
        BedrockDeviceOs::Unknown => "unknown device",
    }
}

/// "Name [ip] (Bedrock 1.26.60, Android) uuid ..." or `None` for a Java player.
fn describe(player: &Player) -> Option<String> {
    let bedrock = player.as_bedrock()?;
    let s = bedrock.get_settings();
    let version = if s.game_version.is_empty() { "unknown version".to_string() } else { s.game_version.clone() };
    Some(format!(
        "{} [{}] (Bedrock {}, {}) uuid {}",
        player.get_name(),
        player.get_ip(),
        version,
        device_name(s.device_os),
        uuid_string(player),
    ))
}

fn uuid_string(player: &Player) -> String {
    let id = player.get_id();
    let v = ((id.high as u128) << 64) | id.low as u128;
    let h = format!("{v:032x}");
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

pub struct JoinLog;
impl EventHandler<PlayerJoinEvent> for JoinLog {
    fn handle(&self, _server: Server, e: <PlayerJoinEvent as FromIntoEvent>::Data) -> <PlayerJoinEvent as FromIntoEvent>::Data {
        if let Some(who) = describe(&e.player) {
            crate::log::info(&format!("{who} joined (native Bedrock connection)"));
        }
        e
    }
}

pub struct LeaveLog;
impl EventHandler<PlayerLeaveEvent> for LeaveLog {
    fn handle(&self, _server: Server, e: <PlayerLeaveEvent as FromIntoEvent>::Data) -> <PlayerLeaveEvent as FromIntoEvent>::Data {
        if let Some(who) = describe(&e.player) {
            crate::log::info(&format!("{who} disconnected"));
        }
        e
    }
}

//! multiversion: a Pumpkin plugin that lets other Minecraft Java versions join.
//!
//! It runs a protocol-translating proxy *inside* the server process: it listens on the public
//! port, speaks each client's own protocol version, verifies the player with Mojang, connects
//! to Pumpkin's localhost listener as a 26.3 client and hands Pumpkin the real identity through
//! Velocity-style forwarding (signed with a shared secret). Game packets flow through a chain of
//! translators between the client's version and Pumpkin's.

#![allow(non_snake_case)] // the library is named Multiversion so the plugin file is Multiversion.wasm
#![allow(clippy::too_many_lines)]

pub mod banner;
pub mod color;
pub mod config;
pub mod crypto;
pub mod net;
pub mod platform;
pub mod proto;
pub mod ratelimit;
pub mod translate;
pub mod update;

#[cfg(target_arch = "wasm32")]
pub mod auth;
#[cfg(target_arch = "wasm32")]
pub mod bedrock_log;
#[cfg(target_arch = "wasm32")]
pub mod conn;
#[cfg(target_arch = "wasm32")]
pub mod http;
#[cfg(target_arch = "wasm32")]
pub mod setup_check;
#[cfg(target_arch = "wasm32")]
pub mod updater;
#[cfg(target_arch = "wasm32")]
pub mod proxy;

#[cfg(target_arch = "wasm32")]
mod plugin {
    use crate::update::Status;
    use crate::updater::{self, Job};
    use pumpkin_plugin_api::command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs};
    use pumpkin_plugin_api::commands::CommandHandler;
    use pumpkin_plugin_api::events::{EventHandler, EventPriority, FromIntoEvent, PlayerJoinEvent};
    use pumpkin_plugin_api::permission::{Permission, PermissionDefault, PermissionLevel};
    use pumpkin_plugin_api::scheduler::SchedulerExt;
    use pumpkin_plugin_api::text::TextComponent;
    use pumpkin_plugin_api::{Context, Plugin, PluginMetadata, Server, permissions, register_plugin};
    use std::sync::Mutex;

    /// Ticks to wait after a player joins before telling them about an update.
    const JOIN_NOTICE_DELAY_TICKS: u64 = 60;
    const UPDATE_PERMISSION: &str = "multiversion:update";

    /// `/multiversion update [check|download]`.
    struct UpdateCommand(Option<Job>);
    impl CommandHandler for UpdateCommand {
        fn handle(&self, sender: CommandSender, _server: Server, _args: ConsumedArgs) -> Result<i32, CommandError> {
            let console = sender.is_console();
            // The console gets ANSI colours (if they are on) and the plugin's tag; chat gets the
            // same colours as chat colours.
            let send = |line: &str| {
                if console {
                    sender.send_message(TextComponent::text(&format!("{} {line}", crate::color::tag("[Multiversion]"))));
                } else {
                    match TextComponent::from_json(&crate::color::chat_json(line)) {
                        Ok(component) => sender.send_message(component),
                        Err(_) => sender.send_message(TextComponent::text(&crate::color::strip(line))),
                    }
                }
            };
            // Builds a line with colours on for chat, whatever console_colors says.
            let coloured = |f: &dyn Fn() -> String| if console { f() } else { crate::color::with_colours(f) };
            let say = |line: &str| send(&coloured(&|| crate::color::dim(line)));
            match self.0 {
                None => {
                    let lines = if console { updater::lines(&updater::status()) } else { crate::color::with_colours(|| updater::lines(&updater::status())) };
                    for line in lines {
                        send(&line);
                    }
                    if console {
                        say("commands: /multiversion update check, /multiversion update download");
                    } else if let Ok(buttons) = TextComponent::from_json(&update_buttons()) {
                        sender.send_message(buttons);
                    }
                }
                // The proxy runs the request between packets, so players never wait on the Market.
                Some(job) => {
                    if matches!(job, Job::Download) && !matches!(updater::status(), Status::Available { .. }) {
                        say("there is no newer version to download; run /multiversion update check first");
                    } else if updater::request(job) {
                        say("asking the Pumpkin Market; the result is logged to the console, and /multiversion update shows it");
                    } else {
                        say("an update request is already running");
                    }
                }
            }
            Ok(1)
        }
    }

    /// Clickable [Check now] and [Download] for chat.
    fn update_buttons() -> String {
        let button = |label: &str, command: &str, hover: &str| {
            serde_json::json!({ "text": format!("[{label}]"), "color": "aqua",
                "click_event": { "action": "run_command", "command": command },
                "hover_event": { "action": "show_text", "value": [{ "text": format!("{hover}\nClick to run {command}"), "color": "gray" }] } })
        };
        serde_json::json!({ "text": "", "extra": [
            button("Check now", "/multiversion update check", "Ask the Pumpkin Market for the latest version"),
            { "text": " ", "color": "gray" },
            button("Download", "/multiversion update download", "Save the Market's signed Multiversion.wasm in the data folder"),
        ]})
        .to_string()
    }

    /// Tells operators about a newer version shortly after they join.
    struct UpdateNotice;
    impl EventHandler<PlayerJoinEvent> for UpdateNotice {
        fn handle(&self, _server: Server, e: <PlayerJoinEvent as FromIntoEvent>::Data) -> <PlayerJoinEvent as FromIntoEvent>::Data {
            let may_see = |p: &pumpkin_plugin_api::Player| p.has_permission(UPDATE_PERMISSION) || matches!(p.get_permission_level(), PermissionLevel::Three | PermissionLevel::Four);
            if crate::config::get().updates.notify_ops && matches!(updater::status(), Status::Available { .. }) && may_see(&e.player) {
                let id = e.player.get_id();
                pumpkin_plugin_api::scheduler::schedule_delayed_task(JOIN_NOTICE_DELAY_TICKS, move |server| {
                    let Status::Available { latest, .. } = updater::status() else { return };
                    let Some(player) = server.get_player_by_uuid(id) else { return };
                    let line = serde_json::json!({ "text": "", "extra": [
                        { "text": "[Multiversion] ", "color": "blue" },
                        { "text": format!("v{latest} is out"), "color": "yellow" },
                        { "text": format!(" (this server runs v{}). ", updater::VERSION), "color": "gray" },
                        { "text": "[Pumpkin Market]", "color": "aqua",
                          "click_event": { "action": "open_url", "url": crate::update::PAGE_URL },
                          "hover_event": { "action": "show_text", "value": [{ "text": crate::update::PAGE_URL, "color": "gray" }] } },
                        { "text": " ", "color": "gray" },
                        { "text": "[Details]", "color": "aqua",
                          "click_event": { "action": "run_command", "command": "/multiversion update" },
                          "hover_event": { "action": "show_text", "value": [{ "text": "Update status and download\nClick to run /multiversion update", "color": "gray" }] } },
                    ]})
                    .to_string();
                    if let Ok(component) = TextComponent::from_json(&line) {
                        player.send_system_message(component, false);
                    }
                });
            }
            e
        }
    }

    pub struct MultiVersion {
        config: Mutex<Option<crate::config::Config>>,
    }

    impl Plugin for MultiVersion {
        fn new() -> Self {
            Self { config: Mutex::new(None) }
        }

        fn metadata(&self) -> PluginMetadata {
            PluginMetadata {
                name: "multiversion".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                authors: vec!["Xathr".into()],
                description: "Lets older and newer Minecraft Java clients join by translating their protocol.".into(),
                dependencies: vec![],
                permissions: vec![
                    permissions::NETWORK_TCP.into(),      // bind the public port, connect to Pumpkin
                    permissions::HTTP_OUTBOUND.into(),    // sessionserver.mojang.com (online mode), update check
                    permissions::FS_READ_DATA.into(),     // config.toml
                    permissions::FS_WRITE_DATA.into(),    // write default config on first start, downloaded updates
                ],
            }
        }

        fn on_load(&self, context: Context) -> pumpkin_plugin_api::Result<()> {
            let folder = context.get_data_folder();
            let path = format!("{folder}/config.toml");
            let config = crate::config::Config::load_or_create(&path).map_err(|e| format!("multiversion: {e}"))?;
            crate::color::set_enabled(config.console_colors);
            crate::config::set_active(config.clone());
            // Pumpkin lets a plugin write only its own data folder, so the pumpkin.toml settings
            // it needs are written out ready to paste, and checked for at runtime.
            let snippet = crate::config::pumpkin_snippet(&config);
            let _ = std::fs::write(format!("{folder}/pumpkin-settings.txt"), &snippet);
            let range = crate::translate::allowed_range(&config.versions);
            let info = crate::banner::Info {
                version: env!("CARGO_PKG_VERSION"),
                range: &range,
                listen: &config.listen.address,
                backend: &config.backend.address,
                online_mode: config.auth.online_mode,
            };
            for line in crate::banner::lines(&info) {
                crate::log::info(&line);
            }
            if config.backend.forwarding_secret.is_empty() {
                crate::log::warn("backend.forwarding_secret is empty; Pumpkin will reject logins. Set it in config.toml and pumpkin.toml.");
            }
            *self.config.lock().unwrap() = Some(config.clone());
            // Ask the Market for a newer version now, before the proxy carries any players.
            updater::set_data_folder(&folder);
            if config.updates.check {
                let mut lines = updater::run_blocking(Job::Check);
                if config.updates.download && matches!(updater::status(), Status::Available { downloaded: None, .. }) {
                    lines = updater::run_blocking(Job::Download);
                }
                for line in lines {
                    crate::log::info(&line);
                }
            }
            let _ = context.register_permission(&Permission {
                node: UPDATE_PERMISSION.into(),
                description: "Use /multiversion update".into(),
                default: PermissionDefault::Op(PermissionLevel::Three),
                children: vec![],
            });
            let command = Command::new(&["multiversion".to_string()], "Multiversion update status").then(
                CommandNode::literal("update")
                    .then(CommandNode::literal("check").execute(UpdateCommand(Some(Job::Check))))
                    .then(CommandNode::literal("download").execute(UpdateCommand(Some(Job::Download))))
                    .execute(UpdateCommand(None)),
            );
            context.register_command(command.execute(UpdateCommand(None)), "update");
            context.register_event_handler::<PlayerJoinEvent, _>(UpdateNotice, EventPriority::Lowest, false)?;
            // Bind now; then run the proxy loop in short slices, one per server tick, so Pumpkin
            // can still call our other hooks (and unload us on shutdown) in between.
            if crate::proxy::start(config.clone()).map_err(|e| format!("multiversion: {e}"))? {
                context.schedule_repeating_task(1, 1, |_server| crate::proxy::tick());
            } else {
                // Pumpkin still holds the public port: this is a first start. Stay loaded and say
                // what to do, rather than fail with an error.
                for line in crate::config::setup_message(&config, &crate::config::ALL_NEEDS) {
                    crate::log::warn(&line);
                }
                crate::log::warn("Until then the plugin is idle and only this server's own version can join.");
            }
            // Bedrock players reach Pumpkin directly; log their version and device the same way.
            context.register_event_handler::<pumpkin_plugin_api::events::PlayerJoinEvent, _>(crate::bedrock_log::JoinLog, pumpkin_plugin_api::events::EventPriority::Lowest, false)?;
            context.register_event_handler::<pumpkin_plugin_api::events::PlayerLeaveEvent, _>(crate::bedrock_log::LeaveLog, pumpkin_plugin_api::events::EventPriority::Lowest, false)?;
            Ok(())
        }

        fn on_unload(&self, _context: Context) -> pumpkin_plugin_api::Result<()> {
            crate::proxy::request_stop();
            Ok(())
        }
    }

    register_plugin!(MultiVersion);
}

/// Logging that goes to the server console through the plugin API on wasm, stderr natively.
pub mod log {
    /// Logs a warning once per `key` for the life of the plugin.
    pub fn warn_once(key: &str, msg: &str) {
        if !crate::config::get().logging.errors {
            return;
        }
        static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
        let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        if seen.iter().any(|k| k == key) {
            return;
        }
        seen.push(key.to_string());
        drop(seen);
        warn(msg);
    }
    #[cfg(target_arch = "wasm32")]
    fn tagged(msg: &str) -> String {
        format!("{} {msg}", crate::color::tag("[Multiversion]"))
    }
    #[cfg(target_arch = "wasm32")]
    pub fn info(msg: &str) {
        pumpkin_plugin_api::logging::log(pumpkin_plugin_api::logging::LogLevel::Info, &tagged(msg));
    }
    #[cfg(target_arch = "wasm32")]
    pub fn warn(msg: &str) {
        pumpkin_plugin_api::logging::log(pumpkin_plugin_api::logging::LogLevel::Warn, &tagged(&crate::color::warn(msg)));
    }
    #[cfg(target_arch = "wasm32")]
    pub fn error(msg: &str) {
        pumpkin_plugin_api::logging::log(pumpkin_plugin_api::logging::LogLevel::Error, &tagged(&crate::color::bad(msg)));
    }
    #[cfg(target_arch = "wasm32")]
    pub fn debug(msg: &str) {
        pumpkin_plugin_api::logging::log(pumpkin_plugin_api::logging::LogLevel::Debug, &tagged(msg));
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn info(msg: &str) {
        eprintln!("[info] {msg}");
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn warn(msg: &str) {
        eprintln!("[warn] {msg}");
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn error(msg: &str) {
        eprintln!("[error] {msg}");
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn debug(msg: &str) {
        eprintln!("[debug] {msg}");
    }
}

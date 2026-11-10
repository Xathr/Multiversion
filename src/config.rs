//! Plugin configuration: `plugins/data/multiversion/config.toml`.
//!
//! A default file is written on first start so the server owner only has to fill in the
//! forwarding secret and (optionally) change ports.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Colour the plugin's console messages (ANSI codes, like Pumpkin's own output).
    pub console_colors: bool,
    pub listen: Listen,
    pub backend: Backend,
    pub auth: Auth,
    pub status: Status,
    pub versions: Versions,
    pub limits: Limits,
    pub updates: Updates,
    pub messages: Messages,
    pub logging: Logging,
}

/// Where players connect. This is the port you open in the firewall.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Listen {
    pub address: String,
}

/// The Pumpkin listener this plugin forwards to. Must be `networking.java.address` in
/// pumpkin.toml, bound to localhost, with velocity forwarding enabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Backend {
    pub address: String,
    /// Shared secret: must equal `networking.proxy.velocity.secret` in pumpkin.toml.
    pub forwarding_secret: String,
    /// Whether Pumpkin compresses what it sends to this plugin. Off by default: the link is
    /// localhost, and the plugin compresses again for each player anyway, so Pumpkin's compression
    /// only costs CPU. Players' connections are compressed either way ([limits].compression_threshold).
    pub pumpkin_compression: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Auth {
    /// Verify players with Mojang. Keep this on for a public server.
    pub online_mode: bool,
    /// Session server used for `hasJoined`. Change only for alternative auth servers.
    pub session_server: String,
    /// Reject players that connect through a proxy Mojang reports for their IP.
    pub prevent_proxy_connections: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Status {
    /// Version name shown in the server list. Empty = "<lowest>-<highest>".
    pub version_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Versions {
    /// Client versions allowed to join. Empty = every version the plugin can translate.
    /// Entries: a version ("26.2"), a family ("1.21.*") or a range ("1.21.5-26.3").
    pub allow: Vec<String>,
    /// Versions refused even if `allow` would admit them. Same syntax as `allow`.
    pub deny: Vec<String>,
    pub kick_unsupported_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    /// Packets to clients larger than this are zlib-compressed. -1 disables compression.
    pub compression_threshold: i32,
    pub max_connections: usize,
    /// Seconds a connection may sit in handshake/login before it is dropped.
    pub login_timeout_secs: u64,
    /// Connections one IP address may have that are still logging in (players already in the game
    /// don't count, so several people behind one router can all play). Stops one address from
    /// filling max_connections. 0 = no limit.
    pub max_logins_per_ip: usize,
    /// New connections one IP address may open per 10 seconds (server-list pings count). Stops
    /// one address from keeping the server busy with a stream of short connections. 0 = no limit.
    pub max_connections_per_ip_per_10s: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: Listen::default(),
            backend: Backend::default(),
            auth: Auth::default(),
            status: Status::default(),
            versions: Versions::default(),
            limits: Limits::default(),
            updates: Updates::default(),
            console_colors: true,
            messages: Messages::default(),
            logging: Logging::default(),
        }
    }
}
/// The update check against the Pumpkin Market.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Updates {
    /// Ask the Pumpkin Market for the latest version when the server starts.
    pub check: bool,
    /// Tell operators in game, when they join, that a newer version is out.
    pub notify_ops: bool,
    /// Download a newer, Market-signed Multiversion.wasm into the data folder at startup.
    pub download: bool,
}
impl Default for Updates {
    fn default() -> Self {
        Self { check: true, notify_ops: true, download: false }
    }
}
impl Default for Listen {
    fn default() -> Self {
        Self { address: "0.0.0.0:25565".into() }
    }
}
impl Default for Backend {
    fn default() -> Self {
        Self { address: "127.0.0.1:25566".into(), forwarding_secret: String::new(), pumpkin_compression: false }
    }
}
impl Default for Auth {
    fn default() -> Self {
        Self {
            online_mode: true,
            session_server: "https://sessionserver.mojang.com".into(),
            prevent_proxy_connections: false,
        }
    }
}
impl Default for Status {
    fn default() -> Self {
        Self { version_name: String::new() }
    }
}
impl Default for Versions {
    fn default() -> Self {
        Self {
            allow: vec!["*".into()],
            deny: Vec::new(),
            kick_unsupported_message: "This server supports Minecraft {versions}. You are on {client}.".into(),
        }
    }
}
impl Default for Limits {
    fn default() -> Self {
        Self { compression_threshold: 256, max_connections: 200, login_timeout_secs: 30, max_logins_per_ip: 5, max_connections_per_ip_per_10s: 40 }
    }
}

/// Messages players see when they are disconnected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Messages {
    /// Pumpkin isn't accepting connections yet (starting up or stopped).
    pub server_starting: String,
    /// Mojang's session servers couldn't be reached.
    pub auth_servers_down: String,
    /// Mojang said the player's session isn't valid.
    pub invalid_session: String,
    /// The name Mojang returned doesn't match the one the client sent.
    pub username_mismatch: String,
    /// Something went wrong in the plugin. Details are in the console.
    pub internal_error: String,
    /// pumpkin.toml isn't set up for the plugin. Details are in the console.
    pub misconfigured: String,
    /// The client sent something the plugin couldn't read. {error} is the detail.
    pub protocol_error: String,
}

impl Default for Messages {
    fn default() -> Self {
        Self {
            server_starting: "The server is starting up. Please try again in a moment.".into(),
            auth_servers_down: "Authentication servers are down. Please try again later, sorry!".into(),
            invalid_session: "Invalid session (Try restarting your game and the launcher)".into(),
            username_mismatch: "Username mismatch".into(),
            internal_error: "Internal proxy error".into(),
            misconfigured: "Server misconfigured: see the server console.".into(),
            protocol_error: "Protocol error: {error}".into(),
        }
    }
}

/// What the plugin writes to the server console.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Logging {
    /// A line when a player joins, with their version and address.
    pub joins: bool,
    /// A line when a player leaves, with the reason.
    pub leaves: bool,
    /// Join and leave lines for Bedrock players (version and device).
    pub bedrock: bool,
    /// Errors such as refused logins and translation problems.
    pub errors: bool,
    /// Every connection step: handshakes, server-list pings, logins, backend connections.
    /// Useful when someone can't join; noisy otherwise.
    pub debug: bool,
}

impl Default for Logging {
    fn default() -> Self {
        Self { joins: true, leaves: true, bedrock: true, errors: true, debug: false }
    }
}

static ACTIVE: std::sync::OnceLock<Config> = std::sync::OnceLock::new();

/// Makes `config` the one `get()` returns. Called once when the plugin loads.
pub fn set_active(config: Config) {
    let _ = ACTIVE.set(config);
}

/// The loaded config (defaults before load, e.g. in tests).
pub fn get() -> &'static Config {
    static DEFAULT: std::sync::OnceLock<Config> = std::sync::OnceLock::new();
    ACTIVE.get().unwrap_or_else(|| DEFAULT.get_or_init(Config::default))
}

pub const FILE_HEADER: &str = "# multiversion: lets other Minecraft versions join this Pumpkin server.\n\
# Players connect to [listen].address; the plugin translates their protocol and forwards them to\n\
# Pumpkin at [backend].address. In pumpkin.toml set:\n\
#   [networking.java]            address = \"127.0.0.1:25566\"   online_mode = false\n\
#   [networking.proxy]           enabled = true\n\
#   [networking.proxy.velocity]  enabled = true   secret = \"<same as forwarding_secret below>\"\n\
#   [networking.java.compression] enabled = false   (skip this if [backend].pumpkin_compression = true)\n\
#\n\
# [backend].pumpkin_compression: leave false to save CPU. Pumpkin's link to this plugin is local, and\n\
# players' connections are still compressed by the plugin ([limits].compression_threshold).\n\
#\n\
# [versions] chooses who may join. Entries are a version (\"1.21.4\"), a family (\"1.21.*\") or a\n\
# range (\"1.21.5-26.3\"). allow = [\"*\"] (the default) or [] means every version the plugin can\n\
# translate; deny wins over\n\
# allow. Versions that share a protocol count as one: 1.19.2 = 1.19.1, 1.20.1 = 1.20, 1.20.4 = 1.20.3, 1.20.6 = 1.20.5, 1.21.1 = 1.21,\n\
# 1.21.3 = 1.21.2, 1.21.8 = 1.21.7, 1.21.10 = 1.21.9. Example: allow = [\"1.21.*\", \"26.3\"], deny = [\"1.21\"].\n\n";

impl Config {
    pub fn load_or_create(path: &str) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let cfg: Self = toml::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
                // A file from an older version, or one written by hand, may not list every
                // setting. Add the ones it lacks so they can all be seen and changed.
                if let Some(filled) = fill_missing(&text, &cfg) {
                    let _ = std::fs::write(path, filled);
                }
                Ok(cfg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut cfg = Self::default();
                cfg.backend.forwarding_secret = crate::crypto::random_token();
                let body = toml::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
                std::fs::write(path, format!("{FILE_HEADER}{body}")).map_err(|e| format!("{path}: {e}"))?;
                Ok(cfg)
            }
            Err(e) => Err(format!("{path}: {e}")),
        }
    }
}

/// `text` (a config file) with every setting it does not mention added, at the value in `cfg`.
/// What is already there, comments and layout included, is left as it is. `None` if nothing is
/// missing or the text cannot be edited.
pub fn fill_missing(text: &str, cfg: &Config) -> Option<String> {
    fn merge(into: &mut toml_edit::Table, from: &toml_edit::Table, changed: &mut bool) {
        for (key, item) in from.iter() {
            match into.get_mut(key) {
                None => {
                    into.insert(key, item.clone());
                    *changed = true;
                }
                Some(existing) => {
                    if let (Some(a), Some(b)) = (existing.as_table_mut(), item.as_table()) {
                        merge(a, b, changed);
                    }
                }
            }
        }
    }
    let mut doc: toml_edit::DocumentMut = text.parse().ok()?;
    let mut full: toml_edit::DocumentMut = toml::to_string_pretty(cfg).ok()?.parse().ok()?;
    // Settings outside any section go under the comment that opens the file, not above it.
    let mut top = String::new();
    let top_keys: Vec<String> = full.iter().filter(|(_, item)| !item.is_table()).map(|(k, _)| k.to_string()).collect();
    for key in top_keys {
        if let Some(item) = full.remove(&key)
            && !doc.contains_key(&key)
        {
            top.push_str(&format!("{key} = {}\n", item.to_string().trim()));
        }
    }
    let mut changed = !top.is_empty();
    merge(doc.as_table_mut(), full.as_table(), &mut changed);
    if !changed {
        return None;
    }
    let body = doc.to_string();
    if top.is_empty() {
        return Some(body);
    }
    let opening: usize = body.lines().take_while(|l| l.trim_start().starts_with('#')).map(|l| l.len() + 1).sum();
    let opening = opening.min(body.len());
    let gap = if opening > 0 { "\n" } else { "" };
    let rest = body[opening..].trim_start_matches('\n');
    Some(format!("{}{gap}{top}\n{rest}", &body[..opening]))
}

/// A pumpkin.toml line the plugin depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Pumpkin listens on the backend address, leaving the public port to the plugin.
    Address,
    ProxyEnabled,
    VelocityEnabled,
    /// Pumpkin's forwarding secret equals the plugin's.
    Secret,
}

pub const ALL_NEEDS: [Need; 4] = [Need::Address, Need::ProxyEnabled, Need::VelocityEnabled, Need::Secret];

/// The lines to change in pumpkin.toml for `needs`: the heading each is found under, then the
/// line as it should read. Every one of them is already in the file Pumpkin writes.
pub fn setup_lines(cfg: &Config, needs: &[Need]) -> Vec<String> {
    let secret = cfg.backend.forwarding_secret.replace('\\', "\\\\").replace('"', "\\\"");
    needs
        .iter()
        .map(|need| {
            let (heading, line) = match need {
                Need::Address => ("[networking.java]", format!("address = \"{}\"", cfg.backend.address)),
                Need::ProxyEnabled => ("[networking.proxy]", "enabled = true".to_string()),
                Need::VelocityEnabled => ("[networking.proxy.velocity]", "enabled = true".to_string()),
                Need::Secret => ("[networking.proxy.velocity]", format!("secret = \"{secret}\"")),
            };
            format!("{heading:<29} {line}")
        })
        .collect()
}

/// What the console says when setup is not finished: the lines still to change, and where.
pub fn setup_message(cfg: &Config, needs: &[Need]) -> Vec<String> {
    let count = if needs.len() == 1 { "this line".to_string() } else { format!("these {} lines", needs.len()) };
    let mut out = vec![format!("SETUP NEEDED (one time): stop the server, open pumpkin.toml and change {count}:")];
    out.extend(setup_lines(cfg, needs).into_iter().map(|l| format!("    {l}")));
    out.push("Each is already in the file, under the heading in [brackets]. Save, then start the server.".to_string());
    out.push("The same list is in plugins/data/multiversion/pumpkin-settings.txt.".to_string());
    out
}

/// `pumpkin-settings.txt`: the same setup steps as a file to read next to pumpkin.toml. Pumpkin's
/// `online_mode` can stay on: with forwarding enabled it trusts the signed profile instead of
/// asking Mojang.
pub fn pumpkin_snippet(cfg: &Config) -> String {
    let lines: String = setup_lines(cfg, &ALL_NEEDS).iter().map(|l| format!("    {l}\n")).collect();
    let backend_port = cfg.backend.address.rsplit(':').next().unwrap_or("25566");
    let compression = if cfg.backend.pumpkin_compression {
        String::new()
    } else {
        format!(
            "    {:<29} enabled = false\n\
             \x20       Saves CPU: Pumpkin stops compressing its local link to the plugin. Players'\n\
             \x20       connections are still compressed by the plugin.\n\n",
            "[networking.java.compression]"
        )
    };
    format!(
        "multiversion: one-time setup\n\
         ============================\n\
         \n\
         1. Stop the server.\n\
         2. Open pumpkin.toml (in the server folder) in a text editor and change these lines.\n\
         \x20  Each one is already in the file, under the heading shown in [brackets]:\n\
         \n\
         {lines}\
         \n\
         3. Save the file and start the server.\n\
         \n\
         Players keep connecting to port {public_port}. Keep port {backend_port} closed in your firewall.\n\
         Every time the server starts, the plugin checks these settings and says in the console\n\
         either that it is ready or which line is still wrong.\n\
         \n\
         Optional\n\
         --------\n\
         \n\
         {compression}\
         \x20   To stop Pumpkin asking to approve the plugin's permissions when it starts, add these\n\
         \x20   two lines at the very end of pumpkin.toml:\n\
         \n\
         [plugins.overrides.multiversion]\n\
         allowed_permissions = [\"network.tcp\", \"http.outbound\", \"fs.read.data\", \"fs.write.data\"]\n",
        public_port = cfg.listen.address.rsplit(':').next().unwrap_or("25565"),
    )
}

impl Limits {
    /// Whether an address that already has `logging_in` connections still logging in may open another.
    pub fn allows_login_from(&self, logging_in: usize) -> bool {
        self.max_logins_per_ip == 0 || logging_in < self.max_logins_per_ip
    }
}

impl Versions {
    /// Whether a client version passes the allow/deny lists (translator support is checked separately).
    pub fn permits(&self, v: crate::proto::version::Version) -> bool {
        if self.deny.iter().any(|e| selector_matches(e, v)) {
            return false;
        }
        self.allow.is_empty() || self.allow.iter().any(|e| selector_matches(e, v))
    }
}

/// Resolves a version name, including names of releases that share a protocol with a listed one
/// (1.19.2 -> 1.19.1, 1.20.1 -> 1.20, 1.20.4 -> 1.20.3, 1.20.6 -> 1.20.5, 1.21.1 -> 1.21, 1.21.3 -> 1.21.2, 1.21.8 -> 1.21.7, 1.21.10 -> 1.21.9).
fn resolve(name: &str) -> Option<crate::proto::version::Version> {
    use crate::proto::version::Version;
    let name = name.trim().trim_start_matches("minecraft ").trim_start_matches('v');
    if let Some(v) = Version::from_name(name) {
        return Some(v);
    }
    if let Ok(p) = name.parse::<i32>() {
        return Some(Version { protocol: p });
    }
    // Patch release sharing the protocol of an earlier one: walk the patch number down.
    let mut parts: Vec<u32> = name.split('.').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    if parts.len() != 3 {
        return None;
    }
    while parts[2] > 0 {
        parts[2] -= 1;
        let candidate = if parts[2] == 0 { format!("{}.{}", parts[0], parts[1]) } else { format!("{}.{}.{}", parts[0], parts[1], parts[2]) };
        if let Some(v) = Version::from_name(&candidate) {
            return Some(v);
        }
    }
    None
}

/// `"26.2"`, `"1.21.*"` (family) or `"1.21.5-26.3"` (inclusive range, either end may be omitted).
pub fn selector_matches(selector: &str, v: crate::proto::version::Version) -> bool {
    let sel = selector.trim();
    if sel == "*" || sel.eq_ignore_ascii_case("all") {
        return true;
    }
    if let Some(family) = sel.strip_suffix(".*") {
        let name = v.name();
        return name == family || name.starts_with(&format!("{family}."));
    }
    if let Some((lo, hi)) = sel.split_once('-') {
        let lo_ok = lo.trim().is_empty() || resolve(lo).is_some_and(|l| v.protocol >= l.protocol);
        let hi_ok = hi.trim().is_empty() || resolve(hi).is_some_and(|h| v.protocol <= h.protocol);
        return lo_ok && hi_ok && (resolve(lo).is_some() || resolve(hi).is_some());
    }
    resolve(sel) == Some(v)
}

#[cfg(test)]
mod version_selector_tests {
    use super::*;
    use crate::proto::version::Version;
    fn v(name: &str) -> Version {
        Version::from_name(name).unwrap()
    }
    #[test]
    fn selectors() {
        assert!(selector_matches("26.2", v("26.2")));
        assert!(!selector_matches("26.2", v("26.3")));
        assert!(selector_matches("1.21.*", v("1.21.5")));
        assert!(selector_matches("1.21.*", v("1.21")));
        assert!(!selector_matches("1.21.*", v("26.1")));
        assert!(selector_matches("1.21.5-26.1", v("1.21.9")));
        assert!(!selector_matches("1.21.5-26.1", v("26.2")));
        assert!(selector_matches("-1.21.4", v("1.21")));
        assert!(selector_matches("26.2-", v("26.3")));
        assert!(selector_matches("1.21.1", v("1.21")));
        assert!(selector_matches("1.20.6", v("1.20.5")));
        assert!(selector_matches("1.20.4", v("1.20.3")));
        assert!(selector_matches("1.20.1", v("1.20")));
        assert!(selector_matches("1.19.2", v("1.19.1")));
        assert!(selector_matches("1.19.*", v("1.19.4")) && selector_matches("1.19.*", v("1.19")));
        assert!(selector_matches("1.21.10", v("1.21.9")));
        assert!(!selector_matches("bogus", v("26.3")));
    }
    #[test]
    fn allow_and_deny() {
        let cfg = Versions { allow: vec!["1.21.*".into(), "26.3".into()], deny: vec!["1.21".into()], ..Default::default() };
        assert!(cfg.permits(v("1.21.4")));
        assert!(cfg.permits(v("26.3")));
        assert!(!cfg.permits(v("1.21")));
        assert!(!cfg.permits(v("26.2")));
        assert!(Versions::default().permits(v("1.21.2")));
        assert!(Versions::default().permits(v("26.3")));
        assert!(Versions { allow: Vec::new(), ..Default::default() }.permits(v("1.21")));
    }
}

#[cfg(test)]
mod snippet_tests {
    use super::*;

    #[test]
    fn logins_per_ip() {
        let l = Limits::default();
        assert!(l.allows_login_from(4));
        assert!(!l.allows_login_from(5));
        let off = Limits { max_logins_per_ip: 0, ..Limits::default() };
        assert!(off.allows_login_from(1000));
    }

    #[test]
    fn snippet_follows_pumpkin_compression() {
        let mut cfg = Config::default();
        assert!(pumpkin_snippet(&cfg).contains("[networking.java.compression] enabled = false"));
        cfg.backend.pumpkin_compression = true;
        assert!(!pumpkin_snippet(&cfg).contains("[networking.java.compression]"));
        let parsed: Config = toml::from_str("[backend]\npumpkin_compression = true\n").unwrap();
        assert!(parsed.backend.pumpkin_compression);
        let old: Config = toml::from_str("[backend]\naddress = \"127.0.0.1:1\"\n").unwrap();
        assert!(!old.backend.pumpkin_compression, "configs without the setting turn Pumpkin's compression off");
    }

    #[test]
    fn missing_settings_are_added_to_an_existing_file() {
        let text = "# multiversion settings (players connect to [listen].address).\n\n[listen]\naddress = \"0.0.0.0:25570\"  # my port\n\n[backend]\naddress = \"127.0.0.1:25566\"\nforwarding_secret = \"abc\"\n\n[limits]\nmax_connections = 50\n";
        let cfg: Config = toml::from_str(text).unwrap();
        let filled = fill_missing(text, &cfg).expect("settings were missing");
        // What was there is untouched, comments included.
        assert!(filled.starts_with("# multiversion settings (players connect to [listen].address).\n\nconsole_colors = true\n\n[listen]\naddress = \"0.0.0.0:25570\"  # my port\n"), "{filled}");
        // A file with no opening comment, and one that sets the top-level setting itself.
        assert!(fill_missing("[listen]\naddress = \"0.0.0.0:1\"\n", &cfg).unwrap().starts_with("console_colors = true\n\n[listen]\n"));
        let own = fill_missing("console_colors = false\n[listen]\naddress = \"0.0.0.0:1\"\n", &Config { console_colors: false, ..Config::default() }).unwrap();
        assert!(own.starts_with("console_colors = false\n[listen]\n") && own.matches("console_colors").count() == 1, "{own}");
        // It reads back as the same settings, now all present.
        let back: Config = toml::from_str(&filled).unwrap();
        assert_eq!(back.listen.address, "0.0.0.0:25570");
        assert!(back.console_colors);
        assert_eq!(back.backend.forwarding_secret, "abc");
        assert_eq!(back.limits.max_connections, 50);
        let value: toml::Value = toml::from_str(&filled).unwrap();
        let full: toml::Value = toml::from_str(&toml::to_string(&Config::default()).unwrap()).unwrap();
        for (section, keys) in full.as_table().unwrap() {
            match keys.as_table() {
                Some(keys) => {
                    for key in keys.keys() {
                        assert!(value[section].get(key).is_some(), "{section}.{key} is in the file");
                    }
                }
                None => assert!(value.get(section).is_some(), "{section} is in the file"),
            }
        }
        // Keys added to a section that was already there sit in that section.
        assert!(filled.contains("[limits]\nmax_connections = 50\n"));
        assert_eq!(back.limits.max_connections_per_ip_per_10s, 40);
        assert!(back.updates.check);
        // A complete file is left alone.
        assert!(fill_missing(&filled, &back).is_none());
        assert!(fill_missing("not [toml", &cfg).is_none());
    }

    #[test]
    fn setup_lines_name_the_heading_and_the_line() {
        let mut cfg = Config::default();
        cfg.backend.forwarding_secret = "s3cret".into();
        let lines = setup_lines(&cfg, &ALL_NEEDS);
        assert_eq!(lines[0], "[networking.java]             address = \"127.0.0.1:25566\"");
        assert_eq!(lines[1], "[networking.proxy]            enabled = true");
        assert_eq!(lines[2], "[networking.proxy.velocity]   enabled = true");
        assert_eq!(lines[3], "[networking.proxy.velocity]   secret = \"s3cret\"");
        // Only what is still wrong is listed.
        let msg = setup_message(&cfg, &[Need::Secret]);
        assert!(msg[0].contains("change this line"));
        assert_eq!(msg.iter().filter(|l| l.contains("[networking.")).count(), 1);
        let file = pumpkin_snippet(&cfg);
        for line in &lines {
            assert!(file.contains(line.as_str()), "{line} is in the settings file");
        }
        assert!(file.contains("1. Stop the server.") && file.contains("Keep port 25566 closed"));
    }
}

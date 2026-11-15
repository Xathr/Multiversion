//! The plugin's text for players, in whatever languages there are files for.
//!
//! Every message a player can see has a key (`kick.invalid_session`, `update.out`, ...). A
//! language is one JSON file of key → text, named after its Minecraft locale: `en_us.json`,
//! `de_de.json`. The files in the repository's `lang/` folder are built into the plugin and
//! written to `plugins/data/multiversion/lang/` on first start. From then on the folder is what
//! counts: edit a file to reword a message, add a file to add a language. Nothing else has to
//! change, here or in the config.
//!
//! A key a file doesn't have falls back to the built-in text for that language, then to English.
//! A player whose exact locale has no file gets a file for the same language (`es_mx` reads
//! `es_es.json`), then English.

use std::collections::{BTreeMap, HashMap};
use std::sync::RwLock;

/// The language every key exists in, and the one used when nothing else fits.
pub const DEFAULT: &str = "en_us";

/// The languages built into the plugin.
const BUNDLED: &[(&str, &str)] = &[
    ("en_us", include_str!("../lang/en_us.json")),
    ("de_de", include_str!("../lang/de_de.json")),
    ("es_es", include_str!("../lang/es_es.json")),
    ("fr_fr", include_str!("../lang/fr_fr.json")),
    ("it_it", include_str!("../lang/it_it.json")),
    ("ja_jp", include_str!("../lang/ja_jp.json")),
    ("ko_kr", include_str!("../lang/ko_kr.json")),
    ("nl_nl", include_str!("../lang/nl_nl.json")),
    ("pl_pl", include_str!("../lang/pl_pl.json")),
    ("pt_br", include_str!("../lang/pt_br.json")),
    ("ru_ru", include_str!("../lang/ru_ru.json")),
    ("tr_tr", include_str!("../lang/tr_tr.json")),
    ("zh_cn", include_str!("../lang/zh_cn.json")),
];

/// Messages the game has words of its own for. With `language = "auto"` these are sent as the
/// game's translation key, so every player reads them in the language their game is set to.
const GAME_KEYS: &[(&str, &str)] = &[("kick.invalid_session", "multiplayer.disconnect.unverified_username"), ("kick.auth_servers_down", "multiplayer.disconnect.authservers_down")];
/// The oldest client that is known to have those keys (1.18), and the oldest that shows a
/// fallback text for a key it lacks (1.19.4).
const GAME_KEYS_FROM: i32 = 757;
const FALLBACK_FROM: i32 = 762;

/// "pt-BR" → "pt_br".
pub fn normalise(code: &str) -> String {
    code.trim().to_ascii_lowercase().replace('-', "_")
}

/// A locale as Minecraft names them: `en_us`, `zh_cn`, `fil_ph`, `tlh_aa`.
pub fn valid_code(code: &str) -> bool {
    let mut parts = code.split('_');
    let part = |p: Option<&str>, max: usize| p.is_some_and(|p| (2..=max).contains(&p.len()) && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    part(parts.next(), 4) && part(parts.next(), 6) && parts.next().is_none()
}

/// Puts values in a text's `{placeholders}`.
pub fn fill(text: &str, values: &[(&str, &str)]) -> String {
    let mut out = text.to_string();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// Every language the plugin has text in.
#[derive(Debug, Clone)]
pub struct Catalog {
    languages: BTreeMap<String, HashMap<String, String>>,
}

impl Catalog {
    /// The built-in languages.
    pub fn bundled() -> Self {
        let mut catalog = Self { languages: BTreeMap::new() };
        for (code, json) in BUNDLED {
            catalog.add(code, json).expect("a built-in language file is valid");
        }
        catalog
    }

    /// Adds a language file, or the changed and new texts of one that is already there. Returns
    /// how many texts it had.
    pub fn add(&mut self, code: &str, json: &str) -> Result<usize, String> {
        let code = normalise(code);
        if !valid_code(&code) {
            return Err(format!("\"{code}\" is not a locale name like en_us"));
        }
        let texts: HashMap<String, serde_json::Value> = serde_json::from_str(json).map_err(|e| format!("not a JSON object of texts: {e}"))?;
        let language = self.languages.entry(code).or_default();
        let mut count = 0;
        for (key, value) in texts {
            // anything that isn't text (a comment object, a number) is left out
            if let serde_json::Value::String(text) = value {
                language.insert(key, text);
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn languages(&self) -> Vec<&str> {
        self.languages.keys().map(String::as_str).collect()
    }

    /// The language file a locale reads: its own, one for the same language, or English.
    pub fn resolve(&self, locale: &str) -> &str {
        let locale = normalise(locale);
        if let Some((code, _)) = self.languages.get_key_value(&locale) {
            return code;
        }
        let language = locale.split('_').next().unwrap_or("");
        // the built-in file for a language comes before others of it (es_es before es_mx)
        let same = |code: &&str| code.split('_').next() == Some(language);
        if !language.is_empty()
            && let Some(code) = BUNDLED.iter().map(|(code, _)| *code).find(same).filter(|code| self.languages.contains_key(*code)).or_else(|| self.languages.keys().map(String::as_str).find(same))
        {
            return self.languages.get_key_value(code).map_or(DEFAULT, |(code, _)| code);
        }
        DEFAULT
    }

    /// The text for `key` in `locale`'s language, in English if that language lacks it, or the
    /// key itself if nothing has it.
    pub fn get<'a>(&'a self, locale: &str, key: &'a str) -> &'a str {
        let text = |code: &str| self.languages.get(code).and_then(|texts| texts.get(key)).map(String::as_str);
        text(self.resolve(locale)).or_else(|| text(DEFAULT)).unwrap_or(key)
    }

    /// A kick message as the JSON text component the client is sent.
    ///
    /// `language` is the config's setting. With a language code the text is in that language.
    /// With `"auto"` it is English, except for messages the game has its own words for: those
    /// are sent as the game's key (to clients known to have it), with the English text as the
    /// fallback for clients that can show one.
    pub fn kick_component(&self, language: &str, protocol: i32, key: &str, values: &[(&str, &str)]) -> String {
        let auto = normalise(language) == "auto";
        let own = self.get(if auto { DEFAULT } else { language }, key);
        let text = fill(own, values);
        // A message the admin has reworded in the English file is theirs: the game's words
        // would replace it.
        let reworded = built_in_english(key).is_some_and(|built_in| built_in != own);
        let game_key = GAME_KEYS.iter().find(|(ours, _)| *ours == key).map(|(_, game)| *game).filter(|_| !reworded);
        match game_key {
            Some(game) if auto && protocol >= FALLBACK_FROM => serde_json::json!({ "translate": game, "fallback": text }),
            Some(game) if auto && protocol >= GAME_KEYS_FROM => serde_json::json!({ "translate": game }),
            _ => serde_json::json!({ "text": text }),
        }
        .to_string()
    }
}

/// The English text the plugin was built with, whatever the lang folder says now.
fn built_in_english(key: &str) -> Option<&'static str> {
    static ENGLISH: std::sync::OnceLock<HashMap<String, String>> = std::sync::OnceLock::new();
    ENGLISH.get_or_init(|| serde_json::from_str(BUNDLED.iter().find(|(code, _)| *code == DEFAULT).map_or("{}", |(_, json)| json)).unwrap_or_default()).get(key).map(String::as_str)
}

/// Writes texts into one language's file in the folder, creating it from the built-in file (or
/// empty, for a language that has none) if it isn't there. A text the file already has in a
/// wording of its own is left alone. Used once, for messages an older config.toml had reworded.
pub fn store_texts(folder: &str, locale: &str, texts: &[(String, String)]) -> Result<usize, String> {
    let code = normalise(locale);
    if !valid_code(&code) {
        return Err(format!("\"{code}\" is not a locale name like en_us"));
    }
    std::fs::create_dir_all(folder).map_err(|e| format!("{folder}: {e}"))?;
    let path = format!("{folder}/{code}.json");
    let built_in = BUNDLED.iter().find(|(c, _)| *c == code).map_or("{}", |(_, json)| json);
    let current = std::fs::read_to_string(&path).unwrap_or_else(|_| built_in.to_string());
    let mut file: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&current).map_err(|e| format!("{path}: {e}"))?;
    let original: serde_json::Map<String, serde_json::Value> = serde_json::from_str(built_in).unwrap_or_default();
    let mut stored = 0;
    for (key, text) in texts {
        if file.get(key).is_none() || file.get(key) == original.get(key) {
            file.insert(key.clone(), serde_json::Value::String(text.clone()));
            stored += 1;
        }
    }
    if stored > 0 {
        let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(&path, json + "\n").map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(stored)
}

/// Loads the language folder: writes the built-in files that aren't there yet, then reads every
/// `<locale>.json` in it over the built-in texts. Returns the catalog and what went wrong with
/// any file (the rest still load).
pub fn load_folder(folder: &str) -> (Catalog, Vec<String>) {
    let mut catalog = Catalog::bundled();
    let mut problems = Vec::new();
    if let Err(e) = std::fs::create_dir_all(folder) {
        problems.push(format!("{folder}: {e}"));
        return (catalog, problems);
    }
    for (code, json) in BUNDLED {
        let path = format!("{folder}/{code}.json");
        if !std::path::Path::new(&path).exists()
            && let Err(e) = std::fs::write(&path, json)
        {
            problems.push(format!("{path}: {e}"));
        }
    }
    let Ok(entries) = std::fs::read_dir(folder) else { return (catalog, problems) };
    let mut files: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    for path in files {
        let code = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let result = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|json| catalog.add(&code, &json));
        if let Err(e) = result {
            problems.push(format!("{}: {e}", path.display()));
        }
    }
    (catalog, problems)
}

static ACTIVE: RwLock<Option<Catalog>> = RwLock::new(None);

/// Makes `catalog` the one [`text`] and [`kick`] read.
pub fn install(catalog: Catalog) {
    *ACTIVE.write().unwrap_or_else(|e| e.into_inner()) = Some(catalog);
}

fn with<T>(f: impl FnOnce(&Catalog) -> T) -> T {
    let mut guard = ACTIVE.write().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Catalog::bundled))
}

/// The language for text that isn't for one player in particular: the config's, or English
/// when that is "auto".
pub fn server_language(setting: &str) -> String {
    let setting = normalise(setting);
    if setting == "auto" || setting.is_empty() { DEFAULT.to_string() } else { setting }
}

/// Whether there is a language file for this locale or for its language.
pub fn has(locale: &str) -> bool {
    let locale = normalise(locale);
    with(|catalog| catalog.resolve(&locale) != DEFAULT || locale.starts_with("en_") || locale == "en")
}

static SERVER_LANGUAGE: RwLock<String> = RwLock::new(String::new());

/// Sets the language of the console and of text sent before a player's language is known
/// (the config's `language`; "auto" means English).
pub fn set_server_language(setting: &str) {
    *SERVER_LANGUAGE.write().unwrap_or_else(|e| e.into_inner()) = server_language(setting);
}

/// The language the console speaks.
pub fn console_language() -> String {
    let language = SERVER_LANGUAGE.read().unwrap_or_else(|e| e.into_inner()).clone();
    if language.is_empty() { DEFAULT.to_string() } else { language }
}

/// The text for `key` in the server's language, with `values` filled in: what the console says.
pub fn console(key: &str, values: &[(&str, &str)]) -> String {
    let language = SERVER_LANGUAGE.read().unwrap_or_else(|e| e.into_inner()).clone();
    text(if language.is_empty() { DEFAULT } else { &language }, key, values)
}

/// A console line in colour: the words of the text are painted with `base`, and each
/// `{placeholder}` is replaced by its value, which keeps whatever colour it already has.
pub fn painted(key: &str, base: impl Fn(&str) -> String, values: &[(&str, &str)]) -> String {
    let text = console(key, &[]);
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        let name = &rest[open + 1..open + close];
        let Some((_, value)) = values.iter().find(|(n, _)| *n == name) else {
            // not a placeholder of this message: part of the words
            out.push_str(&base(&rest[..open + close + 1]));
            rest = &rest[open + close + 1..];
            continue;
        };
        if open > 0 {
            out.push_str(&base(&rest[..open]));
        }
        out.push_str(value);
        rest = &rest[open + close + 1..];
    }
    if !rest.is_empty() {
        out.push_str(&base(rest));
    }
    out
}

/// The text for `key` in a player's language, with `values` filled in.
pub fn text(locale: &str, key: &str, values: &[(&str, &str)]) -> String {
    with(|catalog| fill(catalog.get(locale, key), values))
}

/// A kick message as a JSON text component (see [`Catalog::kick_component`]).
pub fn kick(language: &str, protocol: i32, key: &str, values: &[(&str, &str)]) -> String {
    with(|catalog| catalog.kick_component(language, protocol, key, values))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(text: &str) -> Vec<String> {
        let mut found: Vec<String> = text.split('{').skip(1).filter_map(|rest| rest.split_once('}')).map(|(name, _)| name.to_string()).collect();
        found.sort();
        found
    }

    /// Every built-in language has every English key, with the same placeholders, and nothing
    /// English doesn't have.
    #[test]
    fn built_in_languages_are_complete() {
        let catalog = Catalog::bundled();
        assert_eq!(catalog.languages().len(), BUNDLED.len());
        let english = &catalog.languages[DEFAULT];
        assert!(english.len() >= 20);
        for (code, texts) in &catalog.languages {
            assert!(valid_code(code), "{code}");
            for (key, text) in english {
                let translated = texts.get(key).unwrap_or_else(|| panic!("{code} lacks {key}"));
                assert!(!translated.trim().is_empty(), "{code} {key} is empty");
                assert_eq!(placeholders(translated), placeholders(text), "{code} {key}");
            }
            for key in texts.keys() {
                assert!(english.contains_key(key), "{code} has {key}, which English doesn't");
            }
        }
        // translations really are translations
        for (code, _) in BUNDLED.iter().filter(|(code, _)| *code != DEFAULT) {
            assert_ne!(catalog.get(code, "kick.server_starting"), catalog.get(DEFAULT, "kick.server_starting"), "{code}");
        }
    }

    #[test]
    fn a_locale_reads_its_own_file_or_one_for_its_language() {
        let mut catalog = Catalog::bundled();
        assert_eq!(catalog.resolve("de_de"), "de_de");
        assert_eq!(catalog.resolve("de_AT"), "de_de");
        assert_eq!(catalog.resolve("pt-PT"), "pt_br");
        assert_eq!(catalog.resolve("zh_tw"), "zh_cn");
        assert_eq!(catalog.resolve("sv_se"), DEFAULT);
        assert_eq!(catalog.resolve(""), DEFAULT);
        assert_eq!(catalog.resolve("auto"), DEFAULT);
        // a file for the exact locale wins over the language's built-in one
        catalog.add("es_mx", r#"{"kick.username_mismatch": "El nombre no coincide, compa"}"#).unwrap();
        assert_eq!(catalog.resolve("es_mx"), "es_mx");
        assert_eq!(catalog.resolve("es_ar"), "es_es");
        assert_eq!(catalog.get("es_mx", "kick.username_mismatch"), "El nombre no coincide, compa");
        // what the new file doesn't have comes from English
        assert_eq!(catalog.get("es_mx", "kick.internal_error"), "Internal proxy error");
        // a language nobody shipped
        catalog.add("sv_se", r#"{"kick.internal_error": "Internt proxyfel"}"#).unwrap();
        assert_eq!(catalog.get("sv_fi", "kick.internal_error"), "Internt proxyfel");
        assert_eq!(catalog.get("sv_se", "no.such.key"), "no.such.key");
    }

    #[test]
    fn a_file_changes_only_the_texts_it_has() {
        let mut catalog = Catalog::bundled();
        let count = catalog.add("de_de", r#"{"kick.internal_error": "Kaputt", "_comment": {"by": "admin"}, "extra.key": "Neu"}"#).unwrap();
        assert_eq!(count, 2);
        assert_eq!(catalog.get("de_de", "kick.internal_error"), "Kaputt");
        assert_eq!(catalog.get("de_de", "extra.key"), "Neu");
        assert!(catalog.get("de_de", "kick.server_starting").starts_with("Der Server"));
        assert!(catalog.add("de_de", "not json").is_err());
        assert!(catalog.add("de_de", "[1, 2]").is_err());
        assert!(catalog.add("../etc", "{}").is_err());
        assert!(catalog.add("Deutsch", "{}").is_err());
        assert!(valid_code("fil_ph") && valid_code("en_us") && !valid_code("en") && !valid_code("en_us_x") && !valid_code("e_us"));
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(fill("{a} and {b}, {a}", &[("a", "1"), ("b", "2")]), "1 and 2, 1");
        assert_eq!(fill("nothing {here}", &[]), "nothing {here}");
        let catalog = Catalog::bundled();
        let shown = fill(catalog.get("fr_fr", "kick.unsupported_version"), &[("versions", "1.18-26.3"), ("client", "1.8.9")]);
        assert_eq!(shown, "Ce serveur accepte Minecraft 1.18-26.3. Vous utilisez 1.8.9.");
    }

    #[test]
    fn kick_messages_follow_the_language_setting() {
        let catalog = Catalog::bundled();
        let json = |language: &str, protocol: i32, key: &str| -> serde_json::Value { serde_json::from_str(&catalog.kick_component(language, protocol, key, &[("error", "bad varint")])).unwrap() };
        // a language code: that language, as plain text, for every client
        assert_eq!(json("de_de", 777, "kick.invalid_session"), serde_json::json!({ "text": "Ungültige Sitzung (starte das Spiel und den Launcher neu)" }));
        assert_eq!(json("de-DE", 757, "kick.protocol_error"), serde_json::json!({ "text": "Protokollfehler: bad varint" }));
        // auto: the game's own words where it has them
        assert_eq!(
            json("auto", 777, "kick.invalid_session"),
            serde_json::json!({ "translate": "multiplayer.disconnect.unverified_username", "fallback": "Invalid session (Try restarting your game and the launcher)" })
        );
        assert_eq!(json("auto", 758, "kick.auth_servers_down"), serde_json::json!({ "translate": "multiplayer.disconnect.authservers_down" }));
        // ... but not for a client too old to be sure of, or a message the game doesn't have
        assert_eq!(json("auto", 47, "kick.invalid_session"), serde_json::json!({ "text": "Invalid session (Try restarting your game and the launcher)" }));
        assert_eq!(json("Auto", 777, "kick.protocol_error"), serde_json::json!({ "text": "Protocol error: bad varint" }));
        // a language there is no file for: English
        assert_eq!(json("xx_yy", 777, "kick.internal_error"), serde_json::json!({ "text": "Internal proxy error" }));
        // a message reworded in the English file is sent as written, not as the game's own
        let mut reworded = Catalog::bundled();
        reworded.add("en_us", r#"{"kick.invalid_session": "Log in again, please"}"#).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&reworded.kick_component("auto", 777, "kick.invalid_session", &[])).unwrap(), serde_json::json!({ "text": "Log in again, please" }));
        assert_eq!(server_language("auto"), "en_us");
        assert!(has("de_at") && has("en_gb") && has("pt-PT") && !has("xx_yy") && !has("sv_se"));
        assert_eq!(server_language("pt-BR"), "pt_br");
    }

    #[test]
    fn console_lines_keep_the_colours_of_their_values() {
        let upper = |words: &str| words.to_uppercase();
        // kick.protocol_error is "Protocol error: {error}" in English
        assert_eq!(painted("kick.protocol_error", upper, &[("error", "<bad varint>")]), "PROTOCOL ERROR: <bad varint>");
        assert_eq!(painted("kick.unsupported_version", upper, &[("versions", "[1.18]"), ("client", "[1.8]")]), "THIS SERVER SUPPORTS MINECRAFT [1.18]. YOU ARE ON [1.8].");
        // a placeholder nobody gave a value for stays as it is
        assert_eq!(painted("kick.protocol_error", upper, &[]), "PROTOCOL ERROR: {ERROR}");
        assert_eq!(painted("kick.username_mismatch", upper, &[]), "USERNAME MISMATCH");
        assert_eq!(console("kick.protocol_error", &[("error", "x")]), "Protocol error: x");
    }

    /// Messages an older config.toml had reworded land in the language file, once.
    #[test]
    fn moved_messages_are_stored_in_the_language_file() {
        let folder = std::env::temp_dir().join(format!("multiversion-moved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.to_str().unwrap();
        let moved = [("kick.invalid_session".to_string(), "Log in again".to_string())];
        assert_eq!(store_texts(path, "en_us", &moved), Ok(1));
        let (catalog, problems) = load_folder(path);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(catalog.get("en_us", "kick.invalid_session"), "Log in again");
        assert_eq!(catalog.get("en_us", "kick.username_mismatch"), "Username mismatch", "the rest of the file is the built-in one");
        // a second time changes nothing: the file has its own wording now
        assert_eq!(store_texts(path, "en_us", &[("kick.invalid_session".to_string(), "Something else".to_string())]), Ok(0));
        assert_eq!(load_folder(path).0.get("en_us", "kick.invalid_session"), "Log in again");
        // a language with no built-in file gets a file of its own
        assert_eq!(store_texts(path, "sv_se", &moved), Ok(1));
        assert_eq!(load_folder(path).0.get("sv_se", "kick.invalid_session"), "Log in again");
        assert!(store_texts(path, "../x", &moved).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_folder_is_filled_and_read() {
        let folder = std::env::temp_dir().join(format!("multiversion-lang-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.to_str().unwrap();
        // first start: the built-in files appear
        let (catalog, problems) = load_folder(path);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(catalog.languages().len(), BUNDLED.len());
        for (code, json) in BUNDLED {
            assert_eq!(std::fs::read_to_string(folder.join(format!("{code}.json"))).unwrap(), *json);
        }
        // the admin edits one, adds one, and leaves a broken one and a stray file
        std::fs::write(folder.join("en_us.json"), r#"{"kick.internal_error": "Oops"}"#).unwrap();
        std::fs::write(folder.join("sv_se.json"), r#"{"kick.internal_error": "Internt proxyfel"}"#).unwrap();
        std::fs::write(folder.join("da_dk.json"), "{ broken").unwrap();
        std::fs::write(folder.join("notes.txt"), "hello").unwrap();
        let (catalog, problems) = load_folder(path);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("da_dk.json"));
        assert_eq!(catalog.get("en_us", "kick.internal_error"), "Oops");
        assert_eq!(catalog.get("en_us", "kick.username_mismatch"), "Username mismatch", "what the edited file lacks is still there");
        assert_eq!(catalog.get("sv_se", "kick.internal_error"), "Internt proxyfel");
        assert!(catalog.languages().contains(&"sv_se"));
        // an edited file is not written over
        assert_eq!(std::fs::read_to_string(folder.join("en_us.json")).unwrap(), r#"{"kick.internal_error": "Oops"}"#);
        let _ = std::fs::remove_dir_all(&folder);
    }
}

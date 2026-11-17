//! ANSI colours for console messages, like Pumpkin's own output. The web panel and log files that
//! strip ANSI codes show plain text. Turned off with `console_colors = false` in config.toml.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(true);
/// Text is being made for chat or a kick screen (see `with_colours`), not for the console.
static FOR_CHAT: AtomicBool = AtomicBool::new(false);

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Any ANSI colour code.
pub fn paint(code: &str, s: &str) -> String {
    if ENABLED.load(Ordering::Relaxed) { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
}

/// Text from a player's client, made safe to print: control characters (escape codes, line
/// breaks) are dropped so it cannot colour, clear or forge console lines, and it is cut to `max`
/// characters.
pub fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

/// `s` without ANSI colour codes, for chat (which shows them as garbage).
pub fn strip(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // skip "[...m"
            for d in chars.by_ref() {
                if d == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Runs `f` with colours on, whatever `console_colors` says. For text that is turned into chat
/// colours afterwards rather than printed.
pub fn with_colours<T>(f: impl FnOnce() -> T) -> T {
    let was = ENABLED.swap(true, Ordering::Relaxed);
    let was_chat = FOR_CHAT.swap(true, Ordering::Relaxed);
    let out = f();
    ENABLED.store(was, Ordering::Relaxed);
    FOR_CHAT.store(was_chat, Ordering::Relaxed);
    out
}

/// The sixteen colours of the game, in the order of their codes `0` to `f`: the name chat
/// knows them by, their red, green and blue, and the nearest console colour.
const GAME_COLOURS: [(&str, (u8, u8, u8), u8); 16] = [
    ("black", (0x00, 0x00, 0x00), 30),
    ("dark_blue", (0x00, 0x00, 0xaa), 34),
    ("dark_green", (0x00, 0xaa, 0x00), 32),
    ("dark_aqua", (0x00, 0xaa, 0xaa), 36),
    ("dark_red", (0xaa, 0x00, 0x00), 31),
    ("dark_purple", (0xaa, 0x00, 0xaa), 35),
    ("gold", (0xff, 0xaa, 0x00), 33),
    ("gray", (0xaa, 0xaa, 0xaa), 37),
    ("dark_gray", (0x55, 0x55, 0x55), 90),
    ("blue", (0x55, 0x55, 0xff), 94),
    ("green", (0x55, 0xff, 0x55), 92),
    ("aqua", (0x55, 0xff, 0xff), 96),
    ("red", (0xff, 0x55, 0x55), 91),
    ("light_purple", (0xff, 0x55, 0xff), 95),
    ("yellow", (0xff, 0xff, 0x55), 93),
    ("white", (0xff, 0xff, 0xff), 97),
];

/// How a stretch of text looks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Look {
    /// A game colour's name or `#rrggbb`; `None` is the default (white in chat).
    colour: Option<String>,
    bold: bool,
    italic: bool,
    underlined: bool,
    strikethrough: bool,
    obfuscated: bool,
}

/// Whether `text` has a colour or format code in it (see [`codes`]).
pub fn has_codes(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if (c == '&' || c == '§') && chars.peek().is_some_and(|n| n.is_ascii_hexdigit() || matches!(n.to_ascii_lowercase(), 'k'..='o' | 'r' | '#')) {
            return true;
        }
    }
    false
}

/// Turns the game's colour codes in a text from a language file into console colours.
///
/// `&0` to `&f` are the sixteen colours, `&#rrggbb` any colour, `&l` bold, `&o` italic, `&n`
/// underlined, `&m` struck through, `&k` scrambled, `&r` back to normal; `§` works like `&`,
/// and `&&` is a plain `&`. As in the game, a colour ends the formats before it. An `&` that
/// starts none of these is left as it is. With console colours off the codes are removed.
///
/// Chat and kick screens get the same look: the console codes this writes are what
/// [`chat_json`] reads.
pub fn codes(text: &str) -> String {
    if !text.contains(['&', '§']) {
        return text.to_string();
    }
    let enabled = ENABLED.load(Ordering::Relaxed);
    let exact = FOR_CHAT.load(Ordering::Relaxed);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 16);
    let mut look = Look::default();
    let mut rgb: Option<(u8, u8, u8)> = None;
    let mut console: Option<u8> = None;
    let mut coloured = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if !(c == '&' || c == '§') || next.is_none() {
            out.push(c);
            i += 1;
            continue;
        }
        let code = next.unwrap_or(' ').to_ascii_lowercase();
        let mut used = 2;
        match code {
            '&' if c == '&' => {
                out.push('&');
                i += 2;
                continue;
            }
            '0'..='9' | 'a'..='f' => {
                let (_, colour, nearest) = GAME_COLOURS[code.to_digit(16).unwrap_or(15) as usize];
                look = Look::default();
                rgb = Some(colour);
                console = Some(nearest);
            }
            '#' => {
                let hex: String = chars[i + 2..].iter().take(6).collect();
                let parsed = (hex.len() == 6 && hex.chars().all(|h| h.is_ascii_hexdigit())).then(|| u32::from_str_radix(&hex, 16).ok()).flatten();
                let Some(value) = parsed else {
                    out.push(c);
                    i += 1;
                    continue;
                };
                look = Look::default();
                rgb = Some(((value >> 16) as u8, (value >> 8) as u8, value as u8));
                console = None;
                used = 8;
            }
            'l' => look.bold = true,
            'o' => look.italic = true,
            'n' => look.underlined = true,
            'm' => look.strikethrough = true,
            'k' => look.obfuscated = true,
            'r' => {
                look = Look::default();
                rgb = None;
                console = None;
            }
            _ => {
                out.push(c);
                i += 1;
                continue;
            }
        }
        i += used;
        if !enabled {
            continue;
        }
        // the whole look each time: a console code here starts from nothing
        out.push_str("\x1b[0");
        for (on, sgr) in [(look.bold, 1), (look.italic, 3), (look.underlined, 4), (look.strikethrough, 9), (look.obfuscated, 8)] {
            if on {
                out.push_str(&format!(";{sgr}"));
            }
        }
        match (rgb, console) {
            // the console gets its own nearest colour; chat gets the exact one
            (Some(_), Some(nearest)) if !exact => out.push_str(&format!(";{nearest}")),
            (Some((r, g, b)), _) => out.push_str(&format!(";38;2;{r};{g};{b}")),
            _ => {}
        }
        out.push('m');
        coloured = true;
    }
    if coloured {
        out.push_str("\x1b[0m");
    }
    out
}

/// The look a console colour code stands for: every number in it applied to plain text.
fn look_of(code: &str) -> Look {
    let mut look = Look::default();
    let mut parts = code.split(';').map(|p| p.parse::<u32>().unwrap_or(0));
    while let Some(n) = parts.next() {
        match n {
            0 => look = Look::default(),
            1 => look.bold = true,
            3 => look.italic = true,
            4 => look.underlined = true,
            9 => look.strikethrough = true,
            8 => look.obfuscated = true,
            // the plugin's own "detail" grey reads better in chat as the lighter grey
            90 if !code.contains(';') => look.colour = Some("gray".into()),
            30..=37 | 90..=97 => look.colour = GAME_COLOURS.iter().find(|(_, _, sgr)| u32::from(*sgr) == n).map(|(name, _, _)| (*name).to_string()),
            38 => {
                if parts.next() == Some(2) {
                    let (r, g, b) = (parts.next().unwrap_or(0) as u8, parts.next().unwrap_or(0) as u8, parts.next().unwrap_or(0) as u8);
                    look.colour = Some(match GAME_COLOURS.iter().find(|(_, rgb, _)| *rgb == (r, g, b)) {
                        Some((name, _, _)) => (*name).to_string(),
                        None => format!("#{r:02x}{g:02x}{b:02x}"),
                    });
                }
            }
            _ => {}
        }
    }
    look
}

/// An ANSI-coloured console line as a chat message (a JSON text component) with the same colours.
pub fn chat_json(line: &str) -> String {
    let mut parts = Vec::new();
    let mut look = Look::default();
    let mut rest = line;
    let mut push = |text: &str, look: &Look| {
        if text.is_empty() {
            return;
        }
        let mut part = serde_json::json!({ "text": text, "color": look.colour.as_deref().unwrap_or("white") });
        for (on, name) in [(look.bold, "bold"), (look.italic, "italic"), (look.underlined, "underlined"), (look.strikethrough, "strikethrough"), (look.obfuscated, "obfuscated")] {
            if on {
                part[name] = serde_json::json!(true);
            }
        }
        if text.starts_with("https://") && !text.contains(' ') {
            part["click_event"] = serde_json::json!({ "action": "open_url", "url": text });
            part["hover_event"] = serde_json::json!({ "action": "show_text", "value": [{ "text": "Click to open", "color": "gray" }] });
        }
        parts.push(part);
    };
    while let Some(start) = rest.find("\x1b[") {
        push(&rest[..start], &look);
        let after = &rest[start + 2..];
        let Some(end) = after.find('m') else {
            rest = "";
            break;
        };
        look = look_of(&after[..end]);
        rest = &after[end + 1..];
    }
    push(rest, &look);
    serde_json::json!({ "text": "", "extra": parts }).to_string()
}

/// A chat message from [`chat_json`] with every `#rrggbb` colour made white, for game versions
/// that only know the named colours.
pub fn without_hex(json: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(json) else { return json.to_string() };
    for part in value["extra"].as_array_mut().into_iter().flatten() {
        if part["color"].as_str().is_some_and(|c| c.starts_with('#')) {
            part["color"] = serde_json::json!("white");
        }
    }
    value.to_string()
}

/// Player names.
pub fn name(s: &str) -> String { paint("1;97", s) }
/// Minecraft versions.
pub fn version(s: &str) -> String { paint("96", s) }
/// Addresses, UUIDs and other detail.
pub fn dim(s: &str) -> String { paint("90", s) }
/// Successful events (joined, listening).
pub fn good(s: &str) -> String { paint("92", s) }
/// Neutral departures and warnings.
pub fn warn(s: &str) -> String { paint("93", s) }
/// Failures.
pub fn bad(s: &str) -> String { paint("91", s) }
/// The plugin's tag.
pub fn tag(s: &str) -> String { paint("94", s) }

#[cfg(test)]
mod tests {
    use super::*;

    /// The colour switch is one setting for the whole process: tests that flip it take turns.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }
    #[test]
    fn colours_and_plain() {
        let _guard = lock();
        set_enabled(true);
        assert_eq!(good("joined"), "\x1b[92mjoined\x1b[0m");
        set_enabled(false);
        assert_eq!(good("joined"), "joined");
        set_enabled(true);
    }
    #[test]
    fn console_lines_become_coloured_chat() {
        let _guard = lock();
        let line = with_colours(|| format!("{} {} {}", warn("v1.2 is out"), dim("Pumpkin Market:"), version("https://market.pumpkinmc.org/plugin/x")));
        let v: serde_json::Value = serde_json::from_str(&chat_json(&line)).unwrap();
        let extra = v["extra"].as_array().unwrap();
        assert_eq!(extra[0], serde_json::json!({ "text": "v1.2 is out", "color": "yellow" }));
        assert_eq!(extra[1], serde_json::json!({ "text": " ", "color": "white" }));
        assert_eq!(extra[2]["color"], "gray");
        let link = extra.last().unwrap();
        assert_eq!(link["color"], "aqua");
        assert_eq!(link["click_event"]["url"], "https://market.pumpkinmc.org/plugin/x");
        // bold names, and text with no codes at all
        let v: serde_json::Value = serde_json::from_str(&chat_json(&with_colours(|| name("Steve")))).unwrap();
        assert_eq!(v["extra"][0], serde_json::json!({ "text": "Steve", "color": "white", "bold": true }));
        let v: serde_json::Value = serde_json::from_str(&chat_json("plain")).unwrap();
        assert_eq!(v["extra"][0]["text"], "plain");
    }
    #[test]
    fn with_colours_restores_the_setting() {
        let _guard = lock();
        set_enabled(false);
        assert_eq!(with_colours(|| good("x")), "\x1b[92mx\x1b[0m");
        assert_eq!(good("x"), "x");
        set_enabled(true);
    }
    #[test]
    fn strip_removes_colour_codes() {
        let _guard = lock();
        assert_eq!(strip("\x1b[92mjoined\x1b[0m as \x1b[1;97mSteve\x1b[0m"), "joined as Steve");
        assert_eq!(strip("plain"), "plain");
    }
    #[test]
    fn client_text_cannot_forge_console_lines() {
        let _guard = lock();
        assert_eq!(clean("Steve\x1b[2J\n[Server] ops: Steve", 64), "Steve[2J[Server] ops: Steve");
        assert_eq!(clean("\u{9b}31mred\r", 64), "31mred");
        assert_eq!(clean("abcdef", 3), "abc");
    }
    #[test]
    fn game_colour_codes_become_console_colours() {
        let _guard = lock();
        set_enabled(true);
        assert_eq!(codes("plain text"), "plain text");
        assert_eq!(codes("&aJoined"), "\x1b[0;92mJoined\x1b[0m");
        // a format adds to the colour before it; a colour ends the formats before it
        assert_eq!(codes("&c&lStop &7now"), "\x1b[0;91m\x1b[0;1;91mStop \x1b[0;37mnow\x1b[0m");
        assert_eq!(codes("§e§nlink&r off"), "\x1b[0;93m\x1b[0;4;93mlink\x1b[0m off\x1b[0m");
        assert_eq!(codes("&#ff8800orange"), "\x1b[0;38;2;255;136;0morange\x1b[0m");
        // what is not a code stays: a lone &, an unknown letter, half a colour, an escaped &
        assert_eq!(codes("Tom & Jerry &z &#12 R&&D &"), "Tom & Jerry &z &#12 R&D &");
        assert!(has_codes("&aok") && has_codes("x §l") && has_codes("&#ff8800") && has_codes("&R"));
        assert!(!has_codes("Tom & Jerry") && !has_codes("a&") && !has_codes("&z") && !has_codes(""));
        // with console colours off the codes go and the words stay
        set_enabled(false);
        assert_eq!(codes("&c&lStop &7now &#ff8800!"), "Stop now !");
        set_enabled(true);
    }

    #[test]
    fn coded_text_reaches_chat_with_the_exact_colours() {
        let _guard = lock();
        let json = with_colours(|| chat_json(&codes("&6Gold &lbold&r plain &#12ab34hex &8&odark &kx")));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let extra = v["extra"].as_array().unwrap();
        assert_eq!(extra[0], serde_json::json!({ "text": "Gold ", "color": "gold" }));
        assert_eq!(extra[1], serde_json::json!({ "text": "bold", "color": "gold", "bold": true }));
        assert_eq!(extra[2], serde_json::json!({ "text": " plain ", "color": "white" }));
        assert_eq!(extra[3], serde_json::json!({ "text": "hex ", "color": "#12ab34" }));
        assert_eq!(extra[4], serde_json::json!({ "text": "dark ", "color": "dark_gray", "italic": true }));
        assert_eq!(extra[5], serde_json::json!({ "text": "x", "color": "dark_gray", "italic": true, "obfuscated": true }));
        // every one of the sixteen comes out under its own name
        for (i, (name, _, _)) in GAME_COLOURS.iter().enumerate() {
            let json = with_colours(|| chat_json(&codes(&format!("&{i:x}x"))));
            assert!(json.contains(&format!("\"color\":\"{name}\"")), "{i:x}: {json}");
        }
        // the plugin's own console colours still read as before
        let line = with_colours(|| format!("{}{}", name("Steve"), dim(" detail")));
        let v: serde_json::Value = serde_json::from_str(&chat_json(&line)).unwrap();
        assert_eq!(v["extra"][0], serde_json::json!({ "text": "Steve", "color": "white", "bold": true }));
        assert_eq!(v["extra"][1]["color"], "gray");
    }

}

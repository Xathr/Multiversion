//! ANSI colours for console messages, like Pumpkin's own output. The web panel and log files that
//! strip ANSI codes show plain text. Turned off with `console_colors = false` in config.toml.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(true);

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
    let out = f();
    ENABLED.store(was, Ordering::Relaxed);
    out
}

/// The chat colour for an ANSI colour code used in this file, and whether it is bold.
fn chat_colour(code: &str) -> (&'static str, bool) {
    match code {
        "1;97" => ("white", true),
        "96" => ("aqua", false),
        "90" => ("gray", false),
        "92" => ("green", false),
        "93" => ("yellow", false),
        "91" => ("red", false),
        "94" => ("blue", false),
        _ => ("white", false),
    }
}

/// An ANSI-coloured console line as a chat message (a JSON text component) with the same colours.
/// Links are clickable.
pub fn chat_json(line: &str) -> String {
    let mut parts = Vec::new();
    let (mut colour, mut bold) = ("white", false);
    let mut rest = line;
    let mut push = |text: &str, colour: &str, bold: bool| {
        if text.is_empty() {
            return;
        }
        let mut part = serde_json::json!({ "text": text, "color": colour });
        if bold {
            part["bold"] = serde_json::json!(true);
        }
        if text.starts_with("https://") && !text.contains(' ') {
            part["click_event"] = serde_json::json!({ "action": "open_url", "url": text });
            part["hover_event"] = serde_json::json!({ "action": "show_text", "value": [{ "text": "Click to open", "color": "gray" }] });
        }
        parts.push(part);
    };
    while let Some(start) = rest.find("\x1b[") {
        push(&rest[..start], colour, bold);
        let after = &rest[start + 2..];
        let Some(end) = after.find('m') else {
            rest = "";
            break;
        };
        (colour, bold) = chat_colour(&after[..end]);
        rest = &after[end + 1..];
    }
    push(rest, colour, bold);
    serde_json::json!({ "text": "", "extra": parts }).to_string()
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
    #[test]
    fn colours_and_plain() {
        set_enabled(true);
        assert_eq!(good("joined"), "\x1b[92mjoined\x1b[0m");
        set_enabled(false);
        assert_eq!(good("joined"), "joined");
        set_enabled(true);
    }
    #[test]
    fn console_lines_become_coloured_chat() {
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
        set_enabled(false);
        assert_eq!(with_colours(|| good("x")), "\x1b[92mx\x1b[0m");
        assert_eq!(good("x"), "x");
        set_enabled(true);
    }
    #[test]
    fn strip_removes_colour_codes() {
        assert_eq!(strip("\x1b[92mjoined\x1b[0m as \x1b[1;97mSteve\x1b[0m"), "joined as Steve");
        assert_eq!(strip("plain"), "plain");
    }
    #[test]
    fn client_text_cannot_forge_console_lines() {
        assert_eq!(clean("Steve\x1b[2J\n[Server] ops: Steve", 64), "Steve[2J[Server] ops: Steve");
        assert_eq!(clean("\u{9b}31mred\r", 64), "31mred");
        assert_eq!(clean("abcdef", 3), "abc");
    }
}

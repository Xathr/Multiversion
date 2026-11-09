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

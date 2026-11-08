//! ANSI colours for console messages, like Pumpkin's own output. The web panel and log files that
//! strip ANSI codes show plain text. Turned off with `console_colors = false` in config.toml.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

fn paint(code: &str, s: &str) -> String {
    if ENABLED.load(Ordering::Relaxed) { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
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
pub fn tag(s: &str) -> String { paint("95", s) }

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
}

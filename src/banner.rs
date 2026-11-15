//! The startup banner printed to the console when the plugin loads.

use crate::color;

/// The multiversion logo, three rows tall. Each row is split after "multi" so the halves can be
/// coloured differently.
const LOGO: [(&str, &str); 3] = [
    ("┌┬┐┬ ┬┬  ┌┬┐┬", "┬  ┬┌─┐┬─┐┌─┐┬┌─┐┌┐┌"),
    ("││││ ││   │ │", "└┐┌┘├┤ ├┬┘└─┐││ ││││"),
    ("┴ ┴└─┘┴─┘ ┴ ┴", " └┘ └─┘┴└─└─┘┴└─┘┘└┘"),
];

/// What the banner reports about the current settings.
pub struct Info<'a> {
    pub version: &'a str,
    /// The client versions allowed in, like "1.21-26.3" (see `translate::allowed_range`).
    pub range: &'a str,
    pub listen: &'a str,
    pub backend: &'a str,
    pub online_mode: bool,
    /// The proxy players must come through ("Velocity"), if the server is behind one.
    pub proxy: Option<&'a str>,
}

/// The lowest and highest version in a range like "1.21-26.3" or "1.21, 1.21.4-26.3", or `None`
/// when no version is allowed.
fn ends(range: &str) -> Option<(&str, &str)> {
    let is_version = |s: &&str| s.starts_with(|c: char| c.is_ascii_digit());
    let first = range.split([',', ' ', '-']).find(is_version)?;
    let last = range.rsplit([',', ' ', '-']).find(is_version)?;
    Some((first, last))
}

/// The banner, one console line per entry.
pub fn lines(info: &Info) -> Vec<String> {
    let ribbon = match ends(info.range) {
        Some((low, high)) => format!(
            "{} {}{}{} {}",
            color::version(low),
            color::paint("95", "◆"),
            color::dim("━━━━━━━━━━"),
            color::paint("95", "◆"),
            color::version(high)
        ),
        None => color::bad(info.range),
    };
    let beside = [
        String::new(),
        format!("{} {}", color::name("multiversion"), color::dim(&format!("v{}", info.version))),
        ribbon,
    ];
    let mut out: Vec<String> = LOGO
        .iter()
        .zip(beside)
        .map(|((multi, version), side)| {
            let row = format!("  {}{}", color::version(multi), color::paint("94", version));
            if side.is_empty() { row } else { format!("{row}   {side}") }
        })
        .collect();

    let tr = |key: &str| crate::i18n::console(key, &[]);
    let login = if let Some(proxy) = info.proxy {
        format!("{} {}", color::good(proxy), color::dim(&tr("banner.proxy_detail")))
    } else if info.online_mode {
        format!("{} {}", color::good(&tr("console.online")), color::dim(&tr("banner.online_detail")))
    } else {
        format!("{} {}", color::warn(&tr("console.offline")), color::dim(&tr("banner.offline_detail")))
    };
    for (key, value) in [
        ("banner.versions", color::version(info.range)),
        ("banner.listening", color::name(info.listen)),
        ("banner.pumpkin", color::dim(info.backend)),
        ("banner.login", login),
    ] {
        out.push(format!("  {} {value}", color::dim(&label(&tr(key)))));
    }
    out
}

/// A banner row's name, padded so the values line up in a column (13 characters, or the name
/// and a space if a language's word is longer).
pub fn label(name: &str) -> String {
    let width = name.chars().count();
    format!("{name}{}", " ".repeat(13usize.saturating_sub(width).max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Info<'static> {
        Info { version: "1.2.3", range: "1.21-26.3", listen: "0.0.0.0:25565", backend: "127.0.0.1:25566", online_mode: true, proxy: None }
    }

    fn strip(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn logo_rows_line_up() {
        let widths: Vec<usize> = LOGO.iter().map(|(a, b)| a.chars().count() + b.chars().count()).collect();
        assert!(widths.iter().all(|w| *w == widths[0]), "{widths:?}");
    }

    #[test]
    fn range_ends() {
        assert_eq!(ends("1.21-26.3"), Some(("1.21", "26.3")));
        assert_eq!(ends("1.21, 1.21.4-26.3"), Some(("1.21", "26.3")));
        assert_eq!(ends("26.3"), Some(("26.3", "26.3")));
        assert_eq!(ends("no version"), None);
    }

    #[test]
    fn banner_contents() {
        // Strip colours here rather than turning them off: other tests share that switch.
        let lines: Vec<String> = lines(&info()).iter().map(|l| strip(l)).collect();
        assert!(lines[1].ends_with("multiversion v1.2.3"));
        assert!(lines[2].ends_with("1.21 ◆━━━━━━━━━━◆ 26.3"));
        assert!(lines.iter().any(|l| l.contains("listening on") && l.ends_with("0.0.0.0:25565")));
        assert!(lines.iter().any(|l| l.contains("login") && l.contains("online")));
    }
}

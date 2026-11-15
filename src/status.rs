//! `/multiversion status`: who is online on which game version.

use std::sync::Mutex;

/// One player in the game: name, the protocol number and name of their game version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Online {
    pub name: String,
    pub protocol: i32,
    pub version: String,
}

/// The players in the game right now. The proxy loop keeps it up to date; the command reads it
/// from here because it can run while the loop is in the middle of a turn.
static PLAYERS: Mutex<Vec<Online>> = Mutex::new(Vec::new());

pub fn set(players: Vec<Online>) {
    let mut current = PLAYERS.lock().unwrap_or_else(|e| e.into_inner());
    if *current != players {
        *current = players;
    }
}

pub fn get() -> Vec<Online> {
    PLAYERS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The players by version, newest version first, names in alphabetical order.
pub fn by_version(players: &[Online]) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(i32, String, Vec<String>)> = Vec::new();
    for p in players {
        match groups.iter_mut().find(|g| g.0 == p.protocol) {
            Some(group) => group.2.push(p.name.clone()),
            None => groups.push((p.protocol, p.version.clone(), vec![p.name.clone()])),
        }
    }
    groups.sort_by(|a, b| b.0.cmp(&a.0));
    groups
        .into_iter()
        .map(|(_, version, mut names)| {
            names.sort_by_key(|n| n.to_lowercase());
            (version, names)
        })
        .collect()
}

/// The reply to the command in `locale`: a summary line, then one line per version.
/// `paint` colours a (kind, text) pair; kinds are "version", "name" and "dim".
pub fn lines(players: &[Online], locale: &str, paint: &dyn Fn(&str, &str) -> String) -> Vec<String> {
    if players.is_empty() {
        return vec![paint("dim", &crate::i18n::text(locale, "command.status_none", &[]))];
    }
    let groups = by_version(players);
    let mut out = vec![paint("dim", &crate::i18n::text(locale, "command.status_summary", &[("players", &players.len().to_string()), ("versions", &groups.len().to_string())]))];
    for (version, names) in groups {
        let list = names.iter().map(|n| paint("name", n)).collect::<Vec<_>>().join(&paint("dim", ", "));
        out.push(format!("{} {} {list}", paint("version", &version), paint("dim", &format!("({})", names.len()))));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(name: &str, protocol: i32, version: &str) -> Online {
        Online { name: name.into(), protocol, version: version.into() }
    }

    #[test]
    fn players_are_listed_by_version_newest_first() {
        let players = [on("zed", 758, "1.18.2"), on("Amy", 777, "26.3"), on("bob", 758, "1.18.2"), on("Cy", 769, "1.21.4")];
        assert_eq!(by_version(&players), [("26.3".to_string(), vec!["Amy".to_string()]), ("1.21.4".to_string(), vec!["Cy".to_string()]), ("1.18.2".to_string(), vec!["bob".to_string(), "zed".to_string()])]);
        let plain = |_: &str, text: &str| text.to_string();
        assert_eq!(lines(&players, "en_us", &plain), ["players online: 4, versions in use: 3", "26.3 (1) Amy", "1.21.4 (1) Cy", "1.18.2 (2) bob, zed"]);
        assert_eq!(lines(&[], "en_us", &plain), ["no players online"]);
        // in the asker's language, where there is a file for it
        assert_eq!(lines(&[], "de_de", &plain), ["keine Spieler online"]);
    }

    #[test]
    fn the_list_is_only_replaced_when_it_changed() {
        set(vec![on("a", 777, "26.3")]);
        assert_eq!(get(), [on("a", 777, "26.3")]);
        set(Vec::new());
        assert!(get().is_empty());
    }
}

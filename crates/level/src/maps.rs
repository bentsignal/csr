//! Installed maps and experimental archives from Valve's Workshop snapshots
//! (`workshop` crate), listed by `csr maps`. Archives load with
//! [`crate::Level::load`], which fetches them on first use. They are not
//! offered in the match menu until navigation and fidelity are validated.

use std::path::Path;

/// Where a listed map's data is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapStatus {
    /// BSP available locally; does not imply validated navigation or assets.
    Ready,
    /// An old map a Steam library already has: the first load copies it out.
    InSteam,
    /// An old map the first load downloads from Valve's CDN.
    Download,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapEntry {
    /// The name to load (`de_dust2`, `de_dust2_2017`).
    pub name: String,
    /// Menu title (`DUST2`, `Dust II (2017)`).
    pub title: String,
    /// The original map this is a version of (its own name for installed
    /// maps). Menus can borrow the base map's artwork.
    pub base: String,
    /// Set for old map versions.
    pub old: Option<&'static workshop::OldMap>,
    pub status: MapStatus,
}

/// Map name prefixes the game modes support: bomb defusal and hostage.
pub fn playable(name: &str) -> bool {
    name.starts_with("de_") || name.starts_with("cs_")
}

/// Menu title for a map name.
pub fn title(name: &str) -> String {
    match workshop::find(name) {
        Some(old) => old.title.to_string(),
        None => name.trim_start_matches("de_").trim_start_matches("cs_").replace('_', " ").to_uppercase(),
    }
}

/// The original map a name is a version of (`de_dust2_2017` → `de_dust2`).
pub fn base(name: &str) -> &str {
    workshop::find(name).map(|m| m.base).unwrap_or(name)
}

/// Every playable map, sorted by name so old versions sit next to the
/// current one (`de_dust2`, `de_dust2_2017`, `de_dust2_2019`).
pub fn list(game_dir: &Path, fs: &vfs::Vfs) -> Vec<MapEntry> {
    list_with(game_dir, fs, &workshop::Store::current())
}

pub fn list_with(game_dir: &Path, fs: &vfs::Vfs, store: &workshop::Store) -> Vec<MapEntry> {
    let mut names: Vec<String> = std::fs::read_dir(game_dir.join("maps"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| e.path().file_name()?.to_str().map(str::to_string))
        .filter_map(|n| n.strip_suffix(".bsp").map(str::to_string))
        .collect();
    names.extend(
        fs.archive_paths_with_prefix("maps/")
            .iter()
            .filter_map(|p| p.strip_prefix("maps/")?.strip_suffix(".bsp").map(str::to_string)),
    );
    names.retain(|n| !n.contains('/') && playable(n) && workshop::find(n).is_none());
    names.sort();
    names.dedup();
    let mut out: Vec<MapEntry> = names
        .into_iter()
        .map(|name| MapEntry {
            title: title(&name),
            base: name.clone(),
            name,
            old: None,
            status: MapStatus::Ready,
        })
        .collect();
    for (old, status) in store.list() {
        out.push(MapEntry {
            name: old.name.to_string(),
            title: old.title.to_string(),
            base: old.base.to_string(),
            old: Some(old),
            status: match status {
                workshop::Status::Installed => MapStatus::Ready,
                workshop::Status::InSteam(_) => MapStatus::InSteam,
                workshop::Status::Download => MapStatus::Download,
            },
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_installed_and_old_maps_together() {
        let root = std::env::temp_dir().join(format!("csr-level-maps-{}", std::process::id()));
        let game = root.join("csgo");
        std::fs::create_dir_all(game.join("maps")).unwrap();
        for n in ["de_dust2.bsp", "cs_office.bsp", "ar_shoots.bsp", "de_dust2.nav"] {
            std::fs::write(game.join("maps").join(n), "").unwrap();
        }
        let store = workshop::Store::with_libraries(&root.join("data"), Vec::new());
        let maps = list_with(&game, &vfs::Vfs::new(), &store);
        let names: Vec<&str> = maps.iter().map(|m| m.name.as_str()).collect();
        let at = |n: &str| names.iter().position(|x| *x == n).unwrap();
        assert!(!names.contains(&"ar_shoots"));
        assert!(at("de_dust2") + 1 == at("de_dust2_2017") && at("de_dust2_2017") + 1 == at("de_dust2_2019"));
        let old = &maps[at("de_dust2_2017")];
        assert_eq!((old.title.as_str(), old.base.as_str(), &old.status), ("Dust II (2017)", "de_dust2", &MapStatus::Download));
        assert_eq!(maps[at("cs_office")].title, "OFFICE");
        assert_eq!(names.len(), 2 + workshop::OLD_MAPS.len());
        std::fs::remove_dir_all(&root).ok();
    }
}

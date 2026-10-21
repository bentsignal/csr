//! Installed maps and experimental archives from Valve's Workshop snapshots
//! (`workshop` crate), listed by `csr maps`. Archives load with
//! [`crate::Level::load`], which fetches them on first use. The match menu
//! offers the admitted classic archives in [`workshop::OldMap::menu_supported`];
//! other archives remain development/fidelity selections.

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
    /// Stable identity for content packs and server map selection.
    pub id: content::ContentId,
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

/// Admission for an archived offline/host match after its own metadata loads.
/// This does not synthesize navigation or borrow the modern map's spawns.
pub fn validate_archive_match(level: &crate::Level, bots: u32) -> Result<(), String> {
    let Some(archive) = workshop::find(&level.name) else { return Ok(()); };
    if level.id != archive.content_id() { return Ok(()); }
    if bots > 0 && level.nav.as_ref().is_none_or(|nav| nav.areas.is_empty()) {
        return Err(format!("{} has no usable archived bot navigation. Return to the map picker and choose another map.", archive.title));
    }
    for class in ["info_player_terrorist", "info_player_counterterrorist"] {
        let count = level.bsp.entities_of_class(class)
            .filter_map(|entity| entity.str("origin").and_then(bsp::parse_origin))
            .filter(|origin| origin.is_finite())
            .count();
        if count < 5 {
            return Err(format!("{} does not have the team spawns needed for a ten-player match. Return to the map picker and choose another map.", archive.title));
        }
    }
    Ok(())
}

/// Every playable map, sorted by name so old versions sit next to the
/// current one (`de_dust2`, `de_dust2_2017`, `de_dust2_2019`).
pub fn list(game_dir: &Path, fs: &vfs::Vfs) -> Vec<MapEntry> {
    list_with(game_dir, fs, &workshop::Store::current())
}

pub fn list_with(game_dir: &Path, fs: &vfs::Vfs, store: &workshop::Store) -> Vec<MapEntry> {
    let mut out: Vec<MapEntry> = installed_names(game_dir, fs).into_iter().map(installed_entry).collect();
    for (old, status) in store.list() {
        out.push(archive_entry(old, status));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

// Loading one map must not probe every unrelated Workshop directory. Keep
// install discovery shared with the picker, but query archive status only for
// the requested identity. Names still come from discovered files, never ID.name.
fn installed_names(game_dir: &Path, fs: &vfs::Vfs) -> Vec<String> {
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
    names
}

fn installed_entry(name: String) -> MapEntry {
    MapEntry {
        id: content::ContentId::csgo(content::ContentKind::Map, &name),
        title: title(&name),
        base: name.clone(),
        name,
        old: None,
        status: MapStatus::Ready,
    }
}

fn archive_entry(old: &'static workshop::OldMap, status: workshop::Status) -> MapEntry {
    MapEntry {
        id: old.content_id(),
        name: old.name.to_string(),
        title: old.title.to_string(),
        base: old.base.to_string(),
        old: Some(old),
        status: match status {
            workshop::Status::Installed => MapStatus::Ready,
            workshop::Status::InSteam(_) => MapStatus::InSteam,
            workshop::Status::Download => MapStatus::Download,
        },
    }
}

/// Resolve identity through discovered install files and the fixed archive
/// definitions. An ID's name is never interpreted as a filesystem path.
pub(crate) fn definition(id: &content::ContentId, game_dir: &Path, fs: &vfs::Vfs, store: &workshop::Store) -> Result<MapEntry, String> {
    if id.origin != content::GameOrigin::Csgo || id.kind != content::ContentKind::Map {
        return Err(format!("{id} is not a CS:GO map definition"));
    }
    let installed = installed_names(game_dir, fs).into_iter()
        .filter(|name| content::ContentId::csgo(content::ContentKind::Map, name) == *id)
        .map(installed_entry);
    let archives = workshop::OLD_MAPS.iter()
        .filter(|old| old.content_id() == *id)
        .map(|old| archive_entry(old, store.status(old)));
    let mut matches = installed.chain(archives).filter(|entry| {
        entry.id == *id && packs::valid_bsp_name(&entry.name)
            && (entry.old.is_some() || install_source(fs, &entry.name).is_some())
    });
    let found = matches.next().ok_or_else(|| format!("map ContentId {id} is not in the local install/archive definitions"))?;
    if matches.next().is_some() { return Err(format!("ambiguous local map ContentId {id}")); }
    Ok(found)
}

/// Only the game-directory mounts may define built-in maps. A private pack or
/// embedded BSP cannot turn its filename into an engine-owned stable identity.
fn install_source(fs: &vfs::Vfs, name: &str) -> Option<String> {
    let path = format!("maps/{name}.bsp");
    fs.labels().into_iter().find(|label| {
        (label.starts_with("vpk:") || label.starts_with("dir:")) && fs.exists_from(label, &path)
    }).map(str::to_owned)
}

pub(crate) fn local_source(entry: &MapEntry, fs: &vfs::Vfs, store: &workshop::Store) -> Result<String, String> {
    if entry.old.is_some() {
        let label = format!("workshop:{}", store.root().display());
        if entry.status == MapStatus::Ready && fs.exists_from(&label, &format!("maps/{}.bsp", entry.name)) {
            return Ok(label);
        }
        Err(format!("archive {} is not installed in its verified local source", entry.id))
    } else {
        install_source(fs, &entry.name).ok_or_else(|| format!("installed map {} is no longer available", entry.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archived_fixture(t_spawns: usize, ct_spawns: usize) -> crate::Level {
        use std::{collections::BTreeMap, sync::Arc};
        let archive = workshop::find("de_train_2015").unwrap();
        let mut entities = String::new();
        for (class, count) in [("info_player_terrorist", t_spawns), ("info_player_counterterrorist", ct_spawns)] {
            for index in 0..count {
                entities.push_str(&format!("{{\"classname\" \"{class}\" \"origin\" \"{} 0 0\"}}\n", index * 64));
            }
        }
        let mut bytes = vec![0; 8 + 64 * 16 + 4];
        bytes[..4].copy_from_slice(b"VBSP");
        bytes[4..8].copy_from_slice(&21i32.to_le_bytes());
        let offset = bytes.len() as i32;
        bytes[8..12].copy_from_slice(&offset.to_le_bytes());
        bytes[12..16].copy_from_slice(&(entities.len() as i32).to_le_bytes());
        bytes.extend_from_slice(entities.as_bytes());
        let mut fs = vfs::Vfs::new();
        fs.push_front("fixture", vfs::Mount::Memory(BTreeMap::from([
            (format!("maps/{}.bsp", archive.name), Arc::<[u8]>::from(bytes)),
        ])));
        crate::Level::load_from_vfs(archive.content_id(), archive.name, "no-install".into(), fs).unwrap()
    }

    #[test]
    fn archived_match_rejects_missing_navigation_and_incomplete_teams() {
        let mut level = archived_fixture(5, 5);
        assert!(validate_archive_match(&level, 0).is_ok());
        assert!(validate_archive_match(&level, 9).unwrap_err().contains("navigation"));
        let area = nav::Area {
            id: 1, flags: 0, nw: glam::Vec3::ZERO, se: glam::Vec3::splat(64.),
            ne_z: 0., sw_z: 0., connections: Default::default(), place: 0, hiding_spots: Vec::new(),
        };
        level.nav = Some(std::sync::Arc::new(nav::NavMesh::from_areas(vec![area])));
        assert!(validate_archive_match(&level, 9).is_ok());
        let incomplete = archived_fixture(5, 4);
        assert!(validate_archive_match(&incomplete, 0).unwrap_err().contains("team spawns"));
    }

    #[test]
    fn lists_installed_and_old_maps_together() {
        let root = std::env::temp_dir().join(format!("csr-level-maps-{}", std::process::id()));
        let game = root.join("csgo");
        std::fs::create_dir_all(game.join("maps")).unwrap();
        for n in ["de_dust2.bsp", "cs_office.bsp", "ar_shoots.bsp", "de_dust2.nav"] {
            std::fs::write(game.join("maps").join(n), "").unwrap();
        }
        let store = workshop::Store::with_libraries(&root.join("data"), Vec::new());
        let mut fs = vfs::Vfs::new();
        fs.push_back(format!("dir:{}", game.display()), vfs::Mount::Dir(game.clone()));
        let maps = list_with(&game, &fs, &store);
        // The targeted load resolver must retain the picker's definitions and
        // statuses while avoiding status probes for unrelated archives.
        for entry in &maps {
            assert_eq!(definition(&entry.id, &game, &fs, &store).unwrap(), *entry);
        }
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

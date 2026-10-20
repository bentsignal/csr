//! Presents installed maps and implemented modes through the original data schema.
use std::collections::HashSet;

/// Pinned pre-rework Source 1 archives, distinct from the installed remakes.
#[cfg(test)]
pub(super) const CLASSIC_DUST2: &str = "de_dust2_2017";
#[cfg(test)]
const CLASSIC_GROUP: &str = "mg_de_dust2_2017";
#[cfg(test)]
const CLASSIC_INFERNO: &str = "de_inferno_2016";

pub(super) fn menu_archive(name: &str) -> bool {
    workshop::find(name).is_some_and(workshop::OldMap::menu_supported)
}

pub(super) fn load(fs: &vfs::Vfs, installed: &HashSet<String>) -> kv::Block {
    workshop::Store::current().prefetch_previews(installed);
    let Some(doc) = fs
        .read_string("gamemodes.txt")
        .and_then(|text| kv::parse(&text).ok())
    else {
        return kv::Block::new();
    };
    let mut root = doc.block("GameModes.txt").unwrap_or(&doc).clone();
    let symbols = fs
        .read("resource/flash/sharedlib.swf")
        .and_then(|bytes| scaleform::library_symbols(&bytes).ok())
        .unwrap_or_default();
    for archive in workshop::OLD_MAPS.iter().filter(|archive| archive.menu_supported()) {
        if installed.contains(archive.name) {
            add_classic_map(&mut root, archive);
        }
    }
    filter(&mut root, installed, &symbols);
    root
}

/// Extend the installed data schema, retaining the original map card geometry and
/// controls. Callback text still resolves exclusively through this catalog.
fn add_classic_map(root: &mut kv::Block, archive: &workshop::OldMap) {
    let base_group = format!("mg_{}", archive.base);
    let classic_group = format!("mg_{}", archive.name);
    let Some(mut map) = root
        .block("maps")
        .and_then(|maps| maps.block(archive.base))
        .cloned()
    else {
        return;
    };
    let Some(mut group) = root
        .block("mapgroups")
        .and_then(|groups| groups.block(&base_group))
        .cloned()
    else {
        return;
    };
    let title = archive.title;
    if workshop::preview::supported(archive.name) {
        if let Some(base_image) = group.str("imagename") {
            group.set("imagename", kv::Value::Str(format!("csr-classic-card|{}|{base_image}", archive.name)));
        }
    }
    map.set("name", kv::Value::Str(archive.name.into()));
    map.set("nameID", kv::Value::Str(title.into()));
    group.set("name", kv::Value::Str(classic_group.clone()));
    group.set("nameID", kv::Value::Str(title.into()));
    group.set(
        "tooltipID",
        kv::Value::Str(format!("{}.", archive.note.trim_end_matches('.'))),
    );
    group.set(
        "maps",
        kv::Value::Block(kv::Block {
            entries: vec![(archive.name.into(), kv::Value::Str(String::new()))],
        }),
    );
    root.get_mut("maps")
        .unwrap()
        .as_block_mut()
        .unwrap()
        .set(archive.name, kv::Value::Block(map));
    root.get_mut("mapgroups")
        .unwrap()
        .as_block_mut()
        .unwrap()
        .set(&classic_group, kv::Value::Block(group));
    if let Some(types) = root.get_mut("gameTypes").and_then(kv::Value::as_block_mut) {
        for (_, kind) in &mut types.entries {
            let Some(modes) = kind
                .as_block_mut()
                .and_then(|kind| kind.get_mut("gameModes"))
                .and_then(kv::Value::as_block_mut)
            else {
                continue;
            };
            for (name, mode) in &mut modes.entries {
                if !matches!(name.as_str(), "competitive" | "deathmatch") {
                    continue;
                }
                let Some(mode) = mode.as_block_mut() else {
                    continue;
                };
                for field in ["mapgroupsSP", "mapgroupsMP"] {
                    let Some(groups) = mode.get_mut(field).and_then(kv::Value::as_block_mut) else {
                        continue;
                    };
                    if groups.get(&classic_group).is_none() {
                        let at = groups
                            .entries
                            .iter()
                            .position(|(name, _)| name == &base_group)
                            .map_or(groups.entries.len(), |at| at + 1);
                        // A historical map can still support a match after its
                        // current version was retired from this mode (Cobble).
                        groups.entries.insert(
                            at,
                            (classic_group.clone(), kv::Value::Str(String::new())),
                        );
                    }
                }
            }
        }
    }
}

fn filter(root: &mut kv::Block, installed: &HashSet<String>, symbols: &HashSet<String>) {
    let mut bomb_groups = HashSet::new();
    let mut all_groups = HashSet::new();
    if let Some(kv::Value::Block(groups)) = root.get_mut("mapgroups") {
        groups.entries.retain_mut(|(name, value)| {
            let Some(group) = value.as_block_mut() else {
                return false;
            };
            if !group
                .str("imagename")
                .is_some_and(|name| symbols.contains(name) || classic_card_symbol(name, symbols))
            {
                return false;
            }
            let Some(kv::Value::Block(maps)) = group.get_mut("maps") else {
                return false;
            };
            maps.entries.retain(|(map, _)| installed.contains(map));
            if maps.entries.is_empty() {
                return false;
            }
            if maps.entries.iter().all(|(map, _)| map.starts_with("de_")) {
                bomb_groups.insert(name.clone());
            }
            all_groups.insert(name.clone());
            true
        });
    }
    if let Some(kv::Value::Block(types)) = root.get_mut("gameTypes") {
        for (_, value) in &mut types.entries {
            let Some(kind) = value.as_block_mut() else {
                continue;
            };
            let Some(kv::Value::Block(modes)) = kind.get_mut("gameModes") else {
                continue;
            };
            for (name, value) in &mut modes.entries {
                let Some(mode) = value.as_block_mut() else {
                    continue;
                };
                let allowed = match name.as_str() {
                    "competitive" => Some(&bomb_groups),
                    "deathmatch" => Some(&all_groups),
                    _ => None,
                };
                for field in ["mapgroupsSP", "mapgroupsMP"] {
                    if let Some(kv::Value::Block(groups)) = mode.get_mut(field) {
                        groups
                            .entries
                            .retain(|(group, _)| allowed.is_some_and(|set| set.contains(group)));
                        // The movie addresses slots by these numbers, not by
                        // their position in the object. Filtering must leave
                        // contiguous indices or the first card stays blank.
                        for (index, (_, value)) in groups.entries.iter_mut().enumerate() {
                            *value = kv::Value::Str(index.to_string());
                        }
                    }
                }
            }
        }
    }
    if let Some(kv::Value::Block(maps)) = root.get_mut("maps") {
        maps.entries.retain(|(name, _)| installed.contains(name));
    }
}

fn classic_card_symbol(name: &str, symbols: &HashSet<String>) -> bool {
    let mut parts = name.split('|');
    matches!((parts.next(), parts.next(), parts.next(), parts.next()),
        (Some("csr-classic-card"), Some(map), Some(base), None)
        if workshop::find(map).is_some_and(|m| m.menu_supported())
            && workshop::preview::supported(map) && base.starts_with("map-")
            && base.ends_with("-overall") && symbols.contains(base))
}

/// The installed movie splits this native result on a comma.
pub(super) fn map_group_subkeys(root: &kv::Block, group: &str, attribute: &str) -> String {
    root.block("mapgroups")
        .and_then(|groups| groups.block(group))
        .and_then(|group| group.block(attribute))
        .map(|block| {
            block
                .iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Selection {
    pub map: String,
    pub deathmatch: bool,
}

/// The native dialog validates, supplies the same query, then commits via OnOk.
pub(super) struct OfflineLaunch {
    difficulty: Option<u8>,
    checked: Option<(String, Selection, u8)>,
    staged: Option<(Selection, u8)>,
}
impl Default for OfflineLaunch {
    fn default() -> Self {
        Self {
            difficulty: Some(3),
            checked: None,
            staged: None,
        }
    }
}
impl OfflineLaunch {
    pub fn difficulty(&mut self, value: Option<f64>) {
        self.checked = None;
        self.staged = None;
        self.difficulty = value
            .filter(|value| {
                value.is_finite() && value.fract() == 0.0 && (0.0..=5.0).contains(value)
            })
            .map(|value| value as u8);
    }
    pub fn check(&mut self, root: &kv::Block, text: Option<&str>) -> Result<(), String> {
        self.checked = None;
        self.staged = None;
        let difficulty = self.difficulty.ok_or("Choose a supported bot difficulty.")?;
        let text = text.ok_or("Game settings could not be read.")?;
        let selection = offline_selection(root, text)?;
        self.checked = Some((text.into(), selection, difficulty));
        Ok(())
    }
    pub fn query(&mut self, text: Option<&str>) {
        self.staged = None;
        if let Some((checked, selection, difficulty)) = self.checked.take() {
            if text == Some(checked.as_str()) {
                self.staged = Some((selection, difficulty));
            }
        }
    }
    pub fn commit(&mut self) -> Option<(Selection, u8)> {
        self.staged.take()
    }
}

/// Validates the serialized settings emitted by the installed offline GO button.
/// Map names come only from the filtered local catalog, never from a UI path.
pub(super) fn offline_selection(root: &kv::Block, text: &str) -> Result<Selection, String> {
    if text.len() > 8192 || text.bytes().filter(|&byte| byte == b'{').count() > 32 {
        return Err("Game settings are too large.".into());
    }
    let settings = kv::parse(text).map_err(|_| "Game settings could not be read.")?;
    fn unique(block: &kv::Block) -> bool {
        let mut keys = HashSet::new();
        block.iter().all(|(key, value)| {
            keys.insert(key.to_ascii_lowercase()) && value.as_block().is_none_or(unique)
        })
    }
    if !unique(&settings)
        || settings.block("System").and_then(|b| b.str("network")) != Some("offline")
        || settings.block("Options").and_then(|b| b.str("action")) != Some("create")
    {
        return Err("These game settings are not available offline.".into());
    }
    let game = settings
        .block("Game")
        .ok_or("Choose a game mode and map.")?;
    let kind = game.str("type").ok_or("Choose a game mode.")?;
    let mode = game.str("mode").ok_or("Choose a game mode.")?;
    let deathmatch = match (kind, mode) {
        ("classic", "competitive") => false,
        ("gungame", "deathmatch") => true,
        _ => return Err("This game mode is not available yet.".into()),
    };
    if game
        .str("skirmishmode")
        .is_some_and(|value| value != "undefined")
    {
        return Err("This game mode is not available yet.".into());
    }
    let group = game.str("mapgroupname").ok_or("Choose a map.")?;
    if root
        .path(&["gameTypes", kind, "gameModes", mode, "mapgroupsSP"])
        .and_then(kv::Value::as_block)
        .and_then(|groups| groups.get(group))
        .is_none()
    {
        return Err("This map is not available for the selected mode.".into());
    }
    let maps = root
        .path(&["mapgroups", group, "maps"])
        .and_then(kv::Value::as_block)
        .ok_or("Choose an installed map.")?;
    if maps.entries.len() != 1 {
        return Err("Choose one map to play.".into());
    }
    let map = &maps.entries[0].0;
    if !map
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || !root
            .block("maps")
            .is_some_and(|maps| maps.get(map).is_some())
        || (!deathmatch && !map.starts_with("de_"))
    {
        return Err("Choose an installed map supported by this mode.".into());
    }
    Ok(Selection {
        map: map.clone(),
        deathmatch,
    })
}

pub(super) fn mode_name(root: &kv::Block, game_type: f64, game_mode: f64) -> &str {
    let Some(types) = root.block("gameTypes") else {
        return "";
    };
    for (_, value) in types.iter() {
        let Some(kind) = value
            .as_block()
            .filter(|kind| kind.f32("value").map(f64::from) == Some(game_type))
        else {
            continue;
        };
        if let Some(modes) = kind.block("gameModes") {
            for (name, value) in modes.iter() {
                if value
                    .as_block()
                    .and_then(|mode| mode.f32("value"))
                    .map(f64::from)
                    == Some(game_mode)
                {
                    return name;
                }
            }
        }
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_photo_symbols_require_an_admitted_version_and_existing_card() {
        let symbols = HashSet::from(["map-nuke-overall".into()]);
        assert!(classic_card_symbol("csr-classic-card|de_nuke_2016|map-nuke-overall", &symbols));
        for symbol in [
            "csr-classic-card|de_nuke|map-nuke-overall",
            "csr-classic-card|de_overpass_2019|map-nuke-overall",
            "csr-classic-card|de_nuke_2016|missing",
            "csr-classic-card|de_nuke_2016|map-nuke-overall|extra",
            "../de_nuke_2016",
        ] { assert!(!classic_card_symbol(symbol, &symbols), "{symbol}"); }
    }
    const REQUEST: &str = "System { network offline } Game { type classic mode competitive mapgroupname mg_de_dust2 skirmishmode undefined } Options { action create anytypemode 0 } Contexts {} Properties {}";

    fn installed_modes() -> kv::Block {
        kv::parse("gameTypes { classic { gameModes { competitive { mapgroupsSP { mg_de_dust2 0 } } } } gungame { gameModes { deathmatch { mapgroupsSP { mg_de_dust2 0 } } } } } mapgroups { mg_de_dust2 { maps { de_dust2 0 } } } maps { de_dust2 {} }").unwrap()
    }

    #[test]
    fn classic_card_uses_versioned_photo_and_resolves_only_the_pinned_archive() {
        let mut root = installed_modes();
        root.get_mut("mapgroups")
            .unwrap()
            .as_block_mut()
            .unwrap()
            .get_mut("mg_de_dust2")
            .unwrap()
            .as_block_mut()
            .unwrap()
            .set("imagename", kv::Value::Str("map-dust2-overall".into()));
        add_classic_map(&mut root, workshop::find(CLASSIC_DUST2).unwrap());
        filter(
            &mut root,
            &HashSet::from(["de_dust2".into(), CLASSIC_DUST2.into()]),
            &HashSet::from(["map-dust2-overall".into()]),
        );
        let card = root
            .block("mapgroups")
            .unwrap()
            .block(CLASSIC_GROUP)
            .unwrap();
        assert_eq!(card.str("imagename"), Some("csr-classic-card|de_dust2_2017|map-dust2-overall"));
        assert_eq!(card.str("nameID"), Some("Dust II (2017)"));
        assert_eq!(
            map_group_subkeys(&root, CLASSIC_GROUP, "maps"),
            CLASSIC_DUST2
        );
        let query = REQUEST.replace("mg_de_dust2", CLASSIC_GROUP);
        assert_eq!(offline_selection(&root, &query).unwrap().map, CLASSIC_DUST2);
        assert!(
            offline_selection(&root, &query.replace(CLASSIC_GROUP, "mg_de_dust2_2019")).is_err()
        );
        assert_eq!(
            root.path(&[
                "gameTypes",
                "classic",
                "gameModes",
                "competitive",
                "mapgroupsSP"
            ])
            .unwrap()
            .as_block()
            .unwrap()
            .str(CLASSIC_GROUP),
            Some("1")
        );
    }

    #[test]
    fn overpass_photo_keeps_installed_and_archive_selection_independent() {
        let archive = workshop::find("de_overpass_2015").unwrap();
        assert!(workshop::preview::supported(archive.name));
        assert!(crate::frontend_loading::supported(archive.name));
        assert!(menu_archive(archive.name));
        let mut root = kv::parse("gameTypes { classic { gameModes { competitive { mapgroupsSP { mg_de_overpass 0 } } } } } mapgroups { mg_de_overpass { nameID Overpass imagename map-overpass-overall maps { de_overpass 0 } } } maps { de_overpass { nameID Overpass } }").unwrap();
        let installed_group = root
            .block("mapgroups").unwrap()
            .block("mg_de_overpass").unwrap().clone();
        add_classic_map(&mut root, archive);
        let card = root.block("mapgroups").unwrap()
            .block("mg_de_overpass_2015").unwrap();
        assert_eq!(card.str("imagename"),
            Some("csr-classic-card|de_overpass_2015|map-overpass-overall"));
        assert_eq!(card.str("nameID"), Some("Overpass (2015)"));
        assert!(card.str("tooltipID").unwrap().contains("October 13, 2015"));
        assert_eq!(map_group_subkeys(&root, "mg_de_overpass_2015", "maps"),
            archive.name);
        assert_eq!(root.block("mapgroups").unwrap().block("mg_de_overpass").unwrap(),
            &installed_group);
        filter(&mut root,
            &HashSet::from(["de_overpass".into(), archive.name.into()]),
            &HashSet::from(["map-overpass-overall".into()]));
        let query = REQUEST.replace("mg_de_dust2", "mg_de_overpass");
        let archive_query = query.replace("mg_de_overpass", "mg_de_overpass_2015");
        for (request, expected) in [(&query, "de_overpass"), (&archive_query, archive.name)] {
            assert_eq!(offline_selection(&root, request).unwrap(), Selection {
                map: expected.into(),
                deathmatch: false,
            });
        }
        assert!(offline_selection(&root,
            &query.replace("mg_de_overpass", "mg_de_overpass_2019")).is_err());
        for (available, accepted, rejected) in [
            ("de_overpass", &query, &archive_query),
            (archive.name, &archive_query, &query),
        ] {
            let mut single = root.clone();
            filter(&mut single, &HashSet::from([available.into()]),
                &HashSet::from(["map-overpass-overall".into()]));
            assert_eq!(offline_selection(&single, accepted).unwrap().map, available);
            assert!(offline_selection(&single, rejected).is_err());
        }
    }

    #[test]
    fn old_inferno_is_distinct_from_the_remake_and_preserves_mode_order() {
        let mut root = installed_modes();
        let original = root
            .block("maps")
            .unwrap()
            .block("de_dust2")
            .unwrap()
            .clone();
        root.get_mut("maps")
            .unwrap()
            .as_block_mut()
            .unwrap()
            .set("de_inferno", kv::Value::Block(original));
        let mut group = root
            .block("mapgroups")
            .unwrap()
            .block("mg_de_dust2")
            .unwrap()
            .clone();
        group.set("imagename", kv::Value::Str("map-inferno-overall".into()));
        group.set(
            "maps",
            kv::Value::Block(kv::Block {
                entries: vec![("de_inferno".into(), kv::Value::Str(String::new()))],
            }),
        );
        root.get_mut("mapgroups")
            .unwrap()
            .as_block_mut()
            .unwrap()
            .set("mg_de_inferno", kv::Value::Block(group));
        for (kind, mode) in [("classic", "competitive"), ("gungame", "deathmatch")] {
            root.get_mut("gameTypes")
                .unwrap()
                .as_block_mut()
                .unwrap()
                .get_mut(kind)
                .unwrap()
                .as_block_mut()
                .unwrap()
                .get_mut("gameModes")
                .unwrap()
                .as_block_mut()
                .unwrap()
                .get_mut(mode)
                .unwrap()
                .as_block_mut()
                .unwrap()
                .get_mut("mapgroupsSP")
                .unwrap()
                .as_block_mut()
                .unwrap()
                .set("mg_de_inferno", kv::Value::Str("1".into()));
        }
        add_classic_map(&mut root, workshop::find(CLASSIC_INFERNO).unwrap());
        filter(
            &mut root,
            &HashSet::from(["de_inferno".into(), CLASSIC_INFERNO.into()]),
            &HashSet::from(["map-inferno-overall".into()]),
        );
        let group = root
            .block("mapgroups")
            .unwrap()
            .block("mg_de_inferno_2016")
            .unwrap();
        assert_eq!(group.str("nameID"), Some("Inferno (2016)"));
        assert_eq!(group.str("imagename"), Some("csr-classic-card|de_inferno_2016|map-inferno-overall"));
        for (requested, expected) in [
            ("mg_de_inferno", "de_inferno"),
            ("mg_de_inferno_2016", CLASSIC_INFERNO),
        ] {
            let query = REQUEST.replace("mg_de_dust2", requested);
            assert_eq!(offline_selection(&root, &query).unwrap().map, expected);
        }
        let groups = root
            .path(&[
                "gameTypes",
                "classic",
                "gameModes",
                "competitive",
                "mapgroupsSP",
            ])
            .unwrap()
            .as_block()
            .unwrap();
        assert_eq!(groups.str("mg_de_inferno"), Some("0"));
        assert_eq!(groups.str("mg_de_inferno_2016"), Some("1"));
        assert!(
            offline_selection(&root, &REQUEST.replace("mg_de_dust2", "mg_de_inferno_2020"))
                .is_err()
        );
        assert!(menu_archive(CLASSIC_INFERNO));
        assert!(!menu_archive("de_inferno_2020"));
    }

    #[test]
    fn retired_base_group_does_not_hide_its_playable_historical_map() {
        // Legacy retains Cobble's card metadata but no longer lists the base
        // in competitive. No installed Cobble BSP is needed for its archive.
        let mut root = installed_modes();
        root.get_mut("maps").unwrap().as_block_mut().unwrap()
            .set("de_cbble", kv::Value::Block(kv::Block::new()));
        let group = kv::parse("imagename map-cobble-overall maps { de_cbble 0 }").unwrap();
        root.get_mut("mapgroups").unwrap().as_block_mut().unwrap()
            .set("mg_de_cbble", kv::Value::Block(group));
        let archive = workshop::find("de_cbble_2018").unwrap();
        add_classic_map(&mut root, archive);
        filter(&mut root, &HashSet::from([archive.name.into()]),
            &HashSet::from(["map-cobble-overall".into()]));
        for (kind, mode) in [("classic", "competitive"), ("gungame", "deathmatch")] {
            let groups = root.path(&["gameTypes", kind, "gameModes", mode, "mapgroupsSP"])
                .unwrap().as_block().unwrap();
            assert_eq!(groups.str("mg_de_cbble_2018"), Some("0"));
        }
        assert!(root.block("maps").unwrap().block("de_cbble").is_none());
        let query = REQUEST.replace("mg_de_dust2", "mg_de_cbble_2018");
        assert_eq!(offline_selection(&root, &query).unwrap().map, archive.name);
        assert!(offline_selection(&root, &query.replace("_2018", "_2015")).is_err());
    }

    #[test]
    fn archive_menu_admission_excludes_unresolved_dependencies_and_later_remakes() {
        for name in ["de_nuke_2016", "de_train_2015", "de_vertigo_2017", "de_mirage_2019", "de_cbble_2018", "de_cache_2015", "de_overpass_2015"] {
            assert!(menu_archive(name), "{name}");
        }
        for name in ["de_cache_2019", "de_overpass_2019", "de_nuke_2019", "de_train_2020", "de_canals_2018", "de_train_2014"] {
            assert!(!menu_archive(name), "{name}");
        }
    }

    #[test]
    fn launch_requires_the_checked_query_and_a_single_commit() {
        let root = installed_modes();
        let mut launch = OfflineLaunch::default();
        assert!(launch.commit().is_none());
        launch.check(&root, Some(REQUEST)).unwrap();
        launch.query(Some("a changed query"));
        assert!(launch.commit().is_none());
        launch.difficulty(Some(0.0));
        launch.check(&root, Some(REQUEST)).unwrap();
        launch.query(Some(REQUEST));
        assert_eq!(launch.commit().unwrap().1, 0);
        assert!(launch.commit().is_none());
        launch.check(&root, Some(REQUEST)).unwrap();
        assert!(launch.check(&root, Some("invalid")).is_err());
        launch.query(Some(REQUEST));
        assert!(launch.commit().is_none());
        launch.difficulty(Some(4.0));
        launch.check(&root, Some(REQUEST)).unwrap();
        launch.query(Some(REQUEST));
        assert_eq!(launch.commit().unwrap().1, 4, "retain the selected UI value, not a no-bots flag");
        for invalid in [Some(f64::NAN), Some(3.5), Some(6.0), None] {
            launch.difficulty(invalid);
            assert!(launch.check(&root, Some(REQUEST)).is_err());
        }
    }

    #[test]
    fn observed_offline_go_resolves_only_the_filtered_installed_map_and_mode() {
        let root = installed_modes();
        assert_eq!(
            offline_selection(&root, REQUEST).unwrap(),
            Selection {
                map: "de_dust2".into(),
                deathmatch: false,
            }
        );
        let deathmatch = REQUEST.replace(
            "type classic mode competitive",
            "type gungame mode deathmatch",
        );
        assert!(offline_selection(&root, &deathmatch).unwrap().deathmatch);
        for invalid in [
            REQUEST.replace("offline", "online"),
            REQUEST.replace("competitive", "casual"),
            REQUEST.replace("mg_de_dust2", "../../maps/private"),
            REQUEST.replace("network offline", "network offline NETWORK online"),
            REQUEST.replace("skirmishmode undefined", "skirmishmode arbitrary"),
            "nest {".repeat(33),
            "x".repeat(8193),
        ] {
            assert!(offline_selection(&root, &invalid).is_err());
        }
        let mut unavailable = root;
        unavailable
            .get_mut("maps")
            .unwrap()
            .as_block_mut()
            .unwrap()
            .entries
            .clear();
        assert!(offline_selection(&unavailable, REQUEST).is_err());
    }

    #[test]
    fn native_map_group_result_splits_into_map_keys_in_authored_order() {
        let root = kv::parse("mapgroups { classic { maps { de_dust2 0 de_inferno 1 } } }").unwrap();
        let native = map_group_subkeys(&root, "classic", "maps");
        assert_eq!(
            native.split(',').collect::<Vec<_>>(),
            ["de_dust2", "de_inferno"]
        );
        assert_eq!(map_group_subkeys(&root, "absent", "maps"), "");
        assert_eq!(map_group_subkeys(&root, "classic", "absent"), "");
    }
    #[test]
    fn retains_mode_labels_but_filters_cards_and_reindexes_slots() {
        let mut root = kv::parse(r#"
            gameTypes { classic { value 0 gameModes {
                casual { value 0 mapgroupsSP { dust 0 } }
                competitive { value 1 mapgroupsSP { missing 0 dust 4 office 8 modern 9 } }
            } } gungame { value 1 gameModes { deathmatch { value 2 mapgroupsSP { dust 0 office 1 } } armsrace { value 0 } } } }
            mapgroups { dust { imagename map-dust maps { de_dust2 0 } } office { imagename map-office maps { cs_office 0 } } missing { imagename map-dust maps { de_missing 0 } } modern { imagename map-modern maps { de_modern 0 } } }
            maps { de_dust2 {} cs_office {} de_missing {} de_modern {} }
        "#).unwrap();
        filter(
            &mut root,
            &HashSet::from(["de_dust2".into(), "cs_office".into(), "de_modern".into()]),
            &HashSet::from(["map-dust".into(), "map-office".into()]),
        );
        assert_eq!(mode_name(&root, 0.0, 0.0), "casual");
        assert_eq!(mode_name(&root, 0.0, 1.0), "competitive");
        assert_eq!(mode_name(&root, 1.0, 2.0), "deathmatch");
        let competitive = root
            .path(&[
                "gameTypes",
                "classic",
                "gameModes",
                "competitive",
                "mapgroupsSP",
            ])
            .unwrap()
            .as_block()
            .unwrap();
        assert_eq!(competitive.entries.len(), 1);
        assert_eq!(competitive.str("dust"), Some("0"));
        let casual = root
            .path(&["gameTypes", "classic", "gameModes", "casual", "mapgroupsSP"])
            .unwrap()
            .as_block()
            .unwrap();
        assert!(casual.entries.is_empty());
        assert!(root.block("mapgroups").unwrap().get("modern").is_none());
        assert!(root.block("maps").unwrap().get("de_missing").is_none());
    }
}

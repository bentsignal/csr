//! Presents installed maps and implemented modes through the original data schema.
use std::collections::HashSet;

/// A pinned Source 1 compatibility archive, not the install's remade Dust II.
pub(super) const CLASSIC_DUST2: &str = "de_dust2_2017";
const CLASSIC_GROUP: &str = "mg_de_dust2_2017";

pub(super) fn load(fs: &vfs::Vfs, installed: &HashSet<String>) -> kv::Block {
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
    if installed.contains(CLASSIC_DUST2) {
        add_classic_dust2(&mut root);
    }
    filter(&mut root, installed, &symbols);
    root
}

/// Extend the installed data schema, retaining the original map card/art and
/// controls. Callback text still resolves exclusively through this catalog.
fn add_classic_dust2(root: &mut kv::Block) {
    let Some(mut map) = root
        .block("maps")
        .and_then(|maps| maps.block("de_dust2"))
        .cloned()
    else {
        return;
    };
    let Some(mut group) = root
        .block("mapgroups")
        .and_then(|groups| groups.block("mg_de_dust2"))
        .cloned()
    else {
        return;
    };
    let title = workshop::find(CLASSIC_DUST2)
        .expect("pinned classic map")
        .title;
    map.set("name", kv::Value::Str(CLASSIC_DUST2.into()));
    map.set("nameID", kv::Value::Str(title.into()));
    group.set("name", kv::Value::Str(CLASSIC_GROUP.into()));
    group.set("nameID", kv::Value::Str(title.into()));
    group.set(
        "tooltipID",
        kv::Value::Str("Classic Dust II before the 2017 rework. Experimental archive.".into()),
    );
    group.set(
        "maps",
        kv::Value::Block(kv::Block {
            entries: vec![(CLASSIC_DUST2.into(), kv::Value::Str(String::new()))],
        }),
    );
    root.get_mut("maps")
        .unwrap()
        .as_block_mut()
        .unwrap()
        .set(CLASSIC_DUST2, kv::Value::Block(map));
    root.get_mut("mapgroups")
        .unwrap()
        .as_block_mut()
        .unwrap()
        .set(CLASSIC_GROUP, kv::Value::Block(group));
    if let Some(types) = root.get_mut("gameTypes").and_then(kv::Value::as_block_mut) {
        for (_, kind) in &mut types.entries {
            let Some(modes) = kind
                .as_block_mut()
                .and_then(|kind| kind.get_mut("gameModes"))
                .and_then(kv::Value::as_block_mut)
            else {
                continue;
            };
            for (_, mode) in &mut modes.entries {
                let Some(mode) = mode.as_block_mut() else {
                    continue;
                };
                for field in ["mapgroupsSP", "mapgroupsMP"] {
                    let Some(groups) = mode.get_mut(field).and_then(kv::Value::as_block_mut) else {
                        continue;
                    };
                    if groups.get(CLASSIC_GROUP).is_none() {
                        if let Some(at) = groups
                            .entries
                            .iter()
                            .position(|(name, _)| name == "mg_de_dust2")
                        {
                            groups.entries.insert(
                                at + 1,
                                (CLASSIC_GROUP.into(), kv::Value::Str(String::new())),
                            );
                        }
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
                .is_some_and(|name| symbols.contains(name))
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
    checked: Option<(String, Selection, bool)>,
    staged: Option<(Selection, bool)>,
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
        let no_bots = match self.difficulty {
            Some(0) => true,
            Some(3) => false,
            _ => {
                return Err(
                    "Choose No Bots or Medium Bots; other difficulties are not available yet."
                        .into(),
                );
            }
        };
        let text = text.ok_or("Game settings could not be read.")?;
        let selection = offline_selection(root, text)?;
        self.checked = Some((text.into(), selection, no_bots));
        Ok(())
    }
    pub fn query(&mut self, text: Option<&str>) {
        self.staged = None;
        if let Some((checked, selection, no_bots)) = self.checked.take() {
            if text == Some(checked.as_str()) {
                self.staged = Some((selection, no_bots));
            }
        }
    }
    pub fn commit(&mut self) -> Option<(Selection, bool)> {
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
    const REQUEST: &str = "System { network offline } Game { type classic mode competitive mapgroupname mg_de_dust2 skirmishmode undefined } Options { action create anytypemode 0 } Contexts {} Properties {}";

    fn installed_modes() -> kv::Block {
        kv::parse("gameTypes { classic { gameModes { competitive { mapgroupsSP { mg_de_dust2 0 } } } } gungame { gameModes { deathmatch { mapgroupsSP { mg_de_dust2 0 } } } } } mapgroups { mg_de_dust2 { maps { de_dust2 0 } } } maps { de_dust2 {} }").unwrap()
    }

    #[test]
    fn classic_card_keeps_installed_art_and_resolves_only_the_pinned_archive() {
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
        add_classic_dust2(&mut root);
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
        assert_eq!(card.str("imagename"), Some("map-dust2-overall"));
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
        assert!(launch.commit().unwrap().1);
        assert!(launch.commit().is_none());
        launch.check(&root, Some(REQUEST)).unwrap();
        assert!(launch.check(&root, Some("invalid")).is_err());
        launch.query(Some(REQUEST));
        assert!(launch.commit().is_none());
        for invalid in [Some(f64::NAN), Some(3.5), Some(4.0), None] {
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

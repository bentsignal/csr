//! Presents installed maps and implemented modes through the original data schema.
use std::collections::HashSet;

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
    filter(&mut root, installed, &symbols);
    root
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

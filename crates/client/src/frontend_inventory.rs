//! Local inventory enumeration for the movie thread. IDs remain strings:
//! Flash numbers cannot exactly represent the base-item high-bit identifiers.
use scaleform::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(super) struct Item {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub definition: u32,
    pub rarity: u8,
    pub category: &'static str,
    pub base: bool,
    pub equipped: bool,
    pub stickers: u32,
}

#[derive(Clone)]
pub(super) struct InventoryView {
    items: Vec<Item>,
    visible: Vec<usize>,
    colors: BTreeMap<u8, String>,
    capabilities: BTreeMap<u32, Vec<String>>,
    filter: String,
    sort: String,
    hide_default: bool,
    search: String,
}

// Case stacks do not yet retain acquisition timestamps; offer only real sorts.
const SORTS: &[&str] = &["inv_sort_alpha", "inv_sort_rarity"];

impl InventoryView {
    pub fn new(fs: &vfs::Vfs) -> Self {
        let (colors, capabilities) = fs
            .read_string("scripts/items/items_game.txt")
            .and_then(|text| kv::parse(&text).ok())
            .map(|doc| {
                let root = doc.block("items_game").unwrap_or(&doc);
                (rarity_colors(root), item_capabilities(root))
            })
            .unwrap_or_default();
        Self {
            items: vec![],
            visible: vec![],
            colors,
            capabilities,
            filter: "all".into(),
            sort: SORTS[0].into(),
            hide_default: false,
            search: String::new(),
        }
    }

    pub fn replace(&mut self, items: Vec<Item>) {
        self.items = items;
        self.rebuild();
    }

    pub fn image_path(&self, id: &str) -> Option<String> {
        let icon = &self.items.iter().find(|item| item.id == id)?.icon;
        if icon.is_empty()
            || icon.contains("..")
            || !icon
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'/'))
        {
            return None;
        }
        Some(format!("resource/flash/{icon}.png"))
    }

    fn rebuild(&mut self) {
        let search = self.search.to_lowercase();
        self.visible = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                (!self.hide_default || !item.base)
                    && item.name.to_lowercase().contains(&search)
                    && match self.filter.as_str() {
                        "all" | "" => true,
                        "default" => item.base,
                        "equipped" => item.equipped,
                        "any_equipment" | "only_weapons" => item.category != "not_equipment",
                        category => item.category == category,
                    }
            })
            .map(|(index, _)| index)
            .collect();
        match self.sort.as_str() {
            "inv_sort_alpha" => self
                .visible
                .sort_by_cached_key(|&i| self.items[i].name.to_lowercase()),
            "inv_sort_rarity" => self
                .visible
                .sort_by_key(|&i| std::cmp::Reverse(self.items[i].rarity)),
            _ => {}
        }
    }

    pub fn call(&mut self, method: &str, args: &[Value]) -> Option<Value> {
        let arg = |index| args.get(index).map(Value::str).unwrap_or_default();
        match method {
            "GetSavedFilterMethod" => return Some(self.filter.clone().into()),
            "GetSavedSortMethod" => return Some(self.sort.clone().into()),
            "GetSortMethodsCount" => return Some((SORTS.len() as u32).into()),
            "GetSortMethodByIndex" => {
                return Some(
                    index(args.first(), SORTS.len())
                        .map_or("", |i| SORTS[i])
                        .into(),
                );
            }
            "SetInventorySortAndFilters" => {
                let sort = arg(0);
                if SORTS.contains(&sort.as_str()) {
                    self.sort = sort;
                }
                self.hide_default = matches!(args.get(1), Some(Value::Bool(true)));
                let filter = arg(2);
                if !filter.is_empty() {
                    self.filter = filter;
                }
                self.search = arg(4);
                self.rebuild();
                return Some(Value::Undefined);
            }
            "GetInventoryCount" => return Some((self.visible.len() as u32).into()),
            "GetInventoryItemIDByIndex" => {
                return Some(
                    index(args.first(), self.visible.len())
                        .map_or("0", |i| self.items[self.visible[i]].id.as_str())
                        .into(),
                );
            }
            _ => {}
        }
        let id = arg(0);
        let item = self.items.iter().find(|item| item.id == id);
        let capabilities = item.and_then(|i| self.capabilities.get(&i.definition));
        match method {
            "GetItemCapabilitiesCount" => Some((capabilities.map_or(0, Vec::len) as u32).into()),
            "GetItemCapabilityByIndex" => Some(
                capabilities
                    .and_then(|caps| {
                        let value @ Value::Number(_) = args.get(1)? else {
                            return None;
                        };
                        index(Some(value), caps.len()).map(|i| caps[i].clone().into())
                    })
                    .unwrap_or(Value::Undefined),
            ),
            "IsItemInfoValid" | "IsInventoryImageCachable" => Some(item.is_some().into()),
            "GetItemName" => Some(item.map_or("", |i| i.name.as_str()).into()),
            "GetItemDefinitionIndex" => Some(item.map_or(0, |i| i.definition).into()),
            "GetItemInventoryImage" => Some(item.map_or("", |i| i.icon.as_str()).into()),
            "GetItemRarity" => Some((item.map_or(0, |i| i.rarity) as u32).into()),
            "GetItemRarityColor" => Some(
                item.and_then(|i| self.colors.get(&i.rarity))
                    .map_or("", String::as_str)
                    .into(),
            ),
            "IsEquipped" => Some(item.is_some_and(|i| i.equipped).into()),
            "GetItemStickerCount" => Some(item.map_or(0, |i| i.stickers).into()),
            "DoesItemMatchDefinitionByName" => Some(false.into()), // No sprays in this collection.
            _ => None,
        }
    }
}

/// Only root capability flags are enumerated. Tool usage capabilities, item
/// classes and decoder restrictions have separate, unverified native semantics.
fn item_capabilities(root: &kv::Block) -> BTreeMap<u32, Vec<String>> {
    let items = crate::cosmetic_catalog::table(root, "items");
    let prefabs = crate::cosmetic_catalog::table(root, "prefabs");
    items
        .iter()
        .filter_map(|(key, value)| {
            let definition = key.parse().ok()?;
            let mut flags = kv::Block::new();
            capability_flags(value.as_block()?, &prefabs, &mut flags, 0, &mut 128)?;
            Some((
                definition,
                flags
                    .iter()
                    .filter(|(_, flag)| flag.as_str() == Some("1"))
                    .map(|(name, _)| name.to_string())
                    .collect(),
            ))
        })
        .collect()
}

// Fail closed for missing/cyclic/overlarge inheritance rather than publishing
// a partial list. Later parents and local flags override earlier flags without
// losing unrelated inherited entries. The list preserves schema declaration order.
fn capability_flags(
    item: &kv::Block,
    prefabs: &kv::Block,
    flags: &mut kv::Block,
    depth: usize,
    budget: &mut usize,
) -> Option<()> {
    if depth >= 16 || *budget == 0 {
        return None;
    }
    *budget -= 1;
    for name in item.str("prefab").unwrap_or("").split_whitespace() {
        capability_flags(prefabs.block(name)?, prefabs, flags, depth + 1, budget)?;
    }
    for (name, value) in item
        .get_all("capabilities")
        .filter_map(kv::Value::as_block)
        .flat_map(kv::Block::iter)
    {
        if name.len() > 128 || flags.len() >= 128 && flags.get(name).is_none() {
            return None;
        }
        flags.set(name, value.clone());
    }
    Some(())
}

fn index(value: Option<&Value>, len: usize) -> Option<usize> {
    let value = value?.num();
    (value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value < len as f64)
        .then_some(value as usize)
}

fn rarity_colors(root: &kv::Block) -> BTreeMap<u8, String> {
    let mut out = BTreeMap::new();
    let (Some(rarities), Some(colors)) = (root.block("rarities"), root.block("colors")) else {
        return out;
    };
    for (_, value) in rarities.iter() {
        let Some(rarity) = value.as_block() else {
            continue;
        };
        let Some(number) = rarity.i64("value").and_then(|n| u8::try_from(n).ok()) else {
            continue;
        };
        let Some(color) = rarity
            .str("color")
            .and_then(|name| colors.block(name))
            .and_then(|block| block.str("hex_color"))
        else {
            continue;
        };
        out.insert(number, color.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_merge_prefabs_and_repeated_tables_with_local_disabled_overrides() {
        let root = kv::parse(
            r#"
            prefabs { base { capabilities { decodable 1 can_delete 1 old 1 } }
                second { capabilities { old 0 second 1 } }
                derived { prefab "base second" capabilities { can_delete 0 } } }
            items { 1 { prefab derived capabilities { local 1 malformed yes } } }
            items { 2 { capabilities { own 1 } tool { usage_capabilities { decodable 1 } } } }
        "#,
        )
        .unwrap();
        let caps = item_capabilities(&root);
        assert_eq!(caps[&1], ["decodable", "second", "local"]);
        assert_eq!(caps[&2], ["own"]);
    }

    #[test]
    fn invalid_inheritance_never_exposes_a_partial_capability_list() {
        let root = kv::parse(
            r#"
            prefabs { valid { capabilities { inherited 1 } }
                cycle { prefab cycle capabilities { cyclic 1 } } }
            items { 1 { prefab "valid missing" capabilities { local 1 } }
                2 { prefab cycle } 3 { prefab valid } }
        "#,
        )
        .unwrap();
        let caps = item_capabilities(&root);
        assert!(!caps.contains_key(&1));
        assert!(!caps.contains_key(&2));
        assert_eq!(caps[&3], ["inherited"]);
        let oversized = kv::parse(&format!(
            "items {{ 4 {{ capabilities {{ {} }} }} }}",
            (0..129).map(|n| format!("cap{n} 1 ")).collect::<String>()
        ))
        .unwrap();
        assert!(item_capabilities(&oversized).is_empty());
    }

    #[test]
    fn native_capabilities_follow_owned_definition_even_when_item_is_filtered_out() {
        let mut view = InventoryView::new(&vfs::Vfs::default());
        view.capabilities = item_capabilities(
            &kv::parse("items { 1 { capabilities { decodable 1 can_delete 1 } } }").unwrap(),
        );
        view.replace(vec![item("case:1:0", "Case", "not_equipment", false)]);
        view.call(
            "SetInventorySortAndFilters",
            &[
                Value::Undefined,
                false.into(),
                "rifle".into(),
                "save".into(),
                "".into(),
            ],
        );
        assert_eq!(view.call("GetInventoryCount", &[]), Some(0u32.into()));
        assert_eq!(
            view.call("GetItemCapabilitiesCount", &["case:1:0".into()]),
            Some(2u32.into())
        );
        assert_eq!(
            view.call("GetItemCapabilityByIndex", &["case:1:0".into(), 0.0.into()]),
            Some("decodable".into())
        );
        assert_eq!(
            view.call("GetItemCapabilityByIndex", &["case:1:0".into(), 1.0.into()]),
            Some("can_delete".into())
        );
        for invalid in [-1.0, f64::NAN, f64::INFINITY, 0.5, 2.0] {
            assert_eq!(
                view.call(
                    "GetItemCapabilityByIndex",
                    &["case:1:0".into(), invalid.into()]
                ),
                Some(Value::Undefined)
            );
        }
        for invalid in [
            Value::Undefined,
            Value::Null,
            false.into(),
            "0".into(),
            "invalid".into(),
        ] {
            assert_eq!(
                view.call("GetItemCapabilityByIndex", &["case:1:0".into(), invalid]),
                Some(Value::Undefined)
            );
        }
        for unknown in ["1", "4061", "../private", "case:1:1"] {
            assert_eq!(
                view.call("GetItemCapabilitiesCount", &[unknown.into()]),
                Some(0u32.into())
            );
            assert_eq!(
                view.call("GetItemCapabilityByIndex", &[unknown.into(), 0.0.into()]),
                Some(Value::Undefined)
            );
        }
        assert_eq!(view.call("IsTool", &["case:1:0".into()]), None);
        assert_eq!(
            view.call("GetDecodeableRestriction", &["case:1:0".into()]),
            None
        );
    }

    #[test]
    #[ignore = "requires the read-only CS:GO Legacy item schema"]
    fn installed_chroma_metadata_keeps_root_capabilities_separate_from_tool_fields() {
        let dir = std::env::var_os("CSR_GAME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(
                    r"C:\Program Files (x86)\Steam\steamapps\common\csgo legacy\csgo",
                )
            });
        let mut fs = vfs::Vfs::default();
        fs.push_back("installed-items", vfs::Mount::Dir(dir));
        let text = fs
            .read_string("scripts/items/items_game.txt")
            .expect("installed item schema");
        let doc = kv::parse(&text).unwrap();
        let root = doc.block("items_game").unwrap();
        let caps = item_capabilities(root);
        assert_eq!(caps[&4061], ["decodable", "can_delete"]);
        assert!(caps[&1323].is_empty()); // Tool usage flags are not root capabilities.
        let items = crate::cosmetic_catalog::table(root, "items");
        let prefabs = crate::cosmetic_catalog::table(root, "prefabs");
        assert_eq!(
            items.block("4061").unwrap().str("name"),
            Some("crate_community_6")
        );
        assert_eq!(
            items
                .block("4061")
                .unwrap()
                .path(&["tool", "restriction"])
                .and_then(kv::Value::as_str),
            Some("community_case_6")
        );
        assert_eq!(
            items
                .block("1323")
                .unwrap()
                .path(&["tool", "restriction"])
                .and_then(kv::Value::as_str),
            Some("community_case_6")
        );
        assert_eq!(
            prefabs.block("weapon_case_base").unwrap().str("item_class"),
            Some("supply_crate")
        );
        assert_eq!(
            prefabs
                .block("weapon_case_key")
                .unwrap()
                .path(&["tool", "type"])
                .and_then(kv::Value::as_str),
            Some("decoder_ring")
        );
        let mut view = InventoryView::new(&fs);
        let mut case = item("case:4061:0", "Chroma Case", "not_equipment", false);
        case.definition = 4061;
        view.replace(vec![case]);
        assert_eq!(
            view.call("GetItemCapabilitiesCount", &["case:4061:0".into()]),
            Some(2u32.into())
        );
        assert_eq!(
            view.call(
                "GetItemCapabilityByIndex",
                &["case:4061:0".into(), 0.0.into()]
            ),
            Some("decodable".into())
        );
    }

    fn item(id: &str, name: &str, category: &'static str, base: bool) -> Item {
        Item {
            id: id.into(),
            name: name.into(),
            category,
            base,
            icon: String::new(),
            definition: 1,
            rarity: if base { 0 } else { 4 },
            equipped: false,
            stickers: 0,
        }
    }
    #[test]
    fn native_images_resolve_owned_opaque_ids_without_accepting_paths() {
        let mut view = InventoryView::new(&vfs::Vfs::default());
        let mut weapon = item("9223372036854775809", "Rifle", "rifle", true);
        weapon.icon = "econ/weapons/base_weapons/weapon_ak47".into();
        view.replace(vec![weapon]);
        assert_eq!(
            view.image_path("9223372036854775809").as_deref(),
            Some("resource/flash/econ/weapons/base_weapons/weapon_ak47.png")
        );
        assert!(view.image_path("../../private").is_none());
        assert!(view.image_path("1").is_none());
        view.items[0].icon = "../private".into();
        assert!(view.image_path("9223372036854775809").is_none());
    }

    #[test]
    fn filter_changes_enumeration_without_losing_hidden_item_metadata() {
        let mut view = InventoryView::new(&vfs::Vfs::default());
        view.replace(vec![
            item("9223372036854775809", "Zulu", "secondary", true),
            item("17", "Alpha", "rifle", false),
        ]);
        assert_eq!(
            view.call("GetInventoryItemIDByIndex", &[0.0.into()]),
            Some("17".into())
        );
        view.call(
            "SetInventorySortAndFilters",
            &[
                "inv_sort_rarity".into(),
                true.into(),
                "all".into(),
                "save".into(),
                "".into(),
            ],
        );
        assert_eq!(view.call("GetInventoryCount", &[]), Some(1u32.into()));
        assert_eq!(
            view.call("GetItemName", &["9223372036854775809".into()]),
            Some("Zulu".into())
        );
        view.call(
            "SetInventorySortAndFilters",
            &[
                "inv_sort_alpha".into(),
                false.into(),
                "secondary".into(),
                "save".into(),
                "zu".into(),
            ],
        );
        assert_eq!(
            view.call("GetInventoryItemIDByIndex", &[0.0.into()]),
            Some("9223372036854775809".into())
        );
        for invalid in [-1.0, f64::NAN, 0.5, 1.0] {
            assert_eq!(
                view.call("GetInventoryItemIDByIndex", &[invalid.into()]),
                Some("0".into())
            );
        }
    }
    #[test]
    fn palette_joins_installed_rarity_metadata() {
        let doc = kv::parse(r##"rarities { common { value 1 color desc_common } } colors { desc_common { hex_color "#b0c3d9" } }"##).unwrap();
        assert_eq!(
            rarity_colors(&doc).get(&1).map(String::as_str),
            Some("#b0c3d9")
        );
    }
}

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

pub(super) struct InventoryView {
    items: Vec<Item>,
    visible: Vec<usize>,
    colors: BTreeMap<u8, String>,
    filter: String,
    sort: String,
    hide_default: bool,
    search: String,
}

// Case stacks do not yet retain acquisition timestamps; offer only real sorts.
const SORTS: &[&str] = &["inv_sort_alpha", "inv_sort_rarity"];

impl InventoryView {
    pub fn new(fs: &vfs::Vfs) -> Self {
        let colors = fs
            .read_string("scripts/items/items_game.txt")
            .and_then(|text| kv::parse(&text).ok())
            .map(|doc| rarity_colors(doc.block("items_game").unwrap_or(&doc)))
            .unwrap_or_default();
        Self {
            items: vec![],
            visible: vec![],
            colors,
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
        match method {
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

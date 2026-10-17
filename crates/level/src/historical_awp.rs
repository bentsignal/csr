//! Explicit old-target balance policy, separate from the installed catalog.
use content::{ContentId, ContentKind};
use sha1::{Digest, Sha1};
use weapons::{Catalog, WeaponClass};

// Observed installed Legacy 1.38.8.1 schema. Custom/historical schemas keep
// their own authored capacities, even when an item is named weapon_awp.
const LEGACY_SCHEMA: [u8; 20] = [
    0xc1, 0x36, 0x1d, 0x35, 0x18, 0x02, 0xb7, 0xbe, 0x5f, 0x55, 0x24, 0x5c, 0x33, 0x1f, 0x5f, 0x1d,
    0x6b, 0x36, 0xad, 0x35,
];

/// The project's 2018 target predates the later five-round AWP magazine.
/// Public item-data history (facts only, no copied assets/implementation):
/// https://github.com/SteamDatabase/GameTracking-CSGO/blob/093b49738d/csgo/scripts/items/items_game.txt#L17886
pub(super) fn import_2018_target(
    catalog: &mut Catalog,
    item_schema: Option<[u8; 20]>,
    previous_policy: Option<[u8; 20]>,
) -> Option<[u8; 20]> {
    let previous = previous_policy?;
    if item_schema != Some(LEGACY_SCHEMA) { return Some(previous); }
    let Some(awp) = catalog.weapons.iter_mut().find(|w|
        w.id == ContentId::csgo(ContentKind::Weapon, "awp")
            && w.class_name == "weapon_awp" && w.def_index == 9
            && w.class == WeaponClass::Sniper && w.bullets == 1
            && w.clip_size == 5 && w.reserve_ammo == 30 && w.cycle_time == 1.455
            && w.attr_f32("primary clip size") == Some(5.)
            && w.view_model == "models/weapons/v_snip_awp.mdl") else { return Some(previous); };
    awp.clip_size = 10;
    awp.attributes.insert("primary clip size".into(), "10".into());
    let mut hash = Sha1::new();
    hash.update(previous);
    hash.update(b"csr-2018-awp-magazine-10-v1\0");
    Some(hash.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        // Authored regression metadata, not a copied item-schema fixture.
        Catalog::from_items_game(r#"items_game {
            items { 9 { name weapon_awp baseitem 1 prefab awp } }
            prefabs { awp {
                model_player models/weapons/v_snip_awp.mdl
                visuals { weapon_type SniperRifle }
                attributes { "primary clip size" 5 "primary reserve ammo max" 30
                    cycletime 1.455 "unzoom after shot" 1 }
            } }
        }"#).unwrap()
    }

    #[test]
    fn old_target_restores_ten_rounds_and_changes_policy_without_changing_raw_identity() {
        let mut catalog = catalog();
        let schema = Some(LEGACY_SCHEMA);
        let previous = Some([17; 20]);
        let next = import_2018_target(&mut catalog, schema, previous);
        let awp = catalog.get("weapon_awp").unwrap();
        assert_eq!(awp.clip_size, 10);
        assert_eq!(awp.attr_f32("primary clip size"), Some(10.));
        assert_eq!(awp.reserve_ammo, 30);
        let mut state = sim::weapon::WeaponState::new(std::sync::Arc::new(awp.clone()));
        assert_eq!((state.clip, state.reserve), (10, 30));
        state.clip = 0;
        state.begin_reload(0.);
        let end = state.reload_end.unwrap();
        sim::weapon::think(&mut state, end, 1. / 128., false, false);
        assert_eq!((state.clip, state.reserve, state.reload_end), (10, 20, None));
        assert_ne!(next, previous);
        assert_eq!(import_2018_target(&mut catalog, schema, next), next);
    }

    #[test]
    fn unrelated_and_explicit_pack_capacities_are_never_overridden() {
        let previous = Some([17; 20]);
        for schema in [None, Some([0; 20])] {
            let mut catalog = catalog();
            assert_eq!(import_2018_target(&mut catalog, schema, previous), previous);
            assert_eq!(catalog.weapons[0].clip_size, 5);
        }
        for mismatch in 0..7 {
            let mut catalog = catalog();
            let awp = &mut catalog.weapons[0];
            match mismatch {
                0 => awp.id = "mod_weapon_awp".parse().unwrap(),
                1 => awp.class_name = "weapon_other".into(),
                2 => awp.def_index = 999,
                3 => awp.clip_size = 12,
                4 => awp.reserve_ammo = 60,
                5 => awp.view_model = "models/private/awp.mdl".into(),
                _ => { awp.attributes.insert("primary clip size".into(), "12".into()); }
            }
            let before = awp.clone();
            assert_eq!(import_2018_target(&mut catalog, Some(LEGACY_SCHEMA), previous), previous);
            assert_eq!(catalog.weapons[0], before);
        }
    }
}

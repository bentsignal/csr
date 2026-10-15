//! Clean-room observation policy, separate from installed schema defaults.
use content::{ContentId, ContentKind};
use sha1::{Digest, Sha1};
use weapons::{Catalog, EmptyTrigger, WeaponClass};

const OBSERVED_SCHEMA: [u8; 20] = [
    0xc1, 0x36, 0x1d, 0x35, 0x18, 0x02, 0xb7, 0xbe, 0x5f, 0x55, 0x24, 0x5c, 0x33, 0x1f, 0x5f, 0x1d,
    0x6b, 0x36, 0xad, 0x35,
];

pub(super) fn import(
    catalog: &mut Catalog,
    item_schema: Option<[u8; 20]>,
    previous_policy: Option<[u8; 20]>,
) -> Option<[u8; 20]> {
    let previous = previous_policy?;
    if item_schema != Some(OBSERVED_SCHEMA) { return Some(previous); }
    let Some(ak) = catalog.weapons.iter_mut().find(|def|
        def.id == ContentId::csgo(ContentKind::Weapon, "ak47")
            && def.class_name == "weapon_ak47" && def.def_index == 7
            && def.class == WeaponClass::Rifle && def.full_auto
            && def.clip_size == 30 && def.reserve_ammo == 90
            && def.bullets == 1 && def.cycle_time == 0.1
            && def.empty_trigger.is_none()) else { return Some(previous); };
    let profile = EmptyTrigger {
        sound: "Default.ClipEmpty_Rifle".into(),
        // Candidate inferred from eight native dynamic sound starts with
        // mean gap .2027s. Not a recovered exact server deadline or 2015 law.
        repeat_interval: 0.2,
        reference: "legacy-1.38.8.1-ak-empty-trigger-aggregate".into(),
    };
    let mut digest = Sha1::new();
    digest.update(previous);
    digest.update(b"observed-empty-trigger-v1");
    digest.update(ak.id.to_string().as_bytes());
    digest.update(profile.sound.as_bytes());
    digest.update(profile.repeat_interval.to_le_bytes());
    digest.update(profile.reference.as_bytes());
    ak.empty_trigger = Some(profile);
    Some(digest.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_installed_ak_opts_in_without_overwriting_other_policy_or_content() {
        let game = std::path::PathBuf::from(std::env::var_os("CSR_GAME_DIR").unwrap_or_else(||
            r"C:\Program Files (x86)\Steam\steamapps\common\csgo legacy\csgo".into()));
        let Ok(bytes) = std::fs::read(game.join("scripts/items/items_game.txt")) else { return; };
        let schema: [u8; 20] = Sha1::digest(&bytes).into();
        if schema != OBSERVED_SCHEMA { return; }
        let mut catalog = Catalog::from_items_game(&kv::decode_text(&bytes)).unwrap();
        let previous = [17; 20]; // Already accumulated recoil/animation policies.
        let next = import(&mut catalog, Some(schema), Some(previous)).unwrap();
        assert_ne!(next, previous);
        let ak = catalog.get("weapon_ak47").unwrap();
        assert_eq!(ak.empty_trigger.as_ref().unwrap().sound, "Default.ClipEmpty_Rifle");
        assert!(catalog.weapons.iter().filter(|d| d.class_name != "weapon_ak47")
            .all(|d| d.empty_trigger.is_none()));
        let mut other = Catalog { weapons: vec![ak.clone()] };
        other.weapons[0].empty_trigger = None;
        assert_eq!(import(&mut other, Some([0; 20]), Some(previous)), Some(previous));
        assert!(other.weapons[0].empty_trigger.is_none());
        other.weapons[0].clip_size = 31;
        assert_eq!(import(&mut other, Some(schema), Some(previous)), Some(previous));
        assert!(other.weapons[0].empty_trigger.is_none());
        other.weapons[0].clip_size = 30;
        let mut explicit = ak.empty_trigger.clone().unwrap();
        explicit.sound = "Pack.CustomEmpty".into();
        other.weapons[0].empty_trigger = Some(explicit.clone());
        assert_eq!(import(&mut other, Some(schema), Some(previous)), Some(previous));
        assert_eq!(other.weapons[0].empty_trigger, Some(explicit));
    }
}

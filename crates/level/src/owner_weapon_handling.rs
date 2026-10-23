//! Explicit owner-requested balance; not a claim of historical/native parity.
use content::{ContentId, ContentKind};
use sha1::{Digest, Sha1};
use weapons::{Catalog, WeaponClass};

const LEGACY_SCHEMA: [u8; 20] = [
    0xc1, 0x36, 0x1d, 0x35, 0x18, 0x02, 0xb7, 0xbe, 0x5f, 0x55, 0x24, 0x5c, 0x33, 0x1f, 0x5f, 0x1d,
    0x6b, 0x36, 0xad, 0x35,
];

pub(super) fn import(catalog: &mut Catalog, schema: Option<[u8; 20]>, previous: Option<[u8; 20]>) -> Option<[u8; 20]> {
    let previous = import_revolver(catalog, schema, previous)?;
    if schema != Some(LEGACY_SCHEMA) { return Some(previous); }
    let mut changed = false;
    for gun in &mut catalog.weapons {
        let policy = if gun.id == ContentId::csgo(ContentKind::Weapon, "ssg08")
            && gun.class_name == "weapon_ssg08" && gun.def_index == 40
            && gun.class == WeaponClass::Sniper && gun.cycle_time == 1.25 {
            "csr vertical jump inaccuracy"
        } else if gun.id == ContentId::csgo(ContentKind::Weapon, "negev")
            && gun.class_name == "weapon_negev" && gun.def_index == 28
            && gun.class == WeaponClass::MachineGun && gun.clip_size == 150
            && gun.cycle_time == 0.075 && gun.recoil.seed == 57966 {
            "csr bounded sustained recoil"
        } else { continue; };
        if gun.attr_f32(policy) != Some(1.) {
            gun.attributes.insert(policy.into(), "1".into());
            changed = true;
        }
    }
    if !changed { return Some(previous); }
    let mut hash = Sha1::new();
    hash.update(previous);
    hash.update(b"csr-owner-scout-vertical-negev-bounded-v1\0");
    Some(hash.finalize().into())
}

fn import_revolver(catalog: &mut Catalog, schema: Option<[u8; 20]>, previous: Option<[u8; 20]>) -> Option<[u8; 20]> {
    let previous = previous?;
    if schema != Some(LEGACY_SCHEMA) { return Some(previous); }
    // The owner requests Deagle-like torso damage. Use the mounted donor's
    // declaration rather than retuning the shared hit-group/armor formula.
    let Some(damage) = catalog.get("weapon_deagle").filter(|gun|
        gun.id == ContentId::csgo(ContentKind::Weapon, "deagle")
            && gun.def_index == 1 && gun.class == WeaponClass::Pistol
            && gun.damage.is_finite() && gun.damage > 0.)
        .map(|gun| gun.damage) else { return Some(previous); };
    let Some(gun) = catalog.weapons.iter_mut().find(|w|
        w.id == ContentId::csgo(ContentKind::Weapon, "revolver")
            && w.class_name == "weapon_revolver" && w.def_index == 64
            && w.class == WeaponClass::Pistol && w.full_auto && w.bullets == 1
            && w.cycle_time == 0.5 && w.attr_f32("cycletime") == Some(0.5)) else { return Some(previous); };
    // October 23: another modest primary slowdown, two torso hits at close
    // range and lethal heads. Keep instant fire and the authored .4s fan fire.
    gun.cycle_time = 0.65;
    gun.attributes.insert("cycletime".into(), "0.65".into());
    gun.damage = damage;
    gun.attributes.insert("damage".into(), damage.to_string());
    let mut hash = Sha1::new();
    hash.update(previous);
    hash.update(b"csr-owner-r8-primary-cycle-065-deagle-damage-v2\0");
    hash.update(damage.to_le_bytes());
    Some(hash.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scout_negev_policies_admit_only_selected_legacy_content_and_bind_catalog() {
        let fixture = || Catalog::from_items_game(r#"items_game { items {
            40 { name weapon_ssg08 baseitem 1 attributes { cycletime 1.25 } }
            28 { name weapon_negev baseitem 1 attributes {
                cycletime 0.075 "primary clip size" 150 "recoil seed" 57966
            } }
        } }"#).unwrap();
        let previous = Some([19; 20]);
        let mut custom = fixture();
        assert_eq!(import(&mut custom, Some([0; 20]), previous), previous);
        assert_eq!(custom.get("weapon_ssg08").unwrap().attr_f32("csr vertical jump inaccuracy"), None);
        assert_eq!(custom.get("weapon_negev").unwrap().attr_f32("csr bounded sustained recoil"), None);
        let mut catalog = fixture();
        let next = import(&mut catalog, Some(LEGACY_SCHEMA), previous);
        assert_ne!(next, previous);
        assert_eq!(catalog.get("weapon_ssg08").unwrap().attr_f32("csr vertical jump inaccuracy"), Some(1.));
        assert_eq!(catalog.get("weapon_negev").unwrap().attr_f32("csr bounded sustained recoil"), Some(1.));
        assert_eq!(import(&mut catalog, Some(LEGACY_SCHEMA), next), next);
    }

    #[test]
    fn owner_r8_policy_is_scoped_idempotent_and_preserves_instant_automatic_trigger() {
        let fixture = || Catalog::from_items_game(r#"items_game {
            items { 64 { name weapon_revolver baseitem 1 attributes {
                cycletime 0.5 "is full auto" 1 "primary clip size" 8
                damage 115 "armor ratio" 1.864 "range modifier" 0.98
                "cycletime alt" 0.4
            } }
            1 { name weapon_deagle baseitem 1 attributes { damage 63 } }
            }
        }"#).unwrap();
        let previous = Some([17; 20]);
        let mut custom = fixture();
        assert_eq!(import(&mut custom, Some([0; 20]), previous), previous);
        assert_eq!(custom.get("weapon_revolver").unwrap().cycle_time, 0.5);
        let mut catalog = fixture();
        let next = import(&mut catalog, Some(LEGACY_SCHEMA), previous);
        let gun = catalog.get("weapon_revolver").unwrap();
        assert_eq!(gun.cycle_time, 0.65);
        assert_eq!(gun.damage, 63.);
        assert_eq!(gun.attr_f32("cycletime alt"), Some(0.4));
        for armor in [0, 100] {
            for group in [sim::weapon::HitGroup::Chest, sim::weapon::HitGroup::Stomach] {
                let (health, _) = sim::weapon::damage(gun, 100., group, armor, true);
                assert!((50..100).contains(&health), "two close torso hits: {group:?}, armor={armor}, hp={health}");
            }
            assert!(sim::weapon::damage(gun, 100., sim::weapon::HitGroup::Head, armor, true).0 >= 100);
        }
        assert!(gun.full_auto);
        assert_eq!(sim::weapon::WeaponState::new(std::sync::Arc::new(gun.clone())).next_attack, 0.);
        assert_ne!(next, previous);
        assert_eq!(import(&mut catalog, Some(LEGACY_SCHEMA), next), next);
    }
}

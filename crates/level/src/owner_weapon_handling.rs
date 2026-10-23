//! Explicit owner-requested balance; not a claim of historical/native parity.
use content::{ContentId, ContentKind};
use sha1::{Digest, Sha1};
use weapons::{Catalog, WeaponClass};

const LEGACY_SCHEMA: [u8; 20] = [
    0xc1, 0x36, 0x1d, 0x35, 0x18, 0x02, 0xb7, 0xbe, 0x5f, 0x55, 0x24, 0x5c, 0x33, 0x1f, 0x5f, 0x1d,
    0x6b, 0x36, 0xad, 0x35,
];

pub(super) fn import(catalog: &mut Catalog, schema: Option<[u8; 20]>, previous: Option<[u8; 20]>) -> Option<[u8; 20]> {
    let previous = previous?;
    if schema != Some(LEGACY_SCHEMA) { return Some(previous); }
    let Some(gun) = catalog.weapons.iter_mut().find(|w|
        w.id == ContentId::csgo(ContentKind::Weapon, "revolver")
            && w.class_name == "weapon_revolver" && w.def_index == 64
            && w.class == WeaponClass::Pistol && w.full_auto && w.bullets == 1
            && w.cycle_time == 0.5 && w.attr_f32("cycletime") == Some(0.5)) else { return Some(previous); };
    // October 22: preserve accepted instant R8 firing, slightly slow repeats.
    // A 10% longer primary interval implements that preference without a
    // hammer/charge mechanic or edits to custom/historical content schemas.
    gun.cycle_time = 0.55;
    gun.attributes.insert("cycletime".into(), "0.55".into());
    let mut hash = Sha1::new();
    hash.update(previous);
    hash.update(b"csr-owner-r8-primary-cycle-055-v1\0");
    Some(hash.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_r8_policy_is_scoped_idempotent_and_preserves_instant_automatic_trigger() {
        let fixture = || Catalog::from_items_game(r#"items_game {
            items { 64 { name weapon_revolver baseitem 1 attributes {
                cycletime 0.5 "is full auto" 1 "primary clip size" 8
            } } }
        }"#).unwrap();
        let previous = Some([17; 20]);
        let mut custom = fixture();
        assert_eq!(import(&mut custom, Some([0; 20]), previous), previous);
        assert_eq!(custom.get("weapon_revolver").unwrap().cycle_time, 0.5);
        let mut catalog = fixture();
        let next = import(&mut catalog, Some(LEGACY_SCHEMA), previous);
        let gun = catalog.get("weapon_revolver").unwrap();
        assert_eq!(gun.cycle_time, 0.55);
        assert!(gun.full_auto);
        assert_eq!(sim::weapon::WeaponState::new(std::sync::Arc::new(gun.clone())).next_attack, 0.);
        assert_ne!(next, previous);
        assert_eq!(import(&mut catalog, Some(LEGACY_SCHEMA), next), next);
    }
}

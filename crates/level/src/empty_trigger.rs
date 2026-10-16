//! Clean-room observation policy, separate from installed schema defaults.
use content::{ContentId, ContentKind};
use sha1::{Digest, Sha1};
use weapons::{Catalog, EmptyTrigger, WeaponClass};

const OBSERVED_SCHEMA: [u8; 20] = [
    0xc1, 0x36, 0x1d, 0x35, 0x18, 0x02, 0xb7, 0xbe, 0x5f, 0x55, 0x24, 0x5c, 0x33, 0x1f, 0x5f, 0x1d,
    0x6b, 0x36, 0xad, 0x35,
];

struct ObservedEmptyTrigger {
    content: &'static str,
    class_name: &'static str,
    def_index: u32,
    clip: i32,
    reserve: i32,
    cycle: f32,
    interval: f32,
    reference: &'static str,
}

const OBSERVED_EMPTY_TRIGGERS: &[ObservedEmptyTrigger] = &[
    ObservedEmptyTrigger {
        content: "ak47", class_name: "weapon_ak47", def_index: 7,
        clip: 30, reserve: 90, cycle: 0.1,
        // Inferred from eight native sound starts, mean gap .2027s.
        interval: 0.2, reference: "legacy-1.38.8.1-ak-empty-trigger-aggregate",
    },
    ObservedEmptyTrigger {
        content: "m4a1", class_name: "weapon_m4a1", def_index: 16,
        clip: 30, reserve: 90, cycle: 0.09,
        // Independent 0/0 tap/held observation; nine held cues, mean .20275s.
        interval: 0.2, reference: "legacy-1.38.8.1-m4a4-empty-trigger-aggregate",
    },
    ObservedEmptyTrigger {
        content: "m4a1_silencer", class_name: "weapon_m4a1_silencer", def_index: 60,
        clip: 20, reserve: 80, cycle: 0.1,
        // Separately depleted attached/detached rifle, mean held gap ~.203s.
        interval: 0.2, reference: "legacy-1.38.8.1-m4s-empty-trigger-aggregate",
    },
];

pub(super) fn import(
    catalog: &mut Catalog,
    item_schema: Option<[u8; 20]>,
    previous_policy: Option<[u8; 20]>,
) -> Option<[u8; 20]> {
    let previous = previous_policy?;
    if item_schema != Some(OBSERVED_SCHEMA) { return Some(previous); }
    let mut policy = previous;
    for observed in OBSERVED_EMPTY_TRIGGERS {
        let Some(def) = catalog.weapons.iter_mut().find(|def|
            def.id == ContentId::csgo(ContentKind::Weapon, observed.content)
                && def.class_name == observed.class_name && def.def_index == observed.def_index
                && def.class == WeaponClass::Rifle && def.full_auto
                && def.clip_size == observed.clip && def.reserve_ammo == observed.reserve
                && def.bullets == 1 && def.cycle_time == observed.cycle
                && def.empty_trigger.is_none()) else { continue; };
        let profile = EmptyTrigger {
            sound: "Default.ClipEmpty_Rifle".into(),
            repeat_interval: observed.interval,
            reference: observed.reference.into(),
        };
        let mut digest = Sha1::new();
        digest.update(policy);
        digest.update(b"observed-empty-trigger-v1");
        digest.update(def.id.to_string().as_bytes());
        digest.update(profile.sound.as_bytes());
        digest.update(profile.repeat_interval.to_le_bytes());
        digest.update(profile.reference.as_bytes());
        def.empty_trigger = Some(profile);
        policy = digest.finalize().into();
    }
    Some(policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_depleted_rifles_opt_in_without_overwriting_other_policy_or_content() {
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
        for observed in OBSERVED_EMPTY_TRIGGERS {
            let def = catalog.get(observed.class_name).unwrap();
            let profile = def.empty_trigger.as_ref().unwrap();
            assert_eq!(profile.sound, "Default.ClipEmpty_Rifle");
            assert_eq!(profile.repeat_interval, observed.interval);
            assert_eq!(profile.reference, observed.reference);
        }
        assert!(catalog.weapons.iter().filter(|d|
            !OBSERVED_EMPTY_TRIGGERS.iter().any(|p| p.class_name == d.class_name))
            .all(|d| d.empty_trigger.is_none()));
        for observed in OBSERVED_EMPTY_TRIGGERS {
            let original = catalog.get(observed.class_name).unwrap().clone();
            let mut other = Catalog { weapons: vec![original.clone()] };
            other.weapons[0].empty_trigger = None;
            assert_eq!(import(&mut other, Some([0; 20]), Some(previous)), Some(previous));
            assert!(other.weapons[0].empty_trigger.is_none());
            other.weapons[0].clip_size += 1;
            assert_eq!(import(&mut other, Some(schema), Some(previous)), Some(previous));
            assert!(other.weapons[0].empty_trigger.is_none());
            other.weapons[0] = original;
            let mut explicit = other.weapons[0].empty_trigger.clone().unwrap();
            explicit.sound = "Pack.CustomEmpty".into();
            other.weapons[0].empty_trigger = Some(explicit.clone());
            assert_eq!(import(&mut other, Some(schema), Some(previous)), Some(previous));
            assert_eq!(other.weapons[0].empty_trigger, Some(explicit));
        }
    }
}

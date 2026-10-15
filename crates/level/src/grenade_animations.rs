//! Import timing from installed model declarations; no animation/engine code port.
use sha1::{Digest, Sha1};
use weapons::{GrenadeAnimations, GrenadeReleaseAnimation, WeaponClass};

fn release(model: &mdl::Model, activity: &str) -> Option<GrenadeReleaseAnimation> {
    let sequence = model
        .sequences
        .iter()
        .find(|s| s.activity.eq_ignore_ascii_case(activity))?;
    let animation = model
        .animations
        .get(usize::try_from(*sequence.anims.first()?).ok()?)?;
    let duration = animation.duration();
    let events: Vec<_> = sequence
        .events
        .iter()
        .filter(|e| e.id == 3005 || e.name == "EVENT_WEAPON_THROW")
        .collect();
    if events.len() != 1 || !duration.is_finite() || duration <= 0. || duration > 30. {
        return None;
    }
    let cycle = events[0].cycle;
    if !cycle.is_finite() || !(0. ..=1.).contains(&cycle) {
        return None;
    }
    Some(GrenadeReleaseAnimation {
        duration,
        release_time: cycle * duration,
        command_release_time: None,
    })
}

/// These installed MDL headers were observed in an owned Legacy 1.38.8.1
/// session. Header identity prevents assigning native timing to a content
/// pack's different animation merely because it reuses a CS item name.
const OBSERVED_HEADERS: &[(&str, &str)] = &[
    ("weapon_hegrenade", "7b0a6c02bc3f3993831d0d5ac32fab2855be027d"),
    ("weapon_flashbang", "0226e812fe86e2203dd30d7168e552a0ccb60f39"),
    ("weapon_smokegrenade", "43e9d797e8c9e219b1917a3a608ba1a091c0c4cb"),
    ("weapon_molotov", "644f500876241e0607746bf8be968e945f8392f0"),
    ("weapon_incgrenade", "1941079ae2bcd012d2aeae9af190b0edf799a2ea"),
    ("weapon_decoy", "b59b9ec5629f1f713acfb726b8c8a2034b0b3b67"),
];

fn apply_observed_release(weapon: &weapons::WeaponDefinition, bytes: &[u8], profile: Option<&mut GrenadeAnimations>) {
    let Some((_, expected)) = OBSERVED_HEADERS.iter().find(|(name, _)| *name == weapon.class_name) else { return; };
    let actual: String = Sha1::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect();
    if actual != *expected { return; }
    if let Some(profile) = profile {
        // All six primary/secondary throws and HE combined visibly launch by
        // ~0.12..0.17s, rather than the authored marker's ~0.333s. Tight HE
        // overhand/tap captures narrow first visibility to ~0.105..0.120s.
        // A 0.1s delay quantizes to 0.109375s at 64Hz. This is an
        // observed profile, not proof of an exact server spawn tick or 2015.
        profile.overhand.command_release_time = Some(0.1);
        profile.underhand.command_release_time = Some(0.1);
    }
}

pub(super) fn import(
    catalog: &mut weapons::Catalog,
    fs: &vfs::Vfs,
    previous_hash: Option<[u8; 20]>,
) -> Option<[u8; 20]> {
    let mut digest = Sha1::new();
    digest.update(previous_hash?);
    digest.update(b"grenade-animation-timing-v4-observed-release");
    let mut definitions: Vec<_> = catalog
        .weapons
        .iter_mut()
        .filter(|w| w.class == WeaponClass::Grenade)
        .collect();
    if definitions.is_empty() {
        return previous_hash;
    }
    definitions.sort_by(|a, b| a.class_name.cmp(&b.class_name));
    for weapon in definitions {
        digest.update(weapon.class_name.as_bytes());
        digest.update([0]);
        let Some(bytes) = fs.read(&weapon.view_model) else {
            digest.update(b"unavailable");
            continue;
        };
        digest.update(Sha1::digest(&bytes));
        let Some(model) = mdl::Model::load_header(&bytes).ok() else {
            continue;
        };
        let profile = (|| {
            let pull = model
                .sequences
                .iter()
                .find(|s| s.activity.eq_ignore_ascii_case("ACT_VM_PULLPIN"))?;
            let animation = model
                .animations
                .get(usize::try_from(*pull.anims.first()?).ok()?)?;
            let duration = animation.duration();
            if !duration.is_finite() || duration <= 0. || duration > 30. {
                return None;
            }
            Some(GrenadeAnimations {
                pull_pin_duration: duration,
                overhand: release(&model, "ACT_VM_THROW")?,
                underhand: release(&model, "ACT_VM_RELEASE")?,
            })
        })();
        let mut profile = profile;
        apply_observed_release(weapon, &bytes, profile.as_mut());
        weapon.grenade_animations = profile;
    }
    Some(digest.finalize().into())
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU"]
    fn installed_six_grenades_import_complete_event_timing_profiles() {
        let level = crate::Level::load("de_dust2", None).expect("installed Legacy");
        for name in [
            "weapon_hegrenade",
            "weapon_flashbang",
            "weapon_smokegrenade",
            "weapon_molotov",
            "weapon_incgrenade",
            "weapon_decoy",
        ] {
            let definition = level.catalog.get(name).unwrap();
            let profile = definition.grenade_animations.as_ref().expect(name);
            assert!(profile.pull_pin_duration > 0.);
            for release in [&profile.overhand, &profile.underhand] {
                assert!(
                    release.duration > 0.
                        && (0. ..=release.duration).contains(&release.release_time)
                );
            }
            println!("{name}: {profile:?}");
        }
        assert!(level.catalog_sha1().is_some());
    }
}

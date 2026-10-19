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

// Exact items_game identity accompanying the independently observed HE/Molotov/Smoke pairs.
// A pack that replaces either schema or model must not borrow this response.
const OBSERVED_LAUNCH_SCHEMA: [u8; 20] = [
    0xc1, 0x36, 0x1d, 0x35, 0x18, 0x02, 0xb7, 0xbe, 0x5f, 0x55,
    0x24, 0x5c, 0x33, 0x1f, 0x5f, 0x1d, 0x6b, 0x36, 0xad, 0x35,
];

fn observed_launch(name: &str, schema: Option<[u8; 20]>, header: &str) -> Option<weapons::GrenadeLaunch> {
    if name == "weapon_molotov" && schema == Some(OBSERVED_LAUNCH_SCHEMA)
        && header == OBSERVED_HEADERS[3].1 {
        return Some(weapons::GrenadeLaunch {
            reference: "legacy-1.38.8.1-molotov-ground-primary-horizontal-128hz".into(),
            grounded_overhand_horizontal_inheritance: 1.25,
            grounded_underhand_horizontal_inheritance: None,
            grounded_combined_horizontal_inheritance: None,
            grounded_overhand_velocity: None,
            grounded_underhand_velocity: None,
            grounded_combined_velocity: None,
            airborne_overhand_velocity: None,
            airborne_overhand_horizontal_inheritance: None,
            airborne_overhand_vertical_inheritance: None,
        });
    }
    if name == "weapon_smokegrenade" && schema == Some(OBSERVED_LAUNCH_SCHEMA)
        && header == OBSERVED_HEADERS[2].1 {
        return Some(weapons::GrenadeLaunch {
            reference: "legacy-1.38.8.1-smoke-ground-button-response-128hz".into(),
            grounded_overhand_horizontal_inheritance: 1.25,
            grounded_underhand_horizontal_inheritance: Some(1.25),
            grounded_combined_horizontal_inheritance: Some(1.25),
            // Independent Smoke level/-30 observations fit this family; the
            // positive45 holdout confirms it. Primary-only input and the held
            // strength field establish the mode; spawn time is not fitted.
            grounded_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 675.,
                upward_pitch_bias_degrees: 10.,
            }),
            // Separately fitted stationary Smoke secondary/combined 0/-30
            // observations, checked at +45 without refitting. These do not
            // measure initial Vz or physical spawn time. Separate steady-forward
            // pairs establish each button mode's horizontal inheritance above.
            grounded_underhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 202.5,
                upward_pitch_bias_degrees: 10.,
            }),
            grounded_combined_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 438.75,
                upward_pitch_bias_degrees: 10.,
            }),
            // SOURCE CANDIDATE: UNADMITTED. Root's level jump pilot establishes
            // a horizontal mismatch, not the angular family or inheritance.
            // Populate each component only after root's independent admission;
            // this exact Smoke/schema/model gate must remain the sole importer.
            airborne_overhand_velocity: None,
            airborne_overhand_horizontal_inheritance: None,
            airborne_overhand_vertical_inheritance: None,
        });
    }
    (name == "weapon_hegrenade" && schema == Some(OBSERVED_LAUNCH_SCHEMA)
        && header == OBSERVED_HEADERS[0].1).then(|| weapons::GrenadeLaunch {
            reference: "legacy-1.38.8.1-he-ground-button-inheritance-128hz".into(),
            grounded_overhand_horizontal_inheritance: 1.25,
            grounded_underhand_horizontal_inheritance: Some(1.25),
            grounded_combined_horizontal_inheritance: Some(1.25),
            grounded_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 675.,
                upward_pitch_bias_degrees: 10.,
            }),
            grounded_underhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 202.5,
                upward_pitch_bias_degrees: 10.,
            }),
            grounded_combined_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 438.75,
                upward_pitch_bias_degrees: 10.,
            }),
            airborne_overhand_velocity: None,
            airborne_overhand_horizontal_inheritance: None,
            airborne_overhand_vertical_inheritance: None,
        })
}

pub(super) fn import(
    catalog: &mut weapons::Catalog,
    fs: &vfs::Vfs,
    previous_hash: Option<[u8; 20]>,
    item_schema: Option<[u8; 20]>,
) -> Option<[u8; 20]> {
    let mut digest = Sha1::new();
    digest.update(previous_hash?);
    // Bind the grounded Smoke button velocity corrections as well as the
    // existing mean-velocity gravity law when installed data is unchanged.
    // Retain v14 while installed airborne fields remain empty. Root must bind
    // the final admitted air profile once with v15 before candidate integration.
    digest.update(b"grenade-animation-timing-v14-smoke-ground-button-response");
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
        let header: String = Sha1::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect();
        weapon.grenade_launch = observed_launch(&weapon.class_name, item_schema, &header);
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
    fn installed_air_components_remain_unadmitted_for_every_grenade() {
        use super::*;
        // Root has admitted only the jump pilot's level horizontal mismatch.
        // Do not turn that into a complete angular/inheritance profile, even
        // when the exact installed Smoke identity matches the grounded gate.
        for &(name, header) in OBSERVED_HEADERS {
            if let Some(profile) = observed_launch(name, Some(OBSERVED_LAUNCH_SCHEMA), header) {
                assert!(profile.airborne_overhand_velocity.is_none());
                assert!(profile.airborne_overhand_horizontal_inheritance.is_none());
                assert!(profile.airborne_overhand_vertical_inheritance.is_none());
            }
        }
    }

    #[test]
    fn launch_response_requires_exact_schema_model_and_item() {
        use super::*;
        let header = OBSERVED_HEADERS[0].1;
        assert!(observed_launch("weapon_hegrenade", Some(OBSERVED_LAUNCH_SCHEMA), header).is_some());
        assert!(observed_launch("weapon_smokegrenade", Some(OBSERVED_LAUNCH_SCHEMA), header).is_none());
        assert!(observed_launch("weapon_hegrenade", None, header).is_none());
        let mut other_schema = OBSERVED_LAUNCH_SCHEMA;
        other_schema[0] ^= 1;
        assert!(observed_launch("weapon_hegrenade", Some(other_schema), header).is_none());
        assert!(observed_launch("weapon_hegrenade", Some(OBSERVED_LAUNCH_SCHEMA), "other").is_none());
        for (name, own_header) in [
            ("weapon_molotov", OBSERVED_HEADERS[3].1),
            ("weapon_smokegrenade", OBSERVED_HEADERS[2].1),
        ] {
            let launch = observed_launch(name, Some(OBSERVED_LAUNCH_SCHEMA), own_header).unwrap();
            assert_eq!(launch.grounded_overhand_horizontal_inheritance, 1.25);
            if name == "weapon_smokegrenade" {
                assert_eq!(launch.grounded_underhand_horizontal_inheritance, Some(1.25));
                assert_eq!(launch.grounded_combined_horizontal_inheritance, Some(1.25));
                assert_eq!(launch.reference, "legacy-1.38.8.1-smoke-ground-button-response-128hz");
                for (velocity, speed) in [
                    (&launch.grounded_overhand_velocity, 675.),
                    (&launch.grounded_underhand_velocity, 202.5),
                    (&launch.grounded_combined_velocity, 438.75),
                ] {
                    let velocity = velocity.as_ref().unwrap();
                    assert_eq!(velocity.speed, speed);
                    assert_eq!(velocity.upward_pitch_bias_degrees, 10.);
                }
            } else {
                assert!(launch.grounded_underhand_horizontal_inheritance.is_none()
                    && launch.grounded_combined_horizontal_inheritance.is_none());
                assert!(launch.grounded_overhand_velocity.is_none());
                assert!(launch.grounded_underhand_velocity.is_none() && launch.grounded_combined_velocity.is_none());
            }
            for (name, schema, header) in [
                (name, Some(OBSERVED_LAUNCH_SCHEMA), header),
                (name, Some(OBSERVED_LAUNCH_SCHEMA), "other"),
                (name, Some(other_schema), own_header),
                (name, None, own_header),
                ("weapon_incgrenade", Some(OBSERVED_LAUNCH_SCHEMA), own_header),
            ] {
                assert!(observed_launch(name, schema, header).is_none());
            }
        }
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU"]
    fn installed_six_grenades_import_complete_event_timing_profiles() {
        let level = crate::Level::load("de_dust2", None).expect("installed Legacy");
        assert!(level.catalog.get("weapon_hegrenade").unwrap().grenade_launch.is_some());
        assert!(level.catalog.get("weapon_hegrenade").unwrap().grenade_launch.as_ref()
            .unwrap().grounded_overhand_velocity.is_some());
        let launch = level.catalog.get("weapon_hegrenade").unwrap().grenade_launch.as_ref().unwrap();
        assert!(launch.grounded_underhand_velocity.is_some());
        assert!(launch.grounded_combined_velocity.is_some());
        assert_eq!(launch.grounded_underhand_horizontal_inheritance, Some(1.25));
        assert_eq!(launch.grounded_combined_horizontal_inheritance, Some(1.25));
        for name in ["weapon_molotov", "weapon_smokegrenade"] {
            let launch = level.catalog.get(name).unwrap().grenade_launch.as_ref().unwrap();
            assert_eq!(launch.grounded_overhand_horizontal_inheritance, 1.25);
            assert_eq!(launch.grounded_overhand_velocity.is_some(), name == "weapon_smokegrenade");
            assert_eq!(launch.grounded_underhand_velocity.is_some(), name == "weapon_smokegrenade");
            assert_eq!(launch.grounded_combined_velocity.is_some(), name == "weapon_smokegrenade");
            if name == "weapon_smokegrenade" {
                for (velocity, speed) in [
                    (&launch.grounded_overhand_velocity, 675.),
                    (&launch.grounded_underhand_velocity, 202.5),
                    (&launch.grounded_combined_velocity, 438.75),
                ] {
                    let velocity = velocity.as_ref().unwrap();
                    assert_eq!(velocity.speed, speed);
                    assert_eq!(velocity.upward_pitch_bias_degrees, 10.);
                }
            }
            let expected = (name == "weapon_smokegrenade").then_some(1.25);
            assert_eq!(launch.grounded_underhand_horizontal_inheritance, expected);
            assert_eq!(launch.grounded_combined_horizontal_inheritance, expected);
        }
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

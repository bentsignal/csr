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

// Exact items_game identity accompanying the independently observed launch pairs.
// A pack that replaces either schema or model must not borrow this response.
const OBSERVED_LAUNCH_SCHEMA: [u8; 20] = [
    0xc1, 0x36, 0x1d, 0x35, 0x18, 0x02, 0xb7, 0xbe, 0x5f, 0x55,
    0x24, 0x5c, 0x33, 0x1f, 0x5f, 0x1d, 0x6b, 0x36, 0xad, 0x35,
];

// Components stay independent: stationary angular evidence cannot supply
// movement inheritance, another button mode, or any airborne response.
fn grounded_primary_launch(reference: &str, horizontal_inheritance: f32,
    velocity: Option<weapons::GrenadeLaunchVelocity>) -> weapons::GrenadeLaunch {
    weapons::GrenadeLaunch {
        reference: reference.into(),
        grounded_overhand_horizontal_inheritance: horizontal_inheritance,
        grounded_underhand_horizontal_inheritance: None,
        grounded_combined_horizontal_inheritance: None,
        grounded_overhand_velocity: velocity,
        grounded_underhand_velocity: None,
        grounded_combined_velocity: None,
        airborne_overhand_velocity: None,
        airborne_overhand_horizontal_inheritance: None,
        airborne_overhand_vertical_inheritance: None,
        airborne_underhand_velocity: None,
        airborne_underhand_horizontal_inheritance: None,
        airborne_underhand_vertical_inheritance: None,
        airborne_combined_velocity: None,
        airborne_combined_horizontal_inheritance: None,
        airborne_combined_vertical_inheritance: None,
    }
}

fn observed_launch(name: &str, schema: Option<[u8; 20]>, header: &str) -> Option<weapons::GrenadeLaunch> {
    if schema == Some(OBSERVED_LAUNCH_SCHEMA)
        && OBSERVED_HEADERS.iter().any(|&(item, expected)| name == item && header == expected) {
        let primary = match name {
            // Existing independent movement observation; no angular family yet.
            "weapon_molotov" => Some((
                "legacy-1.38.8.1-molotov-ground-primary-horizontal-128hz", 1.25, None)),
            // Each kind's own grounded stationary 0/-30 fit and +45 holdout.
            // Horizontal movement inheritance is still unmeasured: retain 0.5.
            "weapon_flashbang" => Some((
                "legacy-1.38.8.1-flash-ground-primary-angular-128hz", 0.5,
                Some(weapons::GrenadeLaunchVelocity { speed: 675., upward_pitch_bias_degrees: 10. }))),
            "weapon_decoy" => Some((
                "legacy-1.38.8.1-decoy-ground-primary-angular-128hz", 0.5,
                Some(weapons::GrenadeLaunchVelocity { speed: 675., upward_pitch_bias_degrees: 10. }))),
            _ => None,
        };
        if let Some((reference, inheritance, velocity)) = primary {
            return Some(grounded_primary_launch(reference, inheritance, velocity));
        }
    }
    if name == "weapon_smokegrenade" && schema == Some(OBSERVED_LAUNCH_SCHEMA)
        && header == OBSERVED_HEADERS[2].1 {
        return Some(weapons::GrenadeLaunch {
            reference: "legacy-1.38.8.1-smoke-ground-and-air-button-response-128hz".into(),
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
            // Independent airborne 0/-30/+45 samples establish the angular
            // family; steady-forward and two constant-Vz zero-gravity controls
            // separately establish horizontal and vertical inheritance.
            airborne_overhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 675., upward_pitch_bias_degrees: 10.,
            }),
            airborne_overhand_horizontal_inheritance: Some(1.25),
            airborne_overhand_vertical_inheritance: Some(1.25),
            // No airborne secondary/combined component is admitted. Neither
            // grounded endpoints nor primary-air evidence can supply these.
            // Each alternate mode has its own angular samples, steady-forward
            // pair, and two constant-Vz controls with a grounded zero-G check.
            airborne_underhand_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 202.5, upward_pitch_bias_degrees: 10.,
            }),
            airborne_underhand_horizontal_inheritance: Some(1.25),
            airborne_underhand_vertical_inheritance: Some(1.25),
            airborne_combined_velocity: Some(weapons::GrenadeLaunchVelocity {
                speed: 438.75, upward_pitch_bias_degrees: 10.,
            }),
            airborne_combined_horizontal_inheritance: Some(1.25),
            airborne_combined_vertical_inheritance: Some(1.25),
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
            airborne_underhand_velocity: None,
            airborne_underhand_horizontal_inheritance: None,
            airborne_underhand_vertical_inheritance: None,
            airborne_combined_velocity: None,
            airborne_combined_horizontal_inheritance: None,
            airborne_combined_vertical_inheritance: None,
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
    // Retain v17's supported Smoke activation and bind the newly admitted
    // grounded primary families; wire layout and other launch scopes stay fixed.
    digest.update(b"grenade-animation-timing-v18-ground-primary-families-smoke-supported-activation");
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
    fn flash_and_decoy_admit_only_exact_grounded_primary_angular_profiles() {
        use super::*;
        for (name, header, reference) in [
            ("weapon_flashbang", OBSERVED_HEADERS[1].1, "legacy-1.38.8.1-flash-ground-primary-angular-128hz"),
            ("weapon_decoy", OBSERVED_HEADERS[5].1, "legacy-1.38.8.1-decoy-ground-primary-angular-128hz"),
        ] {
            let profile = observed_launch(name, Some(OBSERVED_LAUNCH_SCHEMA), header).unwrap();
            assert_eq!(profile.reference, reference);
            assert_eq!(profile.grounded_overhand_horizontal_inheritance, 0.5);
            let velocity = profile.grounded_overhand_velocity.as_ref().unwrap();
            assert_eq!(velocity.speed, 675.);
            assert_eq!(velocity.upward_pitch_bias_degrees, 10.);
            assert!(profile.grounded_underhand_horizontal_inheritance.is_none());
            assert!(profile.grounded_combined_horizontal_inheritance.is_none());
            assert!(profile.grounded_underhand_velocity.is_none());
            assert!(profile.grounded_combined_velocity.is_none());
            for (velocity, horizontal, vertical) in [
                (&profile.airborne_overhand_velocity, profile.airborne_overhand_horizontal_inheritance, profile.airborne_overhand_vertical_inheritance),
                (&profile.airborne_underhand_velocity, profile.airborne_underhand_horizontal_inheritance, profile.airborne_underhand_vertical_inheritance),
                (&profile.airborne_combined_velocity, profile.airborne_combined_horizontal_inheritance, profile.airborne_combined_vertical_inheritance),
            ] {
                assert!(velocity.is_none() && horizontal.is_none() && vertical.is_none());
            }
            assert!(observed_launch(name, None, header).is_none());
            let mut changed_schema = OBSERVED_LAUNCH_SCHEMA;
            changed_schema[19] ^= 1;
            assert!(observed_launch(name, Some(changed_schema), header).is_none());
            assert!(observed_launch(name, Some(OBSERVED_LAUNCH_SCHEMA), "other").is_none());
            for &(other_name, other_header) in OBSERVED_HEADERS {
                if other_name != name {
                    assert!(observed_launch(name, Some(OBSERVED_LAUNCH_SCHEMA), other_header).is_none());
                    assert!(observed_launch(other_name, Some(OBSERVED_LAUNCH_SCHEMA), header).is_none());
                }
            }
            assert!(observed_launch("pack_flash", Some(OBSERVED_LAUNCH_SCHEMA), header).is_none());
        }
        assert!(observed_launch("weapon_incgrenade", Some(OBSERVED_LAUNCH_SCHEMA), OBSERVED_HEADERS[4].1).is_none());
    }

    #[test]
    fn only_exact_smoke_identity_admits_air_button_responses() {
        use super::*;
        for &(name, header) in OBSERVED_HEADERS {
            if let Some(profile) = observed_launch(name, Some(OBSERVED_LAUNCH_SCHEMA), header) {
                let smoke = name == "weapon_smokegrenade";
                assert_eq!(profile.airborne_overhand_horizontal_inheritance, smoke.then_some(1.25));
                assert_eq!(profile.airborne_overhand_vertical_inheritance, smoke.then_some(1.25));
                assert_eq!(profile.airborne_overhand_velocity.is_some(), smoke);
                if let Some(velocity) = profile.airborne_overhand_velocity {
                    assert_eq!(velocity.speed, 675.);
                    assert_eq!(velocity.upward_pitch_bias_degrees, 10.);
                }
                for (velocity, horizontal, vertical, speed) in [
                    (&profile.airborne_underhand_velocity, profile.airborne_underhand_horizontal_inheritance, profile.airborne_underhand_vertical_inheritance, 202.5),
                    (&profile.airborne_combined_velocity, profile.airborne_combined_horizontal_inheritance, profile.airborne_combined_vertical_inheritance, 438.75),
                ] {
                    assert_eq!(velocity.is_some(), smoke);
                    assert_eq!(horizontal, smoke.then_some(1.25));
                    assert_eq!(vertical, smoke.then_some(1.25));
                    if let Some(velocity) = velocity {
                        assert_eq!(velocity.speed, speed);
                        assert_eq!(velocity.upward_pitch_bias_degrees, 10.);
                    }
                }
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
                assert_eq!(launch.reference, "legacy-1.38.8.1-smoke-ground-and-air-button-response-128hz");
                for (velocity, speed) in [
                    (&launch.grounded_overhand_velocity, 675.),
                    (&launch.grounded_underhand_velocity, 202.5),
                    (&launch.grounded_combined_velocity, 438.75),
                    (&launch.airborne_underhand_velocity, 202.5),
                    (&launch.airborne_combined_velocity, 438.75),
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
                    (&launch.airborne_underhand_velocity, 202.5),
                    (&launch.airborne_combined_velocity, 438.75),
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
        for name in ["weapon_flashbang", "weapon_decoy"] {
            let launch = level.catalog.get(name).unwrap().grenade_launch.as_ref().unwrap();
            assert_eq!(launch.grounded_overhand_horizontal_inheritance, 0.5);
            let velocity = launch.grounded_overhand_velocity.as_ref().unwrap();
            assert_eq!(velocity.speed, 675.);
            assert_eq!(velocity.upward_pitch_bias_degrees, 10.);
            assert!(launch.grounded_underhand_velocity.is_none());
            assert!(launch.grounded_combined_velocity.is_none());
            assert!(launch.grounded_underhand_horizontal_inheritance.is_none());
            assert!(launch.grounded_combined_horizontal_inheritance.is_none());
        }
        assert!(level.catalog.get("weapon_incgrenade").unwrap().grenade_launch.is_none());
        for name in [
            "weapon_hegrenade",
            "weapon_flashbang",
            "weapon_smokegrenade",
            "weapon_molotov",
            "weapon_incgrenade",
            "weapon_decoy",
        ] {
            let definition = level.catalog.get(name).unwrap();
            let smoke = name == "weapon_smokegrenade";
            assert_eq!(definition.grenade_launch.as_ref().and_then(|p| p.airborne_overhand_horizontal_inheritance), smoke.then_some(1.25));
            assert_eq!(definition.grenade_launch.as_ref().and_then(|p| p.airborne_overhand_vertical_inheritance), smoke.then_some(1.25));
            assert_eq!(definition.grenade_launch.as_ref().and_then(|p| p.airborne_overhand_velocity.as_ref()).is_some(), smoke);
            assert_eq!(definition.grenade_launch.as_ref().and_then(|p| p.airborne_underhand_velocity.as_ref()).is_some(), smoke);
            assert_eq!(definition.grenade_launch.as_ref().and_then(|p| p.airborne_combined_velocity.as_ref()).is_some(), smoke);
            assert_eq!(definition.grenade_launch.as_ref().and_then(|p| p.airborne_underhand_horizontal_inheritance), smoke.then_some(1.25));
            assert_eq!(definition.grenade_launch.as_ref().and_then(|p| p.airborne_underhand_vertical_inheritance), smoke.then_some(1.25));
            assert_eq!(definition.grenade_launch.as_ref().and_then(|p| p.airborne_combined_horizontal_inheritance), smoke.then_some(1.25));
            assert_eq!(definition.grenade_launch.as_ref().and_then(|p| p.airborne_combined_vertical_inheritance), smoke.then_some(1.25));
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

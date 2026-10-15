//! Derived observations only: no native implementation or game data table.
use content::{ContentId, ContentKind};
use sha1::{Digest, Sha1};
use weapons::{Catalog, Recoil, RecoilTrajectory, WeaponDefinition};

// Exact installed item-schema identity observed with Legacy 1.38.8.1.
// Historical schemas and other content packs do not inherit this profile
// merely because they use the same weapon name, seed or magnitude.
const OBSERVED_SCHEMA: [u8; 20] = [
    0xc1, 0x36, 0x1d, 0x35, 0x18, 0x02, 0xb7, 0xbe, 0x5f, 0x55, 0x24, 0x5c, 0x33, 0x1f, 0x5f, 0x1d,
    0x6b, 0x36, 0xad, 0x35,
];

// Shot-ordered, no-spread bullet ray offsets in degrees. Derived from two
// owned standing 128-tick native magazines, independently calibrated by
// known camera rotations (focal length 600px at 1600x900). Native boxes have
// about 2px center uncertainty; rounded degrees do not imply exact precision.
// A normal-timescale held magazine independently reproduces the marker cloud.
// This is a 2023 installed-game profile, not proof of the requested 2015 feel.
const AK_BULLET_ANGLES: [[f32; 2]; 30] = [
    [0.00, 0.00],
    [-0.19, 0.00],
    [-1.19, 0.00],
    [-2.86, -0.20],
    [-4.57, -0.15],
    [-6.18, 0.48],
    [-7.78, 0.91],
    [-8.81, 1.66],
    [-9.65, 0.76],
    [-9.37, -1.61],
    [-9.70, -2.74],
    [-10.30, -2.07],
    [-10.39, -2.92],
    [-9.94, -4.29],
    [-10.13, -4.43],
    [-10.44, -2.50],
    [-10.85, -1.32],
    [-11.13, -0.47],
    [-10.99, 0.94],
    [-10.58, 2.78],
    [-10.67, 1.79],
    [-10.85, 2.02],
    [-11.04, 1.60],
    [-11.31, 1.13],
    [-11.04, 2.16],
    [-11.27, 2.49],
    [-11.54, 1.50],
    [-11.22, -0.42],
    [-10.21, -2.83],
    [-10.12, -3.58],
];

// Independently admitted M4A4 observations at the same measured projection.
// Two ordered slow magazines plus a held-out normal-speed aggregate; 2px
// marker uncertainty. This is neither an AK rescale nor historical proof.
const M4A4_BULLET_ANGLES: [[f32; 2]; 30] = [
    [0.00, 0.00],
    [-0.24, 0.00],
    [-0.72, 0.05],
    [-1.67, -0.20],
    [-2.86, 0.10],
    [-4.19, -0.29],
    [-5.43, -0.58],
    [-6.32, 0.29],
    [-7.13, 0.86],
    [-7.22, 2.01],
    [-7.69, 1.76],
    [-8.16, 0.86],
    [-8.02, -0.71],
    [-7.93, -2.10],
    [-7.51, -3.39],
    [-7.84, -3.48],
    [-8.12, -3.04],
    [-8.08, -3.52],
    [-7.85, -4.28],
    [-8.08, -4.23],
    [-8.16, -2.38],
    [-8.39, -1.81],
    [-8.44, -0.38],
    [-8.62, 0.05],
    [-8.81, 0.66],
    [-8.90, 0.14],
    [-9.00, 0.47],
    [-9.04, 0.66],
    [-9.18, 0.76],
    [-9.18, 1.04],
];

fn is_observed_m4a4(def: &WeaponDefinition) -> bool {
    def.id == ContentId::csgo(ContentKind::Weapon, "m4a1")
        && def.class_name == "weapon_m4a1"
        && def.def_index == 16
        && def.clip_size == 30
        && def.bullets == 1
        && def.full_auto
        && def.cycle_time == 0.09
        && def.recoil
            == Recoil {
                seed: 38965,
                angle: 0.,
                angle_variance: 70.,
                magnitude: 23.,
                magnitude_variance: 0.,
            }
}

// Fresh owner-requested retakes independently observed both suppressor modes.
// Two ordered magazines and a normal-speed aggregate per mode, with 2px
// marker uncertainty. The physically disturbed earlier run is excluded.
const M4S_ATTACHED_BULLET_ANGLES: [[f32; 2]; 20] = [
    [0.00, 0.00],
    [-0.14, 0.00],
    [-0.33, 0.05],
    [-1.00, -0.15],
    [-2.00, 0.05],
    [-2.96, -0.24],
    [-4.10, -0.39],
    [-4.76, 0.29],
    [-5.24, 0.68],
    [-5.34, 1.59],
    [-5.76, 1.25],
    [-6.09, 0.48],
    [-5.90, -0.77],
    [-5.81, -1.73],
    [-5.44, -2.60],
    [-5.81, -2.55],
    [-6.14, -2.11],
    [-6.00, -2.59],
    [-5.82, -3.08],
    [-5.91, -2.98],
];

const M4S_DETACHED_BULLET_ANGLES: [[f32; 2]; 20] = [
    [0.00, 0.00],
    [-0.24, 0.05],
    [-0.86, 0.05],
    [-2.20, -0.24],
    [-3.24, 0.10],
    [-4.57, -0.29],
    [-5.81, -0.58],
    [-6.75, 0.38],
    [-7.50, 0.91],
    [-7.69, 2.24],
    [-8.02, 1.81],
    [-8.44, 0.76],
    [-8.20, -0.81],
    [-7.88, -2.10],
    [-7.23, -3.20],
    [-7.23, -2.96],
    [-7.56, -2.62],
    [-7.51, -3.10],
    [-7.14, -3.73],
    [-7.42, -3.63],
];

// Two independently ordered 35-shot native magazines, with a normal-speed
// held-out aggregate. Projection was re-calibrated in both static ROIs.
// Overlapping boxes were reviewed chronologically; centers have 2px
// uncertainty. This is installed 2023 standing-held evidence, not 2015 or
// recovery/camera parity. No native impulse table or algorithm is copied.
const GALIL_BULLET_ANGLES: [[f32; 2]; 35] = [
    [0.00, 0.00],
    [-0.10, 0.10],
    [-0.33, 0.05],
    [-0.91, 0.49],
    [-1.86, 1.03],
    [-2.91, 1.02],
    [-4.20, 1.16],
    [-5.15, 1.45],
    [-5.72, 2.07],
    [-6.38, 1.82],
    [-6.75, 0.57],
    [-6.66, -0.96],
    [-6.10, -2.59],
    [-6.38, -3.07],
    [-6.44, -3.59],
    [-6.53, -3.93],
    [-6.68, -4.07],
    [-7.00, -3.82],
    [-7.27, -2.44],
    [-7.50, -1.72],
    [-7.41, -0.38],
    [-6.94, 1.34],
    [-6.94, 1.67],
    [-7.22, 1.00],
    [-7.36, 1.62],
    [-7.32, 2.20],
    [-7.00, 3.16],
    [-7.04, 2.49],
    [-6.89, 0.86],
    [-6.75, -0.53],
    [-6.99, -1.24],
    [-7.31, -0.91],
    [-7.22, -1.67],
    [-6.57, -2.97],
    [-6.44, -3.69],
];

// Normal full-auto only: sustained actual receiver/HUD admission, two ordered
// 25-shot magazines and a normal-speed aggregate. Native markers have 2px
// uncertainty. Burst-mode pattern selection excludes this observed profile.
const FAMAS_NORMAL_BULLET_ANGLES: [[f32; 2]; 25] = [
    [0.00, 0.00],
    [-0.05, -0.05],
    [-0.24, -0.10],
    [-0.81, -0.54],
    [-1.86, -0.54],
    [-3.05, -0.49],
    [-3.91, 0.19],
    [-4.58, 1.06],
    [-5.29, 0.77],
    [-5.62, -0.29],
    [-5.90, -1.15],
    [-6.00, -1.92],
    [-6.38, -1.63],
    [-6.56, -0.38],
    [-6.70, 0.14],
    [-6.66, 1.25],
    [-6.85, 1.63],
    [-6.71, 2.39],
    [-6.85, 2.49],
    [-7.08, 2.34],
    [-7.13, 1.00],
    [-7.17, 0.77],
    [-7.13, 1.34],
    [-6.75, 2.16],
    [-6.34, 3.07],
];

fn is_observed_famas_normal(def: &WeaponDefinition) -> bool {
    def.id == ContentId::csgo(ContentKind::Weapon, "famas")
        && def.class_name == "weapon_famas"
        && def.def_index == 10
        && def.clip_size == 25
        && def.bullets == 1
        && def.full_auto
        && def.cycle_time == 0.09
        && def.attr_f32("has burst mode") == Some(1.)
        && def.recoil == Recoil {
            seed: 39623, angle: 0., angle_variance: 60.,
            magnitude: 20., magnitude_variance: 1.,
        }
}

fn is_observed_galil(def: &WeaponDefinition) -> bool {
    def.id == ContentId::csgo(ContentKind::Weapon, "galilar")
        && def.class_name == "weapon_galilar"
        && def.def_index == 13
        && def.clip_size == 35
        && def.bullets == 1
        && def.full_auto
        && def.cycle_time == 0.09
        && def.recoil == Recoil {
            seed: 51191, angle: 0., angle_variance: 70.,
            magnitude: 21., magnitude_variance: 1.,
        }
}

fn is_observed_m4s(def: &WeaponDefinition) -> bool {
    def.id == ContentId::csgo(ContentKind::Weapon, "m4a1_silencer")
        && def.class_name == "weapon_m4a1_silencer"
        && def.def_index == 60
        && def.clip_size == 20
        && def.bullets == 1
        && def.full_auto
        && def.cycle_time == 0.1
        && def.attr_f32("has silencer") == Some(1.)
        && def.recoil
            == Recoil {
                seed: 38965,
                angle: 0.,
                angle_variance: 65.,
                magnitude: 25.,
                magnitude_variance: 3.,
            }
        && def.attr_f32("recoil angle alt") == Some(0.)
        && def.attr_f32("recoil angle variance alt") == Some(65.)
        && def.attr_f32("recoil magnitude alt") == Some(21.)
        && def.attr_f32("recoil magnitude variance alt") == Some(0.)
}

fn hash_profile(digest: &mut Sha1, mode: &[u8], profile: &RecoilTrajectory) {
    digest.update(mode);
    digest.update(profile.reference.as_bytes());
    for angle in &profile.bullet_angles {
        digest.update(angle[0].to_le_bytes());
        digest.update(angle[1].to_le_bytes());
    }
}

fn is_observed_ak(def: &WeaponDefinition) -> bool {
    def.id == ContentId::csgo(ContentKind::Weapon, "ak47")
        && def.class_name == "weapon_ak47"
        && def.def_index == 7
        && def.clip_size == 30
        && def.bullets == 1
        && def.full_auto
        && def.cycle_time == 0.1
        && def.recoil
            == Recoil {
                seed: 223,
                angle: 0.,
                angle_variance: 70.,
                magnitude: 30.,
                magnitude_variance: 0.,
            }
}

pub(super) fn import(catalog: &mut Catalog, schema: Option<[u8; 20]>) -> Option<[u8; 20]> {
    let schema = schema?;
    if schema != OBSERVED_SCHEMA {
        return Some(schema);
    }
    let mut digest = Sha1::new();
    digest.update(schema);
    // A peer with the same assets but the previous recoil policy must fail
    // content admission before prediction/snapshots can silently disagree.
    digest.update(b"observed-recoil-trajectory-v5-ak-m4-m4s-galil-famas-normal-legacy-1.38.8.1");
    let mut applied = false;
    {
        for def in &mut catalog.weapons {
            let (reference, angles): (&str, &[[f32; 2]]) = if is_observed_ak(def) {
                ("legacy-1.38.8.1-ak-held-standing-128hz", &AK_BULLET_ANGLES)
            } else if is_observed_m4a4(def) {
                (
                    "legacy-1.38.8.1-m4a4-held-standing-128hz",
                    &M4A4_BULLET_ANGLES,
                )
            } else if is_observed_famas_normal(def) {
                ("legacy-1.38.8.1-famas-normal-held-standing-128hz", &FAMAS_NORMAL_BULLET_ANGLES)
            } else if is_observed_galil(def) {
                ("legacy-1.38.8.1-galil-held-standing-128hz", &GALIL_BULLET_ANGLES)
            } else if is_observed_m4s(def) {
                def.recoil_trajectory_unsuppressed = Some(RecoilTrajectory {
                    reference: "legacy-1.38.8.1-m4s-detached-held-standing-128hz".into(),
                    bullet_angles: M4S_DETACHED_BULLET_ANGLES.to_vec(),
                });
                (
                    "legacy-1.38.8.1-m4s-attached-held-standing-128hz",
                    &M4S_ATTACHED_BULLET_ANGLES,
                )
            } else {
                continue;
            };
            applied = true;
            def.recoil_trajectory = Some(RecoilTrajectory {
                reference: reference.into(),
                bullet_angles: angles.to_vec(),
            });
            digest.update(def.id.to_string().as_bytes());
            hash_profile(
                &mut digest,
                b"default-equipped",
                def.recoil_trajectory.as_ref().unwrap(),
            );
            if let Some(detached) = &def.recoil_trajectory_unsuppressed {
                hash_profile(&mut digest, b"suppressor-detached", detached);
            }
        }
    }
    if applied {
        Some(digest.finalize().into())
    } else {
        Some(schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_observation_is_attached_only_to_its_exact_catalog_identity() {
        let game =
            std::path::PathBuf::from(std::env::var_os("CSR_GAME_DIR").unwrap_or_else(|| {
                r"C:\Program Files (x86)\Steam\steamapps\common\csgo legacy\csgo".into()
            }));
        let Ok(bytes) = std::fs::read(game.join("scripts/items/items_game.txt")) else {
            return;
        };
        let schema: [u8; 20] = Sha1::digest(&bytes).into();
        if schema != OBSERVED_SCHEMA {
            return;
        }
        let mut catalog = Catalog::from_items_game(&kv::decode_text(&bytes)).unwrap();
        let previous = schema;
        let next = import(&mut catalog, Some(schema)).unwrap();
        assert_ne!(
            previous, next,
            "old recoil policy must not share network identity"
        );
        let ak = catalog.get("weapon_ak47").unwrap();
        let profile = ak.recoil_trajectory.as_ref().unwrap();
        assert_eq!(profile.bullet_angles.len(), 30);
        let m4 = catalog.get("weapon_m4a1").unwrap();
        let m4_profile = m4.recoil_trajectory.as_ref().unwrap();
        assert_eq!(m4_profile.bullet_angles.len(), 30);
        assert_ne!(m4_profile.bullet_angles, profile.bullet_angles);
        let galil = catalog.get("weapon_galilar").unwrap();
        assert_eq!(galil.recoil_trajectory.as_ref().unwrap().bullet_angles.len(), 35);
        let famas = catalog.get("weapon_famas").unwrap();
        assert_eq!(famas.recoil_trajectory.as_ref().unwrap().bullet_angles.len(), 25);
        assert!(famas.recoil_trajectory_unsuppressed.is_none());
        for case in 0..4 {
            let mut changed = famas.clone(); changed.recoil_trajectory = None;
            match case {
                0 => changed.recoil.seed += 1,
                1 => changed.cycle_time = 0.1,
                2 => changed.id = ContentId::csgo(ContentKind::Weapon, "pack_famas"),
                _ => { changed.attributes.insert("has burst mode".into(), "0".into()); }
            }
            let mut unmeasured = Catalog { weapons: vec![changed] };
            assert_eq!(import(&mut unmeasured, Some(schema)), Some(schema));
            assert!(unmeasured.weapons[0].recoil_trajectory.is_none());
        }
        for case in 0..3 {
            let mut changed = galil.clone(); changed.recoil_trajectory = None;
            match case {
                0 => changed.recoil.seed += 1,
                1 => changed.cycle_time = 0.1,
                _ => changed.id = ContentId::csgo(ContentKind::Weapon, "pack_galil"),
            }
            let mut unmeasured = Catalog { weapons: vec![changed] };
            assert_eq!(import(&mut unmeasured, Some(schema)), Some(schema));
            assert!(unmeasured.weapons[0].recoil_trajectory.is_none());
        }
        let m4s = catalog.get("weapon_m4a1_silencer").unwrap();
        let attached = m4s.recoil_trajectory.as_ref().unwrap();
        let detached = m4s.recoil_trajectory_unsuppressed.as_ref().unwrap();
        assert_eq!(attached.bullet_angles.len(), 20);
        assert_eq!(detached.bullet_angles.len(), 20);
        assert_ne!(attached.bullet_angles, detached.bullet_angles);
        let mut modified = m4s.clone();
        modified.recoil_trajectory = None;
        modified.recoil_trajectory_unsuppressed = None;
        modified
            .attributes
            .insert("recoil magnitude alt".into(), "22".into());
        let mut unmeasured = Catalog {
            weapons: vec![modified],
        };
        assert_eq!(import(&mut unmeasured, Some(schema)), Some(schema));
        assert!(unmeasured.weapons[0].recoil_trajectory.is_none());
        assert!(
            unmeasured.weapons[0]
                .recoil_trajectory_unsuppressed
                .is_none()
        );

        let mut other = Catalog {
            weapons: vec![ak.clone()],
        };
        other.weapons[0].recoil_trajectory = None;
        import(&mut other, Some([0; 20]));
        assert!(
            other.weapons[0].recoil_trajectory.is_none(),
            "different schema is unmeasured"
        );
        other.weapons[0].recoil.seed += 1;
        import(&mut other, Some(schema));
        assert!(
            other.weapons[0].recoil_trajectory.is_none(),
            "different stats are unmeasured"
        );
    }
}

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
    let mut digest = Sha1::new();
    digest.update(schema);
    // A peer with the same assets but the previous recoil policy must fail
    // content admission before prediction/snapshots can silently disagree.
    digest.update(b"observed-recoil-trajectory-v1-ak-legacy-1.38.8.1");
    if schema == OBSERVED_SCHEMA {
        for def in catalog.weapons.iter_mut().filter(|def| is_observed_ak(def)) {
            def.recoil_trajectory = Some(RecoilTrajectory {
                reference: "legacy-1.38.8.1-ak-held-standing-128hz".into(),
                bullet_angles: AK_BULLET_ANGLES.to_vec(),
            });
            digest.update(def.id.to_string().as_bytes());
            for angle in AK_BULLET_ANGLES {
                digest.update(angle[0].to_le_bytes());
                digest.update(angle[1].to_le_bytes());
            }
        }
    }
    Some(digest.finalize().into())
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
        assert!(
            catalog
                .get("weapon_m4a1")
                .unwrap()
                .recoil_trajectory
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

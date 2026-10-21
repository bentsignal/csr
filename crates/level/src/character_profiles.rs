//! Read character collision and animation through the selected runtime VFS.
use crate::LoadError;
use character_pose::{AnimInput, AnimState, PlayerAnimSet};
use content::{ContentId, ContentKind};
use sha1::{Digest, Sha1};
use sim::model_pose::{Profile, Profiles};
use std::sync::Arc;

pub(crate) fn load(
    fs: &vfs::Vfs,
    previous: Option<[u8; 20]>,
) -> Result<(Arc<Profiles>, Option<[u8; 20]>), LoadError> {
    let mut digest = Sha1::new();
    digest.update(b"character-model-hitboxes-v1");
    if let Some(previous) = previous {
        digest.update(previous);
    }
    // Both teams independently decode and retarget a large animation set.
    // Overlap that work with one bounded helper; nothing survives this load or
    // crosses its VFS. Replay the original T/CT digest order after joining.
    let (terrorist, counter_terrorist) = std::thread::scope(|scope| {
        let terrorist = std::thread::Builder::new()
            .name("map-team-profile".into())
            .spawn_scoped(scope, || read_identified(fs, "tm_phoenix", T_MODEL));
        let counter_terrorist = read_identified(fs, "ctm_sas", CT_MODEL);
        let terrorist = match terrorist {
            Ok(worker) => worker.join().unwrap_or_else(|_| {
                Err(LoadError::Io("character profile worker stopped".into()))
            }),
            // Resource pressure must not prevent an otherwise valid map load.
            Err(_) => read_identified(fs, "tm_phoenix", T_MODEL),
        };
        (terrorist, counter_terrorist)
    });
    let (terrorist, t_identity) = terrorist?;
    let (counter_terrorist, ct_identity) = counter_terrorist?;
    digest.update(t_identity);
    digest.update(ct_identity);
    let profiles = Profiles { terrorist, counter_terrorist };
    let hash = if profiles.terrorist.is_none() && profiles.counter_terrorist.is_none() {
        previous
    } else {
        previous.map(|_| digest.finalize().into())
    };
    Ok((Arc::new(profiles), hash))
}

const T_MODEL: &str = "models/player/custom_player/legacy/tm_phoenix.mdl";
const CT_MODEL: &str = "models/player/custom_player/legacy/ctm_sas.mdl";

// The small identity transcript contains only paths and fixed-size hashes,
// retaining exactly the pre-parallel import's network catalog identity.
fn read_identified(
    fs: &vfs::Vfs,
    name: &str,
    path: &str,
) -> Result<(Option<Profile>, Vec<u8>), LoadError> {
    let mut identity = Vec::new();
    let profile = read_profile(fs, name, path, &mut identity)?;
    Ok((profile, identity))
}

fn read_profile(
    fs: &vfs::Vfs,
    name: &str,
    path: &str,
    identity: &mut Vec<u8>,
) -> Result<Option<Profile>, LoadError> {
    identity.extend_from_slice(path.as_bytes());
    identity.extend_from_slice(&[0]);
    let Some(bytes) = fs.read(path) else {
        identity.extend_from_slice(b"absent");
        return Ok(None);
    };
    identity.extend_from_slice(&Sha1::digest(&bytes));
    let model = mdl::Model::load_header(&bytes)
        .map_err(|e| LoadError::Io(format!("character profile {path}: {e}")))?;
    let Some(set) = model
        .hitbox_sets
        .first()
        .cloned()
        .filter(|set| !set.hitboxes.is_empty())
    else {
        return Ok(None);
    };
    if set.hitboxes.len() > 128 || model.bones.len() > 256 {
        return Err(LoadError::Io(format!(
            "character profile {path} exceeds pose budget"
        )));
    }
    // The current controller reads the first include model only. Hash exactly
    // its consumed bytes, including the embedded animation data.
    let Some(include) = model.include_models.first() else {
        return Ok(None);
    };
    identity.extend_from_slice(include.as_bytes());
    identity.extend_from_slice(&[0]);
    let Some(include_bytes) = fs.read(include) else {
        identity.extend_from_slice(b"absent");
        return Ok(None);
    };
    identity.extend_from_slice(&Sha1::digest(&include_bytes));
    let Some(animations) = PlayerAnimSet::load(&model, fs) else {
        return Err(LoadError::Io(format!(
            "character animation profile unavailable: {path}"
        )));
    };
    let input = AnimInput {
        speed: 0.,
        move_yaw: 0.,
        on_ground: true,
        ducked: false,
        pitch: 0.,
        class: weapons::WeaponClass::Knife,
        time_since_fire: 10.,
        reload_progress: None,
        air_time: 0.,
    };
    let pose = animations.evaluate_prepared(&mut AnimState::default(), &input, 0.);
    set.pose(&pose)
        .map_err(|e| LoadError::Io(format!("character profile {path}: {e}")))?;
    Ok(Some(Profile {
        id: ContentId::csgo(ContentKind::Character, name),
        animations: Arc::new(animations),
        hitboxes: set,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    // Synthetic MDL declarations only: one rigid bone, one box and an empty
    // include animation set. No installed game data or renderer is needed.
    fn model(include: &str, extent: f32) -> Vec<u8> {
        let mut bytes = vec![0; 704 + include.len() + 1];
        bytes[..4].copy_from_slice(b"IDST");
        for (offset, value) in [
            (4, 49i32), (156, 1), (160, 400), (172, 1), (176, 616),
            (336, 1), (340, 696), (404, -1), (620, 1), (624, 12), (700, 8),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        for (offset, value) in [(456, 1f32), (648, extent), (652, extent), (656, extent), (676, -1.)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[704..704 + include.len()].copy_from_slice(include.as_bytes());
        bytes
    }

    #[test]
    fn parallel_profiles_preserve_digest_order_and_read_each_levels_own_bytes() {
        use std::collections::BTreeMap;
        let previous = [7; 20];
        let mut last = None;
        for extent in [1., 2.] {
            let mut files = BTreeMap::new();
            let mut expected = Sha1::new();
            expected.update(b"character-model-hitboxes-v1");
            expected.update(previous);
            for (path, include) in [(T_MODEL, "models/t_anims.mdl"), (CT_MODEL, "models/ct_anims.mdl")] {
                let model = model(include, extent);
                let mut animations = vec![0; 400];
                animations[..4].copy_from_slice(b"IDST");
                animations[4..8].copy_from_slice(&49i32.to_le_bytes());
                // Original serial loader's exact digest transcript.
                expected.update(path.as_bytes());
                expected.update([0]);
                expected.update(Sha1::digest(&model));
                expected.update(include.as_bytes());
                expected.update([0]);
                expected.update(Sha1::digest(&animations));
                files.insert(path.into(), Arc::<[u8]>::from(model));
                files.insert(include.into(), Arc::<[u8]>::from(animations));
            }
            let mut fs = vfs::Vfs::new();
            fs.push_front("map", vfs::Mount::Memory(files));
            let (profiles, hash) = load(&fs, Some(previous)).unwrap();
            assert_eq!(hash, Some(<[u8; 20]>::from(expected.finalize())));
            for (profile, name) in [(&profiles.terrorist, "tm_phoenix"), (&profiles.counter_terrorist, "ctm_sas")] {
                let profile = profile.as_ref().unwrap();
                assert_eq!(profile.id, ContentId::csgo(ContentKind::Character, name));
                assert_eq!(profile.hitboxes.hitboxes[0].max, glam::Vec3::splat(extent));
            }
            if let Some(last) = last { assert_ne!(hash, last); }
            last = Some(hash);
        }
    }

    #[test]
    fn missing_profiles_keep_previous_identity_and_malformed_profiles_fail() {
        use std::collections::BTreeMap;
        let mut fs = vfs::Vfs::new();
        let (profiles, hash) = load(&fs, Some([3; 20])).unwrap();
        assert!(profiles.terrorist.is_none() && profiles.counter_terrorist.is_none());
        assert_eq!(hash, Some([3; 20]));
        fs.push_front("map", vfs::Mount::Memory(BTreeMap::from([
            (CT_MODEL.into(), Arc::<[u8]>::from(&b"bad header"[..])),
        ])));
        let error = load(&fs, Some([3; 20])).err().expect("malformed model must fail");
        assert!(error.to_string().contains(CT_MODEL));
    }

    #[test]
    #[ignore = "requires installed Legacy character animation data"]
    fn installed_character_shapes_remain_rigid_through_combat_poses() {
        let level = crate::Level::load("de_dust2", None).unwrap();
        for profile in [
            &level.character_profiles.terrorist,
            &level.character_profiles.counter_terrorist,
        ] {
            let profile = profile.as_ref().expect("installed team profile");
            assert_eq!(profile.hitboxes.hitboxes.len(), 19);
            for class in [
                weapons::WeaponClass::Rifle,
                weapons::WeaponClass::Pistol,
                weapons::WeaponClass::Knife,
                weapons::WeaponClass::Sniper,
            ] {
                for (speed, ducked, ground, air, reload, fire) in [
                    (0., false, true, 0., None, 10.),
                    (230., false, true, 0., None, 0.03),
                    (70., true, true, 0., Some(0.5), 10.),
                    (120., false, false, 0.2, None, 10.),
                    (120., true, false, 0.8, None, 0.15),
                ] {
                    for pitch in [-85., 0., 85.] {
                        let input = AnimInput {
                            speed,
                            move_yaw: 137.,
                            on_ground: ground,
                            ducked,
                            pitch,
                            class,
                            time_since_fire: fire,
                            reload_progress: reload,
                            air_time: air,
                        };
                        let pose = profile.animations.evaluate_prepared(
                            &mut AnimState { move_cycle: 0.73 },
                            &input,
                            0.,
                        );
                        let shapes = profile.hitboxes.pose(&pose).unwrap_or_else(|e| {
                            panic!("{} {:?} pitch{pitch}: {e}", profile.id, class)
                        });
                        assert_eq!(shapes.len(), 19);
                    }
                }
            }
        }
    }
}

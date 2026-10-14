//! Read character collision and animation through the selected runtime VFS.
use crate::LoadError;
use character_pose::{AnimInput, AnimState, PlayerAnimSet};
use content::{ContentId, ContentKind, GameOrigin};
use sha1::{Digest, Sha1};
use sim::model_pose::{Profile, Profiles};
use std::sync::Arc;

pub(crate) fn load(
    fs: &vfs::Vfs,
    origin: &GameOrigin,
    previous: Option<[u8; 20]>,
) -> Result<(Arc<Profiles>, Option<[u8; 20]>), LoadError> {
    let mut digest = Sha1::new();
    digest.update(b"character-model-hitboxes-v1");
    if let Some(previous) = previous {
        digest.update(previous);
    }
    let mut read = |name: &str, path: &str| -> Result<Option<Profile>, LoadError> {
        digest.update(path.as_bytes());
        digest.update([0]);
        let Some(bytes) = fs.read(path) else {
            digest.update(b"absent");
            return Ok(None);
        };
        digest.update(Sha1::digest(&bytes));
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
        digest.update(include.as_bytes());
        digest.update([0]);
        let Some(include_bytes) = fs.read(include) else {
            digest.update(b"absent");
            return Ok(None);
        };
        digest.update(Sha1::digest(&include_bytes));
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
            id: ContentId::new(origin.clone(), ContentKind::Character, name),
            animations: Arc::new(animations),
            hitboxes: set,
        }))
    };
    let profiles = Profiles {
        terrorist: read(
            "tm_phoenix",
            "models/player/custom_player/legacy/tm_phoenix.mdl",
        )?,
        counter_terrorist: read("ctm_sas", "models/player/custom_player/legacy/ctm_sas.mdl")?,
    };
    let hash = if profiles.terrorist.is_none() && profiles.counter_terrorist.is_none() {
        previous
    } else {
        previous.map(|_| digest.finalize().into())
    };
    Ok((Arc::new(profiles), hash))
}

#[cfg(test)]
mod tests {
    use super::*;
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

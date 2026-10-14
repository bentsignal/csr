//! Installed static throwcharge poses, separate from ballistic throw strength.
use glam::{Quat, Vec3};
use mdl::anim::Clip;
use sim::utility::{ThrowMode, ThrowPhase, ThrowState};

pub(super) struct HoldPoses {
    poses: [Clip; 3],
    weights: Vec<f32>,
}

impl HoldPoses {
    /// This is the authored three-pose grenade subset, not a general MDL
    /// blend-grid player. Unsupported or incomplete content keeps its base pose.
    pub(super) fn load(model: &mdl::Model, ani: Option<&[u8]>) -> Option<Self> {
        let sequence = model
            .sequences
            .iter()
            .find(|s| s.name == "throwcharge_blend")?;
        let parameter = model
            .pose_params
            .get(usize::try_from(sequence.param_index[0]).ok()?)?;
        if sequence.group_size != [3, 1]
            || sequence.anims.len() != 3
            || sequence.bone_weights.len() != model.bones.len()
            || parameter.0 != "throwcharge"
            || (parameter.1, parameter.2) != (0., 1.)
            || (sequence.param_start[0], sequence.param_end[0]) != (0., 1.)
        {
            return None;
        }
        let mut poses = Vec::with_capacity(3);
        for (&index, name) in
            sequence
                .anims
                .iter()
                .zip(["throwcharge_low", "throwcharge_mid", "throwcharge_high"])
        {
            let index = usize::try_from(index).ok()?;
            if model.animations.get(index)?.name != name {
                return None;
            }
            let clip = model.decode_animation(index, ani).ok()?;
            if clip.delta || clip.frames.len() != 1 {
                return None;
            }
            poses.push(clip);
        }
        let weights: Vec<_> = (0..model.bones.len())
            .map(|bone| sequence.weight(bone))
            .collect();
        if !weights
            .iter()
            .all(|w| w.is_finite() && (0. ..=1.).contains(w))
        {
            return None;
        }
        Some(Self {
            poses: poses.try_into().ok()?,
            weights,
        })
    }

    pub(super) fn apply(&self, state: Option<ThrowState>, pose: &mut [(Vec3, Quat)]) {
        let Some(state) = state.filter(|s| s.phase == ThrowPhase::Holding) else {
            return;
        };
        // Discrete provisional presentation mapping. Do not feed the unmeasured
        // launch-strength coefficients into the authored pose parameter, or
        // invent a native transition/easing law from this static grid.
        let index = match state.mode {
            ThrowMode::Underhand => 0,
            ThrowMode::Combined => 1,
            ThrowMode::Overhand => 2,
        };
        self.poses[index].sample_layered(0., pose, |bone| {
            self.weights.get(bone).copied().unwrap_or(0.)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_modes_use_authored_mask_without_overriding_pin_or_release() {
        let hold = HoldPoses {
            poses: [1., 2., 3.].map(|x| Clip {
                name: "fixture".into(),
                fps: 30.,
                looping: false,
                delta: false,
                frames: vec![vec![(Vec3::X * x, Quat::IDENTITY); 2]],
                animated: vec![true; 2],
            }),
            weights: vec![1., 0.],
        };
        let original = vec![(Vec3::Y, Quat::IDENTITY); 2];
        for (mode, expected) in [
            (ThrowMode::Underhand, 1.),
            (ThrowMode::Combined, 2.),
            (ThrowMode::Overhand, 3.),
        ] {
            for phase in [
                ThrowPhase::PullingPin,
                ThrowPhase::Holding,
                ThrowPhase::Throwing,
            ] {
                let mut pose = original.clone();
                hold.apply(
                    Some(ThrowState {
                        phase,
                        mode,
                        pull_started_tick: 1,
                        throw_started_tick: None,
                        release_requested: false,
                        spawned: false,
                    }),
                    &mut pose,
                );
                assert_eq!(
                    pose[0].0,
                    if phase == ThrowPhase::Holding {
                        Vec3::X * expected
                    } else {
                        Vec3::Y
                    }
                );
                assert_eq!(pose[1], original[1], "unmasked bones retain the base clip");
            }
        }
        let mut pose = original.clone();
        hold.apply(None, &mut pose);
        assert_eq!(pose, original);
    }
}

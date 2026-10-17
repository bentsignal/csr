//! Absolute studio clips include decoded rest values for compressed-out tracks.
//! Retargeting also supplies target rest for bones absent from an include model.
//! Those decoded values are distinct from an authored zero-weight sequence mask.
use glam::{Quat, Vec3};
use mdl::anim::Clip;

pub(super) fn sample(
    clip: &Clip,
    time: f32,
    pose: &mut [(Vec3, Quat)],
    weight: impl Fn(usize) -> f32,
) {
    if clip.delta {
        // Preserve the existing delta-layer path; this change only addresses
        // absolute sequences and does not implement a new additive player.
        clip.sample_layered(time, pose, weight);
        return;
    }
    let mut decoded = pose.to_vec();
    clip.sample(time, &mut decoded);
    for (bone, (current, target)) in pose.iter_mut().zip(decoded).enumerate() {
        let weight = weight(bone);
        if weight >= 0.999 {
            *current = target;
        } else if weight > 0. {
            *current = (current.0.lerp(target.0, weight), current.1.slerp(target.1, weight));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip() -> Clip {
        Clip {
            name: "absolute idle with compressed rest tracks".into(),
            fps: 30., looping: false, delta: false,
            frames: vec![vec![(Vec3::X, Quat::IDENTITY); 3]; 2],
            animated: vec![true, false, false],
        }
    }

    #[test]
    fn omitted_absolute_tracks_return_to_decoded_rest_but_zero_masks_preserve_their_pose() {
        let old = (Vec3::splat(90.), Quat::from_rotation_y(1.));
        let mut pose = [old; 3];
        sample(&clip(), 0., &mut pose, |bone| if bone == 2 { 0. } else { 1. });
        assert_eq!(pose[0], (Vec3::X, Quat::IDENTITY));
        assert_eq!(pose[1], pose[0], "compressed-out absolute hand cannot retain a reload pose");
        assert_eq!(pose[2], old, "an explicit mask still owns preservation");
    }

    #[test]
    fn varying_absolute_samples_follow_timestamp_independently_of_prior_render_history() {
        let mut clip = clip();
        clip.fps = 2.;
        clip.frames = [0., 4., 12.].into_iter().map(|x| vec![
            (Vec3::X * x, Quat::from_rotation_z(x / 12.)),
            (Vec3::Y * 3., Quat::IDENTITY),
            (Vec3::Z * 5., Quat::IDENTITY),
        ]).collect();
        for hz in [30, 144] {
            let mut pose = [(Vec3::splat(-20.), Quat::from_rotation_x(0.5)); 3];
            for frame in 0..=hz {
                let t = frame as f32 / hz as f32;
                sample(&clip, t, &mut pose, |_| 1.);
                // Independent piecewise expectation, not another Clip::sample.
                let x = if t <= 0.5 { 8. * t } else { 4. + 16. * (t - 0.5) };
                assert!((pose[0].0 - Vec3::X * x).length() < 1e-5);
                assert!(pose[0].1.abs_diff_eq(Quat::from_rotation_z(x / 12.), 1e-5));
                assert_eq!(pose[1], (Vec3::Y * 3., Quat::IDENTITY));
                assert_eq!(pose[2], (Vec3::Z * 5., Quat::IDENTITY));
            }
            // A common off-grid timestamp after different histories, followed
            // by a backwards seek and an end clamp, must sample the same curve.
            for (t, x) in [(0.375, 3.), (0.125, 1.), (2., 12.)] {
                sample(&clip, t, &mut pose, |_| 1.);
                assert!((pose[0].0 - Vec3::X * x).length() < 1e-5);
                assert!(pose[0].1.abs_diff_eq(Quat::from_rotation_z(x / 12.), 1e-5));
            }
        }
    }

    #[test]
    fn include_model_retarget_resets_missing_target_bones_preserves_masks_and_updates_merged_arms() {
        use bevy::prelude::{Transform, World};
        use crate::skinning::{self, SkinnedInstance};
        let bone = |name: &str, parent, pos| mdl::skeleton::Bone {
            name: name.into(), parent, pos, quat: Quat::IDENTITY, rot: Vec3::ZERO,
            pos_scale: Vec3::ONE, rot_scale: Vec3::ONE,
            pose_to_bone: glam::Mat4::IDENTITY, flags: 0,
        };
        // Synthetic include-model skeleton omits a target-only hand bone.
        // This exercises the loader's retarget path without claiming a native
        // include-model sequence should or should not preserve that bone.
        let source = vec![bone("root", -1, Vec3::X), bone("masked", 0, Vec3::Y * 2.)];
        let mut target = source.clone();
        target.push(bone("target_only_hand", 0, Vec3::Z * 3.));
        let source_clip = Clip {
            name: "synthetic include".into(), fps: 2., looping: false, delta: false,
            frames: [10., 14.].into_iter().map(|x| vec![
                (Vec3::X * x, Quat::IDENTITY), (Vec3::Y * 200., Quat::IDENTITY),
            ]).collect(),
            animated: vec![true, true],
        };
        let sequence = mdl::anim::Sequence {
            name: "include fixture".into(), activity: "ACT_VM_IDLE".into(), flags: 0,
            anims: vec![0], group_size: [1, 1], param_index: [-1, -1],
            param_start: [0., 0.], param_end: [0., 0.], fade_in: 0., fade_out: 0.,
            bone_weights: vec![1., 0.], events: Vec::new(),
        };
        let clip = mdl::anim::retarget(&source_clip, &source, &target);
        let weights = mdl::anim::retarget_weights(&sequence, &source, &target);
        assert_eq!(clip.animated, [true, true, false]);
        assert_eq!(weights, [1., 0., 1.]);
        let masked = (Vec3::Y * 9., Quat::IDENTITY);
        let mut pose = vec![(Vec3::splat(90.), Quat::IDENTITY), masked,
            (Vec3::splat(80.), Quat::from_rotation_x(1.))];
        sample(&clip, 0.25, &mut pose, |i| weights[i]);
        assert_eq!(pose, [(Vec3::X * 12., Quat::IDENTITY), masked,
            (Vec3::Z * 3., Quat::IDENTITY)]);
        let gun_pose = mdl::skeleton::model_space(&target, &pose);
        let arms = vec![bone("target_only_hand", -1, Vec3::splat(-1.)),
            bone("cuff", 0, Vec3::Y * 2.)];
        let arm_pose = skinning::bone_merge(&arms, &target, &gun_pose);
        // Apply both poses to real ECS joints, then check numeric world-space
        // transforms independently of bone_merge/pose_transforms expectations.
        let mut world = World::new();
        let mut instance = |count| SkinnedInstance {
            joints: (0..count).map(|_| world.spawn(Transform::IDENTITY).id()).collect(),
            meshes: Vec::new(),
        };
        let gun = instance(3);
        let arm = instance(2);
        let world_from_model = bevy::prelude::Mat4::from_translation(
            bevy::prelude::Vec3::new(100., 200., 300.));
        let mut query = world.query::<&mut Transform>();
        skinning::apply_pose(&gun, world_from_model, &gun_pose, &mut query.query_mut(&mut world));
        skinning::apply_pose(&arm, world_from_model, &arm_pose, &mut query.query_mut(&mut world));
        for (entity, expected) in gun.joints.iter().chain(&arm.joints).zip([
            [112., 200., 300.], [112., 200., 291.], [112., 203., 300.],
            [112., 203., 300.], [112., 203., 298.],
        ]) {
            let transform = world.get::<Transform>(*entity).unwrap();
            assert!(transform.translation.abs_diff_eq(bevy::prelude::Vec3::from_array(expected), 1e-5));
            assert!(transform.rotation.abs_diff_eq(bevy::prelude::Quat::IDENTITY, 1e-5));
            assert!(transform.scale.abs_diff_eq(bevy::prelude::Vec3::ONE, 1e-5));
        }
    }

    #[test]
    fn fractional_weights_apply_to_compressed_rest_values_too() {
        let mut pose = [(Vec3::X * 3., Quat::IDENTITY); 3];
        sample(&clip(), 0., &mut pose, |_| 0.5);
        assert_eq!(pose, [(Vec3::X * 2., Quat::IDENTITY); 3]);
    }

    #[test]
    fn delta_layers_keep_the_existing_track_admission() {
        let mut clip = clip();
        clip.delta = true;
        let mut pose = [(Vec3::X * 3., Quat::IDENTITY); 3];
        sample(&clip, 0., &mut pose, |_| 1.);
        assert_eq!(pose[0].0, Vec3::X);
        assert_eq!(pose[1].0, Vec3::X * 3.);
    }
}

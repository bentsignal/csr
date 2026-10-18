//! Absolute studio clips include decoded rest values for compressed-out tracks.
//! A missing track is distinct from an authored zero-weight sequence mask.
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
    fn full_weight_absolute_playback_has_no_prior_pose_or_render_rate_dependency() {
        for hz in [30, 144] {
            let mut pose = [(Vec3::splat(-20.), Quat::from_rotation_x(0.5)); 3];
            for frame in 0..=hz {
                sample(&clip(), frame as f32 / hz as f32, &mut pose, |_| 1.);
                assert_eq!(pose, [(Vec3::X, Quat::IDENTITY); 3]);
            }
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

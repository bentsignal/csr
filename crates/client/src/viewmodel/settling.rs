//! Shared forward speed-settling using the accepted M4A4 presentation.
//! This is the observed cycle mean, not the still-unmeasured periodic bob.
//! Camera/aim, weapon animation and simulated movement are not modified.
use glam::{Mat3, Vec3};

const REFERENCE_SPEED: f32 = 225.;
// Rotation VECTOR in the qualified Source eye-model basis, not QAngles.
// Twelve actual normal-time225u/s frames; forward-axis roll balanced zero.
const ROTATION_DEGREES: Vec3 = Vec3::new(0., 1.9237956, -0.53602093);
const TRANSLATION: Vec3 = Vec3::new(-1.8028913, -0.24614681, -0.20074478);

pub(super) struct Settling {
    pub rotation: Mat3,
    pub translation: Vec3,
    pub weight: f32,
}

impl Default for Settling {
    fn default() -> Self {
        Self { rotation: Mat3::IDENTITY, translation: Vec3::ZERO, weight: 0. }
    }
}

impl Settling {
    /// Continue the same planar-speed response across takeoff, landing and
    /// crouch transitions. Ground flags and vertical jump velocity cannot
    /// switch the accepted held pose to identity. No new air curve or rate.
    /// Coasting still follows actual velocity rather than movement buttons.
    pub fn sample(velocity: Vec3, forward: Vec3) -> Self {
        if !velocity.is_finite() || !forward.is_finite() {
            return Self::default();
        }
        let v = velocity.truncate();
        let f = forward.truncate().normalize_or_zero();
        // Only the observed forward component contributes. A hard direction
        // cutoff would snap the pose while turning; perpendicular speed does
        // not add an unmeasured response and backward motion stays neutral.
        let speed = v.dot(f);
        if !speed.is_finite() || speed <= f32::EPSILON {
            return Self::default();
        }
        let weight = (speed / REFERENCE_SPEED).min(1.);
        let axis_angle = ROTATION_DEGREES * (std::f32::consts::PI / 180.) * weight;
        let angle = axis_angle.length();
        Self {
            rotation: Mat3::from_axis_angle(axis_angle / angle, angle),
            translation: TRANSLATION * weight,
            weight,
        }
    }

    /// The measured points already include viewmodel_offset. Rotate that
    /// offset with the model before adding the measured eye-frame translation.
    pub fn offset(&self, authored_offset: Vec3) -> Vec3 {
        self.rotation * authored_offset + self.translation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_backward_transverse_and_invalid_motion_remain_neutral() {
        for (velocity, forward) in [
            (Vec3::ZERO, Vec3::X),
            (-Vec3::X * 225., Vec3::X),
            (Vec3::Y * 225., Vec3::X),
            (Vec3::splat(f32::NAN), Vec3::X),
        ] {
            let pose = Settling::sample(velocity, forward);
            assert_eq!(pose.rotation, Mat3::IDENTITY);
            assert_eq!(pose.translation, Vec3::ZERO);
            assert_eq!(pose.weight, 0.);
        }
    }

    #[test]
    fn takeoff_apex_and_landing_reuse_the_exact_grounded_planar_pose() {
        let ground = Settling::sample(Vec3::X * 225., Vec3::X);
        for vertical in [301., 100., 0., -100., -301.] {
            let airborne = Settling::sample(Vec3::new(225., 0., vertical), Vec3::X);
            assert_eq!(airborne.rotation, ground.rotation);
            assert_eq!(airborne.translation, ground.translation);
            assert_eq!(airborne.weight, ground.weight);
        }
        // Changes in actual planar speed still use the accepted walk scaling.
        let walk = Settling::sample(Vec3::new(117., 0., -301.), Vec3::X);
        assert!((walk.translation - ground.translation * 0.52).length() < 0.00001);
    }

    #[test]
    fn eye_frame_offset_and_model_rotate_together_under_arbitrary_current_view() {
        // Synthetic geometry, not installed game vertices. This catches the
        // wrong pivot/order and a world-fixed lowering vector after turning.
        let pose = Settling::sample(Vec3::X * 225., Vec3::X);
        let offset = Vec3::new(1., -1., -1.);
        let point = Vec3::new(30., -5., -2.);
        let current = mdl::angles_to_mat3(Vec3::new(-17., 123., 0.));
        let eye = Vec3::new(400., -800., 64.);
        let composed = current * pose.rotation * point + eye + current * pose.offset(offset);
        let direct = eye + current * (pose.rotation * (point + offset) + pose.translation);
        assert!((composed - direct).length() < 0.0001);
        // Presentation rotation never replaces the camera/aim forward vector.
        assert!((current * Vec3::X - movement::angle_vectors(Vec3::new(-17., 123., 0.)).0).length() < 0.00001);
    }

    #[test]
    fn observed_walk_scales_the_cycle_mean_and_speed_above_reference_is_not_extrapolated() {
        let run = Settling::sample(Vec3::X * 225., Vec3::X);
        let walk = Settling::sample(Vec3::Y * 117., Vec3::Y);
        assert!((walk.translation - run.translation * 0.52).length() < 0.00001);
        assert!((walk.weight - 0.52).abs() < 0.00001);
        let fast = Settling::sample(Vec3::X * 450., Vec3::X);
        assert_eq!(fast.rotation, run.rotation);
        assert_eq!(fast.translation, run.translation);
        let transverse = Settling::sample(Vec3::new(117., 200., 0.), Vec3::X);
        assert!((transverse.translation - walk.translation).length() < 0.00001);
    }
}

//! Shared presentation using the accepted Legacy1.38.8.1 M4 profile.
//! Native public parameters were interp0.1/scale1.6. A delayed forward
//! direction translates the model; the camera/model orientation and aim ray
//! remain current. This is not the removed guessed angular low-pass filter.
use std::collections::VecDeque;

const HISTORY_SECONDS: f32 = 0.1;
const SCALE: f32 = 1.6;
const MAX_SAMPLES: usize = 1024;

/// The owner accepted this motion and requested it across the Legacy arsenal.
/// Retain its schema boundary; packs need their own presentation profile.
pub(super) fn shared_profile(schema: Option<&[u8; 20]>, def: &weapons::WeaponDefinition) -> bool {
    const SCHEMA: [u8; 20] = [
        0xc1, 0x36, 0x1d, 0x35, 0x18, 0x02, 0xb7, 0xbe, 0x5f, 0x55,
        0x24, 0x5c, 0x33, 0x1f, 0x5f, 0x1d, 0x6b, 0x36, 0xad, 0x35,
    ];
    let Some(name) = def.class_name.strip_prefix("weapon_") else { return false };
    schema == Some(&SCHEMA)
        && def.id == content::ContentId::csgo(content::ContentKind::Weapon, name)
        && def.view_model.starts_with("models/weapons/")
        && def.view_model.ends_with(".mdl")
}

#[derive(Default)]
pub(super) struct LookMotion {
    samples: VecDeque<(f32, glam::Vec3)>,
}

impl LookMotion {
    pub fn clear(&mut self) { self.samples.clear(); }

    pub fn translation(&mut self, now: f32, forward: glam::Vec3) -> glam::Vec3 {
        if !now.is_finite() || !forward.is_finite() { self.clear(); return glam::Vec3::ZERO; }
        // A clock/domain discontinuity has no observed intermediate camera
        // history. Bootstrap the current direction rather than invent a lag.
        if self.samples.back().is_some_and(|(at, _)| now < *at || now - *at > HISTORY_SECONDS) {
            self.clear();
        }
        if self.samples.is_empty() {
            self.samples.push_back((now - HISTORY_SECONDS, forward));
        }
        if self.samples.back().is_some_and(|(at, _)| *at == now) { self.samples.pop_back(); }
        self.samples.push_back((now, forward));
        let target = now - HISTORY_SECONDS;
        // Keep the sample on either side of the delayed time for interpolation.
        while self.samples.len() > 2 && self.samples[1].0 <= target { self.samples.pop_front(); }
        if self.samples.len() > MAX_SAMPLES { self.clear(); return glam::Vec3::ZERO; }
        let (a, af) = self.samples[0];
        let (b, bf) = self.samples[1];
        let fraction = if b > a { ((target - a) / (b - a)).clamp(0., 1.) } else { 1. };
        let past = af.lerp(bf, fraction);
        let delta = past - forward;
        // The native +/- yaw asymmetry and inward rear-sight movement for
        // BOTH pitch directions admit a positive camera-forward component.
        // Preserve the delayed transverse direction, reflecting only its
        // longitudinal component into the current camera frame.
        (delta - 2. * forward * delta.dot(forward)) * SCALE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn forward(yaw: f32) -> glam::Vec3 { let y = yaw.to_radians(); glam::Vec3::new(y.cos(), y.sin(), 0.) }

    #[test]
    fn observed_step_is_a_short_translation_without_an_angular_decay_tail() {
        let mut motion = LookMotion::default();
        assert_eq!(motion.translation(0., forward(0.)), glam::Vec3::ZERO);
        motion.translation(0.05, forward(0.));
        let current = forward(10.);
        let moved = motion.translation(0.06, current);
        let right = glam::Vec3::new(-current.y, current.x, 0.);
        assert!((moved.dot(right) + 0.2778371).abs() < 0.00001);
        assert!((moved.dot(current) - 0.0243076).abs() < 0.00001);
        let held = motion.translation(0.10, current);
        assert!((held - moved).length() < 0.00001);
        assert!(motion.translation(0.17, current).length() < 0.00001);
    }

    #[test]
    fn opposite_look_steps_both_move_the_model_forward_not_toward_the_camera() {
        // Native rear-sight x moves inward for both pitch signs; yaw steps
        // have smaller right than left screen displacement. A world-space
        // past-minus-current delta has the opposite depth and fails this.
        for pitch in [-10_f32, 10.] {
            let mut m = LookMotion::default(); m.translation(0., glam::Vec3::X);
            let a = pitch.to_radians();
            let current = glam::Vec3::new(a.cos(), 0., -a.sin());
            let translation = m.translation(0.01, current);
            assert!((translation.dot(current) - 0.0243076).abs() < 0.00001);
            assert!((translation.length() - 0.2788984).abs() < 0.00001);
        }
    }

    #[test]
    fn bracketing_history_is_stable_across_render_rates_and_yaw_wrap() {
        let sample = |hz: u32| {
            let mut m = LookMotion::default(); let mut delta = glam::Vec3::ZERO;
            for frame in 0..=hz { let t = frame as f32 / hz as f32; delta = m.translation(t, forward(359. + t * 10.)); }
            delta
        };
        assert!((sample(30) - sample(144)).length() < 0.00002);
        assert!(sample(144).length() < 0.03);
    }

    #[test]
    fn clearing_a_view_domain_and_clock_discontinuities_do_not_replay_old_motion() {
        let mut m = LookMotion::default(); m.translation(1., forward(0.));
        assert!(m.translation(1.01, forward(10.)).length() > 0.2);
        m.clear(); assert_eq!(m.translation(1.02, forward(10.)), glam::Vec3::ZERO);
        assert_eq!(m.translation(0., forward(90.)), glam::Vec3::ZERO);
        assert_eq!(m.translation(5., forward(180.)), glam::Vec3::ZERO);
    }
}

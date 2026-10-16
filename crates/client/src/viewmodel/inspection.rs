//! Restore authored inspection fade-in without changing weapon readiness.
use glam::{Quat, Vec3};

#[derive(Default)]
pub(super) struct Inspection {
    started: Option<f32>,
    duration: f32,
    from: Vec<(Vec3, Quat)>,
}

impl Inspection {
    pub(super) fn begin(
        &mut self,
        started: Option<f32>,
        duration: f32,
        displayed: &[(Vec3, Quat)],
    ) {
        if self.started == started {
            return;
        }
        self.started = started;
        self.from.clear();
        self.duration = if duration.is_finite() && duration > 0. {
            duration
        } else {
            0.
        };
        if started.is_some() && self.duration > 0. {
            self.from.extend_from_slice(displayed);
        }
    }

    /// Masked tracks must also sample from the fixed transition source;
    /// otherwise fractional bone weights feed the last blended frame back in.
    pub(super) fn sample_base(&self, target: &mut [(Vec3, Quat)]) {
        if !self.from.is_empty() && self.from.len() == target.len() {
            target.copy_from_slice(&self.from);
        }
    }

    pub(super) fn apply(&mut self, now: f32, target: &mut [(Vec3, Quat)]) {
        let Some(started) = self.started else { return };
        if self.from.len() != target.len() || self.duration <= 0. {
            return;
        }
        let weight = ((now - started) / self.duration).clamp(0., 1.);
        if weight >= 1. {
            self.from.clear();
            return;
        }
        for (pose, from) in target.iter_mut().zip(&self.from) {
            *pose = (from.0.lerp(pose.0, weight), from.1.slerp(pose.1, weight));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(x: f32) -> [(Vec3, Quat); 1] {
        [(Vec3::X * x, Quat::from_rotation_z(x * 0.1))]
    }

    #[test]
    fn repeated_inspect_starts_at_displayed_pose_and_reaches_authored_target() {
        let mut blend = Inspection::default();
        let displayed = pose(2.);
        blend.begin(Some(1.), 0.3, &displayed);
        let mut target = pose(10.);
        blend.apply(1., &mut target);
        assert_eq!(target, displayed, "new clip cannot snap to its first frame");
        target = pose(10.);
        blend.apply(1.15, &mut target);
        let interrupted = target;
        blend.begin(Some(1.15), 0.3, &interrupted);
        target = pose(-10.);
        blend.apply(1.15, &mut target);
        assert_eq!(
            target, interrupted,
            "spam resumes from the visible blended pose"
        );
        target = pose(-10.);
        blend.apply(1.5, &mut target);
        assert_eq!(target, pose(-10.));
    }

    #[test]
    fn intermediate_render_frames_do_not_change_the_transition() {
        let mut dense = Inspection::default();
        let mut sparse = Inspection::default();
        for blend in [&mut dense, &mut sparse] {
            blend.begin(Some(0.), 0.3, &pose(2.));
        }
        for now in [0.01, 0.02, 0.05, 0.1] {
            dense.begin(Some(0.), 0.3, &pose(99.));
            dense.apply(now, &mut pose(10.));
        }
        let mut a = pose(10.);
        let mut b = a;
        dense.apply(0.15, &mut a);
        sparse.apply(0.15, &mut b);
        assert_eq!(a, b);
        // The production sampler layers fractional masks over its input.
        // Feeding it the previously blended output would depend on FPS even
        // if the outer interpolation used absolute elapsed time.
        dense.sample_base(&mut a);
        sparse.sample_base(&mut b);
        assert_eq!(a, pose(2.));
        assert_eq!(b, pose(2.));
        dense.begin(None, 0.3, &a);
        let mut attack = pose(-5.);
        dense.apply(0.16, &mut attack);
        assert_eq!(
            attack,
            pose(-5.),
            "attack interruption has no inspect pose debt"
        );
    }
}

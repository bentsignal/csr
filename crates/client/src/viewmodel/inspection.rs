//! Restore authored inspection fade-in without changing weapon readiness.
use glam::{Quat, Vec3};
use std::sync::Arc;

/// The attack phase that was actually displayed before quick inspection.
/// Continue that authored motion during the fade instead of holding the hand
/// still at an arbitrary point in the slash and then pulling it toward inspect.
#[derive(Clone)]
pub(super) struct Playback {
    pub clip: Arc<mdl::anim::Clip>,
    pub weights: Arc<Vec<f32>>,
    pub time: f32,
}

#[derive(Default)]
pub(super) struct Inspection {
    started: Option<f32>,
    duration: f32,
    from: Vec<(Vec3, Quat)>,
    outgoing: Option<Playback>,
    serial: u64,
    activity: String,
}

impl Inspection {
    pub(super) fn begin(
        &mut self,
        started: Option<f32>,
        duration: f32,
        displayed: &[(Vec3, Quat)],
        outgoing: Option<Playback>,
        serial: u64,
        activity: &str,
    ) {
        // Render time can be identical for distinct input edges. Inspection,
        // cancellation and an incoming reload must still capture a new pose.
        if self.started == started && self.serial == serial && self.activity == activity {
            return;
        }
        self.started = started;
        self.serial = serial;
        self.activity.clear();
        self.activity.push_str(activity);
        self.from.clear();
        self.outgoing = outgoing;
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
        if weight <= 0. {
            // Slerp's arithmetic can round its nominal zero-weight endpoint.
            // An accepted restart must retain the exact displayed pose.
            target.copy_from_slice(&self.from);
            return;
        }
        if weight >= 1. {
            self.from.clear();
            self.outgoing = None;
            return;
        }
        let mut moving;
        let from = if let Some(outgoing) = &self.outgoing {
            moving = self.from.clone();
            super::sequence_pose::sample(&outgoing.clip,
                outgoing.time + (now - started).max(0.), &mut moving,
                |b| outgoing.weights.get(b).copied().unwrap_or(1.));
            &moving
        } else { &self.from };
        for (pose, from) in target.iter_mut().zip(from) {
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
        blend.begin(Some(1.), 0.3, &displayed, None, 1, "INSPECT");
        let mut target = pose(10.);
        blend.apply(1., &mut target);
        assert_eq!(target, displayed, "new clip cannot snap to its first frame");
        target = pose(10.);
        blend.apply(1.15, &mut target);
        let interrupted = target;
        blend.begin(Some(1.15), 0.3, &interrupted, None, 2, "INSPECT");
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
        let clip = mdl::anim::Clip {
            name: "fractionally masked inspect".into(),
            fps: 5.,
            looping: false,
            delta: false,
            frames: vec![pose(10.).to_vec(), pose(20.).to_vec()],
            animated: vec![true],
        };
        let mut dense = Inspection::default();
        let mut sparse = Inspection::default();
        for blend in [&mut dense, &mut sparse] {
            blend.begin(Some(0.), 0.3, &pose(2.), None, 1, "INSPECT");
        }
        let mut a = pose(2.);
        let mut b = a;
        for now in [0.01, 0.02, 0.05, 0.1] {
            dense.begin(Some(0.), 0.3, &a, None, 1, "INSPECT");
            dense.sample_base(&mut a);
            clip.sample_layered(now, &mut a, |_| 0.5);
            dense.apply(now, &mut a);
        }
        dense.sample_base(&mut a);
        sparse.sample_base(&mut b);
        clip.sample_layered(0.15, &mut a, |_| 0.5);
        clip.sample_layered(0.15, &mut b, |_| 0.5);
        dense.apply(0.15, &mut a);
        sparse.apply(0.15, &mut b);
        assert_eq!(a, b);
        dense.begin(None, 0.3, &a, None, 1, "ACT_VM_PRIMARYATTACK");
        let mut attack = pose(-5.);
        dense.apply(0.16, &mut attack);
        assert_eq!(
            attack,
            pose(-5.),
            "attack interruption has no inspect pose debt"
        );
    }

    #[test]
    fn continuing_slash_is_independent_of_intermediate_frames_and_clears_on_cancel() {
        let outgoing = Playback {
            clip: Arc::new(mdl::anim::Clip {
                name: "continuing slash".into(), fps: 1., looping: false, delta: false,
                frames: vec![pose(2.).to_vec(), pose(6.).to_vec()], animated: vec![true],
            }),
            weights: Arc::new(vec![1.]), time: 0.,
        };
        let mut dense = Inspection::default();
        let mut sparse = Inspection::default();
        for blend in [&mut dense, &mut sparse] {
            blend.begin(Some(0.), 0.3, &pose(2.), Some(outgoing.clone()), 1, "INSPECT");
        }
        for now in [0.01, 0.02, 0.05, 0.1] {
            let mut target = pose(10.);
            dense.apply(now, &mut target);
        }
        let mut a = pose(10.);
        let mut b = a;
        dense.apply(0.15, &mut a);
        sparse.apply(0.15, &mut b);
        assert_eq!(a, b);
        assert!(a[0].0.x > 6., "a frozen slash would blend to exactly six");
        dense.begin(Some(0.15), 0.4, &a, None, 2, "INSPECT_CANCEL");
        let mut cancel = pose(-5.);
        dense.apply(0.15, &mut cancel);
        assert_eq!(cancel, a, "cancel must start from the visible blended pose");
        dense.apply(0.55, &mut cancel);
        assert!(dense.outgoing.is_none());
        dense.begin(None, 0., &cancel, None, 2, "ACT_VM_PRIMARYATTACK");
        let mut next_attack = pose(-10.);
        dense.apply(0.6, &mut next_attack);
        assert_eq!(next_attack, pose(-10.));
    }

    #[test]
    fn distinct_edges_at_the_same_render_time_capture_a_new_pose() {
        let mut blend = Inspection::default();
        blend.begin(Some(1.), 0.3, &pose(2.), None, 1, "INSPECT");
        blend.begin(Some(1.), 0.3, &pose(3.), None, 2, "INSPECT");
        let mut target = pose(10.);
        blend.apply(1., &mut target);
        assert_eq!(target, pose(3.));
        blend.begin(Some(1.), 0.2, &pose(4.), None, 2, "ACT_VM_RELOAD");
        blend.apply(1., &mut target);
        assert_eq!(target, pose(4.));
    }
}

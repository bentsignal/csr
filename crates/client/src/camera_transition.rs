//! Presentation-only camera travel. Reversing a switch starts from the last
//! rendered frame; following the actor's anchor avoids dragging behind motion.
use bevy::math::{Quat, Vec3};
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub position: Vec3,
    pub rotation: Quat,
    pub fov: f32,
}
#[derive(Default)]
pub struct Transition {
    cut: bool,
    subject: Option<(u32, u64)>,
    mode: bool,
    elapsed: f32,
    from: Option<Frame>,
    last: Option<Frame>,
    from_anchor: Vec3,
    anchor: Vec3,
}
const DURATION: f32 = 0.45;
impl Transition {
    pub fn active(&self) -> bool {
        self.from.is_some() && self.elapsed < DURATION
    }
    pub fn frame(&self) -> Option<Frame> {
        self.last
    }
    pub fn constrain_position(&mut self, position: Vec3) {
        if let Some(frame) = &mut self.last {
            frame.position = position;
        }
    }
    pub fn sample(
        &mut self,
        subject: (u32, u64),
        mode: bool,
        anchor: Vec3,
        target: Frame,
        dt: f32,
        cut: bool,
    ) -> Frame {
        let was_cut = self.cut;
        self.cut = cut;
        if cut
            || was_cut
            || self.subject != Some(subject)
            || self.last.is_none()
            || anchor.distance_squared(self.anchor) > 512f32.powi(2)
        {
            self.subject = Some(subject);
            self.mode = mode;
            self.from = None;
            self.last = Some(target);
            self.anchor = anchor;
            return target;
        }
        if self.mode != mode {
            self.from = self.last;
            self.from_anchor = self.anchor;
            self.elapsed = 0.;
            self.mode = mode;
        }
        let result = if let Some(from) = self.from.filter(|_| self.elapsed < DURATION) {
            let t = (self.elapsed / DURATION).clamp(0., 1.);
            let t = t * t * (3. - 2. * t);
            Frame {
                position: (from.position + anchor - self.from_anchor).lerp(target.position, t),
                rotation: from.rotation.slerp(target.rotation, t),
                fov: from.fov + (target.fov - from.fov) * t,
            }
        } else {
            self.from = None;
            target
        };
        self.elapsed += dt.max(0.);
        self.anchor = anchor;
        self.last = Some(result);
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn frame(x: f32) -> Frame {
        Frame {
            position: Vec3::X * x,
            rotation: Quat::from_rotation_y(x / 200.),
            fov: 1. + x / 400.,
        }
    }
    #[test]
    fn entry_exit_and_interrupted_switches_stay_continuous() {
        let mut t = Transition::default();
        let key = (1, 1);
        t.sample(key, false, Vec3::ZERO, frame(0.), 0., false);
        assert_eq!(
            t.sample(key, true, Vec3::ZERO, frame(100.), 0.2, false)
                .position,
            Vec3::ZERO
        );
        let middle = t.sample(key, true, Vec3::ZERO, frame(100.), 0.01, false);
        assert!(middle.position.x > 0. && middle.position.x < 100.);
        let reversed = t.sample(key, false, Vec3::ZERO, frame(0.), 0., false);
        assert_eq!(reversed.position, middle.position);
        assert!(reversed.rotation.dot(middle.rotation) > 0.99999);
        for _ in 0..100 {
            t.sample(key, false, Vec3::ZERO, frame(0.), 0.01, false);
        }
        assert_eq!(t.frame().unwrap().position, Vec3::ZERO);
    }
    #[test]
    fn actor_motion_follows_the_blend_and_new_lives_cut() {
        let mut t = Transition::default();
        t.sample((1, 1), false, Vec3::ZERO, frame(0.), 0., false);
        t.sample((1, 1), true, Vec3::ZERO, frame(100.), 0., false);
        assert_eq!(
            t.sample((1, 1), true, Vec3::X * 20., frame(120.), 0.1, false)
                .position,
            Vec3::X * 20.
        );
        assert_eq!(
            t.sample((1, 2), true, Vec3::ZERO, frame(100.), 0., false)
                .position,
            Vec3::X * 100.
        );
        t.sample((1, 2), false, Vec3::ZERO, frame(1000.), 0., true);
        assert_eq!(
            t.sample((1, 2), true, Vec3::ZERO, frame(100.), 0., false)
                .position,
            Vec3::X * 100.
        );
    }
}

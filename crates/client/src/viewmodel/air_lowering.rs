//! Bounded vertical component observed in stationary Legacy jumps.
//! Separate from accuracy tilt, camera landing kick and periodic running bob.
//! See docs/research/airborne-viewmodel-lowering.md for the reference boundary.

const DROP: f32 = 0.75;
// The first airborne sample is about 0.1 unit below rest and reaches the
// plateau within the following 0.1-second capture interval. This bounded
// transition is a fitted presentation component, not a native tick-law claim.
// Keep its travel time and endpoints, easing the ramp so reaching/leaving
// either held pose does not instantly switch vertical presentation velocity.
// RATE advances the underlying ramp; it is not the eased peak velocity.
const RATE: f32 = 10.0;

#[derive(Default)]
pub(super) struct AirLowering {
    at: Option<f32>,
    progress: f32,
}

impl AirLowering {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Sample every visible viewing-life frame, including a hidden replacement
    /// frame. Vertical velocity changing sign at the apex is not a landing.
    pub fn sample(&mut self, now: f32, airborne: bool) -> f32 {
        if !now.is_finite() {
            self.clear();
            return 0.;
        }
        let previous = self.at.replace(now);
        let Some(previous) = previous else { return 0. };
        let dt = now - previous;
        if !(0. ..=0.25).contains(&dt) {
            self.progress = 0.;
            return 0.;
        }
        let target = if airborne { 1. } else { 0. };
        let step = RATE * dt / DROP;
        self.progress += (target - self.progress).clamp(-step, step);
        // Ease the existing bounded trajectory, without a spring, overshoot
        // or another decay tail. Reversal retains the current progress.
        let t = self.progress;
        -DROP * t * t * (3. - 2. * t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_air_plateau_survives_the_apex_and_returns_without_overshoot() {
        let mut response = AirLowering::default();
        assert_eq!(response.sample(0., false), 0.);
        let edge = 0.001;
        let duration = DROP / RATE;
        let first = response.sample(edge, true);
        assert!(first < 0. && first > -RATE * edge * 0.2,
            "takeoff must ease out of rest instead of starting at full speed");
        let last = response.sample(duration - edge, true);
        assert!(last > -DROP && last < -DROP + RATE * edge * 0.2,
            "approaching the flight plateau must slow before stopping");
        assert!((response.sample(duration, true) + DROP).abs() < 0.000001);
        for i in 1..=20 {
            let offset = response.sample(duration + i as f32 * 0.005, true);
            assert!((-DROP..=0.).contains(&offset));
        }
        assert_eq!(response.sample(0.21, true), -DROP);
        assert_eq!(response.sample(0.22, true), -DROP);
        let first = response.sample(0.22 + edge, false);
        assert!(first > -DROP && first < -DROP + RATE * edge * 0.2,
            "landing must ease out of the flight plateau");
        let last = response.sample(0.22 + duration - edge, false);
        assert!(last < 0. && last > -RATE * edge * 0.2,
            "returning to rest must slow before stopping");
        assert!(response.sample(0.22 + duration, false).abs() < 0.000001);
        for i in 1..=20 {
            let offset = response.sample(0.22 + duration + i as f32 * 0.005, false);
            assert!((-DROP..=0.).contains(&offset));
        }
        assert_eq!(response.sample(0.43, false), 0.);
    }

    #[test]
    fn same_elapsed_time_has_the_same_offset_at_different_render_rates() {
        let evaluate = |step: f32| {
            let mut response = AirLowering::default();
            response.sample(0., false);
            let mut time = 0.;
            while time + step < 0.05 {
                time += step;
                response.sample(time, true);
            }
            response.sample(0.05, true)
        };
        let reference = evaluate(0.05);
        for step in [1. / 30., 1. / 60., 1. / 144., 1. / 300.] {
            assert!((evaluate(step) - reference).abs() < 0.00001);
        }
    }

    #[test]
    fn missing_clock_history_and_new_lives_do_not_reuse_an_old_drop() {
        let mut response = AirLowering::default();
        response.sample(1., false);
        assert_eq!(response.sample(1.1, true), -DROP);
        assert_eq!(response.sample(0.5, true), 0.);
        response.sample(0.6, true);
        assert_eq!(response.sample(2., true), 0.);
        response.sample(2.1, true);
        assert_eq!(response.sample(f32::NAN, true), 0.);
        assert_eq!(response.sample(3., true), 0.);
        response.sample(3.1, true);
        response.clear();
        assert_eq!(response.sample(3.2, false), 0.);
    }
}

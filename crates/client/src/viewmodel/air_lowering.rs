//! Bounded vertical component observed in stationary Legacy jumps.
//! Separate from accuracy tilt, camera landing kick and periodic running bob.
//! See docs/research/airborne-viewmodel-lowering.md for the reference boundary.

const DROP: f32 = 0.75;
// The first airborne sample is about 0.1 unit below rest and reaches the
// plateau within the following 0.1-second capture interval. This bounded
// transition is a fitted presentation component, not a native tick-law claim.
const RATE: f32 = 10.0;

#[derive(Default)]
pub(super) struct AirLowering {
    at: Option<f32>,
    offset: f32,
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
            self.offset = 0.;
            return 0.;
        }
        let target = if airborne { -DROP } else { 0. };
        self.offset += (target - self.offset).clamp(-RATE * dt, RATE * dt);
        self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_air_plateau_survives_the_apex_and_returns_without_overshoot() {
        let mut response = AirLowering::default();
        assert_eq!(response.sample(0., false), 0.);
        for i in 1..=20 {
            let offset = response.sample(i as f32 * 0.01, true);
            assert!((-DROP..=0.).contains(&offset));
        }
        assert_eq!(response.sample(0.21, true), -DROP);
        assert_eq!(response.sample(0.22, true), -DROP);
        for i in 1..=20 {
            let offset = response.sample(0.22 + i as f32 * 0.01, false);
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
        for step in [1. / 30., 1. / 60., 1. / 144., 1. / 300.] {
            assert!((evaluate(step) + 0.5).abs() < 0.00001);
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

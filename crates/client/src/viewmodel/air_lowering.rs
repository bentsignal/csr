//! Bounded vertical component observed in stationary Legacy jumps.
//! Separate from accuracy tilt, camera landing kick and periodic running bob.
//! See docs/research/airborne-viewmodel-lowering.md for the reference boundary.

const DROP: f32 = 0.75;
// The first airborne sample is about 0.1 unit below rest and reaches the
// plateau within the following 0.1-second capture interval. This bounded
// transition is a fitted presentation component, not a native tick-law claim.
// Keep its travel time and endpoints, easing the ramp so reaching/leaving
// either held pose does not instantly switch vertical presentation velocity.
// RATE sets the full travel time; it is not the eased peak velocity.
const RATE: f32 = 10.0;

#[derive(Default)]
pub(super) struct AirLowering {
    at: Option<f32>,
    progress: f32,
    velocity: f32,
    transition: Option<Transition>,
}

struct Transition {
    at: f32,
    from: f32,
    control: f32,
    target: f32,
    duration: f32,
}

impl Transition {
    fn new(at: f32, from: f32, velocity: f32, target: f32) -> Self {
        let mut transition = Self { at, from, control: from, target, duration: DROP / RATE };
        // Retain the full travel time unless the actual curve would leave
        // the observed bounds. A control outside those bounds alone does NOT
        // imply overshoot: shortening for that would rush a near-complete
        // drop all the way back up in a single frame.
        loop {
            transition.control = from + velocity * transition.duration / 3.;
            if transition.bounded() { return transition; }
            transition.duration *= 0.5;
        }
    }

    fn position(&self, t: f32) -> f32 {
        let u = 1. - t;
        u * u * u * self.from + 3. * u * u * t * self.control
            + t * t * (3. * u + t) * self.target
    }

    fn bounded(&self) -> bool {
        // The derivative factors as (1-t) times a linear expression because
        // the last two controls coincide. Only its other root can overshoot.
        let bend = self.from - 3. * self.control + 2. * self.target;
        if bend.abs() < f32::EPSILON { return true; }
        let extremum = (self.from - self.control) / bend;
        !(0. ..1.).contains(&extremum) || (0. ..=1.).contains(&self.position(extremum))
    }

    fn sample(&self, now: f32) -> (f32, f32) {
        if self.duration <= 0. || now >= self.at + self.duration {
            return (self.target, 0.);
        }
        let t = ((now - self.at) / self.duration).clamp(0., 1.);
        let u = 1. - t;
        let velocity = 3. / self.duration * (u * u * (self.control - self.from)
            + 2. * u * t * (self.target - self.control));
        (self.position(t).clamp(0., 1.), velocity)
    }
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
            self.clear();
            self.at = Some(now);
            return 0.;
        }
        let target = if airborne { 1. } else { 0. };
        if self.transition.as_ref().map_or(0., |transition| transition.target) != target {
            // As before, a newly observed edge applies to this frame interval.
            // Retarget from the last displayed position AND velocity instead
            // of retracing smoothstep, which instantly reverses mid-flight.
            self.transition = Some(Transition::new(previous, self.progress, self.velocity, target));
        }
        if let Some(transition) = &self.transition {
            (self.progress, self.velocity) = transition.sample(now);
        }
        -DROP * self.progress
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
    fn interrupted_hops_preserve_velocity_and_remain_bounded() {
        // A short hop can land before the drop ends; another jump can also
        // interrupt the return. Neither edge may instantly reverse velocity.
        for first_target in [0., 1.] {
            let from = 1. - first_target;
            let first = Transition::new(0., from, 0., first_target);
            for fraction in [0.01, 0.25, 0.5, 0.75, 0.99] {
                let at = first.duration * fraction;
                let (position, velocity) = first.sample(at);
                let reversal = Transition::new(at, position, velocity, from);
                assert_eq!(reversal.duration, DROP / RATE,
                    "a near-complete drop must not rush its entire return");
                let (next_position, next_velocity) = reversal.sample(at);
                assert!((next_position - position).abs() < 0.000001);
                assert!((next_velocity - velocity).abs() < 0.0001);
                for i in 0..=100 {
                    let (position, _) = reversal.sample(at + reversal.duration * i as f32 / 100.);
                    assert!((0. ..=1.).contains(&position));
                }
                assert_eq!(reversal.sample(at + reversal.duration), (from, 0.));
            }
        }
    }

    #[test]
    fn sampled_short_hop_and_rejump_do_not_reverse_in_one_frame() {
        for initially_airborne in [false, true] {
            let mut response = AirLowering::default();
            response.sample(0., false);
            response.sample(0.1, initially_airborne);
            let before = response.sample(0.13, !initially_airborne);
            let after = response.sample(0.131, initially_airborne);
            if initially_airborne {
                assert!(after > before, "re-jumping must retain the upward return velocity initially");
            } else {
                assert!(after < before, "a short hop must retain the downward velocity initially");
            }
            let target = if initially_airborne { -DROP } else { 0. };
            assert_eq!(response.sample(0.22, initially_airborne), target);
        }
    }

    #[test]
    fn repeated_contact_edges_and_render_substeps_follow_the_same_curve() {
        let evaluate = |step: f32| {
            let mut response = AirLowering::default();
            response.sample(0., false);
            let mut previous = 0.;
            for (end, airborne) in [(0.03, true), (0.045, false), (0.06, true)] {
                // Explicit edge at the last displayed instant; following
                // subdivisions may not restart or retrace the transition.
                response.sample(previous, airborne);
                let mut now = previous + step;
                while now < end {
                    response.sample(now, airborne);
                    now += step;
                }
                response.sample(end, airborne);
                previous = end;
            }
            response.sample(0.08, true)
        };
        let reference = evaluate(0.1);
        for step in [1. / 60., 1. / 144., 1. / 300.] {
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

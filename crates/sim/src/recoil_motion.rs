//! Recoil state inferred from public, separately observed angle/velocity
//! trajectories. This is not an implementation taken from a native engine.
use serde::{Deserialize, Serialize};
use weapons::{RecoilResponse, RecoilTrajectory};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecoilMotion {
    pub angle: [f32; 2],
    pub velocity: [f32; 2],
    pub updated_tick: u64,
    /// Reconstructs the same held-shot cadence after a snapshot/cache miss.
    pub anchor_tick: u64,
    pub tick_rate: u32,
}

impl RecoilMotion {
    pub(crate) fn new(tick: u64, tick_rate: u32) -> Self {
        Self {
            angle: [0.; 2],
            velocity: [0.; 2],
            updated_tick: tick,
            anchor_tick: tick,
            tick_rate,
        }
    }

    pub(crate) fn advance(&mut self, response: &RecoilResponse, tick: u64) {
        let elapsed = tick.saturating_sub(self.updated_tick) as f64 / self.tick_rate as f64;
        advance(&mut self.angle, &mut self.velocity, response, elapsed);
        self.updated_tick = tick;
    }
}

pub(crate) fn valid(profile: &RecoilTrajectory) -> bool {
    let Some(r) = &profile.response else {
        return false;
    };
    (2..=256).contains(&profile.bullet_angles.len())
        && (32..=512).contains(&r.reference_hz)
        && [r.angle_decay, r.angle_linear, r.velocity_decay]
            .iter()
            .all(|x| x.is_finite() && *x > 0. && *x <= 100.)
        && r.first_impulse
            .iter()
            .chain(&r.final_impulse)
            .all(|x| x.is_finite() && x.abs() <= 10_000.)
        && r.held_tolerance_degrees.is_finite()
        && (0. ..=1.).contains(&r.held_tolerance_degrees)
        && profile
            .bullet_angles
            .iter()
            .flatten()
            .all(|x| x.is_finite())
}

/// The order is constrained by the observed curves: radial angle shrink,
/// then velocity contribution, then velocity decay. In particular angle
/// can reach its displayed zero while velocity remains nonzero.
pub(crate) fn advance(
    angle: &mut [f32; 2],
    velocity: &mut [f32; 2],
    r: &RecoilResponse,
    mut seconds: f64,
) {
    let reference_step = 1. / r.reference_hz as f64;
    while seconds > 1e-10 {
        let dt = seconds.min(reference_step) as f32;
        let before = (*angle, *velocity);
        let length = angle[0].hypot(angle[1]);
        let shrink = if length > 0. {
            (1. - r.angle_decay * dt - r.angle_linear * dt / length).max(0.)
        } else {
            0.
        };
        let decay = (-r.velocity_decay * dt).exp();
        for axis in 0..2 {
            angle[axis] = angle[axis] * shrink + velocity[axis] * dt;
            velocity[axis] *= decay;
        }
        seconds -= dt as f64;
        // f32 subnormals can become fixed points. Avoid an unbounded catch-up
        // loop for a long-idle dropped weapon without inventing a reset time.
        if before == (*angle, *velocity) {
            break;
        }
    }
}

/// Recover the velocity that produces a measured subsequent held ray.
/// No recovery coefficients are fitted here: this only inverts the selected
/// content profile's declared response at the actual scheduled shot interval.
pub(crate) fn velocity_for_target(
    angle: [f32; 2],
    mut velocity: [f32; 2],
    target: [f32; 2],
    r: &RecoilResponse,
    seconds: f64,
) -> Option<[f32; 2]> {
    let sample = |v| {
        let mut a = angle;
        let mut v = v;
        advance(&mut a, &mut v, r, seconds);
        a
    };
    for _ in 0..24 {
        let got = sample(velocity);
        let error = [target[0] - got[0], target[1] - got[1]];
        if error[0].hypot(error[1]) < 0.000002 {
            return Some(velocity);
        }
        let mut columns = [[0.; 2]; 2];
        for axis in 0..2 {
            let mut hi = velocity;
            let mut lo = velocity;
            hi[axis] += 0.05;
            lo[axis] -= 0.05;
            let (hi, lo) = (sample(hi), sample(lo));
            columns[axis] = [(hi[0] - lo[0]) / 0.1, (hi[1] - lo[1]) / 0.1];
        }
        let [[a, c], [b, d]] = columns;
        let determinant = a * d - b * c;
        if !determinant.is_finite() || determinant.abs() < 1e-9 {
            return None;
        }
        velocity[0] += (d * error[0] - b * error[1]) / determinant;
        velocity[1] += (a * error[1] - c * error[0]) / determinant;
        if velocity.iter().any(|x| !x.is_finite() || x.abs() > 10_000.) {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_public_angle_curves_constrain_response_order() {
        let response = RecoilResponse {
            reference: "public-legacy-1.38.8.1-decay".into(),
            reference_hz: 128,
            angle_decay: 8.,
            angle_linear: 18.,
            velocity_decay: 4.5,
            first_impulse: [0.; 2],
            final_impulse: [0.; 2],
            held_tolerance_degrees: 0.2,
        };
        // Readable public values from three release-only observations, not
        // restart rays used to tune a family. Relative update counts align
        // the separately displayed velocity envelope; captured receive-time
        // bounds do not identify an exact server tick or absolute phase.
        let curves: &[([f32; 2], [f32; 2], f32, &[(u32, [f32; 2])])] = &[
            (
                [-0.125, -0.0625],
                [-19.28125, -9.96875],
                1. / 32.,
                &[
                    (4, [-0.15625, -0.0625]),
                    (5, [-0.15625, -0.0625]),
                    (8, [-0.125, -0.0625]),
                    (14, [-0.09375, -0.03125]),
                    (19, [-0.0625, -0.03125]),
                    (25, [-0.0625, -0.03125]),
                    (39, [-0.03125, 0.]),
                    (64, [0., 0.]),
                    (97, [0., 0.]),
                    (132, [0., 0.]),
                ],
            ),
            (
                [-3.4375, 0.3125],
                [-62.28125, 7.9375],
                1. / 32.,
                &[
                    (1, [-3.5625, 0.3125]),
                    (5, [-3.84375, 0.40625]),
                    (9, [-3.875, 0.4375]),
                    (14, [-3.65625, 0.40625]),
                    (18, [-3.34375, 0.40625]),
                    (25, [-2.65625, 0.3125]),
                    (39, [-1.25, 0.15625]),
                    (62, [-0.03125, 0.]),
                    (96, [0., 0.]),
                    (127, [0., 0.]),
                ],
            ),
            (
                [-5.3125, -1.6875],
                [-70.40625, -13.03125],
                // Both the initial and final displayed state are quantized.
                // The independent full-magazine tail has a measured maximum
                // discrepancy of 0.03434°, not exact one-increment agreement.
                2. / 32.,
                &[
                    (1, [-5.40625, -1.625]),
                    (6, [-5.5, -1.4375]),
                    (9, [-5.375, -1.3125]),
                    (12, [-5.125, -1.1875]),
                    (17, [-4.59375, -1.]),
                    (24, [-3.71875, -0.75]),
                    (38, [-1.9375, -0.375]),
                    (62, [-0.0625, 0.]),
                    (94, [0., 0.]),
                    (128, [0., 0.]),
                ],
            ),
        ];
        for (initial_angle, initial_velocity, tolerance, points) in curves {
            for (updates, expected) in *points {
                let (mut angle, mut velocity) = (*initial_angle, *initial_velocity);
                advance(&mut angle, &mut velocity, &response, *updates as f64 / 128.);
                assert!(
                    angle
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| (*a - *b).abs() <= *tolerance),
                    "observed response misses display precision after {updates} updates: {angle:?} vs {expected:?}"
                );
            }
        }
    }
}

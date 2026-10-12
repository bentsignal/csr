//! Server policy for historical view requests, in server ticks.
#[derive(Clone, Copy, Debug)]
pub struct LagPolicy {
    pub max_history_ticks: u32,
    pub interpolation_allowance_ticks: u32,
    pub teleport_distance: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn view_bounds_use_server_latency_and_never_exceed_the_cap() {
        let p = LagPolicy::prototype(128).validate(128).unwrap();
        assert_eq!(p.accepted_tick(200, Some(72), 192, 128, 0.), Some(192));
        assert_eq!(p.accepted_tick(200, Some(72), 191, 128, 0.), None);
        assert_eq!(p.accepted_tick(200, Some(72), 180, 128, 0.25), Some(180));
        assert_eq!(p.accepted_tick(200, Some(72), 72, 128, 1e100), Some(72));
        assert_eq!(p.accepted_tick(200, Some(71), 71, 128, 1e100), None);
        assert_eq!(p.accepted_tick(200, Some(72), 201, 128, 0.25), None);
        assert_eq!(p.accepted_tick(200, Some(181), 180, 128, 0.25), None);
        for invalid in [f64::NAN, f64::INFINITY, -1.] {
            assert_eq!(p.accepted_tick(200, Some(72), 180, 128, invalid), None);
        }
    }
    #[test]
    fn policy_units_scale_by_server_rate_and_invalid_caps_are_rejected() {
        for rate in [64, 128] {
            let p = LagPolicy::prototype(rate).validate(rate).unwrap();
            assert_eq!(p.max_history_ticks, rate);
            assert_eq!(p.interpolation_allowance_ticks, rate / 16);
            assert!(
                LagPolicy {
                    max_history_ticks: rate + 1,
                    ..p
                }
                .validate(rate)
                .is_err()
            );
            assert!(
                LagPolicy {
                    interpolation_allowance_ticks: rate + 1,
                    ..p
                }
                .validate(rate)
                .is_err()
            );
        }
    }
}
impl LagPolicy {
    /// Prototype policy: one second maximum, two snapshot intervals of view
    /// delay. This is not a measured CS:GO Legacy interpolation default.
    pub fn prototype(rate: u32) -> Self {
        Self {
            max_history_ticks: rate,
            interpolation_allowance_ticks: (rate / 32).max(1) * 2,
            teleport_distance: 128.,
        }
    }
    pub fn validate(self, rate: u32) -> Result<Self, String> {
        if !sim::TICK_RATE_RANGE.contains(&rate)
            || self.max_history_ticks == 0
            || self.max_history_ticks > rate
            || self.interpolation_allowance_ticks > self.max_history_ticks
            || !self.teleport_distance.is_finite()
            || self.teleport_distance <= 0.
            || self.teleport_distance > 1e6
        {
            return Err("invalid lag compensation policy".into());
        }
        Ok(self)
    }
    pub(crate) fn accepted_tick(
        self,
        now: u64,
        oldest: Option<u64>,
        requested: u64,
        rate: u32,
        rtt_seconds: f64,
    ) -> Option<u64> {
        let age = now.checked_sub(requested)?;
        if requested < oldest? {
            return None;
        }
        let latency = if rtt_seconds.is_finite() && rtt_seconds >= 0. {
            (rtt_seconds.min(2.) * rate as f64 / 2.).ceil() as u64
        } else {
            0
        };
        let allowance = latency
            .saturating_add(self.interpolation_allowance_ticks as u64)
            .min(self.max_history_ticks as u64);
        (age <= allowance).then_some(requested)
    }
}

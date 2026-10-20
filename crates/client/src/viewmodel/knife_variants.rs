//! Local cosmetic selection from the installed studio activity weights.
//! Named inspections without an activity keep their first-sequence fallback.
use super::SeqClip;

pub(super) struct KnifeVariants {
    random: u64,
}

impl Default for KnifeVariants {
    fn default() -> Self {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos() as u64);
        Self { random: seed }
    }
}

impl KnifeVariants {
    #[cfg(test)]
    pub(super) fn seeded(random: u64) -> Self { Self { random } }

    /// Call only for a new Draw or accepted inspect, never each render frame.
    pub(super) fn choose(&mut self, clips: &[SeqClip]) -> usize {
        if clips.len() <= 1 { return 0; }
        let total: u64 = clips.iter().map(|clip| clip.activity_weight.max(0) as u64).sum();
        if total == 0 { return 0; }
        // Same SplitMix64 cosmetic generator used by impact decals; it never
        // consumes simulation randomness or changes readiness/attack input.
        self.random = self.random.wrapping_add(0x9e3779b97f4a7c15);
        let mut random = self.random;
        random = (random ^ (random >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        random = (random ^ (random >> 27)).wrapping_mul(0x94d049bb133111eb);
        let random = random ^ (random >> 31);
        let ticket = ((random as u128 * total as u128) >> 64) as u64;
        weighted_index(clips.iter().map(|clip| clip.activity_weight), ticket)
    }
}

fn weighted_index(weights: impl IntoIterator<Item = i32>, mut ticket: u64) -> usize {
    for (index, weight) in weights.into_iter().enumerate() {
        let weight = weight.max(0) as u64;
        if ticket < weight { return index; }
        ticket = ticket.saturating_sub(weight);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    // UNRUN: installed declarations, not claims about native RNG implementation.
    #[test]
    fn authored_variant_intervals_preserve_weights_and_named_fallback() {
        assert_eq!((0..5).map(|ticket| weighted_index([1, 4], ticket)).collect::<Vec<_>>(),
            [0, 1, 1, 1, 1]);
        assert_eq!((0..3).map(|ticket| weighted_index([1, 1, 1], ticket)).collect::<Vec<_>>(),
            [0, 1, 2]);
        assert!((0..100).all(|ticket| weighted_index([100, 1], ticket) == 0));
        assert_eq!(weighted_index([100, 1], 100), 1);
        assert_eq!(weighted_index([0], 0), 0);
    }
}

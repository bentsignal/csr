//! Empirical AK/AWP decoy timing candidate, not a recovered native random law.
//! Gaps are native printed sound-time differences from owned Legacy packets;
//! see docs/research/decoy-weapon-identity.md for provenance and limitations.
use super::Field;

pub(super) const LIFETIME_SECONDS: f32 = 15.;

const AK_GAPS_MS: &[u32] = &[
    1060, 135, 515, 4027, 169, 311, 1398, 3673, 1646, 138, 1910, 528, 340,
];
const AWP_GAPS_MS: &[u32] = &[1777, 5217, 1859, 1464, 2382, 1758, 1638];

/// Derive the schedule from replicated field identity/birth, without consuming
/// shared gameplay RNG or adding an unreplicated client timer. Unknown weapons
/// retain the existing schedule; observations of two guns do not calibrate all.
pub(super) fn shot_due(field: &Field, tick: u64, rate: u32) -> bool {
    let gaps = match field.decoy_weapon.as_deref() {
        Some("csgo_weapon_ak47") => AK_GAPS_MS,
        Some("csgo_weapon_awp") => AWP_GAPS_MS,
        _ => return tick % (rate as u64 / 2).max(1) == 0,
    };
    let born_tick = (field.born as f64 * rate as f64).round() as u64;
    let Some(age) = tick.checked_sub(born_tick) else { return false; };
    // The unchanged field lifetime also bounds schedule reconstruction, even
    // if a peer supplies an implausibly old birth in a still-live field.
    if age >= (LIFETIME_SECONDS * rate as f32) as u64 { return false; }
    // Retain the previous half-second scale for the unmeasured first-shot
    // delay, now relative to activation. Only subsequent gaps are observations.
    let mut elapsed_ms = 500_u64;
    let mut random = field.id ^ field.owner.rotate_left(16);
    loop {
        let deadline = (elapsed_ms * rate as u64).div_ceil(1000);
        if deadline >= age { return deadline == age; }
        // Deterministic empirical resampling is an explicit approximation:
        // it preserves observed short taps/long pauses, not native correlation,
        // burst probabilities or the original PRNG/distribution.
        random = random.wrapping_add(0x9e3779b9);
        let mut sample = random;
        sample = (sample ^ (sample >> 16)).wrapping_mul(0x21f0aaad);
        sample = (sample ^ (sample >> 15)).wrapping_mul(0x735a2d97);
        sample ^= sample >> 15;
        elapsed_ms += gaps[sample as usize % gaps.len()] as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utility::Kind;

    #[test]
    fn observed_weapon_schedules_have_varied_gaps_and_independent_field_identity() {
        for rate in [64, 128] {
            for weapon in ["csgo_weapon_ak47", "csgo_weapon_awp"] {
                let field = Field { id: 17, owner: 3, kind: Kind::Decoy,
                    pos: glam::Vec3::ZERO, born: 2., until: 17.,
                    decoy_weapon: Some(weapon.into()) };
                let shots = |f: &Field| (0..17 * rate as u64)
                    .filter(|&tick| shot_due(f, tick, rate)).collect::<Vec<_>>();
                let original = shots(&field);
                assert_eq!(original, shots(&field.clone()), "snapshot identity reproduces timing");
                assert_eq!(original[0], (2 * rate + rate / 2) as u64);
                let gaps: Vec<_> = original.windows(2).map(|w| w[1] - w[0]).collect();
                assert!(gaps.len() >= 2);
                assert!(gaps.windows(2).any(|w| w[0] != w[1]), "no uniform metronome");
                let mut other = field.clone(); other.id += 1;
                assert_ne!(original, shots(&other), "separate decoys do not share a global cadence");
            }
        }
    }

    #[test]
    fn unobserved_weapons_keep_compatibility_schedule() {
        for rate in [64, 128] {
            for weapon in [None, Some("csgo_weapon_glock"), Some("mod_weapon_awp")] {
                let field = Field { id: 17, owner: 3, kind: Kind::Decoy,
                    pos: glam::Vec3::ZERO, born: 2., until: 17.,
                    decoy_weapon: weapon.map(str::to_owned) };
                for tick in 2 * rate as u64..17 * rate as u64 {
                    assert_eq!(shot_due(&field, tick, rate), tick % (rate as u64 / 2) == 0);
                }
            }
        }
    }

    #[test]
    fn active_snapshot_reproduces_remaining_shots_and_single_expiry() {
        for rate in [64, 128] {
            let mut collision = collision::CollisionWorld::new();
            collision.finalize();
            let mut world = crate::World::new(std::sync::Arc::new(collision));
            world.set_tick_rate(rate);
            world.utility_fields.push(Field { id: 17, owner: 3, kind: Kind::Decoy,
                pos: glam::Vec3::ZERO, born: 0., until: LIFETIME_SECONDS,
                decoy_weapon: Some("csgo_weapon_ak47".into()) });
            world.tick = 3 * rate as u64;
            let mut restored = world.clone();
            restored.restore(world.decode(&world.encode_state()).unwrap());
            let mut shots = 0;
            let mut expiries = 0;
            for tick in world.tick..17 * rate as u64 {
                world.tick = tick; restored.tick = tick;
                world.events.clear(); restored.events.clear();
                world.step_utility(); restored.step_utility();
                assert_eq!(world.events, restored.events);
                assert_eq!(world.state(), restored.state());
                for event in &world.events {
                    match event {
                        crate::GameEvent::DecoyShot { .. } => {
                            assert!(tick < 15 * rate as u64);
                            shots += 1;
                        }
                        crate::GameEvent::DecoyExpired { .. } => expiries += 1,
                        _ => panic!("isolated decoy emits only its shots and expiry"),
                    }
                }
            }
            assert!(shots > 0);
            assert_eq!(expiries, 1);
        }
    }
}

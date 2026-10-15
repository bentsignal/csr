//! Tick-authoritative pin/hold/release playback using installed timing metadata.
use super::*;
use movement::buttons;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThrowMode {
    Overhand,
    Underhand,
    Combined,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThrowPhase {
    PullingPin,
    Holding,
    Throwing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThrowState {
    pub phase: ThrowPhase,
    pub mode: ThrowMode,
    pub pull_started_tick: u64,
    pub throw_started_tick: Option<u64>,
    pub release_requested: bool,
    pub spawned: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Team, TickInput, weapon::WeaponState};
    use std::sync::Arc;

    fn fixture(rate: u32) -> (World, PlayerId) {
        let mut catalog=weapons::Catalog::from_items_game(r#""items_game" { "items" { "44" { "name" "weapon_hegrenade" "baseitem" "0" "attributes" { "primary default clip size" "1" } } } }"#).unwrap();
        catalog.weapons[0].grenade_animations = Some(weapons::GrenadeAnimations {
            pull_pin_duration: 0.25,
            overhand: weapons::GrenadeReleaseAnimation { command_release_time: None,
                duration: 0.5,
                release_time: 0.125,
            },
            underhand: weapons::GrenadeReleaseAnimation { command_release_time: None,
                duration: 0.5,
                release_time: 0.125,
            },
        });
        let mut collision = collision::CollisionWorld::new();
        collision.finalize();
        let mut world = World::with_catalog(Arc::new(collision), Arc::new(catalog));
        world.set_tick_rate(rate);
        let id = world.add_player("thrower", Team::Terrorist);
        let def = Arc::new(world.catalog.weapons[0].clone());
        world.players[0].weapons = vec![WeaponState::new(def)];
        world.players[0].active = 0;
        (world, id)
    }
    #[test]
    fn modes_hold_without_fuse_then_release_once_and_restore_at_both_rates() {
        for rate in [64, 128] {
            for (bits, mode) in [
                (buttons::ATTACK, ThrowMode::Overhand),
                (buttons::ATTACK2, ThrowMode::Underhand),
                (buttons::ATTACK | buttons::ATTACK2, ThrowMode::Combined),
            ] {
                let (mut world, id) = fixture(rate);
                for _ in 0..rate * 2 {
                    world.step(&[(
                        id,
                        TickInput {
                            buttons: bits,
                            ..Default::default()
                        },
                    )]);
                }
                assert!(
                    world.projectiles.is_empty(),
                    "held utility has no running fuse"
                );
                let held = world.players[0]
                    .active_weapon()
                    .unwrap()
                    .grenade_throw
                    .unwrap();
                assert_eq!((held.phase, held.mode), (ThrowPhase::Holding, mode));
                let state = world.decode(&world.encode_state()).unwrap();
                let mut restored = world.clone();
                restored.restore(state);
                let release_tick = world.tick;
                let mut throws = 0;
                for _ in 0..rate {
                    world.step(&[(id, TickInput::default())]);
                    restored.step(&[(id, TickInput::default())]);
                    assert_eq!(world.state(), restored.state());
                    if world
                        .events
                        .iter()
                        .any(|e| matches!(e, GameEvent::UtilityThrown { .. }))
                    {
                        throws += 1;
                        assert_eq!(world.tick - 1, release_tick + rate as u64 / 8);
                        let projectile = &world.projectiles[0];
                        let inherited = world.players[0].movement.velocity * 0.5;
                        let expected = Vec3::X * (750. * mode.strength()) + inherited
                            - Vec3::Z * (320. * world.dt());
                        assert!((projectile.velocity - expected).length() < 0.001);
                        assert!(projectile.born >= release_tick as f32 / rate as f32);
                    }
                }
                assert_eq!(throws, 1);
                assert!(world.players[0].weapons.is_empty());
            }
        }
    }
    #[test]
    fn observed_release_profile_drives_projectile_and_authored_pose_event_together() {
        for rate in [64, 128] {
            let (mut world, id) = fixture(rate);
            let definition = std::sync::Arc::make_mut(&mut world.players[0].weapons[0].def);
            let profile = definition.grenade_animations.as_mut().unwrap();
            profile.overhand = weapons::GrenadeReleaseAnimation {
                duration: 0.4, release_time: 1. / 3., command_release_time: Some(0.1),
            };
            let release = profile.overhand.clone();
            assert!((release.sample_time(0.1) - release.release_time).abs() < 0.000001);
            assert!((release.sample_time(0.1 + release.duration - release.release_time) - release.duration).abs() < 0.000001);
            world.step(&[(id, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
            let release_tick = world.tick;
            let expected_tick = release_tick + (0.1 / world.dt()).ceil() as u64;
            let mut actual = None;
            for _ in 0..rate / 2 {
                world.step(&[(id, TickInput::default())]);
                if world.events.iter().any(|e| matches!(e, GameEvent::UtilityThrown { .. })) {
                    actual = Some(world.tick - 1);
                }
            }
            assert_eq!(actual, Some(expected_tick));
        }
    }
    #[test]
    fn releasing_one_button_keeps_preparing_until_the_last_button_edge() {
        for rate in [64, 128] {
            for (held, mode) in [(buttons::ATTACK, ThrowMode::Overhand),
                (buttons::ATTACK2, ThrowMode::Underhand)] {
                let (mut world, id) = fixture(rate);
                world.step(&[(id, TickInput { buttons: buttons::ATTACK | buttons::ATTACK2,
                    ..Default::default() })]);
                world.step(&[(id, TickInput { buttons: held, ..Default::default() })]);
                let state = world.players[0].active_weapon().unwrap().grenade_throw.unwrap();
                assert_eq!((state.phase, state.mode), (ThrowPhase::PullingPin, mode));
                assert!(!state.release_requested);
                world.step(&[(id, TickInput::default())]);
                let state = world.players[0].active_weapon().unwrap().grenade_throw.unwrap();
                assert_eq!((state.phase, state.mode, state.throw_started_tick),
                    (ThrowPhase::Throwing, mode, Some(2)));
                assert!(!state.spawned, "projectile still follows its installed release event");
                let restored: crate::WorldState = world.decode(&world.encode_state()).unwrap();
                assert_eq!(restored, world.state());
            }
        }
    }
    #[test]
    fn tap_interrupts_pull_pin_and_switch_cancels_uncommitted_throw() {
        for rate in [64, 128] {
            let (mut world, id) = fixture(rate);
            world.step(&[(
                id,
                TickInput {
                    buttons: buttons::ATTACK,
                    ..Default::default()
                },
            )]);
            let mut emitted = None;
            for _ in 0..rate {
                world.step(&[(id, TickInput::default())]);
                if world
                    .events
                    .iter()
                    .any(|e| matches!(e, GameEvent::UtilityThrown { .. }))
                {
                    emitted = Some(world.tick - 1);
                    break;
                }
            }
            assert_eq!(emitted, Some(1 + (rate / 8) as u64));
            let (mut world, id) = fixture(rate);
            let spare = world.players[0].weapons[0].clone();
            world.players[0].weapons.push(spare);
            world.step(&[(
                id,
                TickInput {
                    buttons: buttons::ATTACK,
                    ..Default::default()
                },
            )]);
            let now = world.time();
            world.players[0].select(1, now);
            assert!(world.players[0].weapons[0].grenade_throw.is_none());
            for _ in 0..rate {
                world.step(&[(id, TickInput::default())]);
            }
            assert!(world.projectiles.is_empty());
        }
    }
}
impl ThrowMode {
    /// Provisional launch strength. The distinct overhand/underhand/combined
    /// controls are documented; numeric native launch laws remain unmeasured.
    pub fn strength(self) -> f32 {
        match self {
            Self::Overhand => 1.,
            Self::Underhand => 0.3,
            Self::Combined => 0.65,
        }
    }
    fn buttons(bits: u32) -> Option<Self> {
        match (bits & buttons::ATTACK != 0, bits & buttons::ATTACK2 != 0) {
            (true, false) => Some(Self::Overhand),
            (false, true) => Some(Self::Underhand),
            (true, true) => Some(Self::Combined),
            _ => None,
        }
    }
    pub fn activity(self) -> &'static str {
        if self == Self::Underhand {
            "ACT_VM_RELEASE"
        } else {
            "ACT_VM_THROW"
        }
    }
    pub fn animation(self, profile: &weapons::GrenadeAnimations) -> &weapons::GrenadeReleaseAnimation {
        if self == Self::Underhand {
            &profile.underhand
        } else {
            &profile.overhand
        }
    }
}
fn ticks(seconds: f32, dt: f32) -> u64 {
    (seconds / dt).ceil() as u64
}

impl World {
    pub(crate) fn step_grenade(&mut self, i: usize, bits: u32) {
        let tick = self.tick;
        let dt = self.dt();
        let time = self.time();
        let active = self.players[i].active;
        let Some(w) = self.players[i].weapons.get_mut(active) else {
            return;
        };
        let Some(profile) = w.def.grenade_animations.as_ref() else {
            return;
        };
        let mode = ThrowMode::buttons(bits);
        if w.grenade_throw.is_none() {
            if time < w.next_attack {
                return;
            }
            let Some(mode) = mode else { return };
            w.grenade_throw = Some(ThrowState {
                phase: ThrowPhase::PullingPin,
                mode,
                pull_started_tick: tick,
                throw_started_tick: None,
                release_requested: false,
                spawned: false,
            });
        }
        let state = w.grenade_throw.as_mut().unwrap();
        if state.phase != ThrowPhase::Throwing {
            if let Some(mode) = mode {
                state.mode = mode;
            } else {
                state.release_requested = true;
            }
            // Native quick taps interrupt preparation: the complete authored
            // pull-pin clip is not an admission timer for the release clip.
            // Keep the physical release event inside that clip tick-owned.
            if state.release_requested {
                state.phase = ThrowPhase::Throwing;
                state.throw_started_tick = Some(tick);
            } else if tick >= state.pull_started_tick.saturating_add(ticks(profile.pull_pin_duration, dt)) {
                state.phase = ThrowPhase::Holding;
            }
        }
        if state.phase != ThrowPhase::Throwing {
            return;
        }
        let animation = state.mode.animation(profile);
        let started = state.throw_started_tick.unwrap();
        let spawn =
            !state.spawned && tick >= started.saturating_add(ticks(animation.projectile_delay(), dt));
        let finish = state.spawned && tick >= started.saturating_add(ticks(animation.duration, dt));
        if spawn {
            if self.spawn_grenade(i) {
                self.players[i]
                    .active_weapon_mut()
                    .unwrap()
                    .grenade_throw
                    .as_mut()
                    .unwrap()
                    .spawned = true;
            } else {
                // Existing start-solid launch qualification fails without
                // consuming inventory or manufacturing a projectile.
                self.players[i].active_weapon_mut().unwrap().grenade_throw = None;
            }
        } else if finish {
            let p = &mut self.players[i];
            if self.mode == crate::GameMode::Deathmatch && self.rules.grenade_practice && !p.is_bot
            {
                // Replenish only after the authored release clip, so another
                // throw begins from Draw instead of retaining the old hand pose.
                let w = p.active_weapon_mut().unwrap();
                w.grenade_throw = None;
                w.next_attack = time + 0.5;
            } else {
                let index = p.active;
                p.take(index, time);
            }
        }
    }
}

//! A board/deck origin is not a CS foot placement. Admit the complete hull
//! against the live scene before dropping the mechanics state.
use crate::Player;
use collision::contents::MASK_PLAYERSOLID as MASK;
use glam::Vec3;
use movement::{PlayerMove, STAND_MAXS, STAND_MINS, TraceWorld};

fn clear(world: &dyn TraceWorld, position: Vec3, mins: Vec3, maxs: Vec3) -> bool {
    let hit = world.trace_hull(position, position, mins, maxs, MASK);
    !hit.start_solid && !hit.all_solid
}
pub(crate) fn place(player: &mut Player, world: &dyn TraceWorld) -> bool {
    let Some(state) = &player.mechanic else {
        return true;
    };
    let origin = player.movement.origin;
    // Prefer the same location, preserving genuinely airborne momentum. A
    // short upward repair handles deck penetration and uphill hull corners.
    for crouched in [false, true] {
        let (mins, maxs) = if crouched {
            (movement::DUCK_MINS, movement::DUCK_MAXS)
        } else {
            (STAND_MINS, STAND_MAXS)
        };
        for rise in 0..=48 {
            let candidate = origin + Vec3::Z * (rise as f32 * 2.);
            let path = world.trace_hull(
                origin + Vec3::Z * 16.,
                candidate + Vec3::Z * 16.,
                Vec3::ZERO,
                Vec3::ZERO,
                MASK,
            );
            if path.start_solid || path.all_solid || path.fraction < 1. {
                continue;
            }
            if !clear(world, candidate, mins, maxs) {
                continue;
            }
            let mut movement = player.movement;
            movement.origin = candidate;
            movement.ducked = crouched;
            movement.duck_amount = u8::from(crouched) as f32;
            movement.on_ground = false;
            movement.ground_ticks = 0;
            movement.ladder_normal = None;
            if rise > 0 {
                movement.velocity.z = movement.velocity.z.max(0.);
            }
            player.movement = movement;
            return true;
        }
    }
    let backup = state.cs_return;
    if clear(world, backup.origin, backup.hull().0, backup.hull().1) {
        player.movement = PlayerMove {
            velocity: Vec3::ZERO,
            ..backup
        };
        return true;
    }
    // A closing door can invalidate even the prior safe spot. Keep skating
    // rather than admit an embedded CS hull; the player can move and retry.
    false
}

pub(super) fn remember(player: &mut Player, world: &dyn TraceWorld) {
    if clear(world, player.movement.origin, STAND_MINS, STAND_MAXS) {
        let mut safe = player.movement;
        safe.ducked = false;
        safe.duck_amount = 0.;
        if let Some(state) = &mut player.mechanic {
            state.cs_return = safe;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Scene {
        slope: f32,
        ceiling: bool,
        blocked: bool,
    }
    impl TraceWorld for Scene {
        fn trace_hull(
            &self,
            start: Vec3,
            end: Vec3,
            mins: Vec3,
            maxs: Vec3,
            _: i32,
        ) -> collision::Trace {
            let transform = |v: Vec3| Vec3::new(v.x, v.y, v.z - self.slope * v.x);
            let min = Vec3::new(mins.x, mins.y, mins.z - self.slope * maxs.x);
            let mut t = collision::Trace::empty(transform(end));
            collision::clip_bounds(
                transform(start),
                transform(end),
                min,
                maxs,
                Vec3::splat(-10000.),
                Vec3::new(10000., 10000., 0.),
                1,
                0,
                &mut t,
            );
            if self.ceiling {
                collision::clip_bounds(
                    start,
                    end,
                    mins,
                    maxs,
                    Vec3::new(-10000., -10000., 64.),
                    Vec3::new(10000., 10000., 100.),
                    1,
                    0,
                    &mut t,
                );
            }
            if self.blocked {
                collision::clip_bounds(
                    start,
                    end,
                    mins,
                    maxs,
                    Vec3::new(-30., -30., -100.),
                    Vec3::new(30., 30., 200.),
                    1,
                    0,
                    &mut t,
                );
            }
            t.end = start + (end - start) * t.fraction;
            t
        }
    }
    fn player(origin: Vec3) -> Player {
        let mut world = crate::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
        let id = world.add_player("rider", crate::Team::CounterTerrorist);
        let mut p = world.player(id).unwrap().clone();
        p.movement = PlayerMove::spawn_at(origin);
        p.mechanic = Some(super::super::State {
            cs_return: PlayerMove::spawn_at(Vec3::new(100., 0., 30.)),
            id: "test".into(),
            archive_hash: "test".into(),
            data: [0.; mechanics_api::STATE_WORDS],
            angles: Vec3::ZERO,
            height: 0.,
            reset: false,
            external: None,
        });
        p
    }
    #[test]
    fn floor_and_uphill_full_hulls_are_cleared_without_a_jump() {
        for slope in [0., 0.4] {
            let scene = Scene {
                slope,
                ceiling: false,
                blocked: false,
            };
            let mut p = player(Vec3::new(0., 0., -3.));
            assert!(place(&mut p, &scene));
            assert!(clear(&scene, p.movement.origin, STAND_MINS, STAND_MAXS));
            assert!(p.movement.origin.z >= slope * STAND_MAXS.x);
            assert!(p.movement.origin.z < 20.);
        }
    }
    #[test]
    fn low_ceiling_uses_crouch_without_teleporting_through_it() {
        let scene = Scene {
            slope: 0.,
            ceiling: true,
            blocked: false,
        };
        let mut p = player(Vec3::new(0., 0., -2.));
        assert!(place(&mut p, &scene));
        assert!(p.movement.ducked);
        assert!(p.movement.origin.z < 10.);
        assert!(clear(
            &scene,
            p.movement.origin,
            p.movement.hull().0,
            p.movement.hull().1
        ));
    }
    #[test]
    fn airborne_momentum_is_preserved_and_closed_cover_uses_valid_backup() {
        let scene = Scene {
            slope: 0.,
            ceiling: false,
            blocked: false,
        };
        let mut p = player(Vec3::Z * 300.);
        p.movement.velocity = Vec3::new(120., -50., 220.);
        let before = p.movement;
        assert!(place(&mut p, &scene));
        assert_eq!(p.movement.origin, before.origin);
        assert_eq!(p.movement.velocity, before.velocity);
        let blocked = Scene {
            blocked: true,
            ..scene
        };
        p.movement.origin = Vec3::ZERO;
        assert!(place(&mut p, &blocked));
        assert_eq!(p.movement.origin.x, 100.);
        p.movement.origin = Vec3::ZERO;
        p.mechanic.as_mut().unwrap().cs_return.origin = Vec3::ZERO;
        assert!(!place(&mut p, &blocked));
        assert!(p.mechanic.is_some());
    }
}

//! Synthetic lifecycle regressions; native timing/surface parity is separate.
use super::*;
use collision::{CollisionWorld, Convex, Plane};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

fn floor() -> CollisionWorld {
    let mut collision = CollisionWorld::new();
    assert!(collision.add_convex(Convex {
        mins: Vec3::new(-512., -512., -16.),
        maxs: Vec3::new(512., 512., 0.),
        planes: [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
            .into_iter().zip([512., 512., 512., 512., 0., 16.])
            .map(|(normal, dist)| Plane { normal, dist }).collect(),
        contents: contents::SOLID, surface: 0,
    }));
    collision.finalize();
    collision
}

fn projectile(kind: Kind, pos: Vec3, velocity: Vec3) -> Projectile {
    Projectile { id: 1, owner: crate::WORLD, kind, pos, velocity,
        settled: false, born: 0., decoy_weapon: None }
}

fn assert_in_flight(world: &World) {
    assert_eq!(world.projectiles.len(), 1);
    assert!(!world.projectiles[0].settled);
    assert!(world.utility_fields.is_empty());
    assert!(!world.events.iter().any(|e| matches!(e, GameEvent::UtilityDetonated { .. })));
}

#[test]
fn smoke_waits_through_slow_apex_upward_and_drop_flight_past_old_deadline() {
    for rate in [64, 128] {
        for age in [2, 4] {
            for vz in [0., 10., -10., 400., -400.] {
                let mut collision = CollisionWorld::new();
                collision.finalize();
                let mut world = World::new(Arc::new(collision));
                world.set_tick_rate(rate);
                world.tick = (rate * age) as u64;
                world.projectiles.push(projectile(Kind::Smoke, Vec3::Z * 4096., Vec3::Z * vz));
                let mut restored = world.clone();
                restored.restore(world.decode(&world.encode_state()).unwrap());
                let mut predicted = restored.clone();
                for _ in 0..rate {
                    world.step(&[]); restored.step(&[]); predicted.step_prediction(&[]);
                    assert_in_flight(&world);
                    assert_eq!(world.state(), restored.state());
                    assert_eq!(world.state(), predicted.state());
                    assert_eq!(world.events, restored.events);
                }
            }
        }
    }
}

#[test]
fn smoke_landing_does_not_activate_until_settled_and_emits_once() {
    for rate in [64, 128] {
        for velocity in [Vec3::Z * 640., -Vec3::Z * 400.] {
            let mut world = World::new(Arc::new(floor()));
            world.set_tick_rate(rate);
            world.projectiles.push(projectile(Kind::Smoke, Vec3::Z * 64., velocity));
            let mut had_unsettled_contact = false;
            let mut detonations = 0;
            for _ in 0..rate * 16 {
                world.step(&[]);
                if !world.projectiles.is_empty() {
                    if world.events.iter().any(|e| matches!(e, GameEvent::UtilityBounced { .. }))
                        && !world.projectiles[0].settled {
                        had_unsettled_contact = true;
                    }
                    assert!(world.utility_fields.is_empty());
                }
                detonations += world.events.iter().filter(|e| matches!(e,
                    GameEvent::UtilityDetonated { kind: Kind::Smoke, .. })).count();
            }
            assert!(had_unsettled_contact, "landing alone must not replace settling");
            assert_eq!(detonations, 1);
            assert!(world.projectiles.is_empty());
            assert_eq!(world.utility_fields.len(), 1);
            assert!(world.utility_fields[0].pos.z < 3.);
            assert!(world.utility_fields[0].born > 1.5);
            if velocity.z > 0. {
                assert!(world.utility_fields[0].born > 3., "long upward flight must survive the old deadline");
            }
        }
    }
}

#[test]
fn early_supported_rest_waits_for_fuse_and_replays_through_activation() {
    for rate in [64, 128] {
        let mut world = World::new(Arc::new(floor()));
        world.set_tick_rate(rate);
        world.projectiles.push(projectile(Kind::Smoke, Vec3::Z * 2.1, -Vec3::Z * 20.));
        world.step(&[]);
        assert!(world.projectiles[0].settled);
        let mut restored = world.clone();
        restored.restore(world.decode(&world.encode_state()).unwrap());
        let mut predicted = restored.clone();
        for _ in 0..rate * 2 {
            world.step(&[]); restored.step(&[]); predicted.step_prediction(&[]);
            assert_eq!(world.state(), restored.state());
            assert_eq!(world.state(), predicted.state());
            assert_eq!(world.events, restored.events);
            if world.time() <= 1.5 {
                assert!(world.utility_fields.is_empty());
                assert!(world.projectiles[0].settled);
            }
        }
        assert!(world.projectiles.is_empty());
        assert_eq!(world.utility_fields.len(), 1);
        assert!(world.utility_fields[0].born > 1.5);
        assert!(world.utility_fields[0].born <= 1.5 + world.dt());
    }
}

#[test]
fn smoke_losing_support_on_ready_tick_falls_instead_of_activating() {
    struct RemovableFloor { collision: CollisionWorld, present: AtomicBool }
    impl collision::DynamicWorld for RemovableFloor {
        fn trace_box(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32) -> collision::Trace {
            if self.present.load(Ordering::Relaxed) {
                self.collision.trace_box(start, end, mins, maxs, mask)
            } else { collision::Trace::empty(end) }
        }
        fn point_contents(&self, _: Vec3, _: i32) -> i32 { 0 }
    }
    for rate in [64, 128] {
        let floor = Arc::new(RemovableFloor { collision: floor(), present: AtomicBool::new(true) });
        let mut collision = CollisionWorld::new();
        collision.dynamic = Some(floor.clone());
        let mut world = World::new(Arc::new(collision));
        world.set_tick_rate(rate);
        world.tick = rate as u64;
        world.projectiles.push(projectile(Kind::Smoke, Vec3::Z * 2.1, -Vec3::Z * 20.));
        world.step(&[]);
        assert!(world.projectiles[0].settled);
        world.tick = 2 * rate as u64;
        let mut restored = world.clone();
        restored.restore(world.decode(&world.encode_state()).unwrap());
        let mut predicted = restored.clone();
        let previous_z = world.projectiles[0].pos.z;
        floor.present.store(false, Ordering::Relaxed);
        world.step(&[]); restored.step(&[]); predicted.step_prediction(&[]);
        assert_in_flight(&world);
        assert!(world.projectiles[0].pos.z < previous_z);
        assert!(world.projectiles[0].velocity.z < 0.);
        assert_eq!(world.state(), restored.state());
        assert_eq!(world.state(), predicted.state());
    }
}

#[test]
fn upward_or_horizontal_impulse_clears_previous_rest_near_support() {
    for rate in [64, 128] {
        for velocity in [Vec3::Z * 10., Vec3::X * 10., Vec3::X * 100.] {
            let mut world = World::new(Arc::new(floor()));
            world.set_tick_rate(rate);
            world.tick = 4 * rate as u64;
            let mut p = projectile(Kind::Smoke, Vec3::Z * 2.13, velocity);
            p.settled = true;
            world.projectiles.push(p);
            world.step(&[]);
            assert_in_flight(&world);
        }
    }
}

#[test]
fn he_flash_decoy_keep_existing_air_fuses() {
    for rate in [64, 128] {
        for (kind, fuse) in [(Kind::He, 1.5), (Kind::Flash, 1.5)] {
            let mut collision = CollisionWorld::new();
            collision.finalize();
            let mut world = World::new(Arc::new(collision));
            world.set_tick_rate(rate);
            world.tick = (fuse * rate as f32) as u64;
            world.projectiles.push(projectile(kind, Vec3::Z * 4096., Vec3::Z * 400.));
            world.step(&[]);
            assert_in_flight(&world);
            world.step(&[]);
            assert!(world.projectiles.is_empty());
            assert_eq!(world.events.iter().filter(|e| matches!(e,
                GameEvent::UtilityDetonated { kind: actual, .. } if *actual == kind)).count(), 1);
        }
        // Decoy's airborne fallback remains separate pending its own evidence.
        let mut collision = CollisionWorld::new();
        collision.finalize();
        let mut world = World::new(Arc::new(collision));
        world.set_tick_rate(rate);
        world.tick = 4 * rate as u64;
        world.projectiles.push(projectile(Kind::Decoy, Vec3::Z * 4096., Vec3::Z * 400.));
        world.step(&[]);
        assert!(world.projectiles.is_empty());
        assert_eq!(world.utility_fields[0].kind, Kind::Decoy);
    }
}

#[test]
fn fire_air_expiry_uses_exposed_default_without_a_field_and_replays_once() {
    for rate in [64, 128] {
        for kind in [Kind::Molotov, Kind::Incendiary] {
            // The near-floor case deliberately tests the conservative direct-
            // contact policy, not a claim about native floor-near projection.
            for (height, vz) in [(4096., -400.), (4096., 0.), (4096., 400.), (8., 0.)] {
                let mut world = World::new(Arc::new(floor()));
                world.set_tick_rate(rate);
                world.tick = 12 * rate as u64;
                let mut p = projectile(kind, Vec3::Z * height, Vec3::Z * vz);
                p.born = 10.; // The fuse is relative to projectile birth, not world time.
                world.projectiles.push(p);
                let mut restored = world.clone();
                restored.restore(world.decode(&world.encode_state()).unwrap());
                let mut predicted = restored.clone();
                let mut detonations = 0;
                for tick in 0..rate * 4 {
                    world.step(&[]); restored.step(&[]); predicted.step_prediction(&[]);
                    assert_eq!(world.state(), restored.state());
                    assert_eq!(world.state(), predicted.state());
                    assert_eq!(world.events, restored.events);
                    if tick == 0 {
                        // Preserve the existing strict age comparison. This is
                        // a synthetic policy boundary, not native think timing.
                        assert_in_flight(&world);
                    } else {
                        assert!(world.projectiles.is_empty());
                    }
                    detonations += world.events.iter().filter(|e| matches!(e,
                        GameEvent::UtilityDetonated { kind: actual, .. } if *actual == kind)).count();
                    assert!(world.utility_fields.is_empty());
                    assert!(!world.fire_at(Vec3::Z * height));
                    assert!(!world.events.iter().any(|e| matches!(e, GameEvent::UtilityBounced { .. })));
                }
                assert_eq!(detonations, 1);
            }
        }
    }
}

#[test]
fn fire_floor_contact_ignites_before_or_on_expired_tick_and_replays_once() {
    for rate in [64, 128] {
        for kind in [Kind::Molotov, Kind::Incendiary] {
            // Include a floor impact on the first expired tick: an ignition
            // remains distinct from expiry without a supporting contact.
            for tick in [rate as u64, 2 * rate as u64 + 1] {
                let mut world = World::new(Arc::new(floor()));
                world.set_tick_rate(rate);
                world.tick = tick;
                world.projectiles.push(projectile(kind, Vec3::Z * 2.1, -Vec3::Z * 400.));
                let mut restored = world.clone();
                restored.restore(world.decode(&world.encode_state()).unwrap());
                let mut predicted = restored.clone();
                let mut detonations = 0;
                for _ in 0..4 {
                    world.step(&[]); restored.step(&[]); predicted.step_prediction(&[]);
                    assert_eq!(world.state(), restored.state());
                    assert_eq!(world.state(), predicted.state());
                    assert_eq!(world.events, restored.events);
                    assert!(world.projectiles.is_empty());
                    assert_eq!(world.utility_fields.len(), 1);
                    let field = &world.utility_fields[0];
                    assert_eq!((field.id, field.owner, field.kind), (1, crate::WORLD, kind));
                    assert!((2. ..2.3).contains(&field.pos.z));
                    assert_eq!(field.until - field.born, 7.);
                    assert!(world.fire_at(Vec3::ZERO));
                    detonations += world.events.iter().filter(|e| matches!(e,
                        GameEvent::UtilityDetonated { kind: actual, .. } if *actual == kind)).count();
                    assert!(!world.events.iter().any(|e| matches!(e, GameEvent::UtilityBounced { .. })));
                }
                assert_eq!(detonations, 1);
            }
        }
    }
}

#[test]
fn fire_early_contact_keeps_existing_guard_before_later_floor_ignition() {
    for rate in [64, 128] {
        for kind in [Kind::Molotov, Kind::Incendiary] {
            let mut world = World::new(Arc::new(floor()));
            world.set_tick_rate(rate);
            world.projectiles.push(projectile(kind, Vec3::Z * 2.1, -Vec3::Z * 400.));
            world.step(&[]);
            assert_in_flight(&world);
            assert!(world.events.iter().any(|e| matches!(e, GameEvent::UtilityBounced { .. })));
            // Supply a later real floor impact independently of bounce tuning.
            world.tick = rate as u64 / 4;
            world.projectiles[0].pos = Vec3::Z * 2.1;
            world.projectiles[0].velocity = -Vec3::Z * 400.;
            world.step(&[]);
            assert!(world.projectiles.is_empty());
            assert_eq!(world.utility_fields.len(), 1);
            assert_eq!(world.utility_fields[0].kind, kind);
        }
    }
}

#[test]
fn fire_wall_contact_bounces_before_expiry_and_never_creates_ground_fire() {
    let mut wall = CollisionWorld::new();
    assert!(wall.add_convex(Convex {
        mins: Vec3::new(-16., -512., -512.),
        maxs: Vec3::new(0., 512., 512.),
        planes: [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
            .into_iter().zip([0., 16., 512., 512., 512., 512.])
            .map(|(normal, dist)| Plane { normal, dist }).collect(),
        contents: contents::SOLID, surface: 0,
    }));
    wall.finalize();
    let wall = Arc::new(wall);
    for rate in [64, 128] {
        for kind in [Kind::Molotov, Kind::Incendiary] {
            for age in [1, 3] {
                let mut world = World::new(wall.clone());
                world.set_tick_rate(rate);
                world.tick = (age * rate) as u64;
                world.projectiles.push(projectile(kind, Vec3::new(2.1, 0., 64.), -Vec3::X * 400.));
                world.step(&[]);
                assert!(world.utility_fields.is_empty());
                if age == 1 {
                    assert_in_flight(&world);
                    assert!(world.events.iter().any(|e| matches!(e, GameEvent::UtilityBounced { .. })));
                } else {
                    assert!(world.projectiles.is_empty());
                    assert_eq!(world.events.iter().filter(|e| matches!(e,
                        GameEvent::UtilityDetonated { kind: actual, .. } if *actual == kind)).count(), 1);
                }
            }
        }
    }
}

//! Regression coverage for supported Smoke activation; native timing parity is separate.
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
fn other_grenades_keep_existing_air_fuses_and_fire_floor_contact() {
    for rate in [64, 128] {
        for (kind, fuse) in [(Kind::He, 1.5), (Kind::Flash, 1.5),
            (Kind::Molotov, 3.5), (Kind::Incendiary, 3.5)] {
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
        for kind in [Kind::Molotov, Kind::Incendiary] {
            let mut world = World::new(Arc::new(floor()));
            world.set_tick_rate(rate);
            world.tick = rate as u64;
            world.projectiles.push(projectile(kind, Vec3::Z * 2.1, -Vec3::Z * 400.));
            world.step(&[]);
            assert!(world.projectiles.is_empty());
            assert_eq!(world.utility_fields.len(), 1);
            assert_eq!(world.utility_fields[0].kind, kind);
            assert!(!world.events.iter().any(|e| matches!(e, GameEvent::UtilityBounced { .. })));
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

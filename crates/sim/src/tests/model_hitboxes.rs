//! Behavioral regressions for the hitscan path, using asset-independent shapes.
use super::*;
use mdl::hitbox::{PosedHitbox, Shape};

fn fixture(rate: u32) -> (World, PlayerId, PlayerId) {
    let mut w = world();
    w.set_tick_rate(rate);
    let shooter = w.add_player("shooter", Team::Terrorist);
    let target = w.add_player("target", Team::CounterTerrorist);
    w.players[0].movement = PlayerMove::spawn_at(Vec3::ZERO);
    w.players[0].view_angles = Vec3::ZERO;
    let gun = Arc::new(w.catalog.get("weapon_ak47").unwrap().clone());
    w.players[0].weapons = vec![WeaponState::new(gun)];
    w.players[0].active = 0;
    w.players[1].movement = PlayerMove::spawn_at(Vec3::X * 100.);
    w.players[1].armor = 0;
    w.players[1].helmet = false;
    (w, shooter, target)
}

fn capsule(index: usize, group: i32, center: Vec3) -> PosedHitbox {
    PosedHitbox {
        index,
        group,
        shape: Shape::Capsule {
            start: center - Vec3::Z * 4.,
            end: center + Vec3::Z * 4.,
            radius: 3.,
        },
    }
}

fn capture(w: &mut World, tick: u64) {
    w.tick = tick;
    w.rewind
        .as_mut()
        .unwrap()
        .capture(tick, w.round.round, &w.players, &w.character_hitboxes);
}

#[test]
fn bullet_between_authored_shapes_misses_even_inside_movement_hull() {
    for rate in [64, 128] {
        let (mut w, _, target) = fixture(rate);
        let height = w.players[0].eye().z;
        w.set_test_hitboxes(
            target,
            vec![
                capsule(0, 2, Vec3::new(100., -10., height)),
                capsule(1, 3, Vec3::new(100., 10., height)),
            ],
        );
        // Both the old movement hull and the union AABB enclose this ray.
        // Neither authored shape does: broad phase cannot award damage.
        w.fire(0, 1., None);
        assert_eq!(w.player(target).unwrap().health, 100, "rate {rate}");
        assert!(!w.events.iter().any(|event| matches!(
            event, GameEvent::Damage { victim, .. } if *victim == target
        )));
        assert!(w.events.iter().any(|event| matches!(
            event, GameEvent::Tracer { start, end, .. } if end.x - start.x > 8000.
        )));
    }
}

#[test]
fn exposed_arm_outside_movement_hull_takes_authored_arm_damage() {
    for rate in [64, 128] {
        let (mut w, shooter, target) = fixture(rate);
        w.player_mut(shooter).unwrap().movement.origin.y = 26.;
        let height = w.player(shooter).unwrap().eye().z;
        w.set_test_hitboxes(target, vec![capsule(0, 4, Vec3::new(100., 26., height))]);
        // The target's movement hull extends only 16 units sideways. An
        // exposed limb must participate in broad phase and retain its group.
        w.fire(0, 1., None);
        assert!(w.player(target).unwrap().health < 100, "rate {rate}");
        assert!(w.events.iter().any(|event| matches!(
            event,
            GameEvent::Damage { attacker, victim, group: HitGroup::LeftArm, health, .. }
                if *attacker == shooter && *victim == target && *health > 0
        )));
    }
}

#[test]
fn historical_model_hit_uses_old_shapes_without_moving_live_target_or_crossing_lives() {
    for rate in [64, 128] {
        let (mut w, shooter, target) = fixture(rate);
        let height = w.player(shooter).unwrap().eye().z;
        w.configure_hit_history(rate, 128.).unwrap();
        w.set_test_hitboxes(target, vec![capsule(0, 3, Vec3::new(100., 0., height))]);
        capture(&mut w, 0);
        let current_origin = Vec3::new(100., 64., 0.);
        w.player_mut(target).unwrap().movement.origin = current_origin;
        w.set_test_hitboxes(target, vec![capsule(0, 3, Vec3::new(100., 64., height))]);
        capture(&mut w, 1);

        let mut current = w.clone();
        current.fire(0, 1., None);
        assert_eq!(
            current.player(target).unwrap().health,
            100,
            "current pose rate {rate}"
        );

        let mut historical = w.clone();
        historical.fire(0, 1., Some(0));
        assert!(
            historical.player(target).unwrap().health < 100,
            "old pose rate {rate}"
        );
        assert_eq!(
            historical.player(target).unwrap().movement.origin,
            current_origin
        );
        assert!(historical.events.iter().any(|event| matches!(
            event,
            GameEvent::Damage { victim, group: HitGroup::Stomach, pos, .. }
                if *victim == target && pos.y.abs() < 0.001
        )));

        // Exercise the existing rejection guards with model shapes present,
        // rather than proving them only against coarse player hulls.
        for case in 0..3 {
            let mut invalid = w.clone();
            match case {
                0 => {
                    capture(&mut invalid, rate as u64 + 1);
                }
                1 => {
                    invalid.respawn(target);
                    invalid.player_mut(target).unwrap().movement.origin = current_origin;
                    invalid.set_test_hitboxes(
                        target,
                        vec![capsule(0, 3, Vec3::new(100., 64., height))],
                    );
                }
                _ => {
                    invalid.player_mut(target).unwrap().movement.origin.y = 512.;
                    invalid.set_test_hitboxes(
                        target,
                        vec![capsule(0, 3, Vec3::new(100., 512., height))],
                    );
                    capture(&mut invalid, 2);
                }
            }
            invalid.fire(0, 1., Some(0));
            assert_eq!(
                invalid.player(target).unwrap().health,
                100,
                "guard {case}, rate {rate}"
            );
            assert!(!invalid.events.iter().any(|event| matches!(
                event, GameEvent::Damage { victim, .. } if *victim == target
            )));
        }
    }
}

#[test]
fn replicated_pose_phase_survives_restore_and_tick_rate_change_keeps_fire_age() {
    for rate in [64, 128] {
        let (mut w, shooter, _) = fixture(rate);
        w.tick = rate as u64 * 2;
        w.player_mut(shooter).unwrap().animation = model_pose::PoseState {
            move_cycle: 0.375,
            air_time: 0.125,
            last_fire_tick: Some(rate as u64 * 2 - rate as u64 / 4),
        };
        let expected = w.player(shooter).unwrap().animation;
        let decoded: WorldState = w.decode(&w.encode_state()).unwrap();
        let mut restored = World::with_catalog(w.collision.clone(), w.catalog.clone());
        restored.restore(decoded);
        assert_eq!(restored.player(shooter).unwrap().animation, expected);
        let new_rate = if rate == 64 { 128 } else { 64 };
        restored.set_tick_rate(new_rate);
        let phase = restored.player(shooter).unwrap().animation;
        assert_eq!(phase.move_cycle, expected.move_cycle);
        assert_eq!(phase.air_time, expected.air_time);
        let fired = phase.last_fire_tick.unwrap();
        assert!(fired <= restored.tick);
        let age = (restored.tick - fired) as f32 / new_rate as f32;
        assert!((age - 0.25).abs() <= 1. / new_rate as f32);
    }
}

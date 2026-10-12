//! Current-world tracked muzzle admission; no historical collision mutation.
use super::*;

fn match_pair() -> (World, PlayerId, PlayerId) {
    let mut world = world();
    let shooter = world.add_player("tracked", Team::Terrorist);
    let target = world.add_player("target", Team::CounterTerrorist);
    teleport(&mut world, shooter, Vec3::new(0.0, 0.0, 1.0));
    teleport(&mut world, target, Vec3::new(100.0, 0.0, 1.0));
    let gun = Arc::new(def("weapon_test", 7, WeaponClass::Rifle, Side::Both, 0, 30));
    world.players[0].weapons = vec![WeaponState::new(gun)];
    world.players[0].active = 0;
    (world, shooter, target)
}

fn brush(world: &mut World, mins: Vec3, maxs: Vec3) {
    let collision = Arc::get_mut(&mut world.collision).unwrap();
    let planes = [
        (Vec3::X, maxs.x),
        (-Vec3::X, -mins.x),
        (Vec3::Y, maxs.y),
        (-Vec3::Y, -mins.y),
        (Vec3::Z, maxs.z),
        (-Vec3::Z, -mins.z),
    ]
    .into_iter()
    .map(|(normal, dist)| collision::Plane { normal, dist })
    .collect();
    assert!(collision.add_convex(collision::Convex {
        planes,
        mins,
        maxs,
        contents: contents::SOLID,
        surface: 0
    }));
    collision.finalize();
}

fn tracked(origin: Vec3, direction: Vec3) -> TickInput {
    TickInput {
        buttons: buttons::ATTACK,
        aim: Some(AimRay { origin, direction }),
        ..Default::default()
    }
}

#[test]
fn tracked_muzzle_beyond_wall_falls_back_and_cannot_damage() {
    let (mut world, shooter, target) = match_pair();
    wall_at(&mut world, 24.0);
    world.step(&[(shooter, tracked(Vec3::new(40.0, 0.0, 64.0), Vec3::X))]);
    let actor = world.player(shooter).unwrap();
    assert_eq!(
        actor.aim.unwrap().origin,
        Vec3::Z * actor.movement.view_offset()
    );
    assert_eq!(actor.aim.unwrap().direction, Vec3::X);
    assert_eq!(actor.last_input.aim, actor.aim);
    assert_eq!(world.player(target).unwrap().health, 100);
    assert!(world.events.iter().any(|event| matches!(event, GameEvent::Fire { origin, .. } if origin.distance(actor.eye()) < 0.0001)));
    assert!(
        world
            .events
            .iter()
            .any(|event| matches!(event, GameEvent::Impact { .. }))
    );
}

#[test]
fn adjacent_thin_brush_blocks_short_eye_muzzle_segment() {
    let (mut world, shooter, target) = match_pair();
    // The complete eye-muzzle segment is shorter than the collision epsilon.
    brush(
        &mut world,
        Vec3::new(0.01, -20.0, 50.0),
        Vec3::new(0.02, 20.0, 80.0),
    );
    let eye = world.player(shooter).unwrap().eye();
    assert!(
        !world
            .collision
            .trace_ray(eye, eye + Vec3::X * 0.03, contents::MASK_SHOT)
            .hit()
    );
    assert!(
        world
            .collision
            .trace_box(
                eye,
                eye + Vec3::X * 0.03,
                Vec3::splat(-AIM_WALL_MARGIN),
                Vec3::splat(AIM_WALL_MARGIN),
                contents::MASK_SHOT
            )
            .all_solid
    );
    world.step(&[(shooter, tracked(Vec3::new(0.03, 0.0, 64.0), Vec3::X))]);
    let actor = world.player(shooter).unwrap();
    assert_eq!(
        actor.aim.unwrap().origin,
        Vec3::Z * actor.movement.view_offset()
    );
    assert_eq!(world.player(target).unwrap().health, 100);
    assert_eq!(actor.last_input.buttons & buttons::ATTACK, 0);
    assert!(
        !world
            .events
            .iter()
            .any(|event| matches!(event, GameEvent::Fire { .. }))
    );
}

#[test]
fn escaping_start_solid_fraction_one_cannot_fire_stab_or_throw() {
    for kind in [WeaponClass::Rifle, WeaponClass::Knife, WeaponClass::Grenade] {
        let (mut world, shooter, target) = match_pair();
        brush(
            &mut world,
            Vec3::new(-4.0, -4.0, 50.0),
            Vec3::new(4.0, 4.0, 80.0),
        );
        let eye = world.player(shooter).unwrap().eye();
        let escaping = world
            .collision
            .trace_ray(eye, eye + Vec3::X * 20.0, contents::MASK_SHOT);
        assert!(escaping.start_solid && !escaping.all_solid);
        assert_eq!(escaping.fraction, 1.0);
        let name = match kind {
            WeaponClass::Knife => "weapon_knife_t",
            WeaponClass::Grenade => "weapon_hegrenade",
            _ => "weapon_test",
        };
        world.players[0].weapons = vec![WeaponState::new(Arc::new(def(
            name,
            44,
            kind,
            Side::Both,
            0,
            1,
        )))];
        let input = TickInput {
            buttons: buttons::ATTACK
                | buttons::ATTACK2
                | buttons::USE
                | buttons::WALK
                | buttons::RELOAD,
            ..tracked(Vec3::new(20.0, 0.0, 64.0), Vec3::X)
        };
        world.step(&[(shooter, input)]);
        assert_eq!(
            world.player(shooter).unwrap().last_input.buttons,
            buttons::USE | buttons::WALK | buttons::RELOAD
        );
        assert_eq!(world.player(target).unwrap().health, 100);
        assert!(world.projectiles.is_empty());
        assert!(!world.events.iter().any(|event| matches!(
            event,
            GameEvent::Fire { .. } | GameEvent::Melee { .. } | GameEvent::UtilityThrown { .. }
        )));
    }
}

#[test]
fn obstructed_melee_and_grenade_use_the_admitted_eye() {
    for kind in [WeaponClass::Knife, WeaponClass::Grenade] {
        let (mut world, shooter, target) = match_pair();
        teleport(&mut world, target, Vec3::new(52.0, 0.0, 1.0));
        wall_at(&mut world, 24.0);
        let name = if kind == WeaponClass::Knife {
            "weapon_knife_t"
        } else {
            "weapon_hegrenade"
        };
        world.players[0].weapons = vec![WeaponState::new(Arc::new(def(
            name,
            44,
            kind,
            Side::Both,
            0,
            1,
        )))];
        world.step(&[(shooter, tracked(Vec3::new(40.0, 0.0, 64.0), Vec3::X))]);
        let actor = world.player(shooter).unwrap();
        assert_eq!(world.player(target).unwrap().health, 100);
        assert_eq!(
            actor.aim.unwrap().origin,
            Vec3::Z * actor.movement.view_offset()
        );
        if kind == WeaponClass::Knife {
            assert!(world.events.iter().any(|event| matches!(event, GameEvent::Melee { origin, hit: MeleeHit::World, .. } if origin.distance(actor.eye()) < 0.0001)));
        } else {
            assert_eq!(world.projectiles.len(), 1);
            assert!(world.projectiles[0].pos.x < 24.0);
        }
    }
}

#[test]
fn clear_offset_preserves_direction_and_flat_aim_is_unchanged() {
    let (world, shooter, _) = match_pair();
    let mut flat = world.clone();
    let mut explicit_eye = world.clone();
    let eye_offset = Vec3::Z * world.player(shooter).unwrap().movement.view_offset();
    flat.step(&[(
        shooter,
        TickInput {
            buttons: buttons::ATTACK,
            ..Default::default()
        },
    )]);
    explicit_eye.step(&[(shooter, tracked(eye_offset, Vec3::X))]);
    assert_eq!(flat.events, explicit_eye.events);
    assert_eq!(
        flat.player(shooter).unwrap().movement,
        explicit_eye.player(shooter).unwrap().movement
    );
    assert!(flat.player(shooter).unwrap().aim.is_none());
    let mut offset = world;
    offset.step(&[(shooter, tracked(Vec3::new(4.0, 6.0, 64.0), Vec3::X * 3.0))]);
    assert_eq!(
        offset.player(shooter).unwrap().aim.unwrap().origin,
        Vec3::new(4.0, 6.0, 64.0)
    );
    assert_eq!(
        offset.player(shooter).unwrap().aim.unwrap().direction,
        Vec3::X
    );
}

#[test]
fn post_move_eye_and_prediction_use_identical_current_collision() {
    let (mut authority, shooter, _) = match_pair();
    wall_at(&mut authority, 30.0);
    let mut prediction = authority.clone();
    let input = TickInput {
        forward_move: 450.0,
        buttons: buttons::DUCK,
        ..tracked(Vec3::new(40.0, 0.0, 64.0), Vec3::X)
    };
    authority.step(&[(shooter, input)]);
    prediction.step_prediction(&[(shooter, input)]);
    let actor = authority.player(shooter).unwrap();
    assert_eq!(
        actor.aim.unwrap().origin,
        Vec3::Z * actor.movement.view_offset()
    );
    assert_eq!(actor.movement, prediction.player(shooter).unwrap().movement);
    assert_eq!(actor.aim, prediction.player(shooter).unwrap().aim);
    assert_eq!(
        actor.last_input,
        prediction.player(shooter).unwrap().last_input
    );
    assert!(prediction.events.is_empty());
}

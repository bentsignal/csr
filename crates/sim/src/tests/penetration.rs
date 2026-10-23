//! Functional policy regressions; these do not assert native damage parity.
use super::*;
use collision::{bullet_exit::BulletSurface, Convex, Plane};

fn pair() -> (World, PlayerId, PlayerId) {
    let mut world = world();
    let shooter = world.add_player("shooter", Team::Terrorist);
    let victim = world.add_player("behind cover", Team::CounterTerrorist);
    teleport(&mut world, shooter, Vec3::new(0., 0., 1.));
    teleport(&mut world, victim, Vec3::new(140., 0., 1.));
    let mut gun = def("weapon_test", 7, WeaponClass::Rifle, Side::Both, 0, 30);
    gun.damage = 20.;
    gun.penetration = 2.;
    world.players[0].weapons = vec![WeaponState::new(Arc::new(gun))];
    world.players[0].active = 0;
    world.players[1].armor = 0;
    world.players[1].helmet = false;
    (world, shooter, victim)
}

fn slab(world: &mut World, x: f32, thickness: f32, material: u8) {
    let collision = Arc::get_mut(&mut world.collision).unwrap();
    let surface = collision.surface_id(&format!("fixture/{material}/{x}"));
    collision.bullet_surfaces.resize(collision.surfaces.len(), None);
    collision.bullet_surfaces[surface as usize] = Some(BulletSurface {
        material, penetration_modifier: 0.9, damage_modifier: 0.6,
    });
    let mins = Vec3::new(x, -128., 0.);
    let maxs = Vec3::new(x + thickness, 128., 128.);
    collision.add_convex(Convex {
        mins, maxs, contents: contents::SOLID, surface, plane_surfaces: Vec::new(),
        planes: vec![Plane { normal: Vec3::X, dist: maxs.x }, Plane { normal: -Vec3::X, dist: -mins.x },
            Plane { normal: Vec3::Y, dist: maxs.y }, Plane { normal: -Vec3::Y, dist: -mins.y },
            Plane { normal: Vec3::Z, dist: maxs.z }, Plane { normal: -Vec3::Z, dist: -mins.z }],
    });
    collision.finalize();
}

fn damage(world: &World) -> i32 {
    world.events.iter().filter_map(|e| if let GameEvent::Damage { health, .. } = e { Some(*health) } else { None }).sum()
}

#[test]
fn thin_wood_reaches_target_with_loss_entry_feedback_and_one_ammo() {
    let (mut clear, _, _) = pair();
    let mut covered = clear.clone();
    // Drop the shared collision Arc before installing the fixture geometry.
    clear.fire(0, 1., None, None);
    let clear_damage = damage(&clear);
    drop(clear);
    slab(&mut covered, 50., 8., b'W');
    covered.fire(0, 1., None, None);
    assert!(damage(&covered) > 0 && damage(&covered) < clear_damage);
    assert_eq!(covered.players[0].weapons[0].clip, 29);
    assert_eq!(covered.players[0].shots, 1);
    assert_eq!(covered.events.iter().filter(|e| matches!(e, GameEvent::Fire { .. })).count(), 1);
    assert_eq!(covered.events.iter().filter(|e| matches!(e, GameEvent::Impact { .. })).count(), 1);
    assert_eq!(covered.events.iter().filter(|e| matches!(e, GameEvent::Tracer { .. })).count(), 1);
    assert!(covered.events.iter().any(|e| matches!(e, GameEvent::Tracer { start, end, .. }
        if start.x.abs() < 0.001 && end.x > 100.)));
}

#[test]
fn opaque_thick_unknown_zero_power_startsolid_and_backed_wood_stop() {
    for case in 0..11 {
        let (mut world, _, victim) = pair();
        match case {
            0 => slab(&mut world, 50., 8., b'C'),
            1 => slab(&mut world, 50., 40., b'W'),
            2 => { slab(&mut world, 50., 8., b'W'); Arc::get_mut(&mut world.collision).unwrap().bullet_surfaces.clear(); }
            3 => { slab(&mut world, 50., 8., b'W'); Arc::make_mut(&mut world.players[0].weapons[0].def).penetration = 0.; }
            4 => slab(&mut world, -4., 8., b'W'),
            5 => { slab(&mut world, 50., 8., b'W'); slab(&mut world, 54., 20., b'C'); }
            6 => { slab(&mut world, 50., 8., b'W'); slab(&mut world, 58., 20., b'C'); }
            7 => { slab(&mut world, 50., 8., b'W'); Arc::make_mut(&mut world.players[0].weapons[0].def).range = 55.; }
            8 => {
                slab(&mut world, 50., 8., b'W');
                let collision = Arc::get_mut(&mut world.collision).unwrap();
                let solid = collision.convexes.last_mut().unwrap();
                solid.plane_surfaces = vec![0, solid.surface, solid.surface, solid.surface, solid.surface, solid.surface];
            }
            9 => {
                slab(&mut world, 50., 8., b'W');
                *Arc::get_mut(&mut world.collision).unwrap().surfaces.last_mut().unwrap() = "tools/skybox".into();
            }
            _ => {
                slab(&mut world, 50., 8., b'W');
                Arc::get_mut(&mut world.collision).unwrap().bullet_surfaces.last_mut().unwrap().as_mut().unwrap().penetration_modifier = f32::NAN;
            }
        }
        world.fire(0, 1., None, None);
        assert_eq!(world.player(victim).unwrap().health, 100, "case {case}");
        assert_eq!(world.players[0].weapons[0].clip, 29);
    }
}

#[test]
fn shotgun_pellets_each_continue_but_share_one_fire_and_ammo_step() {
    let (mut world, _, victim) = pair();
    slab(&mut world, 50., 8., b'W');
    let def = Arc::make_mut(&mut world.players[0].weapons[0].def);
    def.damage = 5.;
    def.bullets = 5;
    world.fire(0, 1., None, None);
    assert!(world.player(victim).unwrap().alive && world.player(victim).unwrap().health < 100);
    for (count, expected) in [
        (world.events.iter().filter(|e| matches!(e, GameEvent::Damage { .. })).count(), 5),
        (world.events.iter().filter(|e| matches!(e, GameEvent::Impact { .. })).count(), 5),
        (world.events.iter().filter(|e| matches!(e, GameEvent::Fire { .. })).count(), 1),
    ] { assert_eq!(count, expected); }
    assert_eq!(world.players[0].weapons[0].clip, 29);
    assert_eq!(world.players[0].shots, 1);
}

#[test]
fn exhausted_pellets_cannot_gain_damage_from_the_health_minimum() {
    let (mut world, _, victim) = pair();
    slab(&mut world, 50., 8., b'W');
    Arc::make_mut(&mut world.players[0].weapons[0].def).damage = 1.;
    world.fire(0, 1., None, None);
    assert_eq!(world.player(victim).unwrap().health, 100);
}

#[test]
fn every_supported_material_and_separated_cover_layers() {
    for material in [b'W', b'U', b'L', b'Y', b'G'] {
        let (mut world, _, victim) = pair();
        slab(&mut world, 50., 4., material);
        slab(&mut world, 90., 4., material);
        world.fire(0, 1., None, None);
        assert!(world.player(victim).unwrap().health < 100);
        assert_eq!(world.events.iter().filter(|e| matches!(e, GameEvent::Impact { .. })).count(), 2);
        assert_eq!(world.events.iter().filter(|e| matches!(e, GameEvent::Tracer { .. })).count(), 1);
    }
}

#[test]
fn range_falloff_and_armor_remain_in_the_existing_damage_path() {
    let (mut near, _, _) = pair();
    slab(&mut near, 50., 8., b'W');
    let mut far = near.clone();
    let mut armored = near.clone();
    far.players[1].movement.origin.x = 2000.;
    armored.players[1].armor = 100;
    armored.players[1].helmet = true;
    near.fire(0, 1., None, None);
    far.fire(0, 1., None, None);
    armored.fire(0, 1., None, None);
    assert!(damage(&far) > 0 && damage(&far) < damage(&near));
    assert!(damage(&armored) > 0 && damage(&armored) < damage(&near));
    assert!(armored.players[1].armor < 100);
}

#[test]
fn bounded_four_exits_even_with_high_damage_and_high_weapon_power() {
    let (mut world, _, victim) = pair();
    let def = Arc::make_mut(&mut world.players[0].weapons[0].def);
    def.damage = 10000.;
    def.penetration = 100.;
    for x in [30., 45., 60., 75., 90.] { slab(&mut world, x, 2., b'W'); }
    world.fire(0, 1., None, None);
    assert_eq!(world.player(victim).unwrap().health, 100);
    assert_eq!(world.events.iter().filter(|e| matches!(e, GameEvent::Impact { .. })).count(), 5);
}

#[test]
fn tick_authority_prediction_and_snapshot_replay_agree_at_64_and_128() {
    for rate in [64, 128] {
        let (mut authority, shooter, victim) = pair();
        authority.set_tick_rate(rate);
        slab(&mut authority, 50., 8., b'W');
        let mut predicted = authority.clone();
        let mut replay = authority.clone();
        let snapshot = authority.snapshot();
        replay.restore(snapshot.state);
        let input = [(shooter, TickInput { buttons: buttons::ATTACK, ..Default::default() })];
        authority.step(&input);
        predicted.step_prediction(&input);
        replay.step(&input);
        assert!(authority.player(victim).unwrap().health < 100);
        assert_eq!(authority.state(), predicted.state());
        assert_eq!(authority.state(), replay.state());
        assert!(predicted.events.is_empty());
    }
}

#[test]
fn rewound_target_can_be_hit_after_cover_without_moving_live_target() {
    let (mut world, _, victim) = pair();
    slab(&mut world, 50., 8., b'W');
    world.configure_hit_history(128, 128.).unwrap();
    world.rewind.as_mut().unwrap().capture(0, world.round.round, &world.players, &world.character_hitboxes);
    world.players[1].movement.origin.y = 64.;
    world.tick = 1;
    world.rewind.as_mut().unwrap().capture(1, world.round.round, &world.players, &world.character_hitboxes);
    let mut current = world.clone();
    current.fire(0, 1., None, None);
    world.fire(0, 1., Some(0), None);
    assert_eq!(current.player(victim).unwrap().health, 100);
    assert!(world.player(victim).unwrap().health < 100);
    assert_eq!(world.players[1].movement.origin.y, 64.);
}

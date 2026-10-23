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
    slab_with_surface(world, x, thickness, declared_surface(material));
}

fn declared_surface(material: u8) -> BulletSurface {
    // Numeric declarations from installed Legacy scripts/surfaceproperties_cs.txt,
    // not measured native thickness or damage behavior. Other fixtures stay wood-like.
    let (penetration_modifier, damage_modifier) = match material {
        b'C' => (0.5, 0.25),
        b'M' => (0.4, 0.3),
        _ => (0.9, 0.6),
    };
    BulletSurface { material, penetration_modifier, damage_modifier }
}

fn opaque_slab(world: &mut World, x: f32, thickness: f32, material: u8) {
    slab_with_surface(world, x, thickness, BulletSurface {
        penetration_modifier: 0., ..declared_surface(material)
    });
}

fn slab_with_surface(world: &mut World, x: f32, thickness: f32, declaration: BulletSurface) {
    let collision = Arc::get_mut(&mut world.collision).unwrap();
    let material = declaration.material;
    let surface = collision.surface_id(&format!("fixture/{material}/{x}"));
    collision.bullet_surfaces.resize(collision.surfaces.len(), None);
    collision.bullet_surfaces[surface as usize] = Some(declaration);
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
            0 => opaque_slab(&mut world, 50., 8., b'C'),
            1 => slab(&mut world, 50., 40., b'W'),
            2 => { slab(&mut world, 50., 8., b'W'); Arc::get_mut(&mut world.collision).unwrap().bullet_surfaces.clear(); }
            3 => { slab(&mut world, 50., 8., b'W'); Arc::make_mut(&mut world.players[0].weapons[0].def).penetration = 0.; }
            4 => slab(&mut world, -4., 8., b'W'),
            5 => { slab(&mut world, 50., 8., b'W'); opaque_slab(&mut world, 54., 20., b'C'); }
            6 => { slab(&mut world, 50., 8., b'W'); opaque_slab(&mut world, 58., 20., b'C'); }
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
fn declared_concrete_and_metal_keep_existing_thickness_and_retention_policy() {
    for (material, expected_retention) in [(b'C', 0.125), (b'M', 0.1125)] {
        for (thickness, hidden) in [(8., false), (8., true), (20., false)] {
            let (mut world, _, _) = pair();
            if hidden {
                // Visible wood cannot conceal a weaker declared component.
                slab(&mut world, 50., thickness, b'W');
                slab(&mut world, 52., 2., material);
            } else {
                slab(&mut world, 50., thickness, material);
            }
            let start = Vec3::new(0., 0., 65.);
            let end = Vec3::new(140., 0., 65.);
            let hit = world.collision.trace_ray(start, end, contents::MASK_SHOT);
            let continuation = crate::penetration::through(&world.collision, start, end, &hit, 2.);
            if thickness == 8. {
                let (next_start, retention) = continuation.unwrap();
                assert!(next_start.x > 58. && next_start.x < 59.);
                // Existing 16 * power * modifier budget and linear retention,
                // explicitly an approximation rather than native damage parity.
                assert!((retention - expected_retention).abs() < 0.00001,
                    "material={material}, hidden={hidden}");
            } else {
                assert!(continuation.is_none(), "material={material}");
            }
        }
    }
}

#[test]
fn declared_materials_still_require_valid_modifiers_and_supported_codes() {
    for material in [b'C', b'M'] {
        let declared = declared_surface(material);
        for invalid in [
            BulletSurface { penetration_modifier: 0., ..declared },
            BulletSurface { penetration_modifier: -1., ..declared },
            BulletSurface { penetration_modifier: f32::NAN, ..declared },
            BulletSurface { penetration_modifier: f32::INFINITY, ..declared },
            BulletSurface { damage_modifier: 0., ..declared },
            BulletSurface { damage_modifier: -0.1, ..declared },
            BulletSurface { damage_modifier: 1.1, ..declared },
            BulletSurface { damage_modifier: f32::NAN, ..declared },
            BulletSurface { damage_modifier: f32::INFINITY, ..declared },
            BulletSurface { material: b'?', ..declared },
            BulletSurface { material: b'D', ..declared },
        ] {
            let (mut world, _, victim) = pair();
            slab_with_surface(&mut world, 50., 8., invalid);
            world.fire(0, 1., None, None);
            assert_eq!(world.player(victim).unwrap().health, 100, "{invalid:?}");
            assert_eq!(world.players[0].weapons[0].clip, 29);
        }
    }
}

#[test]
fn declared_cover_cannot_authorize_opaque_or_unknown_backing() {
    for material in [b'C', b'M'] {
        // Wholly hidden, overlapping and exactly touching backing all block.
        for (x, thickness) in [(52., 2.), (54., 6.), (58., 2.)] {
            for backing in 0..3 {
                let (mut world, _, victim) = pair();
                slab(&mut world, 50., 8., material);
                match backing {
                    0 => opaque_slab(&mut world, x, thickness, b'M'),
                    1 => slab(&mut world, x, thickness, b'?'),
                    _ => {
                        slab(&mut world, x, thickness, b'W');
                        *Arc::get_mut(&mut world.collision).unwrap().bullet_surfaces.last_mut().unwrap() = None;
                    }
                }
                world.fire(0, 1., None, None);
                assert_eq!(world.player(victim).unwrap().health, 100,
                    "material={material}, x={x}, backing={backing}");
                assert_eq!(world.players[0].weapons[0].clip, 29);
            }
        }
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
fn range_falloff_after_exit_cannot_revive_exhausted_pellets() {
    let (mut covered, _, victim) = pair();
    let def = Arc::make_mut(&mut covered.players[0].weapons[0].def);
    def.damage = 5.;
    // Synthetic falloff isolates exhaustion in free flight, not a native coefficient.
    def.range_modifier = 0.25;
    covered.players[1].movement.origin.x = 1000.;
    let mut clear = covered.clone();
    clear.fire(0, 1., None, None);
    assert!(damage(&clear) > 0);
    drop(clear);
    slab(&mut covered, 50., 8., b'W');
    let mut near = covered.clone();
    near.players[1].movement.origin.x = 140.;
    near.fire(0, 1., None, None);
    assert!(damage(&near) > 0); // The same pellet has budget at the exit and nearby hit.

    covered.fire(0, 1., None, None);
    assert_eq!(covered.player(victim).unwrap().health, 100);
    assert!(!covered.events.iter().any(|e| matches!(e, GameEvent::Damage { .. })));
    assert_eq!(covered.events.iter().filter(|e| matches!(e, GameEvent::Impact { .. })).count(), 1);
    assert_eq!(covered.events.iter().filter(|e| matches!(e, GameEvent::Tracer { .. })).count(), 1);
    assert_eq!(covered.events.iter().filter(|e| matches!(e, GameEvent::Fire { .. })).count(), 1);
    assert_eq!(covered.players[0].weapons[0].clip, 29);
    assert_eq!(covered.players[0].shots, 1);
}

#[test]
fn every_supported_material_and_separated_cover_layers() {
    for material in [b'W', b'U', b'L', b'Y', b'G', b'C', b'M'] {
        let (mut world, _, victim) = pair();
        // C/M retain less damage; keep both exits above the existing
        // sub-one-damage cutoff without changing the original wood fixtures.
        let thickness = if matches!(material, b'C' | b'M') { 1. } else { 4. };
        slab(&mut world, 50., thickness, material);
        slab(&mut world, 90., thickness, material);
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
        for material in [b'W', b'C', b'M'] {
            for connected in [false, true] {
                let (mut authority, shooter, victim) = pair();
                authority.set_tick_rate(rate);
                if connected {
                    slab(&mut authority, 50., 4., material);
                    slab(&mut authority, 54., 4., material);
                } else {
                    slab(&mut authority, 50., 8., material);
                }
                let mut predicted = authority.clone();
                let mut replay = authority.clone();
                let snapshot = authority.snapshot();
                replay.restore(snapshot.state);
                let input = [(shooter, TickInput { buttons: buttons::ATTACK, ..Default::default() })];
                authority.step(&input);
                predicted.step_prediction(&input);
                replay.step(&input);
                assert!(authority.player(victim).unwrap().health < 100,
                    "rate={rate}, material={material}, connected={connected}");
                assert_eq!(authority.state(), predicted.state());
                assert_eq!(authority.state(), replay.state());
                assert!(predicted.events.is_empty());
                assert_eq!(authority.players[0].weapons[0].clip, 29);
            }
        }
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

#[test]
fn connected_wood_matches_one_slab_and_keeps_tick_replay_at_both_rates() {
    for rate in [64, 128] {
        for overlap in [false, true] {
            let (mut joined, shooter, victim) = pair();
            joined.set_tick_rate(rate);
            slab(&mut joined, 50., 8., b'W');
            slab(&mut joined, if overlap { 54. } else { 58. }, if overlap { 12. } else { 8. }, b'W');
            let (mut single, single_shooter, _) = pair();
            single.set_tick_rate(rate);
            slab(&mut single, 50., 16., b'W');
            let mut prediction = joined.clone();
            let mut replay = joined.clone();
            replay.restore(joined.snapshot().state);
            let input = [(shooter, TickInput { buttons: buttons::ATTACK, ..Default::default() })];
            joined.step(&input);
            prediction.step_prediction(&input);
            replay.step(&input);
            single.step(&[(single_shooter, input[0].1)]);
            assert!(joined.player(victim).unwrap().health < 100);
            assert_eq!(damage(&joined), damage(&single), "rate={rate}, overlap={overlap}");
            assert_eq!(joined.state(), prediction.state());
            assert_eq!(joined.state(), replay.state());
            assert!(prediction.events.is_empty());
            assert_eq!(joined.events.iter().filter(|e| matches!(e, GameEvent::Impact { .. })).count(), 1);
            assert_eq!(joined.events.iter().filter(|e| matches!(e, GameEvent::Tracer { .. })).count(), 1);
            assert_eq!(joined.players[0].weapons[0].clip, 29);
        }
    }
}

#[test]
fn hidden_inner_faces_are_validated_and_can_limit_connected_cover() {
    let (mut ordinary, _, _) = pair();
    slab(&mut ordinary, 50., 16., b'W');
    ordinary.fire(0, 1., None, None);
    let ordinary_damage = damage(&ordinary);
    assert!(ordinary_damage > 0);
    for case in 0..7 {
        let (mut world, _, victim) = pair();
        slab(&mut world, 50., 16., b'W');
        slab(&mut world, 54., 4., b'W');
        let collision = Arc::get_mut(&mut world.collision).unwrap();
        let hidden = collision.convexes.last_mut().unwrap();
        // Both endpoints visible from outside stay wood. Change just one
        // hidden face, without changing the fallback or opposite face.
        hidden.plane_surfaces = vec![hidden.surface; 6];
        let internal = collision.bullet_surfaces.len() as u32;
        hidden.plane_surfaces[case % 2] = internal;
        let mut material = BulletSurface { material: b'W', penetration_modifier: 0.9, damage_modifier: 0.6 };
        match case {
            0 => material = BulletSurface { penetration_modifier: 0., ..declared_surface(b'C') },
            1 => material = BulletSurface { penetration_modifier: 0., ..declared_surface(b'M') },
            3 => material.penetration_modifier = f32::NAN,
            4 => material.penetration_modifier = 0.25,
            5 => material.damage_modifier = 0.2,
            6 => material.penetration_modifier = 0.6,
            _ => {}
        }
        collision.bullet_surfaces.push(if case == 2 { None } else { Some(material) });
        world.fire(0, 1., None, None);
        if case >= 5 {
            assert!(damage(&world) > 0 && damage(&world) < ordinary_damage, "case {case}");
        } else {
            assert_eq!(world.player(victim).unwrap().health, 100, "case {case}");
        }
        assert_eq!(world.players[0].weapons[0].clip, 29);
    }
}

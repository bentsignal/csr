use super::*;

#[test]
fn suppressor_completion_selects_separate_observed_rays_after_cache_and_snapshot() {
    for rate in [64, 128] {
        let (mut world, id) = fixture(rate);
        let mut definitions = world.catalog.weapons.clone();
        let gun = definitions.iter_mut().find(|def| def.class_name == "weapon_m4a1_silencer").unwrap();
        gun.full_auto = true;
        gun.accuracy = weapons::Accuracy::default();
        let profile = |reference: &str, angle: [f32; 2]| {
            let mut bullet_angles = vec![angle; 20];
            bullet_angles[0] = [0., 0.];
            weapons::RecoilTrajectory { reference: reference.into(), bullet_angles, response: None }
        };
        gun.recoil_trajectory = Some(profile("synthetic-attached", [-2., 0.5]));
        gun.recoil_trajectory_unsuppressed = Some(profile("synthetic-detached", [-4., -0.5]));
        world.player_mut(id).unwrap().active_weapon_mut().unwrap().set_def(Arc::new(gun.clone()));
        world.catalog = Arc::new(weapons::Catalog { weapons: definitions });
        let fire = TickInput { buttons: buttons::ATTACK, ..Default::default() };
        for (mode, expected_angle) in [(true, [-2., 0.5]), (false, [-4., -0.5]), (true, [-2., 0.5])] {
            let before = world.player(id).unwrap().shots;
            let mut second_ray = None;
            for _ in 0..world.ticks(2.) {
                world.step(&[(id, fire)]);
                if world.player(id).unwrap().shots == before + 2 {
                    second_ray = world.events.iter().find_map(|event| match event {
                        GameEvent::Tracer { start, end, .. } => Some((*end - *start).normalize()),
                        _ => None,
                    });
                    break;
                }
            }
            assert_eq!(world.player(id).unwrap().active_weapon().unwrap().suppressor_attached, mode);
            let expected = movement::angle_vectors(Vec3::new(expected_angle[0], expected_angle[1], 0.)).0;
            assert!((second_ray.expect("two real bullets") - expected).length() < 0.0001);
            if before == 4 { break; }
            for _ in 0..world.ticks(1.) { world.step(&[(id, TickInput::default())]); }
            world.step(&[(id, TickInput { buttons: buttons::ATTACK2, ..Default::default() })]);
            assert!(world.player(id).unwrap().active_weapon().unwrap().suppressor_transition.is_some());
            let snapshot = world.decode::<Snapshot>(&state::encode(&world.snapshot())).unwrap();
            let mut restored = world.clone();
            restored.restore(snapshot.state);
            for _ in 0..world.ticks(0.6) {
                world.step(&[(id, TickInput::default())]);
                restored.step_prediction(&[(id, TickInput::default())]);
                assert_eq!(world.state_hash(), restored.state_hash());
            }
            assert_eq!(world.player(id).unwrap().active_weapon().unwrap().aim_punch(),
                restored.player(id).unwrap().active_weapon().unwrap().aim_punch());
        }
    }
}

fn fixture(rate: u32) -> (World, PlayerId) {
    let mut catalog = catalog();
    let mut gun = def(
        "weapon_m4a1_silencer",
        60,
        WeaponClass::Rifle,
        Side::CounterTerrorist,
        2900,
        20,
    );
    gun.attributes.insert("has silencer".into(), "1".into());
    gun.suppressor_animations = Some(weapons::SuppressorAnimations {
        attach: weapons::SuppressorAnimation {
            duration: 0.5,
            complete_time: 0.25,
            visibility: vec![(0., false), (0.125, true)],
        },
        detach: weapons::SuppressorAnimation {
            duration: 0.5,
            complete_time: 0.25,
            visibility: vec![(0., true), (0.375, false)],
        },
        draw_attached: "ACT_VM_DRAW_SILENCED".into(),
        draw_detached: "ACT_VM_DRAW".into(),
    });
    catalog.weapons.push(gun);
    let mut world = World::with_catalog(Arc::new(CollisionWorld::new()), Arc::new(catalog));
    world.set_tick_rate(rate);
    let id = world.add_player("suppressor player", Team::CounterTerrorist);
    let gun = WeaponState::new(Arc::new(
        world.catalog.get("weapon_m4a1_silencer").unwrap().clone(),
    ));
    let knife = WeaponState::new(Arc::new(world.catalog.get("weapon_knife").unwrap().clone()));
    let p = world.player_mut(id).unwrap();
    p.weapons = vec![gun, knife];
    p.active = 0;
    (world, id)
}

#[test]
fn suppressor_hold_blocks_fire_reload_and_repeats_and_replays_from_snapshot() {
    for rate in [64, 128] {
        let (mut world, id) = fixture(rate);
        assert!(
            world.players[0]
                .active_weapon()
                .unwrap()
                .suppressor_attached
        );
        let right = TickInput {
            buttons: buttons::ATTACK2,
            ..Default::default()
        };
        world.step(&[(id, right)]);
        let start = world.players[0]
            .active_weapon()
            .unwrap()
            .suppressor_transition
            .unwrap();
        assert!(!start.attached);
        let snapshot = world
            .decode::<Snapshot>(&state::encode(&world.snapshot()))
            .unwrap();
        let mut restored = world.clone();
        restored.restore(snapshot.state);
        let mut predicted = world.clone();
        let held = TickInput {
            buttons: buttons::ATTACK2 | buttons::ATTACK | buttons::RELOAD,
            ..Default::default()
        };
        while world.tick < rate as u64 / 2 {
            world.step(&[(id, held)]);
            restored.step(&[(id, held)]);
            predicted.step_prediction(&[(id, held)]);
            assert_eq!(world.state_hash(), restored.state_hash());
            assert_eq!(world.state_hash(), predicted.state_hash());
            assert!(
                !world
                    .events
                    .iter()
                    .any(|e| matches!(e, GameEvent::Fire { .. } | GameEvent::Reload { .. }))
            );
            let gun = world.players[0].active_weapon().unwrap();
            assert_eq!(gun.suppressor_transition, Some(start));
            if world.tick > rate as u64 / 4 {
                assert!(!gun.suppressor_attached);
            }
        }
        world.step(&[(id, right)]); // full clip has finished; held right must not toggle again
        let gun = world.players[0].active_weapon().unwrap();
        assert!(gun.suppressor_transition.is_none());
        assert!(!gun.suppressor_attached);
        world.step(&[(
            id,
            TickInput {
                buttons: buttons::ATTACK,
                ..Default::default()
            },
        )]);
        assert!(world.events.iter().any(|e| matches!(
            e,
            GameEvent::Fire {
                suppressed: false,
                ..
            }
        )));
        while world.players[0].active_weapon().unwrap().next_attack > world.time() {
            world.step(&[(id, TickInput::default())]);
        }
        world.step(&[(id, right)]);
        assert!(
            world.players[0]
                .active_weapon()
                .unwrap()
                .suppressor_transition
                .unwrap()
                .attached
        );
    }
}

#[test]
fn completed_suppressor_mode_survives_drop_pickup_and_incomplete_holster_cancels() {
    let (mut world, id) = fixture(128);
    let right = TickInput {
        buttons: buttons::ATTACK2,
        ..Default::default()
    };
    world.step(&[(id, right)]);
    world.step(&[(
        id,
        TickInput {
            weapon_slot: 3,
            ..Default::default()
        },
    )]);
    assert!(world.players[0].weapons[0].suppressor_transition.is_none());
    assert!(world.players[0].weapons[0].suppressor_attached);
    let time = world.time();
    world.players[0].select(0, time);
    world.players[0].weapons[0].next_attack = 0.;
    world.step(&[(id, right)]);
    let completion = world.tick + 32;
    while world.tick <= completion {
        world.step(&[(id, TickInput::default())]);
    }
    assert!(!world.players[0].weapons[0].suppressor_attached);
    world.step(&[(
        id,
        TickInput {
            drop: true,
            ..Default::default()
        },
    )]);
    let item = world
        .items
        .iter()
        .find(|i| i.class_name() == "weapon_m4a1_silencer")
        .unwrap();
    let GroundKind::Weapon(gun) = &item.kind else {
        unreachable!()
    };
    assert!(!gun.suppressor_attached);
    assert!(gun.suppressor_transition.is_none());
    let pos = item.pos;
    let pickup_after = item.pickup_after;
    world.players[0].movement.origin = pos;
    world.tick = (pickup_after * world.tick_rate as f32).ceil() as u64;
    world.players[0].movement.velocity = Vec3::ZERO;
    world.step(&[(id, TickInput::default())]);
    assert!(
        !world.players[0]
            .weapons
            .iter()
            .find(|w| w.def.class_name == "weapon_m4a1_silencer")
            .unwrap()
            .suppressor_attached
    );
}

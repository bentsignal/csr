use super::*;

fn fixture(rate: u32) -> (World, PlayerId) {
    let mut gun = def("weapon_pack_empty", 991, WeaponClass::Rifle, Side::Both, 0, 30);
    gun.empty_trigger = Some(weapons::EmptyTrigger {
        sound: "Pack.Empty".into(), repeat_interval: 0.2, reference: "explicit-test-content".into(),
    });
    gun.reserve_ammo = 0;
    let mut catalog = catalog(); catalog.weapons.push(gun.clone());
    let mut world = World::with_catalog(Arc::new(CollisionWorld::new()), Arc::new(catalog));
    world.set_tick_rate(rate);
    let id = world.add_player("empty", Team::Terrorist);
    world.players[0].weapons = vec![WeaponState::new(Arc::new(gun))];
    world.players[0].active = 0; world.players[0].weapons[0].clip = 0;
    (world, id)
}

fn held() -> TickInput { TickInput { buttons: buttons::ATTACK, ..Default::default() } }

#[test]
fn held_depleted_trigger_repeats_without_shots_and_replays_remaining_cooldown() {
    for rate in [64, 128] {
        let (mut world, id) = fixture(rate);
        world.step(&[(id, held())]);
        assert_eq!(world.events.iter().filter(|e| matches!(e, GameEvent::EmptyTrigger { .. })).count(), 1);
        let snapshot = world.decode::<Snapshot>(&state::encode(&world.snapshot())).unwrap();
        let mut restored = world.clone(); restored.restore(snapshot.state);
        let mut predicted = world.clone();
        let mut clicks = vec![0];
        for tick in 1..rate * 3 / 2 {
            world.step(&[(id, held())]);
            restored.step(&[(id, held())]);
            predicted.step_prediction(&[(id, held())]);
            assert_eq!(world.state_hash(), restored.state_hash());
            assert_eq!(world.state_hash(), predicted.state_hash());
            assert!(predicted.events.is_empty());
            for event in &world.events {
                assert!(!matches!(event, GameEvent::Fire { .. } | GameEvent::Tracer { .. }
                    | GameEvent::Impact { .. } | GameEvent::Damage { .. } | GameEvent::Reload { .. }));
                if let GameEvent::EmptyTrigger { player, weapon, origin } = event {
                    assert_eq!((*player, weapon.as_str()), (id, "weapon_pack_empty"));
                    assert_eq!(*origin, world.players[0].eye()); clicks.push(tick);
                }
            }
        }
        assert_eq!(clicks.len(), 8);
        for gap in clicks.windows(2).map(|pair| (pair[1] - pair[0]) as f32 / rate as f32) {
            assert!(gap >= 0.2 && gap < 0.2 + world.dt() + 0.00001);
        }
        let p = &world.players[0]; let w = &p.weapons[0];
        assert_eq!((p.shots, w.clip, w.reserve), (0, 0, 0));
        assert_eq!((w.recoil_index, w.accuracy_penalty), (0., 0.));
        for _ in 0..rate { world.step(&[(id, TickInput::default())]);
            assert!(!world.events.iter().any(|e| matches!(e, GameEvent::EmptyTrigger { .. }))); }
    }
}

#[test]
fn empty_feedback_respects_readiness_and_requires_explicit_content_policy() {
    for rate in [64, 128] { for case in 0..7 {
        let (mut world, id) = fixture(rate);
        match case {
            0 => world.players[0].weapons[0].reserve = 90,
            1 => world.players[0].weapons[0].next_attack = 10.,
            2 => world.players[0].weapons[0].reload_end = Some(10.),
            3 => { world.mode = GameMode::Competitive; world.round.phase = Phase::Freeze; world.round.live_start = 10.; }
            4 => Arc::make_mut(&mut world.players[0].weapons[0].def).empty_trigger = None,
            5 => Arc::make_mut(&mut world.players[0].weapons[0].def).empty_trigger.as_mut().unwrap().repeat_interval = f32::NAN,
            _ => world.players[0].weapons[0].clip = 1,
        }
        world.step(&[(id, held())]);
        assert!(!world.events.iter().any(|e| matches!(e, GameEvent::EmptyTrigger { .. })), "case{case}");
        if case == 0 { assert!(world.players[0].weapons[0].reload_end.is_some()); }
        if case == 6 { assert_eq!(world.players[0].shots, 1); }
    } }
}

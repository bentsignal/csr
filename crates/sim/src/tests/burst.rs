use super::*;

fn fixture(rate: u32, automatic: bool) -> (World, PlayerId) {
    let mut gun = def("weapon_pack_burst", 900, WeaponClass::Rifle, Side::Both, 1000, 20);
    gun.full_auto = automatic;
    gun.cycle_time = 0.09;
    for (key, value) in [("has burst mode", "1"),
        ("cycletime when in burst mode", if automatic { "0.55" } else { "0.5" }),
        ("time between burst shots", if automatic { "0.075" } else { "0.05" }),
        ("cycletime alt", "0.3")] { gun.attributes.insert(key.into(), value.into()); }
    let mut catalog = catalog();
    catalog.weapons.push(gun.clone());
    let mut world = World::with_catalog(Arc::new(CollisionWorld::new()), Arc::new(catalog));
    world.set_tick_rate(rate);
    let id = world.add_player("burst player", Team::CounterTerrorist);
    let knife = world.catalog.get("weapon_knife").unwrap().clone();
    world.players[0].weapons = vec![WeaponState::new(Arc::new(gun)), WeaponState::new(Arc::new(knife))];
    world.players[0].active = 0;
    (world, id)
}

fn input(buttons: u32) -> TickInput { TickInput { buttons, ..Default::default() } }

#[test]
fn three_round_release_cadence_ammo_and_pending_snapshot_replay() {
    for rate in [64, 128] { for automatic in [false, true] {
        let (mut world, id) = fixture(rate, automatic);
        world.step(&[(id, input(buttons::ATTACK2))]);
        world.step(&[(id, input(buttons::ATTACK2))]);
        assert!(world.players[0].weapons[0].burst_mode, "held secondary does not toggle twice");
        let first = world.time();
        world.step(&[(id, input(buttons::ATTACK))]);
        assert_eq!(world.players[0].shots, 1);
        let snapshot = world.decode::<Snapshot>(&state::encode(&world.snapshot())).unwrap();
        let mut restored = world.clone(); restored.restore(snapshot.state);
        let mut predicted = world.clone();
        let profile = world.players[0].weapons[0].burst_profile().unwrap();
        let mut times = vec![first];
        while world.time() < first + profile.cycle + 0.1 {
            let time = world.time();
            world.step(&[(id, TickInput::default())]);
            restored.step(&[(id, TickInput::default())]);
            predicted.step_prediction(&[(id, TickInput::default())]);
            assert_eq!(world.state_hash(), restored.state_hash());
            assert_eq!(world.state_hash(), predicted.state_hash());
            for event in &world.events { if matches!(event, GameEvent::Fire { player, .. } if *player == id) { times.push(time); } }
        }
        assert_eq!(times.len(), 3);
        for (shot, time) in times.iter().enumerate() {
            let intended = first + shot as f32 * profile.interval;
            assert!(*time >= intended - 0.00001 && *time < intended + world.dt() + 0.00001, "{times:?}");
        }
        let gun = &world.players[0].weapons[0];
        assert_eq!(gun.clip, 17); assert!(gun.burst.is_none());
        assert!((gun.next_attack - first - profile.cycle).abs() < 0.00001);
    } }
}

#[test]
fn held_burst_is_one_cycle_and_switching_back_restores_automatic_fire() {
    let (mut world, id) = fixture(128, true);
    world.step(&[(id, input(buttons::ATTACK2))]);
    for _ in 0..128 { world.step(&[(id, input(buttons::ATTACK))]); }
    assert_eq!(world.players[0].shots, 3);
    world.step(&[(id, input(buttons::ATTACK2))]);
    assert!(!world.players[0].weapons[0].burst_mode);
    for _ in 0..64 { world.step(&[(id, input(buttons::ATTACK))]); }
    assert!(world.players[0].shots > 6);
}

#[test]
fn partial_clip_and_holster_drop_death_cannot_emit_queued_offhand_bullets() {
    for clip in [1, 2] {
        let (mut world, id) = fixture(128, false);
        world.players[0].weapons[0].clip = clip;
        world.step(&[(id, input(buttons::ATTACK2))]);
        world.step(&[(id, input(buttons::ATTACK))]);
        for _ in 0..32 { world.step(&[(id, TickInput::default())]); }
        assert_eq!(world.players[0].shots, clip as u64);
        assert_eq!(world.players[0].weapons[0].clip, 0);
        assert!(world.players[0].weapons[0].burst.is_none());
    }
    for action in 0..3 {
        let (mut world, id) = fixture(128, false);
        world.step(&[(id, input(buttons::ATTACK2))]);
        world.step(&[(id, input(buttons::ATTACK))]);
        match action {
            0 => world.step(&[(id, TickInput { weapon_slot: 3, ..Default::default() })]),
            1 => world.step(&[(id, TickInput { drop: true, ..Default::default() })]),
            _ => world.damage_environment(id, 1000, "test"),
        }
        for _ in 0..32 { world.step(&[(id, TickInput::default())]); }
        assert_eq!(world.players[0].shots, 1);
        assert!(world.players[0].weapons.iter().all(|w| w.burst.is_none()));
        assert!(world.items.iter().all(|item| match &item.kind { GroundKind::Weapon(w) => w.burst.is_none(), _ => true }));
    }
}

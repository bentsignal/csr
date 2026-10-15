use super::*;

fn rifle(rate: u32) -> (World, PlayerId) {
    let mut gun = def("weapon_pack_automatic", 901, WeaponClass::Rifle, Side::Both, 1000, 30);
    gun.full_auto = true;
    gun.cycle_time = 0.1;
    gun.reserve_ammo = 0;
    equipped(rate, gun)
}

fn equipped(rate: u32, gun: WeaponDefinition) -> (World, PlayerId) {
    let mut catalog = catalog();
    catalog.weapons.retain(|existing| existing.id != gun.id);
    catalog.weapons.push(gun.clone());
    let mut world = World::with_catalog(Arc::new(CollisionWorld::new()), Arc::new(catalog));
    world.set_tick_rate(rate);
    let id = world.add_player("automatic", Team::CounterTerrorist);
    world.players[0].weapons = vec![WeaponState::new(Arc::new(gun))];
    world.players[0].active = 0;
    (world, id)
}

#[test]
fn normal_ak_and_m4_reload_released_empty_magazines_and_replay_from_snapshot() {
    for rate in [64, 128] { for (name, index, cycle) in [("weapon_ak47", 7, 0.1), ("weapon_m4a1", 16, 0.09)] {
        let mut gun = def(name, index, WeaponClass::Rifle, Side::Both, 1000, 30);
        gun.full_auto = true; gun.cycle_time = cycle; gun.reserve_ammo = 60;
        let (mut world, id) = equipped(rate, gun);
        world.players[0].weapons[0].clip = 1;
        for _ in 0..rate { world.step(&[(id, held())]); }
        let weapon = &world.players[0].weapons[0];
        assert_eq!((world.players[0].shots, weapon.clip, weapon.reserve), (1, 0, 60));
        assert!(weapon.reload_end.is_none(), "held empty must not begin automatic reload");

        let snapshot = world.decode::<Snapshot>(&state::encode(&world.snapshot())).unwrap();
        let mut restored = world.clone(); restored.restore(snapshot.state);
        let mut predicted = world.clone();
        world.step(&[(id, TickInput::default())]);
        restored.step(&[(id, TickInput::default())]);
        predicted.step_prediction(&[(id, TickInput::default())]);
        assert!(world.players[0].weapons[0].reload_end.is_some());
        assert_eq!(world.events.iter().filter(|event| matches!(event,
            GameEvent::Reload { player } if *player == id)).count(), 1);
        for _ in 0..rate * 4 {
            assert_eq!(world.state_hash(), restored.state_hash());
            assert_eq!(world.state_hash(), predicted.state_hash());
            world.step(&[(id, TickInput::default())]);
            restored.step(&[(id, TickInput::default())]);
            predicted.step_prediction(&[(id, TickInput::default())]);
        }
        let weapon = &world.players[0].weapons[0];
        assert_eq!((weapon.clip, weapon.reserve), (30, 30));
        assert!(weapon.reload_end.is_none());

        // Preserve the prior fallback for a new press on an already empty
        // weapon, independently of a continuous empty trigger above.
        world.players[0].weapons[0].clip = 0;
        world.step(&[(id, held())]);
        assert!(world.players[0].weapons[0].reload_end.is_some());
        assert_eq!(world.players[0].shots, 1);
    } }
}

fn held() -> TickInput { TickInput { buttons: buttons::ATTACK, ..Default::default() } }

#[test]
fn sustained_actual_fire_spans_one_authored_magazine_without_accumulating_tick_ceil() {
    for rate in [64, 128] {
        let (mut world, id) = rifle(rate);
        let mut shots = Vec::new();
        for _ in 0..rate * 4 {
            let tick = world.tick;
            world.step(&[(id, held())]);
            let actual = world.events.iter().filter(|event| matches!(event,
                GameEvent::Fire { player, .. } if *player == id)).count();
            assert!(actual <= 1, "no automatic catch-up volley on one tick");
            if actual == 1 { shots.push(tick); }
        }
        assert_eq!(shots.len(), 30);
        let span = (shots[29] - shots[0]) as f32 / rate as f32;
        assert!(span >= 2.9 - 0.00001 && span < 2.9 + world.dt() + 0.00001, "{rate}Hz: {span}s");
        assert_eq!(world.players[0].weapons[0].clip, 0);
    }
}

#[test]
fn admitted_new_press_and_stale_deadline_reset_without_catchup() {
    for rate in [64, 128] {
        let (mut world, id) = rifle(rate);
        world.step(&[(id, held())]);
        let deadline = world.players[0].weapons[0].next_attack;
        while world.time() < deadline { world.step(&[(id, TickInput::default())]); }
        let fired = world.time();
        world.step(&[(id, held())]);
        assert_eq!(world.players[0].shots, 2);
        assert!((world.players[0].weapons[0].next_attack - fired - 0.1).abs() < 0.00001);

        // A missed simulation interval cannot repay a backlog of old shots.
        world.tick += rate as u64;
        let fired = world.time();
        world.step(&[(id, held())]);
        assert_eq!(world.players[0].shots, 3);
        assert!((world.players[0].weapons[0].next_attack - fired - 0.1).abs() < 0.00001);
    }
}

#[test]
fn snapshot_prediction_retains_automatic_deadline_phase() {
    for rate in [64, 128] {
        let (mut authority, id) = rifle(rate);
        while authority.players[0].shots < 7 { authority.step(&[(id, held())]); }
        let snapshot = authority.decode::<Snapshot>(&state::encode(&authority.snapshot())).unwrap();
        let mut restored = authority.clone(); restored.restore(snapshot.state);
        let mut predicted = authority.clone();
        for _ in 0..rate * 3 {
            authority.step(&[(id, held())]);
            restored.step(&[(id, held())]);
            predicted.step_prediction(&[(id, held())]);
            assert_eq!(authority.state_hash(), restored.state_hash());
            assert_eq!(authority.state_hash(), predicted.state_hash());
        }
        assert_eq!(authority.players[0].shots, 30);
    }
}

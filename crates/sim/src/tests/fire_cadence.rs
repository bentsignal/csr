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
fn magazine_reload_holster_cancels_and_completed_reload_preserves_trigger_mode() {
    for rate in [64, 128] { for automatic in [false, true] {
        let class = if automatic { WeaponClass::Rifle } else { WeaponClass::Pistol };
        let mut gun = def("weapon_pack_magazine", 902, class, Side::Both, 1000, 6);
        gun.full_auto = automatic;
        gun.reserve_ammo = 12;
        let (mut world, id) = equipped(rate, gun);
        // Keep another slot available without changing the selected gun.
        let other = world.catalog.get(if automatic { "weapon_hkp2000" } else { "weapon_m4a1" }).unwrap().clone();
        let other_slot = weapon::slot(&other);
        world.players[0].weapons.push(WeaponState::new(Arc::new(other)));
        world.players[0].weapons[0].clip = 2;
        let reload = TickInput { buttons: buttons::RELOAD, ..Default::default() };
        world.step(&[(id, reload)]);
        let cancelled_end = world.players[0].weapons[0].reload_end.unwrap();
        assert!(world.events.iter().any(|event| matches!(event,
            GameEvent::Reload { player } if *player == id)));

        world.step(&[(id, TickInput { weapon_slot: other_slot, ..Default::default() })]);
        assert_eq!(world.players[0].active, 1);
        // Wait beyond the abandoned deadline: a holstered magazine must
        // neither finish loading nor consume any reserve ammunition.
        while world.time() <= cancelled_end + world.dt() {
            world.step(&[(id, TickInput::default())]);
        }
        let gun = &world.players[0].weapons[0];
        assert_eq!((gun.clip, gun.reserve, gun.reload_end), (2, 12, None));
        world.step(&[(id, TickInput { weapon_slot: SLOT_LAST, ..Default::default() })]);
        assert_eq!(world.players[0].active, 0);
        let ready = world.players[0].weapons[0].next_attack;
        while world.time() < ready { world.step(&[(id, TickInput::default())]); }

        world.step(&[(id, reload)]);
        let end = world.players[0].weapons[0].reload_end.unwrap();
        while world.players[0].weapons[0].reload_end.is_some() {
            assert!(world.time() <= end + world.dt(), "reload must finish at its deadline");
            world.step(&[(id, held())]);
            assert_eq!(world.players[0].shots, 0, "no magazine-reload interruption by firing");
        }
        let gun = &world.players[0].weapons[0];
        assert_eq!((gun.clip, gun.reserve, gun.reload_end), (6, 8, None));
        for _ in 0..world.ticks(0.25) { world.step(&[(id, held())]); }
        let shots = world.players[0].shots;
        if automatic {
            assert!(shots >= 2, "{rate} Hz: held automatic trigger must resume sustained fire");
        } else {
            assert_eq!(shots, 0, "{rate} Hz: a held pistol trigger needs a new press");
        }
        let gun = &world.players[0].weapons[0];
        assert_eq!((gun.clip, gun.reserve), (6 - shots as i32, 8));

        // A released/repressed trigger works after either reload path.
        let ready = world.players[0].weapons[0].next_attack;
        world.step(&[(id, TickInput::default())]);
        while world.time() < ready { world.step(&[(id, TickInput::default())]); }
        world.step(&[(id, held())]);
        assert_eq!(world.players[0].shots, shots + 1);
    } }
}

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

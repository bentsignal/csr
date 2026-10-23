use super::*;

fn fixture() -> (World, PlayerId, PlayerId) {
    let mut w = world();
    let shooter = w.add_player("shooter", Team::Terrorist);
    let victim = w.add_player("moving target", Team::CounterTerrorist);
    w.players[0].movement = PlayerMove::spawn_at(Vec3::ZERO);
    w.players[0].view_angles = Vec3::ZERO;
    let gun = Arc::new(w.catalog.get("weapon_ak47").unwrap().clone());
    w.players[0].weapons = vec![WeaponState::new(gun)];
    w.players[0].active = 0;
    w.players[1].movement = PlayerMove::spawn_at(Vec3::X * 100.);
    w.players[1].armor = 0;
    w.players[1].helmet = false;
    w.configure_hit_history(128, 128.).unwrap();
    capture(&mut w, 0);
    w.players[1].movement.origin.y = 64.;
    capture(&mut w, 1);
    (w, shooter, victim)
}
fn capture(w: &mut World, tick: u64) {
    w.tick = tick;
    w.rewind
        .as_mut()
        .unwrap()
        .capture(tick, w.round.round, &w.players, &w.character_hitboxes);
}

#[test]
fn historical_hits_damage_current_life_without_moving_live_players() {
    let (mut authority, _, victim) = fixture();
    let mut current = authority.clone();
    current.fire(0, 1., None, None);
    assert_eq!(current.player(victim).unwrap().health, 100);
    let positions: Vec<_> = authority
        .players
        .iter()
        .map(|p| p.movement.origin)
        .collect();
    authority.fire(0, 1., Some(0), None);
    assert!(authority.player(victim).unwrap().health < 100);
    assert_eq!(
        authority
            .players
            .iter()
            .map(|p| p.movement.origin)
            .collect::<Vec<_>>(),
        positions
    );
    assert!(authority
        .events
        .iter()
        .any(|e| matches!(e, GameEvent::Damage { .. })));
    assert_eq!(current.player(victim).unwrap().health, 100);
}

#[test]
fn authoritative_tick_input_selects_history_but_prediction_uses_current_targets() {
    let (mut authority, shooter, victim) = fixture();
    let mut predicted = authority.clone();
    let input = TickInput {
        buttons: buttons::ATTACK,
        rewind_tick: Some(0),
        ..Default::default()
    };
    authority.step(&[(shooter, input)]);
    predicted.step_prediction(&[(shooter, input)]);
    assert!(authority.player(victim).unwrap().health < 100);
    assert_eq!(predicted.player(victim).unwrap().health, 100);
    assert!(predicted.events.is_empty());
}

#[test]
fn historical_hits_respect_current_map_walls() {
    let (mut w, _, victim) = fixture();
    wall_at(&mut w, 50.);
    w.fire(0, 1., Some(0), None);
    assert_eq!(w.player(victim).unwrap().health, 100);
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, GameEvent::Impact { .. })));
}

#[test]
fn future_expired_life_team_round_and_teleport_requests_use_current_hulls() {
    for case in 0..7 {
        let (mut w, _, victim) = fixture();
        let mut requested = 0;
        match case {
            0 => requested = u64::MAX,
            1 => capture(&mut w, 129),
            2 => {
                w.respawn(victim);
                w.players[1].movement.origin = Vec3::new(100., 64., 0.);
            }
            3 => w.players[1].team = Team::Terrorist,
            4 => w.round.round += 1,
            5 => {
                w.players[1].movement.origin.y = 512.;
                capture(&mut w, 2);
            }
            _ => {
                w.players[1].alive = false;
                capture(&mut w, 2);
                w.players[1].alive = true;
            }
        }
        w.fire(0, 1., Some(requested), None);
        assert_eq!(w.player(victim).unwrap().health, 100, "case {case}");
    }
}

#[test]
fn prediction_bots_and_repeated_controls_never_reuse_a_historical_hit() {
    for case in 0..3 {
        let (mut w, shooter, victim) = fixture();
        let input = TickInput {
            buttons: buttons::ATTACK,
            rewind_tick: Some(0),
            ..Default::default()
        };
        if case == 0 {
            w.step_prediction(&[(shooter, input)]);
        } else if case == 1 {
            w.players[0].is_bot = true;
            w.step(&[(shooter, input)]);
        } else {
            w.players[0].last_input = input;
            w.step(&[]);
        }
        assert_eq!(w.player(victim).unwrap().health, 100, "case {case}");
        assert_eq!(w.player(shooter).unwrap().last_input.rewind_tick, None);
        if case == 0 {
            assert!(w.events.is_empty());
        }
    }
}

#[test]
fn life_epoch_survives_replication_and_every_placement_advances_it() {
    let (mut w, _, victim) = fixture();
    let epoch = w.player(victim).unwrap().life_epoch;
    w.respawn(victim);
    assert_eq!(w.player(victim).unwrap().life_epoch, epoch + 1);
    w.spawn_for_round(victim, true);
    assert_eq!(w.player(victim).unwrap().life_epoch, epoch + 2);
    let state = w.state();
    let decoded: WorldState = w.decode(&w.encode_state()).unwrap();
    assert_eq!(decoded, state);
    let mut restored = World::with_catalog(w.collision.clone(), w.catalog.clone());
    restored.restore(decoded);
    assert_eq!(restored.player(victim).unwrap().life_epoch, epoch + 2);
    assert!(restored.oldest_hit_tick().is_none());
}

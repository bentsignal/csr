use super::*;

fn right() -> TickInput {
    TickInput { buttons: buttons::ATTACK2, ..Default::default() }
}

fn scope_levels(world: &World) -> Vec<u8> {
    world.events.iter().filter_map(|event| match event {
        GameEvent::Scope { level, .. } => Some(*level),
        _ => None,
    }).collect()
}

#[test]
fn held_scope_repeats_on_first_ready_tick_without_firing_at_64_and_128_hz() {
    // The existing 0.3s secondary policy is deliberately retained. This is
    // a held-input regression, not a measurement of native zoom cadence.
    for rate in [64, 128] {
        let (mut world, id) = scoped_world();
        world.set_tick_rate(rate);
        let reserve = world.player(id).unwrap().weapons[0].reserve;
        let mut due = 0.;
        let mut levels = Vec::new();
        for _ in 0..rate * 2 {
            let now = world.time();
            world.step(&[(id, right())]);
            let changes = scope_levels(&world);
            assert_eq!(changes.len(), usize::from(now >= due), "rate={rate}, time={now}, due={due}");
            let w = world.player(id).unwrap().active_weapon().unwrap();
            if !changes.is_empty() {
                assert!(now < due + world.dt());
                levels.extend(changes);
                due = now + 0.3;
            }
            assert_eq!(w.next_secondary_attack, due);
            assert_eq!((w.next_attack, w.resume_zoom, w.clip, w.reserve), (0., None, 5, reserve));
            assert!(!world.events.iter().any(|event| matches!(event,
                GameEvent::Fire { .. } | GameEvent::Tracer { .. } | GameEvent::Reload { .. })));
        }
        assert_eq!(levels, vec![1, 2, 0, 1, 2, 0, 1]);
    }
}

#[test]
fn releasing_scope_discards_an_early_tap_and_stops_repetition() {
    for rate in [64, 128] {
        let (mut world, id) = scoped_world();
        world.set_tick_rate(rate);
        world.step(&[(id, right())]);
        let due = world.player(id).unwrap().weapons[0].next_secondary_attack;
        // Release, then tap on the last tick before readiness. The released
        // command must not leave a queued scope action at the deadline.
        while world.time() + world.dt() < due {
            world.step(&[(id, TickInput::default())]);
        }
        assert!(world.time() < due);
        world.step(&[(id, right())]);
        assert!(scope_levels(&world).is_empty());
        let events = run(&mut world, 0.5, &[(id, TickInput::default())]);
        assert!(!events.iter().any(|event| matches!(event, GameEvent::Scope { .. })));
        assert_eq!(world.player(id).unwrap().weapons[0].zoom_level, 1);
        world.step(&[(id, right())]);
        assert_eq!(scope_levels(&world), vec![2]);
        let events = run(&mut world, 0.5, &[(id, TickInput::default())]);
        assert!(!events.iter().any(|event| matches!(event, GameEvent::Scope { .. })));
        assert_eq!(world.player(id).unwrap().weapons[0].zoom_level, 2);
        world.step(&[(id, right())]);
        assert_eq!(scope_levels(&world), vec![0]);
        assert_eq!(world.player(id).unwrap().weapons[0].clip, 5);
    }
}

#[test]
fn held_scope_waits_through_deploy_and_reload_without_requiring_another_edge() {
    for rate in [64, 128] {
        for reload in [false, true] {
            let (mut world, id) = scoped_world();
            world.set_tick_rate(rate);
            if reload {
                let w = &mut world.player_mut(id).unwrap().weapons[0];
                w.set_zoom(2, 0.);
                w.clip = 4;
                world.step(&[(id, TickInput { buttons: buttons::RELOAD | buttons::ATTACK2, ..Default::default() })]);
                assert!(world.player(id).unwrap().weapons[0].reload_end.is_some());
                assert_eq!(world.player(id).unwrap().weapons[0].zoom_level, 0);
            } else {
                world.player_mut(id).unwrap().weapons[0].next_attack = 0.5;
            }
            let deadline = world.player(id).unwrap().weapons[0].next_attack;
            let ready_after = world.player(id).unwrap().weapons[0].reload_end.unwrap_or(0.).max(deadline);
            let mut changed = false;
            for _ in 0..rate * 6 {
                let now = world.time();
                let w = &world.player(id).unwrap().weapons[0];
                let ready = now >= deadline && w.reload_end.is_none();
                world.step(&[(id, right())]);
                assert_eq!(scope_levels(&world), if ready { vec![1] } else { vec![] });
                let w = &world.player(id).unwrap().weapons[0];
                assert_eq!(w.next_attack, deadline);
                assert!(!world.events.iter().any(|event| matches!(event, GameEvent::Fire { .. })));
                if ready {
                    assert!(now >= ready_after && now < ready_after + world.dt() * 2.);
                    assert_eq!(w.clip, 5);
                    changed = true;
                    break;
                }
                assert_eq!((w.zoom_level, w.resume_zoom), (0, None));
            }
            assert!(changed, "rate={rate}, reload={reload}: held command must eventually scope");
        }
    }
}

#[test]
fn held_scope_cannot_cancel_a_pending_bolt_resume_before_primary_readiness() {
    for rate in [64, 128] {
        for release in [false, true] {
            let (mut world, id) = scoped_world();
            world.set_tick_rate(rate);
            world.player_mut(id).unwrap().weapons[0].set_zoom(2, 0.);
            world.step(&[(id, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
            let deadline = world.player(id).unwrap().weapons[0].next_attack;
            while world.time() < deadline {
                world.step(&[(id, right())]);
                let w = &world.player(id).unwrap().weapons[0];
                assert_eq!((w.zoom_level, w.resume_zoom, w.clip), (0, Some(2), 4));
                assert_eq!(w.next_attack, deadline);
                assert!(scope_levels(&world).is_empty());
                assert!(!world.events.iter().any(|event| matches!(event, GameEvent::Fire { .. })));
            }
            world.step(&[(id, if release { TickInput::default() } else { right() })]);
            let w = &world.player(id).unwrap().weapons[0];
            // A released secondary preserves the ordinary same-level resume.
            // A still-held secondary gets the existing ready-tap behavior:
            // cancel pending resume and explicitly select the first level.
            assert_eq!((w.zoom_level, w.resume_zoom), (if release { 2 } else { 1 }, None));
            assert_eq!(w.next_attack, deadline);
            assert_eq!(w.clip, 4);
            assert!(!world.events.iter().any(|event| matches!(event, GameEvent::Fire { .. })));
        }
    }
}

#[test]
fn held_scope_and_primary_attempts_cannot_rapid_fire_a_magazine() {
    for rate in [64, 128] {
        for automatic in [false, true] {
            let (mut world, id) = scoped_world();
            world.set_tick_rate(rate);
            let w = &mut world.player_mut(id).unwrap().weapons[0];
            // Synthetic ten-round fixture; production magazine policy is untouched.
            Arc::make_mut(&mut w.def).clip_size = 10;
            Arc::make_mut(&mut w.def).full_auto = automatic;
            w.clip = 10;
            w.set_zoom(2, 0.);
            let mut deadline = 0.;
            let mut shots = Vec::new();
            for tick in 0..rate * 16 {
                let now = world.time();
                let attack = automatic || tick % 2 == 0;
                let buttons = (if attack { buttons::ATTACK } else { 0 })
                    | (if tick > 0 { buttons::ATTACK2 } else { 0 });
                world.step(&[(id, TickInput { buttons, ..Default::default() })]);
                let w = &world.player(id).unwrap().weapons[0];
                let fired = world.events.iter().filter(|event| matches!(event, GameEvent::Fire { .. })).count();
                assert!(fired <= 1);
                if fired == 1 {
                    assert!(now >= deadline, "rate={rate}, automatic={automatic}: early shot {now} < {deadline}");
                    // Existing automatic cadence carries at most one tick of
                    // rounding, while new semiautomatic presses re-anchor.
                    assert!(w.next_attack >= now + 1.455 - world.dt() - 0.00001);
                    assert!(w.next_attack <= now + 1.455 + 0.00001);
                    deadline = w.next_attack;
                    shots.push(now);
                }
                assert_eq!(w.next_attack, deadline, "secondary cannot rewrite primary readiness");
                assert_eq!(w.clip, 10 - shots.len() as i32);
                assert!(!world.events.iter().any(|event| matches!(event, GameEvent::Reload { .. })));
            }
            assert_eq!(shots.len(), 10);
            assert!(shots.windows(2).all(|pair| pair[1] - pair[0] >= 1.455 - world.dt() - 0.00001));
            let w = &world.player(id).unwrap().weapons[0];
            assert_eq!((w.clip, w.resume_zoom), (0, None));
        }
    }
}

#[test]
fn held_scope_stops_on_holster_and_death_and_waits_for_redeploy() {
    for rate in [64, 128] {
        let (mut world, id) = scoped_world();
        world.set_tick_rate(rate);
        world.step(&[(id, right())]);
        world.step(&[(id, TickInput { weapon_slot: 3, ..right() })]);
        let events = run(&mut world, 0.5, &[(id, right())]);
        assert!(!events.iter().any(|event| matches!(event, GameEvent::Scope { .. } | GameEvent::Fire { .. })));
        assert_eq!((world.player(id).unwrap().weapons[0].zoom_level,
            world.player(id).unwrap().weapons[0].resume_zoom), (0, None));
        world.step(&[(id, TickInput { weapon_slot: 1, ..right() })]);
        assert!(scope_levels(&world).is_empty());
        let deadline = world.player(id).unwrap().weapons[0].next_attack;
        while world.time() < deadline {
            world.step(&[(id, right())]);
            assert!(scope_levels(&world).is_empty());
        }
        world.step(&[(id, right())]);
        assert_eq!(scope_levels(&world), vec![1]);
        world.kill(0, None, "scope hold test", false, None);
        let events = run(&mut world, 0.5, &[(id, right())]);
        assert!(!world.player(id).unwrap().alive);
        assert!(!events.iter().any(|event| matches!(event, GameEvent::Scope { .. } | GameEvent::Fire { .. })));
        for item in &world.items {
            if let GroundKind::Weapon(w) = &item.kind {
                assert_eq!((w.zoom_level, w.resume_zoom), (0, None));
            }
        }
    }
}

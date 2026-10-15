//! Recoil progression semantics, not evidence of an original spray pattern.
use super::*;

fn rifle(rate: u32, automatic: bool) -> (World, PlayerId) {
    let mut w = world();
    w.set_tick_rate(rate);
    let id = w.add_player("shooter", Team::Terrorist);
    let mut gun = w.catalog.get("weapon_ak47").unwrap().clone();
    gun.full_auto = automatic;
    // Synthetic nonzero, straight recoil isolates progression from RNG/spread.
    gun.recoil = weapons::Recoil {
        magnitude: 30.0,
        ..Default::default()
    };
    let p = w.player_mut(id).unwrap();
    p.weapons = vec![WeaponState::new(Arc::new(gun))];
    p.active = 0;
    (w, id)
}

fn held() -> TickInput {
    TickInput {
        buttons: buttons::ATTACK,
        ..Default::default()
    }
}

fn burst(w: &mut World, id: PlayerId, shots: u64) {
    for _ in 0..w.ticks(2.0) {
        w.step(&[(id, held())]);
        let p = w.player(id).unwrap();
        let gun = p.active_weapon().unwrap();
        assert_eq!(
            gun.recoil_index, p.shots as f32,
            "held automatic burst must not recover on cooldown tick {}",
            w.tick
        );
        if p.shots == shots {
            return;
        }
    }
    panic!("burst did not reach {shots} shots");
}

#[test]
fn held_automatic_burst_accumulates_through_cooldown_at_both_rates() {
    for rate in [64, 128] {
        let (mut w, id) = rifle(rate, true);
        burst(&mut w, id, 5);
        let index = w.player(id).unwrap().active_weapon().unwrap().recoil_index;
        let shots = w.player(id).unwrap().shots;
        assert!(w.time() < w.player(id).unwrap().active_weapon().unwrap().next_attack);
        w.step(&[(id, held())]);
        let p = w.player(id).unwrap();
        assert_eq!(p.shots, shots, "the cooldown tick must not emit a shot");
        assert_eq!(p.active_weapon().unwrap().recoil_index, index);
        assert!(
            p.view_punch()[0] < -2.0,
            "accumulated punch reaches the camera seam"
        );
    }
}

#[test]
fn release_recovers_and_a_second_burst_resumes_from_recovered_index() {
    for rate in [64, 128] {
        let (mut w, id) = rifle(rate, true);
        burst(&mut w, id, 5);
        run(&mut w, 0.25, &[(id, TickInput::default())]);
        let before = w.player(id).unwrap().active_weapon().unwrap().recoil_index;
        assert!((before - 2.0).abs() < 0.001);
        w.step(&[(id, held())]);
        assert_eq!(w.player(id).unwrap().shots, 6);
        assert!(
            (w.player(id).unwrap().active_weapon().unwrap().recoil_index - before - 1.0).abs()
                < 0.001
        );
        run(&mut w, 1.0, &[(id, TickInput::default())]);
        assert_eq!(
            w.player(id).unwrap().active_weapon().unwrap().recoil_index,
            0.0
        );
    }
}

#[test]
fn held_trigger_during_magazine_reload_and_dry_fire_does_not_freeze_recovery() {
    for rate in [64, 128] {
        let (mut w, id) = rifle(rate, true);
        burst(&mut w, id, 5);
        let reload = TickInput {
            buttons: buttons::ATTACK | buttons::RELOAD,
            ..Default::default()
        };
        w.step(&[(id, reload)]);
        let p = w.player(id).unwrap();
        assert!(p.active_weapon().unwrap().reload_end.is_some());
        assert!(p.active_weapon().unwrap().recoil_index < 5.0);
        run(&mut w, 0.25, &[(id, held())]);
        assert_eq!(w.player(id).unwrap().shots, 5);
        assert!(w.player(id).unwrap().active_weapon().unwrap().recoil_index < 2.0);

        let (mut dry, id) = rifle(rate, true);
        let gun = dry.player_mut(id).unwrap().active_weapon_mut().unwrap();
        gun.clip = 0;
        gun.reserve = 0;
        gun.recoil_index = 5.0;
        run(&mut dry, 0.25, &[(id, held())]);
        let p = dry.player(id).unwrap();
        assert_eq!(p.shots, 0);
        assert!(p.active_weapon().unwrap().reload_end.is_none());
        assert!((p.active_weapon().unwrap().recoil_index - 2.0).abs() < 0.001);
    }
}

#[test]
fn new_empty_trigger_can_autoreload_and_semiautomatic_hold_recovers() {
    for rate in [64, 128] {
        let (mut w, id) = rifle(rate, true);
        let gun = w.player_mut(id).unwrap().active_weapon_mut().unwrap();
        gun.clip = 0;
        gun.recoil_index = 5.0;
        w.step(&[(id, held())]);
        let gun = w.player(id).unwrap().active_weapon().unwrap();
        assert!(gun.reload_end.is_some());
        assert!(gun.recoil_index < 5.0);

        let (mut semi, id) = rifle(rate, false);
        semi.step(&[(id, held())]);
        assert_eq!(semi.player(id).unwrap().shots, 1);
        run(&mut semi, 0.25, &[(id, held())]);
        assert_eq!(semi.player(id).unwrap().shots, 1);
        assert_eq!(
            semi.player(id)
                .unwrap()
                .active_weapon()
                .unwrap()
                .recoil_index,
            0.0
        );
        semi.step(&[(id, TickInput::default())]);
        semi.step(&[(id, held())]);
        assert_eq!(semi.player(id).unwrap().shots, 2);
    }
}

#[test]
fn sustained_burst_prediction_and_authority_share_punch_and_bullet_direction() {
    for rate in [64, 128] {
        let (mut server, id) = rifle(rate, true);
        let mut predicted = server.clone();
        for _ in 0..server.ticks(0.5) {
            let p = server.player(id).unwrap();
            let punch = p.active_weapon().unwrap().aim_punch();
            let expected = movement::angle_vectors(
                p.view_angles
                    + Vec3::new(
                        punch[0] * weapon::RECOIL_SCALE,
                        punch[1] * weapon::RECOIL_SCALE,
                        0.0,
                    ),
            )
            .0;
            server.step(&[(id, held())]);
            predicted.step_prediction(&[(id, held())]);
            assert_eq!(server.state_hash(), predicted.state_hash());
            for event in &server.events {
                if let GameEvent::Tracer { start, end, .. } = event {
                    assert!(((*end - *start).normalize() - expected).length() < 0.0001);
                }
            }
        }
        assert!(
            server
                .player(id)
                .unwrap()
                .active_weapon()
                .unwrap()
                .recoil_index
                >= 4.0
        );
    }
}

#[test]
fn observed_ray_profile_survives_hold_snapshot_prediction_and_recovered_restart() {
    // Deliberately non-monotonic bullet-space angles expose double scaling,
    // cumulative addition and off-by-one indexing in the actual fire path.
    let mut angles = vec![[-2.5, -0.5]; 30];
    angles[..5].copy_from_slice(&[[0., 0.], [-1., 0.5], [-3., -1.], [-2., 1.5], [-2.5, -0.5]]);
    for rate in [64, 128] {
        let (mut server, id) = rifle(rate, true);
        let mut definitions = server.catalog.weapons.clone();
        let gun = definitions.iter_mut().find(|def| def.class_name == "weapon_ak47").unwrap();
        gun.recoil_trajectory = Some(weapons::RecoilTrajectory {
            reference: "synthetic-ray-seam".into(), bullet_angles: angles.clone(), response: None,
        });
        server.player_mut(id).unwrap().active_weapon_mut().unwrap().set_def(Arc::new(gun.clone()));
        server.catalog = Arc::new(weapons::Catalog { weapons: definitions });
        let mut predicted = server.clone();
        for _ in 0..server.ticks(1.0) {
            let before = server.player(id).unwrap().shots as usize;
            server.step(&[(id, held())]);
            predicted.step_prediction(&[(id, held())]);
            assert_eq!(server.state_hash(), predicted.state_hash());
            for event in &server.events {
                if let GameEvent::Tracer { start, end, .. } = event {
                    let expected = movement::angle_vectors(Vec3::new(angles[before][0], angles[before][1], 0.)).0;
                    assert!(((*end - *start).normalize() - expected).length() < 0.0001,
                        "actual post-spread ray must sample observed shot {} once at {rate}Hz", before + 1);
                }
            }
            if server.player(id).unwrap().shots == 5 { break; }
        }
        assert_eq!(server.player(id).unwrap().shots, 5);
        let decoded: Snapshot = server.decode(&state::encode(&server.snapshot())).unwrap();
        predicted.restore(decoded.state);
        assert_eq!(server.player(id).unwrap().active_weapon().unwrap().aim_punch(),
            predicted.player(id).unwrap().active_weapon().unwrap().aim_punch());
        // This asserts only that the existing recovery policy eventually
        // reaches the recovered start; its native timing remains unmeasured.
        for _ in 0..server.ticks(2.0) {
            server.step(&[(id, TickInput::default())]);
            predicted.step_prediction(&[(id, TickInput::default())]);
            assert_eq!(server.state_hash(), predicted.state_hash());
        }
        assert_eq!(server.player(id).unwrap().active_weapon().unwrap().aim_punch(), [0., 0.]);
        server.step(&[(id, held())]);
        assert_eq!(server.player(id).unwrap().shots, 6);
        let ray = server.events.iter().find_map(|event| match event {
            GameEvent::Tracer { start, end, .. } => Some((*end - *start).normalize()), _ => None,
        }).unwrap();
        assert!((ray - Vec3::X).length() < 0.0001, "recovered restart uses profile's first ray");
    }
}

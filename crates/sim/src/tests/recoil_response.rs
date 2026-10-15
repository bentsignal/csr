//! Authoritative state/lifecycle checks using a synthetic content profile.
//! Native comparisons are separate installed-data diagnostics.
use super::*;

fn fixture(rate: u32) -> (World, PlayerId, Vec<[f32; 2]>) {
    let mut world = world();
    world.set_tick_rate(rate);
    let id = world.add_player("response", Team::Terrorist);
    let mut definitions = world.catalog.weapons.clone();
    let gun = definitions
        .iter_mut()
        .find(|x| x.class_name == "weapon_ak47")
        .unwrap();
    gun.accuracy = weapons::Accuracy::default();
    let mut angles = vec![[-6., 1.]; gun.clip_size as usize];
    angles[..7].copy_from_slice(&[
        [0., 0.],
        [-0.2, -0.1],
        [-1.2, 0.],
        [-2.8, -0.2],
        [-4.5, -0.1],
        [-6.2, 0.5],
        [-7.8, 0.9],
    ]);
    gun.recoil_trajectory = Some(weapons::RecoilTrajectory {
        reference: "synthetic-state-response".into(),
        bullet_angles: angles.clone(),
        response: Some(weapons::RecoilResponse {
            reference: "synthetic-response".into(),
            reference_hz: 128,
            angle_decay: 8.,
            angle_linear: 18.,
            velocity_decay: 4.5,
            first_impulse: [-20., -10.],
            final_impulse: [-20., 0.],
            held_tolerance_degrees: 0.2,
        }),
    });
    let p = world.player_mut(id).unwrap();
    p.weapons = vec![WeaponState::new(Arc::new(gun.clone()))];
    p.active = 0;
    world.catalog = Arc::new(weapons::Catalog {
        weapons: definitions,
    });
    (world, id, angles)
}

fn input(attack: bool) -> TickInput {
    TickInput {
        buttons: if attack { buttons::ATTACK } else { 0 },
        ..Default::default()
    }
}
fn ray(world: &World) -> Option<[f32; 2]> {
    world.events.iter().find_map(|event| {
        if let GameEvent::Tracer { start, end, .. } = event {
            let v = (*end - *start).normalize();
            Some([-v.z.asin().to_degrees(), v.y.atan2(v.x).to_degrees()])
        } else {
            None
        }
    })
}

#[test]
fn response_preserves_held_rays_deadline_rounding_and_snapshot_prediction() {
    for rate in [64, 128] {
        let (mut world, id, angles) = fixture(rate);
        // A nonzero anchor exposes differences from a cadence rounded once.
        run(&mut world, 1.37, &[(id, input(false))]);
        let mut predicted = world.clone();
        let mut seen = 0;
        for _ in 0..world.ticks(4.) {
            world.step(&[(id, input(true))]);
            predicted.step_prediction(&[(id, input(true))]);
            assert_eq!(world.state_hash(), predicted.state_hash());
            if let Some(actual) = ray(&world) {
                let error = (actual[0] - angles[seen][0]).hypot(actual[1] - angles[seen][1]);
                assert!(
                    error < if seen == 1 { 0.2 } else { 0.0001 },
                    "shot {seen} rate {rate}: {actual:?} vs {:?}",
                    angles[seen]
                );
                seen += 1;
                if seen == 6 {
                    let snapshot: Snapshot =
                        world.decode(&state::encode(&world.snapshot())).unwrap();
                    predicted.restore(snapshot.state); // Excludes all derived impulse caches.
                }
            }
            if seen == angles.len() {
                break;
            }
        }
        assert_eq!(seen, angles.len());
        let before = world
            .player(id)
            .unwrap()
            .active_weapon()
            .unwrap()
            .recoil_motion
            .unwrap();
        assert!(before.velocity.iter().any(|x| x.abs() > 0.1));
        for _ in 0..world.ticks(3.) {
            world.step(&[(id, input(false))]);
            predicted.step_prediction(&[(id, input(false))]);
            assert_eq!(world.state_hash(), predicted.state_hash());
        }
        assert!(
            world.player(id).unwrap().active_weapon().unwrap().clip > 0,
            "ordinary reload completes"
        );
        world.step(&[(id, input(true))]);
        assert!(
            ray(&world).unwrap().iter().all(|x| x.abs() < 0.001),
            "recovered magazine starts at aim"
        );
    }
}

#[test]
fn released_motion_continues_with_phase_zero_and_while_holstered() {
    for rate in [64, 128] {
        let (mut world, id, _) = fixture(rate);
        while world.player(id).unwrap().shots < 6 {
            world.step(&[(id, input(true))]);
        }
        let mut holstered = world.clone();
        let knife = WeaponState::new(Arc::new(world.catalog.get("weapon_knife").unwrap().clone()));
        holstered.player_mut(id).unwrap().weapons.push(knife);
        holstered.player_mut(id).unwrap().active = 1;
        for _ in 0..world.ticks(0.6) {
            world.step(&[(id, input(false))]);
            holstered.step(&[(id, input(false))]);
            assert_eq!(
                world.player(id).unwrap().weapons[0].recoil_motion,
                holstered.player(id).unwrap().weapons[0].recoil_motion
            );
        }
        let gun = &world.player(id).unwrap().weapons[0];
        assert_eq!(gun.recoil_index, 0., "legacy phase counter is separate");
        assert!(
            gun.recoil_motion.unwrap().velocity[0].abs() > 1.,
            "velocity survives displayed near-zero angle"
        );
        world.respawn(id);
        assert!(
            world
                .player(id)
                .unwrap()
                .weapons
                .iter()
                .all(|w| w.recoil_motion.is_none())
        );
    }
}

#[test]
fn first_impulse_has_no_first_ray_offset_and_state_retimes_without_a_reset() {
    let (mut world, id, _) = fixture(128);
    world.step(&[(id, input(true))]);
    assert_eq!(ray(&world), Some([0., 0.]));
    let state = world
        .player(id)
        .unwrap()
        .active_weapon()
        .unwrap()
        .recoil_motion
        .unwrap();
    assert_eq!(state.velocity, [-20., -10.]);
    world.step(&[(id, input(false))]);
    let before = world
        .player(id)
        .unwrap()
        .active_weapon()
        .unwrap()
        .recoil_motion
        .unwrap();
    world.set_tick_rate(64);
    let after = world
        .player(id)
        .unwrap()
        .active_weapon()
        .unwrap()
        .recoil_motion
        .unwrap();
    assert_eq!(before.angle, after.angle);
    assert_eq!(before.velocity, after.velocity);
    assert_eq!(after.tick_rate, 64);
    world.step(&[(id, input(false))]);
    assert!(
        world
            .player(id)
            .unwrap()
            .active_weapon()
            .unwrap()
            .recoil_motion
            .unwrap()
            .velocity[0]
            .abs()
            < before.velocity[0].abs()
    );
}

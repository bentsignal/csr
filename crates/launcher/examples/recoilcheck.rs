//! Emit actual authoritative no-spread rays from the installed catalog.
//! No native assets or expected native trajectory are embedded in this tool.
use glam::Vec3;
use std::sync::Arc;

fn main() {
    let mut args = std::env::args().skip(1);
    let weapon = args.next().unwrap_or_else(|| "weapon_ak47".to_owned());
    let mut unsuppressed = false;
    let mut restart_gap = None;
    let mut restart_shots = 6;
    let mut resumed_shots = 1;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--unsuppressed" => unsuppressed = true,
            "--restart-gap" => {
                let gap: f32 = args
                    .next()
                    .expect("restart gap seconds")
                    .parse()
                    .expect("numeric gap");
                assert!(
                    gap.is_finite() && gap > 0. && gap <= 2.,
                    "gap must be in (0, 2] seconds"
                );
                restart_gap = Some(gap);
            }
            "--restart-shots" => {
                restart_shots = args
                    .next()
                    .expect("partial shot count")
                    .parse::<usize>()
                    .expect("integer shot count");
            }
            "--restart-count" => {
                resumed_shots = args
                    .next()
                    .expect("resumed shot count")
                    .parse::<usize>()
                    .expect("integer shot count");
                assert!((1..=32).contains(&resumed_shots));
            }
            _ => panic!(
                "expected --unsuppressed, --restart-gap seconds, --restart-shots count or --restart-count count"
            ),
        }
    }
    let level = level::Level::load("de_dust2", None).expect("installed Legacy map/catalog");
    let installed = level.catalog.get(&weapon).expect("weapon definition");
    let profile = if unsuppressed {
        installed.recoil_trajectory_unsuppressed.as_ref()
    } else {
        installed.recoil_trajectory.as_ref()
    }
    .expect("observed installed weapon/mode profile");
    assert!((1..profile.bullet_angles.len()).contains(&restart_shots));
    eprintln!(
        "profile={} rays={} catalog={:?}",
        profile.reference,
        profile.bullet_angles.len(),
        level.catalog_sha1()
    );
    println!(
        "tickrate,shot,tick,time,pitch,yaw,error_degrees,punch_pitch,punch_yaw,punch_velocity_pitch,punch_velocity_yaw"
    );
    for rate in [64, 128] {
        let mut gun = installed.clone();
        // Equivalent direction isolation to native weapon_accuracy_nospread1;
        // the loaded profile/recoil stats remain unchanged. No files are edited.
        gun.accuracy = weapons::Accuracy::default();
        let gun = Arc::new(gun);
        let mut collision = collision::CollisionWorld::new();
        let surface = collision.surface_id("diagnostic-floor");
        for points in [
            [
                Vec3::new(-4096., -4096., 0.),
                Vec3::new(4096., -4096., 0.),
                Vec3::new(4096., 4096., 0.),
            ],
            [
                Vec3::new(-4096., -4096., 0.),
                Vec3::new(4096., 4096., 0.),
                Vec3::new(-4096., 4096., 0.),
            ],
        ] {
            collision.add_triangle(points, bsp::contents::SOLID, surface);
        }
        collision.finalize();
        let mut world = sim::World::with_catalog(Arc::new(collision), level.catalog.clone());
        world.set_tick_rate(rate);
        let id = world.add_player("ray-observer", sim::Team::Terrorist);
        let p = world.player_mut(id).unwrap();
        p.movement.origin = Vec3::new(0., 0., 1.);
        let mut state = sim::weapon::WeaponState::new(gun);
        if unsuppressed {
            assert!(
                state.def.suppressor_animations.is_some(),
                "detachable suppressor required"
            );
            state.suppressor_attached = false;
        }
        p.weapons = vec![state];
        p.active = 0;
        let total = if restart_gap.is_some() {
            restart_shots + resumed_shots
        } else {
            profile.bullet_angles.len()
        };
        assert!(profile.bullet_angles.len() >= total);
        let mut emitted = 0;
        let mut partial_time = None;
        let mut partial_tick = None;
        for _ in 0..world.ticks(5.) {
            // Release after actual tracers, then request one restart.
            // The printed last-to-restart interval is authoritative sim time;
            // it can differ from the request because of cooldown/tick rounding.
            let attack = emitted < restart_shots
                || restart_gap.is_none()
                || (world.tick > partial_tick.unwrap_or(0)
                    && world.time() + world.dt()
                        >= partial_time.unwrap_or(0.) + restart_gap.unwrap());
            let input = sim::TickInput {
                buttons: if attack { movement::buttons::ATTACK } else { 0 },
                ..Default::default()
            };
            world.step(&[(id, input)]);
            for event in &world.events {
                if let sim::GameEvent::Tracer { start, end, .. } = event {
                    let direction = (*end - *start).normalize();
                    let pitch = -direction.z.asin().to_degrees();
                    let yaw = direction.y.atan2(direction.x).to_degrees();
                    let held_reference = restart_gap.is_none() || emitted < restart_shots;
                    let error = if held_reference {
                        let expected = profile.bullet_angles[emitted];
                        ((pitch - expected[0]).powi(2) + (yaw - expected[1]).powi(2)).sqrt()
                    } else {
                        // There is no admitted recovery procedure in this
                        // diagnostic. Emit the real ray without inventing an
                        // expected seventh angle from the held-magazine table.
                        f32::NAN
                    };
                    let motion = world
                        .player(id)
                        .unwrap()
                        .active_weapon()
                        .unwrap()
                        .recoil_motion;
                    let (a, v) =
                        motion.map_or(([f32::NAN; 2], [f32::NAN; 2]), |m| (m.angle, m.velocity));
                    println!(
                        "{rate},{},{},{:.6},{pitch:.6},{yaw:.6},{error:.6},{:.6},{:.6},{:.6},{:.6}",
                        emitted + 1,
                        world.tick,
                        world.time(),
                        a[0],
                        a[1],
                        v[0],
                        v[1]
                    );
                    // The first two native boxes overlap. Independent first
                    // velocity evidence resolves their ambiguity only within
                    // the original marker uncertainty; later held rays retain
                    // their existing exact reconstruction check.
                    let tolerance = if emitted == 1 {
                        profile
                            .response
                            .as_ref()
                            .map_or(0.001, |r| r.held_tolerance_degrees)
                    } else {
                        0.001
                    };
                    assert!(
                        !held_reference || error < tolerance,
                        "actual authoritative bullet differs from attached observed profile"
                    );
                    emitted += 1;
                    if emitted == restart_shots {
                        partial_time = Some(world.time());
                        partial_tick = Some(world.tick);
                    } else if emitted == restart_shots + 1 && restart_gap.is_some() {
                        eprintln!(
                            "tickrate={rate} requested_restart_gap={} actual_restart_interval={:.6} clip={}",
                            restart_gap.unwrap(),
                            world.time() - partial_time.unwrap(),
                            world.player(id).unwrap().active_weapon().unwrap().clip
                        );
                    }
                }
            }
            if emitted == total {
                break;
            }
        }
        assert_eq!(
            emitted, total,
            "requested actual ray sequence must complete"
        );
    }
}

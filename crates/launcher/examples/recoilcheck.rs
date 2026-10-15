//! Emit actual authoritative no-spread rays from the installed catalog.
//! No native assets or expected native trajectory are embedded in this tool.
use glam::Vec3;
use std::sync::Arc;

fn main() {
    let level = level::Level::load("de_dust2", None).expect("installed Legacy map/catalog");
    let installed = level.catalog.get("weapon_ak47").expect("AK definition");
    let profile = installed
        .recoil_trajectory
        .as_ref()
        .expect("observed installed AK profile");
    eprintln!(
        "profile={} rays={} catalog={:?}",
        profile.reference,
        profile.bullet_angles.len(),
        level.catalog_sha1()
    );
    println!("tickrate,shot,tick,time,pitch,yaw,error_degrees");
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
        p.weapons = vec![sim::weapon::WeaponState::new(gun)];
        p.active = 0;
        let input = sim::TickInput {
            buttons: movement::buttons::ATTACK,
            ..Default::default()
        };
        let mut emitted = 0;
        for _ in 0..world.ticks(5.) {
            world.step(&[(id, input)]);
            for event in &world.events {
                if let sim::GameEvent::Tracer { start, end, .. } = event {
                    let direction = (*end - *start).normalize();
                    let pitch = -direction.z.asin().to_degrees();
                    let yaw = direction.y.atan2(direction.x).to_degrees();
                    let expected = profile.bullet_angles[emitted];
                    let error =
                        ((pitch - expected[0]).powi(2) + (yaw - expected[1]).powi(2)).sqrt();
                    println!(
                        "{rate},{},{},{:.6},{pitch:.6},{yaw:.6},{error:.6}",
                        emitted + 1,
                        world.tick,
                        world.time()
                    );
                    assert!(
                        error < 0.001,
                        "actual authoritative bullet differs from attached observed profile"
                    );
                    emitted += 1;
                }
            }
            if emitted == profile.bullet_angles.len() {
                break;
            }
        }
        assert_eq!(emitted, 30, "held magazine must reach all observed rays");
    }
}

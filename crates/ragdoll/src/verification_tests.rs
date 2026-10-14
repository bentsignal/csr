//! CPU-only checks against runtime-owned Legacy models and map collision.
use super::*;
use std::time::Instant;

fn installed_rig(fs: &vfs::Vfs, name: &str) -> (Arc<Rig>, Vec<Mat4>) {
    let base = format!("models/player/custom_player/legacy/{name}");
    let bytes = fs.read(&format!("{base}.mdl")).unwrap();
    let model = mdl::Model::load_header(&bytes).unwrap();
    let rig =
        Arc::new(Rig::load(&model, &bytes, &fs.read(&format!("{base}.phy")).unwrap()).unwrap());
    let pose = mdl::skeleton::model_space(&model.bones, &mdl::skeleton::rest_pose(&model.bones));
    (rig, pose)
}

#[test]
#[ignore = "requires read-only installed CS:GO Legacy models; no GPU"]
fn installed_initial_world_matrices_preserve_captured_pose_and_pause() {
    let (_, fs) = level::mount_game(None).unwrap();
    let mut collision = collision::CollisionWorld::new();
    collision.finalize();
    for name in ["ctm_sas", "tm_phoenix"] {
        let (rig, _) = installed_rig(&fs, name);
        let mut local = mdl::skeleton::rest_pose(&rig.bones);
        // A non-rest captured pose exposes accidental bind-pose replacement.
        let physical_bone = rig.bone_ids[1];
        local[physical_bone].1 *= Quat::from_rotation_x(0.13);
        let captured = mdl::skeleton::model_space(&rig.bones, &local);
        let origin = Vec3::new(320., -420., 64.);
        let yaw = 90_f32;
        let root = Mat4::from_rotation_translation(Quat::from_rotation_z(yaw.to_radians()), origin);
        let mut corpse = Corpse::new(
            rig,
            &collision,
            &captured,
            origin,
            yaw,
            Vec3::X * 120.,
            None,
        )
        .unwrap();
        let initial = corpse.bone_matrices();
        for (index, (actual, model)) in initial.iter().zip(&captured).enumerate() {
            let expected = root * *model;
            let error = actual
                .to_cols_array()
                .into_iter()
                .zip(expected.to_cols_array())
                .map(|(a, b)| (a - b).abs())
                .fold(0_f32, f32::max);
            assert!(
                error < 0.003,
                "{name} bone {index} initial matrix error {error}"
            );
        }
        for dt in [0., -1., f32::NAN] {
            corpse.advance(dt);
        }
        assert_eq!(
            corpse.bone_matrices(),
            initial,
            "pause/invalid dt cannot move a corpse"
        );
        println!(
            "{name}: all {} initial bone matrices preserve captured pose; failures: 0",
            initial.len()
        );
    }
}

fn vec3(text: &str) -> Vec3 {
    let v: Vec<f32> = text
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect();
    Vec3::new(v[0], v[1], v[2])
}

#[test]
#[ignore = "requires read-only installed CS:GO Legacy Dust II; CPU cost audit, no GPU"]
fn installed_dust2_corpse_creation_and_sixteen_body_step_cost() {
    let level = level::Level::load("de_dust2", None).unwrap();
    let (ct, ct_pose) = installed_rig(&level.vfs, "ctm_sas");
    let (t, t_pose) = installed_rig(&level.vfs, "tm_phoenix");
    let mut corpses = Vec::new();
    let mut create_ms = Vec::new();
    for (class, rig, pose) in [
        ("info_player_counterterrorist", ct, ct_pose),
        ("info_player_terrorist", t, t_pose),
    ] {
        for entity in level.bsp.entities_of_class(class).take(8) {
            let origin = vec3(entity.str("origin").unwrap()) + Vec3::Z * 24.;
            let yaw = entity.str("angles").map(vec3).unwrap_or(Vec3::ZERO).y;
            let start = Instant::now();
            let corpse = Corpse::new(
                rig.clone(),
                &level.collision,
                &pose,
                origin,
                yaw,
                Vec3::X * 60.,
                None,
            )
            .unwrap();
            let cost = start.elapsed().as_secs_f64() * 1000.;
            let nearby_convexes = level
                .collision
                .convexes
                .iter()
                .filter(|c| {
                    overlaps(
                        c.mins,
                        c.maxs,
                        origin - Vec3::splat(WORLD_RADIUS),
                        origin + Vec3::splat(WORLD_RADIUS),
                    )
                })
                .count();
            let max_planes = level
                .collision
                .convexes
                .iter()
                .filter(|c| {
                    overlaps(
                        c.mins,
                        c.maxs,
                        origin - Vec3::splat(WORLD_RADIUS),
                        origin + Vec3::splat(WORLD_RADIUS),
                    )
                })
                .map(|c| c.planes.len())
                .max()
                .unwrap_or(0);
            println!(
                "create {} at {origin:?}: {cost:.3} ms; colliders={} nearbyconvexes={nearby_convexes} maxplanes={max_planes}",
                corpses.len(),
                corpse.world.colliders.len()
            );
            create_ms.push(cost);
            corpses.push(corpse);
        }
    }
    assert_eq!(corpses.len(), 16, "audit the actual configured body cap");
    let mut update_ms = Vec::new();
    for _ in 0..120 {
        let start = Instant::now();
        for corpse in &mut corpses {
            corpse.advance(1. / 60.);
            assert!(corpse.bone_matrices().iter().all(|m| m.is_finite()));
        }
        update_ms.push(start.elapsed().as_secs_f64() * 1000.);
    }
    create_ms.sort_by(f64::total_cmp);
    update_ms.sort_by(f64::total_cmp);
    println!(
        "Dust II: create median={:.3}ms max={:.3}ms; 16-body advance+pose mean={:.3}ms p95={:.3}ms max={:.3}ms; failures: 0",
        create_ms[create_ms.len() / 2],
        create_ms.last().unwrap(),
        update_ms.iter().sum::<f64>() / update_ms.len() as f64,
        update_ms[(update_ms.len() * 95 / 100).min(update_ms.len() - 1)],
        update_ms.last().unwrap()
    );
}

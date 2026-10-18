use super::*;
use glam::Vec3;

fn bounds(points: impl Iterator<Item = Vec3>) -> (Vec3, Vec3) {
    points.fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(low, high), point| (low.min(point), high.max(point)))
}

#[test]
#[ignore = "requires selected installed Legacy maps; parent owns executable checks"]
fn installed_cover_and_door_initial_geometry_spans_the_authored_phy_opening() {
    use content::{ContentId, ContentKind};
    for (name, covers, door_model) in [
        ("de_nuke", vec![237usize, 30, 388], "models/props/de_nuke/hr_nuke/metal_door_001/metal_door_001_br.mdl"),
        ("de_cache", vec![236usize, 565], "models/newcache/nc_bluedoor/nc_bluedoor.mdl"),
    ] {
        let (game_dir, fs) = level::mount_game(None).expect("selected Legacy install");
        let id = ContentId::csgo(ContentKind::Map, name);
        let level = level::Level::load_local_id_from_vfs(&id, game_dir, fs, &workshop::Store::current()).unwrap();
        let mut cases = Vec::new();
        for entity in covers {
            let def = level.collision.breakables.iter().find(|def| def.entity == entity).expect("actual admitted cover");
            let studio = def.studio.as_ref().unwrap();
            cases.push((studio.model.as_str(), studio.body));
        }
        assert!(level.collision.doors.iter().any(|def| def.model == door_model), "actual admitted {name} door");
        cases.push((door_model, 0));
        for (path, body) in cases {
            let read = |path: &str| level.vfs.read(path).unwrap_or_else(|| panic!("missing mounted {path}"));
            let base = path.strip_suffix(".mdl").unwrap();
            let model = mdl::Model::load_with_body_value(&read(path), &read(&format!("{base}.vvd")),
                &read(&format!("{base}.dx90.vtx")), body).unwrap();
            assert!(!model.meshes.is_empty() && !model.bones.is_empty());
            assert!(model.pose_params.is_empty(), "fixture needs deliberate update if authored pose parameters change");
            let rest = mdl::skeleton::model_space(&model.bones, &mdl::skeleton::rest_pose(&model.bones));
            for (bone, matrix) in model.bones.iter().zip(&rest) {
                let error = (*matrix * bone.pose_to_bone).to_cols_array().into_iter()
                    .zip(Mat4::IDENTITY.to_cols_array()).map(|(a, b)| (a - b).abs()).fold(0., f32::max);
                assert!(error < 1e-3, "{path}: rest/inverse bind identity changed");
            }
            // Require successful authored decoding as well as the production
            // wrapper: a warning/rest fallback cannot pass this regression.
            let pose = sequence_zero(&model, None).unwrap();
            assert_eq!(initial(&model, &level.vfs), pose);
            let vertices = || model.meshes.iter().flat_map(|mesh| &mesh.vertices);
            let raw_bounds = bounds(vertices().map(|v| mdl::skeleton::skin_vertex(v, &rest, &model.bones)));
            let posed_bounds = bounds(vertices().map(|v| mdl::skeleton::skin_vertex(v, &pose, &model.bones)));
            let phy = mdl::Phy::parse(&read(&format!("{base}.phy"))).unwrap();
            assert!(!phy.hulls.is_empty());
            let phy_bounds = bounds(phy.hulls.iter().flat_map(|hull| hull.points.iter().copied()));
            let size = phy_bounds.1 - phy_bounds.0;
            let thin = (0..3).min_by(|&a, &b| size[a].total_cmp(&size[b])).unwrap();
            let posed_size = posed_bounds.1 - posed_bounds.0;
            assert_eq!(thin, (0..3).min_by(|&a, &b| posed_size[a].total_cmp(&posed_size[b])).unwrap(), "{path}");
            assert!((raw_bounds.1 - raw_bounds.0)[thin] > posed_size[thin] * 3., "{path}: fixture must expose the old edge-on pose");
            // Render slats/handles have depth beyond the simple PHY slab. All
            // five inspected models agree within six Source units per bound;
            // a perpendicular cover differs by tens of units and must fail.
            for (a, b) in [(posed_bounds.0, phy_bounds.0), (posed_bounds.1, phy_bounds.1)] {
                assert!((a - b).abs().max_element() < 6., "{path}: initial {posed_bounds:?}, PHY {phy_bounds:?}");
            }
        }
    }
}

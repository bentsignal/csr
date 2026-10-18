//! Synthetic declarations only. Source added UNRUN under the parent's build slot.
use super::*;
use crate::{CollisionWorld, MapCollisionOptions, contents};

fn mdl(data: &str) -> Vec<u8> {
    let text = format!("mdlkeyvalue {{ prop_data {{ {data} }} }}\0");
    let mut out = vec![0; 384]; out[..4].copy_from_slice(b"IDST");
    out[4..8].copy_from_slice(&49i32.to_le_bytes());
    out[312..316].copy_from_slice(&384i32.to_le_bytes());
    out[316..320].copy_from_slice(&(text.len() as i32).to_le_bytes());
    out.extend_from_slice(text.as_bytes()); out
}
fn hull() -> mdl::ConvexHull {
    mdl::ConvexHull { points: vec![
        Vec3::new(-1., -20., 0.), Vec3::new(1., -20., 0.), Vec3::new(1., 20., 0.), Vec3::new(-1., 20., 0.),
        Vec3::new(-1., -20., 80.), Vec3::new(1., -20., 80.), Vec3::new(1., 20., 80.), Vec3::new(-1., 20., 80.),
    ], triangles: vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4],
        [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]] }
}
fn prop(metadata: &str) -> kv::Block {
    kv::parse(&format!(r#"p {{ classname prop_dynamic model models/private_fixture.mdl solid 6
        origin "17 23 31" angles "0 0 0" {metadata} }}"#)).unwrap().block("p").unwrap().clone()
}

#[test]
fn model_health_is_required_and_local_overrides_resolve_bounded_mounted_bases() {
    let mut map = bsp::Bsp::default(); map.entities.push(prop("health 999"));
    let presets = kv::parse(r#"MediumFixture { base BaseFixture } BaseFixture { health 0 "dmg.bullets" 0.5 "dmg.club" 2 }
        Cycle { base Cycle }"#).unwrap();
    let load = |data: &str| load_props(&map, &|_| Some(vec![hull()]), &|_| Some(mdl(data)), &presets);
    let defs = load("base MediumFixture health 1");
    assert_eq!(defs.len(), 1);
    assert_eq!((defs[0].health, defs[0].bullet_scale, defs[0].club_scale), (1., 0.5, 2.));
    for data in ["", "base Missing", "base MediumFixture", "base Cycle", "health 0", "health -1", "health NaN", "health inf", "health 1000001"] {
        assert!(load(data).is_empty(), "must not invent health for {data}");
    }
    for data in ["health 1", r#"health 1 "dmg.bullets" NaN "dmg.club" 1"#] {
        assert!(load_props(&map, &|_| Some(vec![hull()]), &|_| Some(mdl(data)), &kv::Block::default()).is_empty());
    }
    for metadata in ["StartDisabled 1", "parentname moving_fixture", "modelscale -1", "body -1", "skin -1", "RandomAnimation 1"] {
        map.entities[0] = prop(metadata);
        assert!(load_props(&map, &|_| Some(vec![hull()]), &|_| Some(mdl(r#"health 1 "dmg.bullets" 1 "dmg.club" 1"#)), &presets).is_empty());
    }
    map.entities[0] = prop("");
    assert!(load_props(&map, &|_| None, &|_| Some(mdl(r#"health 1 "dmg.bullets" 1 "dmg.club" 1"#)), &presets).is_empty());
    assert!(load_props(&map, &|_| Some(vec![hull()]), &|_| None, &presets).is_empty());
}

#[test]
fn arbitrary_pitch_yaw_and_scale_transform_the_authored_hull_without_map_cases() {
    let mut map = bsp::Bsp::default();
    let mut ent = prop("modelscale 1.5");
    ent.set("angles", kv::Value::Str("90 213.5 0".into())); map.entities.push(ent);
    let base = CollisionWorld::from_bsp_with_model_data(&map, &MapCollisionOptions::default(),
        &|_| Some(vec![hull()]), &|_| Some(mdl(r#"health 1 "dmg.bullets" 1 "dmg.club" 1"#)), &kv::Block::default());
    let def = &base.breakables[0]; let studio = def.studio.as_ref().unwrap();
    let rotation = mdl::angles_to_mat3(studio.angles);
    for point in &hull().points {
        let expected = def.origin + rotation * (*point * studio.scale);
        assert!(def.solids.iter().any(|s| expected.cmpge(s.mins - Vec3::splat(0.001)).all()
            && expected.cmple(s.maxs + Vec3::splat(0.001)).all()
            && s.planes.iter().all(|p| p.normal.dot(expected) <= p.dist + 0.001)));
    }
    let from = def.origin - rotation * Vec3::new(10., 0., -40.);
    let to = def.origin + rotation * Vec3::new(10., 0., 40.);
    let solids = def.solids.clone(); let owners = vec![Some(0); solids.len()];
    let world = CollisionWorld::with_runtime_solids(std::sync::Arc::new(base), solids, owners);
    let (trace, owner) = world.trace_breakable_ray(from, to, contents::MASK_SHOT);
    assert!(trace.hit()); assert_eq!(owner, Some(0));
    assert_eq!(world.surface_name(trace.surface), "models/private_fixture.mdl");
}

#[test]
fn only_literal_immediate_kill_edges_remove_supported_static_dependencies() {
    let mut map = bsp::Bsp::default();
    map.entities.push(prop(r#"OnBreak "shared,Kill,,0,-1" OnBreak "delayed,Kill,,1,-1"
        OnBreak "wild*,Kill,,0,-1" OnBreak "argument,Kill,x,0,-1"
        OnBreak "exhausted,Kill,,0,0" OnBreak "moving,Kill,,0,-1""#));
    map.entities.push(prop(r#"OnBreak "shared,Kill,,0,1""#));
    for (name, class, flags) in [("shared", "func_button", 1), ("delayed", "func_wall", 0),
        ("wildcard", "func_wall", 0), ("argument", "func_wall", 0), ("exhausted", "func_wall", 0), ("moving", "func_button", 0)] {
        let doc = kv::parse(&format!("e {{ classname {class} targetname {name} model *1 spawnflags {flags} }}")).unwrap();
        map.entities.push(doc.block("e").unwrap().clone());
    }
    map.models.push(bsp::Model { mins: Vec3::ZERO, maxs: Vec3::ONE, origin: Vec3::ZERO,
        head_node: -1, first_face: 0, num_faces: 0 });
    map.models.push(map.models[0]);
    let defs = load_props(&map, &|_| Some(vec![hull()]), &|_| Some(mdl(r#"health 1 "dmg.bullets" 1 "dmg.club" 1"#)), &kv::Block::default());
    let deps = dependencies(&map, &defs);
    assert_eq!(deps.len(), 1); assert_eq!(deps[0].entity, 2); assert_eq!(deps[0].killed_by, vec![0, 1]);
    // Real entity outputs use ESC rather than commas. Keep the same bounded admission.
    map.entities[0].set("OnBreak", kv::Value::Str("shared\u{1b}Kill\u{1b}\u{1b}0\u{1b}-1".into()));
    assert_eq!(dependencies(&map, &defs)[0].killed_by, vec![0, 1]);
}

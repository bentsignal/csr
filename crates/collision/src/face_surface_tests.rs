use super::*;
use std::sync::Arc;

const MATERIALS: [&str; 6] = ["fixture/metal", "fixture/wood", "fixture/concrete",
    "fixture/glass", "tools/toolsskybox", "fixture/dirt"];

fn brush_map() -> bsp::Bsp {
    let mut map = bsp::Bsp::default();
    for (index, normal) in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z].into_iter().enumerate() {
        map.planes.push(bsp::Plane { normal, dist: 16., kind: 0 });
        map.brush_sides.push(bsp::BrushSide { plane: index as u16, texinfo: index as i16, dispinfo: -1, bevel: false });
        map.texinfo.push(bsp::TexInfo { texture_vecs: [glam::Vec4::ZERO; 2], lightmap_vecs: [glam::Vec4::ZERO; 2], flags: 0, texdata: index as i32 });
        map.texdata.push(bsp::TexData { reflectivity: Vec3::ONE, name_id: index as i32, width: 16, height: 16 });
        map.texdata_strings.push(MATERIALS[index].into());
    }
    map.brushes.push(bsp::Brush { first_side: 0, num_sides: 6, contents: contents::SOLID });
    map.leaf_brushes.push(0);
    map.leafs.push(bsp::Leaf { contents: 0, cluster: 0, area: 0, mins: [-16; 3], maxs: [16; 3],
        first_leaf_face: 0, num_leaf_faces: 0, first_leaf_brush: 0, num_leaf_brushes: 1 });
    map.models.push(bsp::Model { mins: Vec3::splat(-16.), maxs: Vec3::splat(16.), origin: Vec3::ZERO,
        head_node: -1, first_face: 0, num_faces: 0 });
    map
}

fn check_faces(world: &CollisionWorld) {
    for (index, normal) in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z].into_iter().enumerate() {
        for ext in [Vec3::ZERO, Vec3::splat(2.)] {
            let trace = world.trace_box(normal * 64., Vec3::ZERO, -ext, ext, contents::MASK_SHOT);
            assert!(trace.hit());
            assert_eq!(world.surface_name(trace.surface), MATERIALS[index]);
            assert_eq!(trace.normal, normal);
            assert!((trace.end - normal * (16. + ext.max_element() + DIST_EPSILON)).length() < 1e-5);
            assert_eq!(world.is_sky(trace.surface), index == 4);
        }
    }
}

#[test]
fn bsp_contacts_use_the_hit_face_without_changing_clip_geometry() {
    let map = brush_map();
    let mut world = CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &|_| None);
    check_faces(&world);
    let queries: Vec<_> = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z].into_iter()
        .map(|n| world.trace_ray(n * 64., Vec3::ZERO, contents::MASK_SHOT)).collect();
    world.convexes[0].plane_surfaces.clear();
    for (normal, expected) in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z].into_iter().zip(queries) {
        let actual = world.trace_ray(normal * 64., Vec3::ZERO, contents::MASK_SHOT);
        assert_eq!(actual.surface, world.convexes[0].surface);
        assert_eq!(actual.fraction.to_bits(), expected.fraction.to_bits());
        assert_eq!(actual.end, expected.end);
        assert_eq!(actual.normal, expected.normal);
        assert_eq!(actual.plane_dist, expected.plane_dist);
    }
}

#[test]
fn breakable_overlay_retains_face_materials_with_its_exact_owner() {
    let mut map = brush_map();
    map.models.push(map.models[0]);
    let mut empty_leaf = map.leafs[0]; empty_leaf.num_leaf_brushes = 0;
    map.leafs.push(empty_leaf); map.models[0].head_node = -2;
    map.entities.push(kv::parse("e { classname func_breakable model *1 health 10 }").unwrap().block("e").unwrap().clone());
    let base = Arc::new(CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &|_| None));
    assert!(base.convexes.is_empty());
    let solids = base.breakables[0].solids.clone();
    let owners = vec![Some(0); solids.len()];
    let world = CollisionWorld::with_runtime_solids(base, solids, owners);
    check_faces(&world);
    let (trace, owner) = world.trace_breakable_ray(-Vec3::X * 64., Vec3::ZERO, contents::MASK_SHOT);
    assert_eq!(owner, Some(0));
    assert_eq!(world.surface_name(trace.surface), "fixture/wood");
}

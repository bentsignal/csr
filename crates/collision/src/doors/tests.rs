//! Synthetic geometry only. These integration fixtures are source-reviewed;
//! executable checks are UNRUN while the parent owns the Cargo slot.
use super::*;
use crate::MapCollisionOptions;

fn hull(low: Vec3, high: Vec3) -> mdl::ConvexHull {
    mdl::ConvexHull {
        points: vec![
            Vec3::new(low.x, low.y, low.z), Vec3::new(high.x, low.y, low.z),
            Vec3::new(high.x, high.y, low.z), Vec3::new(low.x, high.y, low.z),
            Vec3::new(low.x, low.y, high.z), Vec3::new(high.x, low.y, high.z),
            Vec3::new(high.x, high.y, high.z), Vec3::new(low.x, high.y, high.z),
        ],
        triangles: vec![
            [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7],
            [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5],
            [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7],
        ],
    }
}

fn fixture() -> Arc<CollisionWorld> {
    let mut map = bsp::Bsp::default();
    let doc = kv::parse(r#"door { classname prop_door_rotating
        model models/fixture_leaf.mdl origin "0 0 0" angles "0 0 0" }"#).unwrap();
    map.entities.push(doc.block("door").unwrap().clone());
    let mut world = CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &|_| {
        Some(vec![hull(Vec3::new(-0.25, 0., 0.), Vec3::new(0.25, 48., 96.))])
    });
    let ladder = hull(Vec3::new(0.5, 0., 0.), Vec3::new(1.5, 48., 256.));
    let surface = world.surface_id("fixture_ladder");
    world.add_convex(convex_from_hull(&ladder.points, &ladder.triangles,
        contents::LADDER | contents::GRATE, surface).unwrap());
    world.finalize();
    Arc::new(world)
}

fn posed(base: &Arc<CollisionWorld>, angle: f32) -> Arc<CollisionWorld> {
    CollisionWorld::with_door_solids(base.clone(), base.doors[0].solids(angle))
}

#[test]
fn posed_door_world_keeps_static_ladders_but_the_leaf_can_block_attachment() {
    let base = fixture();
    let origin = Vec3::new(-16.75, 24., 32.);
    let mins = Vec3::new(-16., -16., 0.);
    let maxs = Vec3::new(16., 16., 72.);
    assert_eq!(base.ladder_normal(origin, mins, maxs), Some(-Vec3::X));
    let closed = posed(&base, 0.);
    let open = posed(&base, -90.);
    assert_eq!(open.ladder_normal(origin, mins, maxs), Some(-Vec3::X));
    assert_eq!(closed.ladder_normal(origin, mins, maxs), None,
        "the outer leaf must still obstruct the static brush contact sweep");
    assert_eq!(open.ladder_normal(origin, mins, maxs), Some(-Vec3::X),
        "independent door overlays must not mutate each other's geometry");
}

#[test]
fn door_overlay_respects_masks_and_static_exclusions_without_excluding_the_leaf() {
    let base = fixture();
    let closed = posed(&base, 0.);
    let open = posed(&base, -90.);
    let start = Vec3::new(-16., 24., 64.);
    let end = Vec3::new(16., 24., 64.);
    // The ladder is static convex zero; the first leaf also has local index
    // zero. A static exclusion must never remove that unrelated moving solid.
    let excluded = 0..1;
    let trace = |world: &CollisionWorld, mask| world.trace_box_excluding(
        start, end, Vec3::ZERO, Vec3::ZERO, mask, Some(&excluded));
    for mask in [contents::MASK_PLAYERSOLID, contents::MASK_SHOT] {
        let hit = trace(&closed, mask);
        assert!(hit.hit() && !hit.start_solid && !hit.all_solid);
        assert_eq!(hit.normal, -Vec3::X);
        assert_eq!(closed.surface_name(hit.surface), "models/fixture_leaf.mdl");
        assert!(!trace(&open, mask).hit());
    }
    assert!(!trace(&closed, contents::WATER).hit());
    let inside = Vec3::new(0., 24., 64.);
    assert!(closed.trace_ray(inside, end, contents::MASK_SHOT).start_solid);
    assert_eq!(closed.point_contents(inside, contents::MASK_SHOT), contents::SOLID);
    assert_eq!(closed.point_contents(inside, contents::WATER), 0);
    assert_eq!(open.point_contents(inside, contents::MASK_SHOT), 0);
}

#[test]
fn runtime_swept_bounds_rejection_matches_unfiltered_convex_clipping() {
    let base = fixture();
    let mut solids = base.doors[0].solids(37.);
    // Same contact twice, with different surfaces: first-winner ties survive.
    let mut duplicate = solids[0].clone();
    duplicate.surface = 0;
    solids.push(duplicate);
    let mut remote = solids[0].clone();
    let translation = Vec3::new(4096., -4096., 8192.);
    remote.mins += translation;
    remote.maxs += translation;
    for plane in &mut remote.planes { plane.dist += plane.normal.dot(translation); }
    solids.push(remote);
    let world = CollisionWorld::with_door_solids(base, solids);
    let excluded = 0..1;
    for (mins, maxs) in [(Vec3::ZERO, Vec3::ZERO),
        (Vec3::new(-16., -16., 0.), Vec3::new(16., 16., 72.))] {
        for mask in [contents::MASK_SHOT, contents::MASK_PLAYERSOLID, contents::WATER] {
            for exclusion in [None, Some(&excluded)] {
                for start in [Vec3::new(-64., 24., 64.), Vec3::new(-12., 16., 64.),
                    Vec3::new(0., 24., 64.), translation + Vec3::new(-12., 16., 64.)] {
                    for end in [start, start + Vec3::X * 128., start - Vec3::X * 128.,
                        Vec3::new(8192., 8192., 64.), start + Vec3::Z * 0.01] {
                        let offset = (mins + maxs) * 0.5;
                        let ext = (maxs - mins) * 0.5;
                        let mut expected = world.base.as_ref().unwrap()
                            .trace_box_excluding(start, end, mins, maxs, mask, exclusion);
                        for solid in &world.convexes {
                            if solid.contents & mask != 0 {
                                clip_convex_surfaces(&solid.planes, start + offset, end + offset, ext,
                                    solid.contents, solid.surface, &solid.plane_surfaces, &mut expected);
                            }
                        }
                        expected.end = if expected.all_solid { start } else { start + (end - start) * expected.fraction };
                        let actual = world.trace_box_excluding(start, end, mins, maxs, mask, exclusion);
                        assert_eq!(actual.fraction.to_bits(), expected.fraction.to_bits());
                        assert_eq!(actual.end, expected.end);
                        assert_eq!(actual.normal, expected.normal);
                        assert_eq!(actual.plane_dist.to_bits(), expected.plane_dist.to_bits());
                        assert_eq!((actual.start_solid, actual.all_solid, actual.contents, actual.surface),
                            (expected.start_solid, expected.all_solid, expected.contents, expected.surface));
                    }
                }
            }
        }
    }
    // Inside the enclosing sweep AABB, but away from the diagonal segment.
    assert!(!crate::swept_bounds_overlap(Vec3::ZERO, Vec3::splat(8192.), Vec3::ZERO,
        Vec3::new(4000., 0., 0.), Vec3::new(4100., 64., 64.)));
    // The clipper admits a short approaching ray within DIST_EPSILON even
    // without crossing the geometric face. Broad phase must retain it too.
    assert!(crate::swept_bounds_overlap(Vec3::new(-0.02, 0., 0.), Vec3::new(-0.01, 0., 0.),
        Vec3::ZERO, Vec3::ZERO, Vec3::ONE));
}

#[test]
fn runtime_hit_ownership_preserves_door_ties_and_start_solid_flags() {
    let base = fixture();
    let leaf = base.doors[0].solids(0.);
    let mut solids = leaf.clone();
    solids.extend(leaf.clone());
    let mut owners = vec![None; leaf.len()];
    owners.resize(solids.len(), Some(0));
    let world = CollisionWorld::with_runtime_solids(base.clone(), solids, owners);
    let start = Vec3::new(-16., 24., 64.);
    let end = Vec3::new(16., 24., 64.);
    let (tr, owner) = world.trace_breakable_ray(start, end, contents::MASK_SHOT);
    assert!(tr.hit());
    assert_eq!(owner, None, "the earlier door wins a coincident breakable tie");
    let owned = CollisionWorld::with_runtime_solids(base, leaf.clone(), vec![Some(0); leaf.len()]);
    assert_eq!(owned.trace_breakable_ray(start, end, contents::MASK_SHOT).1, Some(0));
    let inside = Vec3::new(0., 24., 64.);
    let (stationary, owner) = owned.trace_breakable_ray(inside, inside, contents::MASK_SHOT);
    assert!(stationary.start_solid && stationary.all_solid);
    assert_eq!(owner, Some(0));
    let (leaving, owner) = owned.trace_breakable_ray(inside, start, contents::MASK_SHOT);
    assert!(leaving.start_solid && !leaving.all_solid);
    assert_eq!(owner, None, "start-solid contents alone do not replace a hit surface");
    assert_eq!(owned.trace_breakable_ray(start, end, contents::WATER).1, None);
}

#[test]
fn exact_door_owner_obeys_static_flush_ties_runtime_order_and_masks() {
    use crate::HitOwner;
    let start = Vec3::new(-16., 24., 64.);
    let end = Vec3::new(16., 24., 64.);
    // A wall in front by less than the old surface-matching tolerance, or
    // exactly flush, must never donate its impact to the door behind it.
    for shift in [-0.01, 0.0] {
        let mut base = fixture();
        let leaf = base.doors[0].solids(0.);
        let wall = hull(Vec3::new(-0.25 + shift, 0., 0.), Vec3::new(0.25 + shift, 48., 96.));
        let map = Arc::get_mut(&mut base).unwrap();
        map.add_convex(convex_from_hull(&wall.points, &wall.triangles, contents::SOLID, 0).unwrap());
        map.finalize();
        let world = CollisionWorld::with_runtime_owners(base, leaf.clone(), vec![Some(HitOwner::Door(0)); leaf.len()]);
        let (tr, owner) = world.trace_owned_ray(start, end, contents::MASK_SHOT);
        assert!(tr.hit());
        assert_eq!(owner, None, "static winner at offset {shift}");
        assert_eq!(tr.end, world.trace_ray(start, end, contents::MASK_SHOT).end);
    }
    let base = fixture();
    let leaf = base.doors[0].solids(0.);
    for first in [HitOwner::Door(0), HitOwner::Breakable(0)] {
        let second = if first == HitOwner::Door(0) { HitOwner::Breakable(0) } else { HitOwner::Door(0) };
        let mut solids = leaf.clone(); solids.extend(leaf.clone());
        let mut owners = vec![Some(first); leaf.len()]; owners.resize(solids.len(), Some(second));
        let world = CollisionWorld::with_runtime_owners(base.clone(), solids, owners);
        let (tr, owner) = world.trace_owned_ray(start, end, contents::MASK_SHOT);
        assert!(tr.hit()); assert_eq!(owner, Some(first));
        assert_eq!(world.trace_breakable_ray(start, end, contents::MASK_SHOT).1, first.breakable());
        assert_eq!(world.trace_owned_ray(start, end, contents::WATER).1, None);
    }
    let mut masked = leaf.clone();
    for solid in &mut masked { solid.contents = contents::WATER; }
    masked.extend(leaf.clone());
    let mut owners = vec![Some(HitOwner::Door(0)); leaf.len()];
    owners.resize(masked.len(), Some(HitOwner::Breakable(0)));
    let world = CollisionWorld::with_runtime_owners(base, masked, owners);
    assert_eq!(world.trace_owned_ray(start, end, contents::MASK_SHOT).1, Some(HitOwner::Breakable(0)));
    assert_eq!(world.trace_owned_ray(start, end, contents::WATER).1, Some(HitOwner::Door(0)));
}

fn mixed_brush_door() -> (Arc<CollisionWorld>, Vec3, Vec<Vec3>) {
    let mut map = bsp::Bsp::default();
    let center = Vec3::new(20., 8., 32.);
    let authored = Mat3::from_rotation_z(17.0f32.to_radians());
    let normals: Vec<_> = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
        .into_iter().map(|normal| authored * normal).collect();
    let materials = ["fixture/metal", "fixture/wood", "fixture/concrete",
        "fixture/glass", "tools/toolsskybox", "fixture/dirt"];
    for (index, (&normal, material)) in normals.iter().zip(materials).enumerate() {
        map.planes.push(bsp::Plane { normal, dist: 16. + normal.dot(center), kind: 0 });
        map.brush_sides.push(bsp::BrushSide { plane: index as u16, texinfo: index as i16, dispinfo: -1, bevel: false });
        map.texinfo.push(bsp::TexInfo { texture_vecs: [glam::Vec4::ZERO; 2],
            lightmap_vecs: [glam::Vec4::ZERO; 2], flags: 0, texdata: index as i32 });
        map.texdata.push(bsp::TexData { reflectivity: Vec3::ONE, name_id: index as i32, width: 16, height: 16 });
        map.texdata_strings.push(material.into());
    }
    // Authored compiler bevels do not donate a material or change recovery of
    // the closed brush. Runtime world-axis/edge bevels are rebuilt as before.
    map.brush_sides.push(bsp::BrushSide { plane: 0, texinfo: 4, dispinfo: -1, bevel: true });
    map.brushes.push(bsp::Brush { first_side: 0, num_sides: 7, contents: contents::SOLID });
    map.leaf_brushes.push(0);
    let leaf = bsp::Leaf { contents: 0, cluster: 0, area: 0, mins: [-64; 3], maxs: [64; 3],
        first_leaf_face: 0, num_leaf_faces: 0, first_leaf_brush: 0, num_leaf_brushes: 1 };
    map.leafs.push(leaf);
    let mut empty_leaf = leaf; empty_leaf.num_leaf_brushes = 0;
    map.leafs.push(empty_leaf);
    let model = bsp::Model { mins: Vec3::splat(-64.), maxs: Vec3::splat(64.), origin: Vec3::ZERO,
        head_node: -1, first_face: 0, num_faces: 0 };
    let mut world_model = model; world_model.head_node = -2;
    map.models.extend([world_model, model]);
    let doc = kv::parse(r#"
        door { classname func_door_rotating model *1 targetname hinge
            origin "128 -256 48" spawnflags 272 }
        visual { classname prop_dynamic parentname hinge model models/fixture_leaf.mdl
            origin "128 -256 48" solid 6 }
    "#).unwrap();
    map.entities.extend([doc.block("door").unwrap().clone(), doc.block("visual").unwrap().clone()]);
    let base = CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &|_| None);
    assert_eq!(base.doors.len(), 1);
    assert!(base.convexes.is_empty(), "moving brush must not remain in static collision");
    (Arc::new(base), center, normals)
}

#[test]
fn rotating_brush_hit_faces_keep_material_owner_and_transformed_geometry() {
    use crate::HitOwner;
    let (base, local_center, normals) = mixed_brush_door();
    let door = &base.doors[0];
    let materials = ["fixture/metal", "fixture/wood", "fixture/concrete",
        "fixture/glass", "tools/toolsskybox", "fixture/dirt"];
    assert!(!door.has_surface(door.surface), "visual MDL is not a brush contact material");
    assert!(!door.has_surface(u32::MAX));
    assert_eq!(door.hulls[0].geometry.points.len(), 8);
    assert_eq!(door.hulls[0].geometry.triangles.len(), 12);
    for angle in [0., 37., 90., -63.] {
        let rotation = door.rotation(angle);
        let center = door.origin + rotation * local_center;
        let solids = door.solids(angle);
        let points: Vec<_> = door.hulls[0].geometry.points.iter()
            .map(|point| door.origin + rotation * *point).collect();
        let uniform = convex_from_hull(&points, &door.hulls[0].geometry.triangles,
            contents::SOLID, door.hulls[0].surface).unwrap();
        assert_eq!(solids.len(), 1);
        assert_eq!(solids[0].planes, uniform.planes, "face metadata must preserve every face/bevel");
        assert_eq!((solids[0].mins, solids[0].maxs), (uniform.mins, uniform.maxs));
        assert_eq!(solids[0].plane_surfaces.len(), solids[0].planes.len());
        assert_eq!(solids[0].plane_surfaces.iter().filter(|&&id| id != u32::MAX).count(), 6);
        let world = CollisionWorld::with_runtime_owners(base.clone(), solids, vec![Some(HitOwner::Door(0))]);
        let bare = CollisionWorld::with_runtime_owners(base.clone(), vec![uniform], vec![Some(HitOwner::Door(0))]);
        for (index, local_normal) in normals.iter().enumerate() {
            let normal = rotation * *local_normal;
            let start = center + normal * 64.;
            for ext in [Vec3::ZERO, Vec3::splat(2.)] {
                let (trace, owner) = world.trace_box_owned(start, center, -ext, ext, contents::MASK_SHOT, None);
                let (expected, expected_owner) = bare.trace_box_owned(start, center, -ext, ext, contents::MASK_SHOT, None);
                assert!(trace.hit() && !trace.start_solid);
                assert_eq!(owner, Some(HitOwner::Door(0)));
                assert_eq!(owner, expected_owner);
                assert_eq!(world.surface_name(trace.surface), materials[index], "angle {angle}, face {index}");
                assert!(door.has_surface(trace.surface), "marks must accept each actual face material");
                assert_eq!(world.is_sky(trace.surface), index == 4);
                assert!((trace.normal - normal).length() < 1e-5);
                let support = normal.abs().dot(ext);
                let contact = center + normal * (16. + support + crate::DIST_EPSILON);
                assert!((trace.end - contact).length() < 2e-4);
                assert!((trace.plane_dist - (normal.dot(center) + 16. + support)).abs() < 2e-4);
                assert_eq!(trace.fraction.to_bits(), expected.fraction.to_bits());
                assert_eq!(trace.end, expected.end);
                assert_eq!(trace.normal, expected.normal);
                assert_eq!(trace.plane_dist.to_bits(), expected.plane_dist.to_bits());
                if ext == Vec3::ZERO {
                    // A mark's hinge-local point/normal must still be on the
                    // same material face after the leaf moves to another pose.
                    let local = rotation.transpose() * (trace.end - door.origin);
                    let moved_rotation = door.rotation(angle + 23.);
                    let moved_point = door.origin + moved_rotation * local;
                    let moved_normal = moved_rotation * *local_normal;
                    let moved = CollisionWorld::with_runtime_owners(base.clone(), door.solids(angle + 23.),
                        vec![Some(HitOwner::Door(0))]);
                    let (hit, owner) = moved.trace_owned_ray(moved_point + moved_normal * 8.,
                        moved_point - moved_normal * 8., contents::MASK_SHOT);
                    assert_eq!(owner, Some(HitOwner::Door(0)));
                    assert_eq!(hit.surface, trace.surface);
                    assert!((hit.end - moved_point).length() < 2e-4);
                }
            }
        }
    }
}

#[test]
fn distant_rotating_brush_matches_materials_to_the_rebuilt_face_planes() {
    let (mut base, local_center, normals) = mixed_brush_door();
    let world = Arc::get_mut(&mut base).unwrap();
    Arc::get_mut(&mut world.doors).unwrap()[0].origin = Vec3::new(32768., -49152., 8192.);
    let door = &base.doors[0];
    let materials = ["fixture/metal", "fixture/wood", "fixture/concrete",
        "fixture/glass", "tools/toolsskybox", "fixture/dirt"];
    for angle in [37., -63.] {
        let rotation = door.rotation(angle);
        let center = door.origin + rotation * local_center;
        let world = CollisionWorld::with_runtime_owners(base.clone(), door.solids(angle),
            vec![Some(crate::HitOwner::Door(0))]);
        for (normal, material) in normals.iter().zip(materials) {
            let (trace, owner) = world.trace_owned_ray(center + rotation * *normal * 64., center, contents::MASK_SHOT);
            assert_eq!(owner, Some(crate::HitOwner::Door(0)));
            assert_eq!(world.surface_name(trace.surface), material);
            assert!(door.has_surface(trace.surface));
        }
    }
}

#[test]
fn rotating_phy_door_retains_uniform_model_surface_and_bevels() {
    let base = fixture();
    let door = &base.doors[0];
    assert!(door.has_surface(door.surface));
    assert!(!door.has_surface(u32::MAX));
    for angle in [0., 37., -90.] {
        let rotation = door.rotation(angle);
        let solids = door.solids(angle);
        assert!(solids[0].plane_surfaces.is_empty());
        let points: Vec<_> = door.hulls[0].geometry.points.iter()
            .map(|point| door.origin + rotation * *point).collect();
        let expected = convex_from_hull(&points, &door.hulls[0].geometry.triangles,
            contents::SOLID, door.surface).unwrap();
        assert_eq!(solids[0].planes, expected.planes);
        assert_eq!((solids[0].mins, solids[0].maxs), (expected.mins, expected.maxs));
        let normal = rotation * -Vec3::X;
        let center = door.center_at(angle);
        let world = CollisionWorld::with_runtime_owners(base.clone(), solids, vec![Some(crate::HitOwner::Door(0))]);
        let (trace, owner) = world.trace_owned_ray(center + normal * 64., center, contents::MASK_SHOT);
        assert_eq!(owner, Some(crate::HitOwner::Door(0)));
        assert_eq!(trace.surface, door.surface);
        assert_eq!(world.surface_name(trace.surface), "models/fixture_leaf.mdl");
    }
}

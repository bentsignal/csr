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

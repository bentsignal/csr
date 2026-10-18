//! Source-only fixtures for the map-loader -> TickInput -> collision/snapshot
//! path. UNRUN during the parent's exclusive performance/build slot.
use super::*;
use crate::{PlayerId, SpawnPoint, Team};
use collision::{CollisionWorld, MapCollisionOptions};
use std::sync::Arc;

fn hull() -> mdl::ConvexHull {
    let points = vec![Vec3::new(-2., 0., 0.), Vec3::new(2., 0., 0.),
        Vec3::new(2., 48., 0.), Vec3::new(-2., 48., 0.),
        Vec3::new(-2., 0., 96.), Vec3::new(2., 0., 96.),
        Vec3::new(2., 48., 96.), Vec3::new(-2., 48., 96.)];
    let triangles = vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7],
        [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5],
        [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
    mdl::ConvexHull { points, triangles }
}

fn fixture(rate: u32, extra: &str) -> (World, PlayerId) {
    let mut map = bsp::Bsp::default();
    let doc = kv::parse(&format!(r#"door {{ classname prop_door_rotating
        model models/fixture_leaf.mdl origin "0 0 0" angles "0 0 0"
        distance 90 speed 180 soundmoveoverride Fixture.Move
        soundopenoverride Fixture.Open soundcloseoverride Fixture.Close {extra} }}"#)).unwrap();
    map.entities.push(doc.block("door").unwrap().clone());
    let mut collision = CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &|_| Some(vec![hull()]));
    let size = 2048.;
    collision.add_triangle([Vec3::new(-size, -size, 0.), Vec3::new(size, -size, 0.), Vec3::new(size, size, 0.)], contents::SOLID, 0);
    collision.add_triangle([Vec3::new(-size, -size, 0.), Vec3::new(size, size, 0.), Vec3::new(-size, size, 0.)], contents::SOLID, 0);
    collision.finalize();
    assert_eq!(collision.doors.len(), 1);
    let mut world = World::new(Arc::new(collision));
    world.set_tick_rate(rate);
    world.spawns.push(SpawnPoint { origin: Vec3::new(-60., 24., 1.), yaw: 0., team: Team::CounterTerrorist });
    let id = world.add_player("fixture", Team::CounterTerrorist);
    (world, id)
}

fn use_input(yaw: f32) -> TickInput {
    TickInput { buttons: buttons::USE, view_angles: Vec3::new(0., yaw, 0.), ..Default::default() }
}

fn blocked(world: &World) -> bool {
    let from = Vec3::new(-80., 24., 1.);
    let to = Vec3::new(80., 24., 1.);
    world.collision.trace_box(from, to, Vec3::new(-16., -16., 0.), Vec3::new(16., 16., 72.), contents::MASK_PLAYERSOLID).hit()
}

#[test]
fn use_opens_holds_and_closes_real_collision_at_64_and_128_ticks() {
    for rate in [64, 128] {
        let (mut world, id) = fixture(rate, "");
        assert!(blocked(&world));
        // A cloned world must retain its closed pose while this copy opens.
        let untouched = world.clone();
        let mut open_cues = 0;
        for _ in 0..rate * 2 {
            world.step(&[(id, use_input(0.))]);
            open_cues += world.events.iter().filter(|event| matches!(event, GameEvent::DoorSound { cue: Cue::Open, .. })).count();
        }
        assert_eq!(world.doors[0].angle, -90.);
        assert_eq!(open_cues, 1, "held USE must not retrigger");
        assert!(!blocked(&world));
        assert!(blocked(&untouched));
        let mut walker = world.clone();
        let mut stopped = untouched;
        for _ in 0..rate {
            let walk = [(id, TickInput { forward_move: 450.0, ..Default::default() })];
            walker.step(&walk); stopped.step(&walk);
        }
        assert!(walker.player(id).unwrap().movement.origin.x > 80.0);
        assert!(stopped.player(id).unwrap().movement.origin.x < -17.0);
        // Aim at the now-open leaf rather than its old closed location.
        world.step(&[(id, TickInput::default())]);
        let yaw = (-24.0f32).atan2(84.0).to_degrees();
        world.step(&[(id, use_input(yaw))]);
        for _ in 0..rate { world.step(&[(id, use_input(yaw))]); }
        assert_eq!(world.doors[0].angle, 0.);
        assert!(blocked(&world));
    }
}

#[test]
fn restored_and_predicted_door_poses_step_identically_without_shared_mutation() {
    let (mut authority, id) = fixture(128, "");
    authority.step(&[(id, use_input(0.))]);
    let snapshot: crate::WorldState = authority.decode(&authority.encode_state()).unwrap();
    let mut restored = fixture(128, "").0;
    restored.restore(snapshot);
    let mut predicted = authority.clone();
    for _ in 0..128 {
        let input = [(id, use_input(0.))];
        authority.step(&input); restored.step(&input); predicted.step_prediction(&input);
        assert_eq!(authority.state(), restored.state());
        assert_eq!(authority.state(), predicted.state());
        assert!(predicted.events.is_empty());
    }
    let closed: crate::WorldState = fixture(128, "").0.state();
    restored.restore(closed);
    assert!(blocked(&restored));
    assert!(!blocked(&authority));
}

#[test]
fn obstruction_stops_the_leaf_then_released_space_allows_completion() {
    let (mut world, id) = fixture(128, "");
    world.step(&[(id, use_input(0.))]);
    let blocker = world.add_player("occupant", Team::CounterTerrorist);
    world.player_mut(blocker).unwrap().movement.origin = Vec3::new(24., 24., 1.);
    for _ in 0..128 { world.step(&[(id, use_input(0.))]); }
    assert!(world.doors[0].angle.abs() < 90.);
    let angle = world.doors[0].angle;
    for _ in 0..32 { world.step(&[(id, use_input(0.))]); }
    assert_eq!(world.doors[0].angle, angle);
    world.player_mut(blocker).unwrap().movement.origin = Vec3::new(200., 200., 1.);
    for _ in 0..128 { world.step(&[(id, use_input(0.))]); }
    assert_eq!(world.doors[0].angle, -90.);
}

#[test]
fn locked_occluded_and_out_of_reach_doors_ignore_use() {
    for case in 0..3 {
        let (mut world, id) = fixture(128, if case == 0 { "spawnflags 2048" } else { "" });
        if case == 1 {
            // A real map solid before the leaf, not a mocked selection result.
            let solids = collision::convex_from_hull(&hull().points.iter().map(|p| *p - Vec3::X * 30.).collect::<Vec<_>>(),
                &hull().triangles, contents::SOLID, 0).unwrap();
            // Drop the pose overlay's Arc before mutating this fixture's map.
            world.collision = CollisionWorld::map_arc(&world.collision);
            let base = Arc::get_mut(&mut world.collision).unwrap();
            base.add_convex(solids); base.finalize(); world.rebuild_door_collision();
        }
        if case == 2 { world.player_mut(id).unwrap().movement.origin.x = -160.; }
        world.step(&[(id, use_input(0.))]);
        assert_eq!(world.doors[0].target, 0., "case {case}");
    }
}

#[test]
fn automatic_return_round_reset_and_map_dependent_state_admission() {
    let (mut world, id) = fixture(64, "returndelay 0.25");
    world.step(&[(id, use_input(0.))]);
    for _ in 0..128 { world.step(&[(id, TickInput::default())]); }
    assert_eq!(world.doors[0].angle, 0.);
    world.step(&[(id, use_input(0.))]);
    assert_ne!(world.doors[0].angle, 0.);
    world.start_round(false);
    assert_eq!(world.doors[0].angle, 0.);
    assert!(blocked(&world));
    assert!(world.validate_door_state(&[]).is_err());
    let mut bad = world.doors.clone(); bad[0].angle = f32::NAN;
    assert!(world.validate_door_state(&bad).is_err());
    bad[0].angle = 100.;
    assert!(world.validate_door_state(&bad).is_err());
}

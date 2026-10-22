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

#[test]
fn moving_door_releases_supported_kit_at_64_and_128_ticks() {
    for rate in [64, 128] {
        let (mut world, id) = fixture(rate, "");
        let pos = Vec3::new(0., 24., 97. + collision::DIST_EPSILON);
        world.new_item(crate::GroundKind::DefuseKit, pos, 37., id, 0.);
        world.step(&[]);
        assert!(world.items[0].settled);
        assert_eq!(world.items[0].pos, pos);
        for _ in 0..rate { world.step(&[(id, use_input(0.))]); }
        let item = &world.items[0];
        assert_ne!(world.doors[0].angle, 0.);
        assert!(item.settled && item.vertical_velocity == 0.);
        assert!((item.pos.z - 1. - collision::DIST_EPSILON).abs() < 0.001);
        assert_eq!(item.pos.truncate(), pos.truncate());
        assert_eq!(item.yaw, 37.);
        assert!(matches!(item.kind, crate::GroundKind::DefuseKit));
    }
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

#[test]
fn door_use_and_ladder_climbing_survive_combined_snapshot_prediction_64_128() {
    for rate in [64, 128] {
        let (mut world, user) = fixture(rate, "");
        world.collision = CollisionWorld::map_arc(&world.collision);
        let base = Arc::get_mut(&mut world.collision).unwrap();
        // A separate route in the same map. Adding any supported door must
        // not disable the authored ladder index for the whole level.
        let shape = hull();
        let points: Vec<_> = shape.points.iter().map(|p|
            Vec3::new((p.x + 2.) * 0.25, p.y + 200., p.z * 4.)).collect();
        base.add_convex(collision::convex_from_hull(&points, &shape.triangles,
            contents::LADDER | contents::GRATE, 0).unwrap());
        base.finalize();
        world.rebuild_door_collision();
        let climber = world.add_player("climber", Team::CounterTerrorist);
        world.player_mut(climber).unwrap().movement = movement::PlayerMove::spawn_at(
            Vec3::new(-16. - collision::DIST_EPSILON, 224., 64.));
        let inputs = [(user, use_input(0.)), (climber, TickInput {
            forward_move: 450., ..Default::default()
        })];
        world.step(&inputs);
        assert_ne!(world.doors[0].angle, 0.);
        assert_eq!(world.player(climber).unwrap().movement.ladder_normal, Some(-Vec3::X));
        let state: crate::WorldState = world.decode(&world.encode_state()).unwrap();
        let mut predicted = world.clone();
        predicted.restore(state);
        for _ in 0..rate / 2 {
            world.step(&inputs);
            predicted.step_prediction(&inputs);
            assert_eq!(world.state(), predicted.state());
            assert_eq!(world.player(climber).unwrap().movement.ladder_normal, Some(-Vec3::X));
            assert!(predicted.events.is_empty());
        }
        assert!(world.player(climber).unwrap().movement.origin.z > 128.);
        assert!(!blocked(&world));
    }
}

#[test]
fn tick_input_impacts_capture_shot_pose_and_survive_prediction_restore_and_death() {
    for angle in [0., -37., -90.] {
        let (mut world, id) = fixture(128, "");
        let catalog = weapons::Catalog::from_items_game(r##"items_game { items {
            999 { name weapon_fixture item_class weapon_fixture baseitem 1 item_type_name "#CSGO_Type_Rifle"
                attributes { damage 20 "range modifier" 1 "primary clip size" 30 cycletime 0.1 } }
        } }"##).unwrap();
        world.players[0].weapons = vec![crate::weapon::WeaponState::new(Arc::new(catalog.get("weapon_fixture").unwrap().clone()))];
        world.players[0].active = 0;
        world.catalog = Arc::new(catalog);
        world.doors[0].angle = angle; world.doors[0].target = angle;
        let def = &world.collision.doors[0];
        world.players[0].movement.origin = def.origin + def.rotation(angle) * Vec3::new(-60., 24., 1.);
        world.rebuild_door_collision();
        let mut predicted = world.clone();
        let input = [(id, TickInput { buttons: buttons::ATTACK,
            view_angles: Vec3::new(0., angle, 0.), ..Default::default() })];
        world.step(&input); predicted.step_prediction(&input);
        assert_eq!(world.state(), predicted.state()); assert!(predicted.events.is_empty());
        let (pos, normal, impact) = world.events.iter().find_map(|event| match event {
            GameEvent::Impact { pos, normal, door: Some(impact), .. } => Some((*pos, *normal, *impact)),
            _ => None,
        }).expect("the real bullet hit the moving door");
        assert!(impact.valid()); assert_eq!(impact.door, 0);
        let def = &world.collision.doors[0];
        assert!((def.origin + def.rotation(angle) * impact.pos - pos).length() < 0.001);
        assert!((def.rotation(angle) * impact.normal - normal).length() < 0.001);
        let state: crate::WorldState = world.decode(&world.encode_state()).unwrap();
        predicted.restore(state); assert_eq!(predicted.state(), world.state());
        let before = world.doors.clone();
        world.damage_environment(id, 1000, "fixture");
        assert_eq!(world.doors, before, "shooter death must not relocate a map-owned mark");
        world.start_round(false);
        assert_eq!(world.doors[0].angle, 0.);
        let eye = Vec3::new(-60., 24., 64.);
        let (tr, owner) = world.collision.trace_owned_ray(eye, eye + Vec3::X * 120., contents::MASK_SHOT);
        assert_eq!(owner, Some(collision::HitOwner::Door(0)));
        assert!(world.door_impact(owner, &tr).is_some());
        assert!(world.door_impact(None, &tr).is_none());
        let inside = Vec3::new(0., 24., 64.);
        let (tr, owner) = world.collision.trace_owned_ray(inside, inside, contents::MASK_SHOT);
        assert!(world.door_impact(owner, &tr).is_none(), "no decal for an undefined start-solid contact");
    }
}


/// Compiled brush plus a separately positioned visual leaf, deliberately with
/// no PHY. This exercises map admission, tick collision and snapshot together.
fn brush_fixture(rate: u32) -> (World, PlayerId) {
    brush_fixture_with_contents(rate, contents::SOLID)
}

fn brush_fixture_with_contents(rate: u32, mask: i32) -> (World, PlayerId) {
    let mut map = bsp::Bsp::default();
    let doc = kv::parse(r#"
        hinge { classname func_door_rotating model *1 targetname hinge
            origin "100 200 50" angles "0 0 0" spawnflags 65842
            speed 180 distance 90 wait 4 }
        leaf { classname prop_dynamic parentname hinge model models/fixture.mdl
            origin "100 224 2" angles "0 90 0" solid 6 }
    "#).unwrap();
    map.entities = ["hinge", "leaf"].map(|name| doc.block(name).unwrap().clone()).into();
    let mins = Vec3::new(-2., 0., -50.); let maxs = Vec3::new(2., 48., 46.);
    map.models = vec![
        bsp::Model { mins, maxs, origin: Vec3::ZERO, head_node: -1, first_face: 0, num_faces: 0 },
        bsp::Model { mins, maxs, origin: Vec3::ZERO, head_node: -2, first_face: 0, num_faces: 0 },
    ];
    let leaf = bsp::Leaf { contents: 0, cluster: -1, area: 0, mins: [0; 3], maxs: [0; 3],
        first_leaf_face: 0, num_leaf_faces: 0, first_leaf_brush: 0, num_leaf_brushes: 0 };
    map.leafs = vec![leaf, bsp::Leaf { num_leaf_brushes: 1, ..leaf }];
    map.leaf_brushes = vec![0];
    map.planes = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
        .into_iter().zip([maxs.x, -mins.x, maxs.y, -mins.y, maxs.z, -mins.z])
        .map(|(normal, dist)| bsp::Plane { normal, dist, kind: 0 }).collect();
    map.brush_sides = (0..6).map(|plane| bsp::BrushSide { plane, texinfo: -1, dispinfo: -1, bevel: false }).collect();
    map.brushes = vec![bsp::Brush { first_side: 0, num_sides: 6, contents: mask }];
    let mut collision = CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &|_| None);
    assert_eq!(collision.doors.len(), 1);
    let size = 2048.;
    collision.add_triangle([Vec3::new(-size,-size,0.), Vec3::new(size,-size,0.), Vec3::new(size,size,0.)], contents::SOLID, 0);
    collision.add_triangle([Vec3::new(-size,-size,0.), Vec3::new(size,size,0.), Vec3::new(-size,size,0.)], contents::SOLID, 0);
    collision.finalize();
    let mut world = World::new(Arc::new(collision)); world.set_tick_rate(rate);
    world.spawns.push(SpawnPoint { origin: Vec3::new(40.,224.,1.), yaw: 0., team: Team::CounterTerrorist });
    let id = world.add_player("fixture", Team::CounterTerrorist);
    (world, id)
}

#[test]
fn parented_visual_brush_door_moves_collision_once_and_preserves_toggle_and_snapshot() {
    for rate in [64, 128] {
        let (mut world, id) = brush_fixture(rate);
        let def = &world.collision.doors[0];
        assert_eq!(def.brush_model, Some(1)); assert_eq!(def.visual_entity, 1);
        let (closed, _) = def.visual_pose(0.);
        assert!((closed - Vec3::new(100.,224.,2.)).length() < 1e-5);
        let (opened, rotation) = def.visual_pose(-90.);
        assert!((opened - Vec3::new(124.,200.,2.)).length() < 1e-4);
        assert!((rotation * Vec3::X - Vec3::X).length() < 1e-4);
        let ray = |world: &World| world.collision.trace_owned_ray(Vec3::new(40.,224.,64.),
            Vec3::new(180.,224.,64.), contents::MASK_SHOT);
        assert_eq!(ray(&world).1, Some(collision::HitOwner::Door(0)));
        let untouched = world.clone();
        for _ in 0..rate * 6 { world.step(&[(id, use_input(0.))]); }
        assert_eq!(world.doors[0].angle, -90.);
        assert!(!ray(&world).0.hit(), "the closed brush must not remain in immutable collision");
        assert!(ray(&untouched).0.hit());
        let state = world.decode(&world.encode_state()).unwrap();
        let mut restored = brush_fixture(rate).0; restored.restore(state);
        assert_eq!(world.state(), restored.state());
        for _ in 0..rate {
            let input = [(id, TickInput { forward_move: 450., ..Default::default() })];
            world.step(&input); restored.step_prediction(&input);
            assert_eq!(world.state(), restored.state());
        }
        assert!(world.player(id).unwrap().movement.origin.x > 180.);
    }
}


#[test]
fn moving_brush_preserves_grate_and_player_clip_masks_across_rotations() {
    for contents in [collision::contents::GRATE, collision::contents::PLAYERCLIP] {
        let (mut world, _) = brush_fixture_with_contents(128, contents);
        for angle in [0., -45., -90.] {
            world.doors[0].angle=angle; world.doors[0].target=angle;
            world.rebuild_door_collision();
            let def=&world.collision.doors[0];
            let center=def.center_at(angle);
            let normal=def.rotation(angle)*Vec3::X;
            let a=center-normal*60.; let b=center+normal*60.;
            assert!(world.collision.trace_ray(a,b,collision::contents::MASK_PLAYERSOLID).hit());
            assert!(!world.collision.trace_ray(a,b,collision::contents::MASK_SHOT).hit());
            let state=world.decode(&world.encode_state()).unwrap();
            let mut restored=brush_fixture_with_contents(128,contents).0;
            restored.restore(state);
            assert_eq!(restored.state(),world.state());
            assert!(!restored.collision.trace_ray(a,b,collision::contents::MASK_SHOT).hit());
        }
    }
}

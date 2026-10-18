//! Synthetic authored BSP -> ordinary TickInput -> health/collision/state.
//! Source added only; all tests UNRUN under the parent's exclusive build slot.
use super::*;
use crate::{GameEvent, PlayerId, SpawnPoint, Team, TickInput, weapon::WeaponState};
use collision::{CollisionWorld, MapCollisionOptions, contents};
use glam::Vec3;
use movement::buttons;
use std::sync::Arc;

fn fixture(rate: u32, metadata: &str, door: bool) -> (World, PlayerId) {
    fixture_with_model(rate, metadata, door, None)
}

fn fixture_with_model(rate: u32, metadata: &str, door: bool, prop_data: Option<&str>) -> (World, PlayerId) {
    let mut map = bsp::Bsp::default();
    let doc = kv::parse(&format!(r#"vent {{ classname func_breakable model *1
        origin "0 0 0" {metadata} }}"#)).unwrap();
    map.entities.push(doc.block("vent").unwrap().clone());
    let bounds = (Vec3::new(-2., -32., 0.), Vec3::new(2., 32., 96.));
    for (normal, dist) in [(Vec3::X, 2.), (-Vec3::X, 2.), (Vec3::Y, 32.),
        (-Vec3::Y, 32.), (Vec3::Z, 96.), (-Vec3::Z, 0.)] {
        map.planes.push(bsp::Plane { normal, dist, kind: 0 });
    }
    map.brush_sides = (0..6).map(|plane| bsp::BrushSide { plane, texinfo: -1, dispinfo: -1, bevel: false }).collect();
    map.brushes.push(bsp::Brush { first_side: 0, num_sides: 6, contents: contents::SOLID });
    for (head_node, num_faces) in [(-1, 0), (-2, 0)] {
        map.models.push(bsp::Model { mins: bounds.0, maxs: bounds.1, origin: Vec3::ZERO,
            head_node, first_face: 0, num_faces });
    }
    for num_leaf_brushes in [0, 1] {
        map.leafs.push(bsp::Leaf { contents: 0, cluster: -1, area: 0, mins: [-64; 3], maxs: [128; 3],
            first_leaf_face: 0, num_leaf_faces: 0, first_leaf_brush: 0, num_leaf_brushes });
    }
    map.leaf_brushes.push(0);
    if door {
        let doc = kv::parse(r#"door { classname prop_door_rotating model models/fixture.mdl
            origin "100 0 0" angles "0 0 0" distance 90 speed 180 }"#).unwrap();
        map.entities.push(doc.block("door").unwrap().clone());
    }
    let loader = |_: &str| Some(vec![mdl::ConvexHull {
        points: vec![Vec3::new(-2., -32., 0.), Vec3::new(2., -32., 0.), Vec3::new(2., 32., 0.), Vec3::new(-2., 32., 0.),
            Vec3::new(-2., -32., 96.), Vec3::new(2., -32., 96.), Vec3::new(2., 32., 96.), Vec3::new(-2., 32., 96.)],
        triangles: vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4],
            [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]],
    }]);
    if prop_data.is_some() {
        let mut button = map.entities[0].clone();
        for (key, value) in [("classname", "func_button"), ("targetname", "release_fixture"), ("spawnflags", "1")] {
            button.set(key, kv::Value::Str(value.into()));
        }
        map.entities.push(button);
        let doc = kv::parse(r#"cover { classname prop_dynamic model models/private_cover.mdl solid 6
            origin "0 0 0" angles "0 0 0" rendercolor "121 173 211" skin 2 body 3
            OnBreak "release_fixture,Kill,,0,-1" }"#).unwrap();
        map.entities[0] = doc.block("cover").unwrap().clone();
    }
    let model_loader = |_: &str| prop_data.map(|data| {
        let text = format!("mdlkeyvalue {{ prop_data {{ {data} }} }}\0");
        let mut bytes = vec![0; 384]; bytes[..4].copy_from_slice(b"IDST");
        bytes[4..8].copy_from_slice(&49i32.to_le_bytes());
        bytes[312..316].copy_from_slice(&384i32.to_le_bytes());
        bytes[316..320].copy_from_slice(&(text.len() as i32).to_le_bytes());
        bytes.extend_from_slice(text.as_bytes()); bytes
    });
    let presets = kv::parse(r#"MetalFixture { base BaseFixture } BaseFixture { health 0 "dmg.bullets" 1 "dmg.club" 1 }"#).unwrap();
    let mut collision = CollisionWorld::from_bsp_with_model_data(&map, &MapCollisionOptions::default(),
        &loader, &model_loader, &presets);
    assert_eq!(collision.breakables.len(), 1);
    let s = 2048.;
    collision.add_triangle([Vec3::new(-s, -s, 0.), Vec3::new(s, -s, 0.), Vec3::new(s, s, 0.)], contents::SOLID, 0);
    collision.add_triangle([Vec3::new(-s, -s, 0.), Vec3::new(s, s, 0.), Vec3::new(-s, s, 0.)], contents::SOLID, 0);
    collision.finalize();
    let catalog = weapons::Catalog::from_items_game(r##"items_game { items {
        999 { name weapon_fixture item_class weapon_fixture baseitem 1 item_type_name "#CSGO_Type_Rifle"
            attributes { damage 20 "range modifier" 1 "primary clip size" 30 "is full auto" 1 cycletime 0.1 } }
        42 { name weapon_knife item_class weapon_knife baseitem 1 }
    } }"##).unwrap();
    let gun = catalog.get("weapon_fixture").unwrap().clone();
    let mut world = World::with_catalog(Arc::new(collision), Arc::new(catalog));
    world.set_tick_rate(rate);
    world.spawns.push(SpawnPoint { origin: Vec3::new(-28., 0., 1.), yaw: 0., team: Team::CounterTerrorist });
    let id = world.add_player("breaker", Team::CounterTerrorist);
    world.players[0].weapons = vec![WeaponState::new(Arc::new(gun))];
    world.players[0].active = 0;
    (world, id)
}

fn attack() -> TickInput { TickInput { buttons: buttons::ATTACK, ..Default::default() } }
fn blocked(world: &World) -> bool {
    world.collision.trace_box(Vec3::new(-64., 0., 1.), Vec3::new(64., 0., 1.),
        Vec3::new(-16., -16., 0.), Vec3::new(16., 16., 72.), contents::MASK_PLAYERSOLID).hit()
}

#[test]
fn authored_damage_breaks_collision_then_restore_prediction_and_round_reset_agree() {
    for rate in [64, 128] {
        let (mut world, id) = fixture(rate, "health 15 material 2 \"dmg.bullets\" 0.5", true);
        assert!(blocked(&world));
        let intact = world.clone();
        world.step(&[(id, attack())]);
        assert_eq!(world.breakables[0], State { health: 5., destroyed: false });
        assert!(blocked(&world));
        let state = world.decode::<crate::WorldState>(&world.encode_state()).unwrap();
        let mut restored = intact.clone(); restored.restore(state);
        let mut predicted = world.clone();
        for _ in 0..rate / 2 {
            let input = [(id, attack())];
            world.step(&input); restored.step(&input); predicted.step_prediction(&input);
            assert_eq!(world.state(), restored.state()); assert_eq!(world.state(), predicted.state());
            assert!(predicted.events.is_empty());
        }
        assert!(world.breakables[0].broken()); assert!(!blocked(&world)); assert!(blocked(&intact));
        // The other runtime owner must survive a vent's removal/reconciliation.
        assert!(world.collision.trace_ray(Vec3::new(64., 0., 64.), Vec3::new(128., 0., 64.), contents::MASK_SHOT).hit());
        for _ in 0..rate / 2 { world.step(&[(id, TickInput { forward_move: 450., ..Default::default() })]); }
        assert!(world.players[0].movement.origin.x > 18.);
        world.damage_environment(id, 1000, "fixture");
        assert!(world.breakables[0].broken(), "individual death must not reset map state");
        restored.restore(intact.state()); assert!(blocked(&restored));
        world.start_round(false); assert_eq!(world.breakables, intact.breakables); assert!(blocked(&world));
    }
}

#[test]
fn knife_uses_real_tick_input_and_authored_club_scale_without_fire_events() {
    for secondary in [false, true] {
        let (mut world, id) = fixture(128, "health 50 material 2 \"dmg.club\" 2", false);
        let knife = world.catalog.get("weapon_knife").unwrap().clone();
        world.players[0].weapons = vec![WeaponState::new(Arc::new(knife))];
        world.step(&[(id, TickInput { buttons: if secondary { buttons::ATTACK2 } else { buttons::ATTACK }, ..Default::default() })]);
        assert!(world.breakables[0].broken()); assert!(!blocked(&world));
        assert!(world.events.iter().any(|e| matches!(e, GameEvent::Melee { hit: crate::MeleeHit::World, .. })));
        assert!(!world.events.iter().any(|e| matches!(e, GameEvent::Fire { .. } | GameEvent::Tracer { .. })));
    }
}

#[test]
fn indestructible_trigger_only_minimum_damage_and_occlusion_cannot_be_bypassed() {
    for metadata in ["health 0", "health 1 material 7", "health 1 spawnflags 1", "health 1 minhealthdmg 21"] {
        let (mut world, id) = fixture(128, metadata, false);
        let initial = world.breakables.clone();
        for _ in 0..128 { world.step(&[(id, attack())]); }
        assert_eq!(world.breakables, initial); assert!(blocked(&world));
    }
    let (mut world, id) = fixture(128, "health 1", false);
    world.collision = CollisionWorld::map_arc(&world.collision);
    let collision = Arc::get_mut(&mut world.collision).unwrap();
    collision.add_convex(collision::Convex { planes: vec![
        collision::Plane { normal: Vec3::X, dist: -10. }, collision::Plane { normal: -Vec3::X, dist: 12. },
        collision::Plane { normal: Vec3::Y, dist: 64. }, collision::Plane { normal: -Vec3::Y, dist: 64. },
        collision::Plane { normal: Vec3::Z, dist: 96. }, collision::Plane { normal: -Vec3::Z, dist: 0. }],
        mins: Vec3::new(-12., -64., 0.), maxs: Vec3::new(-10., 64., 96.), contents: contents::SOLID, surface: 0 });
    collision.finalize(); world.rebuild_door_collision();
    world.step(&[(id, attack())]);
    assert_eq!(world.breakables[0].health, 1.);
}

#[test]
fn map_admission_rejects_invalid_health_or_removal_and_propdata_inheritance_is_bounded() {
    let (world, _) = fixture(128, "health 40 propdata 24", false);
    assert!(world.validate_breakable_state(&[]).is_err());
    for state in [State { health: f32::NAN, destroyed: false }, State { health: 41., destroyed: false },
        State { health: 1., destroyed: true }, State { health: 0., destroyed: false }] {
        assert!(world.validate_breakable_state(&[state]).is_err());
    }
    assert!(world.validate_breakable_state(&[State { health: 0., destroyed: true }]).is_ok());
    let mut collision = fixture(128, "health 40 propdata 24", false).0.collision;
    collision = CollisionWorld::map_arc(&collision);
    let data = kv::parse(r#"Glass.Window { base fixture health 1 "dmg.bullets" 0.5 }
        fixture { "dmg.club" 2 } cycle { base cycle }"#).unwrap();
    let base = Arc::get_mut(&mut collision).unwrap();
    let def = &mut Arc::get_mut(&mut base.breakables).unwrap()[0];
    def.apply_prop_data(&data);
    assert_eq!((def.health, def.bullet_scale, def.club_scale), (1., 0.5, 2.));
    def.prop_data = "cycle".into(); def.apply_prop_data(&data);
    assert_eq!(def.health, 1.);
}

#[test]
fn combat_damages_only_the_exact_masked_first_brush_owner() {
    // A wall within the old 1e-6 fraction tolerance, and an exactly flush wall,
    // must both retain their hit instead of transferring damage to the vent.
    for front in [-2.004, -2.0] {
        let (mut world, id) = fixture(128, "health 1", false);
        world.collision = CollisionWorld::map_arc(&world.collision);
        let base = Arc::get_mut(&mut world.collision).unwrap();
        let mut wall = base.breakables[0].solids[0].clone();
        wall.mins.x = front;
        wall.maxs.x = front + 0.001;
        for plane in &mut wall.planes {
            if plane.normal == -Vec3::X { plane.dist = -front; }
            if plane.normal == Vec3::X { plane.dist = wall.maxs.x; }
        }
        base.add_convex(wall);
        base.finalize();
        world.rebuild_door_collision();
        let start = world.players[0].eye();
        let end = start + Vec3::X * 8192.;
        let (hit, owner) = world.collision.trace_breakable_ray(start, end, contents::MASK_SHOT);
        let vent = collision::doors::ray_fraction(&world.collision.breakables[0].solids, start, end).unwrap();
        assert!((vent - hit.fraction).abs() <= 0.000001, "exercise the former false match");
        assert_eq!(owner, None);
        world.step(&[(id, attack())]);
        assert!(world.events.iter().any(|e| matches!(e, GameEvent::Fire { .. })));
        assert_eq!(world.breakables[0].health, 1.);
    }
    // A mask-excluded brush must not steal damage from a coincident shot-solid
    // vent. Among actual equal hits the original convex order wins.
    for first_contents in [contents::PLAYERCLIP, contents::SOLID] {
        let (mut world, id) = fixture(128, "health 1", false);
        world.collision = CollisionWorld::map_arc(&world.collision);
        let base = Arc::get_mut(&mut world.collision).unwrap();
        let solids = base.breakables[0].solids.clone();
        let make = |entity, contents| {
            let mut solids = solids.clone();
            for solid in &mut solids { solid.contents = contents; }
            collision::breakables::Breakable { entity, model: 1, origin: Vec3::ZERO,
                health: 1., material: 0, prop_data: String::new(), bullet_scale: 1., club_scale: 1.,
                minimum_damage: 0., trigger_only: false, solids, studio: None }
        };
        base.breakables = vec![make(0, first_contents), make(1, contents::SOLID)].into();
        world.reset_breakables();
        world.rebuild_door_collision();
        let start = world.players[0].eye();
        let expected = if first_contents == contents::SOLID { 0 } else { 1 };
        assert_eq!(world.collision.trace_breakable_ray(start, start + Vec3::X * 8192., contents::MASK_SHOT).1,
            Some(expected));
        world.step(&[(id, attack())]);
        assert!(world.breakables[expected].broken());
        assert!(!world.breakables[1 - expected].broken());
        assert!(blocked(&world), "the other intact movement-solid owner remains");
        world.start_round(false);
        assert!(world.breakables.iter().all(|state| !state.broken()));
    }
}


#[test]
fn authored_studio_cover_shot_and_knife_remove_linked_brush_and_restore_as_one_state() {
    for rate in [64, 128] {
        for knife in [false, true] {
            let (mut world, id) = fixture_with_model(rate, "health 999", true,
                Some(r#"base MetalFixture health 1"#));
            let def = &world.collision.breakables[0];
            assert_eq!(def.entity, 0);
            assert_eq!(def.health, 1., "local MDL health overrides the inherited zero");
            let studio = def.studio.as_ref().unwrap();
            assert_eq!((studio.skin, studio.body), (2, 3));
            assert_eq!(studio.tint, Vec3::new(121., 173., 211.) / 255.);
            assert_eq!(world.collision.breakable_dependencies.len(), 1);
            assert_eq!(world.collision.breakable_dependencies[0].killed_by, vec![0]);
            assert_eq!(world.collision.breakable_dependencies[0].solids.len(), 1);
            assert!(blocked(&world));
            if knife {
                world.players[0].weapons = vec![WeaponState::new(Arc::new(world.catalog.get("weapon_knife").unwrap().clone()))];
            }
            let intact = world.clone();
            let mut predicted = world.clone();
            world.step(&[(id, attack())]); predicted.step_prediction(&[(id, attack())]);
            assert_eq!(world.state(), predicted.state());
            assert!(predicted.events.is_empty());
            assert!(world.breakables[0].broken());
            assert!(!blocked(&world), "the linked invisible brush must also leave the passage");
            assert!(blocked(&intact), "prediction clones must retain independent collision");
            let mut restored = intact.clone();
            restored.restore(world.decode::<crate::WorldState>(&world.encode_state()).unwrap());
            assert!(!blocked(&restored));
            assert!(world.collision.trace_ray(Vec3::new(64., 0., 64.), Vec3::new(128., 0., 64.), contents::MASK_SHOT).hit(),
                "removal must preserve the independent door leaf");
            for _ in 0..rate / 2 { world.step(&[(id, TickInput { forward_move: 450., ..Default::default() })]); }
            assert!(world.players[0].movement.origin.x > 18.);
            world.damage_environment(id, 1000, "fixture");
            assert!(world.breakables[0].broken());
            restored.restore(intact.state()); assert!(blocked(&restored));
            world.start_round(false); assert!(blocked(&world));
            assert_eq!(world.breakables, intact.breakables);
        }
    }
}

//! Source-only regressions: ALL UNRUN. Synthetic map data, no installed assets.
use super::*;
use collision::{CollisionWorld, MapCollisionOptions, contents};

fn fixture(rate: u32, metadata: &str, mask: i32, side_window: bool, wall: bool,
    melee: bool, automatic: bool) -> (World, Bots, PlayerId) {
    let mut map = bsp::Bsp::default();
    let doc = kv::parse(&format!(r#"vent {{ classname func_breakable model *1
        origin "0 0 0" {metadata} }}"#)).unwrap();
    map.entities.push(doc.block("vent").unwrap().clone());
    let (min_y, max_y) = if side_window { (48., 80.) } else { (-64., 64.) };
    let mins = Vec3::new(-2., min_y, 0.);
    let maxs = Vec3::new(2., max_y, 256.);
    for (normal, dist) in [(Vec3::X, maxs.x), (-Vec3::X, -mins.x),
        (Vec3::Y, maxs.y), (-Vec3::Y, -mins.y), (Vec3::Z, maxs.z), (-Vec3::Z, -mins.z)] {
        map.planes.push(bsp::Plane { normal, dist, kind: 0 });
    }
    map.brush_sides = (0..6).map(|plane| bsp::BrushSide {
        plane, texinfo: -1, dispinfo: -1, bevel: false,
    }).collect();
    map.brushes.push(bsp::Brush { first_side: 0, num_sides: 6, contents: contents::SOLID });
    for head_node in [-1, -2] {
        map.models.push(bsp::Model { mins, maxs, origin: Vec3::ZERO,
            head_node, first_face: 0, num_faces: 0 });
    }
    for num_leaf_brushes in [0, 1] {
        map.leafs.push(bsp::Leaf { contents: 0, cluster: -1, area: 0,
            mins: [-128; 3], maxs: [256; 3], first_leaf_face: 0, num_leaf_faces: 0,
            first_leaf_brush: 0, num_leaf_brushes });
    }
    map.leaf_brushes.push(0);
    let mut collision = CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &|_| None);
    assert_eq!(collision.breakables.len(), 1);
    // Exercise the mounted definition masks too, including a shot-only solid
    // that the BSP importer would normally discard as non-world collision.
    for solid in &mut Arc::get_mut(&mut collision.breakables).unwrap()[0].solids { solid.contents = mask; }
    for points in [[Vec3::new(-1024., -1024., 0.), Vec3::new(1024., -1024., 0.), Vec3::new(1024., 1024., 0.)],
        [Vec3::new(-1024., -1024., 0.), Vec3::new(1024., 1024., 0.), Vec3::new(-1024., 1024., 0.)]] {
        collision.add_triangle(points, contents::SOLID, 0);
    }
    if wall {
        // A solid wall before the vent wins the ordinary collision ray.
        collision.add_convex(collision::Convex { plane_surfaces: Vec::new(),
            planes: vec![collision::Plane { normal: Vec3::X, dist: -8. },
                collision::Plane { normal: -Vec3::X, dist: 10. },
                collision::Plane { normal: Vec3::Y, dist: 64. },
                collision::Plane { normal: -Vec3::Y, dist: 64. },
                collision::Plane { normal: Vec3::Z, dist: 256. },
                collision::Plane { normal: -Vec3::Z, dist: 0. }],
            mins: Vec3::new(-10., -64., 0.), maxs: Vec3::new(-8., 64., 256.),
            contents: contents::SOLID, surface: 0,
        });
    }
    collision.finalize();
    let catalog = weapons::Catalog::from_items_game(&format!(r##"items_game {{ items {{
        999 {{ name weapon_fixture item_class weapon_fixture baseitem 1 item_type_name "#CSGO_Type_Rifle"
            attributes {{ damage 20 "range modifier" 1 "primary clip size" 30
                "is full auto" {} cycletime 0.1 }} }}
        42 {{ name weapon_knife item_class weapon_knife baseitem 1 }}
    }} }}"##, u8::from(automatic))).unwrap();
    let mut world = World::with_catalog(Arc::new(collision), Arc::new(catalog));
    world.set_tick_rate(rate);
    world.tick = world.ticks(2.);
    let id = world.add_bot("route breaker", Team::CounterTerrorist);
    let weapons = ["weapon_fixture", "weapon_knife"].map(|name|
        crate::WeaponState::new(Arc::new(world.catalog.get(name).unwrap().clone())));
    let bot = world.player_mut(id).unwrap();
    bot.weapons = weapons.into_iter().collect();
    bot.active = usize::from(melee);
    if melee { bot.weapons[0].clip = 0; bot.weapons[0].reserve = 0; }
    bot.last_input.weapon_slot = if melee { 3 } else { 1 };
    bot.movement = movement::PlayerMove::spawn_at(Vec3::new(-60., 0., 1.));
    bot.movement.on_ground = true;
    world.mode = GameMode::Competitive;
    world.round.phase = Phase::Live;
    world.objectives.bomb_sites.push(crate::objectives::BombSite { name: 'A',
        zone: Zone { mins: Vec3::new(550., -40., 0.), maxs: Vec3::new(650., 40., 128.) } });
    world.bomb = Some(crate::PlantedBomb { pos: Vec3::new(600., 0., 1.), site: 0,
        planter: id, planted_at: 0., explode_at: 40., defuser: None });
    let area = |id, min_x, max_x| nav::Area {
        id, flags: 0, nw: Vec3::new(min_x, -40., 0.), se: Vec3::new(max_x, 40., 0.),
        ne_z: 0., sw_z: 0., connections: Default::default(), place: 0, hiding_spots: vec![],
    };
    let mut start = area(1, -128., 0.);
    start.connections[1].push(2);
    let nav = NavMesh::from_areas(vec![start, area(2, 0., 768.)]);
    (world, Bots::with_skill(1.).with_nav(Some(Arc::new(nav))), id)
}

fn route_blocked(world: &World) -> bool {
    world.collision.trace_box(Vec3::new(-60., 0., 1.), Vec3::new(80., 0., 1.),
        movement::STAND_MINS, movement::STAND_MAXS, contents::MASK_PLAYERSOLID).hit()
}

#[test]
fn think_primary_trigger_clears_and_traverses_the_actual_route() {
    for rate in [64, 128] {
        for (melee, automatic) in [(false, true), (false, false), (true, false)] {
            let (mut world, mut bots, id) = fixture(rate, "health 60", contents::SOLID, false, false, melee, automatic);
            assert!(route_blocked(&world));
            let mut attacks = 0;
            for _ in 0..rate * 3 {
                let health = world.breakables[0].health;
                let blocked = route_blocked(&world);
                let inputs = bots.think(&world);
                assert_eq!(world.breakables[0].health, health, "think must not damage the vent");
                assert_eq!(route_blocked(&world), blocked, "think must not edit collision");
                let input = inputs.iter().find(|(pid, _)| *pid == id).unwrap().1;
                assert_eq!(input.buttons & buttons::ATTACK2, 0);
                if input.buttons & buttons::ATTACK != 0 {
                    attacks += 1;
                    assert!(!world.breakables[0].broken(), "no attacks after clearance");
                    assert!(world.player(id).unwrap().movement.origin.x > -32., "no distant shots/slashes");
                }
                world.step(&inputs);
                if world.player(id).unwrap().movement.origin.x > 80. { break; }
            }
            assert!(attacks > 0);
            assert!(world.breakables[0].broken(), "ordinary damage clears at {rate} Hz, melee={melee}");
            assert!(!route_blocked(&world));
            assert!(world.player(id).unwrap().movement.origin.x > 80., "ordinary movement crosses the cleared vent: rate={rate} melee={melee} auto={automatic} movement={:?} path={:?} index={} failed={:?}", world.player(id).unwrap().movement, bots.brains[&id].path, bots.brains[&id].path_index, bots.brains[&id].failed_transitions);
            assert!(bots.brains[&id].failed_transitions.is_empty(), "do not replace successful traversal with a replan");
        }
    }
}

#[test]
fn non_destructible_walls_and_mask_controls_do_not_spam_attacks() {
    for rate in [64, 128] {
        for melee in [false, true] {
            for (metadata, mask, side, wall, blocked) in [
                ("health 0", contents::SOLID, false, false, true),
                ("health 60 material 7", contents::SOLID, false, false, true),
                ("health 60 spawnflags 1", contents::SOLID, false, false, true),
                ("health 60 \"dmg.bullets\" 0 \"dmg.club\" 0", contents::SOLID, false, false, true),
                ("health 60 minhealthdmg 100", contents::SOLID, false, false, true),
                ("health 60", contents::GRATE, false, false, true), // excluded from MASK_SHOT
                ("health 60", contents::HITBOX, false, false, false), // no player hull collision
                ("health 60", contents::SOLID, true, false, false), // beside the route
                ("health 60", contents::SOLID, false, true, true), // behind an earlier world hit
            ] {
                let (mut world, mut bots, id) = fixture(rate, metadata, mask, side, wall, melee, true);
                assert_eq!(route_blocked(&world), blocked);
                let health = world.breakables[0].health;
                let mut recovered = false;
                for _ in 0..rate * 3 {
                    let inputs = bots.think(&world);
                    assert_eq!(inputs[0].1.buttons & (buttons::ATTACK | buttons::ATTACK2), 0,
                        "no spam: {metadata}, mask={mask}, side={side}, wall={wall}, melee={melee}");
                    recovered |= !bots.brains[&id].failed_transitions.is_empty();
                    world.step(&inputs);
                }
                assert_eq!(world.breakables[0].health, health);
                assert!(!world.breakables[0].broken());
                assert_eq!(route_blocked(&world), blocked);
                if blocked { assert!(recovered, "ordinary bounded recovery still owns an uncleared passage"); }
                else { assert!(world.player(id).unwrap().movement.origin.x > 80.); }
            }
        }
    }
}

#[test]
fn route_attack_respects_final_aim_players_readiness_and_shooting_disabled() {
    for rate in [64, 128] {
        for melee in [false, true] {
            let (mut world, mut bots, id) = fixture(rate, "health 60", contents::SOLID, false, false, melee, true);
            let bot = world.player_mut(id).unwrap();
            bot.movement.origin.x = -19.;
            bot.view_angles.y = 90.;
            let inputs = bots.think(&world);
            assert_eq!(inputs[0].1.buttons & buttons::ATTACK, 0, "smoothed aim must actually hit the vent");
            world.step(&inputs);
            assert_eq!(world.breakables[0].health, 60.);

            let (mut world, mut bots, _) = fixture(rate, "health 60", contents::SOLID, false, false, melee, true);
            bots.shooting = false;
            for _ in 0..rate * 3 {
                let inputs = bots.think(&world);
                assert_eq!(inputs[0].1.buttons & (buttons::ATTACK | buttons::ATTACK2), 0);
                world.step(&inputs);
            }
            assert_eq!(world.breakables[0].health, 60.);

            for team in [Team::CounterTerrorist, Team::Terrorist] {
                let (mut world, _, id) = fixture(rate, "health 60", contents::SOLID, false, false, melee, true);
                world.player_mut(id).unwrap().movement.origin.x = -19.;
                let other = world.add_player("intervening player", team);
                world.respawn(other);
                world.player_mut(other).unwrap().movement.origin = Vec3::new(-16., 0., 1.);
                let bot = world.player(id).unwrap();
                let input = TickInput { weapon_slot: bot.last_input.weapon_slot,
                    forward_move: 450., ..Default::default() };
                assert!(!clear_route_attack(&world, bot, &input, 0), "any player ahead vetoes obstruction fire");
            }

            for pending in 0..(if melee { 7 } else { 8 }) {
                let (mut world, mut bots, id) = fixture(rate, "health 60", contents::SOLID, false, false, melee, true);
                let time = world.time();
                let bot = world.player_mut(id).unwrap();
                bot.movement.origin.x = -19.;
                let w = bot.active_weapon_mut().unwrap();
                match pending {
                    0 => w.reload_end = Some(time + 1.),
                    1 => w.next_attack = time + 1.,
                    2 => w.next_secondary_attack = time + 1.,
                    3 => w.burst = Some(crate::weapon::BurstState { remaining: 2, next_shot: time + 1. }),
                    4 => w.suppressor_transition = Some(crate::weapon::SuppressorTransition {
                        started_tick: 0, attached: false,
                    }),
                    5 => w.grenade_throw = Some(crate::utility::ThrowState {
                        phase: crate::utility::ThrowPhase::Holding, mode: crate::utility::ThrowMode::Overhand,
                        pull_started_tick: 0, throw_started_tick: None, release_requested: false, spawned: false,
                    }),
                    6 => w.burst_mode = true,
                    _ => {
                        let mut def = (*w.def).clone();
                        def.bullets = 8;
                        w.set_def(Arc::new(def));
                    }
                }
                let inputs = bots.think(&world);
                assert_eq!(inputs[0].1.buttons & (buttons::ATTACK | buttons::ATTACK2), 0,
                    "do not borrow a pending weapon action");
                assert_eq!(world.breakables[0].health, 60.);
            }
        }
    }
}

#[test]
fn an_eye_visible_vent_does_not_override_a_solid_hull_obstruction_or_a_spread_miss() {
    for rate in [64, 128] {
        let (mut world, _, id) = fixture(rate, "health 60", contents::SOLID, false, false, false, true);
        world.player_mut(id).unwrap().movement.origin.x = -19.;
        world.collision = CollisionWorld::map_arc(&world.collision);
        let base = Arc::get_mut(&mut world.collision).unwrap();
        base.add_convex(collision::Convex { plane_surfaces: Vec::new(),
            // The eye can see the vent above this overlapping low solid;
            // the player hull cannot perform the attempted horizontal move.
            planes: vec![collision::Plane { normal: Vec3::X, dist: 1. },
                collision::Plane { normal: -Vec3::X, dist: 1.5 },
                collision::Plane { normal: Vec3::Y, dist: 64. },
                collision::Plane { normal: -Vec3::Y, dist: 64. },
                collision::Plane { normal: Vec3::Z, dist: 32. },
                collision::Plane { normal: -Vec3::Z, dist: 0. }],
            mins: Vec3::new(-1.5, -64., 0.), maxs: Vec3::new(1., 64., 32.),
            contents: contents::SOLID, surface: 0,
        });
        base.finalize();
        world.rebuild_door_collision();
        let bot = world.player(id).unwrap();
        assert_eq!(world.collision.trace_owned_ray(bot.eye(), bot.eye() + Vec3::X * 100., contents::MASK_SHOT).1,
            Some(collision::HitOwner::Breakable(0)));
        assert!(blocking_route_breakable(&world, bot, Vec3::X * 450.).is_none());

        let (mut world, _, id) = fixture(rate, "health 60", contents::SOLID, false, false, false, true);
        let bot = world.player_mut(id).unwrap();
        bot.movement.origin.x = -19.;
        let w = bot.active_weapon_mut().unwrap();
        let mut def = (*w.def).clone();
        def.accuracy.spread = 1_000_000.; // A centre ray alone would incorrectly admit this shot.
        w.set_def(Arc::new(def));
        let bot = world.player(id).unwrap();
        assert!(blocking_route_breakable(&world, bot, Vec3::X * 450.).is_some());
        assert!(!clear_route_attack(&world, bot, &TickInput {
            weapon_slot: 1, forward_move: 450., ..Default::default()
        }, 0));
    }
}

#[test]
fn an_uncleared_destructible_route_still_expires_without_renewing_progress() {
    for rate in [64, 128] {
        let (mut world, mut bots, id) = fixture(rate, "health 1000000", contents::SOLID, false, false, false, true);
        let mut attacked = false;
        let mut expired = false;
        for _ in 0..rate * 3 {
            let inputs = bots.think(&world);
            attacked |= inputs[0].1.buttons & buttons::ATTACK != 0;
            if !bots.brains[&id].failed_transitions.is_empty() {
                expired = true;
                assert_eq!(inputs[0].1.buttons & buttons::ATTACK, 0, "an expired crossing cannot keep firing");
            }
            world.step(&inputs);
        }
        assert!(attacked && expired);
        assert!(!world.breakables[0].broken());
        assert!(route_blocked(&world));
    }
}

#[test]
fn remembered_combat_and_active_bomb_work_keep_priority_over_the_vent() {
    for rate in [64, 128] {
        let (mut world, mut bots, id) = fixture(rate, "health 60", contents::SOLID, false, false, false, true);
        world.player_mut(id).unwrap().movement.origin.x = -19.;
        let enemy = world.add_player("remembered enemy behind vent", Team::Terrorist);
        world.respawn(enemy);
        world.player_mut(enemy).unwrap().movement.origin = Vec3::new(200., 0., 1.);
        let mut brain = Brain::fresh(world.player(id).unwrap(), world.round.round);
        brain.target = Some(enemy);
        brain.target_pos = world.player(enemy).unwrap().eye();
        brain.last_seen = world.time();
        bots.brains.insert(id, brain);
        let inputs = bots.think(&world);
        assert_eq!(inputs[0].1.buttons & buttons::ATTACK, 0, "do not replace remembered combat with vent shooting");
        world.step(&inputs);
        assert_eq!(world.breakables[0].health, 60.);

        let (mut world, mut bots, id) = fixture(rate, "health 60", contents::SOLID, false, false, false, true);
        world.player_mut(id).unwrap().movement.origin.x = -19.;
        world.bomb.as_mut().unwrap().pos = Vec3::new(-40., 0., 1.);
        world.bomb.as_mut().unwrap().defuser = Some((id, 10.));
        let inputs = bots.think(&world);
        assert_ne!(inputs[0].1.buttons & buttons::USE, 0);
        assert_eq!(inputs[0].1.buttons & buttons::ATTACK, 0);
        world.step(&inputs);
        assert_eq!(world.bomb.unwrap().defuser, Some((id, 10.)));
        assert_eq!(world.breakables[0].health, 60.);

        let (mut world, mut bots, id) = fixture(rate, "health 60", contents::SOLID, false, false, false, true);
        world.catalog = Arc::new(weapons::Catalog::from_items_game(r#"items_game { items {
            49 { name weapon_c4 item_class weapon_c4 baseitem 1 }
        } }"#).unwrap());
        world.bomb = None;
        world.objectives.bomb_sites[0].zone = Zone {
            mins: Vec3::new(-128., -40., 0.), maxs: Vec3::new(-20., 40., 128.),
        };
        let started = world.time();
        let c4 = crate::WeaponState::new(Arc::new(world.catalog.get("weapon_c4").unwrap().clone()));
        let bot = world.player_mut(id).unwrap();
        bot.movement.origin.x = -60.;
        bot.team = Team::Terrorist;
        bot.weapons = vec![c4];
        bot.active = 0;
        bot.plant_start = Some(started);
        let inputs = bots.think(&world);
        assert_eq!(inputs[0].1.weapon_slot, 5);
        assert_ne!(inputs[0].1.buttons & buttons::ATTACK, 0, "plant retains its primary trigger");
        world.step(&inputs);
        assert_eq!(world.player(id).unwrap().plant_start, Some(started));
        assert_eq!(world.breakables[0].health, 60.);
    }
}

use super::*;
use glam::Vec3;

// Exact installed-world fixtures; numeric starting points identify the authored
// routes, but all geometry comes from the player's read-only Legacy install.
// No copied BSP/PHY data, game processes, settings changes or content fetches.
#[test]
#[ignore = "requires read-only CS:GO Legacy installed data; no native process"]
fn installed_nuke_vent_and_mirage_ladder_room_climb_through_tick_input() {
    let game = vfs::find_csgo_dir().expect("CS:GO Legacy install / CSR_GAME_DIR required");
    for (name, start, yaw, normal, goal_z) in [
        ("de_nuke", Vec3::new(216.96875, -848., -415.96875), 0., -Vec3::X, -200.),
        ("de_mirage", Vec3::new(-996., -199.96875, -167.96875), -90., Vec3::Y, -64.),
    ] {
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&game).unwrap();
        let map = bsp::Bsp::parse(&fs.read(&format!("maps/{name}.bsp")).expect("installed BSP")).unwrap();
        fs.mount_map_pakfile(map.pakfile.clone()).unwrap();
        let loader = |model: &str| {
            let bytes = fs.read(&format!("{}.phy", model.trim_end_matches(".mdl")))?;
            mdl::Phy::parse(&bytes).ok().map(|phy| phy.hulls)
        };
        let collision = Arc::new(CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &loader));
        for rate in [64, 128] {
            let mut world = sim::World::new(collision.clone());
            world.set_tick_rate(rate);
            let actor = world.add_player(name, sim::Team::CounterTerrorist);
            world.player_mut(actor).unwrap().movement = movement::PlayerMove::spawn_at(start);
            for _ in 0..rate * 3 {
                world.step(&[(actor, sim::TickInput {
                    view_angles: Vec3::new(0., yaw, 0.),
                    forward_move: 450.,
                    weapon_slot: 3,
                    ..Default::default()
                })]);
                let player = world.player(actor).unwrap();
                assert_eq!(player.movement.ladder_normal, Some(normal), "{name} {rate}Hz: {:?}", player.movement);
                assert!(player.alive && player.last_events.landed.is_none());
                let tr = collision.trace_box(player.movement.origin, player.movement.origin,
                    movement::STAND_MINS, movement::STAND_MAXS, collision::contents::MASK_PLAYERSOLID);
                assert!(!tr.start_solid && !tr.all_solid, "{name} {rate}Hz: {tr:?}");
                if player.movement.origin.z >= goal_z { break; }
            }
            assert!(world.player(actor).unwrap().movement.origin.z >= goal_z,
                "{name} {rate}Hz: failed to cross authored clip seam/side: {:?}", world.player(actor).unwrap().movement);
        }
    }
}

#[test]
#[ignore = "requires read-only CS:GO Legacy installed data; no native process"]
fn installed_mirage_ct_window_entrance_is_traversable_in_both_directions() {
    let game = vfs::find_csgo_dir().expect("CS:GO Legacy install / CSR_GAME_DIR required");
    let mut fs = vfs::Vfs::new();
    fs.mount_game_dir(&game).unwrap();
    let map = bsp::Bsp::parse(&fs.read("maps/de_mirage.bsp").expect("installed Mirage BSP")).unwrap();
    fs.mount_map_pakfile(map.pakfile.clone()).unwrap();
    let loader = |model: &str| {
        let bytes = fs.read(&format!("{}.phy", model.trim_end_matches(".mdl")))?;
        mdl::Phy::parse(&bytes).ok().map(|phy| phy.hulls)
    };
    let collision = Arc::new(CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &loader));
    // The STANDING CT-side south doorway of Sniper's Nest, between the authored
    // wooden jambs. Keep the same player for both legs, without noclip or
    // changing the standing/crouching hull to make the passage fit. This is
    // distinct from the owner's small west-wall crouch/jump-hole regression.
    for rate in [64, 128] {
        let mut world = sim::World::new(collision.clone());
        world.set_tick_rate(rate);
        let actor = world.add_player("window passage", sim::Team::CounterTerrorist);
        world.player_mut(actor).unwrap().movement = movement::PlayerMove::spawn_at(Vec3::new(-1200., -1260., -167.96875));
        for (yaw, goal, direction) in [(90., -1140., 1.), (-90., -1260., -1.)] {
            for _ in 0..rate * 3 {
                world.step(&[(actor, sim::TickInput {
                    view_angles: Vec3::new(0., yaw, 0.),
                    forward_move: 130.,
                    buttons: movement::buttons::WALK,
                    weapon_slot: 3,
                    ..Default::default()
                })]);
                let player = world.player(actor).unwrap();
                assert!(player.alive && !player.movement.ducked && !player.last_events.jumped);
                let tr = collision.trace_box(player.movement.origin, player.movement.origin,
                    movement::STAND_MINS, movement::STAND_MAXS, collision::contents::MASK_PLAYERSOLID);
                assert!(!tr.start_solid && !tr.all_solid, "{rate}Hz: {tr:?}");
                if (player.movement.origin.y - goal) * direction >= 0. { break; }
            }
            let player = world.player(actor).unwrap();
            assert!((player.movement.origin.y - goal) * direction >= 0. && player.movement.on_ground,
                "{rate}Hz: window entrance blocked toward {goal}: {:?}", player.movement);
            assert!((player.movement.origin.z + 168.).abs() < 1., "{rate}Hz: wrong floor: {:?}", player.movement);
        }
    }
}

#[test]
#[ignore = "requires read-only CS:GO Legacy installed data; no native process"]
fn installed_mirage_small_ct_window_hole_break_and_duck_jump_through_tick_input() {
    mirage_small_ct_window_hole(None);
}

#[test]
#[ignore = "requires CSR_TRAVERSAL_MIRAGE_BSP pointing to a private classic BSP and Legacy dependencies"]
fn cached_classic_mirage_small_ct_window_hole_break_and_duck_jump_through_tick_input() {
    let path = std::env::var_os("CSR_TRAVERSAL_MIRAGE_BSP").expect("private classic Mirage BSP path");
    mirage_small_ct_window_hole(Some(std::fs::read(path).expect("read-only private classic BSP")));
}

fn mirage_small_ct_window_hole(bytes: Option<Vec<u8>>) {
    let game = vfs::find_csgo_dir().expect("CS:GO Legacy install / CSR_GAME_DIR required");
    let mut fs = vfs::Vfs::new();
    fs.mount_game_dir(&game).unwrap();
    let bytes = bytes.unwrap_or_else(|| fs.read("maps/de_mirage.bsp").expect("installed Mirage BSP"));
    let map = bsp::Bsp::parse(&bytes).unwrap();
    fs.mount_map_pakfile(map.pakfile.clone()).unwrap();
    let loader = |model: &str| {
        let bytes = fs.read(&format!("{}.phy", model.trim_end_matches(".mdl")))?;
        mdl::Phy::parse(&bytes).ok().map(|phy| phy.hulls)
    };
    let propdata = fs.read_string("scripts/propdata.txt").and_then(|text| kv::parse(&text).ok()).unwrap_or_default();
    let presets = propdata.block("PropData.txt").unwrap_or(&propdata);
    let collision = Arc::new(CollisionWorld::from_bsp_with_model_data(&map,
        &MapCollisionOptions::default(), &loader, &|model| fs.read(model), presets));
    let owner = |name: &str| collision.breakables.iter().position(|def|
        map.entities[def.entity].str("targetname") == Some(name)).expect("authored hole owner");
    let cover = owner("prop.breakable.01");
    let grate = owner("prop.breakable.01.brush");
    assert!(collision.breakable_break_links.contains(&(cover, grate)));
    assert!(collision.breakable_break_links.contains(&(grate, cover)));
    assert!(collision.breakables[cover].studio.is_some());
    assert!(collision.breakables[grate].solids.iter().all(|solid|
        solid.contents & collision::contents::MASK_SHOT == 0
            && solid.contents & collision::contents::MASK_PLAYERSOLID != 0));

    // Owner confirmed this SMALL west-wall crouch/jump hole, not the standing
    // south doorway at X -1200. NAV area 219 is CROUCH and connects CTSpawn to
    // SnipersNest area 145. Keep full brush/PHY collision and normal hulls.
    let start = Vec3::new(-1364., -974., -167.96875);
    let end = Vec3::new(-1284., -974., -167.96875);
    for rate in [64, 128] {
        for approach in ["crouch", "jump then duck", "duck then jump"] {
            let mut world = sim::World::new(collision.clone());
            world.set_tick_rate(rate);
            let actor = world.add_player("small window hole", sim::Team::CounterTerrorist);
            world.player_mut(actor).unwrap().movement = movement::PlayerMove::spawn_at(start);
            // Select/deploy through ordinary input before the one knife attack.
            let aim = Vec3::new(36., 0., 0.);
            for _ in 0..rate {
                world.step(&[(actor, sim::TickInput { view_angles: aim, weapon_slot: 3, ..Default::default() })]);
            }
            let trace = world.collision.trace_box(start, end, movement::DUCK_MINS,
                movement::DUCK_MAXS, collision::contents::MASK_PLAYERSOLID);
            assert!(trace.hit(), "intact grate must block even a crouched hull");
            let eye = world.player(actor).unwrap().movement.eye_position();
            let (direction, _, _) = movement::angle_vectors(aim);
            assert_eq!(world.collision.trace_breakable_ray(eye, eye + direction * sim::MELEE_RANGE,
                collision::contents::MASK_SHOT).1, Some(cover), "knife must hit the visible cover");
            world.step(&[(actor, sim::TickInput { view_angles: aim, weapon_slot: 3,
                buttons: movement::buttons::ATTACK, ..Default::default() })]);
            assert!(world.breakables[cover].broken() && world.breakables[grate].broken(),
                "{rate}Hz {approach}: reciprocal Break must remove both owners");
            assert!(!world.collision.trace_box(start, end, movement::DUCK_MINS,
                movement::DUCK_MAXS, collision::contents::MASK_PLAYERSOLID).hit(),
                "the authored crouch hull must fit the cleared hole");
            assert!(world.collision.trace_box(start, end, movement::STAND_MINS,
                movement::STAND_MAXS, collision::contents::MASK_PLAYERSOLID).hit(),
                "header/frame must still stop the standing hull");
            if approach == "duck then jump" {
                for _ in 0..rate / 4 {
                    world.step(&[(actor, sim::TickInput { buttons: movement::buttons::DUCK, ..Default::default() })]);
                }
            }
            let mut jumps = 0;
            for tick in 0..rate * 3 {
                let buttons = match (approach, tick) {
                    ("jump then duck", 0) => movement::buttons::JUMP,
                    ("duck then jump", 0) => movement::buttons::DUCK | movement::buttons::JUMP,
                    _ => movement::buttons::DUCK,
                };
                world.step(&[(actor, sim::TickInput { forward_move: 450., buttons,
                    weapon_slot: 3, ..Default::default() })]);
                let player = world.player(actor).unwrap();
                jumps += usize::from(player.last_events.jumped);
                let (mins, maxs) = player.movement.hull();
                let trace = world.collision.trace_box(player.movement.origin, player.movement.origin,
                    mins, maxs, collision::contents::MASK_PLAYERSOLID);
                assert!(!trace.start_solid && !trace.all_solid && player.alive,
                    "{rate}Hz {approach}: {:?} {trace:?}", player.movement);
                if player.movement.origin.x >= end.x && player.movement.on_ground { break; }
            }
            let player = world.player(actor).unwrap();
            assert!(player.movement.origin.x >= end.x && player.movement.on_ground,
                "{rate}Hz {approach}: small hole blocked: {:?}", player.movement);
            assert_eq!(jumps, usize::from(approach != "crouch"));
            // Return through the same small hole, without relocation/reset.
            for _ in 0..rate * 3 {
                world.step(&[(actor, sim::TickInput { view_angles: Vec3::new(0., 180., 0.),
                    forward_move: 450., buttons: movement::buttons::DUCK, weapon_slot: 3, ..Default::default() })]);
                if world.player(actor).unwrap().movement.origin.x <= start.x { break; }
            }
            let player = world.player(actor).unwrap();
            assert!(player.movement.origin.x <= start.x && player.movement.on_ground,
                "{rate}Hz {approach}: return blocked: {:?}", player.movement);
            assert!((player.movement.origin.z + 168.).abs() < 1.);
        }
    }
}

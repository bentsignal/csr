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
    // The CT-side south entrance of Sniper's Nest, between the authored
    // wooden jambs. Keep the same player for both legs, without noclip or
    // changing the standing/crouching hull to make the passage fit.
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

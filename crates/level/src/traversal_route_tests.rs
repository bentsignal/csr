use super::*;
use glam::Vec3;

fn private_classic_level(name: &str) -> Level {
    let (game, fs) = mount_game(None).expect("read-only Legacy dependencies");
    let id = workshop::find(name).expect("qualified classic map").content_id();
    Level::load_local_id_from_vfs(&id, game, fs, &workshop::Store::current())
        .expect("already-cached private classic BSP; no download")
}

// Source-reviewed, UNRUN private-data fixtures. CSR_DATA_DIR selects the
// already-cached October 2015 BSP; private_classic_level never downloads it.
mod cached_classic_overpass_2015 {
    use super::*;

    fn world(level: &Level, rate: u32) -> sim::World {
        let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
        world.set_character_profiles(level.character_profiles.clone());
        world.set_tick_rate(rate);
        world.load_map(&level.bsp, level.overview().as_deref());
        world
    }

    fn assert_clear(world: &sim::World, actor: sim::PlayerId, stage: &str) {
        let player = world.player(actor).unwrap();
        let (mins, maxs) = player.movement.hull();
        let at = world.collision.trace_box(player.movement.origin, player.movement.origin,
            mins, maxs, collision::contents::MASK_PLAYERSOLID);
        assert!(player.alive && player.health > 0 && player.deaths == 0
            && !player.movement.ducked && !at.start_solid && !at.all_solid,
            "{}Hz {stage}, actor {actor}, tick {}: alive={} health={} deaths={}, {:?}, {at:?}",
            world.tick_rate, world.tick, player.alive, player.health, player.deaths, player.movement);
    }

    #[test]
    #[ignore = "requires private cached de_overpass_2015 via CSR_DATA_DIR and read-only Legacy dependencies"]
    fn ten_player_competitive_starts_use_shared_spawn_path() {
        let level = private_classic_level("de_overpass_2015");
        let cvars = level.cfg("gamemode_competitive.cfg")
            .map(|text| sim::rules::parse_cfg(&text)).unwrap_or_default();
        let rules = sim::RulesConfig::from_cvars(&cvars);
        for rate in [64, 128] {
            let mut world = world(&level, rate);
            // All 36 enabled authored spawns must reach the shared pool,
            // rather than a test-specific substitute or a fallback origin.
            assert_eq!(world.spawns.len(), 36);
            assert_eq!(world.objectives.buy_zones.len(), 2);
            for (team, count) in [(sim::Team::Terrorist, 20), (sim::Team::CounterTerrorist, 16)] {
                assert_eq!(world.spawns.iter().filter(|spawn| spawn.team == team).count(), count);
            }
            // Same add_bot -> set_mode order as launcher's round_check:
            // first deathmatch placements, then the first competitive round.
            for i in 0..5 {
                world.add_bot(&format!("T bot {i}"), sim::Team::Terrorist);
                world.add_bot(&format!("CT bot {i}"), sim::Team::CounterTerrorist);
            }
            for stage in ["initial deathmatch placement", "first competitive placement"] {
                if stage == "first competitive placement" {
                    world.set_mode(sim::GameMode::Competitive, rules.clone());
                }
                assert_eq!(world.players.len(), 10);
                for (i, player) in world.players.iter().enumerate() {
                    assert_clear(&world, player.id, stage);
                    assert!(world.spawns.iter().any(|spawn| spawn.team == player.team
                        && spawn.origin.truncate().distance(player.movement.origin.truncate()) < 0.01),
                        "{rate}Hz {stage}: {} left the authored team spawn pool: {:?}", player.name, player.movement);
                    let (mins, maxs) = player.bounds();
                    assert!(world.objectives.in_buy_zone(player.team, mins, maxs),
                        "{rate}Hz {stage}: {} outside own buy zone: {:?}", player.name, player.movement);
                    for other in &world.players[..i] {
                        let delta = (player.movement.origin - other.movement.origin).abs();
                        assert!(delta.x >= 32. || delta.y >= 32. || delta.z >= 72.,
                            "{rate}Hz {stage}: standing players {} and {} overlap", player.id, other.id);
                    }
                }
            }
            let idle: Vec<_> = world.players.iter().map(|player| (player.id,
                sim::TickInput { weapon_slot: 3, ..Default::default() })).collect();
            // Explicit commands override AI without replacing the bot/player
            // placement path. Exercise authoritative buy flags and settling.
            for _ in 0..rate {
                world.step(&idle);
                for player in &world.players {
                    assert_clear(&world, player.id, "competitive idle");
                    assert!(player.in_buy_zone, "{rate}Hz {} lost buy zone at tick {}: {:?}",
                        player.name, world.tick, player.movement);
                }
            }
            assert!(world.players.iter().all(|player| player.movement.on_ground));
        }
    }

    #[test]
    #[ignore = "requires private cached de_overpass_2015 via CSR_DATA_DIR and read-only Legacy dependencies"]
    fn tunnel_ladder_bottom_approach_climb_and_south_dismount() {
        let level = private_classic_level("de_overpass_2015");
        // Brush 2450: X -1696..-1694, Y -1995..-1973, Z 96..384.
        // Bottom NAV 11206; south upper platform/NAV 9775 is at Z336.
        // Its shaft opening is between the authored Z336 grate strips.
        // Y-1980 also keeps the standing hull north of the two tilted
        // pallet PHY bounds (the nearer one ends at Y-1999.2594).
        let start = Vec3::new(-1656., -1980., 96.03125);
        for rate in [64, 128] {
            let mut world = world(&level, rate);
            let actor = world.add_player("tunnel ladder", sim::Team::CounterTerrorist);
            world.player_mut(actor).unwrap().movement = movement::PlayerMove::spawn_at(start);
            let epoch = world.player(actor).unwrap().life_epoch;
            assert_clear(&world, actor, "ladder bottom start");
            assert_eq!(world.collision.ladder_normal(start, movement::STAND_MINS, movement::STAND_MAXS), None,
                "approach must start off the ladder");
            let mut attached = false;
            for _ in 0..rate * 3 {
                world.step(&[(actor, sim::TickInput { view_angles: Vec3::new(0., 180., 0.),
                    forward_move: 450., weapon_slot: 3, ..Default::default() })]);
                assert_clear(&world, actor, "ladder approach/climb");
                let movement = world.player(actor).unwrap().movement;
                attached |= movement.ladder_normal == Some(Vec3::X);
                if movement.origin.z >= 338. { break; }
            }
            let movement = world.player(actor).unwrap().movement;
            assert!(attached && movement.origin.x < start.x - 16. && movement.origin.z >= 338.,
                "{rate}Hz ordinary bottom approach/climb failed: {movement:?}");
            // Move south along the face, then off its side onto the platform.
            // No jump, crouch, teleport, hull adjustment or collision removal.
            for _ in 0..rate * 2 {
                world.step(&[(actor, sim::TickInput { view_angles: Vec3::new(0., -90., 0.),
                    forward_move: 450., weapon_slot: 3, ..Default::default() })]);
                assert_clear(&world, actor, "ladder south dismount");
                let movement = world.player(actor).unwrap().movement;
                if movement.origin.y <= -2032. && movement.on_ground { break; }
            }
            let player = world.player(actor).unwrap();
            assert!(player.movement.origin.y <= -2032. && player.movement.origin.y >= -2050.
                && (-1700. ..=-1625.).contains(&player.movement.origin.x)
                && (player.movement.origin.z - 336.).abs() < 1.
                && player.movement.on_ground && player.movement.ladder_normal.is_none(),
                "{rate}Hz south upper NAV/platform not reached: {:?}", player.movement);
            assert_eq!(player.life_epoch, epoch, "route must finish in the same life");
            for _ in 0..rate / 2 {
                world.step(&[(actor, sim::TickInput { weapon_slot: 3, ..Default::default() })]);
                assert_clear(&world, actor, "upper platform idle");
            }
            let player = world.player(actor).unwrap();
            assert!(player.movement.on_ground && (player.movement.origin.z - 336.).abs() < 1.
                && player.movement.ladder_normal.is_none(), "{rate}Hz dismount did not hold: {:?}", player.movement);
            assert_eq!(player.life_epoch, epoch);
        }
    }

    fn use_door(world: &mut sim::World, actor: sim::PlayerId, door: usize, angle: f32) {
        // Release USE and movement explicitly; missing inputs repeat movement.
        world.step(&[(actor, sim::TickInput::default())]);
        assert_clear(world, actor, "door use release");
        let center = world.collision.doors[door].center_at(world.doors[door].angle);
        let delta = center - world.player(actor).unwrap().eye();
        let yaw = delta.y.atan2(delta.x).to_degrees();
        let pitch = (-delta.z).atan2(delta.truncate().length()).to_degrees();
        world.step(&[(actor, sim::TickInput { buttons: movement::buttons::USE,
            view_angles: Vec3::new(pitch, yaw, 0.), ..Default::default() })]);
        assert_clear(world, actor, "door use press");
        for _ in 0..world.tick_rate * 2 {
            world.step(&[(actor, sim::TickInput::default())]);
            assert_clear(world, actor, "door swing");
            if (world.doors[door].angle - angle).abs() < 0.01 { break; }
        }
        assert!((world.doors[door].angle - angle).abs() < 0.01
            && (world.doors[door].target - angle).abs() < 0.01,
            "{}Hz door use failed, expected {angle}: {:?}, actor {:?}",
            world.tick_rate, world.doors[door], world.player(actor).unwrap().movement);
    }

    #[test]
    #[ignore = "requires private cached de_overpass_2015 via CSR_DATA_DIR and read-only Legacy dependencies"]
    fn connector_door_opens_from_both_sides_and_allows_return_passage() {
        let level = private_classic_level("de_overpass_2015");
        let entity = level.bsp.entities.iter().position(|entity|
            entity.str("hammerid") == Some("4363685")).expect("authored Connector door");
        let door = level.collision.doors.iter().position(|door| door.entity == entity)
            .expect("Connector door must load through shared mounted PHY seam");
        let def = &level.collision.doors[door];
        assert_eq!(def.model, "models/props_downtown/metal_door_112.mdl");
        assert_eq!(def.origin, Vec3::new(-1748., -889., 96.));
        assert_eq!(def.angles, Vec3::new(0., 270., 0.));
        assert_eq!(level.bsp.entities[entity].str("skin"), Some("1"));
        // Frame opening X -1747..-1693, threshold Z96. Keep the full
        // standing hull centered at X-1720, well clear of either jamb.
        let south = Vec3::new(-1720., -949., 96.03125);
        let north = Vec3::new(-1720., -829., 96.03125);
        for rate in [64, 128] {
            let mut world = world(&level, rate);
            let actor = world.add_player("Connector passage", sim::Team::CounterTerrorist);
            world.player_mut(actor).unwrap().movement = movement::PlayerMove::spawn_at(south);
            let epoch = world.player(actor).unwrap().life_epoch;
            assert_clear(&world, actor, "Connector south start");
            let passage = |world: &sim::World| world.collision.trace_box(south, north,
                movement::STAND_MINS, movement::STAND_MAXS, collision::contents::MASK_PLAYERSOLID);
            let closed = passage(&world);
            assert!(closed.hit() && !closed.start_solid && !closed.all_solid
                && world.collision.surface_name(closed.surface) == def.model.as_str(),
                "{rate}Hz closed authored leaf must block passage: {closed:?}");
            for (goal, yaw, angle) in [(north, 90., 90.), (south, -90., -90.)] {
                use_door(&mut world, actor, door, angle);
                let open = passage(&world);
                assert!(!open.hit() && !open.start_solid && !open.all_solid,
                    "{rate}Hz open leaf must clear the standing passage: {open:?}");
                for _ in 0..rate * 2 {
                    world.step(&[(actor, sim::TickInput { view_angles: Vec3::new(0., yaw, 0.),
                        forward_move: 450., weapon_slot: 3, ..Default::default() })]);
                    assert_clear(&world, actor, "Connector passage");
                    let movement = world.player(actor).unwrap().movement;
                    if (movement.origin.y - goal.y) * yaw.signum() >= 0. && movement.on_ground { break; }
                }
                let player = world.player(actor).unwrap();
                assert!((player.movement.origin.y - goal.y) * yaw.signum() >= 0.
                    && (player.movement.origin.x - goal.x).abs() < 1.
                    && (player.movement.origin.z - 96.).abs() < 1. && player.movement.on_ground,
                    "{rate}Hz Connector passage toward {goal:?} failed: {:?}", player.movement);
                assert_eq!(player.life_epoch, epoch, "both passages must use the same life");
                use_door(&mut world, actor, door, 0.);
                assert!(passage(&world).hit(), "{rate}Hz closed leaf must block again");
            }
        }
    }
}

#[test]
#[ignore = "requires private classic Nuke/Train BSPs via CSR_DATA_DIR and read-only Legacy dependencies"]
fn cached_classic_silo_and_tanker_roofs_support_falling_players() {
    for (name, model, local_xy, expected_top, count) in [
        ("de_nuke_2016", "models/props/props_gameplay/biohazard_tank_highpoly.mdl", (0., 0.), 189., 3),
        ("de_train_2015", "models/props/de_train/hr_t/train_car_b/train_car_b.mdl", (0., 160.), 150., 26),
    ] {
        let level = private_classic_level(name);
        let bytes = level.vfs.read(&format!("{}.phy", model.trim_end_matches(".mdl"))).unwrap();
        let phy = mdl::Phy::parse(&bytes).unwrap();
        assert_eq!(phy.hulls.len(), count, "{model}: leaf support pieces missing");
        let mut checked = 0;
        for prop in &level.bsp.static_props.props {
            if level.bsp.static_props.models[prop.model as usize] != model || prop.solid != 6 { continue; }
            checked += 1;
            let rotation = mdl::angles_to_mat3(prop.angles);
            let placed = |z| prop.origin + rotation * Vec3::new(local_xy.0, local_xy.1, z);
            let trace = level.collision.trace_box(placed(expected_top + 16.), placed(expected_top - 48.),
                movement::STAND_MINS, movement::STAND_MAXS, collision::contents::MASK_PLAYERSOLID);
            assert!(trace.hit() && !trace.start_solid && trace.normal.z > 0.7, "{name}: {prop:?}: {trace:?}");
            assert!((trace.end.z - prop.origin.z - expected_top).abs() < 0.1,
                "{name}: support must match authored roof, not a brace: {prop:?}: {trace:?}");
            for rate in [64, 128] {
                let mut world = sim::World::new(level.collision.clone());
                world.set_tick_rate(rate);
                let actor = world.add_player("roof support", sim::Team::CounterTerrorist);
                world.player_mut(actor).unwrap().movement = movement::PlayerMove::spawn_at(placed(expected_top + 16.));
                let mut landed = false;
                for _ in 0..rate * 2 {
                    world.step(&[(actor, sim::TickInput { weapon_slot: 3, ..Default::default() })]);
                    let player = world.player(actor).unwrap();
                    landed |= player.last_events.landed.is_some();
                    let at = world.collision.trace_box(player.movement.origin, player.movement.origin,
                        movement::STAND_MINS, movement::STAND_MAXS, collision::contents::MASK_PLAYERSOLID);
                    assert!(player.alive && !at.start_solid && !at.all_solid, "{name} {rate}Hz: {at:?}");
                }
                let player = world.player(actor).unwrap();
                assert!(landed && player.movement.on_ground
                    && (player.movement.origin.z - trace.end.z).abs() < 0.1,
                    "{name} {rate}Hz: roof landing/idle support failed: {:?}", player.movement);
            }
        }
        assert!(checked > 0, "{name}: exact authored model must be present");
    }
}

#[test]
#[ignore = "requires private classic Cobble BSP via CSR_DATA_DIR and read-only Legacy dependencies"]
fn cached_classic_cobble_tall_t_ladder_climbs_through_tick_input() {
    let level = private_classic_level("de_cbble_2018");
    // Tall authored ladder brush: X -1136..-1104, Y 2488..2490,
    // Z -140..320. Approach its south face with the ordinary standing hull.
    let start = Vec3::new(-1120., 2471.96875, -127.96875);
    for z in [start.z, -64., 0., 64., 128., 192., 256.] {
        let point = Vec3::new(start.x, start.y, z);
        assert_eq!(level.collision.ladder_normal(point, movement::STAND_MINS, movement::STAND_MAXS), Some(-Vec3::Y));
        let at = level.collision.trace_box(point, point, movement::STAND_MINS,
            movement::STAND_MAXS, collision::contents::MASK_PLAYERSOLID);
        assert!(!at.start_solid && !at.all_solid, "ladder front at {z}: {at:?}");
    }
    for rate in [64, 128] {
        let mut world = sim::World::new(level.collision.clone());
        world.set_tick_rate(rate);
        let actor = world.add_player("tall T ladder", sim::Team::CounterTerrorist);
        world.player_mut(actor).unwrap().movement = movement::PlayerMove::spawn_at(start);
        for _ in 0..rate * 4 {
            world.step(&[(actor, sim::TickInput { view_angles: Vec3::new(0., 90., 0.),
                forward_move: 450., weapon_slot: 3, ..Default::default() })]);
            let player = world.player(actor).unwrap();
            assert!(player.alive);
            let at = world.collision.trace_box(player.movement.origin, player.movement.origin,
                movement::STAND_MINS, movement::STAND_MAXS, collision::contents::MASK_PLAYERSOLID);
            assert!(!at.start_solid && !at.all_solid, "{rate}Hz: {at:?}");
            if player.movement.origin.z >= 256. { break; }
            assert_eq!(player.movement.ladder_normal, Some(-Vec3::Y));
        }
        assert!(world.player(actor).unwrap().movement.origin.z >= 256.,
            "{rate}Hz: tall ladder climb failed: {:?}", world.player(actor).unwrap().movement);
    }
}

#[test]
#[ignore = "requires read-only CS:GO Legacy installed Overpass; no native process"]
fn installed_overpass_initial_competitive_spawns_touch_buy_zones() {
    let (game, fs) = mount_game(None).expect("installed Legacy dependencies");
    let level = Level::load_from_vfs(ContentId::csgo(ContentKind::Map, "de_overpass"),
        "de_overpass", game, fs).expect("installed Legacy Overpass; no download");
    let cvars = level.cfg("gamemode_competitive.cfg")
        .map(|text| sim::rules::parse_cfg(&text)).unwrap_or_default();
    let rules = sim::RulesConfig::from_cvars(&cvars);
    for rate in [64, 128] {
        let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
        world.set_character_profiles(level.character_profiles.clone());
        world.set_tick_rate(rate);
        world.load_map(&level.bsp, level.overview().as_deref());
        // Exactly the launcher's round_check setup, including the initial
        // deathmatch placements before set_mode starts the first round.
        for i in 0..5 {
            world.add_bot(&format!("T bot {i}"), sim::Team::Terrorist);
            world.add_bot(&format!("CT bot {i}"), sim::Team::CounterTerrorist);
        }
        world.set_mode(sim::GameMode::Competitive, rules.clone());
        for player in &world.players {
            println!("{rate}Hz initial {} id {} {:?}: {:?}",
                player.name, player.id, player.team, player.movement.origin);
        }
        world.step(&[]);
        assert_eq!(world.players.len(), 10);
        for player in &world.players {
            assert!(player.alive && player.in_buy_zone,
                "{rate}Hz {} id {} {:?}: {:?}, buy_zones={:?}",
                player.name, player.id, player.team, player.movement, world.objectives.buy_zones);
        }
    }
}

#[test]
#[ignore = "requires private cached de_cbble_2018 BSP via CSR_DATA_DIR and installed Legacy dependencies"]
fn cached_classic_cobble_admission_routes_never_enter_solid() {
    // Match launcher's movement_checks, including the other player and every
    // preceding route: respawn selection depends on the global tick and ID.
    // Load only already-local content; this entry point never fetches a map.
    let (game, fs) = mount_game(None).expect("installed Legacy dependencies");
    let id = workshop::find("de_cbble_2018").unwrap().content_id();
    let level = Level::load_local_id_from_vfs(&id, game, fs, &workshop::Store::current())
        .expect("private cached Cobble BSP required; no download");
    let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
    world.set_character_profiles(level.character_profiles.clone());
    world.set_tick_rate(128);
    world.load_spawns(&level.bsp);
    let terrorist = world.add_player("terrorist", sim::Team::Terrorist);
    let ct = world.add_player("counter-terrorist", sim::Team::CounterTerrorist);
    for _ in 0..world.ticks(0.5) { world.step(&[]); }
    for actor in [terrorist, ct] {
        for dir in 0..8 {
            world.respawn(actor);
            for _ in 0..8 { world.step(&[]); }
            let start = world.player(actor).unwrap().movement.origin;
            let yaw = dir as f32 * 45.;
            let input = sim::TickInput {
                view_angles: Vec3::new(0., yaw, 0.),
                forward_move: 450.,
                weapon_slot: 3,
                ..Default::default()
            };
            for tick in 0..world.ticks(2.) {
                let before = world.player(actor).unwrap().movement;
                world.step(&[(actor, input)]);
                let player = world.player(actor).unwrap();
                let after = player.movement;
                let (mins, maxs) = after.hull();
                let at = world.collision.trace_box(after.origin, after.origin, mins, maxs,
                    collision::contents::MASK_PLAYERSOLID);
                if at.start_solid || at.all_solid {
                    // One failing command's real sweeps, not a second solver
                    // or changed admission threshold. Keep all operands private
                    // in test output; no installed geometry is copied here.
                    struct Traces<'a>(&'a CollisionWorld, std::cell::RefCell<Vec<(Vec3, Vec3, collision::Trace)>>);
                    impl movement::TraceWorld for Traces<'_> {
                        fn ladder_normal(&self, origin: Vec3, mins: Vec3, maxs: Vec3) -> Option<Vec3> {
                            self.0.ladder_normal(origin, mins, maxs)
                        }
                        fn trace_hull(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32) -> collision::Trace {
                            let tr = self.0.trace_box(start, end, mins, maxs, mask);
                            self.1.borrow_mut().push((start, end, tr));
                            tr
                        }
                    }
                    let traces = Traces(&world.collision, Default::default());
                    let mut replay = before;
                    movement::player_move(&mut replay, &movement::MoveInput {
                        view_angles: input.view_angles,
                        forward_move: input.forward_move,
                        weapon_max_speed: player.active_weapon().map_or(250., |weapon| weapon.max_speed()),
                        ..Default::default()
                    }, &world.config, &traces, world.dt());
                    panic!("{} yaw {yaw}, route start {start:?}, command {tick}, world tick {}: before={before:?}, after={after:?}, at={at:?}, replay_matches={}, sweeps={:#?}",
                        player.name, world.tick, replay == after, traces.1.borrow());
                }
            }
        }
    }
}

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
    let catalog = Arc::new(weapons::Catalog::from_items_game(
        &fs.read_string("scripts/items/items_game.txt").expect("installed weapon schema")
    ).expect("installed weapon catalog"));
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
            let mut world = sim::World::with_catalog(collision.clone(), catalog.clone());
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
                "{rate}Hz {approach}: reciprocal Break must remove both owners; cover={:?}, grate={:?}, scales={:?}, weapon={:?}, events={:?}",
                world.breakables[cover], world.breakables[grate],
                (collision.breakables[cover].club_scale, collision.breakables[cover].minimum_damage),
                world.player(actor).unwrap().active_weapon(), world.events);
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


#[test]
#[ignore = "requires private cached old Vertigo and read-only Legacy dependencies; no native process"]
fn cached_classic_vertigo_parented_doors_open_and_admit_full_player_hulls() {
    let (game, fs) = mount_game(None).expect("installed Legacy dependencies");
    let id = workshop::find("de_vertigo_2017").unwrap().content_id();
    let level = Level::load_local_id_from_vfs(&id, game, fs, &workshop::Store::current())
        .expect("private cached Vertigo BSP; no download");
    let brush_doors: Vec<_> = level.collision.doors.iter().enumerate()
        .filter(|(_, door)| door.brush_model.is_some()).map(|(i, _)| i).collect();
    assert_eq!(brush_doors.len(), 3, "all three authored paired leaves");
    for rate in [64, 128] { for &index in &brush_doors {
        let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
        world.set_character_profiles(level.character_profiles.clone());
        world.set_tick_rate(rate); world.load_spawns(&level.bsp);
        let actor = world.add_player("traversal", sim::Team::CounterTerrorist);
        let def = &world.collision.doors[index];
        let entity = def.entity;
        let center = def.center_at(0.);
        let outside = Vec3::Y * if def.center.x < 0. { 1. } else { -1. };
        let start = Vec3::new(center.x, center.y, def.origin.z - 49.) + outside * 60.;
        world.player_mut(actor).unwrap().movement.origin = start;
        world.player_mut(actor).unwrap().movement.velocity = Vec3::ZERO;
        let eye = world.player(actor).unwrap().eye();
        let delta = center - eye;
        let yaw = delta.y.atan2(delta.x).to_degrees();
        let pitch = (-delta.z).atan2(delta.truncate().length()).to_degrees();
        world.step(&[(actor, sim::TickInput { buttons: movement::buttons::USE,
            view_angles: Vec3::new(pitch,yaw,0.), ..Default::default() })]);
        for _ in 0..rate * 2 { world.step(&[(actor, sim::TickInput::default())]); }
        assert!((world.doors[index].angle.abs()-90.).abs()<0.01,
            "{rate}Hz entity{} failed to open: {:?} from{start:?}", entity, world.doors[index]);
        let player = world.player(actor).unwrap(); let (mins,maxs)=player.movement.hull();
        // The mounted toilet body has an authored ~5.68-unit floor lip and
        // a seat at the back. A horizontal floor-height sweep is not a walking
        // route: let normal step movement cross the lip, and stop before the
        // seat. Check full standing clearance above the existing 18-unit step.
        let step = Vec3::Z * 18.;
        let inside = start - outside * 80.;
        let trace = world.collision.trace_box(start + step, inside + step,
            mins, maxs, collision::contents::MASK_PLAYERSOLID);
        assert!(!trace.hit() && !trace.start_solid,
            "{rate}Hz entity{} open leaf must admit full hull: {trace:?}", entity);
        for _ in 0..rate { world.step(&[(actor, sim::TickInput {
            view_angles: Vec3::new(0.,yaw,0.), forward_move: 450., ..Default::default() })]); }
        assert!((world.player(actor).unwrap().movement.origin-start).dot(-outside)>70.,
            "{rate}Hz entity{} player did not enter", entity);
    } }
}

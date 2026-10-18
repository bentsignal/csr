use super::*;
use glam::Vec3;

/// Read-only installed-data integration. No game/native process, settings,
/// workshop fetch, render system, teleport during traversal, or noclip.
#[test]
#[ignore = "requires read-only CS:GO Legacy installed data; no native process"]
fn installed_inferno_apartment_routes_through_tick_input() {
    let game = vfs::find_csgo_dir().expect("CS:GO Legacy install / CSR_GAME_DIR required");
    let mut fs = vfs::Vfs::new();
    fs.mount_game_dir(&game).unwrap();
    let map = bsp::Bsp::parse(
        &fs.read("maps/de_inferno.bsp")
            .expect("installed Inferno BSP"),
    )
    .unwrap();
    fs.mount_map_pakfile(map.pakfile.clone()).unwrap();
    let loader = |model: &str| {
        let bytes = fs.read(&format!("{}.phy", model.trim_end_matches(".mdl")))?;
        mdl::Phy::parse(&bytes).ok().map(|phy| phy.hulls)
    };
    let collision = Arc::new(CollisionWorld::from_bsp(
        &map,
        &MapCollisionOptions::default(),
        &loader,
    ));
    // Numeric waypoints observed in the installed BSP/NAV, in Source feet
    // coordinates. Cover both A-apartment stair approaches, the corridor to
    // balcony, and both western apartment/bridge approaches. Actual geometry
    // still decides every step and support height; NAV heights aren't solids.
    let routes: &[(&str, [f32; 3], &[[f32; 3]])] = &[
        (
            "CT-side A apartments",
            [1390., 312., 150.],
            &[
                [1212.5, 312., 128.],
                [1212.5, 30., 256.],
                [1225., -325., 256.],
                [1875., -325., 256.],
                [1875., -188., 256.],
                [2037., -188., 256.],
            ],
        ),
        (
            "T-side back alley",
            [900., -632., 160.],
            &[
                [1075., -632., 128.],
                [1225., -632., 128.],
                [1225., -320., 256.],
                [1875., -325., 256.],
                [1875., -188., 256.],
                [2037., -188., 256.],
            ],
        ),
        (
            "western apartment stairs",
            [200., -450., 110.],
            &[
                [-300., -450., 192.],
                [-387.5, -450., 192.],
                [-387.5, 0., 192.],
            ],
        ),
        (
            "western upstairs and bridge",
            [-568., 455., 60.],
            &[
                [-568., 100., 184.],
                [-387.5, 100., 184.],
                [-387.5, -312., 192.],
            ],
        ),
    ];
    for rate in [64, 128] {
        for &(name, start, goals) in routes {
            let mut world = sim::World::new(collision.clone());
            world.set_tick_rate(rate);
            assert_eq!(world.tick_rate, rate);
            let actor = world.add_player(name, sim::Team::CounterTerrorist);
            let start = world.drop_to_floor(Vec3::from_array(start), false);
            world.player_mut(actor).unwrap().movement = movement::PlayerMove::spawn_at(start);
            // The same player walks back out through every threshold. There
            // is no relocation or reset of velocity between the two legs.
            let outward = goals.iter().copied().map(Vec3::from_array);
            let returning = goals[..goals.len() - 1]
                .iter()
                .rev()
                .copied()
                .map(Vec3::from_array);
            for goal in outward.chain(returning).chain(std::iter::once(start)) {
                let mut reached = false;
                // Gentle ordinary input near corners prevents momentum from
                // steering the fixture into the side of a narrow doorway.
                for _ in 0..rate * 16 {
                    let player = world.player(actor).unwrap();
                    let delta = (goal - player.movement.origin).truncate();
                    let distance = delta.length();
                    if distance < 2. && player.movement.on_ground {
                        assert!(
                            (player.movement.origin.z - goal.z).abs() < 8.,
                            "{name} {rate}Hz: wrong floor at {goal:?}: {:?}",
                            player.movement
                        );
                        let support = collision.trace_box(
                            player.movement.origin,
                            player.movement.origin - Vec3::Z * 2.,
                            movement::STAND_MINS,
                            movement::STAND_MAXS,
                            collision::contents::MASK_PLAYERSOLID,
                        );
                        assert!(
                            !support.start_solid
                                && support.fraction < 1.
                                && support.normal.z >= movement::MIN_GROUND_NORMAL,
                            "{name} {rate}Hz: waypoint has no walkable support at {goal:?}: {support:?}"
                        );
                        reached = true;
                        break;
                    }
                    world.step(&[(
                        actor,
                        sim::TickInput {
                            view_angles: Vec3::new(0., delta.y.atan2(delta.x).to_degrees(), 0.),
                            forward_move: (distance * 4.).min(130.),
                            buttons: movement::buttons::WALK,
                            weapon_slot: 3,
                            ..Default::default()
                        },
                    )]);
                    let player = world.player(actor).unwrap();
                    assert!(
                        player.alive
                            && !player.movement.ducked
                            && player.movement.duck_amount == 0.
                            && !player.last_events.jumped
                            && player.movement.origin.is_finite()
                            && player.movement.velocity.is_finite(),
                        "{name} {rate}Hz: route requires finite, living standing movement without jumping"
                    );
                    let tr = collision.trace_box(
                        player.movement.origin,
                        player.movement.origin,
                        movement::STAND_MINS,
                        movement::STAND_MAXS,
                        collision::contents::MASK_PLAYERSOLID,
                    );
                    assert!(
                        !tr.start_solid && !tr.all_solid,
                        "{name} {rate}Hz: entered solid: {:?}",
                        player.movement
                    );
                }
                assert!(
                    reached,
                    "{name} {rate}Hz: blocked before {goal:?}: {:?}",
                    world.player(actor).unwrap().movement
                );
            }
        }
    }
}

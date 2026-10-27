//! Focused private-pack proof. No original assets or private pack fixture in CI.
use glam::Vec3;
use sim::{Team, TickInput};
use std::{path::Path, sync::Arc};

fn load_module(pack: &packs::Pack) -> Result<Arc<sim::mechanics::Module>, String> {
    let def = pack
        .manifest()
        .mechanics
        .first()
        .ok_or("pack has no mechanics")?;
    Ok(Arc::new(sim::mechanics::Module {
        id: def.id.clone(),
        name: def.name.clone(),
        archive_hash: pack.archive_hash().into(),
        program: if def.api_version == 1 {
            Some(mechanics::Program::compile(
                &pack.file(&def.module).ok_or("missing module")?,
            )?)
        } else {
            None
        },
        config: mechanics::parse_config(&pack.file(&def.config).ok_or("missing configuration")?)?,
        external: if def.api_version == 2 {
            Some(Arc::new(mechanics::external::Program::new(
                std::env::var_os("CSR_SKATE_RUNTIME")
                    .map(std::path::PathBuf::from)
                    .ok_or("CSR_SKATE_RUNTIME missing")?,
                mechanics::data::materialize(
                    &pack.file(&def.module).ok_or("missing runtime data")?,
                    &std::env::temp_dir().join("csr-skatecheck-cache"),
                )?,
            )?))
        } else {
            None
        },
    }))
}

fn run() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: skatecheck <private pack.csrpack> [map]")?;
    let pack = packs::Pack::open(Path::new(&path))?;
    let module = load_module(&pack)?;
    let full = pack.manifest().mechanics[0].api_version == 2;
    if full {
        grindcheck(module.clone())?;
    }
    let mut collision = collision::CollisionWorld::new();
    let floor = [
        Vec3::new(-5000., -5000., 0.),
        Vec3::new(5000., -5000., 0.),
        Vec3::new(5000., 5000., 0.),
        Vec3::new(-5000., 5000., 0.),
    ];
    for tri in [[0, 1, 2], [0, 2, 3]] {
        collision.add_triangle(tri.map(|i| floor[i]), collision::contents::SOLID, 0);
    }
    collision.finalize();
    let collision = Arc::new(collision);
    if full {
        markercheck(module.clone(),collision.clone())?;
        offboardcheck(module.clone(), collision.clone())?;
        flipcheck(module.clone(), collision.clone())?;
        dynamiccheck(&module)?;
    }
    for rate in [64, 128] {
        let mut world = sim::World::new(collision.clone());
        world.set_tick_rate(rate);
        world.mechanics.push(module.clone());
        let id = world.add_player("Skate check", Team::CounterTerrorist);
        world.player_mut(id).unwrap().movement.origin = Vec3::Z * 0.1;
        for _ in 0..4 {
            world.step(&[(id, TickInput::default())]);
        }
        let before = world.player(id).unwrap().weapons.len();
        world.step(&[(
            id,
            TickInput {
                toggle_mode: true,
                ..Default::default()
            },
        )]);
        if world.player(id).unwrap().mechanic.is_none() {
            return Err("module activation failed".into());
        }
        let mut peak = 0f32;
        for tick in 0..rate * 3 {
            let input = TickInput {
                controller: if full {
                    controller(tick as f32 / rate as f32)
                } else {
                    Default::default()
                },
                forward_move: 450.,
                buttons: if tick == rate {
                    movement::buttons::JUMP
                } else {
                    0
                },
                ..Default::default()
            };
            world.step(&[(id, input)]);
            peak = peak.max(world.player(id).unwrap().movement.origin.z);
        }
        let p = world.player(id).unwrap();
        println!(
            "{rate} Hz: position={:?} speed={:.2} peak={peak:.2} mode={:?}",
            p.movement.origin,
            p.movement.speed_2d(),
            p.mechanic.as_ref().map(|s| &s.id)
        );
        if p.mechanic.is_none() || p.movement.origin.x < 30. || peak < 8. {
            return Err("riding/ollie did not advance".into());
        }
        let state = world.state();
        let mut replay = world.clone();
        replay.restore(state);
        let mut future = Vec::new();
        for tick in 0..rate {
            let input = TickInput {
                controller: if full {
                    controller(3. + tick as f32 / rate as f32)
                } else {
                    Default::default()
                },
                forward_move: 450.,
                side_move: if tick < rate / 2 { 450. } else { 0. },
                ..Default::default()
            };
            world.step(&[(id, input)]);
            future.push((input, world.state_hash()));
        }
        for (input, hash) in future {
            replay.step(&[(id, input)]);
            if hash != replay.state_hash() {
                return Err("movement diverged after snapshot restore".into());
            }
        }
        world.step(&[(
            id,
            TickInput {
                toggle_mode: true,
                ..Default::default()
            },
        )]);
        if world.player(id).unwrap().mechanic.is_some()
            || world.player(id).unwrap().weapons.len() != before
        {
            return Err("mode switching changed inventory or failed".into());
        }
    }
    let map = std::env::args().nth(2).unwrap_or("de_dust2".into());
    let level = level::Level::load(&map, None).map_err(|e| e.to_string())?;
    let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
    world.set_character_profiles(level.character_profiles.clone());
    world.load_map(&level.bsp, level.overview().as_deref());
    if let Some(provider)=&module.external {
        let started=std::time::Instant::now();provider.prepare_world(&world.collision)?;
        println!("Map provider preparation: {:?}",started.elapsed());
    }
    world.mechanics.push(module);
    let id = world.add_player("Map skate check", Team::CounterTerrorist);
    world.step(&[(id, TickInput {
        weapon_slot: sim::SLOT_PREVIOUS,
        ..Default::default()
    })]);
    if world.player(id).unwrap().equipped_mechanic.is_none() {
        return Err("map inventory did not select the board".into());
    }
    let activation=std::time::Instant::now();
    world.step(&[(
        id,
        TickInput {
            buttons: movement::buttons::ATTACK,
            controller: mechanics::link::Controller {
                connected: true,
                triggers: [0, 255],
                ..Default::default()
            },
            ..Default::default()
        },
    )]);
    println!("Prepared first activation: {:?}",activation.elapsed());
    let started = std::time::Instant::now();
    let mut foot_error = Vec3::ZERO;
    let mut max_foot_error = 0f32;
    println!(
        "Map entry: origin={:?} yaw={}",
        world.player(id).unwrap().movement.origin,
        world.player(id).unwrap().view_angles.y
    );
    for tick in 0..512 {
        let input = TickInput {
            controller: if full {
                controller(tick as f32 / 128.)
            } else {
                Default::default()
            },
            view_angles: Vec3::ZERO,
            forward_move: 450.,
            side_move: if (128..256).contains(&tick) { 450. } else { 0. },
            buttons: if tick == 300 {
                movement::buttons::JUMP
            } else {
                0
            },
            ..Default::default()
        };
        world.step(&[(id, input)]);
        if tick == 128 {
            let p = world.player(id).unwrap();
            if let (Some(state), Some(profile)) = (
                p.mechanic.as_ref().and_then(|s| s.external.as_ref()),
                world.character_profiles().for_team(p.team),
            ) {
                let bones = profile.animations.bones();
                let mut pose = mdl::skeleton::model_space(bones, &mdl::skeleton::rest_pose(bones));
                sim::model_pose::apply_movement_pose(p, bones, &mut pose);
                let frame = glam::Mat4::from_rotation_translation(
                    glam::Quat::from_rotation_z(p.view_angles.y.to_radians()),
                    p.movement.origin,
                );
                let conversion = glam::Mat4::from_cols_array_2d(&state.rig.to_source);
                for (cs, skate) in [("ankle_L", "LEFTFOOT"), ("ankle_R", "RIGHTFOOT")] {
                    if let (Some(a), Some(b)) = (
                        bones.iter().position(|b| b.name == cs),
                        state.rig.names.iter().position(|n| n == skate),
                    ) {
                        let target = conversion.transform_point3(
                            glam::Mat4::from_cols_array_2d(&state.pose.bones[b])
                                .w_axis
                                .truncate(),
                        );
                        let actual = frame.transform_point3(pose[a].w_axis.truncate());
                        foot_error += (actual - target) * 0.5;
                        max_foot_error = max_foot_error.max(actual.distance(target));
                    }
                }
            }
        }
    }
    let player = world.player(id).unwrap();
    if let Some(state) = player.mechanic.as_ref().and_then(|s| s.external.as_ref()) {
        println!(
            "native state={} position={:?}",
            state.pose.state, state.pose.position
        );
        if let Some(profile) = world.character_profiles().for_team(player.team) {
            let bones = profile.animations.bones();
            let mut pose = mdl::skeleton::model_space(bones, &mdl::skeleton::rest_pose(bones));
            sim::model_pose::apply_movement_pose(player, bones, &mut pose);
            if pose.iter().any(|m| !m.is_finite()) {
                return Err("retarget produced a nonfinite CS pose".into());
            }
        }
    }
    if player.mechanic.is_none() || !player.movement.origin.is_finite() {
        return Err("real-map movement module failed".into());
    }
    println!(
        "{map}: 512 ticks in {:?}, origin={:?}, archive={}",
        started.elapsed(),
        player.movement.origin,
        pack.archive_hash()
    );
    println!("Retarget average ankle offset: {foot_error:?}");
    if max_foot_error > 0.1 {
        return Err(format!(
            "CS feet miss the native board pose by {max_foot_error:.2} inches"
        ));
    }
    world.step(&[(id,TickInput{toggle_mode:true,..Default::default()})]);
    let actor=world.player(id).unwrap();
    let (mins,maxs)=actor.movement.hull();
    let hit=world.collision.trace_box(actor.movement.origin,actor.movement.origin,mins,maxs,collision::contents::MASK_PLAYERSOLID);
    println!("Real-map CS return: mode={}, position={:?}, start_solid={}, all_solid={}",actor.mechanic.is_some(),actor.movement.origin,hit.start_solid,hit.all_solid);
    if actor.mechanic.is_some()||hit.start_solid||hit.all_solid{return Err("real-map CS exit did not produce a clear player hull".into());}
    println!("failures: 0");
    Ok(())
}
fn controller(t: f32) -> mechanics::link::Controller {
    let mut report = mechanics::link::Controller {
        connected: true,
        ..Default::default()
    };
    if t < 1.5 {
        report.buttons = 0x1000;
    }
    if (1.5..2.).contains(&t) {
        report.right = [0, -32768];
    }
    if (2.0..2.2).contains(&t) {
        report.right = [0, 32767];
    }
    if (3.0..3.5).contains(&t) {
        report.left = [18000, 0];
    }
    report
}
fn markercheck(module:Arc<sim::mechanics::Module>,collision:Arc<collision::CollisionWorld>)->Result<(),String>{
    use mechanics::link::Controller;
    let mut w=sim::World::new(collision.clone());w.mechanics.push(module);
    let id=w.add_player("Session marker",Team::CounterTerrorist);
    w.player_mut(id).unwrap().movement.origin=Vec3::Z*0.1;
    w.step(&[(id,TickInput{toggle_mode:true,..Default::default()})]);
    for _ in 0..128 {w.step(&[(id,TickInput{controller:Controller{connected:true,..Default::default()},..Default::default()})]);}
    w.step(&[(id,TickInput{controller:Controller{connected:true,buttons:0x102,..Default::default()},..Default::default()})]);
    let marker=w.player(id).unwrap().mechanic_checkpoint.clone().ok_or("grounded marker placement failed")?;
    let p=marker.target.transform[3];let expected=Vec3::new(p[0],-p[2],p[1])/0.0254;
    let saved=w.state();let mut replay=w.clone();replay.restore(saved);
    let mut future=Vec::new();
    for _ in 0..256 {
        let input=TickInput{controller:Controller{connected:true,buttons:0x1000,..Default::default()},..Default::default()};
        w.step(&[(id,input)]);future.push((input,w.state_hash()));
    }
    for (input,hash) in future {replay.step(&[(id,input)]);if hash!=replay.state_hash(){return Err("marker diverged after restore".into());}}
    let distance=w.player(id).unwrap().movement.origin.truncate().distance(expected.truncate());
    if distance<40. {return Err("marker test did not travel away".into());}
    w.step(&[(id,TickInput{toggle_mode:true,..Default::default()})]);
    let actor=w.player(id).unwrap();let (mins,maxs)=actor.movement.hull();let hit=collision.trace_box(actor.movement.origin,actor.movement.origin,mins,maxs,collision::contents::MASK_PLAYERSOLID);
    if actor.mechanic.is_some()||hit.start_solid||hit.all_solid{return Err("grounded Skate exit embedded CS hull".into());}
    if actor.mechanic_checkpoint.as_ref()!=Some(&marker){return Err("CS exit erased session marker".into());}
    w.step(&[(id,TickInput::default())]);
    w.step(&[(id,TickInput{toggle_mode:true,..Default::default()})]);
    for _ in 0..256 {w.step(&[(id,TickInput{controller:Controller{connected:true,buttons:0x101,..Default::default()},..Default::default()})]);}
    let returned=w.player(id).unwrap().movement.origin.truncate().distance(expected.truncate());
    println!("Session marker: travelled={distance:.2}in, return error={returned:.2}in; CS exit hull clear and marker preserved");
    if returned>20. {return Err("session marker did not return through the native actor reset".into());}
    Ok(())
}
fn grindcheck(module: Arc<sim::mechanics::Module>) -> Result<(), String> {
    let mut collision = collision::CollisionWorld::new();
    let floor = [
        Vec3::new(-5000., -5000., 0.),
        Vec3::new(5000., -5000., 0.),
        Vec3::new(5000., 5000., 0.),
        Vec3::new(-5000., 5000., 0.),
    ];
    for tri in [[0, 1, 2], [0, 2, 3]] {
        collision.add_triangle(tri.map(|i| floor[i]), collision::contents::SOLID, 0);
    }
    let low = Vec3::new(0., 20., 0.);
    let high = Vec3::new(1000., 80., 32.);
    collision.add_convex(collision::Convex {
        planes: vec![
            collision::Plane {
                normal: Vec3::X,
                dist: high.x,
            },
            collision::Plane {
                normal: -Vec3::X,
                dist: -low.x,
            },
            collision::Plane {
                normal: Vec3::Y,
                dist: high.y,
            },
            collision::Plane {
                normal: -Vec3::Y,
                dist: -low.y,
            },
            collision::Plane {
                normal: Vec3::Z,
                dist: high.z,
            },
            collision::Plane {
                normal: -Vec3::Z,
                dist: -low.z,
            },
        ],
        mins: low,
        maxs: high,
        contents: collision::contents::SOLID,
        surface: 0,
        plane_surfaces: vec![],
    });
    collision.finalize();
    let collision = Arc::new(collision);
    for side in [20., 19., 21.] {
        let mut world = sim::World::new(collision.clone());
        world.mechanics.push(module.clone());
        let id = world.add_player("Ledge check", Team::CounterTerrorist);
        let player = world.player_mut(id).unwrap();
        player.movement.origin = Vec3::new(100., side, 32.1);
        player.movement.velocity = Vec3::X * 180.;
        let mut acquired = false;
        for tick in 0..256 {
            world.step(&[(
                id,
                TickInput {
                    toggle_mode: tick == 0,
                    controller: mechanics::link::Controller {
                        connected: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )]);
            let state = world
                .player(id)
                .unwrap()
                .mechanic
                .as_ref()
                .and_then(|s| s.external.as_ref())
                .ok_or("grind check runtime failed")?;
            if (400..406).contains(&state.pose.state) {
                acquired = true;
            }
        }
        println!("Ledge side={side}: native grind acquired={acquired}");
        if acquired {
            return Ok(());
        }
    }
    Err("native grind acquisition did not occur on the exported ledge".into())
}
fn offboardcheck(
    module: Arc<sim::mechanics::Module>,
    collision: Arc<collision::CollisionWorld>,
) -> Result<(), String> {
    let mut world = sim::World::new(collision);
    world.mechanics.push(module);
    let id = world.add_player("Walk/remount check", Team::CounterTerrorist);
    world.player_mut(id).unwrap().movement.origin = Vec3::Z * 0.1;
    let mut walked = false;
    let mut remounted = false;
    let mut start = None;
    let mut distance = 0f32;
    let mut look_baseline=None;
    let mut look_up=None;
    for tick in 0..128 * 7 {
        let t = tick as f32 / 128.;
        let mut report = mechanics::link::Controller {
            connected: true,
            ..Default::default()
        };
        if (0.75..0.9).contains(&t) || (4.0..4.15).contains(&t) {
            report.buttons = 0x8000;
        }
        if (1.5..3.5).contains(&t) {
            report.left = [0, 25000];
        }
        if (2.5..3.5).contains(&t){report.right=[0,24000];}
        world.step(&[(
            id,
            TickInput {
                toggle_mode: tick == 0,
                controller: report,
                ..Default::default()
            },
        )]);
        let p = world.player(id).unwrap();
        let state = p
            .mechanic
            .as_ref()
            .and_then(|s| s.external.as_ref())
            .ok_or("offboard runtime failed")?;
        if (500..600).contains(&state.pose.state) {
            walked = true;
            let start = *start.get_or_insert(p.movement.origin);
            distance = distance.max(p.movement.origin.distance(start));
            if (2.45..2.5).contains(&t){look_baseline=state.pose.camera.map(|m|Vec3::from_slice(&m[2][..3]).normalize_or_zero().z);}
            if (3.45..3.5).contains(&t){look_up=state.pose.camera.map(|m|Vec3::from_slice(&m[2][..3]).normalize_or_zero().z);}
        }
        if t > 4.5 && (100..200).contains(&state.pose.state) {
            remounted = true;
        }
    }
    println!("Offboard: walked={walked}, distance={distance:.2}, remounted={remounted}");
    if !walked || !remounted || distance < 24. {
        return Err("native walking/remount did not complete".into());
    }
    println!("Offboard vertical look: before={look_baseline:?}, stick-up={look_up:?}");
    if !matches!((look_baseline,look_up),(Some(before),Some(after)) if after>before+0.03){return Err("walking stick-up did not raise the camera aim".into());}
    Ok(())
}
fn dynamiccheck(module: &sim::mechanics::Module) -> Result<(), String> {
    use mechanics::link::*;
    let program = module.external.as_ref().unwrap();
    let floor = [
        [-5000., -5000., 0.],
        [5000., -5000., 0.],
        [5000., 5000., 0.],
        [-5000., 5000., 0.],
    ];
    let points = |x: f32| {
        [x, x + 4.]
            .into_iter()
            .flat_map(|x| {
                [-64., 64.]
                    .into_iter()
                    .flat_map(move |y| [0., 120.].map(move |z| [x, y, z]))
            })
            .collect::<Vec<_>>()
    };
    let mut distances = vec![];
    for (case, (door, removed)) in [(true, false), (true, true), (false, false), (false, true)]
        .into_iter()
        .enumerate()
    {
        let part = points(100.);
        let mut scene = Scene {
            triangles: [[0, 1, 2], [0, 2, 3]]
                .map(|ids| Triangle {
                    points: ids.map(|i| floor[i]),
                    surface: 0,
                })
                .to_vec(),
            ..Default::default()
        };
        if door {
            scene.doors.push(Door {
                origin: [100., -64., 0.],
                triangles: vec![],
                parts: vec![part],
            });
        } else {
            scene.breakables.push(Breakable {
                killed_by: vec![0],
                triangles: vec![],
                parts: vec![part],
            });
        }
        let start = Start {
            checkpoint:None,
            position: [0., 0., 3.5],
            velocity: [240., 0., 0.],
            yaw: 0.,
        };
        let mut journal = mechanics::external::Journal::default();
        let mut hash = mechanics::external::digest([0; 32], &start);
        let mut x = 0.;
        for _ in 0..512 {
            let frame = Frame {
                input: Controller {
                    connected: true,
                    buttons: 0x1000,
                    ..Default::default()
                },
                dt: 1. / 128.,
                scene: SceneState {
                    doors: vec![if removed { 90. } else { 0. }],
                    broken: vec![removed],
                },
            };
            let (pose, _) = program.advance(
                &scene,
                100 + case,
                100 + case as u64,
                &start,
                &journal,
                hash,
                &frame,
            )?;
            x = pose.position[0];
            hash = mechanics::external::digest(hash, &frame);
            journal.push(frame);
        }
        distances.push(x);
    }
    println!(
        "Dynamic contacts: door closed/open x={:.1}/{:.1}, breakable intact/broken x={:.1}/{:.1}",
        distances[0], distances[1], distances[2], distances[3]
    );
    if distances[0] > 130. || distances[2] > 130. || distances[1] < 250. || distances[3] < 250. {
        return Err("native dynamic contact did not match live CS geometry".into());
    }
    Ok(())
}
fn flipcheck(
    module: Arc<sim::mechanics::Module>,
    collision: Arc<collision::CollisionWorld>,
) -> Result<(), String> {
    let mut world = sim::World::new(collision);
    world.mechanics.push(module);
    let id = world.add_player("Flick check", Team::CounterTerrorist);
    world.player_mut(id).unwrap().movement.origin = Vec3::Z * 0.1;
    let mut inverted = false;
    let mut trick = String::new();
    for tick in 0..384 {
        let t = tick as f32 / 128.;
        let mut report = mechanics::link::Controller {
            connected: true,
            ..Default::default()
        };
        if (0.8..1.3).contains(&t) {
            report.right = [0, -32768];
        }
        if (1.3..1.5).contains(&t) {
            report.right = [30000, 30000];
        }
        world.step(&[(
            id,
            TickInput {
                toggle_mode: tick == 0,
                controller: report,
                ..Default::default()
            },
        )]);
        let state = world
            .player(id)
            .unwrap()
            .mechanic
            .as_ref()
            .and_then(|s| s.external.as_ref())
            .ok_or("flick runtime failed")?;
        if let Some(i) = state.rig.names.iter().position(|n| n == "SKATEBOARD_ROOT") {
            inverted |= state.pose.bones[i][1][1] < -0.5;
        }
        if state.pose.trick.to_ascii_lowercase().contains("flip") {
            trick = state.pose.trick.clone();
        }
    }
    println!("Flick: inverted={inverted}, trick={trick}");
    if !inverted || trick.is_empty() {
        return Err("right-stick flick did not produce an articulated flip trick".into());
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

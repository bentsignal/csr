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
        program: if def.api_version==1{Some(mechanics::Program::compile(&pack.file(&def.module).ok_or("missing module")?)?)}else{None},
        config: mechanics::parse_config(&pack.file(&def.config).ok_or("missing configuration")?)?,
        external: if def.api_version==2{Some(Arc::new(mechanics::external::Program::new(
            std::env::var_os("CSR_SKATE_RUNTIME").map(std::path::PathBuf::from).ok_or("CSR_SKATE_RUNTIME missing")?,
            mechanics::data::materialize(&pack.file(&def.module).ok_or("missing runtime data")?,&std::env::temp_dir().join("csr-skatecheck-cache"))?)?))}else{None},
    }))
}

fn run() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: skatecheck <private pack.csrpack> [map]")?;
    let pack = packs::Pack::open(Path::new(&path))?;
    let module = load_module(&pack)?;
    let full=pack.manifest().mechanics[0].api_version==2;
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
                controller: if full {controller(tick as f32/rate as f32)}else{Default::default()},
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
        let mut future=Vec::new();
        for tick in 0..rate {
            let input = TickInput {
                controller: if full {controller(3.+tick as f32/rate as f32)}else{Default::default()},
                forward_move: 450.,
                side_move: if tick < rate / 2 { 450. } else { 0. },
                ..Default::default()
            };
            world.step(&[(id, input)]);
            future.push((input,world.state_hash()));
        }
        for (input,hash) in future {
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
    world.mechanics.push(module);
    let id = world.add_player("Map skate check", Team::CounterTerrorist);
    world.step(&[(
        id,
        TickInput {
            toggle_mode: true,
            ..Default::default()
        },
    )]);
    let started = std::time::Instant::now();
    for tick in 0..512 {
        let input = TickInput {
            controller: if full {controller(tick as f32/128.)}else{Default::default()},
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
    }
    let player = world.player(id).unwrap();
    if player.mechanic.is_none() || !player.movement.origin.is_finite() {
        return Err("real-map movement module failed".into());
    }
    println!(
        "{map}: 512 ticks in {:?}, origin={:?}, archive={}",
        started.elapsed(),
        player.movement.origin,
        pack.archive_hash()
    );
    println!("failures: 0");
    Ok(())
}
fn controller(t:f32)->mechanics::link::Controller{
    let mut report=mechanics::link::Controller{connected:true,..Default::default()};
    if t<1.5{report.buttons=0x1000;}
    if (1.5..2.).contains(&t){report.right=[0,-32768];}
    if (2.0..2.2).contains(&t){report.right=[0,32767];}
    if (3.0..3.5).contains(&t){report.left=[18000,0];}
    report
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

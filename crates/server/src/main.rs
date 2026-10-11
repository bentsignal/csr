//! Dedicated server, intentionally independent of the Bevy client.
use netcode::{
    prediction::{Predictor, Reconciliation},
    protocol::{ContentManifest, InputCommand, StateUpdate},
    session::Session,
    transport::{Client, Server},
};
use sha1::{Digest, Sha1};
use sim::{GameMode, RulesConfig, Team, TickInput, World};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct Options {
    map: String,
    bind: SocketAddr,
    tick_rate: u32,
    bots: usize,
    max_players: usize,
    deathmatch: bool,
    ticks: Option<u64>,
    netcheck: bool,
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut out = Options {
        map: "de_dust2".into(),
        bind: "0.0.0.0:27015".parse().unwrap(),
        tick_rate: 128,
        bots: 0,
        max_players: 10,
        deathmatch: false,
        ticks: None,
        netcheck: false,
    };
    let mut args = args.iter().peekable();
    if args.peek().is_some_and(|s| !s.starts_with('-')) {
        out.map = args.next().unwrap().clone();
    }
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--deathmatch" => out.deathmatch = true,
            "--netcheck" => out.netcheck = true,
            "--bind" => {
                out.bind = args
                    .next()
                    .ok_or("--bind needs IP:PORT")?
                    .parse()
                    .map_err(|_| "invalid bind address")?
            }
            "--tickrate" => {
                out.tick_rate = args
                    .next()
                    .ok_or("--tickrate needs a number")?
                    .parse()
                    .map_err(|_| "invalid tick rate")?
            }
            "--bots" => {
                out.bots = args
                    .next()
                    .ok_or("--bots needs a number")?
                    .parse()
                    .map_err(|_| "invalid bot count")?
            }
            "--maxplayers" => {
                out.max_players = args
                    .next()
                    .ok_or("--maxplayers needs a number")?
                    .parse()
                    .map_err(|_| "invalid player count")?
            }
            "--ticks" => {
                out.ticks = Some(
                    args.next()
                        .ok_or("--ticks needs a number")?
                        .parse()
                        .map_err(|_| "invalid tick count")?,
                )
            }
            _ => return Err(format!("unknown option: {arg}")),
        }
    }
    if !sim::TICK_RATE_RANGE.contains(&out.tick_rate) {
        return Err("tick rate must be from 16 to 256".into());
    }
    if out.bots > 64
        || out.max_players == 0
        || out.max_players > 64
        || out.bots + out.max_players > 64
    {
        return Err("server supports 1 to 64 total player slots".into());
    }
    if out.ticks == Some(0) {
        return Err("--ticks must be positive".into());
    }
    Ok(out)
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!(
            "CSGO Rewrite dedicated server\ncsr-server [MAP] [--bind IP:PORT] [--tickrate 128] [--bots N] [--maxplayers N]\n           [--deathmatch] [--ticks N] [--netcheck]\nReads CS:GO Legacy automatically; CSR_GAME_DIR overrides.\nDirect-IP prototype: no Steam identity or server browser yet."
        );
        return;
    }
    if let Err(error) = options(&args).and_then(run) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(options: Options) -> Result<(), String> {
    let level = level::Level::load(&options.map, None).map_err(|e| e.to_string())?;
    let map_bytes = level
        .vfs
        .read(&format!("maps/{}.bsp", level.name))
        .ok_or("map bytes unavailable")?;
    let catalog_bytes = level
        .vfs
        .read("scripts/items/items_game.txt")
        .ok_or("weapon catalog unavailable")?;
    let hash = |bytes: &[u8]| {
        Sha1::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let content = ContentManifest {
        map_id: level.id.to_string(),
        map_name: level.name.clone(),
        map_hash: hash(&map_bytes),
        catalog_hash: hash(&catalog_bytes),
    };
    let mut world = World::with_catalog(level.collision.clone(), level.catalog.clone());
    world.set_tick_rate(options.tick_rate);
    let overview = level
        .vfs
        .read_string(&format!("resource/overviews/{}.txt", level.name));
    world.load_map(&level.bsp, overview.as_deref());
    world.bots.nav = level.nav.clone();
    let cfg = level
        .vfs
        .read_string("cfg/gamemode_competitive.cfg")
        .unwrap_or_default();
    let rules = RulesConfig::from_cvars(&sim::rules::parse_cfg(&cfg));
    world.set_mode(
        if options.deathmatch || options.netcheck {
            GameMode::Deathmatch
        } else {
            GameMode::Competitive
        },
        rules,
    );
    for n in 0..options.bots {
        world.add_bot(
            &format!("Bot {}", n + 1),
            if n % 2 == 0 {
                Team::Terrorist
            } else {
                Team::CounterTerrorist
            },
        );
    }
    let session = Session::new(world, content.clone())?;
    if options.netcheck {
        return netcheck(session, level.catalog);
    }
    let mut server = Server::bind(options.bind, session, options.max_players)?;
    println!(
        "Serving {} ({}) at {}: {} tick, {} bots, {} joining slots",
        level.name,
        level.id,
        server.address(),
        options.tick_rate,
        options.bots,
        options.max_players
    );
    let running = Arc::new(AtomicBool::new(true));
    let stop = running.clone();
    ctrlc::set_handler(move || {
        stop.store(false, Ordering::Relaxed);
    })
    .map_err(|e| e.to_string())?;
    let mut previous = Instant::now();
    while running.load(Ordering::Relaxed)
        && options
            .ticks
            .is_none_or(|ticks| server.session().world().tick < ticks)
    {
        let now = Instant::now();
        server.update(now.duration_since(previous))?;
        previous = now;
        std::thread::sleep(Duration::from_millis(1));
    }
    println!(
        "Stopped at tick {}; discarded wall ticks: {}",
        server.session().world().tick,
        server.discarded_wall_ticks()
    );
    server.shutdown();
    Ok(())
}

fn netcheck(session: Session, catalog: Arc<weapons::Catalog>) -> Result<(), String> {
    let content = session.content().clone();
    let dt = Duration::from_secs_f64(1. / session.world().tick_rate as f64);
    let mut server = Server::bind("127.0.0.1:0".parse().unwrap(), session, 4)?;
    let mut clients = vec![
        Client::connect(
            server.address(),
            "Netcheck CT".into(),
            Team::CounterTerrorist,
            content.clone(),
            catalog.clone(),
        )?,
        Client::connect(
            server.address(),
            "Netcheck T".into(),
            Team::Terrorist,
            content,
            catalog,
        )?,
    ];
    let mut history = std::collections::BTreeMap::new();
    for _ in 0..256 {
        for client in &mut clients {
            client.update(dt)?;
        }
        server.update(dt)?;
        history.insert(
            server.session().world().tick,
            server.session().world().state(),
        );
        if clients.iter().all(|c| c.player().is_some()) {
            break;
        }
        std::thread::yield_now();
    }
    let ids: Vec<_> = clients
        .iter()
        .map(|c| c.player().ok_or("loopback admission failed"))
        .collect::<Result<_, _>>()?;
    if ids[0] == ids[1] {
        return Err("connections share player ownership".into());
    }
    let mut starting: Vec<Option<Predictor>> = vec![None, None];
    for _ in 0..256 {
        for client in &mut clients {
            client.update(dt)?;
        }
        server.update(dt)?;
        history.insert(
            server.session().world().tick,
            server.session().world().state(),
        );
        for (n, client) in clients.iter_mut().enumerate() {
            if starting[n].is_none() {
                if let Some(initial) = client.take_state() {
                    let source = server.session().world();
                    let mut map_world =
                        World::with_catalog(source.collision.clone(), source.catalog.clone());
                    map_world.spawns = source.spawns.clone();
                    map_world.objectives = source.objectives.clone();
                    starting[n] = Some(Predictor::new(
                        map_world,
                        client.welcome().unwrap().clone(),
                        initial,
                    )?);
                }
            }
        }
        if starting.iter().all(Option::is_some) {
            break;
        }
    }
    let mut predictors: Vec<_> = starting
        .into_iter()
        .map(|p| p.ok_or("initial prediction state missing"))
        .collect::<Result<_, _>>()?;
    let mut snapshots = [0; 2];
    let mut acks = [0; 2];
    let mut maximum_replay = 0;
    for step in 1..=128 {
        // Delay transport polling for four ticks while changing movement and
        // jumping. The server repeats old input; prediction keeps local intent.
        let delayed = (25..=28).contains(&step);
        for (n, client) in clients.iter_mut().enumerate() {
            let rotation = glam::Quat::IDENTITY;
            let input = TickInput {
                forward_move: (if n == 0 { 450. } else { -450. }) * if delayed { -1. } else { 1. },
                buttons: if step == 25 {
                    movement::buttons::JUMP
                } else {
                    0
                },
                view_angles: glam::Vec3::new(0., n as f32 * 180., 0.),
                tracked: Some(sim::TrackedPose {
                    head: sim::Pose {
                        position: glam::Vec3::Z * 64.,
                        rotation,
                    },
                    left_hand: sim::Pose {
                        rotation,
                        ..Default::default()
                    },
                    right_hand: sim::Pose {
                        rotation,
                        ..Default::default()
                    },
                }),
                ..Default::default()
            };
            let view_tick = predictors[n].authoritative_tick();
            let sequence = client.send_input(input, view_tick)?;
            predictors[n].predict(InputCommand {
                sequence,
                view_tick,
                input,
            })?;
            if !delayed {
                client.update(dt)?;
            }
        }
        server.update(dt)?;
        history.insert(
            server.session().world().tick,
            server.session().world().state(),
        );
        for (n, client) in clients.iter_mut().enumerate() {
            if delayed {
                continue;
            }
            client.update(Duration::ZERO)?;
            if let Some(state) = client.take_state() {
                snapshots[n] += 1;
                acks[n] = state.acknowledged_input;
                maximum_replay =
                    maximum_replay.max(reconcile_checked(&mut predictors[n], state, &history)?);
            }
            while client.take_events().is_some() {}
        }
    }
    for _ in 0..32 {
        for client in &mut clients {
            client.update(dt)?;
        }
        server.update(dt)?;
        history.insert(
            server.session().world().tick,
            server.session().world().state(),
        );
        for (n, client) in clients.iter_mut().enumerate() {
            client.update(Duration::ZERO)?;
            if let Some(state) = client.take_state() {
                snapshots[n] += 1;
                acks[n] = state.acknowledged_input;
                maximum_replay =
                    maximum_replay.max(reconcile_checked(&mut predictors[n], state, &history)?);
            }
            while client.take_events().is_some() {}
        }
        if predictors
            .iter()
            .all(|p| p.pending_inputs() == 0 && p.acknowledged_input() == 128)
        {
            break;
        }
    }
    if maximum_replay == 0 {
        return Err("delayed IO did not exercise prediction replay".into());
    }
    for n in 0..2 {
        if snapshots[n] < 16 || acks[n] != 128 || predictors[n].pending_inputs() != 0 {
            return Err(format!(
                "insufficient snapshots/acknowledgements for client {n}: {} / {}",
                snapshots[n], acks[n]
            ));
        }
        if predictors[n].world().state() != *predictors[n].authoritative_state() {
            return Err("fully acknowledged prediction differs from authority".into());
        }
        if server
            .session()
            .world()
            .player(ids[n])
            .is_none_or(|p| p.tracked.is_none())
        {
            return Err("tracked poses did not reach authority".into());
        }
    }
    for client in &mut clients {
        client.disconnect();
    }
    for _ in 0..8 {
        server.update(dt)?;
    }
    if ids
        .iter()
        .any(|id| server.session().world().player(*id).is_some())
    {
        return Err("disconnected players remain in simulation".into());
    }
    println!(
        "UDP netcheck: 2 clients, {} tick, snapshots {:?}, acknowledged {:?}, ContentId resolution and VR poses verified\nPrediction: delayed IO, up to {} replayed inputs, no replayed effects, fully reconciled\nfailures: 0",
        server.session().world().tick_rate,
        snapshots,
        acks,
        maximum_replay
    );
    Ok(())
}

fn reconcile_checked(
    predictor: &mut Predictor,
    state: StateUpdate,
    history: &std::collections::BTreeMap<u64, sim::WorldState>,
) -> Result<usize, String> {
    if history.get(&state.snapshot.state.tick) != Some(&state.snapshot.state) {
        return Err(format!(
            "decoded content snapshot differs from authority at tick {}",
            state.snapshot.state.tick
        ));
    }
    let result = predictor.reconcile(state)?;
    if !predictor.world().events.is_empty() {
        return Err("prediction replay exposed gameplay effects".into());
    }
    Ok(match result {
        Reconciliation::Corrected {
            replayed_inputs, ..
        } => replayed_inputs,
        Reconciliation::IgnoredStale => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_server_options() {
        assert_eq!(options(&[]).unwrap().tick_rate, 128);
        assert!(options(&["--tickrate".into(), "0".into()]).is_err());
        assert!(options(&["--bind".into(), "no".into()]).is_err());
        assert!(options(&["--bots".into(), "64".into()]).is_err());
        assert!(options(&["--unknown".into()]).is_err());
    }
}

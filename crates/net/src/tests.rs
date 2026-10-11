//! A real server and two scripted clients over localhost UDP.

use crate::client::{predicted_kind, Connection, Incoming, Predictor};
use crate::protocol::{ClientMessage, ServerMessage, GAME_VERSION};
use crate::server::{Server, ServerConfig};
use collision::{contents, CollisionWorld};
use glam::Vec3;
use sim::{GameEvent, GameMode, Loadout, PlayerId, SpawnPoint, Team, TickInput, World};
use std::sync::Arc;
use std::time::{Duration, Instant};
use weapons::{Catalog, Side, WeaponClass, WeaponDefinition};

fn def(name: &str, index: u32, class: WeaponClass, side: Side) -> WeaponDefinition {
    WeaponDefinition {
        id: content::ContentId::csgo(content::ContentKind::Weapon, name.trim_start_matches("weapon_")),
        class_name: name.into(),
        def_index: index,
        name_token: String::new(),
        class,
        side,
        price: 2700,
        damage: 36.0,
        armor_ratio: 1.55,
        penetration: 2.0,
        range: 8192.0,
        range_modifier: 0.98,
        cycle_time: 0.1,
        full_auto: true,
        bullets: 1,
        clip_size: if class == WeaponClass::Knife { 0 } else { 30 },
        reserve_ammo: 90,
        max_player_speed: 215.0,
        accuracy: Default::default(),
        recoil: Default::default(),
        view_model: String::new(),
        world_model: String::new(),
        fire_sound: String::new(),
        zoom_in_sound: String::new(),
        zoom_out_sound: String::new(),
        kill_award: 300,
        attributes: Default::default(),
    }
}

/// A flat floor with T spawns at x = -400 and CT spawns at x = 400, in
/// deathmatch. Server and clients build it the same way, as they would
/// from the same map.
fn arena() -> World {
    let mut c = CollisionWorld::new();
    let s = 4096.0;
    let floor = c.surface_id("floor");
    c.add_triangle([Vec3::new(-s, -s, 0.0), Vec3::new(s, -s, 0.0), Vec3::new(s, s, 0.0)], contents::SOLID, floor);
    c.add_triangle([Vec3::new(-s, -s, 0.0), Vec3::new(s, s, 0.0), Vec3::new(-s, s, 0.0)], contents::SOLID, floor);
    c.finalize();
    let catalog = Catalog {
        weapons: vec![
            def("weapon_ak47", 7, WeaponClass::Rifle, Side::Terrorist),
            def("weapon_glock", 4, WeaponClass::Pistol, Side::Terrorist),
            def("weapon_knife_t", 59, WeaponClass::Knife, Side::Terrorist),
            def("weapon_m4a1_silencer", 60, WeaponClass::Rifle, Side::CounterTerrorist),
            def("weapon_usp_silencer", 61, WeaponClass::Pistol, Side::CounterTerrorist),
            def("weapon_knife", 42, WeaponClass::Knife, Side::CounterTerrorist),
        ],
    };
    let mut w = World::with_catalog(Arc::new(c), Arc::new(catalog));
    for (x, team) in [(-400.0, Team::Terrorist), (400.0, Team::CounterTerrorist)] {
        for k in 0..3 {
            w.spawns.push(SpawnPoint { origin: Vec3::new(x, k as f32 * 64.0, 1.0), yaw: 0.0, team });
        }
    }
    w.set_mode(GameMode::Deathmatch, Default::default());
    w
}

/// A scripted player: a connection, a predictor and its own world.
struct Bot {
    conn: Connection,
    world: World,
    predictor: Option<Predictor>,
    events: Vec<GameEvent>,
    predicted_events: Vec<GameEvent>,
    script: fn(&Bot, u64) -> TickInput,
    ticks: u64,
}

impl Bot {
    fn join(port: u16, name: &str, team: Team, script: fn(&Bot, u64) -> TickInput) -> Self {
        let hello = ClientMessage::Hello { name: name.into(), version: GAME_VERSION.into(), team: Some(team), loadout: Loadout::default() };
        let conn = Connection::connect(format!("127.0.0.1:{port}").parse().unwrap(), hello).unwrap();
        Self { conn, world: arena(), predictor: None, events: Vec::new(), predicted_events: Vec::new(), script, ticks: 0 }
    }

    fn me(&self) -> Option<PlayerId> {
        self.predictor.as_ref().map(|p| p.local)
    }

    /// One client tick.
    fn tick(&mut self) {
        for msg in self.conn.poll() {
            match msg {
                Incoming::Message(ServerMessage::Welcome { player, tick_rate, .. }) => {
                    self.world.set_tick_rate(tick_rate);
                    self.predictor = Some(Predictor::new(player, tick_rate));
                }
                Incoming::Message(ServerMessage::Events { events, .. }) => {
                    let me = self.me();
                    self.events.extend(events.into_iter().filter(|e| me.is_none() || predicted_kind(e) != me));
                }
                Incoming::Snapshot(s) => {
                    if let Some(p) = &mut self.predictor {
                        p.receive(&self.world, s);
                    }
                }
                Incoming::Message(other) => panic!("unexpected {other:?}"),
                Incoming::Disconnected(why) => panic!("{why}"),
            }
        }
        if self.predictor.as_ref().is_none_or(|p| p.latest().is_none()) {
            return;
        }
        let input = (self.script)(self, self.ticks);
        self.ticks += 1;
        let predictor = self.predictor.as_mut().unwrap();
        let packet = predictor.push_input(input);
        self.conn.send_input(&packet);
        let events = predictor.predict(&mut self.world);
        self.predicted_events.extend(events);
    }

    fn other(&self) -> Option<&sim::Player> {
        self.world.players.iter().find(|p| Some(p.id) != self.me())
    }
}

fn walk_in_circles(_: &Bot, n: u64) -> TickInput {
    TickInput { view_angles: Vec3::new(0.0, (n % 360) as f32, 0.0), forward_move: 450.0, ..Default::default() }
}

/// Stands still for two seconds, then faces the other player and fires.
fn hunt(bot: &Bot, n: u64) -> TickInput {
    let Some((me, target)) = bot.me().and_then(|id| bot.world.player(id)).zip(bot.other()) else { return TickInput::default() };
    if n < 256 {
        return TickInput { side_move: if n < 128 { 450.0 } else { 0.0 }, ..Default::default() };
    }
    let (pitch, yaw) = sim::bots::look_angles(me.eye(), target.movement.origin + Vec3::Z * 40.0);
    TickInput { view_angles: Vec3::new(pitch, yaw, 0.0), buttons: movement::buttons::ATTACK, ..Default::default() }
}

#[test]
fn server_and_two_clients_agree_and_a_kill_registers() {
    let mut server = Server::bind(arena(), ServerConfig { port: 0, map: "arena".into(), log: false, ..Default::default() }).unwrap();
    let port = server.local_addr().port();
    let mut a = Bot::join(port, "hunter", Team::Terrorist, hunt);
    let mut b = Bot::join(port, "walker", Team::CounterTerrorist, walk_in_circles);
    let tick = Duration::from_secs_f64(1.0 / 128.0);
    let start = Instant::now();
    let mut next = start;
    let mut b_seen_moving_by_a = Vec::new();
    let mut killed = false;
    while start.elapsed() < Duration::from_secs(30) {
        server.tick();
        a.tick();
        b.tick();
        if let Some(p) = a.other() {
            b_seen_moving_by_a.push(p.movement.origin);
        }
        killed = a.events.iter().any(|e| matches!(e, GameEvent::Kill { attacker, victim, .. } if Some(*attacker) == a.me() && Some(*victim) == b.me()));
        if killed && b.events.iter().any(|e| matches!(e, GameEvent::Kill { .. })) {
            break;
        }
        next += tick;
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
    assert_eq!(server.players(), 2, "both clients joined");
    assert!(killed, "the hunter's kill reached the hunter");
    assert!(b.events.iter().any(|e| matches!(e, GameEvent::Kill { victim, .. } if Some(*victim) == b.me())), "and the victim");
    assert!(a.predicted_events.iter().any(|e| matches!(e, GameEvent::Fire { .. })), "shots were predicted locally");
    assert!(!a.events.iter().any(|e| predicted_kind(e) == a.me()), "the server's copies of predicted events are skipped");
    // The hunter saw the walker move (interpolated, so in small steps).
    let travelled: f32 = b_seen_moving_by_a.windows(2).map(|w| w[0].distance(w[1])).sum();
    assert!(travelled > 200.0, "the walker was seen moving {travelled} units");
    let jumps = b_seen_moving_by_a.windows(2).filter(|w| w[0].distance(w[1]) > 16.0).count();
    assert!(jumps <= 3, "smoothly, apart from respawns ({jumps} jumps)");

    // Every client holds exactly the bytes the server sent for a tick.
    let pa = a.predictor.as_ref().unwrap();
    let pb = b.predictor.as_ref().unwrap();
    let t = pa.latest_tick().min(pb.latest_tick());
    let hash = |p: &Predictor| p.hash_at(t);
    assert!(server.hash_at(t).is_some());
    assert_eq!(hash(pa), server.hash_at(t));
    assert_eq!(hash(pb), server.hash_at(t));
    assert_eq!(pa.dropped + pb.dropped, 0, "no snapshot failed to decode");
}

#[test]
fn prediction_converges_on_the_server_state() {
    let mut server = Server::bind(arena(), ServerConfig { port: 0, map: "arena".into(), log: false, ..Default::default() }).unwrap();
    let port = server.local_addr().port();
    // Run for a second, then stop.
    fn run_then_stop(_: &Bot, n: u64) -> TickInput {
        if n < 128 { TickInput { forward_move: 450.0, side_move: 200.0, view_angles: Vec3::new(0.0, 30.0, 0.0), ..Default::default() } } else { TickInput::default() }
    }
    let mut a = Bot::join(port, "runner", Team::Terrorist, run_then_stop);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) && a.ticks < 400 {
        server.tick();
        a.tick();
        std::thread::sleep(Duration::from_secs_f64(1.0 / 128.0));
    }
    let me = a.me().unwrap();
    let predicted = a.world.player(me).unwrap().movement.origin;
    let actual = server.world.player(me).unwrap().movement.origin;
    assert!(predicted.distance(actual) < 0.01, "predicted {predicted} vs server {actual}");
    let start_pos = Vec3::new(-400.0, 0.0, 1.0);
    assert!(actual.distance(start_pos) > 100.0, "the runner moved");
}

#[test]
fn refuses_other_versions() {
    let mut server = Server::bind(arena(), ServerConfig { port: 0, map: "arena".into(), log: false, ..Default::default() }).unwrap();
    let port = server.local_addr().port();
    let hello = ClientMessage::Hello { name: "old".into(), version: "0.0.1".into(), team: None, loadout: Loadout::default() };
    let conn = Connection::connect(format!("127.0.0.1:{port}").parse().unwrap(), hello).unwrap();
    let start = Instant::now();
    let mut refused = None;
    while start.elapsed() < Duration::from_secs(10) && refused.is_none() {
        server.tick();
        for msg in conn.poll() {
            if let Incoming::Message(ServerMessage::Refused(why)) = msg {
                refused = Some(why);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(refused.is_some_and(|w| w.contains(GAME_VERSION)));
    assert_eq!(server.players(), 0);
}

#[test]
fn humans_replace_bots_and_bots_return() {
    let mut world = arena();
    world.add_bot("bot t", Team::Terrorist);
    world.add_bot("bot ct", Team::CounterTerrorist);
    let mut server = Server::bind(world, ServerConfig { port: 0, map: "arena".into(), log: false, ..Default::default() }).unwrap();
    let port = server.local_addr().port();
    let a = Bot::join(port, "human", Team::CounterTerrorist, walk_in_circles);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) && server.players() == 0 {
        server.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
    let ct: Vec<_> = server.world.players.iter().filter(|p| p.team == Team::CounterTerrorist).map(|p| p.is_bot).collect();
    assert_eq!(ct, vec![false], "the human took the CT bot's place");
    drop(a);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) && server.players() == 1 {
        server.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
    let ct: Vec<_> = server.world.players.iter().filter(|p| p.team == Team::CounterTerrorist).map(|p| p.is_bot).collect();
    assert_eq!(ct, vec![true], "a bot came back");
}

#[test]
fn addresses_resolve_with_the_default_port() {
    use crate::client::resolve;
    assert_eq!(resolve("127.0.0.1").unwrap().port(), 27015);
    assert_eq!(resolve("127.0.0.1:27020").unwrap().port(), 27020);
    assert_eq!(resolve("localhost:5").unwrap().port(), 5);
    assert!(resolve("not a host name").is_err());
}

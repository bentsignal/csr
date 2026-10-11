//! The authoritative server: owns the [`sim::World`], steps it at the tick
//! rate with every client's input, and sends each client the new state.
//!
//! Per tick:
//! 1. Receive: connections, hellos and loadouts (reliable), input packets
//!    (unreliable; new inputs join that client's queue).
//! 2. Take one input from each client's queue (a client that has none
//!    repeats its last one, as `World::step` does) and step the world. The
//!    bots think inside the step.
//! 3. Send the tick's events to everyone (reliable) and each client a
//!    snapshot delta-coded against the newest one it acknowledged.
//!
//! Humans take bots' places: joining removes a bot from the team, leaving
//! brings one back, so teams keep the size the server started with.

use crate::delta;
use crate::protocol::{self, ClientMessage, InputPacket, ServerMessage, SnapshotPacket, RELIABLE, UNRELIABLE};
use renet::{ClientId, RenetServer, ServerEvent};
use renet_netcode::{NetcodeServerTransport, ServerAuthentication};
use sim::{GameEvent, PlayerId, Team, TickInput, World};
use std::collections::{BTreeMap, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// Inputs a client may have queued; older ones beyond this are dropped so
/// a client whose clock ran ahead cannot build up latency.
const MAX_QUEUE: usize = 16;
/// Seconds of encoded snapshots kept as delta baselines.
const HISTORY_SECONDS: f32 = 2.0;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Shown to players.
    pub name: String,
    /// The map the world was loaded from (sent to joining clients).
    pub map: String,
    /// UDP port; 0 picks a free one (tests).
    pub port: u16,
    pub max_players: usize,
    /// Print joins, kills, rounds and traffic.
    pub log: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { name: "CSGO Rewrite server".into(), map: String::new(), port: protocol::DEFAULT_PORT, max_players: 32, log: true }
    }
}

struct Remote {
    name: String,
    player: Option<PlayerId>,
    queue: VecDeque<(u32, TickInput)>,
    /// Newest input sequence received / applied.
    received: u32,
    applied: u32,
    ack_snapshot: u64,
}

#[derive(Default)]
struct Traffic {
    snapshots: u64,
    snapshot_bytes: u64,
    full: u64,
    since: Option<Instant>,
}

pub struct Server {
    pub world: World,
    config: ServerConfig,
    renet: RenetServer,
    transport: NetcodeServerTransport,
    remotes: BTreeMap<ClientId, Remote>,
    /// Encoded states of recent ticks, oldest first.
    history: VecDeque<(u64, Arc<Vec<u8>>)>,
    /// Players per team the server started with (T, CT); bots fill the gaps.
    team_size: [usize; 2],
    last_update: Instant,
    traffic: Traffic,
}

fn now_since_epoch() -> Duration {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default()
}

fn team_index(team: Team) -> usize {
    if team == Team::Terrorist { 0 } else { 1 }
}

impl Server {
    /// Starts listening. `world` is ready to play: map loaded, mode set,
    /// bots added (they define the team sizes humans fill).
    pub fn bind(world: World, config: ServerConfig) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", config.port))?;
        let addr = socket.local_addr()?;
        let netcode = renet_netcode::ServerConfig {
            current_time: now_since_epoch(),
            max_clients: config.max_players.clamp(1, 64),
            protocol_id: protocol::PROTOCOL_ID,
            public_addresses: vec![addr],
            authentication: ServerAuthentication::Unsecure,
        };
        let transport = NetcodeServerTransport::new(netcode, socket)?;
        let count = |team| world.players.iter().filter(|p| p.team == team).count();
        let team_size = [count(Team::Terrorist), count(Team::CounterTerrorist)];
        Ok(Self {
            world,
            config,
            renet: RenetServer::new(protocol::connection_config()),
            transport,
            remotes: BTreeMap::new(),
            history: VecDeque::new(),
            team_size,
            last_update: Instant::now(),
            traffic: Traffic::default(),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.transport.addresses()[0]
    }

    /// Humans connected and in the game.
    pub fn players(&self) -> usize {
        self.remotes.values().filter(|r| r.player.is_some()).count()
    }

    /// FNV hash of the encoded state the server sent for `tick`, while it
    /// is still in the history (tests compare it with clients').
    pub fn hash_at(&self, tick: u64) -> Option<u64> {
        self.history.iter().find(|(t, _)| *t == tick).map(|(_, b)| sim::state::fnv1a(b))
    }

    fn log(&self, line: std::fmt::Arguments) {
        if self.config.log {
            println!("[server {:>8.2}s] {line}", self.world.time());
        }
    }

    /// Runs ticks at the world's tick rate until `stop` is set.
    pub fn run(&mut self, stop: &AtomicBool) {
        let start = Instant::now();
        let first_tick = self.world.tick;
        self.log(format_args!(
            "'{}' on {} ({}), {} tick, port {}",
            self.config.name,
            self.config.map,
            format!("{:?}", self.world.mode).to_lowercase(),
            self.world.tick_rate,
            self.local_addr().port()
        ));
        while !stop.load(Ordering::Relaxed) {
            self.tick();
            // Schedule from the start, so sleep error never accumulates.
            let next = start + Duration::from_secs_f64((self.world.tick - first_tick) as f64 / self.world.tick_rate as f64);
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else if now - next > Duration::from_secs(1) {
                self.log(format_args!("running {:.1}s behind; skipping ahead", (now - next).as_secs_f32()));
            }
        }
        self.transport.disconnect_all(&mut self.renet);
    }

    /// One server tick: receive, step, send.
    pub fn tick(&mut self) {
        let now = Instant::now();
        let dt = now - self.last_update;
        self.last_update = now;
        self.renet.update(dt);
        if let Err(e) = self.transport.update(dt, &mut self.renet) {
            self.log(format_args!("network: {e}"));
        }
        self.receive();

        let mut inputs = Vec::new();
        for r in self.remotes.values_mut() {
            let Some(id) = r.player else { continue };
            while r.queue.len() > MAX_QUEUE {
                r.queue.pop_front();
            }
            if let Some((seq, input)) = r.queue.pop_front() {
                r.applied = seq;
                inputs.push((id, input));
            }
        }
        self.world.step(&inputs);
        self.log_events();

        let state = Arc::new(self.world.encode_state());
        let tick = self.world.tick;
        let keep = self.world.ticks(HISTORY_SECONDS) as usize;
        while self.history.len() >= keep.max(2) {
            self.history.pop_front();
        }
        self.history.push_back((tick, state.clone()));

        let events = (!self.world.events.is_empty())
            .then(|| protocol::encode(&ServerMessage::Events { tick, events: self.world.events.clone() }));
        let ids: Vec<ClientId> = self.remotes.keys().copied().collect();
        for id in ids {
            let r = &self.remotes[&id];
            if r.player.is_none() {
                continue;
            }
            if let Some(events) = &events {
                self.renet.send_message(id, RELIABLE, events.clone());
            }
            let base = self.history.iter().find(|(t, _)| *t == r.ack_snapshot && *t != 0);
            let (baseline, data) = match base {
                Some((t, bytes)) => (*t, delta::encode(bytes, &state)),
                None => (0, state.to_vec()),
            };
            let packet = SnapshotPacket { tick, baseline, ack_input: r.applied, queued: r.queue.len().min(255) as u8, data };
            let bytes = protocol::encode(&packet);
            self.traffic.snapshots += 1;
            self.traffic.snapshot_bytes += bytes.len() as u64;
            self.traffic.full += (baseline == 0) as u64;
            self.renet.send_message(id, UNRELIABLE, bytes);
        }
        self.transport.send_packets(&mut self.renet);
        self.report_traffic();
    }

    fn receive(&mut self) {
        while let Some(event) = self.renet.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    self.remotes.insert(client_id, Remote { name: String::new(), player: None, queue: VecDeque::new(), received: 0, applied: 0, ack_snapshot: 0 });
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    if let Some(r) = self.remotes.remove(&client_id) {
                        if let Some(id) = r.player {
                            let team = self.world.player(id).map(|p| p.team);
                            self.world.remove_player(id);
                            if let Some(team) = team {
                                self.refill_bots(team);
                            }
                            self.log(format_args!("{} left ({reason})", r.name));
                        }
                    }
                }
            }
        }
        let ids: Vec<ClientId> = self.remotes.keys().copied().collect();
        for id in ids {
            while let Some(bytes) = self.renet.receive_message(id, RELIABLE) {
                match protocol::decode::<ClientMessage>(&bytes) {
                    Some(msg) => self.handle(id, msg),
                    None => self.renet.disconnect(id),
                }
            }
            while let Some(bytes) = self.renet.receive_message(id, UNRELIABLE) {
                let Some(packet) = self.world.decode::<InputPacket>(&bytes).ok() else { continue };
                let Some(r) = self.remotes.get_mut(&id) else { continue };
                r.ack_snapshot = r.ack_snapshot.max(packet.ack_snapshot);
                let n = packet.inputs.len() as u32;
                for (k, input) in packet.inputs.into_iter().enumerate() {
                    let seq = packet.last_seq.wrapping_sub(n - 1 - k as u32);
                    if seq > r.received {
                        r.received = seq;
                        r.queue.push_back((seq, input));
                    }
                }
            }
        }
    }

    fn handle(&mut self, client: ClientId, msg: ClientMessage) {
        match msg {
            ClientMessage::Hello { name, version, team, loadout } => {
                if self.remotes.get(&client).is_some_and(|r| r.player.is_some()) {
                    return;
                }
                let name = protocol::clean_name(&name);
                if version != protocol::GAME_VERSION {
                    let why = format!("This server runs version {}; you have {version}. Update the game to join.", protocol::GAME_VERSION);
                    self.renet.send_message(client, RELIABLE, protocol::encode(&ServerMessage::Refused(why)));
                    self.log(format_args!("refused {name}: version {version}"));
                    return;
                }
                let team = team.filter(|t| *t != Team::Spectator).unwrap_or_else(|| self.emptier_team());
                // A human takes a bot's place.
                if let Some(bot) = self.world.players.iter().rev().find(|p| p.is_bot && p.team == team).map(|p| p.id) {
                    self.world.remove_player(bot);
                } else {
                    self.team_size[team_index(team)] += 1;
                }
                let id = self.world.add_player(&name, team);
                self.world.set_loadout(id, loadout);
                if self.world.mode == sim::GameMode::Deathmatch {
                    // Practice weapons apply on spawning.
                    self.world.respawn(id);
                }
                let welcome = ServerMessage::Welcome {
                    player: id,
                    map: self.config.map.clone(),
                    tick_rate: self.world.tick_rate,
                    mode: self.world.mode,
                    server: self.config.name.clone(),
                };
                self.renet.send_message(client, RELIABLE, protocol::encode(&welcome));
                if let Some(r) = self.remotes.get_mut(&client) {
                    r.name = name.clone();
                    r.player = Some(id);
                }
                self.log(format_args!("{name} joined {team:?} as player {id} ({} humans)", self.players()));
            }
            ClientMessage::SetLoadout(loadout) => {
                if let Some(id) = self.remotes.get(&client).and_then(|r| r.player) {
                    self.world.set_loadout(id, loadout);
                }
            }
            ClientMessage::Chat(text) => {
                let Some(r) = self.remotes.get(&client).filter(|r| r.player.is_some()) else { return };
                let text: String = text.chars().filter(|c| !c.is_control()).take(200).collect();
                self.log(format_args!("{}: {text}", r.name));
                let msg = protocol::encode(&ServerMessage::Chat { from: r.name.clone(), text });
                for id in self.remotes.keys() {
                    self.renet.send_message(*id, RELIABLE, msg.clone());
                }
            }
        }
    }

    /// The team with fewer humans (then fewer players).
    fn emptier_team(&self) -> Team {
        let humans = |team| self.world.players.iter().filter(|p| p.team == team && !p.is_bot).count();
        let all = |team| self.world.players.iter().filter(|p| p.team == team).count();
        let (t, ct) = (Team::Terrorist, Team::CounterTerrorist);
        match humans(t).cmp(&humans(ct)).then(all(t).cmp(&all(ct))) {
            std::cmp::Ordering::Greater => ct,
            _ => t,
        }
    }

    /// Brings a team back up to its size with bots.
    fn refill_bots(&mut self, team: Team) {
        let have = self.world.players.iter().filter(|p| p.team == team).count();
        let names = if team == Team::Terrorist { &sim::bots::NAMES_T } else { &sim::bots::NAMES_CT };
        for _ in have..self.team_size[team_index(team)] {
            let name = names.iter().find(|n| !self.world.players.iter().any(|p| p.name == **n)).copied().unwrap_or("Bot");
            self.world.add_bot(name, team);
        }
    }

    fn log_events(&self) {
        if !self.config.log {
            return;
        }
        let name = |id: PlayerId| self.world.player(id).map_or_else(|| format!("#{id}"), |p| p.name.clone());
        for e in &self.world.events {
            match e {
                GameEvent::Kill { attacker, victim, weapon, headshot } => self.log(format_args!(
                    "kill: {} killed {} with {}{}",
                    if *attacker == sim::WORLD { "the world".to_string() } else { name(*attacker) },
                    name(*victim),
                    weapon.trim_start_matches("weapon_"),
                    if *headshot { " (headshot)" } else { "" }
                )),
                GameEvent::RoundEnd { reason } => self.log(format_args!(
                    "round {} over: {} (T {} - CT {})",
                    self.world.round.round,
                    reason.message(),
                    self.world.round.score_t,
                    self.world.round.score_ct
                )),
                GameEvent::BombPlanted { player, site, .. } => self.log(format_args!("{} planted the bomb at {site}", name(*player))),
                GameEvent::BombDefused { player } => self.log(format_args!("{} defused the bomb", name(*player))),
                _ => {}
            }
        }
    }

    fn report_traffic(&mut self) {
        let since = *self.traffic.since.get_or_insert_with(Instant::now);
        if since.elapsed() < Duration::from_secs(30) || self.traffic.snapshots == 0 {
            return;
        }
        let secs = since.elapsed().as_secs_f64();
        let per_client: Vec<String> = self
            .remotes
            .iter()
            .filter(|(_, r)| r.player.is_some())
            .map(|(id, r)| format!("{} rtt {:.0} ms loss {:.1}% queue {}", r.name, self.renet.rtt(*id), self.renet.packet_loss(*id) * 100.0, r.queue.len()))
            .collect();
        self.log(format_args!(
            "traffic: {} snapshots, {:.0} bytes avg ({} full), {:.1} KB/s per client; {}",
            self.traffic.snapshots,
            self.traffic.snapshot_bytes as f64 / self.traffic.snapshots as f64,
            self.traffic.full,
            self.traffic.snapshot_bytes as f64 / secs / 1024.0 / self.players().max(1) as f64,
            per_client.join(", ")
        ));
        self.traffic = Traffic { since: Some(Instant::now()), ..Default::default() };
    }
}

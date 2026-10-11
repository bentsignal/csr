//! The client end: a [`Connection`] that keeps the socket alive on its own
//! thread, and a [`Predictor`] that turns snapshots and local inputs into
//! the world the player sees.
//!
//! Each client tick:
//! 1. [`Predictor::receive`] every snapshot that arrived (decoding deltas
//!    against stored baselines).
//! 2. The local input for this tick gets a sequence number and the tick the
//!    player is seeing others at ([`Predictor::push_input`]); the packet with
//!    it and the previous few inputs goes to the server.
//! 3. [`Predictor::predict`] rebuilds the world: the newest server state;
//!    other players replaced by their state [`Predictor::interp_ticks`] in the
//!    past, blended between snapshots (entity interpolation); then the local
//!    player re-simulated through every input the server has not applied
//!    yet with `World::predict` (prediction and reconciliation).

use crate::delta;
use crate::protocol::{self, ClientMessage, InputPacket, ServerMessage, SnapshotPacket, RELIABLE, UNRELIABLE};
use renet::RenetClient;
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};
use sim::{GameEvent, Player, PlayerId, TickInput, World, WorldState};
use std::collections::VecDeque;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Something from the server.
#[derive(Debug)]
pub enum Incoming {
    Message(ServerMessage),
    Snapshot(SnapshotPacket),
    /// The connection ended; the reason, for the player.
    Disconnected(String),
}

enum Outgoing {
    Reliable(Vec<u8>),
    Unreliable(Vec<u8>),
}

/// Round trip and loss as renet measures them.
#[derive(Clone, Copy, Debug, Default)]
pub struct NetStats {
    pub rtt_ms: f64,
    pub loss: f64,
    pub sent_per_sec: f64,
    pub received_per_sec: f64,
}

/// A connection to a server. The socket is serviced every millisecond on a
/// background thread, so loading a map or a slow frame never times it out.
pub struct Connection {
    pub server: SocketAddr,
    to_net: Sender<Outgoing>,
    from_net: Receiver<Incoming>,
    stats: Arc<Mutex<NetStats>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// `host`, `host:port`, `ip` or `ip:port` (default port 27015).
pub fn resolve(address: &str) -> Result<SocketAddr, String> {
    let address = address.trim();
    let with_port = if address.parse::<SocketAddr>().is_ok() || address.rsplit_once(':').is_some_and(|(h, p)| !h.contains(':') && p.parse::<u16>().is_ok()) {
        address.to_string()
    } else {
        format!("{address}:{}", protocol::DEFAULT_PORT)
    };
    with_port
        .to_socket_addrs()
        .map_err(|e| format!("cannot find server {address}: {e}"))?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| format!("cannot find an IPv4 address for {address}"))
}

impl Connection {
    /// Starts connecting; `hello` is sent as soon as the connection is up.
    pub fn connect(server: SocketAddr, hello: ClientMessage) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
        // Any id unique among the server's clients will do.
        let client_id = now.as_nanos() as u64 ^ ((std::process::id() as u64) << 40) ^ socket.local_addr()?.port() as u64;
        let auth = ClientAuthentication::Unsecure { protocol_id: protocol::PROTOCOL_ID, client_id, server_addr: server, user_data: None };
        let transport = NetcodeClientTransport::new(now, auth, socket).map_err(|e| std::io::Error::other(e.to_string()))?;
        let (to_net, outgoing) = mpsc::channel();
        let (incoming, from_net) = mpsc::channel();
        let stats = Arc::new(Mutex::new(NetStats::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (stats, stop) = (stats.clone(), stop.clone());
            std::thread::Builder::new()
                .name("csr-net".into())
                .spawn(move || net_thread(transport, hello, outgoing, incoming, stats, stop))?
        };
        Ok(Self { server, to_net, from_net, stats, stop, thread: Some(thread) })
    }

    pub fn send(&self, msg: &ClientMessage) {
        let _ = self.to_net.send(Outgoing::Reliable(protocol::encode(msg)));
    }

    pub fn send_input(&self, packet: &InputPacket) {
        let _ = self.to_net.send(Outgoing::Unreliable(protocol::encode(packet)));
    }

    /// Everything received since the last call.
    pub fn poll(&self) -> Vec<Incoming> {
        self.from_net.try_iter().collect()
    }

    pub fn stats(&self) -> NetStats {
        *self.stats.lock().unwrap()
    }

    /// Waits for the server's welcome (or refusal) for up to `timeout`.
    /// Returns the welcome and anything that arrived with it.
    pub fn wait_for_welcome(&self, timeout: Duration) -> Result<(ServerMessage, Vec<Incoming>), String> {
        let until = Instant::now() + timeout;
        let mut early = Vec::new();
        while Instant::now() < until {
            match self.from_net.recv_timeout(Duration::from_millis(20)) {
                Ok(Incoming::Message(welcome @ ServerMessage::Welcome { .. })) => return Ok((welcome, early)),
                Ok(Incoming::Message(ServerMessage::Refused(why))) => return Err(why),
                Ok(Incoming::Disconnected(why)) => return Err(why),
                Ok(other) => early.push(other),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err("connection closed".into()),
            }
        }
        Err(format!("no answer from {} (is the server running, and UDP port {} open?)", self.server, self.server.port()))
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn net_thread(
    mut transport: NetcodeClientTransport,
    hello: ClientMessage,
    outgoing: Receiver<Outgoing>,
    incoming: Sender<Incoming>,
    stats: Arc<Mutex<NetStats>>,
    stop: Arc<AtomicBool>,
) {
    let mut client = RenetClient::new(protocol::connection_config());
    let mut hello = Some(protocol::encode(&hello));
    let mut last = Instant::now();
    let started = Instant::now();
    let reason = loop {
        if stop.load(Ordering::Relaxed) {
            transport.disconnect();
            return;
        }
        let now = Instant::now();
        let dt = now - last;
        last = now;
        client.update(dt);
        if let Err(e) = transport.update(dt, &mut client) {
            if client.is_disconnected() || transport.disconnect_reason().is_some() {
                break format!("disconnected: {e}");
            }
            // Windows reports an unreachable port as an error on the next
            // receive; keep retrying until netcode gives up.
            if !client.is_connected() && started.elapsed() > Duration::from_secs(8) {
                break format!("cannot reach the server: {e}");
            }
        }
        if client.is_connected() {
            if let Some(hello) = hello.take() {
                client.send_message(RELIABLE, hello);
            }
            for msg in outgoing.try_iter() {
                match msg {
                    Outgoing::Reliable(b) => client.send_message(RELIABLE, b),
                    Outgoing::Unreliable(b) => client.send_message(UNRELIABLE, b),
                }
            }
            while let Some(bytes) = client.receive_message(RELIABLE) {
                if let Some(msg) = protocol::decode::<ServerMessage>(&bytes) {
                    let _ = incoming.send(Incoming::Message(msg));
                }
            }
            while let Some(bytes) = client.receive_message(UNRELIABLE) {
                if let Some(packet) = protocol::decode::<SnapshotPacket>(&bytes) {
                    let _ = incoming.send(Incoming::Snapshot(packet));
                }
            }
            *stats.lock().unwrap() = NetStats {
                rtt_ms: client.rtt() * 1000.0,
                loss: client.packet_loss(),
                sent_per_sec: client.bytes_sent_per_sec(),
                received_per_sec: client.bytes_received_per_sec(),
            };
        } else {
            // Inputs queued while connecting are stale.
            for _ in outgoing.try_iter() {}
        }
        if client.is_disconnected() {
            break match client.disconnect_reason() {
                Some(r) => format!("disconnected: {r}"),
                None => "disconnected".into(),
            };
        }
        if let Err(e) = transport.send_packets(&mut client) {
            if transport.disconnect_reason().is_some() {
                break format!("disconnected: {e}");
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let _ = incoming.send(Incoming::Disconnected(reason));
}

/// Events the client plays from its own prediction for the local player,
/// so the server's copies of them are skipped.
pub fn predicted_kind(event: &GameEvent) -> Option<PlayerId> {
    match *event {
        GameEvent::Fire { player, .. }
        | GameEvent::Tracer { player, .. }
        | GameEvent::Impact { player, .. }
        | GameEvent::Melee { player, .. }
        | GameEvent::Scope { player, .. }
        | GameEvent::Reload { player }
        | GameEvent::Jump { player }
        | GameEvent::Land { player, .. } => Some(player),
        _ => None,
    }
}

struct Stored {
    tick: u64,
    bytes: Arc<Vec<u8>>,
    state: WorldState,
}

/// Prediction, reconciliation and interpolation for one local player.
pub struct Predictor {
    pub local: PlayerId,
    /// Decoded snapshots, oldest first (about two seconds).
    snapshots: VecDeque<Stored>,
    /// Inputs the server has not applied yet, oldest first.
    pending: VecDeque<(u32, TickInput)>,
    /// Recently sent inputs (resent for loss).
    recent: VecDeque<TickInput>,
    next_seq: u32,
    acked_input: u32,
    /// The (fractional) server tick other players are shown at.
    render_tick: f64,
    /// Smoothed server-side input queue length.
    queue: f64,
    tick_rate: u32,
    /// Snapshots that could not be decoded (missing baseline, bad data).
    pub dropped: u64,
    /// How far the newest prediction was from the server once it arrived
    /// (units), for diagnostics.
    pub last_error: f32,
    predicted: VecDeque<(u32, glam::Vec3)>,
}

impl Predictor {
    pub fn new(local: PlayerId, tick_rate: u32) -> Self {
        Self {
            local,
            snapshots: VecDeque::new(),
            pending: VecDeque::new(),
            recent: VecDeque::new(),
            next_seq: 1,
            acked_input: 0,
            render_tick: 0.0,
            queue: 2.0,
            tick_rate: tick_rate.max(1),
            dropped: 0,
            last_error: 0.0,
            predicted: VecDeque::new(),
        }
    }

    /// Ticks other players are drawn in the past: CS:GO's default
    /// `cl_interp` of 31 ms.
    pub fn interp_ticks(&self) -> f64 {
        (0.031 * self.tick_rate as f64).ceil().max(2.0)
    }

    /// The newest snapshot tick, 0 before the first.
    pub fn latest_tick(&self) -> u64 {
        self.snapshots.back().map_or(0, |s| s.tick)
    }

    pub fn latest(&self) -> Option<&WorldState> {
        self.snapshots.back().map(|s| &s.state)
    }

    /// FNV hash of the bytes received for `tick` (compare with
    /// `Server::hash_at`).
    pub fn hash_at(&self, tick: u64) -> Option<u64> {
        self.snapshots.iter().find(|s| s.tick == tick).map(|s| sim::state::fnv1a(&s.bytes))
    }

    /// The tick other players are being drawn at.
    pub fn render_tick(&self) -> f64 {
        self.render_tick
    }

    /// Stores a snapshot. `world` supplies the weapon catalog.
    pub fn receive(&mut self, world: &World, packet: SnapshotPacket) {
        if self.snapshots.iter().any(|s| s.tick == packet.tick) {
            return;
        }
        let bytes = if packet.baseline == 0 {
            Some(packet.data)
        } else {
            self.snapshots.iter().find(|s| s.tick == packet.baseline).and_then(|base| delta::decode(&base.bytes, &packet.data))
        };
        let Some(state) = bytes.as_ref().and_then(|b| world.decode::<WorldState>(b).ok()) else {
            self.dropped += 1;
            return;
        };
        if packet.tick >= self.latest_tick() {
            self.acked_input = self.acked_input.max(packet.ack_input);
            self.queue += (packet.queued as f64 - self.queue) * 0.05;
            // How far off the prediction of this input was.
            if let (Some(me), Some(&(_, guess))) = (
                state.players.iter().find(|p| p.id == self.local),
                self.predicted.iter().find(|(seq, _)| *seq == packet.ack_input),
            ) {
                self.last_error = me.movement.origin.distance(guess);
            }
        }
        let at = self.snapshots.partition_point(|s| s.tick < packet.tick);
        self.snapshots.insert(at, Stored { tick: packet.tick, bytes: Arc::new(bytes.unwrap()), state });
        let keep = 2 * self.tick_rate as usize;
        while self.snapshots.len() > keep {
            self.snapshots.pop_front();
        }
    }

    /// Records the input for this tick (adding the view tick for lag
    /// compensation) and returns the packet to send.
    pub fn push_input(&mut self, mut input: TickInput) -> InputPacket {
        input.view_tick = (self.render_tick > 0.0).then(|| self.render_tick.round() as u64);
        let seq = self.next_seq;
        self.next_seq += 1;
        self.pending.push_back((seq, input));
        // A server that stops answering must not grow this forever.
        while self.pending.len() > 4 * self.tick_rate as usize {
            self.pending.pop_front();
        }
        self.recent.push_back(input);
        while self.recent.len() > protocol::INPUT_REDUNDANCY {
            self.recent.pop_front();
        }
        InputPacket { ack_snapshot: self.latest_tick(), last_seq: seq, inputs: self.recent.iter().copied().collect() }
    }

    /// How much faster (> 1) or slower the client should tick so that its
    /// inputs reach the server just in time: a queue of about two.
    pub fn clock_scale(&self) -> f64 {
        if self.queue < 1.0 {
            1.02
        } else if self.queue > 4.0 {
            0.98
        } else {
            1.0
        }
    }

    /// Rebuilds `world` for this tick (see the module docs). Returns the
    /// local player's own events from the newest input, to play now.
    pub fn predict(&mut self, world: &mut World) -> Vec<GameEvent> {
        let Some(latest) = self.snapshots.back() else { return Vec::new() };
        let latest_tick = latest.tick;
        world.restore(latest.state.clone());
        world.events.clear();

        // Others: advance the render clock one tick, easing toward the
        // newest snapshot minus the interpolation delay.
        let target = latest_tick as f64 - self.interp_ticks();
        if self.render_tick == 0.0 || (target - self.render_tick).abs() > self.tick_rate as f64 * 0.25 {
            self.render_tick = target;
        } else {
            self.render_tick += 1.0 + (target - (self.render_tick + 1.0)) * 0.1;
        }
        self.render_tick = self.render_tick.min(latest_tick as f64).max(0.0);
        let local = self.local;
        for p in world.players.iter_mut().filter(|p| p.id != local) {
            if let Some(past) = self.sample(p.id, self.render_tick) {
                *p = past;
            }
        }

        // Me: the server's state plus every input it has not applied.
        let acked = self.acked_input;
        self.pending.retain(|(seq, _)| *seq > acked);
        let mut events = Vec::new();
        let n = self.pending.len();
        for (k, (_, input)) in self.pending.iter().enumerate() {
            world.predict(local, *input);
            if k + 1 == n {
                events = world.events.iter().filter(|e| predicted_kind(e) == Some(local)).cloned().collect();
            }
        }
        if let (Some(&(seq, _)), Some(me)) = (self.pending.back(), world.player(local)) {
            self.predicted.push_back((seq, me.movement.origin));
            while self.predicted.len() > 4 * self.tick_rate as usize {
                self.predicted.pop_front();
            }
        }
        events
    }

    /// A player as they were at fractional tick `t`, blended between the
    /// two snapshots around it.
    fn sample(&self, id: PlayerId, t: f64) -> Option<Player> {
        let after = self.snapshots.partition_point(|s| (s.tick as f64) < t);
        let find = |i: usize| self.snapshots.get(i).and_then(|s| s.state.players.iter().find(|p| p.id == id).map(|p| (s.tick, p)));
        let (b_tick, b) = find(after).or_else(|| find(after.saturating_sub(1)))?;
        let Some((a_tick, a)) = after.checked_sub(1).and_then(find) else { return Some(b.clone()) };
        if b_tick == a_tick || a.alive != b.alive || a.movement.origin.distance(b.movement.origin) > 64.0 {
            return Some(if (t - a_tick as f64) < (b_tick as f64 - t) { a.clone() } else { b.clone() });
        }
        let f = ((t - a_tick as f64) / (b_tick - a_tick) as f64).clamp(0.0, 1.0) as f32;
        let mut p = b.clone();
        p.movement.origin = a.movement.origin.lerp(b.movement.origin, f);
        p.movement.velocity = a.movement.velocity.lerp(b.movement.velocity, f);
        p.movement.duck_amount = a.movement.duck_amount + (b.movement.duck_amount - a.movement.duck_amount) * f;
        let angle = |x: f32, y: f32| x + ((y - x + 180.0).rem_euclid(360.0) - 180.0) * f;
        p.view_angles = glam::Vec3::new(angle(a.view_angles.x, b.view_angles.x), angle(a.view_angles.y, b.view_angles.y), 0.0);
        Some(p)
    }
}

//! Renet channels over netcode.io UDP. Direct-IP admission is deliberately
//! unauthenticated until the identity service supplies secure connect tokens.
use crate::{
    protocol::*,
    session::{Session, TickClock},
};
use renet::{RenetClient, RenetServer, ServerEvent};
use renet_netcode::{
    ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport, ServerAuthentication,
    ServerConfig,
};
use sim::{Team, TickInput};
use std::{
    collections::VecDeque,
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    time::{Duration, SystemTime},
};

fn now() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
}

pub struct Server {
    connection: RenetServer,
    transport: NetcodeServerTransport,
    session: Session,
    clock: TickClock,
    snapshot_every: u64,
    pending_hellos: std::collections::HashMap<u64, Duration>,
    elapsed: Duration,
}

impl Server {
    pub fn bind(address: SocketAddr, session: Session, max_clients: usize) -> Result<Self, String> {
        if max_clients == 0 || max_clients > 64 {
            return Err("max clients must be from 1 to 64".into());
        }
        let socket = UdpSocket::bind(address).map_err(|e| e.to_string())?;
        let actual = socket.local_addr().map_err(|e| e.to_string())?;
        let transport = NetcodeServerTransport::new(
            ServerConfig {
                current_time: now(),
                max_clients,
                protocol_id: PROTOCOL_ID,
                public_addresses: vec![actual],
                authentication: ServerAuthentication::Unsecure,
            },
            socket,
        )
        .map_err(|e| e.to_string())?;
        let rate = session.world().tick_rate;
        Ok(Self {
            connection: RenetServer::new(channels()),
            transport,
            session,
            clock: TickClock::new(rate)?,
            snapshot_every: (rate / 32).max(1) as u64,
            pending_hellos: Default::default(),
            elapsed: Duration::ZERO,
        })
    }
    pub fn address(&self) -> SocketAddr {
        self.transport.addresses()[0]
    }
    pub fn session(&self) -> &Session {
        &self.session
    }
    pub fn discarded_wall_ticks(&self) -> u64 {
        self.clock.discarded_wall_ticks
    }

    /// Drive IO once and advance the sim only for elapsed fixed ticks.
    pub fn update(&mut self, elapsed: Duration) -> Result<usize, String> {
        self.elapsed = self.elapsed.saturating_add(elapsed);
        self.connection.update(elapsed);
        self.transport
            .update(elapsed, &mut self.connection)
            .map_err(|e| e.to_string())?;
        while let Some(event) = self.connection.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    self.pending_hellos.insert(client_id, self.elapsed);
                }
                ServerEvent::ClientDisconnected { client_id, .. } => {
                    self.session.leave(client_id);
                    self.pending_hellos.remove(&client_id);
                }
            }
        }
        for client in self.connection.clients_id() {
            // Bound decoded work per peer per update, even if it floods.
            for _ in 0..16 {
                let Some(bytes) = self.connection.receive_message(client, CONTROL) else {
                    break;
                };
                match decode::<ClientControl>(&bytes) {
                    Ok(ClientControl::Leave) => {
                        self.session.leave(client);
                        self.connection.disconnect(client);
                        break;
                    }
                    Ok(hello) => {
                        let reply = self.session.join(client, hello);
                        if matches!(reply, ServerControl::Welcome { .. }) {
                            self.pending_hellos.remove(&client);
                        }
                        self.connection
                            .send_message(client, CONTROL, encode(&reply)?);
                    }
                    Err(_) => {
                        self.connection.disconnect(client);
                        break;
                    }
                }
            }
            for _ in 0..32 {
                let Some(bytes) = self.connection.receive_message(client, INPUT) else {
                    break;
                };
                if decode::<InputBatch>(&bytes)
                    .and_then(|batch| self.session.input(client, batch))
                    .is_err()
                {
                    self.connection.disconnect(client);
                    break;
                }
            }
            if self
                .pending_hellos
                .get(&client)
                .is_some_and(|at| self.elapsed.saturating_sub(*at) > Duration::from_secs(5))
            {
                self.connection.disconnect(client);
            }
        }
        let ticks = self.clock.advance(elapsed);
        for _ in 0..ticks {
            let events = self.session.step();
            for client in self.connection.clients_id() {
                if self.session.player_for(client).is_none() {
                    continue;
                }
                if !events.events.is_empty() {
                    self.connection
                        .send_message(client, EVENTS, encode(&events)?);
                }
            }
        }
        // Send one latest state per IO update; never enqueue stale catch-up states.
        if ticks > 0 && self.session.world().tick % self.snapshot_every < ticks as u64 {
            for client in self.connection.clients_id() {
                if let Some(state) = self.session.snapshot_for(client) {
                    let bytes = encode(&state)?;
                    if self
                        .connection
                        .can_send_message(client, SNAPSHOT, bytes.len())
                    {
                        self.connection.send_message(client, SNAPSHOT, bytes);
                    }
                }
            }
        }
        self.transport.send_packets(&mut self.connection);
        Ok(ticks)
    }
    pub fn shutdown(&mut self) {
        self.transport.disconnect_all(&mut self.connection);
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub struct Client {
    connection: RenetClient,
    transport: NetcodeClientTransport,
    catalog: Arc<weapons::Catalog>,
    content: ContentManifest,
    hello: ClientControl,
    sent_hello: bool,
    welcome: Option<ServerControl>,
    pending: VecDeque<InputCommand>,
    sequence: u64,
    latest_tick: Option<u64>,
    latest: Option<StateUpdate>,
    events: VecDeque<EventBatch>,
    failure: Option<String>,
}

impl Client {
    pub fn connect(
        server: SocketAddr,
        name: String,
        team: Team,
        content: ContentManifest,
        catalog: Arc<weapons::Catalog>,
    ) -> Result<Self, String> {
        let bind = if server.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let socket = UdpSocket::bind(bind).map_err(|e| e.to_string())?;
        let client_id = u64::from_le_bytes(renet_netcode::generate_random_bytes());
        let transport = NetcodeClientTransport::new(
            now(),
            ClientAuthentication::Unsecure {
                client_id,
                protocol_id: PROTOCOL_ID,
                server_addr: server,
                user_data: None,
            },
            socket,
        )
        .map_err(|e| e.to_string())?;
        let hello = ClientControl::Hello {
            version: VERSION,
            name,
            team,
            content: content.clone(),
        };
        Ok(Self {
            connection: RenetClient::new(channels()),
            transport,
            catalog,
            content,
            hello,
            sent_hello: false,
            welcome: None,
            pending: VecDeque::new(),
            sequence: 0,
            latest_tick: None,
            latest: None,
            events: VecDeque::new(),
            failure: None,
        })
    }
    pub fn welcome(&self) -> Option<&ServerControl> {
        self.welcome.as_ref()
    }
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
    pub fn player(&self) -> Option<sim::PlayerId> {
        match &self.welcome {
            Some(ServerControl::Welcome { player, .. }) => Some(*player),
            _ => None,
        }
    }
    pub fn take_state(&mut self) -> Option<StateUpdate> {
        self.latest.take()
    }
    pub fn take_events(&mut self) -> Option<EventBatch> {
        self.events.pop_front()
    }

    /// One call per local simulation tick; batches repeat unacknowledged input
    /// so isolated lost datagrams do not lose jump/buy/drop commands.
    pub fn send_input(&mut self, input: TickInput, view_tick: u64) -> Result<u64, String> {
        if self.player().is_none() {
            return Err("not admitted to match".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("input sequence overflow")?;
        self.pending.push_back(InputCommand {
            sequence: self.sequence,
            view_tick,
            input,
        });
        while self.pending.len() > MAX_INPUT_BATCH {
            self.pending.pop_front();
        }
        self.connection.send_message(
            INPUT,
            encode(&InputBatch(self.pending.iter().copied().collect()))?,
        );
        Ok(self.sequence)
    }

    pub fn update(&mut self, elapsed: Duration) -> Result<(), String> {
        let result = self.poll(elapsed);
        if let Err(reason) = &result {
            self.failure = Some(reason.clone());
            self.transport.disconnect();
        }
        result
    }
    fn poll(&mut self, elapsed: Duration) -> Result<(), String> {
        self.connection.update(elapsed);
        self.transport
            .update(elapsed, &mut self.connection)
            .map_err(|e| e.to_string())?;
        if self.connection.is_connected() && !self.sent_hello {
            self.connection.send_message(CONTROL, encode(&self.hello)?);
            self.sent_hello = true;
        }
        while let Some(bytes) = self.connection.receive_message(CONTROL) {
            let reply: ServerControl = decode(&bytes)?;
            match &reply {
                ServerControl::Rejected { reason } => return Err(reason.clone()),
                ServerControl::Welcome {
                    version,
                    content,
                    tick_rate,
                    ..
                } => {
                    if *version != VERSION
                        || content != &self.content
                        || !sim::TICK_RATE_RANGE.contains(tick_rate)
                    {
                        return Err("server welcome does not match local content/protocol".into());
                    }
                }
            }
            self.welcome = Some(reply);
        }
        while let Some(bytes) = self.connection.receive_message(SNAPSHOT) {
            if bytes.len() > MAX_MESSAGE {
                return Err("snapshot exceeds limit".into());
            }
            let state: StateUpdate =
                sim::state::decode(&self.catalog, &bytes).map_err(|e| e.to_string())?;
            if self
                .latest_tick
                .is_some_and(|tick| state.snapshot.state.tick <= tick)
            {
                continue;
            }
            self.latest_tick = Some(state.snapshot.state.tick);
            self.pending
                .retain(|command| command.sequence > state.acknowledged_input);
            self.latest = Some(state);
        }
        while let Some(bytes) = self.connection.receive_message(EVENTS) {
            if self.events.len() >= 512 {
                return Err("client is not consuming match events".into());
            }
            self.events.push_back(decode(&bytes)?);
        }
        self.transport
            .send_packets(&mut self.connection)
            .map_err(|e| e.to_string())
    }
    pub fn disconnect(&mut self) {
        self.transport.disconnect();
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::World;
    fn manifest() -> ContentManifest {
        ContentManifest {
            map_id: "csgo_map_loopback".into(),
            map_name: "loopback".into(),
            map_hash: "fixture".into(),
            catalog_hash: "fixture".into(),
        }
    }
    #[test]
    fn reliable_control_and_events_survive_loss_and_do_not_replay() {
        let mut server = RenetServer::new(channels());
        let mut client = RenetClient::new(channels());
        server.add_connection(7);
        client.set_connected();
        server.send_message(
            7,
            CONTROL,
            encode(&ServerControl::Rejected {
                reason: "content mismatch".into(),
            })
            .unwrap(),
        );
        server.send_message(
            7,
            EVENTS,
            encode(&EventBatch {
                tick: 4,
                events: vec![sim::GameEvent::Jump { player: 1 }],
            })
            .unwrap(),
        );
        // Lose every first-transmission packet, then deliver retransmissions
        // backwards, twice, as a network can do.
        assert!(!server.get_packets_to_send(7).unwrap().is_empty());
        server.update(Duration::from_millis(200));
        client.update(Duration::from_millis(200));
        let packets = server.get_packets_to_send(7).unwrap();
        assert!(!packets.is_empty());
        for packet in packets.iter().rev() {
            client.process_packet(packet);
            client.process_packet(packet);
        }
        let control: ServerControl = decode(&client.receive_message(CONTROL).unwrap()).unwrap();
        assert!(matches!(control, ServerControl::Rejected { .. }));
        let events: EventBatch = decode(&client.receive_message(EVENTS).unwrap()).unwrap();
        assert_eq!(events.events, vec![sim::GameEvent::Jump { player: 1 }]);
        assert!(client.receive_message(CONTROL).is_none());
        assert!(client.receive_message(EVENTS).is_none());
    }
    #[test]
    fn fragmented_unreliable_snapshots_reassemble_with_reordering_and_recover_after_loss() {
        let mut server = RenetServer::new(channels());
        let mut client = RenetClient::new(channels());
        server.add_connection(7);
        client.set_connected();
        let old = vec![17; 16 * 1024];
        server.send_message(7, SNAPSHOT, old);
        let packets = server.get_packets_to_send(7).unwrap();
        assert!(packets.len() > 2);
        for packet in packets.iter().skip(1).rev() {
            client.process_packet(packet);
        }
        assert!(client.receive_message(SNAPSHOT).is_none());
        let current = vec![29; 24 * 1024];
        server.send_message(7, SNAPSHOT, current.clone());
        for packet in server.get_packets_to_send(7).unwrap().iter().rev() {
            client.process_packet(packet);
        }
        assert_eq!(client.receive_message(SNAPSHOT).unwrap().as_ref(), current);
        assert!(client.receive_message(SNAPSHOT).is_none());
    }
    #[test]
    fn udp_join_input_snapshot_and_disconnect_use_the_authoritative_world() {
        let world = World::new(Arc::new(collision::CollisionWorld::new()));
        let catalog = world.catalog.clone();
        let session = Session::new(world, manifest()).unwrap();
        let mut server = Server::bind("127.0.0.1:0".parse().unwrap(), session, 4).unwrap();
        let mut client = Client::connect(
            server.address(),
            "Loopback".into(),
            Team::CounterTerrorist,
            manifest(),
            catalog,
        )
        .unwrap();
        let dt = Duration::from_secs_f64(1. / 128.);
        for _ in 0..128 {
            client.update(dt).unwrap();
            server.update(dt).unwrap();
            if client.player().is_some() {
                break;
            }
            std::thread::yield_now();
        }
        let id = client.player().expect("loopback admission");
        assert_eq!(server.session.world().players.len(), 1);
        assert!(matches!(
            client.welcome(),
            Some(ServerControl::Welcome { tick_rate: 128, .. })
        ));
        client
            .send_input(
                TickInput {
                    forward_move: 450.,
                    tracked: Some(sim::TrackedPose {
                        head: sim::Pose {
                            position: glam::Vec3::Z * 64.,
                            rotation: glam::Quat::IDENTITY,
                        },
                        left_hand: sim::Pose {
                            rotation: glam::Quat::IDENTITY,
                            ..Default::default()
                        },
                        right_hand: sim::Pose {
                            rotation: glam::Quat::IDENTITY,
                            ..Default::default()
                        },
                    }),
                    ..Default::default()
                },
                0,
            )
            .unwrap();
        let mut state = None;
        for _ in 0..32 {
            client.update(dt).unwrap();
            server.update(dt).unwrap();
            if let Some(next) = client.take_state() {
                if next.acknowledged_input == 1 {
                    state = Some(next);
                    break;
                }
            }
        }
        let state = state.expect("acknowledged snapshot");
        assert!(
            state
                .snapshot
                .state
                .players
                .iter()
                .any(|p| p.id == id && p.tracked.is_some())
        );
        assert!(state.snapshot.events.is_empty());
        client.disconnect();
        for _ in 0..8 {
            server.update(dt).unwrap();
        }
        assert!(server.session.world().player(id).is_none());
    }
}

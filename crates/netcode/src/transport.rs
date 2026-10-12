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
        if session.world().players.len() + max_clients > 64 {
            return Err("server supports at most 64 total player slots".into());
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
    fn info(&self) -> ServerInfo {
        ServerInfo {
            version: VERSION,
            tick_rate: self.session.world().tick_rate,
            snapshot_interval_ticks: self.snapshot_every as u32,
            max_unlag_ticks: self.session.lag_policy().max_history_ticks,
            interpolation_allowance_ticks: self.session.lag_policy().interpolation_allowance_ticks,
            content: self.session.content().clone(),
        }
    }

    /// Drive IO once and advance the sim only for elapsed fixed ticks.
    pub fn update(&mut self, elapsed: Duration) -> Result<usize, String> {
        self.elapsed = self.elapsed.saturating_add(elapsed);
        self.connection.update(elapsed);
        self.transport
            .update_with_packet_limit(elapsed, &mut self.connection, 1024)
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
            self.session
                .observe_rtt(client, self.connection.rtt(client));
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
                    Ok(ClientControl::Inspect { version }) => {
                        let reply = if version == VERSION {
                            ServerControl::Info(self.info())
                        } else {
                            ServerControl::Rejected {
                                reason: "incompatible protocol version".into(),
                            }
                        };
                        self.connection
                            .send_message(client, CONTROL, encode(&reply)?);
                    }
                    Ok(hello) => {
                        let reply = self.session.join(client, hello);
                        if matches!(reply, ServerControl::Welcome { .. }) {
                            self.pending_hellos.remove(&client);
                            // Reliable control ordering carries current policy before
                            // admission, including changes during local map loading.
                            self.connection.send_message(
                                client,
                                CONTROL,
                                encode(&ServerControl::Info(self.info()))?,
                            );
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
            if self.session.world().tick >= MAX_PROTOCOL_COUNTER {
                return Err("server timeline exhausted".into());
            }
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
    content: Option<ContentManifest>,
    hello: ClientControl,
    sent_hello: bool,
    welcome: Option<ServerControl>,
    server_info: Option<ServerInfo>,
    pending: VecDeque<InputCommand>,
    sequence: u64,
    latest_tick: Option<u64>,
    latest_ack: u64,
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
        content.validate()?;
        let hello = ClientControl::Hello {
            version: VERSION,
            name,
            team,
            content: content.clone(),
        };
        Self::open(server, hello, catalog, Some(content))
    }
    pub fn probe(server: SocketAddr) -> Result<Self, String> {
        Self::open(
            server,
            ClientControl::Inspect { version: VERSION },
            Arc::new(weapons::Catalog::default()),
            None,
        )
    }
    fn open(
        server: SocketAddr,
        hello: ClientControl,
        catalog: Arc<weapons::Catalog>,
        content: Option<ContentManifest>,
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
        Ok(Self {
            connection: RenetClient::new(channels()),
            transport,
            catalog,
            content,
            hello,
            sent_hello: false,
            welcome: None,
            server_info: None,
            pending: VecDeque::new(),
            sequence: 0,
            latest_tick: None,
            latest_ack: 0,
            latest: None,
            events: VecDeque::new(),
            failure: None,
        })
    }
    pub fn welcome(&self) -> Option<&ServerControl> {
        self.welcome.as_ref()
    }
    pub fn server_info(&self) -> Option<&ServerInfo> {
        self.server_info.as_ref()
    }
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
    /// A validated metadata request. The front end checks its content registry
    /// and loads supported content before reconnecting; this never mounts data.
    pub fn required_content(&self) -> Option<&ContentManifest> {
        match &self.welcome {
            Some(ServerControl::ContentRequired { content, .. }) => Some(content),
            _ => None,
        }
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
        if let Some(reason) = &self.failure {
            return Err(reason.clone());
        }
        if self.player().is_none() {
            return Err("not admitted to match".into());
        }
        if view_tick > MAX_PROTOCOL_COUNTER {
            return Err("view timeline exceeds protocol bounds".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .filter(|sequence| *sequence <= MAX_PROTOCOL_COUNTER)
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
            .update_with_packet_limit(elapsed, &mut self.connection, 256)
            .map_err(|e| e.to_string())?;
        if self.connection.is_disconnected() {
            return Err("remote transport disconnected".into());
        }
        if self.connection.is_connected() && !self.sent_hello {
            self.connection.send_message(CONTROL, encode(&self.hello)?);
            self.sent_hello = true;
        }
        for _ in 0..16 {
            let Some(bytes) = self.connection.receive_message(CONTROL) else {
                break;
            };
            let reply: ServerControl = decode(&bytes)?;
            match &reply {
                ServerControl::Info(info) => {
                    info.validate()?;
                    self.server_info = Some(info.clone());
                    continue;
                }
                ServerControl::Rejected { reason } => {
                    if reason.len() > 256 || reason.chars().any(char::is_control) {
                        return Err("invalid rejection metadata".into());
                    }
                    return Err(reason.clone());
                }
                ServerControl::ContentRequired { version, content } => {
                    if *version != VERSION {
                        return Err("incompatible content request protocol".into());
                    }
                    content.validate()?;
                    self.welcome = Some(reply);
                    return Err("server requires different map or weapon content".into());
                }
                ServerControl::Welcome {
                    version,
                    content,
                    tick_rate,
                    server_tick,
                    ..
                } => {
                    if *version != VERSION
                        || Some(content) != self.content.as_ref()
                        || !sim::TICK_RATE_RANGE.contains(tick_rate)
                        || *server_tick > MAX_PROTOCOL_COUNTER
                        || !self.server_info.as_ref().is_some_and(|info| {
                            info.version == *version
                                && info.tick_rate == *tick_rate
                                && info.content == *content
                        })
                    {
                        return Err("server welcome does not match local content/protocol".into());
                    }
                }
            }
            self.welcome = Some(reply);
        }
        for _ in 0..32 {
            let Some(bytes) = self.connection.receive_message(SNAPSHOT) else {
                break;
            };
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
            validate_state(
                &state,
                self.welcome.as_ref(),
                self.sequence,
                self.latest_ack,
            )?;
            self.latest_tick = Some(state.snapshot.state.tick);
            self.latest_ack = state.acknowledged_input;
            self.pending
                .retain(|command| command.sequence > state.acknowledged_input);
            self.latest = Some(state);
        }
        for _ in 0..64 {
            let Some(bytes) = self.connection.receive_message(EVENTS) else {
                break;
            };
            if self.events.len() >= 512 {
                return Err("client is not consuming match events".into());
            }
            let batch: EventBatch = decode(&bytes)?;
            validate_events(&batch)?;
            self.events.push_back(batch);
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
impl Server {
    pub(crate) fn inject_events(&mut self, batch: &EventBatch) {
        let bytes = encode(batch).unwrap();
        for client in self.connection.clients_id() {
            if self.session.player_for(client).is_some() {
                self.connection.send_message(client, EVENTS, bytes.clone());
            }
        }
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
            map_hash: "0".repeat(40),
            catalog_hash: "1".repeat(40),
        }
    }
    #[test]
    fn discovery_returns_bounded_metadata_without_admitting_gameplay() {
        let session = Session::new(
            World::new(Arc::new(collision::CollisionWorld::new())),
            manifest(),
        )
        .unwrap();
        let mut server = Server::bind("127.0.0.1:0".parse().unwrap(), session, 4).unwrap();
        let mut probe = Client::probe(server.address()).unwrap();
        let dt = Duration::from_secs_f64(1. / 128.);
        for _ in 0..256 {
            probe.update(dt).unwrap();
            server.update(dt).unwrap();
            if probe.server_info().is_some() {
                break;
            }
        }
        let info = probe.server_info().unwrap();
        info.validate().unwrap();
        assert_eq!(info.content, manifest());
        assert_eq!(info.tick_rate, 128);
        assert_eq!(info.snapshot_interval_ticks, 4);
        assert_eq!(info.interpolation_allowance_ticks, 8);
        assert!(server.session().world().players.is_empty());
        assert!(probe.player().is_none());
        assert!(probe.take_state().is_none());
        assert!(probe.take_events().is_none());
        probe.disconnect();
        for _ in 0..8 {
            server.update(dt).unwrap();
        }
        assert!(server.pending_hellos.is_empty());
    }
    #[test]
    fn malformed_fragment_indices_and_inconsistent_counts_disconnect_without_panicking() {
        let mut client = RenetClient::new(channels());
        client.set_connected();
        let mut packet = vec![3, 0, SNAPSHOT, 0, 2, 2, 0x44, 0xb0];
        packet.extend([0; 1200]);
        client.process_packet(&packet);
        assert!(client.is_disconnected());
        let mut client = RenetClient::new(channels());
        client.set_connected();
        let mut first = vec![2, 0, CONTROL, 0, 0, 2, 0x44, 0xb0];
        first.extend([0; 1200]);
        client.process_packet(&first);
        assert!(!client.is_disconnected());
        let mut changed = vec![2, 1, CONTROL, 0, 1, 3, 0x44, 0xb0];
        changed.extend([0; 1200]);
        client.process_packet(&changed);
        assert!(client.is_disconnected());
    }
    #[test]
    fn client_cannot_send_snapshot_or_event_channels_to_authority() {
        for channel in [SNAPSHOT, EVENTS] {
            let mut server = RenetServer::new(channels());
            server.add_connection(7);
            // Forged unreliable message on a server-to-client channel.
            server
                .process_packet_from(&[1, 0, channel, 0, 1, 1, 0], 7)
                .unwrap();
            assert!(!server.is_connected(7));
        }
    }
    #[test]
    fn snapshot_acknowledgements_and_ownership_are_validated_before_restore() {
        let mut world = World::new(Arc::new(collision::CollisionWorld::new()));
        let id = world.add_player("Player", Team::CounterTerrorist);
        let welcome = ServerControl::Welcome {
            version: VERSION,
            player: id,
            tick_rate: 128,
            server_tick: 0,
            content: manifest(),
        };
        let mut state = StateUpdate {
            acknowledged_input: 1,
            snapshot: world.snapshot(),
        };
        assert!(validate_state(&state, Some(&welcome), 1, 0).is_ok());
        assert!(validate_state(&state, Some(&welcome), 0, 0).is_err());
        state.snapshot.state.tick_rate = 0;
        assert!(validate_state(&state, Some(&welcome), 1, 0).is_err());
        state.snapshot.state.tick_rate = 128;
        state.snapshot.state.players.clear();
        assert!(validate_state(&state, Some(&welcome), 1, 0).is_err());
    }
    #[test]
    fn udp_content_negotiation_reports_a_safe_manifest_without_admitting_a_player() {
        let world = World::new(Arc::new(collision::CollisionWorld::new()));
        let catalog = world.catalog.clone();
        let session = Session::new(world, manifest()).unwrap();
        let mut server = Server::bind("127.0.0.1:0".parse().unwrap(), session, 4).unwrap();
        let mut different = manifest();
        different.map_name = "other".into();
        let mut client = Client::connect(
            server.address(),
            "Player".into(),
            Team::CounterTerrorist,
            different,
            catalog,
        )
        .unwrap();
        let dt = Duration::from_secs_f64(1. / 128.);
        for _ in 0..128 {
            if client.update(dt).is_err() {
                break;
            }
            server.update(dt).unwrap();
            std::thread::yield_now();
        }
        assert_eq!(client.required_content(), Some(&manifest()));
        assert!(client.player().is_none());
        assert!(server.session.world().players.is_empty());
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

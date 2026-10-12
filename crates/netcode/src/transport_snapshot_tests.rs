//! Actual loopback UDP with a bounded datagram relay; no simulated Renet
//! process_packet shortcut. These fixtures retain their ephemeral ports.
use super::*;
use std::io::ErrorKind;

struct Relay {
    socket: UdpSocket,
    server: SocketAddr,
    client: Option<SocketAddr>,
    held: Vec<Vec<u8>>,
    dropped: usize,
    large_packets: usize,
}
#[derive(Clone, Copy)]
enum Delivery {
    Forward,
    DropLarge,
    Hold,
}
impl Relay {
    fn new(server: SocketAddr) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        Self {
            socket,
            server,
            client: None,
            held: Vec::new(),
            dropped: 0,
            large_packets: 0,
        }
    }
    fn address(&self) -> SocketAddr {
        self.socket.local_addr().unwrap()
    }
    fn pump(&mut self, mode: Delivery) {
        let mut replies = Vec::new();
        let mut dropped_one = false;
        for _ in 0..2048 {
            let mut packet = [0_u8; 2048];
            let (length, source) = match self.socket.recv_from(&mut packet) {
                Ok(received) => received,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => panic!("relay receive: {e}"),
            };
            if source == self.server {
                if length > 1000 {
                    self.large_packets += 1;
                }
                if matches!(mode, Delivery::DropLarge) && length > 1000 && !dropped_one {
                    self.dropped += 1;
                    dropped_one = true;
                } else if matches!(mode, Delivery::Hold) {
                    assert!(self.held.len() < 1024);
                    self.held.push(packet[..length].to_vec());
                } else {
                    replies.push(packet[..length].to_vec());
                }
            } else {
                assert!(self.client.is_none_or(|client| client == source));
                self.client = Some(source);
                self.socket.send_to(&packet[..length], self.server).unwrap();
            }
        }
        // Every server batch is physically sent backwards, including fragments.
        if let Some(client) = self.client {
            for packet in replies.iter().rev() {
                self.socket.send_to(packet, client).unwrap();
            }
        }
    }
    fn release(&mut self) {
        for packet in self.held.drain(..).rev() {
            self.socket.send_to(&packet, self.client.unwrap()).unwrap();
        }
    }
}
fn manifest() -> ContentManifest {
    ContentManifest {
        map_id: "csgo_map_snapshot".into(),
        map_name: "snapshot".into(),
        map_hash: "0".repeat(40),
        catalog_hash: "1".repeat(40),
    }
}
fn fixture(rate: u32, actors: usize) -> (Server, Relay, Client) {
    let mut world = sim::World::new(Arc::new(collision::CollisionWorld::new()));
    world.set_tick_rate(rate);
    for index in 0..actors {
        world.add_player(&format!("Actor{index}"), Team::CounterTerrorist);
    }
    let catalog = world.catalog.clone();
    let server = Server::bind(
        "127.0.0.1:0".parse().unwrap(),
        Session::new(world, manifest()).unwrap(),
        1,
    )
    .unwrap();
    let relay = Relay::new(server.address());
    let client = Client::connect_local(
        relay.address(),
        "Local".into(),
        Team::CounterTerrorist,
        manifest(),
        catalog,
        77,
    )
    .unwrap();
    (server, relay, client)
}
fn cycle(server: &mut Server, relay: &mut Relay, client: &mut Client, dt: Duration) {
    client.update(dt).unwrap();
    relay.pump(Delivery::Forward);
    server.update(dt).unwrap();
    relay.pump(Delivery::Forward);
    client.update(Duration::ZERO).unwrap();
    relay.pump(Delivery::Forward);
    server.update(Duration::ZERO).unwrap();
    relay.pump(Delivery::Forward);
}
fn admitted(server: &mut Server, relay: &mut Relay, client: &mut Client, rate: u32) {
    let dt = Duration::from_secs_f64(1. / rate as f64);
    for _ in 0..512 {
        cycle(server, relay, client, dt);
        if client.snapshots.latest().is_some() && server.snapshots.confirmed(77).is_some() {
            return;
        }
    }
    panic!("UDP admission/receipt did not complete");
}

#[test]
fn actual_udp_fragment_loss_reorder_eviction_recovery_events_and_disconnect() {
    for rate in [64, 128] {
        let (mut server, mut relay, mut client) = fixture(rate, 31);
        admitted(&mut server, &mut relay, &mut client, rate);
        let dt = Duration::from_secs_f64(1. / 32.);
        let before = client.snapshots.latest().unwrap().id;
        // Force a multi-fragment full body, then lose one actual encrypted UDP
        // datagram while delivering remaining fragments in reverse order.
        server.snapshots.recover(77).unwrap();
        server.update(dt).unwrap();
        let lost_frame = server.snapshots.cache.latest().unwrap().id;
        relay.pump(Delivery::DropLarge);
        client.update(Duration::ZERO).unwrap();
        assert_eq!(relay.dropped, 1);
        assert!(relay.large_packets > 2);
        assert_eq!(client.snapshots.latest().unwrap().id, before);
        for _ in 0..8 {
            cycle(&mut server, &mut relay, &mut client, dt);
        }
        assert!(client.snapshots.latest().unwrap().id > lost_frame);
        // Evict all local baselines without touching authoritative state/ACK.
        // A missing patch must not publish partial state or acknowledge input.
        client.snapshots = Cache::default();
        let tick = client.latest_tick;
        let ack = client.latest_ack;
        // Ensure a non-keyframe delta is selected from the server's receipt.
        server.update(dt).unwrap();
        if server.snapshots.cache.latest().unwrap().id % snapshot_codec::CACHE_FRAMES as u64 == 0 {
            relay.pump(Delivery::Forward);
            client.update(Duration::ZERO).unwrap();
            relay.pump(Delivery::Forward);
            server.update(Duration::ZERO).unwrap();
            client.snapshots = Cache::default();
            server.update(dt).unwrap();
        }
        relay.pump(Delivery::Forward);
        client.update(Duration::ZERO).unwrap();
        assert!(client.snapshots.latest().is_none());
        assert_eq!(client.latest_tick, tick);
        assert_eq!(client.latest_ack, ack);
        relay.pump(Delivery::Forward);
        server.update(Duration::ZERO).unwrap();
        server.update(dt).unwrap();
        relay.pump(Delivery::Forward);
        client.update(Duration::ZERO).unwrap();
        assert!(client.snapshots.latest().is_some());
        relay.pump(Delivery::Forward);
        server.update(Duration::ZERO).unwrap();
        // Hold an entire older frame and release it only after a newer one.
        server.update(dt).unwrap();
        relay.pump(Delivery::Hold);
        let held = server.snapshots.cache.latest().unwrap().id;
        server.inject_events(&EventBatch {
            tick: server.session.world().tick,
            events: vec![sim::GameEvent::Jump {
                player: client.player().unwrap(),
            }],
        });
        server.update(dt).unwrap();
        relay.pump(Delivery::Forward);
        client.update(Duration::ZERO).unwrap();
        let latest = client.snapshots.latest().unwrap().id;
        assert!(latest > held);
        relay.release();
        client.update(Duration::ZERO).unwrap();
        assert_eq!(client.snapshots.latest().unwrap().id, latest);
        assert_eq!(client.take_events().unwrap().events.len(), 1);
        assert!(client.take_events().is_none());
        let state = client.take_state().unwrap();
        assert!(state.snapshot.events.is_empty());
        assert_eq!(
            sim::state::encode(&state.snapshot.state),
            &*client.snapshots.latest().unwrap().bytes
        );
        let player = client.player().unwrap();
        client.disconnect();
        relay.pump(Delivery::Forward);
        for _ in 0..8 {
            server.update(dt).unwrap();
            relay.pump(Delivery::Forward);
        }
        assert!(server.session.world().player(player).is_none());
    }
}

#[test]
fn preparing_same_tick_roster_changes_use_exact_frame_identity_then_begin() {
    for rate in [64, 128] {
        let (mut server, mut relay, mut client) = fixture(rate, 0);
        let dt = Duration::from_secs_f64(1. / 32.);
        for _ in 0..256 {
            client.update(dt).unwrap();
            relay.pump(Delivery::Forward);
            server.prepare(dt, 77).unwrap();
            relay.pump(Delivery::Forward);
            if client.snapshots.latest().is_some() {
                break;
            }
        }
        assert!(client.snapshots.latest().is_some());
        assert_eq!(server.session.world().tick, 0);
        // Change a server-owned actor roster at the frozen tick. This direct
        // test-only Session join exercises exact cache identity, not permission
        // to remotely admit another player while hosted Preparing is private.
        let added = server.session.join(
            88,
            ClientControl::Hello {
                version: VERSION,
                name: "Added".into(),
                team: Team::Terrorist,
                content: manifest(),
            },
        );
        assert!(matches!(added, ServerControl::Welcome { .. }));
        let first = client.snapshots.latest().unwrap().clone();
        for _ in 0..8 {
            client.update(dt).unwrap();
            relay.pump(Delivery::Forward);
            server.prepare(dt, 77).unwrap();
            relay.pump(Delivery::Forward);
        }
        client.update(Duration::ZERO).unwrap();
        let second = client.snapshots.latest().unwrap();
        assert_eq!(second.tick, first.tick);
        assert!(second.id > first.id);
        assert_ne!(second.hash, first.hash);
        let state: sim::state::WorldState =
            sim::state::decode(&client.catalog, &second.bytes).unwrap();
        assert_eq!(state.players.len(), 2);
        server.begin(77, 0).unwrap();
        for _ in 0..8 {
            cycle(&mut server, &mut relay, &mut client, dt);
        }
        let state = client.take_state().unwrap();
        assert!(state.snapshot.state.tick > 0);
        assert_eq!(state.snapshot.state.players.len(), 2);
    }
}

#[test]
fn queued_feedback_is_bounded_coalesced_and_cancelled() {
    let (_server, _relay, mut client) = fixture(128, 0);
    client.connection.set_connected();
    // Lose every transport ACK by not polling a peer; feedback keeps only the
    // newest pending value once its strict reliable queue budget is exhausted.
    for frame in 1..=2000 {
        client.snapshot_feedback = Some(ClientControl::SnapshotReceipt { frame, hash: frame });
        client.flush_snapshot_feedback().unwrap();
        assert!(CONTROL_MEMORY - client.connection.channel_available_memory(CONTROL) <= 512);
    }
    assert!(matches!(
        client.snapshot_feedback,
        Some(ClientControl::SnapshotReceipt { frame: 2000, .. })
    ));
    client.disconnect();
    assert!(client.snapshot_feedback.is_none());
    assert!(client.snapshots.latest().is_none());
}

#[test]
fn invalid_owner_state_cannot_mutate_cache_input_ack_or_pending_commands() {
    let (mut server, mut relay, mut client) = fixture(128, 0);
    admitted(&mut server, &mut relay, &mut client, 128);
    let latest_id = client.snapshots.latest().unwrap().id;
    let tick = client.latest_tick;
    let ack = client.latest_ack;
    client
        .send_input(
            TickInput {
                forward_move: 100.,
                ..Default::default()
            },
            tick.unwrap(),
        )
        .unwrap();
    assert_eq!(client.pending.len(), 1);
    let mut bad = server.session.world().state();
    bad.tick += 1;
    bad.players.clear();
    server
        .snapshots
        .capture(bad.tick, sim::state::encode(&bad))
        .unwrap();
    let packet = server.snapshots.message(77, 1).unwrap();
    server.connection.send_message(77, SNAPSHOT, packet);
    server.transport.send_packets(&mut server.connection);
    relay.pump(Delivery::Forward);
    assert!(client.update(Duration::ZERO).is_err());
    assert_eq!(client.snapshots.latest().unwrap().id, latest_id);
    assert_eq!(client.latest_tick, tick);
    assert_eq!(client.latest_ack, ack);
    assert_eq!(client.sequence, 1);
    assert_eq!(client.pending.len(), 1);
    assert!(client.snapshot_feedback.is_none());
}

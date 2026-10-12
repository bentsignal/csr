//! Owned, renderer-free joining lifecycle. The frontend loads verified maps.
use crate::{
    prediction::Predictor, protocol::*, remote_pose::PoseHistory, session::TickClock,
    transport::Client,
};
use sim::{PlayerId, Team, TickInput, World, WorldState};
use std::{collections::VecDeque, net::SocketAddr, time::Duration};

pub const MAX_EVENT_BATCHES: usize = 128;
pub const MAX_QUEUED_EVENTS: usize = 4096;
const JOIN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidElapsed,
    InvalidName,
    NotReady,
    ContentMismatch,
    UnsupportedWorld,
    InvalidInput(String),
    InvalidState(String),
    Transport(String),
    Timeout,
    EventOverflow,
    SequenceMismatch,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session::Session, transport::Server};
    use std::sync::Arc;
    fn manifest() -> ContentManifest {
        ContentManifest {
            map_id: "csgo_map_session".into(),
            map_name: "session".into(),
            map_hash: "0".repeat(40),
            catalog_hash: "1".repeat(40),
        }
    }
    fn map() -> World {
        let mut c = collision::CollisionWorld::new();
        let surface = c.surface_id("floor");
        c.add_triangle(
            [
                glam::Vec3::new(-4096., -4096., 0.),
                glam::Vec3::new(4096., -4096., 0.),
                glam::Vec3::new(4096., 4096., 0.),
            ],
            collision::contents::SOLID,
            surface,
        );
        c.add_triangle(
            [
                glam::Vec3::new(-4096., -4096., 0.),
                glam::Vec3::new(4096., 4096., 0.),
                glam::Vec3::new(-4096., 4096., 0.),
            ],
            collision::contents::SOLID,
            surface,
        );
        c.finalize();
        World::new(Arc::new(c))
    }
    fn server(rate: u32, allowance: u32) -> Server {
        let mut world = map();
        world.set_tick_rate(rate);
        let policy = crate::lag::LagPolicy {
            interpolation_allowance_ticks: allowance,
            ..crate::lag::LagPolicy::prototype(rate)
        };
        Server::bind(
            "127.0.0.1:0".parse().unwrap(),
            Session::with_lag_policy(world, manifest(), policy).unwrap(),
            4,
        )
        .unwrap()
    }
    fn discover(server: &mut Server, remote: &mut RemoteSession) {
        for _ in 0..256 {
            remote.update(1. / 128.).unwrap();
            server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
            if matches!(remote.status(), Status::AwaitingMap(_)) {
                return;
            }
        }
        panic!("discovery failed");
    }
    fn ready(server: &mut Server, remote: &mut RemoteSession) {
        discover(server, remote);
        remote.provide_map(map(), manifest()).unwrap();
        for _ in 0..256 {
            remote.update(1. / 128.).unwrap();
            server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
            if remote.status() == &Status::Ready {
                return;
            }
        }
        panic!("join failed");
    }
    #[test]
    fn join_owns_one_player_and_obeys_observed_64_tick_pose_policy() {
        let mut server = server(64, 1);
        let mut remote =
            RemoteSession::discover(server.address(), "Local".into(), Team::CounterTerrorist)
                .unwrap();
        ready(&mut server, &mut remote);
        assert_eq!(server.session().world().players.len(), 1);
        let info = remote.server_info().unwrap();
        assert_eq!(info.tick_rate, 64);
        assert_eq!(info.snapshot_interval_ticks, 2);
        let id = remote.player().unwrap();
        let tick = remote.authoritative_state().unwrap().tick;
        let sample = remote
            .pose_history()
            .unwrap()
            .sample(id, tick as f64 + 100.)
            .unwrap()
            .unwrap();
        assert_eq!(sample.used_tick, tick as f64 + 1.);
        assert!(remote.predicted_world().unwrap().events.is_empty());
    }
    #[test]
    fn invalid_input_elapsed_and_future_view_never_advance_sender_sequence() {
        let mut server = server(128, 8);
        let mut remote =
            RemoteSession::discover(server.address(), "Local".into(), Team::CounterTerrorist)
                .unwrap();
        ready(&mut server, &mut remote);
        let tick = remote.authoritative_state().unwrap().tick;
        let before = remote.predicted_world().unwrap().state();
        for elapsed in [f64::NAN, f64::INFINITY, -1., f64::MAX] {
            assert_eq!(
                remote.advance_inputs(elapsed, tick, |_| TickInput::default()),
                Err(Error::InvalidElapsed)
            );
        }
        assert!(
            remote
                .advance_inputs(1. / 128., tick + 1, |_| TickInput::default())
                .is_err()
        );
        assert!(
            remote
                .advance_inputs(1. / 128., tick, |_| TickInput {
                    side_move: f32::NAN,
                    ..Default::default()
                })
                .is_err()
        );
        assert_eq!(remote.predicted_world().unwrap().state(), before);
        assert_eq!(remote.predictor.as_ref().unwrap().pending_inputs(), 0);
        assert_eq!(
            remote
                .advance_inputs(0., tick, |_| TickInput::default())
                .unwrap()
                .predicted_ticks,
            0
        );
        assert_eq!(
            remote
                .advance_inputs(1. / 128., tick, |_| TickInput::default())
                .unwrap()
                .predicted_ticks,
            1
        );
        assert_eq!(remote.status(), &Status::Ready); // No sender/predictor sequence divergence.
        for _ in 0..8 {
            server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
            remote.update(1. / 128.).unwrap();
        }
        assert_eq!(remote.predictor.as_ref().unwrap().acknowledged_input(), 1);
    }
    #[test]
    fn a_render_stall_sends_at_most_eight_commands_and_bounded_history_backpressures() {
        let mut server = server(128, 8);
        let mut remote =
            RemoteSession::discover(server.address(), "Local".into(), Team::CounterTerrorist)
                .unwrap();
        ready(&mut server, &mut remote);
        let tick = remote.authoritative_state().unwrap().tick;
        let mut calls = 0;
        let advance = remote
            .advance_inputs(1000., tick, |_| {
                calls += 1;
                TickInput::default()
            })
            .unwrap();
        assert_eq!(advance.predicted_ticks, 8);
        assert_eq!(calls, 8);
        assert!(advance.discarded_wall_ticks > 0);
        for _ in 0..31 {
            remote
                .advance_inputs(1., tick, |_| TickInput::default())
                .unwrap();
        }
        assert_eq!(remote.predictor.as_ref().unwrap().pending_inputs(), 256);
        let advance = remote
            .advance_inputs(1., tick, |_| panic!("backpressure must not sample input"))
            .unwrap();
        assert!(advance.backpressured);
        assert_eq!(advance.predicted_ticks, 0);
    }
    #[test]
    fn cancellation_is_idempotent_in_every_join_stage_and_drops_ownership() {
        for stage in 0..4 {
            let mut server = server(128, 8);
            let mut remote =
                RemoteSession::discover(server.address(), "Local".into(), Team::CounterTerrorist)
                    .unwrap();
            if stage > 0 {
                discover(&mut server, &mut remote);
            }
            if stage > 1 {
                remote.provide_map(map(), manifest()).unwrap();
            }
            if stage > 2 {
                for _ in 0..256 {
                    remote.update(1. / 128.).unwrap();
                    server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
                    if remote.status() == &Status::Ready {
                        break;
                    }
                }
                assert_eq!(remote.status(), &Status::Ready);
            }
            remote.events.push_back(EventBatch {
                tick: 1,
                events: vec![sim::GameEvent::Jump { player: 1 }],
            });
            remote.queued_events = 1;
            remote.cancel();
            remote.cancel();
            assert_eq!(remote.status(), &Status::Closed(Error::Cancelled));
            assert!(remote.client.is_none());
            assert!(remote.predicted_world().is_none());
            assert!(remote.pose_history().is_none());
            assert!(remote.authoritative_state().is_none());
            assert!(remote.take_events().is_none());
            assert!(remote.events.is_empty());
            assert_eq!(remote.queued_events, 0);
            assert_eq!(remote.provide_map(map(), manifest()), Err(Error::NotReady));
            for _ in 0..16 {
                server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
            }
            assert!(server.session().world().players.is_empty());
        }
    }
    #[test]
    fn mismatched_content_never_sends_a_gameplay_hello() {
        let mut server = server(128, 8);
        let mut remote =
            RemoteSession::discover(server.address(), "Local".into(), Team::CounterTerrorist)
                .unwrap();
        discover(&mut server, &mut remote);
        let mut wrong = manifest();
        wrong.map_hash = "2".repeat(40);
        assert_eq!(
            remote.provide_map(map(), wrong),
            Err(Error::ContentMismatch)
        );
        assert!(matches!(remote.status(), Status::AwaitingMap(_)));
        assert!(remote.client.is_none());
        assert!(server.session().world().players.is_empty());
    }

    #[test]
    fn changed_server_rate_reprobes_before_using_stale_render_timing() {
        let mut old = server(128, 8);
        let address = old.address();
        let mut remote =
            RemoteSession::discover(address, "Local".into(), Team::CounterTerrorist).unwrap();
        discover(&mut old, &mut remote);
        let mut world = map();
        world.set_tick_rate(64);
        old.replace_unjoined_session(Session::new(world, manifest()).unwrap());
        let mut changed = old;
        assert_eq!(changed.address(), address);
        assert_eq!(
            std::net::UdpSocket::bind(address).unwrap_err().kind(),
            std::io::ErrorKind::AddrInUse
        );
        remote.provide_map(map(), manifest()).unwrap();
        discover(&mut changed, &mut remote);
        assert_eq!(remote.server_info().unwrap().tick_rate, 64);
        assert!(remote.predicted_world().is_none());
        assert!(remote.pose_history().is_none());
        for _ in 0..16 {
            changed.update(Duration::from_secs_f64(1. / 128.)).unwrap();
        }
        assert!(changed.session().world().players.is_empty());
    }

    #[test]
    fn changed_same_rate_policy_reprobes_before_ready() {
        let mut old = server(128, 8);
        let address = old.address();
        let mut remote =
            RemoteSession::discover(address, "Local".into(), Team::CounterTerrorist).unwrap();
        discover(&mut old, &mut remote);
        let mut world = map();
        world.set_tick_rate(128);
        let policy = crate::lag::LagPolicy {
            interpolation_allowance_ticks: 1,
            ..crate::lag::LagPolicy::prototype(128)
        };
        old.replace_unjoined_session(Session::with_lag_policy(world, manifest(), policy).unwrap());
        let mut changed = old;
        assert_eq!(changed.address(), address);
        assert_eq!(
            std::net::UdpSocket::bind(address).unwrap_err().kind(),
            std::io::ErrorKind::AddrInUse
        );
        remote.provide_map(map(), manifest()).unwrap();
        discover(&mut changed, &mut remote);
        assert_eq!(remote.server_info().unwrap().tick_rate, 128);
        assert_eq!(
            remote.server_info().unwrap().interpolation_allowance_ticks,
            1
        );
        assert!(remote.predicted_world().is_none());
        assert!(remote.pose_history().is_none());
        for _ in 0..16 {
            changed.update(Duration::from_secs_f64(1. / 128.)).unwrap();
        }
        assert!(changed.session().world().players.is_empty());
    }

    #[test]
    fn ready_policy_change_reprobes_and_discards_old_owned_state() {
        let mut server = server(128, 8);
        let mut remote =
            RemoteSession::discover(server.address(), "Local".into(), Team::CounterTerrorist)
                .unwrap();
        ready(&mut server, &mut remote);
        let mut changed = remote.server_info().unwrap().clone();
        changed.interpolation_allowance_ticks = 1;
        changed.validate().unwrap();
        remote.events.push_back(EventBatch {
            tick: 1,
            events: vec![sim::GameEvent::Jump {
                player: remote.player().unwrap(),
            }],
        });
        remote.queued_events = 1;
        server.inject_info(changed);
        server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
        remote.update(1. / 128.).unwrap();
        assert_eq!(remote.status(), &Status::Discovering);
        assert!(remote.predicted_world().is_none());
        assert!(remote.authoritative_state().is_none());
        assert!(remote.pose_history().is_none());
        assert!(remote.server_info().is_none());
        assert!(remote.events.is_empty());
        assert!(remote.take_events().is_none());
        assert_eq!(remote.queued_events, 0);
        discover(&mut server, &mut remote);
        assert!(matches!(remote.status(), Status::AwaitingMap(_)));
        assert!(remote.predicted_world().is_none());
    }

    #[test]
    fn hostile_effect_packet_closes_before_renderer_exposure_and_clears_queues() {
        let mut server = server(128, 8);
        let mut remote =
            RemoteSession::discover(server.address(), "Local".into(), Team::CounterTerrorist)
                .unwrap();
        ready(&mut server, &mut remote);
        server.inject_events(&EventBatch {
            tick: 1,
            events: vec![sim::GameEvent::Fire {
                player: remote.player().unwrap(),
                weapon: "weapon_test".into(),
                origin: glam::Vec3::NAN,
            }],
        });
        server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
        assert!(remote.update(1. / 128.).is_err());
        assert!(matches!(remote.status(), Status::Closed(_)));
        assert!(remote.take_events().is_none());
        assert!(remote.predicted_world().is_none());
        assert!(remote.pose_history().is_none());
    }

    #[test]
    fn malformed_late_death_receipt_closes_without_publishing_valid_prefix() {
        for rate in [64, 128] {
            let mut server = server(rate, rate / 16);
            let mut remote = RemoteSession::discover(server.address(), "Local".into(), Team::CounterTerrorist).unwrap();
            ready(&mut server, &mut remote);
            let mut death = crate::protocol::test_death_event();
            if let sim::GameEvent::Kill { death, .. } = &mut death {
                death.pose.velocity.x = f32::NAN;
            }
            server.inject_events(&EventBatch {
                tick: server.session().world().tick,
                events: vec![sim::GameEvent::Jump { player: remote.player().unwrap() }, death],
            });
            server.update(Duration::from_secs_f64(1. / rate as f64)).unwrap();
            let mut rejected = false;
            for _ in 0..32 {
                if remote.update(1. / rate as f64).is_err() { rejected = true; break; }
                server.update(Duration::from_secs_f64(1. / rate as f64)).unwrap();
            }
            assert!(rejected);
            assert!(matches!(remote.status(), Status::Closed(_)));
            assert!(remote.take_events().is_none());
            assert!(remote.predicted_world().is_none());
            assert!(remote.pose_history().is_none());
        }
    }

    #[test]
    fn snapshots_overtaking_admission_recover_without_exposing_or_replaying_effects() {
        let mut server = server(128, 8);
        let mut remote =
            RemoteSession::discover(server.address(), "Local".into(), Team::CounterTerrorist)
                .unwrap();
        discover(&mut server, &mut remote);
        server.hold_admission();
        remote.provide_map(map(), manifest()).unwrap();
        let dt = Duration::from_secs_f64(1. / 128.);
        for _ in 0..16 {
            remote.update(1. / 128.).unwrap();
            server.update(dt).unwrap();
        }
        assert_eq!(server.session().world().players.len(), 1);
        let id = server.session().world().players[0].id;
        server.inject_events(&EventBatch {
            tick: server.session().world().tick,
            events: vec![sim::GameEvent::Jump { player: id }],
        });
        for _ in 0..8 {
            server.update(dt).unwrap();
            remote.update(1. / 128.).unwrap();
        }
        assert_eq!(remote.status(), &Status::Synchronizing);
        assert!(remote.authoritative_state().is_none());
        assert!(remote.predicted_world().is_none());
        assert!(remote.take_events().is_none());
        assert!(
            remote
                .events
                .iter()
                .flat_map(|b| &b.events)
                .any(|e| matches!(e, sim::GameEvent::Jump { player } if *player == id))
        );
        server.release_admission();
        for _ in 0..16 {
            server.update(dt).unwrap();
            remote.update(1. / 128.).unwrap();
            if remote.status() == &Status::Ready {
                break;
            }
        }
        assert_eq!(remote.status(), &Status::Ready);
        let tick = remote.authoritative_state().unwrap().tick;
        remote
            .advance_inputs(1. / 128., tick, |_| TickInput::default())
            .unwrap();
        for _ in 0..16 {
            server.update(dt).unwrap();
            remote.update(1. / 128.).unwrap();
            if remote.acknowledged_input() == Some(1) {
                break;
            }
        }
        assert_eq!(remote.acknowledged_input(), Some(1));
        assert_eq!(remote.pending_inputs(), Some(0));
        assert_eq!(
            remote.predicted_world().unwrap().state(),
            *remote.authoritative_state().unwrap()
        );
        assert!(remote.predicted_world().unwrap().events.is_empty());
        let mut jumps = 0;
        while let Some(batch) = remote.take_events() {
            jumps += batch
                .events
                .iter()
                .filter(|e| matches!(e, sim::GameEvent::Jump { player } if *player == id))
                .count();
        }
        assert_eq!(jumps, 1);
        assert!(remote.take_events().is_none());
    }

    #[test]
    fn event_queue_overflow_is_typed_and_discards_old_session_state() {
        let mut server = server(128, 8);
        let mut remote =
            RemoteSession::discover(server.address(), "Local".into(), Team::CounterTerrorist)
                .unwrap();
        ready(&mut server, &mut remote);
        while remote.take_events().is_some() {}
        let batch = EventBatch {
            tick: 1,
            events: vec![
                sim::GameEvent::Jump {
                    player: remote.player().unwrap()
                };
                MAX_QUEUED_EVENTS
            ],
        };
        server.inject_events(&batch);
        server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
        remote.update(1. / 128.).unwrap();
        assert_eq!(remote.queued_events, MAX_QUEUED_EVENTS);
        server.inject_events(&EventBatch {
            tick: 2,
            events: vec![sim::GameEvent::Jump { player: 1 }],
        });
        server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
        assert_eq!(remote.update(1. / 128.), Err(Error::EventOverflow));
        assert_eq!(remote.status(), &Status::Closed(Error::EventOverflow));
        assert!(remote.events.is_empty());
        assert_eq!(remote.queued_events, 0);
        assert!(remote.authoritative_state().is_none());
    }

    #[test]
    fn server_disconnect_becomes_typed_closed_status() {
        let mut server = server(128, 8);
        let mut remote =
            RemoteSession::discover(server.address(), "Local".into(), Team::CounterTerrorist)
                .unwrap();
        ready(&mut server, &mut remote);
        drop(server);
        assert!(remote.update(1. / 128.).is_err());
        assert!(matches!(
            remote.status(),
            Status::Closed(Error::Transport(_))
        ));
        assert!(remote.predicted_world().is_none());
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Discovering,
    AwaitingMap(ServerInfo),
    Synchronizing,
    Ready,
    Closed(Error),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputContext {
    pub player: PlayerId,
    pub predicted_tick: u64,
    pub authoritative_tick: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Advance {
    pub predicted_ticks: usize,
    pub backpressured: bool,
    pub discarded_wall_ticks: u64,
}

pub struct RemoteSession {
    pub(crate) local_owner: Option<crate::host::LocalOwner>,
    address: SocketAddr,
    name: String,
    team: Team,
    status: Status,
    info: Option<ServerInfo>,
    client: Option<Client>,
    prepared_world: Option<World>,
    predictor: Option<Predictor>,
    poses: Option<PoseHistory>,
    clock: Option<TickClock>,
    events: VecDeque<EventBatch>,
    queued_events: usize,
    joining_elapsed: Duration,
}
impl RemoteSession {
    /// A passive local-host reservation. No socket, Hello or join deadline
    /// exists until the renderer acknowledges preparation and calls provide_map.
    /// The opaque ticket is consumed once; map loading itself stays external.
    pub fn local(
        admission: crate::host::LocalAdmission,
        name: String,
        team: Team,
    ) -> Result<Self, Error> {
        let name = name.trim().to_owned();
        if name.is_empty() || name.len() > 64 || name.chars().any(char::is_control) {
            return Err(Error::InvalidName);
        }
        admission.info.validate().map_err(Error::InvalidState)?;
        if admission.owner.cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(Self {
            address: admission.address,
            name,
            team,
            status: Status::AwaitingMap(admission.info.clone()),
            info: Some(admission.info),
            local_owner: Some(admission.owner),
            client: None,
            prepared_world: None,
            predictor: None,
            poses: None,
            clock: None,
            events: VecDeque::new(),
            queued_events: 0,
            joining_elapsed: Duration::ZERO,
        })
    }

    pub fn discover(address: SocketAddr, name: String, team: Team) -> Result<Self, Error> {
        let name = name.trim().to_owned();
        if name.is_empty() || name.len() > 64 || name.chars().any(char::is_control) {
            return Err(Error::InvalidName);
        }
        let client = Client::probe(address).map_err(Error::Transport)?;
        Ok(Self {
            local_owner: None,
            address,
            name,
            team,
            status: Status::Discovering,
            info: None,
            client: Some(client),
            prepared_world: None,
            predictor: None,
            poses: None,
            clock: None,
            events: VecDeque::new(),
            queued_events: 0,
            joining_elapsed: Duration::ZERO,
        })
    }
    pub fn status(&self) -> &Status {
        &self.status
    }
    pub fn server_info(&self) -> Option<&ServerInfo> {
        self.info.as_ref()
    }
    pub fn predicted_world(&self) -> Option<&World> {
        self.predictor.as_ref().map(Predictor::world)
    }
    pub fn authoritative_state(&self) -> Option<&WorldState> {
        self.predictor.as_ref().map(Predictor::authoritative_state)
    }
    pub fn pose_history(&self) -> Option<&PoseHistory> {
        self.poses.as_ref()
    }
    pub fn player(&self) -> Option<PlayerId> {
        self.predictor.as_ref().map(Predictor::player_id)
    }
    pub(crate) fn submitted_sequence(&self) -> Option<u64> {
        self.client.as_ref().map(Client::submitted_sequence)
    }
    pub fn pending_inputs(&self) -> Option<usize> {
        self.predictor.as_ref().map(Predictor::pending_inputs)
    }
    pub fn acknowledged_input(&self) -> Option<u64> {
        self.predictor.as_ref().map(Predictor::acknowledged_input)
    }
    pub fn take_events(&mut self) -> Option<EventBatch> {
        if self.status != Status::Ready {
            return None;
        }
        let batch = self.events.pop_front()?;
        self.queued_events -= batch.events.len();
        Some(batch)
    }

    /// Caller must resolve the exact id to a verified installed definition and
    /// build this World from its matching local bytes. No host path is used.
    pub fn provide_map(&mut self, world: World, manifest: ContentManifest) -> Result<(), Error> {
        let Status::AwaitingMap(info) = &self.status else {
            return Err(Error::NotReady);
        };
        manifest.validate().map_err(Error::InvalidState)?;
        if manifest != info.content {
            return Err(Error::ContentMismatch);
        }
        if world.collision.dynamic.is_some() {
            return Err(Error::UnsupportedWorld);
        }
        let client = if let Some(owner) = &self.local_owner {
            if owner.cancelled() {
                return Err(Error::Cancelled);
            }
            Client::connect_local(
                self.address,
                self.name.clone(),
                self.team,
                manifest,
                world.catalog.clone(),
                owner.client_id,
            )
        } else {
            Client::connect(
                self.address,
                self.name.clone(),
                self.team,
                manifest,
                world.catalog.clone(),
            )
        }
        .map_err(Error::Transport)?;
        self.client = Some(client);
        self.prepared_world = Some(world);
        self.status = Status::Synchronizing;
        self.joining_elapsed = Duration::ZERO;
        Ok(())
    }

    pub fn cancel(&mut self) {
        if !matches!(self.status, Status::Closed(_)) {
            self.close(Error::Cancelled);
        }
    }
    fn close(&mut self, reason: Error) {
        self.client = None; // Client::drop sends disconnect.
        self.prepared_world = None;
        self.predictor = None;
        self.poses = None;
        self.clock = None;
        self.events.clear();
        self.queued_events = 0;
        self.info = None;
        self.status = Status::Closed(reason);
    }
    fn reprobe(&mut self) -> Result<(), Error> {
        // A local reservation is tied to this exact host generation/policy.
        // Never turn a stale local ticket into an ordinary remote admission.
        if self.local_owner.is_some() {
            return Err(Error::ContentMismatch);
        }
        self.close(Error::Cancelled);
        self.client = Some(Client::probe(self.address).map_err(Error::Transport)?);
        self.status = Status::Discovering;
        self.joining_elapsed = Duration::ZERO;
        Ok(())
    }
    fn elapsed(seconds: f64) -> Result<Duration, Error> {
        if !seconds.is_finite() || seconds < 0. {
            return Err(Error::InvalidElapsed);
        }
        Duration::try_from_secs_f64(seconds).map_err(|_| Error::InvalidElapsed)
    }

    /// Poll bounded IO once per render update. No gameplay steps here.
    pub fn update(&mut self, elapsed_seconds: f64) -> Result<(), Error> {
        let elapsed = Self::elapsed(elapsed_seconds)?;
        if let Status::Closed(reason) = &self.status {
            return Err(reason.clone());
        }
        if self
            .local_owner
            .as_ref()
            .is_some_and(|owner| owner.cancelled())
        {
            self.close(Error::Cancelled);
            return Err(Error::Cancelled);
        }
        if matches!(self.status, Status::AwaitingMap(_)) {
            return Ok(());
        }
        let result = self.poll(elapsed);
        if let Err(reason) = &result {
            self.close(reason.clone());
        }
        result
    }
    fn poll(&mut self, elapsed: Duration) -> Result<(), Error> {
        if matches!(self.status, Status::Discovering | Status::Synchronizing) {
            self.joining_elapsed = self.joining_elapsed.saturating_add(elapsed);
            if self.joining_elapsed > JOIN_TIMEOUT {
                return Err(Error::Timeout);
            }
        }
        // Renet's internal Duration arithmetic cannot receive an enormous stall.
        let client = self.client.as_mut().ok_or(Error::NotReady)?;
        if let Err(reason) = client.update(elapsed.min(Duration::from_secs(1))) {
            if client.required_content().is_some() {
                // Map/rate may have changed during asynchronous loading. Obtain
                // fresh bounded metadata; don't trust the stale discovery info.
                return self.reprobe();
            }
            return Err(Error::Transport(reason));
        }
        if matches!(self.status, Status::Discovering) {
            if let Some(info) = client.server_info().cloned() {
                info.validate().map_err(Error::InvalidState)?;
                self.client = None; // Free the server's pending slot while loading.
                self.info = Some(info.clone());
                self.status = Status::AwaitingMap(info);
                self.joining_elapsed = Duration::ZERO;
            }
            return Ok(());
        }
        if let Some(current) = client.server_info() {
            current.validate().map_err(Error::InvalidState)?;
            if Some(current) != self.info.as_ref() {
                return self.reprobe();
            }
        }
        if let Some(ServerControl::Welcome {
            version,
            tick_rate,
            content,
            ..
        }) = client.welcome()
        {
            let info = self.info.as_ref().ok_or(Error::NotReady)?;
            info.validate().map_err(Error::InvalidState)?;
            if *version != info.version || *tick_rate != info.tick_rate || *content != info.content
            {
                return self.reprobe();
            }
        }
        if let Some(state) = client.take_state() {
            if state.snapshot.state.tick_rate
                != self.info.as_ref().ok_or(Error::NotReady)?.tick_rate
            {
                return self.reprobe();
            }
            if let Some(predictor) = &mut self.predictor {
                self.poses
                    .as_mut()
                    .unwrap()
                    .push(&state.snapshot.state)
                    .map_err(Error::InvalidState)?;
                predictor.reconcile(state).map_err(Error::InvalidState)?;
            } else {
                let welcome = client.welcome().cloned().ok_or(Error::NotReady)?;
                let rate = state.snapshot.state.tick_rate;
                let info = self.info.as_ref().ok_or(Error::NotReady)?;
                let extrapolation = info
                    .snapshot_interval_ticks
                    .saturating_mul(2)
                    .min(info.interpolation_allowance_ticks);
                let mut poses =
                    PoseHistory::new(rate, 32, extrapolation, 128.).map_err(Error::InvalidState)?;
                poses
                    .push(&state.snapshot.state)
                    .map_err(Error::InvalidState)?;
                let world = self.prepared_world.take().ok_or(Error::NotReady)?;
                self.predictor =
                    Some(Predictor::new(world, welcome, state).map_err(Error::InvalidState)?);
                self.poses = Some(poses);
                self.clock = Some(TickClock::new(rate).map_err(Error::InvalidState)?);
                self.status = Status::Ready;
            }
        }
        while let Some(batch) = client.take_events() {
            validate_events(&batch).map_err(Error::InvalidState)?;
            if self.events.len() >= MAX_EVENT_BATCHES
                || self.queued_events.saturating_add(batch.events.len()) > MAX_QUEUED_EVENTS
            {
                return Err(Error::EventOverflow);
            }
            self.queued_events += batch.events.len();
            self.events.push_back(batch);
        }
        Ok(())
    }

    /// Bound local catch-up to TickClock's eight ticks. The callback supplies
    /// per-tick controls; authoritative events alone are exposed for effects.
    pub fn advance_inputs(
        &mut self,
        elapsed_seconds: f64,
        view_tick: u64,
        mut input: impl FnMut(InputContext) -> TickInput,
    ) -> Result<Advance, Error> {
        let elapsed = Self::elapsed(elapsed_seconds)?;
        if self
            .local_owner
            .as_ref()
            .is_some_and(|owner| owner.cancelled())
        {
            self.close(Error::Cancelled);
            return Err(Error::Cancelled);
        }
        if self.status != Status::Ready {
            return Err(Error::NotReady);
        }
        let predictor = self.predictor.as_mut().unwrap();
        if view_tick > predictor.authoritative_tick() {
            return Err(Error::InvalidInput(
                "view tick is newer than authority".into(),
            ));
        }
        let ticks = self.clock.as_mut().unwrap().advance(elapsed);
        let mut advance = Advance {
            discarded_wall_ticks: self.clock.as_ref().unwrap().discarded_wall_ticks,
            ..Default::default()
        };
        for _ in 0..ticks {
            if !predictor.can_predict() {
                advance.backpressured = true;
                break;
            }
            let context = InputContext {
                player: predictor.player_id(),
                predicted_tick: predictor.world().tick,
                authoritative_tick: predictor.authoritative_tick(),
            };
            let command = predictor
                .prepare(input(context), view_tick)
                .map_err(Error::InvalidInput)?;
            // Validate and step first; a bad local command must never advance
            // transport's sequence. A subsequent transport failure closes both.
            predictor.predict(command).map_err(Error::InvalidInput)?;
            let sent = self
                .client
                .as_mut()
                .unwrap()
                .send_input(command.input, command.view_tick)
                .map_err(Error::Transport);
            let sequence = match sent {
                Ok(sequence) => sequence,
                Err(reason) => {
                    self.close(reason.clone());
                    return Err(reason);
                }
            };
            if sequence != command.sequence {
                self.close(Error::SequenceMismatch);
                return Err(Error::SequenceMismatch);
            }
            advance.predicted_ticks += 1;
        }
        if let Err(reason) = self.client.as_mut().unwrap().update(Duration::ZERO) {
            let reason = Error::Transport(reason);
            self.close(reason.clone());
            return Err(reason);
        }
        Ok(advance)
    }
}

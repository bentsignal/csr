use super::*;
use crate::{protocol::*, remote_session::Error, transport::Client};
use sim::{GameMode, RulesConfig, Team};

fn manifest() -> ContentManifest {
    ContentManifest {
        map_id: "csgo_map_host".into(),
        map_name: "host".into(),
        map_hash: "0".repeat(40),
        catalog_hash: "1".repeat(40),
    }
}
fn map(rate: u32) -> World {
    let mut collision = collision::CollisionWorld::new();
    let surface = collision.surface_id("floor");
    for points in [
        [
            glam::Vec3::new(-4096., -4096., 0.),
            glam::Vec3::new(4096., -4096., 0.),
            glam::Vec3::new(4096., 4096., 0.),
        ],
        [
            glam::Vec3::new(-4096., -4096., 0.),
            glam::Vec3::new(4096., 4096., 0.),
            glam::Vec3::new(-4096., 4096., 0.),
        ],
    ] {
        collision.add_triangle(points, collision::contents::SOLID, surface);
    }
    collision.finalize();
    let mut world = World::new(Arc::new(collision));
    world.set_tick_rate(rate);
    world
}
fn config() -> HostConfig {
    HostConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        max_clients: 4,
    }
}
fn fixture(rate: u32) -> (Authority, HostHandle, LocalAdmission) {
    let mut world = map(rate);
    world.add_bot("Reserved bot", Team::Terrorist);
    world.set_mode(
        GameMode::Competitive,
        RulesConfig {
            freeze_time: 0.,
            ..Default::default()
        },
    );
    let owner_id = u64::from_le_bytes(renet_netcode::generate_random_bytes());
    let server = Server::bind(
        config().bind,
        Session::new(world, manifest()).unwrap(),
        config().max_clients,
    )
    .unwrap();
    let shared = Arc::new(Shared {
        stopped: AtomicBool::new(false),
        status: Mutex::new(HostStatus::Preparing {
            bound_address: server.address(),
            local_address: server.address(),
            info: server.info(),
        }),
    });
    let (commands, _rx) = mpsc::sync_channel(COMMAND_CAPACITY);
    let mut handle = HostHandle {
        shared,
        owner_id,
        admission_taken: false,
        begin_requested: false,
        commands,
        worker: None,
    };
    let ticket = handle.take_local_admission().unwrap();
    (
        Authority {
            server,
            owner_id,
            running: false,
            admission_elapsed: None,
        },
        handle,
        ticket,
    )
}
fn local_ready(authority: &mut Authority, ticket: LocalAdmission, rate: u32) -> RemoteSession {
    let mut local = RemoteSession::local(ticket, "Owner".into(), Team::CounterTerrorist).unwrap();
    local.provide_map(map(rate), manifest()).unwrap();
    for _ in 0..512 {
        local.update(1. / rate as f64).unwrap();
        authority
            .update(Duration::from_secs_f64(1. / rate as f64))
            .unwrap();
        if local.status() == &Status::Ready {
            return local;
        }
    }
    panic!("frozen local snapshot admission failed");
}

#[test]
fn passive_ticket_and_session_survive_loading_without_actor_or_deadline() {
    let (mut authority, mut handle, ticket) = fixture(128);
    assert!(handle.take_local_admission().is_none());
    let before = authority.server.session().world().state();
    for _ in 0..20 {
        authority.update(Duration::from_secs(30)).unwrap();
    }
    assert_eq!(authority.server.session().world().state(), before);
    let mut local = RemoteSession::local(ticket, "Owner".into(), Team::CounterTerrorist).unwrap();
    local.update(3600.).unwrap();
    assert!(matches!(local.status(), Status::AwaitingMap(_)));
    assert!(local.predicted_world().is_none());
    assert_eq!(handle.begin(&local), Err(HostError::NotReady));
    assert_eq!(authority.server.session().world().players.len(), 1);
    let mut wrong = manifest();
    wrong.catalog_hash = "2".repeat(40);
    assert_eq!(
        local.provide_map(map(128), wrong),
        Err(Error::ContentMismatch)
    );
    assert!(local.predicted_world().is_none());
    // A failed local validation leaves the one-use reservation usable.
    local.provide_map(map(128), manifest()).unwrap();
    assert_eq!(local.status(), &Status::Synchronizing);
}

#[test]
fn discovery_never_creates_actor_and_other_hello_cannot_take_owner_slot() {
    let (mut authority, _handle, _ticket) = fixture(128);
    let address = authority.server.address();
    let mut probe = Client::probe(address).unwrap();
    let mut stranger = Client::connect(
        address,
        "Stranger".into(),
        Team::CounterTerrorist,
        manifest(),
        map(128).catalog,
    )
    .unwrap();
    for _ in 0..128 {
        probe.update(Duration::from_secs_f64(1. / 128.)).unwrap();
        let _ = stranger.update(Duration::from_secs_f64(1. / 128.));
        authority
            .update(Duration::from_secs_f64(1. / 128.))
            .unwrap();
        if probe.server_info().is_some() && stranger.failure().is_some() {
            break;
        }
    }
    assert_eq!(probe.server_info(), Some(&authority.server.info()));
    assert!(stranger.failure().is_some());
    assert_eq!(authority.server.session().world().players.len(), 1);
    assert_eq!(authority.server.session().world().tick, 0);
    assert!(authority.admission_elapsed.is_none());
}

#[test]
fn frozen_zero_freeze_admission_then_continuous_authority_at_64_and_128() {
    for rate in [64, 128] {
        let (mut authority, _handle, ticket) = fixture(rate);
        let mut local = local_ready(&mut authority, ticket, rate);
        assert_eq!(authority.server.session().world().tick, 0);
        assert_eq!(
            authority.server.session().world().round.phase,
            sim::Phase::Freeze
        );
        let id = local.player().unwrap();
        assert!(authority.server.session().world().player(id).unwrap().alive);
        assert_eq!(local.authoritative_state().unwrap().tick, 0);
        assert!(local.take_events().is_none());
        authority.begin(id, 0).unwrap();
        authority.begin(id, 0).unwrap(); // idempotent, no round restart
        let dt = 1. / rate as f64;
        for _ in 0..32 {
            let view = local.authoritative_state().unwrap().tick;
            local
                .advance_inputs(dt, view, |_| sim::TickInput {
                    side_move: 180.,
                    ..Default::default()
                })
                .unwrap();
            authority.update(Duration::from_secs_f64(dt)).unwrap();
            local.update(dt).unwrap();
        }
        assert_eq!(authority.server.session().world().tick, 32);
        assert_eq!(
            authority.server.session().world().round.phase,
            sim::Phase::Live
        );
        assert!(local.acknowledged_input().unwrap() > 0);
        let before = authority.server.session().world().tick;
        // No render/UI/input callback is needed to advance Running.
        for _ in 0..8 {
            authority.update(Duration::from_secs_f64(dt)).unwrap();
        }
        assert_eq!(authority.server.session().world().tick, before + 8);
        assert_eq!(
            authority
                .server
                .session()
                .world()
                .players
                .iter()
                .filter(|p| p.is_bot)
                .count(),
            1
        );
    }
}

#[test]
fn preparing_inputs_cannot_return_as_retransmissions_or_in_flight_after_begin() {
    for drained in [true, false] {
        let (mut authority, _handle, ticket) = fixture(128);
        let mut local = local_ready(&mut authority, ticket, 128);
        let player = local.player().unwrap();
        let before = authority
            .server
            .session()
            .world()
            .player(player)
            .unwrap()
            .movement
            .origin;
        local
            .advance_inputs(1. / 128., 0, |_| sim::TickInput {
                forward_move: 450.,
                buttons: movement::buttons::ATTACK,
                ..Default::default()
            })
            .unwrap();
        if drained {
            // The packet is received/discarded, but Client still retains it.
            authority
                .update(Duration::from_secs_f64(1. / 128.))
                .unwrap();
        }
        authority
            .begin(player, local.submitted_sequence().unwrap())
            .unwrap();
        // This batch also retransmits the old unacknowledged attack.
        local
            .advance_inputs(1. / 128., 0, |_| sim::TickInput::default())
            .unwrap();
        for _ in 0..12 {
            authority
                .update(Duration::from_secs_f64(1. / 128.))
                .unwrap();
            let actor = authority.server.session().world().player(player).unwrap();
            assert_eq!(
                actor.last_input.buttons & movement::buttons::ATTACK,
                0,
                "preparing attack crossed Begin; drained={drained}"
            );
            assert_eq!(
                actor.last_input.forward_move, 0.,
                "preparing movement crossed Begin; drained={drained}"
            );
            assert_eq!(actor.movement.origin.truncate(), before.truncate());
            local.update(1. / 128.).unwrap();
        }
        assert_eq!(local.acknowledged_input(), Some(2));
        assert_eq!(local.pending_inputs(), Some(0));
        assert!(local.predicted_world().unwrap().events.is_empty());
        assert_eq!(
            local.predicted_world().unwrap().state(),
            local.authoritative_state().unwrap().clone()
        );
    }
}

#[test]
fn begin_requires_exact_reserved_owner_and_validated_ready_session() {
    let (mut authority, mut handle, ticket) = fixture(128);
    let mut local = local_ready(&mut authority, ticket, 128);
    assert_eq!(
        authority.begin(local.player().unwrap() + 1, 0),
        Err(HostError::WrongOwner)
    );
    let (_other_authority, mut other, _ticket) = fixture(128);
    assert_eq!(other.begin(&local), Err(HostError::WrongOwner));
    // Fixture intentionally dropped its receiver: validation succeeds before
    // the closed queue is reported, whereas cancelled/ordinary peers cannot.
    assert_eq!(handle.begin(&local), Err(HostError::Cancelled));
    local.cancel();
    assert_eq!(handle.begin(&local), Err(HostError::NotReady));
    let ordinary =
        RemoteSession::discover(authority.server.address(), "Guest".into(), Team::Terrorist)
            .unwrap();
    assert_eq!(other.begin(&ordinary), Err(HostError::WrongOwner));
}

#[test]
fn local_connection_starts_bounded_admission_timeout_and_disconnect_closes_owner() {
    let (mut authority, _handle, ticket) = fixture(128);
    let _waiting_owner = local_ready(&mut authority, ticket, 128);
    assert!(authority.admission_elapsed.is_some());
    assert_eq!(
        authority.update(Duration::from_secs(6)),
        Err(HostError::OwnerAdmissionTimeout)
    );
    // A separate fixture tests running departure; no blind UDP rebind/retry.
    let (mut authority, _handle, ticket) = fixture(128);
    let mut local = local_ready(&mut authority, ticket, 128);
    authority.begin(local.player().unwrap(), 0).unwrap();
    local.cancel();
    for _ in 0..128 {
        if authority.update(Duration::from_secs_f64(1. / 128.)) == Err(HostError::OwnerDisconnected)
        {
            return;
        }
    }
    panic!("owner disconnect was not observed");
}

fn wait(handle: &mut HostHandle, predicate: impl Fn(&HostStatus) -> bool) -> HostStatus {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        handle.reap_if_finished();
        let status = handle.status();
        if predicate(&status) {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "worker status timeout: {status:?}"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn wait_closed_and_reaped(handle: &mut HostHandle) -> HostStatus {
    let closed = wait(handle, |s| matches!(s, HostStatus::Closed(_)));
    let deadline = Instant::now() + Duration::from_secs(5);
    // Closed is published after socket teardown, but the thread can still
    // be completing its epilogue when the status is observed.
    while !handle.reap_if_finished() {
        assert_eq!(handle.status(), closed);
        assert!(Instant::now() < deadline, "closed worker did not finish");
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(handle.status(), closed);
    closed
}

#[test]
fn closed_publication_can_precede_thread_completion_without_changing_terminal_status() {
    let shared = Arc::new(Shared {
        stopped: AtomicBool::new(false),
        status: Mutex::new(HostStatus::Binding),
    });
    let (commands, _receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
    let (published, publication) = mpsc::sync_channel(1);
    let (release, continuation) = mpsc::sync_channel(1);
    let worker_shared = shared.clone();
    let worker = thread::spawn(move || {
        worker_shared.publish(HostStatus::Closed(HostError::Cancelled));
        published.send(()).unwrap();
        continuation.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let mut handle = HostHandle {
        shared,
        owner_id: 1,
        admission_taken: false,
        begin_requested: false,
        commands,
        worker: Some(worker),
    };
    publication.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(handle.status(), HostStatus::Closed(HostError::Cancelled));
    assert!(!handle.reap_if_finished()); // worker is held by the channel
    release.send(()).unwrap();
    assert_eq!(
        wait_closed_and_reaped(&mut handle),
        HostStatus::Closed(HostError::Cancelled)
    );
    assert!(handle.worker.is_none());
    assert!(handle.reap_if_finished());
}

#[test]
fn owned_worker_wildcard_endpoint_cancel_socket_release_and_stale_ticket() {
    let mut handle = HostHandle::spawn(
        map(128),
        manifest(),
        HostConfig {
            bind: "0.0.0.0:0".parse().unwrap(),
            max_clients: 4,
        },
    )
    .unwrap();
    let HostStatus::Preparing {
        bound_address,
        local_address,
        ..
    } = wait(&mut handle, |s| matches!(s, HostStatus::Preparing { .. }))
    else {
        unreachable!()
    };
    assert!(local_address.ip().is_loopback());
    assert!(bound_address.ip().is_unspecified());
    assert_ne!(bound_address.port(), 0);
    assert_eq!(bound_address.port(), local_address.port());
    let ticket = handle.take_local_admission().unwrap();
    assert_eq!(ticket.address(), local_address);
    handle.cancel();
    handle.cancel();
    assert_eq!(
        wait_closed_and_reaped(&mut handle),
        HostStatus::Closed(HostError::Cancelled)
    );
    assert!(handle.reap_if_finished());
    assert!(matches!(
        RemoteSession::local(ticket, "Owner".into(), Team::Terrorist),
        Err(Error::Cancelled)
    ));
    assert!(handle.take_local_admission().is_none());
    // Closed means the socket is gone; keep the port reserved until test end.
    let _reserved = std::net::UdpSocket::bind(SocketAddr::new(
        Ipv4Addr::UNSPECIFIED.into(),
        local_address.port(),
    ))
    .unwrap();
}

#[test]
fn worker_bind_error_and_invalid_authority_are_typed() {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut handle = HostHandle::spawn(
        map(128),
        manifest(),
        HostConfig {
            bind: socket.local_addr().unwrap(),
            max_clients: 4,
        },
    )
    .unwrap();
    assert!(matches!(
        wait_closed_and_reaped(&mut handle),
        HostStatus::Closed(HostError::Bind(_))
    ));
    assert!(handle.reap_if_finished());
    let mut world = map(128);
    world.add_player("Offline You", Team::CounterTerrorist);
    assert!(matches!(
        HostHandle::spawn(world, manifest(), config()),
        Err(HostError::InvalidAuthority(_))
    ));
    assert!(matches!(
        HostHandle::spawn(
            map(128),
            manifest(),
            HostConfig {
                max_clients: 0,
                ..config()
            }
        ),
        Err(HostError::InvalidAuthority(_))
    ));
}

#[test]
fn delayed_cancel_and_preparing_publications_cannot_replace_terminal_result() {
    for error in [
        HostError::Cancelled,
        HostError::Transport("owned socket failed".into()),
    ] {
        let shared = Shared {
            stopped: AtomicBool::new(false),
            status: Mutex::new(HostStatus::Binding),
        };
        // Cancel's atomic exchange happens before the worker publishes Closed,
        // but its status publication can be delayed until after that result.
        shared.stopped.store(true, Ordering::Release);
        shared.publish(HostStatus::Closed(error.clone()));
        shared.publish(HostStatus::Stopping);
        shared.publish(HostStatus::Preparing {
            bound_address: "0.0.0.0:27015".parse().unwrap(),
            local_address: config().bind,
            info: ServerInfo {
                version: VERSION,
                tick_rate: 128,
                snapshot_interval_ticks: 4,
                max_unlag_ticks: 128,
                interpolation_allowance_ticks: 8,
                content: manifest(),
            },
        });
        assert_eq!(*shared.status.lock().unwrap(), HostStatus::Closed(error));
    }
}

#[test]
fn actual_worker_begin_is_idempotent_and_cancel_invalidates_ready_client() {
    let mut wildcard = config();
    wildcard.bind = "0.0.0.0:0".parse().unwrap();
    let mut handle = HostHandle::spawn(map(128), manifest(), wildcard).unwrap();
    let HostStatus::Preparing {
        bound_address,
        local_address,
        ..
    } = wait(&mut handle, |s| matches!(s, HostStatus::Preparing { .. }))
    else {
        unreachable!()
    };
    assert!(bound_address.ip().is_unspecified());
    assert!(local_address.ip().is_loopback());
    assert_ne!(bound_address.port(), 0);
    assert_eq!(bound_address.port(), local_address.port());
    let mut local = RemoteSession::local(
        handle.take_local_admission().unwrap(),
        "Owner".into(),
        Team::CounterTerrorist,
    )
    .unwrap();
    local.provide_map(map(128), manifest()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while local.status() != &Status::Ready {
        local.update(0.001).unwrap();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    // Capture the real sender's sequence, not a caller-provided guess.
    local
        .advance_inputs(1. / 128., 0, |_| sim::TickInput {
            forward_move: 450.,
            buttons: movement::buttons::ATTACK,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(local.submitted_sequence(), Some(1));
    handle.begin(&local).unwrap();
    handle.begin(&local).unwrap();
    let HostStatus::Running {
        bound_address: running_bound,
        local_address: running_local,
        ..
    } = wait(&mut handle, |s| matches!(s, HostStatus::Running { .. }))
    else {
        unreachable!()
    };
    assert_eq!(running_bound, bound_address);
    assert_eq!(running_local, local_address);
    let mut observed = 0;
    while observed == 0 {
        local.update(0.001).unwrap();
        observed = local.authoritative_state().unwrap().tick;
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(local.acknowledged_input(), Some(1));
    assert_eq!(local.pending_inputs(), Some(0));
    let owner = local
        .authoritative_state()
        .unwrap()
        .players
        .iter()
        .find(|p| p.id == local.player().unwrap())
        .unwrap();
    assert_eq!(owner.last_input.forward_move, 0.);
    assert_eq!(owner.last_input.buttons & movement::buttons::ATTACK, 0);
    assert_eq!(owner.shots, 0);
    handle.cancel();
    // No intervening update: cached Ready cannot let cancelled controllers
    // sample, accumulate time, predict or submit a fresh sequence.
    assert_eq!(
        local.advance_inputs(1000., 0, |_| panic!("cancelled input was sampled")),
        Err(Error::Cancelled)
    );
    assert_eq!(local.status(), &Status::Closed(Error::Cancelled));
    assert_eq!(local.submitted_sequence(), None);
    assert_eq!(local.pending_inputs(), None);
    assert_eq!(local.update(0.), Err(Error::Cancelled));
    assert_eq!(handle.begin(&local), Err(HostError::Cancelled));
    wait_closed_and_reaped(&mut handle);
    assert!(handle.reap_if_finished());
}

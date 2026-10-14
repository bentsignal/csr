//! Headless real-map proof of the same lifecycle used by a rendered frontend.
use netcode::{
    remote_session::{RemoteSession, Status},
    session::Session,
    transport::Server,
};
use sim::{Team, TickInput, World};
use std::time::Duration;

fn map_world(source: &World) -> World {
    let mut world = World::with_catalog(source.collision.clone(), source.catalog.clone());
    world.set_character_profiles(source.character_profiles());
    world.spawns = source.spawns.clone();
    world.objectives = source.objectives.clone();
    world
}
pub fn run(session: Session) -> Result<(), String> {
    let rate = session.world().tick_rate;
    let dt = 1. / rate as f64;
    let duration = Duration::from_secs_f64(dt);
    let manifest = session.content().clone();
    let mut server = Server::bind("127.0.0.1:0".parse().unwrap(), session, 4)?;
    let mut remotes = vec![
        RemoteSession::discover(
            server.address(),
            "Wrapper CT".into(),
            Team::CounterTerrorist,
        )
        .map_err(|e| format!("{e:?}"))?,
        RemoteSession::discover(server.address(), "Wrapper T".into(), Team::Terrorist)
            .map_err(|e| format!("{e:?}"))?,
    ];
    for _ in 0..256 {
        for remote in &mut remotes {
            remote.update(dt).map_err(|e| format!("{e:?}"))?;
        }
        server.update(duration)?;
        if remotes
            .iter()
            .all(|r| matches!(r.status(), Status::AwaitingMap(_)))
        {
            break;
        }
    }
    for remote in &mut remotes {
        let Status::AwaitingMap(info) = remote.status() else {
            return Err("wrapper discovery failed".into());
        };
        if info.content != manifest || info.tick_rate != rate {
            return Err("real-map discovery mismatch".into());
        }
        remote
            .provide_map(map_world(server.session().world()), manifest.clone())
            .map_err(|e| format!("{e:?}"))?;
    }
    for _ in 0..256 {
        for remote in &mut remotes {
            remote.update(dt).map_err(|e| format!("{e:?}"))?;
        }
        server.update(duration)?;
        if remotes.iter().all(|r| r.status() == &Status::Ready) {
            break;
        }
    }
    if remotes.iter().any(|r| r.status() != &Status::Ready) {
        return Err("wrapper synchronization failed".into());
    }
    let ids: Vec<_> = remotes.iter().map(|r| r.player().unwrap()).collect();
    if ids[0] == ids[1] {
        return Err("wrapper connections share player ownership".into());
    }
    let mut fractional = 0;
    for step in 1..=128 {
        for (n, remote) in remotes.iter_mut().enumerate() {
            if !(25..=28).contains(&step) {
                remote.update(dt).map_err(|e| format!("{e:?}"))?;
            }
            let state = remote
                .authoritative_state()
                .ok_or("wrapper lost authority")?;
            let tick = state.tick;
            if let Some(oldest) = remote.pose_history().unwrap().oldest_tick() {
                if oldest < tick {
                    let pose = remote
                        .pose_history()
                        .unwrap()
                        .sample(ids[1 - n], tick as f64 - 0.5)?
                        .ok_or("wrapper remote pose missing")?;
                    if !pose.pose.origin.is_finite() {
                        return Err("wrapper pose is not finite".into());
                    }
                    fractional += 1;
                }
            }
            let advance = remote
                .advance_inputs(dt, tick, |_| TickInput {
                    forward_move: if n == 0 { 450. } else { -450. },
                    view_angles: glam::Vec3::new(0., n as f32 * 180., 0.),
                    buttons: if step == 25 {
                        movement::buttons::JUMP
                    } else {
                        0
                    },
                    tracked: Some(sim::TrackedPose {
                        head: sim::Pose {
                            position: glam::Vec3::Z * 64.,
                            rotation: glam::Quat::IDENTITY,
                        },
                        ..Default::default()
                    }),
                    aim: Some(sim::AimRay {
                        origin: glam::Vec3::Z * 64.,
                        direction: glam::Vec3::Y,
                    }),
                    ..Default::default()
                })
                .map_err(|e| format!("{e:?}"))?;
            if advance.predicted_ticks != 1 || advance.backpressured {
                return Err("wrapper input clock mismatch".into());
            }
            if !remote.predicted_world().unwrap().events.is_empty() {
                return Err("wrapper exposed predicted effects".into());
            }
            while remote.take_events().is_some() {}
        }
        server.update(duration)?;
    }
    for _ in 0..32 {
        for remote in &mut remotes {
            remote.update(dt).map_err(|e| format!("{e:?}"))?;
            while remote.take_events().is_some() {}
        }
        if remotes
            .iter()
            .all(|r| r.pending_inputs() == Some(0) && r.acknowledged_input() == Some(128))
        {
            break;
        }
        server.update(duration)?;
    }
    for (n, remote) in remotes.iter_mut().enumerate() {
        if remote.pending_inputs() != Some(0) || remote.acknowledged_input() != Some(128) {
            return Err("wrapper did not acknowledge all 128 inputs".into());
        }
        if remote.predicted_world().unwrap().state() != *remote.authoritative_state().unwrap() {
            return Err("wrapper final prediction did not fully reconcile".into());
        }
        let player = server
            .session()
            .world()
            .player(ids[n])
            .ok_or("wrapper ownership missing")?;
        if player.tracked.is_none() || player.aim.is_none() {
            return Err("wrapper VR/aim data was lost".into());
        }
        remote.cancel();
        remote.cancel();
        if remote.predicted_world().is_some()
            || remote.pose_history().is_some()
            || remote.take_events().is_some()
        {
            return Err("wrapper cancellation retained old data".into());
        }
    }
    for _ in 0..16 {
        server.update(duration)?;
    }
    if ids
        .iter()
        .any(|id| server.session().world().player(*id).is_some())
    {
        return Err("wrapper cancellation retained server ownership".into());
    }
    if fractional == 0 {
        return Err("wrapper did not exercise fractional remote sampling".into());
    }
    println!(
        "RemoteSession check: 2 clients, {rate} tick, metadata discovery, exact installed content, 128 inputs each\n{fractional} fractional poses, independent tracked/aim data, exact final reconciliation, repeated cancellation\nfailures: 0"
    );
    Ok(())
}

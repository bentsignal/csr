//! Owned remote gameplay adapter. Frontend scheduling/loading stays outside.
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use netcode::{
    protocol::{ContentManifest, ServerInfo},
    remote_pose::RenderSample,
    remote_session::{Advance, Error, RemoteSession, Status},
};
use std::{collections::BTreeMap, net::SocketAddr};

#[derive(Resource)]
pub struct RemoteClient {
    session: RemoteSession,
    status: Status,
    host_running: bool,
    last_published: Option<u64>,
    snapshot_age: f64,
    frame: RenderFrame,
    pub last_advance: Option<Advance>,
}

/// Render data remains separate from the authoritative SimWorld.
#[derive(Default)]
pub struct RenderFrame {
    pub local: Option<crate::interpolation::Pose>,
    pub local_tracked: Option<sim::TrackedPose>,
    pub local_aim: Option<sim::AimRay>,
    pub remote: BTreeMap<sim::PlayerId, RenderSample>,
    pub render_tick: f64,
    pub view_tick: u64,
}

impl RemoteClient {
    pub fn discover(address: SocketAddr, name: String, team: sim::Team) -> Result<Self, Error> {
        Ok(Self::from_session(RemoteSession::discover(
            address, name, team,
        )?))
    }
    pub fn from_session(session: RemoteSession) -> Self {
        Self {
            status: session.status().clone(),
            host_running: true,
            session,
            last_published: None,
            snapshot_age: 0.,
            frame: RenderFrame::default(),
            last_advance: None,
        }
    }
    pub fn status(&self) -> &Status {
        &self.status
    }
    pub(crate) fn hosted(
        admission: netcode::host::LocalAdmission,
        name: String,
        team: sim::Team,
    ) -> Result<Self, Error> {
        let mut client = Self::from_session(RemoteSession::local(admission, name, team)?);
        client.host_running = false;
        Ok(client)
    }
    pub(crate) fn begin_host(
        &self,
        handle: &mut netcode::host::HostHandle,
    ) -> Result<(), netcode::host::HostError> {
        handle.begin(&self.session)
    }
    pub(crate) fn mark_host_running(&mut self) {
        self.host_running = true;
    }
    pub(crate) fn gameplay_ready(&self) -> bool {
        self.status == Status::Ready && self.host_running
    }
    pub fn server_info(&self) -> Option<&ServerInfo> {
        self.session.server_info()
    }
    pub fn frame(&self) -> &RenderFrame {
        &self.frame
    }
    pub fn provide_map(
        &mut self,
        world: sim::World,
        manifest: ContentManifest,
    ) -> Result<(), Error> {
        self.session.provide_map(world, manifest)?;
        self.status = self.session.status().clone();
        Ok(())
    }
    pub fn cancel(&mut self) {
        let previous = matches!(self.status, Status::Closed(_)).then(|| self.status.clone());
        self.session.cancel();
        self.status = previous.unwrap_or_else(|| self.session.status().clone());
        self.clear_publication();
    }
    fn clear_publication(&mut self) {
        self.frame = RenderFrame::default();
        self.last_advance = None;
        self.last_published = None;
        self.snapshot_age = 0.;
    }
    fn close(&mut self, error: Error) {
        self.cancel();
        self.status = Status::Closed(error);
    }
}

/// Tracking stays independent from the desktop camera and aim direction.
#[derive(Resource, Default, Clone, Copy)]
pub struct TrackedControls {
    pub tracked: Option<sim::TrackedPose>,
    pub aim: Option<sim::AimRay>,
}

/// Root schedules this before camera, characters, effects and HUD. Networking
/// uses real time even when offline virtual time is paused by the menu.
#[allow(clippy::too_many_arguments)]
pub fn drive(
    mut client: ResMut<RemoteClient>,
    real: Res<Time<Real>>,
    mut world: Option<ResMut<crate::player::SimWorld>>,
    mut local: Option<ResMut<crate::player::LocalPlayer>>,
    mut poses: ResMut<crate::interpolation::RenderPoses>,
    mut pending: ResMut<crate::effects::PendingEvents>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&CursorOptions, With<PrimaryWindow>>,
    settings: Res<crate::settings::Settings>,
    front: Res<crate::menu::Menu>,
    buy_menu: Res<crate::buymenu::BuyMenu>,
    enabled: Option<Res<crate::player::InputEnabled>>,
    block: Option<Res<crate::player::InputBlockFrames>>,
    tracking: Option<Res<TrackedControls>>,
) {
    let client = &mut *client;
    if matches!(client.status, Status::Closed(_)) {
        client.clear_publication();
        if let Some(local) = local.as_mut() {
            local.clear_remote_lifecycle();
        }
        poses.clear_overrides();
        pending.0.clear();
        if let Some(world) = world.as_mut() {
            world.0.events.clear();
        }
        return;
    }
    let elapsed = real.delta_secs_f64();
    if let Err(error) = client.session.update(elapsed) {
        client.close(error);
    } else {
        client.status = client.session.status().clone();
    }
    if client.status != Status::Ready {
        client.clear_publication();
        if let Some(local) = local.as_mut() {
            local.clear_remote_lifecycle();
        }
        poses.clear_overrides();
        pending.0.clear();
        if let Some(world) = world.as_mut() {
            world.0.events.clear();
        }
        return;
    }
    let (Some(world), Some(local)) = (world.as_mut(), local.as_mut()) else {
        client.clear_publication();
        poses.clear_overrides();
        return;
    };
    let authority = client.session.authoritative_state().unwrap();
    if client.last_published != Some(authority.tick) {
        world.0.restore(authority.clone());
        client.last_published = Some(authority.tick);
        client.snapshot_age = 0.;
    } else {
        client.snapshot_age = (client.snapshot_age + elapsed).min(1.);
    }
    let id = client.session.player().unwrap();
    local.sync_remote(&world.0, id);
    // Only reliable authority supplies effects. Keep the downstream queue
    // bounded too, including when a frontend temporarily stops consuming it.
    world.0.events.clear();
    while let Some(batch) = client.session.take_events() {
        if pending.0.len().saturating_add(batch.events.len())
            > netcode::remote_session::MAX_QUEUED_EVENTS
        {
            pending.0.clear();
            world.0.events.clear();
            poses.clear_overrides();
            client.close(Error::EventOverflow);
            return;
        }
        world.0.events.extend(batch.events.iter().cloned());
        pending.0.extend(batch.events);
    }
    let info = client.session.server_info().unwrap();
    let tick = world.0.tick;
    let allowance = info.interpolation_allowance_ticks as f64;
    let extrapolation = info
        .snapshot_interval_ticks
        .saturating_mul(2)
        .min(info.interpolation_allowance_ticks) as f64;
    let render_tick = (tick as f64 + client.snapshot_age * info.tick_rate as f64 - allowance)
        .max(
            client
                .session
                .pose_history()
                .unwrap()
                .oldest_tick()
                .unwrap_or(tick) as f64,
        )
        .min(tick as f64 + extrapolation);
    let view_tick = (render_tick.floor() as u64).min(tick);
    let tracked = tracking.as_deref().copied().unwrap_or_default();
    let paused = front.open() || !client.host_running;
    if paused {
        local.latched = 0;
        local.pending_slot = 0;
        local.pending_drop = false;
        local.pending_buys.clear();
    }
    let controls = crate::player::Controls {
        keys: &keys,
        mouse: &mouse,
        settings: &settings,
        enabled: enabled.as_ref().is_some_and(|e| e.0),
        blocked: block.as_ref().is_some_and(|b| b.0 != 0),
        captured: windows
            .single()
            .is_ok_and(|c| c.grab_mode != CursorGrabMode::None),
        buy_menu: buy_menu.open,
    };
    match client.session.advance_inputs(elapsed, view_tick, |_| {
        let mut input = if paused {
            sim::TickInput {
                view_angles: glam::Vec3::new(local.pitch, local.yaw, 0.),
                ..Default::default()
            }
        } else {
            crate::player::sample_tick_input(local, &controls)
        };
        input.tracked = tracked.tracked;
        input.aim = tracked.aim;
        input
    }) {
        Ok(advance) => client.last_advance = Some(advance),
        Err(error) => {
            client.close(error);
            poses.clear_overrides();
            pending.0.clear();
            world.0.events.clear();
            return;
        }
    }
    let mut frame = RenderFrame {
        render_tick,
        view_tick,
        ..Default::default()
    };
    let predicted = client.session.predicted_world().unwrap();
    frame.local = predicted
        .player(id)
        .map(|actor| poses.capture_prediction_at(actor, predicted.tick, predicted.tick_rate));
    if let Some(actor) = predicted.player(id) {
        frame.local_tracked = actor.tracked;
        frame.local_aim = actor.aim;
    }
    let mut overrides = Vec::with_capacity(world.0.players.len());
    if let Some(pose) = frame.local {
        overrides.push((id, pose));
    }
    for actor in world.0.players.iter().filter(|p| p.id != id) {
        match client
            .session
            .pose_history()
            .unwrap()
            .sample(actor.id, render_tick)
        {
            Ok(Some(sample)) => {
                overrides.push((actor.id, crate::interpolation::Pose::remote(&sample.pose)));
                frame.remote.insert(actor.id, sample);
            }
            Ok(None) => {}
            Err(reason) => {
                client.close(Error::InvalidState(reason));
                poses.clear_overrides();
                pending.0.clear();
                world.0.events.clear();
                return;
            }
        }
    }
    poses.set_overrides(overrides);
    client.frame = frame;
}

/// Root calls this on leaving a remote session, before starting an offline one.
pub fn clear(world: &mut World) {
    if let Some(mut client) = world.remove_resource::<RemoteClient>() {
        client.cancel();
    }
    if let Some(mut poses) = world.get_resource_mut::<crate::interpolation::RenderPoses>() {
        poses.clear_overrides();
    }
    if let Some(mut events) = world.get_resource_mut::<crate::effects::PendingEvents>() {
        events.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netcode::{session::Session, transport::Server};
    use std::{sync::Arc, time::Duration};
    fn manifest() -> ContentManifest {
        ContentManifest {
            map_id: "csgo_map_adapter".into(),
            map_name: "adapter".into(),
            map_hash: "0".repeat(40),
            catalog_hash: "1".repeat(40),
        }
    }
    fn map() -> sim::World {
        let mut collision = collision::CollisionWorld::new();
        let floor = collision.surface_id("floor");
        for triangle in [
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
            collision.add_triangle(triangle, collision::contents::SOLID, floor);
        }
        collision.finalize();
        sim::World::new(Arc::new(collision))
    }
    fn setup() -> (App, Server, sim::PlayerId) {
        let mut world = map();
        let peer = world.add_bot("Server bot", sim::Team::Terrorist);
        let mut server = Server::bind(
            "127.0.0.1:0".parse().unwrap(),
            Session::new(world, manifest()).unwrap(),
            4,
        )
        .unwrap();
        let mut remote = RemoteSession::discover(
            server.address(),
            "Local".into(),
            sim::Team::CounterTerrorist,
        )
        .unwrap();
        let dt = Duration::from_secs_f64(1. / 128.);
        for _ in 0..64 {
            remote.update(1. / 128.).unwrap();
            server.update(dt).unwrap();
            if matches!(remote.status(), Status::AwaitingMap(_)) {
                break;
            }
        }
        remote.provide_map(map(), manifest()).unwrap();
        for _ in 0..64 {
            remote.update(1. / 128.).unwrap();
            server.update(dt).unwrap();
            if remote.status() == &Status::Ready {
                break;
            }
        }
        assert_eq!(remote.status(), &Status::Ready);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(dt))
            .insert_resource(RemoteClient::from_session(remote))
            .insert_resource(crate::player::SimWorld(map()))
            .insert_resource(crate::player::LocalPlayer::new(999, glam::Vec3::ZERO, 0.))
            .insert_resource(crate::player::InputEnabled(true))
            .init_resource::<crate::player::InputBlockFrames>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<crate::settings::Settings>()
            .init_resource::<crate::menu::Menu>()
            .init_resource::<crate::buymenu::BuyMenu>()
            .init_resource::<crate::interpolation::RenderPoses>()
            .init_resource::<crate::effects::PendingEvents>()
            .init_resource::<crate::sound_probe::SoundProbe>()
            .add_systems(Update, drive)
            // Even accidentally registered offline ticking cannot run gameplay.
            .add_systems(FixedUpdate, crate::player::tick);
        app.world_mut().spawn((
            PrimaryWindow,
            CursorOptions {
                grab_mode: CursorGrabMode::Locked,
                ..Default::default()
            },
        ));
        app.update();
        app.world_mut()
            .resource_mut::<crate::effects::PendingEvents>()
            .0
            .clear();
        (app, server, peer)
    }
    #[test]
    fn hosted_constructor_retains_passive_map_admission_until_render_preparation() {
        let mut host = netcode::host::HostHandle::spawn(
            map(),
            manifest(),
            netcode::host::HostConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                max_clients: 2,
            },
        )
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let admission = loop {
            if let Some(admission) = host.take_local_admission() {
                break admission;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        let mut client =
            RemoteClient::hosted(admission, "Host".into(), sim::Team::CounterTerrorist).unwrap();
        client.session.update(1000.).unwrap();
        assert!(matches!(client.status(), Status::AwaitingMap(_)));
        assert!(!client.gameplay_ready());
        assert!(crate::remote_join::loading(Some(&client)));
        assert!(client.session.player().is_none());
        client.cancel();
        host.cancel();
        while !host.reap_if_finished() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn local_host_ready_transport_is_neutral_until_running_then_accepts_controls() {
        let (mut app, mut server, _) = setup();
        app.world_mut().resource_mut::<RemoteClient>().host_running = false;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.world_mut()
            .resource_mut::<crate::player::LocalPlayer>()
            .latched = movement::buttons::JUMP;
        let dt = Duration::from_secs_f64(1. / 128.);
        for _ in 0..12 {
            app.update();
            server.update(dt).unwrap();
        }
        let client = app.world().resource::<RemoteClient>();
        assert_eq!(client.status(), &Status::Ready);
        assert!(crate::remote_join::loading(Some(client)));
        let id = client.session.player().unwrap();
        let input = &server.session().world().player(id).unwrap().last_input;
        assert_eq!(input.buttons, 0);
        assert_eq!(input.forward_move, 0.);
        assert_eq!(input.side_move, 0.);
        assert_eq!(
            app.world().resource::<crate::player::LocalPlayer>().latched,
            0
        );

        app.world_mut()
            .resource_mut::<RemoteClient>()
            .mark_host_running();
        assert!(!crate::remote_join::loading(Some(
            app.world().resource::<RemoteClient>()
        )));
        for _ in 0..12 {
            app.update();
            server.update(dt).unwrap();
        }
        let input = &server.session().world().player(id).unwrap().last_input;
        assert!(input.forward_move > 0.);
        assert_ne!(input.buttons & movement::buttons::ATTACK, 0);
    }

    #[test]
    fn prediction_and_render_samples_never_change_authoritative_hud_or_run_offline_ai() {
        let (mut app, mut server, peer) = setup();
        let before = app.world().resource::<crate::player::SimWorld>().0.state();
        let local = app.world().resource::<crate::player::LocalPlayer>().id;
        assert_ne!(local, 999);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..8 {
            app.update();
        }
        let world = &app.world().resource::<crate::player::SimWorld>().0;
        assert_eq!(world.state(), before); // No authoritative packets/AI steps.
        let remote = app.world().resource::<RemoteClient>();
        assert!(
            remote
                .frame
                .local
                .unwrap()
                .origin
                .distance(world.player(local).unwrap().movement.origin)
                > 0.
        );
        let sample = remote.frame.remote.get(&peer).unwrap();
        let rendered = app
            .world()
            .resource::<crate::interpolation::RenderPoses>()
            .sample(world.player(peer).unwrap(), 0.5);
        assert_eq!(rendered.origin, sample.pose.origin);
        assert!(
            app.world()
                .resource::<crate::effects::PendingEvents>()
                .0
                .is_empty()
        );
        assert!(remote.session.predicted_world().unwrap().events.is_empty());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                Duration::ZERO,
            ));
        for _ in 0..64 {
            server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
            app.update();
            if app
                .world()
                .resource::<RemoteClient>()
                .session
                .pending_inputs()
                == Some(0)
            {
                break;
            }
        }
        let remote = app.world().resource::<RemoteClient>();
        assert_eq!(remote.session.pending_inputs(), Some(0));
        assert_eq!(
            remote.session.predicted_world().unwrap().state(),
            *remote.session.authoritative_state().unwrap()
        );
        assert_eq!(
            app.world().resource::<crate::player::SimWorld>().0.state(),
            *remote.session.authoritative_state().unwrap()
        );
    }
    #[test]
    fn pause_keeps_network_running_with_neutral_controls_and_independent_tracking() {
        let (mut app, mut server, _) = setup();
        let tracked = sim::TrackedPose {
            head: sim::Pose {
                position: glam::Vec3::Z * 64.,
                rotation: glam::Quat::IDENTITY,
            },
            ..Default::default()
        };
        let aim = sim::AimRay {
            origin: glam::Vec3::Z * 64.,
            direction: glam::Vec3::Y,
        };
        app.world_mut().insert_resource(TrackedControls {
            tracked: Some(tracked),
            aim: Some(aim),
        });
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        app.world_mut().resource_mut::<crate::menu::Menu>().page = Some(crate::menu::Page::Pause);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        {
            let mut local = app.world_mut().resource_mut::<crate::player::LocalPlayer>();
            local.latched = movement::buttons::ATTACK;
            local.pending_buys.push(sim::Purchase::Kevlar);
        }
        for _ in 0..16 {
            app.update();
            server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
        }
        let remote = app.world().resource::<RemoteClient>();
        assert!(remote.session.acknowledged_input().unwrap() > 0);
        let actor = server
            .session()
            .world()
            .player(app.world().resource::<crate::player::LocalPlayer>().id)
            .unwrap();
        assert_eq!(actor.last_input.forward_move, 0.);
        assert_eq!(actor.last_input.buttons, 0);
        assert_eq!(actor.last_input.buy, sim::Purchase::None);
        assert_eq!(actor.tracked, Some(tracked));
        assert_eq!(actor.aim, Some(aim));
        assert_eq!(remote.frame.local_tracked, Some(tracked));
        assert_eq!(remote.frame.local_aim, Some(aim));
        assert!(
            app.world()
                .resource::<crate::player::LocalPlayer>()
                .pending_buys
                .is_empty()
        );
    }
    #[test]
    fn cancellation_clears_old_render_and_events_and_removes_owned_server_player() {
        let (mut app, mut server, _) = setup();
        let id = app.world().resource::<crate::player::LocalPlayer>().id;
        app.world_mut()
            .resource_mut::<crate::effects::PendingEvents>()
            .0
            .push(sim::GameEvent::Jump { player: id });
        app.world_mut().resource_mut::<RemoteClient>().cancel();
        app.world_mut().resource_mut::<RemoteClient>().cancel();
        app.update();
        assert_eq!(
            app.world().resource::<RemoteClient>().status(),
            &Status::Closed(Error::Cancelled)
        );
        assert!(app.world().resource::<RemoteClient>().frame.local.is_none());
        assert!(
            app.world()
                .resource::<crate::effects::PendingEvents>()
                .0
                .is_empty()
        );
        assert!(
            app.world()
                .resource::<crate::player::SimWorld>()
                .0
                .events
                .is_empty()
        );
        clear(app.world_mut());
        clear(app.world_mut());
        assert!(!app.world().contains_resource::<RemoteClient>());
        for _ in 0..8 {
            server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
        }
        assert!(server.session().world().player(id).is_none());
    }
    #[test]
    fn reliable_effects_arrive_once_after_silent_prediction() {
        let (mut app, mut server, _) = setup();
        let id = app.world().resource::<crate::player::LocalPlayer>().id;
        app.world_mut()
            .resource_mut::<crate::player::LocalPlayer>()
            .latched = movement::buttons::JUMP;
        app.update();
        assert!(
            app.world()
                .resource::<crate::effects::PendingEvents>()
                .0
                .is_empty()
        );
        assert!(
            app.world()
                .resource::<RemoteClient>()
                .session
                .predicted_world()
                .unwrap()
                .events
                .is_empty()
        );
        for _ in 0..16 {
            server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
            app.update();
        }
        let events = &app.world().resource::<crate::effects::PendingEvents>().0;
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, sim::GameEvent::Jump { player } if *player == id))
                .count(),
            1
        );
    }
    #[test]
    fn downstream_effect_overflow_closes_and_clears_the_session() {
        let (mut app, mut server, _) = setup();
        let id = app.world().resource::<crate::player::LocalPlayer>().id;
        app.world_mut()
            .resource_mut::<crate::effects::PendingEvents>()
            .0 =
            vec![sim::GameEvent::Jump { player: id }; netcode::remote_session::MAX_QUEUED_EVENTS];
        app.world_mut()
            .resource_mut::<crate::player::LocalPlayer>()
            .latched = movement::buttons::JUMP;
        for _ in 0..16 {
            app.update();
            server.update(Duration::from_secs_f64(1. / 128.)).unwrap();
            if matches!(
                app.world().resource::<RemoteClient>().status(),
                Status::Closed(_)
            ) {
                break;
            }
        }
        assert_eq!(
            app.world().resource::<RemoteClient>().status(),
            &Status::Closed(Error::EventOverflow)
        );
        app.world_mut().resource_mut::<RemoteClient>().cancel();
        assert_eq!(
            app.world().resource::<RemoteClient>().status(),
            &Status::Closed(Error::EventOverflow)
        );
        assert!(
            app.world()
                .resource::<crate::effects::PendingEvents>()
                .0
                .is_empty()
        );
        assert!(app.world().resource::<RemoteClient>().frame.local.is_none());
    }
    #[test]
    fn invalid_local_controls_close_with_original_typed_error_and_no_old_effects() {
        let (mut app, _server, _) = setup();
        let id = app.world().resource::<crate::player::LocalPlayer>().id;
        app.world_mut()
            .resource_mut::<crate::effects::PendingEvents>()
            .0
            .push(sim::GameEvent::Jump { player: id });
        app.world_mut()
            .resource_mut::<crate::player::LocalPlayer>()
            .pitch = f32::NAN;
        app.update();
        assert!(matches!(
            app.world().resource::<RemoteClient>().status(),
            Status::Closed(Error::InvalidInput(_))
        ));
        let reason = app.world().resource::<RemoteClient>().status().clone();
        app.world_mut().resource_mut::<RemoteClient>().cancel();
        assert_eq!(app.world().resource::<RemoteClient>().status(), &reason);
        assert!(
            app.world()
                .resource::<crate::effects::PendingEvents>()
                .0
                .is_empty()
        );
        assert!(app.world().resource::<RemoteClient>().frame.local.is_none());
    }
    #[test]
    fn rediscovery_clears_publication_and_same_tick_baseline_is_restored() {
        let (mut app, server, _) = setup();
        let id = app.world().resource::<crate::player::LocalPlayer>().id;
        let tick = app
            .world()
            .resource::<RemoteClient>()
            .session
            .authoritative_state()
            .unwrap()
            .tick;
        let probe = RemoteSession::discover(
            server.address(),
            "Probe".into(),
            sim::Team::CounterTerrorist,
        )
        .unwrap();
        // Retain a real admitted baseline to model rejoining a replacement
        // session whose initial tick equals the previous publication's tick.
        let admitted = {
            let mut client = app.world_mut().resource_mut::<RemoteClient>();
            client.frame.local_tracked = Some(sim::TrackedPose::default());
            client.frame.local_aim = Some(sim::AimRay::default());
            std::mem::replace(&mut client.session, probe)
        };
        app.world_mut()
            .resource_mut::<crate::effects::PendingEvents>()
            .0
            .push(sim::GameEvent::Jump { player: id });
        app.world_mut()
            .resource_mut::<crate::player::SimWorld>()
            .0
            .player_mut(id)
            .unwrap()
            .money = 1;
        app.world_mut()
            .resource_mut::<crate::player::LocalPlayer>()
            .yaw = 42.;
        app.update();
        let client = app.world().resource::<RemoteClient>();
        assert_eq!(client.status(), &Status::Discovering);
        assert!(client.frame.local.is_none());
        assert!(client.frame.local_tracked.is_none());
        assert!(client.frame.local_aim.is_none());
        assert!(client.frame.remote.is_empty());
        assert!(client.last_advance.is_none());
        assert!(client.last_published.is_none());
        assert_eq!(client.snapshot_age, 0.);
        assert!(
            app.world()
                .resource::<crate::effects::PendingEvents>()
                .0
                .is_empty()
        );
        app.world_mut().resource_mut::<RemoteClient>().session = admitted;
        app.update();
        let client = app.world().resource::<RemoteClient>();
        assert_eq!(client.status(), &Status::Ready);
        assert_eq!(client.session.authoritative_state().unwrap().tick, tick);
        assert_eq!(
            app.world().resource::<crate::player::SimWorld>().0.state(),
            *client.session.authoritative_state().unwrap()
        );
        assert_eq!(
            app.world().resource::<crate::player::LocalPlayer>().yaw,
            client
                .session
                .authoritative_state()
                .unwrap()
                .players
                .iter()
                .find(|p| p.id == id)
                .unwrap()
                .view_angles
                .y
        );
    }
}

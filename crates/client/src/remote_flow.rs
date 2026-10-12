//! Opt-in, loopback-only engine proof. Captures and player data stay private.
use crate::{
    remote::RemoteClient,
    remote_join::{Connect, JoinRequest},
    session::AppState,
};
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
};
use std::time::{Duration, Instant};

pub(crate) fn enabled() -> bool {
    std::env::var_os("CSR_TEST_REMOTE").is_some()
}

pub(crate) struct Proof {
    start: Instant,
    entered: Instant,
    ready_at: Option<Instant>,
    phase: u8,
    tick: u64,
}
impl Default for Proof {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            entered: Instant::now(),
            ready_at: None,
            phase: 0,
            tick: 0,
        }
    }
}
impl Proof {
    fn next(&mut self) {
        self.phase += 1;
        self.entered = Instant::now();
        info!("remote flow: phase {}", self.phase);
    }
}

#[derive(Resource, Default)]
struct Captures(u8);

fn capture(world: &mut World, name: &str) {
    if !world.contains_resource::<Captures>() {
        world.init_resource::<Captures>();
    }
    let destination = world
        .resource::<crate::Options>()
        .0
        .screenshot
        .as_ref()
        .unwrap()
        .with_file_name(name);
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(destination))
        .observe(|_: On<ScreenshotCaptured>, mut captures: ResMut<Captures>| captures.0 += 1);
}
fn failed(world: &mut World, message: &str) {
    error!("remote flow: {message}; failures: 1");
    world.write_message(AppExit::Error(std::num::NonZeroU8::new(7).unwrap()));
}

pub(crate) fn update(world: &mut World, mut proof: Local<Proof>) {
    // Each hidden loopback window owns its engine inputs independently. This
    // fixture cannot claim simultaneous operating-system window focus.
    if let Ok(mut window) = world
        .query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>()
        .single_mut(world)
    {
        window.focused = true;
    }
    if proof.start.elapsed() > Duration::from_secs(180) {
        error!(
            "remote flow: phase {}, scene {:?}, menu open {}, PauseMenu loaded {}, virtual paused {}",
            proof.phase,
            world.resource::<State<AppState>>().get(),
            world.resource::<crate::menu::Menu>().open(),
            world
                .resource::<crate::frontend::Frontend>()
                .loaded("PauseMenu"),
            world.resource::<Time<Virtual>>().is_paused()
        );
        failed(world, "timed out");
        return;
    }
    let state = *world.resource::<State<AppState>>().get();
    let ready = world
        .get_resource::<RemoteClient>()
        .is_some_and(|client| client.status() == &netcode::remote_session::Status::Ready);
    match proof.phase {
        0 => {
            if !world.resource::<crate::frontend::Frontend>().ready || state != AppState::Menu {
                return;
            }
            if proof.ready_at.get_or_insert_with(Instant::now).elapsed() < Duration::from_secs(5) {
                return;
            }
            let Ok(address) = std::env::var("CSR_TEST_REMOTE")
                .unwrap()
                .parse::<std::net::SocketAddr>()
            else {
                failed(world, "invalid loopback endpoint");
                return;
            };
            if !address.ip().is_loopback() {
                failed(world, "proof requires a loopback endpoint");
                return;
            }
            let name =
                std::env::var("CSR_TEST_REMOTE_NAME").unwrap_or_else(|_| "Rendered client".into());
            info!("remote flow: loopback engine-input fixture for {name}; no OS focus/input claim");
            *world.resource::<JoinRequest>().0.lock().unwrap() = Some(Connect {
                address,
                name,
                team: sim::Team::CounterTerrorist,
            });
            proof.next();
        }
        1 if ready => {
            let local = world.resource::<crate::player::LocalPlayer>();
            let client = world.resource::<RemoteClient>();
            if local.id == 0 || client.frame().local.is_none() {
                failed(world, "authority did not publish a local actor");
                return;
            }
            proof.tick = world.resource::<crate::player::SimWorld>().0.tick;
            proof.next();
        }
        2 if ready && proof.entered.elapsed() > Duration::from_secs(2) => {
            let world_state = &world.resource::<crate::player::SimWorld>().0;
            if world_state.tick <= proof.tick || world_state.players.len() < 2 {
                failed(world, "authority or remote actor did not advance");
                return;
            }
            info!(
                "remote flow: authoritative {} Hz, {} actors, tick {}",
                world_state.tick_rate,
                world_state.players.len(),
                world_state.tick
            );
            capture(world, "remote-ready.png");
            proof.next();
        }
        3 if ready
            && world
                .get_resource::<Captures>()
                .is_some_and(|captures| captures.0 >= 1)
            && proof.entered.elapsed() > Duration::from_millis(250) =>
        {
            proof.tick = world.resource::<crate::player::SimWorld>().0.tick;
            // Screenshot runs disable controls by default. Enable the normal
            // menu-input path so this phase actually exercises pause ownership.
            world.resource_mut::<crate::player::InputEnabled>().0 = true;
            world.resource_mut::<crate::menu::Menu>().page = Some(crate::menu::Page::Main);
            world.resource_mut::<crate::player::LocalPlayer>().latched = movement::buttons::JUMP;
            proof.next();
        }
        4 if ready
            && world
                .resource::<crate::frontend::Frontend>()
                .loaded("PauseMenu")
            && proof.entered.elapsed() > Duration::from_secs(5) =>
        {
            if world.resource::<crate::player::SimWorld>().0.tick <= proof.tick
                || world.resource::<Time<Virtual>>().is_paused()
                || world.resource::<crate::player::LocalPlayer>().latched != 0
            {
                error!(
                    "remote pause proof: tick {} -> {}, virtual paused {}, latched {}",
                    proof.tick,
                    world.resource::<crate::player::SimWorld>().0.tick,
                    world.resource::<Time<Virtual>>().is_paused(),
                    world.resource::<crate::player::LocalPlayer>().latched
                );
                failed(world, "pause stopped authority or retained gameplay input");
                return;
            }
            capture(world, "remote-paused.png");
            proof.next();
        }
        5 if world
            .get_resource::<Captures>()
            .is_some_and(|captures| captures.0 >= 2)
            && proof.entered.elapsed() > Duration::from_millis(250) =>
        {
            let mut menu = world.resource_mut::<crate::menu::Menu>();
            menu.in_match = false;
            menu.page = Some(crate::menu::Page::Main);
            proof.next();
        }
        6 if state == AppState::Menu && proof.entered.elapsed() > Duration::from_secs(3) => {
            if world.contains_resource::<RemoteClient>()
                || world.contains_resource::<crate::LoadedLevel>()
                || world.contains_resource::<crate::player::SimWorld>()
                || world.contains_resource::<crate::remote_join::Admission>()
            {
                failed(world, "Home retained remote or match resources");
                return;
            }
            capture(world, "remote-returned-home.png");
            proof.next();
        }
        7 if world
            .get_resource::<Captures>()
            .is_some_and(|captures| captures.0 >= 3)
            && proof.entered.elapsed() > Duration::from_millis(250) =>
        {
            let options = crate::ClientOptions {
                map: "de_dust2".into(),
                bots: 0,
                deathmatch: true,
                ..world.resource::<crate::Options>().0.clone()
            };
            *world
                .resource::<crate::menu::LoadRequest>()
                .0
                .lock()
                .unwrap() = Some(options.into());
            proof.ready_at = None;
            proof.next();
        }
        8 if state == AppState::Match
            && !world.contains_resource::<RemoteClient>()
            && proof.entered.elapsed() > Duration::from_secs(2) =>
        {
            if proof.ready_at.get_or_insert_with(Instant::now).elapsed() < Duration::from_secs(2) {
                return;
            }
            if world.resource::<crate::player::SimWorld>().0.players.len() != 1 {
                failed(world, "offline reload retained remote actors");
                return;
            }
            capture(world, "remote-offline-reload.png");
            proof.next();
        }
        9 if world
            .get_resource::<Captures>()
            .is_some_and(|captures| captures.0 == 4)
            && proof.entered.elapsed() > Duration::from_secs(1) =>
        {
            info!(
                "remote flow: discovery -> exact local map -> prepared render -> authority -> paused authority -> Home -> offline reload; failures: 0"
            );
            world.write_message(AppExit::Success);
            proof.next();
        }
        _ => (),
    }
}

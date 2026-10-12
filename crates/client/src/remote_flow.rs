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

fn authored_deathmatch() -> bool {
    std::env::var("CSR_TEST_HOST_MENU").as_deref() == Ok("deathmatch")
}

pub(crate) fn enabled() -> bool {
    std::env::var_os("CSR_TEST_REMOTE").is_some() || std::env::var_os("CSR_TEST_HOST").is_some()
}

pub(crate) struct Proof {
    start: Instant,
    entered: Instant,
    ready_at: Option<Instant>,
    phase: u8,
    tick: u64,
    presented_serial: u64,
    host_menu: AuthoredHost,
}
impl Default for Proof {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            entered: Instant::now(),
            ready_at: None,
            phase: 0,
            tick: 0,
            presented_serial: 0,
            host_menu: AuthoredHost::default(),
        }
    }
}

/// The installed movies receive pointer inputs, not calls to their controls.
/// Production hosting keeps its normal port and 128-tick defaults.
#[derive(Default)]
struct AuthoredHost {
    step: usize,
    settled: Option<(Instant, u64)>,
    captures_requested: u8,
}
#[derive(Resource, Default)]
struct AuthoredCaptures(u8);
fn capture_authored(world: &mut World, name: &str, flag: u8) {
    world.init_resource::<AuthoredCaptures>();
    let path = world
        .resource::<crate::Options>()
        .0
        .screenshot
        .as_ref()
        .unwrap()
        .with_file_name(name);
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(
            move |_: On<ScreenshotCaptured>, mut captures: ResMut<AuthoredCaptures>| {
                captures.0 |= flag
            },
        );
}
impl AuthoredHost {
    fn update(&mut self, world: &mut World) -> bool {
        let front = world.resource::<crate::frontend::Frontend>();
        use crate::frontend_lobby::Phase;
        let (expected, phase) = match self.step {
            0 | 1 | 6 | 7 => ("MainMenu", Phase::Closed),
            2 | 8 => ("LobbyBrowser", Phase::Browser),
            3 | 5 | 9 => ("LobbyScreen", Phase::Lobby),
            4 | 10..=13 => ("StartSinglePlayer", Phase::Picker),
            _ => ("LobbyScreen", Phase::Lobby),
        };
        if !front.movie_ready(expected)
            || front.lobby_phase() != phase
            || (self.step >= 14 && !front.lobby_staged())
        {
            self.settled = None;
            return false;
        }
        let serial = front.presented_serial();
        let (since, first_frame) = self.settled.get_or_insert((Instant::now(), serial));
        if since.elapsed() < Duration::from_secs(2) || serial.saturating_sub(*first_frame) < 2 {
            return false;
        }
        let capture = match self.step {
            5 => Some(("host-authored-cancelled-picker.png", 1)),
            6 => Some(("host-authored-cancelled-home.png", 2)),
            11 => Some(("host-authored-picker.png", 4)),
            _ => None,
        };
        if let Some((name, flag)) = capture {
            if self.captures_requested & flag == 0 {
                capture_authored(world, name, flag);
                self.captures_requested |= flag;
                self.settled = None;
                return false;
            }
        }
        let captures = world
            .get_resource::<AuthoredCaptures>()
            .map_or(0, |captures| captures.0);
        if capture.is_some_and(|(_, flag)| captures & flag == 0)
            || (self.step == 15 && captures & 8 == 0)
        {
            return false;
        }
        let points = [
            (486., 26.),   // Play
            (503., 138.),  // Play with friends
            (1390., 970.), // Create lobby
            (758., 879.),  // Settings
            (1615., 172.), // Cancel the first picker through its original X
            (333., 26.),   // Cancel the first lobby through original Home
            (486., 26.),   // Reopen Play
            (503., 138.),  // Play with friends
            (1390., 970.), // Reopen the cached lobby
            (758., 879.),  // Reopen the cached picker
            (if authored_deathmatch() { 850. } else { 1450. }, 206.),
            // Each mode's filtered grid was inspected in a private capture.
            (810., if authored_deathmatch() { 500. } else { 680. }),
            (1450., 1017.), // Accept
            (1164., 754.),  // No Bots modal: Go
            (1430., 1004.), // Lobby: Go
        ];
        if self.step == 14 {
            capture_authored(world, "host-authored-lobby.png", 8);
            self.step += 1;
            self.settled = None;
            return false;
        }
        let (x, y) = points[self.step.min(14)];
        let window = world
            .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
            .single(world)
            .unwrap();
        let (x, y) = (
            x * window.physical_width() as f64 / 1920.,
            y * window.physical_height() as f64 / 1080.,
        );
        let front = world.resource::<crate::frontend::Frontend>();
        front.send(scaleform::Input::MouseMove { x, y });
        front.send(scaleform::Input::MouseDown {
            x,
            y,
            button: scaleform::MouseButton::Left,
        });
        front.send(scaleform::Input::MouseUp {
            x,
            y,
            button: scaleform::MouseButton::Left,
        });
        info!("remote flow: authored host pointer step {}", self.step);
        self.step += 1;
        self.settled = None;
        self.step == points.len() + 1
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

fn guest_capture_ready(key: &str) -> bool {
    std::env::var_os(key).is_none_or(|path| {
        let path = std::path::PathBuf::from(path);
        path.is_absolute() && path.is_file()
    })
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
        .is_some_and(|client| client.gameplay_ready());
    match proof.phase {
        0 => {
            if !world.resource::<crate::frontend::Frontend>().ready || state != AppState::Menu {
                return;
            }
            if proof.ready_at.get_or_insert_with(Instant::now).elapsed() < Duration::from_secs(5) {
                return;
            }
            let hosted = std::env::var_os("CSR_TEST_HOST").is_some();
            let Ok(address) = std::env::var(if hosted {
                "CSR_TEST_HOST"
            } else {
                "CSR_TEST_REMOTE"
            })
            .unwrap()
            .parse::<std::net::SocketAddr>() else {
                failed(world, "invalid loopback endpoint");
                return;
            };
            if !address.ip().is_loopback() {
                failed(world, "proof requires a loopback endpoint");
                return;
            }
            let name =
                std::env::var("CSR_TEST_REMOTE_NAME").unwrap_or_else(|_| "Rendered client".into());
            if hosted && std::env::var_os("CSR_TEST_HOST_MENU").is_some() {
                if address.port() != 27015 {
                    failed(world, "authored host proof uses the production port 27015");
                    return;
                }
                if proof.host_menu.step >= 14
                    && world.resource::<crate::frontend::Frontend>().lobby_staged()
                    && world
                        .resource::<crate::frontend::Frontend>()
                        .lobby_staged_map()
                        .as_deref()
                        != Some("de_dust2")
                {
                    failed(world, "authored picker did not stage Dust II");
                    return;
                }
                if proof.host_menu.update(world) {
                    proof.next();
                }
                return;
            }
            info!("remote flow: loopback engine-input fixture for {name}; no OS focus/input claim");
            if hosted {
                let map = crate::ClientOptions {
                    bots: 1,
                    deathmatch: true,
                    ..world.resource::<crate::Options>().0.clone()
                };
                let tick_rate = std::env::var("CSR_TEST_HOST_RATE")
                    .ok()
                    .and_then(|rate| rate.parse().ok())
                    .unwrap_or(128);
                *world
                    .resource::<crate::hosting::HostRequest>()
                    .0
                    .lock()
                    .unwrap() = Some(crate::hosting::HostSelection {
                    map: map.into(),
                    config: netcode::host::HostConfig {
                        bind: address,
                        max_clients: 4,
                    },
                    tick_rate,
                    name,
                    team: sim::Team::CounterTerrorist,
                });
            } else {
                *world.resource::<JoinRequest>().0.lock().unwrap() = Some(Connect {
                    address,
                    name,
                    team: sim::Team::CounterTerrorist,
                });
            }
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
            if std::env::var_os("CSR_TEST_HOST").is_some()
                && !guest_capture_ready("CSR_TEST_HOST_GUEST_READY")
            {
                return;
            }
            let world_state = &world.resource::<crate::player::SimWorld>().0;
            let minimum_actors = if std::env::var_os("CSR_TEST_HOST_MENU").is_some() {
                1
            } else {
                2
            };
            if world_state.tick <= proof.tick || world_state.players.len() < minimum_actors {
                failed(world, "authority or remote actor did not advance");
                return;
            }
            if std::env::var_os("CSR_TEST_REMOTE_REQUIRE_ALIVE").is_some() {
                if world_state.mode != sim::GameMode::Deathmatch {
                    failed(
                        world,
                        "active-player proof did not receive Deathmatch rules",
                    );
                    return;
                }
                let local = world.resource::<crate::player::LocalPlayer>().id;
                if !world_state.player(local).is_some_and(|player| player.alive)
                    || world_state
                        .players
                        .iter()
                        .filter(|player| !player.is_bot && player.alive)
                        .count()
                        < 2
                {
                    return;
                }
            }
            if std::env::var_os("CSR_TEST_HOST_GUEST_READY").is_some()
                && world_state
                    .players
                    .iter()
                    .filter(|player| !player.is_bot)
                    .count()
                    < 2
            {
                failed(
                    world,
                    "guest capture did not correspond to a second authoritative human",
                );
                return;
            }
            info!(
                "remote flow: authoritative {} Hz, {} actors, {} active humans, tick {}",
                world_state.tick_rate,
                world_state.players.len(),
                world_state
                    .players
                    .iter()
                    .filter(|player| !player.is_bot && player.alive)
                    .count(),
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
            if std::env::var_os("CSR_TEST_HOST").is_some()
                && !guest_capture_ready("CSR_TEST_HOST_GUEST_DONE")
            {
                return;
            }
            if std::env::var_os("CSR_TEST_HOST_GUEST_DONE").is_some()
                && world
                    .resource::<crate::player::SimWorld>()
                    .0
                    .players
                    .iter()
                    .filter(|player| !player.is_bot)
                    .count()
                    != 1
            {
                failed(world, "departed guest remained in the hosting authority");
                return;
            }
            proof.tick = world.resource::<crate::player::SimWorld>().0.tick;
            // Screenshot runs disable controls by default. Enable the normal
            // menu-input path so this phase actually exercises pause ownership.
            world.resource_mut::<crate::player::InputEnabled>().0 = true;
            world.resource_mut::<crate::menu::Menu>().page = Some(crate::menu::Page::Main);
            world.resource_mut::<crate::player::LocalPlayer>().latched = movement::buttons::JUMP;
            proof.ready_at = None;
            proof.next();
        }
        4 if ready
            && world
                .resource::<crate::frontend::Frontend>()
                .loaded("PauseMenu") =>
        {
            let serial = world
                .resource::<crate::frontend::Frontend>()
                .presented_serial();
            if proof.ready_at.is_none() {
                proof.ready_at = Some(Instant::now());
                proof.presented_serial = serial;
            }
            // Loaded is emitted during the worker's step, before its first
            // rendered frame. Wait from that observation, then require actual
            // uploaded frames rather than capturing the outgoing menu image.
            if proof.ready_at.unwrap().elapsed() < Duration::from_secs(5)
                || serial.saturating_sub(proof.presented_serial) < 2
            {
                return;
            }
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
            if crate::host_sharing_ui::proof_enabled() {
                match crate::host_sharing_ui::prove_copy(world) {
                    Ok(true) => (),
                    Ok(false) => return,
                    Err(error) => { failed(world, &error); return; }
                }
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
            proof.ready_at = None;
            proof.next();
        }
        6 if state == AppState::Menu => {
            if !world.resource::<crate::hosting::Hosting>().closed() {
                return;
            }
            let serial = world
                .resource::<crate::frontend::Frontend>()
                .presented_serial();
            if proof.ready_at.is_none() {
                proof.ready_at = Some(Instant::now());
                proof.presented_serial = serial;
            }
            if proof.ready_at.unwrap().elapsed() < Duration::from_secs(3)
                || serial.saturating_sub(proof.presented_serial) < 2
            {
                return;
            }
            if world.contains_resource::<RemoteClient>()
                || world.contains_resource::<crate::LoadedLevel>()
                || world.contains_resource::<crate::player::SimWorld>()
                || world.contains_resource::<crate::remote_join::Admission>()
            {
                failed(world, "Home retained remote or match resources");
                return;
            }
            if crate::host_sharing_ui::proof_enabled() {
                let receipt = world.resource::<crate::host_sharing_ui::Ui>().receipt();
                if receipt.host_generation.is_some() || !receipt.candidates.is_empty() {
                    failed(world, "Home retained host address receipts");
                    return;
                }
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

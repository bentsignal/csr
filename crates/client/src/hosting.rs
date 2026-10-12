//! Frontend ownership of an in-process server. One loader prepares the map;
//! the existing remote render gate admits the host before authority starts.
use bevy::prelude::*;
use netcode::host::{HostConfig, HostHandle, HostStatus};
use std::sync::Mutex;

#[derive(Clone)]
pub struct HostSelection {
    pub map: crate::menu::MapRequest,
    pub config: HostConfig,
    pub tick_rate: u32,
    pub name: String,
    pub team: sim::Team,
}
impl HostSelection {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !matches!(self.tick_rate, 64 | 128) {
            return Err("Choose a supported server tick rate".into());
        }
        if !(1..=64).contains(&self.config.max_clients) {
            return Err("Choose between 1 and 64 connection slots".into());
        }
        let bots =
            u64::from(self.map.options.bots) + u64::from(self.map.options.bots.saturating_sub(1));
        if bots + self.config.max_clients as u64 > 64 {
            return Err("The selected bots and connection slots exceed the server limit".into());
        }
        Ok(())
    }
}

#[derive(Resource, Default)]
pub struct HostRequest(pub Mutex<Option<HostSelection>>);

pub(crate) struct PreparedHost {
    pub selection: HostSelection,
    pub authority: sim::World,
    pub map_world: sim::World,
    pub manifest: netcode::protocol::ContentManifest,
}

#[derive(Resource, Default)]
pub(crate) struct Hosting {
    handle: Option<HostHandle>,
    staged: Option<(u64, crate::session::LoadedMap, HostSelection, sim::World)>,
    serial: Option<u64>,
    exit: Option<AppExit>,
}
impl Hosting {
    #[cfg(test)]
    pub(crate) fn retain_generation_for_test(&mut self, serial: u64) {
        self.serial = Some(serial);
    }
    pub(crate) fn cancel(&mut self) {
        self.staged = None;
        self.serial = None;
        if let Some(handle) = &self.handle {
            handle.cancel();
        }
    }

    fn reap(&mut self) {
        // Reaping a panicked worker publishes its typed Closed status. Poll
        // live owners too, retaining the handle until update consumes that status.
        let finished = self
            .handle
            .as_mut()
            .is_some_and(HostHandle::reap_if_finished);
        if self.serial.is_none() && finished {
            self.handle = None;
        }
    }

    /// A live server's actual socket; the private admission address remains separate.
    pub(crate) fn share_binding(&self) -> Option<(u64, std::net::SocketAddr)> {
        if self.exit.is_some() {
            return None;
        }
        let serial = self.serial?;
        match self.handle.as_ref()?.status() {
            HostStatus::Preparing { bound_address, .. }
            | HostStatus::Running { bound_address, .. } => Some((serial, bound_address)),
            _ => None,
        }
    }

    pub(crate) fn preparing(&self) -> bool {
        self.serial.is_some()
    }

    pub(crate) fn closed(&self) -> bool {
        self.serial.is_none() && self.handle.is_none() && self.staged.is_none()
    }

    pub(crate) fn exiting(&self) -> bool {
        self.exit.is_some()
    }

    pub(crate) fn stage(
        &mut self,
        serial: u64,
        mut loaded: crate::session::LoadedMap,
    ) -> Result<(), String> {
        self.reap();
        if self.handle.is_some() || self.exit.is_some() {
            return Err("The previous server is still closing. Try again.".into());
        }
        let prepared = loaded.host.take().ok_or("Missing prepared host")?;
        let handle = HostHandle::spawn(
            prepared.authority,
            prepared.manifest,
            prepared.selection.config,
        )
        .map_err(|error| host_error(&error))?;
        self.handle = Some(handle);
        self.serial = Some(serial);
        self.staged = Some((serial, loaded, prepared.selection, prepared.map_world));
        Ok(())
    }
}

pub(crate) fn request(world: &mut World) {
    let selected = world.resource::<HostRequest>().0.lock().unwrap().take();
    let Some(selected) = selected else { return };
    if world.resource::<Hosting>().exit.is_some() {
        return;
    }
    crate::remote_join::home(world);
    world.resource_mut::<crate::Options>().0 = selected.map.options.clone();
    world
        .resource_mut::<crate::session::Loader>()
        .select_host(selected);
    world
        .resource_mut::<NextState<crate::session::AppState>>()
        .set(crate::session::AppState::Loading);
}

pub(crate) fn update(world: &mut World) {
    world.resource_mut::<Hosting>().reap();
    let Some(status) = world
        .resource::<Hosting>()
        .handle
        .as_ref()
        .map(HostHandle::status)
    else {
        return;
    };
    // A cancelled handle remains owned until reaped, but cannot publish a map.
    if world.resource::<Hosting>().serial.is_none() {
        return;
    }
    match status {
        HostStatus::Binding => {
            world.resource_mut::<crate::session::Loader>().status = "Starting server…".into();
        }
        HostStatus::Preparing { .. } => {
            let offer = world
                .resource_mut::<Hosting>()
                .handle
                .as_mut()
                .unwrap()
                .take_local_admission();
            if let Some(offer) = offer {
                let info = offer.server_info().clone();
                let (serial, mut loaded, selected, map_world) =
                    world.resource_mut::<Hosting>().staged.take().unwrap();
                match crate::remote::RemoteClient::hosted(offer, selected.name, selected.team) {
                    Ok(client) => {
                        loaded.remote = Some(crate::remote_join::Admission::new(
                            serial,
                            info.clone(),
                            map_world,
                            info.content.clone(),
                        ));
                        world
                            .resource_mut::<crate::session::Loader>()
                            .bind_host_generation(serial, info);
                        *world.resource::<crate::PendingLevel>().0.lock().unwrap() = Some(loaded);
                        world.insert_resource(client);
                        world
                            .resource_mut::<NextState<crate::session::AppState>>()
                            .set(crate::session::AppState::Match);
                    }
                    Err(error) => {
                        fail(world, crate::remote_join::error_message(&error));
                        return;
                    }
                }
            }
            let ready = world
                .get_resource::<crate::remote::RemoteClient>()
                .is_some_and(|client| client.status() == &netcode::remote_session::Status::Ready);
            if ready {
                let result = world.resource_scope(|world, mut hosting: Mut<Hosting>| {
                    world
                        .resource::<crate::remote::RemoteClient>()
                        .begin_host(hosting.handle.as_mut().unwrap())
                });
                if let Err(error) = result {
                    fail(world, host_error(&error));
                }
            }
        }
        HostStatus::Running { .. } => {
            if let Some(mut client) = world.get_resource_mut::<crate::remote::RemoteClient>() {
                client.mark_host_running();
            }
        }
        HostStatus::Closed(error) => fail(world, host_error(&error)),
        HostStatus::Stopping => (),
    }
}

fn fail(world: &mut World, message: String) {
    crate::remote_join::home(world);
    world.resource_mut::<crate::menu::Menu>().status = message;
    world.resource_mut::<crate::menu::Menu>().in_match = false;
    world
        .resource_mut::<NextState<crate::session::AppState>>()
        .set(crate::session::AppState::Menu);
}

fn host_error(error: &netcode::host::HostError) -> String {
    use netcode::host::HostError;
    match error {
        HostError::Bind(_) => {
            "Could not open the server port. Another game or server may be using it."
        }
        HostError::InvalidAuthority(_) => "The selected server settings are not supported.",
        HostError::OwnerAdmissionTimeout => "Joining your server timed out. Please try again.",
        HostError::OwnerDisconnected => "Your connection to the server was lost.",
        HostError::Cancelled => "Hosting cancelled.",
        _ => "The server could not continue. Return to Play and try again.",
    }
    .into()
}

pub(crate) fn gate_exit(world: &mut World) {
    let exits: Vec<_> = world.resource_mut::<Messages<AppExit>>().drain().collect();
    let first = !world.resource::<Hosting>().exiting();
    {
        let mut hosting = world.resource_mut::<Hosting>();
        for exit in exits {
            if hosting.exit.as_ref().is_none_or(|old| !old.is_error()) {
                hosting.exit = Some(exit);
            }
        }
    }
    let Some(exit) = world.resource::<Hosting>().exit.clone() else {
        return;
    };
    if first {
        crate::remote_join::home(world);
    }
    let mut hosting = world.resource_mut::<Hosting>();
    hosting.cancel();
    hosting.reap();
    if hosting.handle.is_none() {
        world.write_message(exit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::UdpSocket,
        sync::Arc,
        time::{Duration, Instant},
    };

    fn authority() -> HostHandle {
        let manifest = netcode::protocol::ContentManifest {
            map_id: "csgo_map_host_lifetime".into(),
            map_name: "host_lifetime".into(),
            map_hash: "0".repeat(40),
            catalog_hash: "1".repeat(40),
        };
        HostHandle::spawn(
            sim::World::new(Arc::new(collision::CollisionWorld::new())),
            manifest,
            HostConfig {
                bind: "127.0.0.1:0".parse().unwrap(),
                max_clients: 4,
            },
        )
        .unwrap()
    }

    #[test]
    fn settings_bounds_reject_overflow_and_allow_exact_actor_capacity() {
        let mut selected = HostSelection {
            map: crate::ClientOptions {
                bots: 0,
                ..default()
            }
            .into(),
            config: HostConfig {
                max_clients: 64,
                ..default()
            },
            tick_rate: 128,
            name: "Host".into(),
            team: sim::Team::CounterTerrorist,
        };
        assert!(selected.validate().is_ok());
        for slots in [0, 65, usize::MAX] {
            selected.config.max_clients = slots;
            assert!(selected.validate().is_err());
        }
        selected.config.max_clients = 1;
        selected.map.options.bots = 32;
        assert!(selected.validate().is_ok());
        selected.map.options.bots = 33;
        assert!(selected.validate().is_err());
        selected.map.options.bots = u32::MAX;
        assert!(selected.validate().is_err());
        selected.map.options.bots = 0;
        selected.tick_rate = 0;
        assert!(selected.validate().is_err());
    }

    #[test]
    fn home_revokes_queued_host_and_join_intents_before_the_next_update() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(crate::session::AppState::Menu)
            .init_resource::<Hosting>()
            .init_resource::<HostRequest>()
            .init_resource::<crate::remote_join::JoinRequest>()
            .init_resource::<crate::menu::LoadRequest>()
            .add_systems(Update, (crate::remote_join::request, request).chain());
        *app.world().resource::<HostRequest>().0.lock().unwrap() = Some(HostSelection {
            map: crate::ClientOptions::default().into(),
            config: HostConfig::default(),
            tick_rate: 128,
            name: "Host".into(),
            team: sim::Team::CounterTerrorist,
        });
        *app.world()
            .resource::<crate::remote_join::JoinRequest>()
            .0
            .lock()
            .unwrap() = Some(crate::remote_join::Connect {
            address: "127.0.0.1:27015".parse().unwrap(),
            name: "Stale join".into(),
            team: sim::Team::CounterTerrorist,
        });
        crate::remote_join::home(app.world_mut());
        app.update();
        assert!(
            app.world()
                .resource::<HostRequest>()
                .0
                .lock()
                .unwrap()
                .is_none()
        );
        assert!(
            app.world()
                .resource::<crate::remote_join::JoinRequest>()
                .0
                .lock()
                .unwrap()
                .is_none()
        );
        assert!(
            !app.world()
                .contains_resource::<crate::remote::RemoteClient>()
        );
        assert!(app.world().resource::<Hosting>().closed());
    }

    #[test]
    fn cancelled_host_keeps_worker_owned_until_reaped_then_releases_socket() {
        let handle = authority();
        let deadline = Instant::now() + Duration::from_secs(2);
        let address = loop {
            if let HostStatus::Preparing { local_address, .. } = handle.status() {
                break local_address;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(UdpSocket::bind(address).is_err());
        let mut hosting = Hosting {
            handle: Some(handle),
            serial: Some(7),
            ..default()
        };
        hosting.cancel();
        assert!(hosting.handle.is_some());
        assert!(!hosting.preparing());
        while hosting.handle.is_some() {
            hosting.reap();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(UdpSocket::bind(address).is_ok());
    }

    #[test]
    fn process_exit_reaps_authority_and_retains_first_error_code() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Hosting {
                handle: Some(authority()),
                serial: Some(7),
                ..default()
            })
            .add_systems(Last, gate_exit);
        app.world_mut().write_message(AppExit::from_code(7));
        app.world_mut().write_message(AppExit::Success);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            app.update();
            if let Some(exit) = app.should_exit() {
                assert_eq!(exit, AppExit::from_code(7));
                assert!(app.world().resource::<Hosting>().handle.is_none());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_downstream_exit_barrier_cannot_keep_authority_running_or_reopen_hosting() {
        fn hold(mut messages: ResMut<Messages<AppExit>>) {
            messages.drain().for_each(drop);
        }
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Hosting {
                handle: Some(authority()),
                serial: Some(7),
                ..default()
            })
            .init_resource::<HostRequest>()
            .add_systems(Update, request)
            .add_systems(Last, (gate_exit, hold).chain());
        app.world_mut().write_message(AppExit::from_code(7));
        app.update();
        assert!(app.should_exit().is_none());
        assert_eq!(
            app.world().resource::<Hosting>().exit,
            Some(AppExit::from_code(7))
        );
        assert!(app.world().resource::<Hosting>().serial.is_none());
        if let Some(handle) = &app.world().resource::<Hosting>().handle {
            assert!(matches!(
                handle.status(),
                HostStatus::Stopping | HostStatus::Closed(_)
            ));
        }
        *app.world().resource::<HostRequest>().0.lock().unwrap() = Some(HostSelection {
            map: crate::ClientOptions::default().into(),
            config: HostConfig::default(),
            tick_rate: 128,
            name: "Late host".into(),
            team: sim::Team::CounterTerrorist,
        });
        app.update();
        assert!(app.world().resource::<Hosting>().serial.is_none());
        assert!(
            app.world()
                .resource::<HostRequest>()
                .0
                .lock()
                .unwrap()
                .is_none()
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.world().resource::<Hosting>().handle.is_some() {
            app.update();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

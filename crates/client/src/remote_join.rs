//! Frontend-owned remote lifetime. Disk preparation and render admission are
//! separate from the real-time network driver and authoritative gameplay.
use crate::{
    remote::RemoteClient,
    remote_render_gate::{GateStatus, GateTicket, RenderGate},
    session::{AppState, Loader},
};
use bevy::{asset::AssetId, prelude::*};
use netcode::{
    protocol::{ContentManifest, ServerInfo},
    remote_session::{Error, Status},
};
use std::{net::SocketAddr, path::PathBuf, sync::Mutex};

#[derive(Clone)]
pub struct Connect {
    pub address: SocketAddr,
    pub name: String,
    pub team: sim::Team,
}

/// One selected endpoint; the browser resolves address text before requesting
/// a join. This resource never contains a remote path or download URL.
#[derive(Resource, Default)]
pub struct JoinRequest(pub Mutex<Option<Connect>>);

pub(crate) fn local_stores() -> Result<(packs::Installed, workshop::Store), String> {
    let current = std::env::current_dir().map_err(|e| format!("Could not locate player data: {e}"))?;
    let data = match std::env::var_os("CSR_DATA_DIR") {
        Some(path) => PathBuf::from(path),
        None => current.clone(),
    };
    let data = if data.is_absolute() { data } else { current.join(data) };
    Ok((packs::Installed::new(&data), workshop::Store::new(&data)))
}

#[derive(Resource)]
pub(crate) struct Admission {
    pub serial: u64,
    pub info: ServerInfo,
    pub world: Option<sim::World>,
    pub manifest: ContentManifest,
    pub meshes: Vec<AssetId<Mesh>>,
    pub images: Vec<AssetId<Image>>,
    pub assets_captured: bool,
    ticket: Option<GateTicket>,
}
impl Admission {
    pub fn new(
        serial: u64,
        info: ServerInfo,
        mut world: sim::World,
        manifest: ContentManifest,
    ) -> Self {
        world.set_tick_rate(info.tick_rate);
        Self {
            serial,
            info,
            world: Some(world),
            manifest,
            meshes: Vec::new(),
            images: Vec::new(),
            assets_captured: false,
            ticket: None,
        }
    }
}

pub(crate) fn ready(
    state: Res<State<AppState>>,
    menu: Res<crate::menu::Menu>,
    remote: Option<Res<RemoteClient>>,
) -> bool {
    *state.get() == AppState::Match
        && menu.in_match
        && remote
            .as_ref()
            .is_none_or(|client| client.status() == &Status::Ready)
}
pub(crate) fn offline(remote: Option<Res<RemoteClient>>) -> bool {
    remote.is_none()
}
pub(crate) fn loading(remote: Option<&RemoteClient>) -> bool {
    remote.is_some_and(|client| client.status() != &Status::Ready)
}

fn cancel_preparation(world: &mut World) {
    if let Some(mut gate) = world.get_resource_mut::<RenderGate>() {
        gate.cancel();
    }
    if let Some(admission) = world.remove_resource::<Admission>() {
        if let Some(ticket) = admission.ticket {
            ticket.cancel();
        }
    }
    if let Some(pending) = world.get_resource::<crate::PendingLevel>() {
        *pending.0.lock().unwrap() = None;
    }
}

pub(crate) fn home(world: &mut World) {
    cancel_preparation(world);
    crate::remote::clear(world);
    if let Some(mut loader) = world.get_resource_mut::<Loader>() {
        loader.invalidate();
    }
}

fn fail(world: &mut World, message: String) {
    home(world);
    if let Some(mut menu) = world.get_resource_mut::<crate::menu::Menu>() {
        menu.status = message;
        menu.in_match = false;
    }
    world
        .resource_mut::<NextState<AppState>>()
        .set(AppState::Menu);
}

pub(crate) fn request(world: &mut World) {
    let selected = world.resource::<JoinRequest>().0.lock().unwrap().take();
    let Some(selected) = selected else {
        return;
    };
    home(world);
    match RemoteClient::discover(selected.address, selected.name, selected.team) {
        Ok(client) => {
            world.insert_resource(client);
            world.resource_mut::<crate::menu::Menu>().status.clear();
            world
                .resource_mut::<NextState<AppState>>()
                .set(AppState::Loading);
        }
        Err(error) => fail(world, error_message(&error)),
    }
}

/// Schedule immediately after drive, before consumers: a policy change or
/// close invalidates gameplay in this Update, before deferred state teardown.
pub(crate) fn update(world: &mut World) {
    let Some(status) = world
        .get_resource::<RemoteClient>()
        .map(|client| client.status().clone())
    else {
        return;
    };
    let state = *world.resource::<State<AppState>>().get();
    match status {
        Status::Closed(error) => fail(world, error_message(&error)),
        Status::Discovering => {
            if state == AppState::Match {
                cancel_preparation(world);
                world.resource_mut::<Loader>().invalidate();
                crate::session::clear_match(world);
                world
                    .resource_mut::<NextState<AppState>>()
                    .set(AppState::Loading);
            }
            world.resource_mut::<Loader>().status = "Contacting server…".into();
        }
        Status::AwaitingMap(info) => {
            if state == AppState::Match {
                let owned = world.get_resource::<Admission>().is_some_and(|admission| {
                    admission.info == info
                        && world
                            .resource::<Loader>()
                            .owns_remote(admission.serial, &info)
                });
                if !owned {
                    cancel_preparation(world);
                    world.resource_mut::<Loader>().invalidate();
                    crate::session::clear_match(world);
                    world
                        .resource_mut::<NextState<AppState>>()
                        .set(AppState::Loading);
                    return;
                }
                world.resource_mut::<Loader>().status = "Preparing the map renderer…".into();
                // OnEnter(Match) has applied spawn commands before this Update.
                // Capture owners rather than transient asset-store differences.
                if !world.resource::<Admission>().assets_captured {
                    match crate::remote_scene_assets::gather(world) {
                        Ok(assets) => {
                            let mut admission = world.resource_mut::<Admission>();
                            admission.meshes = assets.meshes;
                            admission.images = assets.images;
                            admission.assets_captured = true;
                        }
                        Err(_) => {
                            fail(world, "The map renderer could not prepare its assets".into());
                            return;
                        }
                    }
                }
                if world.resource::<Admission>().ticket.is_none() {
                    let admission = world.resource::<Admission>();
                    let meshes = admission.meshes.clone();
                    let images = admission.images.clone();
                    let result = world
                        .get_resource_mut::<RenderGate>()
                        .ok_or("The map renderer is unavailable")
                        .and_then(|mut gate| {
                            gate.begin(&meshes, &images)
                                .map_err(|_| "The map exceeds renderer preparation limits")
                        });
                    match result {
                        Ok(ticket) => world.resource_mut::<Admission>().ticket = Some(ticket),
                        Err(message) => {
                            fail(world, message.into());
                            return;
                        }
                    }
                }
                match world
                    .resource::<Admission>()
                    .ticket
                    .as_ref()
                    .unwrap()
                    .status()
                {
                    GateStatus::Pending => (),
                    GateStatus::Ready => {
                        let mut admission = world.remove_resource::<Admission>().unwrap();
                        let result = world
                            .resource_mut::<RemoteClient>()
                            .provide_map(admission.world.take().unwrap(), admission.manifest);
                        if let Err(error) = result {
                            fail(world, error_message(&error));
                        } else {
                            world.resource_mut::<Loader>().status =
                                "Synchronizing with server…".into();
                        }
                    }
                    GateStatus::Cancelled => fail(world, "Map preparation was cancelled".into()),
                    GateStatus::Failed => fail(
                        world,
                        "The map renderer could not prepare its shaders".into(),
                    ),
                }
            } else if state == AppState::Loading {
                let game_dir = world.resource::<crate::GameData>().game_dir.clone();
                world
                    .resource_mut::<Loader>()
                    .prepare_remote(&info, &game_dir);
            }
        }
        Status::Synchronizing => {
            world.resource_mut::<Loader>().status = "Synchronizing with server…".into()
        }
        Status::Ready => {
            world.resource_mut::<Loader>().status.clear();
        }
    }
}

fn error_message(error: &Error) -> String {
    match error {
        Error::Timeout => "The server did not respond. Check the address and try again.".into(),
        Error::ContentMismatch => {
            "The installed map or weapon catalog differs from the server.".into()
        }
        Error::Cancelled => "Connection cancelled.".into(),
        Error::InvalidName => "Choose a valid player name before joining.".into(),
        Error::UnsupportedWorld => "This server uses an unsupported game mode.".into(),
        _ => "The connection could not continue. Refresh the server and try again.".into(),
    }
}

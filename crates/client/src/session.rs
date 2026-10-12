//! Front-end/match lifetime. Loading is bounded to one worker; cancelling
//! discards its result, so a late map cannot replace a newer selection.
#[cfg(test)]
use crate::ClientOptions;
use crate::{GameData, LoadedLevel, Options, PendingLevel};
use bevy::prelude::*;
use std::sync::{
    Mutex,
    mpsc::{self, Receiver, TryRecvError},
};

#[derive(States, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Menu,
    Loading,
    Match,
    Browser,
}

/// Front-end entities, including its camera and UI sounds, survive map changes.
#[derive(Component)]
pub struct Persistent;
#[derive(Component)]
pub struct MatchEntity;

pub(crate) fn own_entity<T: Component>(
    added: On<Add, T>,
    state: Res<State<AppState>>,
    persistent: Query<(), With<Persistent>>,
    mut commands: Commands,
) {
    if *state.get() == AppState::Match && !persistent.contains(added.entity) {
        commands.entity(added.entity).insert(MatchEntity);
    }
}

struct Job {
    serial: u64,
    result: Mutex<Receiver<Result<LoadedMap, String>>>,
}

#[derive(Clone)]
enum Selection {
    Offline(crate::menu::MapRequest),
    Remote(netcode::protocol::ServerInfo),
}
impl Selection {
    fn label(&self) -> String {
        match self {
            Self::Offline(request) => request.label(),
            Self::Remote(info) => info.content.map_name.clone(),
        }
    }
}

pub(crate) struct LoadedMap {
    pub level: level::Level,
    pub remote: Option<crate::remote_join::Admission>,
}

#[derive(Resource, Default)]
pub(crate) struct Loader {
    serial: u64,
    wanted: Option<(u64, Selection)>,
    selection: Option<crate::menu::MapRequest>,
    job: Option<Job>,
    pub status: String,
    title: String,
}

impl Loader {
    fn start(&mut self, game_dir: &std::path::Path) {
        if self.job.is_some() {
            return;
        }
        let Some((serial, options)) = &self.wanted else {
            return;
        };
        let serial = *serial;
        let selection = options.clone();
        let stores = crate::remote_join::local_stores();
        let game_dir = game_dir.to_path_buf();
        let (tx, rx) = mpsc::channel();
        self.status = format!("Loading {}…", options.label());
        match std::thread::Builder::new()
            .name("map-loader".into())
            .spawn(move || {
                let result = match selection {
                    Selection::Offline(request) => {
                        let result = match request.pack {
                            Some((store, entry, id)) => {
                                level::Level::load_catalog_map(&store, &entry, &id, Some(&game_dir))
                            }
                            None => level::Level::load(&request.options.map, Some(&game_dir)),
                        };
                        result
                            .map(|level| LoadedMap {
                                level,
                                remote: None,
                            })
                            .map_err(|e| e.to_string())
                    }
                    Selection::Remote(info) => {
                        let (packs, archives) = match stores {
                            Ok(stores) => stores,
                            Err(error) => {
                                let _ = tx.send(Err(error));
                                return;
                            }
                        };
                        crate::remote_map::LocalMapRequest::resolve(
                            &info.content,
                            &game_dir,
                            &packs,
                            &archives,
                        )
                        .and_then(|request| request.prepare())
                        .map(|prepared| LoadedMap {
                            level: prepared.level,
                            remote: Some(crate::remote_join::Admission::new(
                                serial,
                                info,
                                prepared.world,
                                prepared.manifest,
                            )),
                        })
                        .map_err(|e| e.to_string())
                    }
                };
                let _ = tx.send(result);
            }) {
            Ok(_) => {
                self.job = Some(Job {
                    serial,
                    result: Mutex::new(rx),
                })
            }
            Err(e) => self.status = format!("Could not start map loader: {e}"),
        }
    }

    pub(crate) fn prepare_remote(
        &mut self,
        info: &netcode::protocol::ServerInfo,
        game_dir: &std::path::Path,
    ) {
        if self.wanted.as_ref().is_some_and(
            |(_, selected)| matches!(selected, Selection::Remote(current) if current == info),
        ) {
            return;
        }
        self.serial = self.serial.wrapping_add(1);
        self.title = info.content.map_name.clone();
        self.wanted = Some((self.serial, Selection::Remote(info.clone())));
        self.status = format!("Loading {}…", self.title);
        self.start(game_dir);
    }

    pub(crate) fn owns_remote(&self, serial: u64, info: &netcode::protocol::ServerInfo) -> bool {
        self.wanted.as_ref().is_some_and(|(current, selected)| {
            *current == serial && matches!(selected, Selection::Remote(current) if current == info)
        })
    }

    pub(crate) fn invalidate(&mut self) {
        self.wanted = None;
        self.selection = None;
    }
}

pub(crate) fn begin_load(
    mut loader: ResMut<Loader>,
    opts: Res<Options>,
    data: Res<GameData>,
    remote: Option<Res<crate::remote::RemoteClient>>,
) {
    if remote.is_some() {
        loader.invalidate();
        loader.status = "Contacting server…".into();
        return;
    }
    loader.serial = loader.serial.wrapping_add(1);
    let selection = loader
        .selection
        .take()
        .unwrap_or_else(|| opts.0.clone().into());
    loader.title = selection.label();
    loader.wanted = Some((loader.serial, Selection::Offline(selection)));
    loader.status = format!("Loading {}…", loader.title);
    loader.start(&data.game_dir);
}

pub(crate) fn cancel_load(mut loader: ResMut<Loader>) {
    // A remote preparation retains its generation through Match render warmup.
    if !loader
        .wanted
        .as_ref()
        .is_some_and(|(_, selection)| matches!(selection, Selection::Remote(_)))
    {
        loader.wanted = None;
    }
}

pub(crate) fn request_load(
    request: Res<crate::menu::LoadRequest>,
    mut loader: ResMut<Loader>,
    mut options: ResMut<Options>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    menu: Res<crate::menu::Menu>,
    remote: Option<Res<crate::remote::RemoteClient>>,
) {
    if *state.get() == AppState::Match && !menu.in_match && !crate::remote_join::loading(remote.as_deref()) {
        next.set(AppState::Menu);
    }
    if *state.get() == AppState::Loading {
        return;
    }
    if let Some(request) = request.0.lock().unwrap().take() {
        options.0 = request.options.clone();
        loader.selection = Some(request);
        next.set(AppState::Loading);
    }
}

pub(crate) fn poll_load(
    mut loader: ResMut<Loader>,
    data: Res<GameData>,
    pending: Res<PendingLevel>,
    mut options: ResMut<Options>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    mut menu: ResMut<crate::menu::Menu>,
    remote: Option<Res<crate::remote::RemoteClient>>,
) {
    let Some(job) = &loader.job else {
        if loader.wanted.is_some() && *state.get() == AppState::Loading {
            menu.status = loader.status.clone();
            next.set(AppState::Menu);
        }
        return;
    };
    let result = job.result.lock().unwrap().try_recv();
    let result = match result {
        Ok(result) => result,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err("Map loader stopped before finishing".into()),
    };
    let job = loader.job.take().unwrap();
    let wanted = loader
        .wanted
        .as_ref()
        .filter(|(serial, _)| *serial == job.serial)
        .map(|(_, options)| options.clone());
    if *state.get() == AppState::Loading {
        if let Some(wanted) = wanted {
            match result {
                Ok(loaded) => {
                    if let Selection::Remote(info) = &wanted {
                        if !remote.as_ref().is_some_and(|client| matches!(client.status(), netcode::remote_session::Status::AwaitingMap(current) if current == info)) {
                            loader.invalidate();
                            return;
                        }
                    }
                    info!("loaded {}: {:?}", loaded.level.name, loaded.level.timings);
                    *pending.0.lock().unwrap() = Some(loaded);
                    if let Selection::Offline(wanted) = wanted {
                        options.0 = wanted.options;
                    }
                    next.set(AppState::Match);
                }
                Err(error) => {
                    error!("Map load failed: {error}");
                    menu.status = error;
                    next.set(AppState::Menu);
                }
            }
        } else {
            loader.start(&data.game_dir);
        }
    }
}

pub(crate) fn match_ready(
    mut menu: ResMut<crate::menu::Menu>,
    opts: Res<Options>,
    loader: Res<Loader>,
    mut clock: ResMut<Time<Virtual>>,
    mut title: ResMut<crate::LevelInfo>,
    mut window: Single<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    menu.in_match = true;
    if std::env::var("CSR_TEST_MENU").as_deref() != Ok("pause") {
        menu.page = None;
    }
    if !menu.open() {
        clock.unpause();
    }
    title.name = if loader.title.is_empty() {
        opts.0.map.clone()
    } else {
        loader.title.clone()
    };
    window.title = format!("CSGO Rewrite — {}", title.name);
}

pub(crate) fn clear_match(world: &mut World) {
    let entities: Vec<_> = world
        .query_filtered::<Entity, (With<MatchEntity>, Without<Persistent>)>()
        .iter(world)
        .collect();
    for entity in entities {
        // Despawning a parent already removes its related children.
        if world.get_entity(entity).is_ok() {
            world.despawn(entity);
        }
    }
    world.remove_resource::<LoadedLevel>();
    world.remove_resource::<crate::player::SimWorld>();
    world.remove_resource::<crate::player::LocalPlayer>();
    world.remove_resource::<crate::characters::CharacterModels>();
    world.remove_resource::<crate::viewmodel::ViewModels>();
    world.remove_resource::<crate::items::GroundModels>();
    world.remove_resource::<crate::feedback::FeedbackAssets>();
    world.remove_resource::<crate::utility_fx::AssetsFx>();
    world.remove_resource::<crate::hud_art::Radar>();
    world.remove_resource::<crate::hud_art::Inventory>();
    world.insert_resource(crate::characters::Characters::default());
    world.insert_resource(crate::viewmodel::ViewModelState::default());
    world.insert_resource(crate::items::GroundInstances::default());
    world.insert_resource(crate::effects::Effects::default());
    world.insert_resource(crate::effects::PendingEvents::default());
    world.insert_resource(crate::foley::Foley::default());
    world.insert_resource(crate::tracers::Muzzles::default());
    world.insert_resource(crate::tracers::Tracers::default());
    world.insert_resource(crate::tracers::InspectRequest::default());
    world.insert_resource(crate::decorations::Decorations::default());
    world.insert_resource(crate::skin_finish::SkinTextures::default());
    world.insert_resource(crate::interpolation::RenderPoses::default());
    world.insert_resource(crate::buymenu::BuyMenu::default());
    world.insert_resource(crate::scoreboard::Scoreboard::default());
    world.insert_resource(crate::feedback::Hurt::default());
    if let Some(mut menu) = world.get_resource_mut::<crate::menu::Menu>() {
        menu.reset_match();
    }
    if let Some(mut clock) = world.get_resource_mut::<Time<Virtual>>() {
        clock.pause();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn loading_app(state: AppState) -> App {
        let mut app = App::new();
        let catalog = weapons::Catalog::from_items_game("\"items_game\" { \"items\" {} }").unwrap();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(state)
            .insert_resource(GameData {
                game_dir: std::env::temp_dir().join(format!(
                    "csr-missing-session-install-{}",
                    std::process::id()
                )),
                vfs: std::sync::Arc::new(vfs::Vfs::new()),
                catalog: std::sync::Arc::new(catalog),
            })
            .insert_resource(Options(ClientOptions::default()))
            .insert_resource(PendingLevel(Mutex::new(None)))
            .init_resource::<crate::menu::Menu>()
            .init_resource::<Loader>()
            .add_systems(Update, poll_load);
        app
    }

    #[test]
    fn cancelling_a_load_discards_its_late_error() {
        let mut app = loading_app(AppState::Menu);
        let (tx, rx) = mpsc::channel();
        app.world_mut().resource_mut::<Loader>().job = Some(Job {
            serial: 1,
            result: Mutex::new(rx),
        });
        tx.send(Err("Cancelled map must not replace menu status".into()))
            .unwrap();
        app.update();
        assert!(app.world().resource::<Loader>().job.is_none());
        assert!(
            app.world()
                .resource::<crate::menu::Menu>()
                .status
                .is_empty()
        );
        assert_eq!(
            *app.world().resource::<State<AppState>>().get(),
            AppState::Menu
        );
    }

    #[test]
    fn a_new_selection_waits_for_the_existing_worker_then_starts_once() {
        let mut app = loading_app(AppState::Loading);
        let (tx, rx) = mpsc::channel();
        {
            let mut loader = app.world_mut().resource_mut::<Loader>();
            loader.job = Some(Job {
                serial: 1,
                result: Mutex::new(rx),
            });
            loader.wanted = Some((
                2,
                Selection::Offline(
                    ClientOptions {
                        map: "de_new_selection".into(),
                        ..default()
                    }
                    .into(),
                ),
            ));
            loader.start(std::path::Path::new("unused"));
            assert_eq!(loader.job.as_ref().unwrap().serial, 1);
        }
        tx.send(Err("Previous selection".into())).unwrap();
        app.update();
        let loader = app.world().resource::<Loader>();
        assert_eq!(loader.job.as_ref().unwrap().serial, 2);
        assert!(loader.status.contains("de_new_selection"));
        assert!(
            app.world()
                .resource::<crate::menu::Menu>()
                .status
                .is_empty()
        );
    }

    #[test]
    fn leaving_match_removes_dynamic_entities_and_preserves_front_end() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::Match)
            .add_observer(own_entity::<Transform>)
            .add_observer(own_entity::<Node>)
            .init_resource::<crate::menu::Menu>()
            .add_systems(OnExit(AppState::Match), clear_match);
        app.update();
        let camera = app
            .world_mut()
            .spawn((Persistent, Transform::default()))
            .id();
        let joint = app.world_mut().spawn(Transform::default()).id();
        let child = app
            .world_mut()
            .spawn((Transform::default(), ChildOf(joint)))
            .id();
        let hud = app.world_mut().spawn(Node::default()).id();
        // UI audio can receive its persistent marker in a later command,
        // after the generic match ownership observer has already tagged it.
        let ui_sound = app.world_mut().spawn((MatchEntity, Persistent)).id();
        app.world_mut().flush();
        assert!(app.world().get::<MatchEntity>(joint).is_some());
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Menu);
        app.update();
        assert!(app.world().get_entity(camera).is_ok());
        assert!(app.world().get_entity(joint).is_err());
        assert!(app.world().get_entity(child).is_err());
        assert!(app.world().get_entity(hud).is_err());
        assert!(app.world().get_entity(ui_sound).is_ok());
        assert_eq!(
            *app.world().resource::<State<AppState>>().get(),
            AppState::Menu
        );
        assert!(!app.world().resource::<crate::menu::Menu>().in_match);
    }

    fn waiting_remote(
        rate: u32,
    ) -> (
        crate::remote::RemoteClient,
        netcode::protocol::ServerInfo,
        sim::World,
    ) {
        use netcode::{
            remote_session::{RemoteSession, Status},
            session::Session,
            transport::Server,
        };
        let manifest = netcode::protocol::ContentManifest {
            map_id: "csgo_map_session".into(),
            map_name: "session".into(),
            map_hash: "0".repeat(40),
            catalog_hash: "1".repeat(40),
        };
        let mut map = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
        map.set_tick_rate(rate);
        let mut server = Server::bind(
            "127.0.0.1:0".parse().unwrap(),
            Session::new(map.clone(), manifest).unwrap(),
            4,
        )
        .unwrap();
        let mut remote = RemoteSession::discover(
            server.address(),
            "Local".into(),
            sim::Team::CounterTerrorist,
        )
        .unwrap();
        for _ in 0..128 {
            remote.update(1.0 / rate as f64).unwrap();
            server
                .update(std::time::Duration::from_secs_f64(1.0 / rate as f64))
                .unwrap();
            if matches!(remote.status(), Status::AwaitingMap(_)) {
                break;
            }
        }
        let Status::AwaitingMap(info) = remote.status().clone() else {
            panic!("discovery did not complete")
        };
        // CPU/map preparation may span any elapsed time without admission.
        remote.update(30.0).unwrap();
        (crate::remote::RemoteClient::from_session(remote), info, map)
    }

    #[test]
    fn missing_scene_materials_close_without_starting_remote_admission() {
        for rate in [64, 128] {
            let (remote, info, map) = waiting_remote(rate);
            let mut app = loading_app(AppState::Match);
            app.world_mut().resource_mut::<crate::menu::Menu>().in_match = true;
            app.world_mut().resource_mut::<Loader>().wanted =
                Some((7, Selection::Remote(info.clone())));
            app.insert_resource(crate::remote_join::Admission::new(
                7,
                info.clone(),
                map,
                info.content.clone(),
            ));
            app.insert_resource(remote);
            crate::remote_join::update(app.world_mut());
            assert!(!app.world().contains_resource::<crate::remote::RemoteClient>());
            assert!(
                !app.world()
                    .contains_resource::<crate::remote_join::Admission>()
            );
            assert!(
                !app.world()
                    .contains_resource::<crate::remote_render_gate::RenderGate>()
            );
            assert!(app.world().resource::<crate::menu::Menu>().status.contains("renderer"));
        }
    }

    #[test]
    fn changed_policy_discards_the_owned_scene_before_deferred_transition() {
        let (remote, info, map) = waiting_remote(128);
        let mut old = info.clone();
        old.tick_rate = 64;
        let mut app = loading_app(AppState::Match);
        app.insert_resource(crate::player::SimWorld(map.clone()));
        app.insert_resource(crate::player::LocalPlayer::new(0, glam::Vec3::ZERO, 0.0));
        app.world_mut().resource_mut::<crate::menu::Menu>().in_match = true;
        app.world_mut().resource_mut::<Loader>().wanted = Some((7, Selection::Remote(old.clone())));
        app.insert_resource(crate::remote_join::Admission::new(
            7,
            old,
            map,
            info.content,
        ));
        app.insert_resource(remote);
        app.init_resource::<crate::menu::LoadRequest>()
            .add_systems(Update, (crate::remote_join::update, request_load).chain());
        app.update();
        assert!(!app.world().contains_resource::<crate::player::SimWorld>());
        assert!(
            !app.world()
                .contains_resource::<crate::player::LocalPlayer>()
        );
        assert!(
            !app.world()
                .contains_resource::<crate::remote_join::Admission>()
        );
        assert!(!app.world().resource::<crate::menu::Menu>().in_match);
        assert!(
            app.world()
                .contains_resource::<crate::remote::RemoteClient>()
        );
        assert!(app.world().resource::<Loader>().wanted.is_none());
        assert!(matches!(app.world().resource::<NextState<AppState>>(), NextState::Pending(AppState::Loading)));
    }

    #[test]
    fn returning_home_revokes_the_render_ticket_and_remote_generation() {
        let (remote, info, map) = waiting_remote(128);
        let mut app = loading_app(AppState::Match);
        let mut gate = crate::remote_render_gate::RenderGate::default();
        let ticket = gate.begin(&[], &[]).unwrap();
        app.insert_resource(gate);
        app.world_mut().resource_mut::<Loader>().wanted =
            Some((7, Selection::Remote(info.clone())));
        app.insert_resource(crate::remote_join::Admission::new(
            7,
            info.clone(),
            map,
            info.content,
        ));
        app.insert_resource(remote);
        crate::remote_join::home(app.world_mut());
        assert_eq!(
            ticket.status(),
            crate::remote_render_gate::GateStatus::Cancelled
        );
        assert!(
            !app.world()
                .contains_resource::<crate::remote::RemoteClient>()
        );
        assert!(
            !app.world()
                .contains_resource::<crate::remote_join::Admission>()
        );
        assert!(app.world().resource::<Loader>().wanted.is_none());
        assert!(
            app.world()
                .resource::<PendingLevel>()
                .0
                .lock()
                .unwrap()
                .is_none()
        );
    }
}

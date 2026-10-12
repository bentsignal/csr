//! Front-end/match lifetime. Loading is bounded to one worker; cancelling
//! discards its result, so a late map cannot replace a newer selection.
use crate::{ClientOptions, GameData, LoadedLevel, Options, PendingLevel};
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
    result: Mutex<Receiver<Result<level::Level, String>>>,
}

#[derive(Resource, Default)]
pub(crate) struct Loader {
    serial: u64,
    wanted: Option<(u64, crate::menu::MapRequest)>,
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
        let map = options.options.map.clone();
        let pack = options.pack.clone();
        let game_dir = game_dir.to_path_buf();
        let (tx, rx) = mpsc::channel();
        self.status = format!("Loading {}…", options.label());
        match std::thread::Builder::new()
            .name("map-loader".into())
            .spawn(move || {
                let result = match pack {
                    Some((store, entry, id)) => {
                        level::Level::load_catalog_map(&store, &entry, &id, Some(&game_dir))
                    }
                    None => level::Level::load(&map, Some(&game_dir)),
                }
                .map_err(|e| e.to_string());
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
}

pub(crate) fn begin_load(mut loader: ResMut<Loader>, opts: Res<Options>, data: Res<GameData>) {
    loader.serial = loader.serial.wrapping_add(1);
    let selection = loader
        .selection
        .take()
        .unwrap_or_else(|| opts.0.clone().into());
    loader.title = selection.label();
    loader.wanted = Some((loader.serial, selection));
    loader.status = format!("Loading {}…", loader.title);
    loader.start(&data.game_dir);
}

pub(crate) fn cancel_load(mut loader: ResMut<Loader>) {
    loader.wanted = None;
}

pub(crate) fn request_load(
    request: Res<crate::menu::LoadRequest>,
    mut loader: ResMut<Loader>,
    mut options: ResMut<Options>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    menu: Res<crate::menu::Menu>,
) {
    if *state.get() == AppState::Match && !menu.in_match {
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
                Ok(level) => {
                    info!("loaded {}: {:?}", level.name, level.timings);
                    *pending.0.lock().unwrap() = Some(level);
                    options.0 = wanted.options;
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
                ClientOptions {
                    map: "de_new_selection".into(),
                    ..default()
                }
                .into(),
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
}

//! Front-end/match lifetime. One active loader and at most one retired classic
//! acquisition keep cancellation from blocking installed maps or racing writers.
#[cfg(test)]
use crate::ClientOptions;
use crate::{GameData, LoadedLevel, Options, PendingLevel};
use bevy::prelude::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
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
    importing: Option<Res<crate::scene_import::ApplyingImport>>,
    loader: Option<Res<Loader>>,
    mut commands: Commands,
) {
    if persistent.contains(added.entity) { return; }
    if *state.get() == AppState::Match {
        commands.entity(added.entity).insert(MatchEntity);
    } else if let Some(importing) = importing.filter(|importing|
        *state.get() == AppState::Loading
            && loader.as_ref().is_some_and(|loader| loader.generation() == Some(importing.0))) {
        commands.entity(added.entity).insert((MatchEntity, crate::scene_import::ImportedEntity(importing.0)));
    }
}

struct Job {
    serial: u64,
    classic: bool,
    result: Mutex<Receiver<Result<LoadedMap, String>>>,
    progress: Arc<LoadProgress>,
}

#[derive(Default)]
struct LoadProgress {
    cancelled: AtomicBool,
    status: Mutex<Option<String>>,
}

impl LoadProgress {
    fn report(&self, title: &str, p: workshop::Progress) -> bool {
        if self.cancelled.load(Ordering::Relaxed) {
            return false;
        }
        let stage = match p.stage {
            workshop::Stage::Query => "Contacting Steam",
            workshop::Stage::Download => "Downloading",
            workshop::Stage::Verify => "Checking download",
            workshop::Stage::Extract => "Preparing map",
        };
        let percent = if p.total > 0 {
            format!(
                " — {}%",
                p.done
                    .saturating_mul(100)
                    .checked_div(p.total)
                    .unwrap_or(0)
                    .min(100)
            )
        } else {
            String::new()
        };
        *self.status.lock().unwrap() = Some(format!("{stage} {title}{percent}"));
        true
    }
}

fn load_requested_map(
    request: &crate::menu::MapRequest,
    game_dir: &std::path::Path,
    progress: &LoadProgress,
) -> Result<level::Level, String> {
    let level = load_requested_map_content(request, game_dir, progress)?;
    // Validate explicit choices on the worker before entering Match/hosting.
    level.configured_bots(request.options.bot_difficulty)?;
    if request.pack.is_none() && !request.options.grenade_practice {
        level::maps::validate_archive_match(&level, request.options.bots)?;
    }
    Ok(level)
}

fn load_requested_map_content(
    request: &crate::menu::MapRequest,
    game_dir: &std::path::Path,
    progress: &LoadProgress,
) -> Result<level::Level, String> {
    if progress.cancelled.load(Ordering::Relaxed) {
        return Err(workshop::CANCELLED.into());
    }
    if let Some((store, entry, id)) = &request.pack {
        return level::Level::load_catalog_map(store, entry, id, Some(game_dir))
            .map_err(|e| e.to_string());
    }
    if let Some(old) = workshop::find(&request.options.map) {
        workshop::Store::current().fetch(old, &mut |p| progress.report(old.title, p)).map_err(|error| {
            if error == workshop::CANCELLED { return error; }
            error!("Classic map acquisition failed for {}: {error}", old.name);
            format!("Could not prepare {}. Check your connection and available storage, then choose GO to retry.", old.title)
        })?;
        if progress.cancelled.load(Ordering::Relaxed) {
            return Err(workshop::CANCELLED.into());
        }
        *progress.status.lock().unwrap() = Some(format!("Loading {}…", old.title));
        return level::Level::load_id(&old.content_id(), Some(game_dir)).map_err(|error| {
            error!("Classic map load failed for {}: {error}", old.name);
            format!(
                "Could not open {}. Return to the map picker and try again.",
                old.title
            )
        });
    }
    level::Level::load(&request.options.map, Some(game_dir)).map_err(|e| e.to_string())
}

#[derive(Clone)]
enum Selection {
    Offline(crate::menu::MapRequest),
    Host(crate::hosting::HostSelection),
    Remote(netcode::protocol::ServerInfo),
}
impl Selection {
    fn classic(&self) -> bool {
        let request = match self {
            Self::Offline(request) => request,
            Self::Host(selected) => &selected.map,
            Self::Remote(_) => return false,
        };
        request.pack.is_none() && workshop::find(&request.options.map).is_some()
    }

    fn installed_map(&self) -> bool {
        let request = match self {
            Self::Offline(request) => request,
            Self::Host(selected) => &selected.map,
            Self::Remote(_) => return false,
        };
        request.pack.is_none() && workshop::find(&request.options.map).is_none()
    }

    fn label(&self) -> String {
        match self {
            Self::Offline(request) => request.label(),
            Self::Host(request) => request.map.label(),
            Self::Remote(info) => info.content.map_name.clone(),
        }
    }
}

pub(crate) struct LoadedMap {
    pub movement:Option<crate::movement_modes::Prepared>,
    pub level: level::Level,
    pub remote: Option<crate::remote_join::Admission>,
    pub host: Option<crate::hosting::PreparedHost>,
}

// A completed CPU import is not yet permission to block the main thread on
// scene/character uploads. Keep it owned until Loading gets a render opportunity.
struct ReadyLoad {
    serial: u64,
    loaded: LoadedMap,
    since: std::time::Instant,
}

#[derive(Resource, Default)]
pub(crate) struct Loader {
    serial: u64,
    wanted: Option<(u64, Selection)>,
    selection: Option<crate::menu::MapRequest>,
    host_selection: Option<crate::hosting::HostSelection>,
    job: Option<Job>,
    retired: Option<Job>,
    ready: Option<ReadyLoad>,
    pub(crate) importing: Option<u64>,
    pub status: String,
    title: String,
}

/// A selected Legacy map, owned by one loader generation. Other content keeps
/// the engine's progress presentation without borrowing unrelated Legacy art.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoadingMap {
    pub serial: u64,
    pub name: String,
    /// Known local rules only. Discovery does not advertise a mode, and the
    /// grenade sandbox must not borrow an ordinary game-mode description.
    pub mode: Option<sim::GameMode>,
}

impl Loader {
    pub(crate) fn generation(&self) -> Option<u64> { self.wanted.as_ref().map(|(serial, _)| *serial) }

    pub(crate) fn loading_map(&self) -> Option<LoadingMap> {
        let (serial, selection) = self.wanted.as_ref()?;
        let name = match selection {
            Selection::Offline(request) if request.pack.is_none() && (workshop::find(&request.options.map).is_none() || crate::frontend_loading::supported(&request.options.map)) => &request.options.map,
            Selection::Host(selected) if selected.map.pack.is_none() && (workshop::find(&selected.map.options.map).is_none() || crate::frontend_loading::supported(&selected.map.options.map)) => &selected.map.options.map,
            Selection::Remote(info) => {
                let id = info.content.validate().ok()?;
                if id.origin != content::GameOrigin::Csgo || id.name != info.content.map_name { return None; }
                &info.content.map_name
            }
            _ => return None,
        };
        let options = match selection {
            Selection::Offline(request) => Some(&request.options),
            Selection::Host(selected) => Some(&selected.map.options),
            Selection::Remote(_) => None,
        };
        let mode = options.filter(|options| !options.grenade_practice).map(|options| {
            if options.deathmatch { sim::GameMode::Deathmatch } else { sim::GameMode::Competitive }
        });
        (!name.is_empty() && name.len() <= 128
            && content::sanitize(name) == *name)
            .then(|| LoadingMap { serial: *serial, name: name.clone(), mode })
    }

    pub(crate) fn select_host(&mut self, selected: crate::hosting::HostSelection) {
        self.host_selection = Some(selected);
    }

    pub(crate) fn bind_host_generation(
        &mut self,
        serial: u64,
        info: netcode::protocol::ServerInfo,
    ) {
        self.wanted = Some((serial, Selection::Remote(info)));
    }

    fn start(&mut self, game_dir: &std::path::Path) {
        if self.importing.is_some() && self.importing == self.generation() { return; }
        if self.ready.as_ref().is_some_and(|ready| self.generation() == Some(ready.serial)) { return; }
        self.ready = None;
        if self.job.is_some() {
            return;
        }
        let Some((serial, options)) = &self.wanted else {
            return;
        };
        if self.retired.is_some() && !options.installed_map() {
            self.status =
                "Finishing the previous map cancellation. You can choose an installed map now."
                    .into();
            return;
        }
        let serial = *serial;
        let classic = options.classic();
        let selection = options.clone();
        let game_dir = game_dir.to_path_buf();
        let (tx, rx) = mpsc::channel();
        let progress = Arc::new(LoadProgress::default());
        let worker_progress = progress.clone();
        self.status = format!("Loading {}…", options.label());
        match std::thread::Builder::new()
            .name("map-loader".into())
            .spawn(move || {
                let result = match selection {
                    Selection::Offline(request) => {
                        let result = load_requested_map(&request, &game_dir, &worker_progress);
                        result
                            .and_then(|level| {
                                let movement=crate::movement_modes::prepare(&level.collision)?;
                                if worker_progress.cancelled.load(Ordering::Relaxed){return Err(workshop::CANCELLED.into());}
                                Ok(LoadedMap {level,movement:Some(movement),remote:None,host:None})
                            })
                            .map_err(|e| e.to_string())
                    }
                    Selection::Host(selected) => {
                        if let Err(error) = selected.validate() {
                            let _ = tx.send(Err(error));
                            return;
                        }
                        let result = load_requested_map(&selected.map, &game_dir, &worker_progress);
                        result.map_err(|error| error.to_string()).and_then(|level| {
                            if worker_progress.cancelled.load(Ordering::Relaxed) {
                                return Err(workshop::CANCELLED.into());
                            }
                            let manifest = crate::remote_map::manifest_for_level(&level)
                                .map_err(|error| error.to_string())?;
                            let map_world = crate::remote_map::map_world_for_level(&level);
                            let mut authority = map_world.clone();
                            authority.set_tick_rate(selected.tick_rate);
                            authority.bots = level.configured_bots(selected.map.options.bot_difficulty)?;
                            if !selected.map.options.deathmatch {
                                authority.set_mode(
                                    sim::GameMode::Competitive,
                                    crate::competitive_rules(&level),
                                );
                            }
                            let mine = selected.team;
                            let theirs = if mine == sim::Team::Terrorist {
                                sim::Team::CounterTerrorist
                            } else {
                                sim::Team::Terrorist
                            };
                            let bots = if selected.map.options.bot_difficulty == Some(0) { 0 } else { selected.map.options.bots };
                            let names = |team| {
                                if team == sim::Team::Terrorist {
                                    &crate::BOT_NAMES_T
                                } else {
                                    &crate::BOT_NAMES_CT
                                }
                            };
                            for i in 0..bots {
                                authority.add_bot(names(theirs)[i as usize % 8], theirs);
                            }
                            for i in 0..bots.saturating_sub(1) {
                                authority.add_bot(names(mine)[i as usize % 8], mine);
                            }
                            Ok(LoadedMap {
                                movement:None,
                                level,
                                remote: None,
                                host: Some(crate::hosting::PreparedHost {
                                    selection: selected,
                                    authority,
                                    map_world,
                                    manifest,
                                }),
                            })
                        })
                    }
                    Selection::Remote(info) => {
                        // Store discovery touches Steam/player-data paths. Do it only
                        // for a remote request, on this worker rather than the UI.
                        let (packs, archives) = match crate::remote_join::local_stores() {
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
                            movement:None,
                            level: prepared.level,
                            remote: Some(crate::remote_join::Admission::new(
                                serial,
                                info,
                                prepared.world,
                                prepared.manifest,
                            )),
                            host: None,
                        })
                        .map_err(|e| e.to_string())
                    }
                };
                let _ = tx.send(result);
            }) {
            Ok(_) => {
                self.job = Some(Job {
                    serial,
                    classic,
                    result: Mutex::new(rx),
                    progress,
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

    fn cancel_current(&mut self) {
        self.importing = None;
        self.ready = None;
        if let Some(job) = &self.job {
            job.progress.cancelled.store(true, Ordering::Relaxed);
        }
        if self.retired.is_none() && self.job.as_ref().is_some_and(|job| job.classic) {
            self.retired = self.job.take();
        }
    }

    pub(crate) fn invalidate(&mut self) {
        self.cancel_current();
        self.wanted = None;
        self.selection = None;
        self.host_selection = None;
    }
}

pub(crate) fn begin_load(
    mut loader: ResMut<Loader>,
    opts: Res<Options>,
    data: Res<GameData>,
    remote: Option<Res<crate::remote::RemoteClient>>,
    hosting: Res<crate::hosting::Hosting>,
) {
    if hosting.exiting() {
        loader.invalidate();
        return;
    }
    if remote.is_some() {
        loader.invalidate();
        loader.status = "Contacting server…".into();
        return;
    }
    loader.serial = loader.serial.wrapping_add(1);
    if let Some(selected) = loader.host_selection.take() {
        loader.title = selected.map.label();
        loader.wanted = Some((loader.serial, Selection::Host(selected)));
        loader.start(&data.game_dir);
        return;
    }
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
    loader.cancel_current();
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
    hosting: Res<crate::hosting::Hosting>,
) {
    if hosting.exiting() {
        request.0.lock().unwrap().take();
        return;
    }
    if *state.get() == AppState::Match
        && !menu.in_match
        && !crate::remote_join::loading(remote.as_deref())
    {
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
    mut hosting: ResMut<crate::hosting::Hosting>,
    front: Option<Res<crate::frontend::Frontend>>,
    app_frame: Option<Res<bevy::diagnostic::FrameCount>>,
) {
    if loader.importing.is_some() && loader.importing == loader.generation() { return; }
    if let Some(ready) = &loader.ready {
        let wanted = loader.wanted.as_ref().filter(|(serial, _)| *serial == ready.serial)
            .map(|(_, selection)| selection.clone());
        if *state.get() != AppState::Loading || wanted.is_none() {
            loader.ready = None;
            if *state.get() == AppState::Loading { loader.start(&data.game_dir); }
        } else {
            let wanted = wanted.unwrap();
            if let Selection::Remote(info) = &wanted {
                if !remote.as_ref().is_some_and(|client| matches!(client.status(), netcode::remote_session::Status::AwaitingMap(current) if current == info)) {
                    loader.invalidate();
                    return;
                }
            }
            let painted = front.as_ref().is_none_or(|front|
                app_frame.as_ref().is_some_and(|frame| front.loading_painted(ready.serial, frame.0)));
            // A missing/broken movie cannot deadlock map entry. This is a
            // maximum escape path, never a minimum loading-screen duration.
            if crate::frontend_loading_presentation::waiting(painted, ready.since.elapsed()) { return; }
            if !painted { warn!(generation = ready.serial, "Loading art presentation unavailable; retaining progress fallback"); }
            else if front.is_some() { info!(generation = ready.serial, "Loading presentation admitted before scene imports"); }
            let ReadyLoad { serial, loaded, .. } = loader.ready.take().unwrap();
            if matches!(wanted, Selection::Host(_)) {
                if let Err(error) = hosting.stage(serial, loaded) {
                    loader.invalidate();
                    menu.status = error;
                    next.set(AppState::Menu);
                }
                return;
            }
            info!("loaded {}: {:?}", loaded.level.name, loaded.level.timings);
            *pending.0.lock().unwrap() = Some(loaded);
            loader.importing = Some(serial);
            if let Selection::Offline(wanted) = wanted { options.0 = wanted.options; }
            return;
        }
    }
    if let Some(retired) = &loader.retired {
        let finished = !matches!(
            retired.result.lock().unwrap().try_recv(),
            Err(TryRecvError::Empty)
        );
        if finished {
            // Neither its result nor its last status may affect the new generation.
            loader.retired = None;
            if loader.job.is_none() && *state.get() == AppState::Loading && !hosting.preparing() {
                loader.start(&data.game_dir);
            }
        }
    }
    let Some(job) = &loader.job else {
        if loader.retired.is_some() {
            return;
        }
        if loader.wanted.is_some() && *state.get() == AppState::Loading && !hosting.preparing() {
            menu.status = loader.status.clone();
            next.set(AppState::Menu);
        }
        return;
    };
    let status = (*state.get() == AppState::Loading
        && loader
            .wanted
            .as_ref()
            .is_some_and(|(serial, _)| *serial == job.serial))
    .then(|| job.progress.status.lock().unwrap().clone())
    .flatten();
    let result = job.result.lock().unwrap().try_recv();
    if let Some(status) = status {
        loader.status = status;
    }
    let result = match result {
        Ok(result) => result,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err("Map loader stopped before finishing".into()),
    };
    let job = loader.job.take().unwrap();
    let wanted = loader
        .wanted
        .as_ref()
        .filter(|(serial, _)| {
            *serial == job.serial && !job.progress.cancelled.load(Ordering::Relaxed)
        })
        .map(|(_, options)| options.clone());
    if *state.get() == AppState::Loading {
        if wanted.is_some() {
            match result {
                Ok(loaded) => {
                    loader.ready = Some(ReadyLoad { serial: job.serial, loaded, since: std::time::Instant::now() });
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
    world.remove_resource::<crate::movement_modes::Visuals>();
    world.remove_resource::<crate::movement_modes::full_board::MaterialTemplate>();
    world.insert_resource(crate::board_presentation::HeldBoard::default());
    world.insert_resource(crate::board_audio::BoardAudio::default());
    world.remove_resource::<crate::player::SimWorld>();
    world.remove_resource::<crate::player::LocalPlayer>();
    world.remove_resource::<crate::characters::CharacterModels>();
    world.remove_resource::<crate::viewmodel::ViewModels>();
    world.remove_resource::<crate::items::GroundModels>();
    world.remove_resource::<crate::doors::DoorScene>();
    world.remove_resource::<crate::prop_breakables::PropBreakableScene>();
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

    #[test]
    fn loading_mode_belongs_to_the_current_known_selection_not_the_previous_map() {
        let mut loader = Loader::default();
        loader.wanted = Some((41, Selection::Offline(ClientOptions::default().into())));
        let first = loader.loading_map().unwrap();
        assert_eq!((first.serial, first.mode), (41, Some(sim::GameMode::Competitive)));
        loader.wanted = Some((42, Selection::Host(crate::hosting::HostSelection {
            map: ClientOptions { deathmatch: true, ..default() }.into(),
            config: netcode::host::HostConfig::default(), tick_rate: 128,
            name: "Host".into(), team: sim::Team::CounterTerrorist,
        })));
        let next = loader.loading_map().unwrap();
        assert_eq!(next.name, first.name);
        assert_eq!((next.serial, next.mode), (42, Some(sim::GameMode::Deathmatch)));
        assert_ne!(first, next);
        loader.wanted = Some((43, Selection::Offline(ClientOptions {
            grenade_practice: true, ..default()
        }.into())));
        assert_eq!(loader.loading_map().unwrap().mode, None);
        loader.wanted = Some((44, Selection::Remote(netcode::protocol::ServerInfo {
            version: netcode::protocol::VERSION, players: 0, max_players: 4,
            tick_rate: 128, snapshot_interval_ticks: 2, max_unlag_ticks: 128,
            interpolation_allowance_ticks: 2,
            content: netcode::protocol::ContentManifest {
                map_id: "csgo_map_de_dust2".into(), map_name: "de_dust2".into(),
                map_hash: "0".repeat(40), catalog_hash: "1".repeat(40),
            },
        })));
        let remote = loader.loading_map().unwrap();
        assert_eq!(remote.name, first.name);
        assert_eq!((remote.serial, remote.mode), (44, None));
        loader.invalidate();
        assert!(loader.loading_map().is_none());
    }

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
            .init_resource::<crate::hosting::Hosting>()
            .add_systems(Update, poll_load);
        app
    }

    #[test]
    fn main_thread_import_keeps_its_generation_without_a_duplicate_worker_or_false_failure() {
        let mut app = loading_app(AppState::Loading);
        {
            let mut loader = app.world_mut().resource_mut::<Loader>();
            loader.wanted = Some((7, Selection::Offline(ClientOptions::default().into())));
            loader.importing = Some(7);
            loader.status = "Preparing props".into();
            loader.start(std::path::Path::new("unused"));
            assert!(loader.job.is_none());
        }
        app.update();
        assert_eq!(app.world().resource::<Loader>().importing, Some(7));
        assert_eq!(app.world().resource::<Loader>().status, "Preparing props");
        assert!(!matches!(app.world().resource::<NextState<AppState>>(), NextState::Pending(AppState::Menu)));
        app.world_mut().resource_mut::<Loader>().invalidate();
        assert!(app.world().resource::<Loader>().importing.is_none());
    }

    #[test]
    fn cancelling_a_load_discards_its_late_error() {
        let mut app = loading_app(AppState::Menu);
        let (tx, rx) = mpsc::channel();
        app.world_mut().resource_mut::<Loader>().job = Some(Job {
            serial: 1,
            classic: false,
            result: Mutex::new(rx),
            progress: Default::default(),
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
    fn archive_progress_belongs_to_its_load_generation_and_cancels_acquisition() {
        let mut app = loading_app(AppState::Loading);
        let (_tx, rx) = mpsc::channel();
        let progress = Arc::new(LoadProgress::default());
        let update = workshop::Progress {
            stage: workshop::Stage::Download,
            done: 50,
            total: 100,
        };
        assert!(progress.report("Dust II (2017)", update));
        {
            let mut loader = app.world_mut().resource_mut::<Loader>();
            loader.wanted = Some((1, Selection::Offline(ClientOptions::default().into())));
            loader.job = Some(Job {
                serial: 1,
                classic: false,
                result: Mutex::new(rx),
                progress: progress.clone(),
            });
        }
        app.update();
        assert_eq!(
            app.world().resource::<Loader>().status,
            "Downloading Dust II (2017) — 50%"
        );
        {
            let mut loader = app.world_mut().resource_mut::<Loader>();
            loader.wanted = Some((2, Selection::Offline(ClientOptions::default().into())));
            loader.status = "Loading a newer selection".into();
        }
        assert!(progress.report("Dust II (2017)", workshop::Progress { done: 90, ..update }));
        app.update();
        assert_eq!(
            app.world().resource::<Loader>().status,
            "Loading a newer selection"
        );
        app.world_mut().resource_mut::<Loader>().invalidate();
        assert!(!progress.report("Dust II (2017)", update));
        assert!(progress.cancelled.load(Ordering::Relaxed));
        // The old worker remains retained until its result, avoiding a second
        // writer to the resumable archive while cancellation is in flight.
        assert!(app.world().resource::<Loader>().job.is_some());
    }

    #[test]
    fn cancelled_stalled_archive_allows_installed_map_but_not_another_archive() {
        let mut app = loading_app(AppState::Loading);
        let (stalled, rx) = mpsc::channel();
        let progress = Arc::new(LoadProgress::default());
        assert!(progress.report(
            "Dust II (2017)",
            workshop::Progress {
                stage: workshop::Stage::Download,
                done: 50,
                total: 100,
            }
        ));
        let game_dir = app.world().resource::<GameData>().game_dir.clone();
        let installed_status;
        {
            let mut loader = app.world_mut().resource_mut::<Loader>();
            loader.job = Some(Job {
                serial: 1,
                classic: true,
                result: Mutex::new(rx),
                progress: progress.clone(),
            });
            loader.invalidate();
            assert!(loader.job.is_none());
            assert!(loader.retired.is_some());
            assert!(progress.cancelled.load(Ordering::Relaxed));

            let archive = ClientOptions {
                map: crate::frontend_game::CLASSIC_DUST2.into(),
                ..Default::default()
            };
            loader.wanted = Some((2, Selection::Offline(archive.into())));
            loader.start(&game_dir);
            assert!(loader.job.is_none(), "a second archive writer must wait");

            loader.wanted = Some((3, Selection::Offline(ClientOptions::default().into())));
            loader.start(&game_dir);
            let installed = loader
                .job
                .as_ref()
                .expect("installed map admitted immediately");
            assert_eq!(installed.serial, 3);
            assert!(!installed.classic);
            assert!(loader.retired.is_some());
            installed_status = loader.status.clone();
            // The synthetic install is absent: join this bounded ordinary load
            // through its existing result channel, without real game data.
            assert!(
                installed
                    .result
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .unwrap()
                    .is_err()
            );
        }
        stalled.send(Err("stale archive failure".into())).unwrap();
        app.update();
        assert!(app.world().resource::<Loader>().retired.is_none());
        assert_eq!(app.world().resource::<Loader>().status, installed_status);
        assert!(
            !app.world()
                .resource::<crate::menu::Menu>()
                .status
                .contains("stale archive")
        );
        assert!(
            app.world()
                .resource::<PendingLevel>()
                .0
                .lock()
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn host_binding_without_a_map_job_does_not_report_loader_failure() {
        let mut app = loading_app(AppState::Loading);
        let (remote, info, _map) = waiting_remote(128);
        app.insert_resource(remote);
        app.world_mut().resource_mut::<Loader>().wanted = Some((7, Selection::Remote(info)));
        // Passive host binding/offer publication occupies the same load generation.
        app.world_mut()
            .resource_mut::<crate::hosting::Hosting>()
            .retain_generation_for_test(7);
        app.update();
        assert!(
            app.world()
                .resource::<crate::menu::Menu>()
                .status
                .is_empty()
        );
        assert!(!matches!(
            app.world().resource::<NextState<AppState>>(),
            NextState::Pending(AppState::Menu)
        ));
    }

    #[test]
    fn a_new_host_during_loading_waits_for_the_old_worker_and_keeps_its_selection() {
        let mut app = loading_app(AppState::Loading);
        app.init_resource::<crate::hosting::HostRequest>()
            .init_resource::<crate::menu::LoadRequest>()
            .add_systems(Update, crate::hosting::request.before(poll_load));
        app.update();
        app.add_systems(OnEnter(AppState::Loading), begin_load)
            .add_systems(OnExit(AppState::Loading), cancel_load);
        let (tx, rx) = mpsc::channel();
        {
            let mut loader = app.world_mut().resource_mut::<Loader>();
            loader.serial = 100;
            loader.job = Some(Job {
                serial: 100,
                classic: false,
                result: Mutex::new(rx),
                progress: Default::default(),
            });
            loader.wanted = Some((100, Selection::Offline(ClientOptions::default().into())));
        }
        *app.world()
            .resource::<crate::hosting::HostRequest>()
            .0
            .lock()
            .unwrap() = Some(crate::hosting::HostSelection {
            map: ClientOptions {
                map: "de_replacement_host".into(),
                ..default()
            }
            .into(),
            config: netcode::host::HostConfig::default(),
            tick_rate: 128,
            name: "Host".into(),
            team: sim::Team::CounterTerrorist,
        });
        app.update();
        app.update();
        {
            let loader = app.world().resource::<Loader>();
            assert_eq!(loader.job.as_ref().unwrap().serial, 100);
            assert!(
                matches!(&loader.wanted, Some((101, Selection::Host(selected))) if selected.map.options.map == "de_replacement_host")
            );
        }
        tx.send(Err("Discard this old map failure".into())).unwrap();
        app.update();
        assert_eq!(
            app.world()
                .resource::<Loader>()
                .job
                .as_ref()
                .unwrap()
                .serial,
            101
        );
        assert!(
            app.world()
                .resource::<crate::menu::Menu>()
                .status
                .is_empty()
        );
    }

    #[test]
    fn quit_revokes_pending_map_and_launches_while_a_downstream_exit_is_held() {
        fn hold(mut messages: ResMut<Messages<AppExit>>) {
            messages.drain().for_each(drop);
        }
        let mut app = loading_app(AppState::Loading);
        app.init_resource::<crate::hosting::HostRequest>()
            .init_resource::<crate::remote_join::JoinRequest>()
            .init_resource::<crate::menu::LoadRequest>()
            .add_systems(Last, (crate::hosting::gate_exit, hold).chain());
        app.update();
        let (tx, rx) = mpsc::channel();
        {
            let mut loader = app.world_mut().resource_mut::<Loader>();
            loader.job = Some(Job {
                serial: 100,
                classic: false,
                result: Mutex::new(rx),
                progress: Default::default(),
            });
            loader.wanted = Some((
                101,
                Selection::Host(crate::hosting::HostSelection {
                    map: ClientOptions::default().into(),
                    config: netcode::host::HostConfig::default(),
                    tick_rate: 128,
                    name: "Queued host".into(),
                    team: sim::Team::CounterTerrorist,
                }),
            ));
        }
        app.world_mut().write_message(AppExit::Success);
        app.update();
        assert!(app.should_exit().is_none());
        assert!(app.world().resource::<Loader>().wanted.is_none());
        *app.world()
            .resource::<crate::remote_join::JoinRequest>()
            .0
            .lock()
            .unwrap() = Some(crate::remote_join::Connect {
            address: "127.0.0.1:27015".parse().unwrap(),
            name: "Late join".into(),
            team: sim::Team::CounterTerrorist,
        });
        *app.world()
            .resource::<crate::menu::LoadRequest>()
            .0
            .lock()
            .unwrap() = Some(ClientOptions::default().into());
        app.add_systems(
            Update,
            (
                crate::remote_join::request,
                crate::hosting::request,
                request_load,
            )
                .chain()
                .before(poll_load),
        )
        .add_systems(OnEnter(AppState::Loading), begin_load);
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Loading);
        tx.send(Err("Old map worker finished during Quit".into()))
            .unwrap();
        app.update();
        assert!(app.world().resource::<Loader>().job.is_none());
        assert!(app.world().resource::<Loader>().wanted.is_none());
        assert!(
            !app.world()
                .contains_resource::<crate::remote::RemoteClient>()
        );
        assert!(
            app.world()
                .resource::<crate::menu::LoadRequest>()
                .0
                .lock()
                .unwrap()
                .is_none()
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
                classic: false,
                result: Mutex::new(rx),
                progress: Default::default(),
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
        app.world_mut().insert_resource(crate::prop_breakables::PropBreakableScene::default());
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
        assert!(!app.world().contains_resource::<crate::prop_breakables::PropBreakableScene>());
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
            assert!(
                !app.world()
                    .contains_resource::<crate::remote::RemoteClient>()
            );
            assert!(
                !app.world()
                    .contains_resource::<crate::remote_join::Admission>()
            );
            assert!(
                !app.world()
                    .contains_resource::<crate::remote_render_gate::RenderGate>()
            );
            assert!(
                app.world()
                    .resource::<crate::menu::Menu>()
                    .status
                    .contains("renderer")
            );
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
        assert!(matches!(
            app.world().resource::<NextState<AppState>>(),
            NextState::Pending(AppState::Loading)
        ));
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

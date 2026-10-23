//! Main-thread continuation of one Loader generation. Loading keeps its camera,
//! movie and input until every resource required by OnEnter(Match) is installed.
use crate::{session::{AppState, Loader}, *};
use bevy::ecs::world::CommandQueue;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;

pub(crate) struct WorldPreparation {
    pub textures: textures::TextureCache,
    pub shared: world_mesh::SharedHandles,
    pub moving_door_builds: Vec<(usize, world_mesh::WorldBuild)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase { World, Props, Characters, ViewModels, Ground, Finish, Complete }

#[derive(Resource)]
struct SceneImport {
    serial: u64,
    options: ClientOptions,
    loaded: Option<session::LoadedMap>,
    world: Option<WorldPreparation>,
    props: Option<props::PropImport<'static>>,
    characters: Option<characters::CharacterModels>,
    view_models: Option<viewmodel::ViewModels>,
    ground: Option<items::GroundModels>,
    cosmetics: Option<cosmetics::Catalog>,
    phase: Phase,
    published: bool,
}

/// Exists only while applying this import's private command queue, never while
/// frontend systems run. The ownership observer cannot adopt unrelated UI.
#[derive(Resource)]
pub(crate) struct ApplyingImport(pub u64);
#[derive(Component)]
pub(crate) struct ImportedEntity(pub u64);

pub(crate) fn cancel(world: &mut World) {
    let Some(import) = world.remove_resource::<SceneImport>() else { return };
    // Quit/home can revoke readiness in Last after Finish requested Match.
    // Never leave a transition which would run HUD setup without its scene.
    if world.get_resource::<NextState<AppState>>().is_some_and(|next|
        matches!(next, NextState::Pending(AppState::Match))) {
        world.resource_mut::<NextState<AppState>>().set(AppState::Menu);
    }
    let entities: Vec<_> = world.query_filtered::<(Entity, &ImportedEntity), Without<session::Persistent>>().iter(world)
        .filter(|(_, owner)| owner.0 == import.serial).map(|(entity, _)| entity).collect();
    for entity in entities {
        if world.get_entity(entity).is_ok() { world.despawn(entity); }
    }
    // Until Finish, all resources/handles belong to the continuation. Dropping
    // it releases CPU caches and asset owners; no frontend resource is reset.
    if import.published { session::clear_match(world); }
}

pub(crate) fn admit(world: &mut World) {
    let import = world.remove_resource::<SceneImport>().expect("scene ready before Match");
    assert!(import.phase == Phase::Complete && import.published, "incomplete scene cannot enter Match");
    // MatchEntity now follows the ordinary match teardown path.
}

pub(crate) fn update(world: &mut World) {
    world.flush();
    let state = *world.resource::<State<AppState>>().get();
    let generation = world.resource::<Loader>().generation();
    let importing = world.resource::<Loader>().importing;
    let current = state == AppState::Loading && generation.is_some() && generation == importing;
    if world.get_resource::<SceneImport>().is_some_and(|import|
        !current || Some(import.serial) != generation) {
        cancel(world);
    }
    if !current { return; }
    let mut import = match world.remove_resource::<SceneImport>() {
        Some(import) => import,
        None => {
            let loaded = world.resource::<PendingLevel>().0.lock().unwrap().take();
            let Some(loaded) = loaded else { return };
            SceneImport {
                serial: generation.unwrap(), options: world.resource::<Options>().0.clone(),
                loaded: Some(loaded), world: None, props: None, characters: None,
                view_models: None, ground: None, cosmetics: None, phase: Phase::World, published: false,
            }
        }
    };
    if import.phase == Phase::Complete {
        world.insert_resource(import);
        return;
    }
    let mut queue = CommandQueue::default();
    // All Assets and Commands remain on the main thread. Resource scopes allow
    // a private command queue without borrowing Assets through the World.
    world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
        world.resource_scope(|world, mut materials: Mut<Assets<WorldMaterial>>| {
            world.resource_scope(|world, mut images: Mut<Assets<Image>>| {
                world.resource_scope(|world, mut bindposes: Mut<Assets<SkinnedMeshInverseBindposes>>| {
                    let mut commands = Commands::new(&mut queue, world);
                    import.step(&mut commands, &mut meshes, &mut materials, &mut images, &mut bindposes);
                });
            });
        });
    });
    let serial = import.serial;
    let complete = import.phase == Phase::Complete;
    let status = import.status();
    world.insert_resource(import);
    if !apply_commands(world, serial, &mut queue) {
        if world.get_resource::<SceneImport>().is_some_and(|import| import.serial == serial) { cancel(world); }
        return;
    }
    if complete { world.resource_mut::<SceneImport>().published = true; }
    world.resource_mut::<Loader>().status = status;
    if complete { world.resource_mut::<NextState<AppState>>().set(AppState::Match); }
}

fn apply_commands(world: &mut World, serial: u64, queue: &mut CommandQueue) -> bool {
    // CommandQueue::apply first drains World commands. Flush those before
    // installing the marker so unrelated frontend commands cannot be adopted.
    world.flush();
    let current = *world.resource::<State<AppState>>().get() == AppState::Loading
        && world.resource::<Loader>().generation() == Some(serial)
        && world.resource::<Loader>().importing == Some(serial)
        && world.get_resource::<SceneImport>().is_some_and(|import| import.serial == serial);
    if !current {
        // Drop the bundles and handles instead of applying a retired import's
        // commands to a newer scene or the frontend. The queue never survives
        // this exclusive update, including cancellation/error paths.
        drop(std::mem::take(queue));
        return false;
    }
    world.insert_resource(ApplyingImport(serial));
    queue.apply(world);
    world.remove_resource::<ApplyingImport>();
    true
}

impl SceneImport {
    fn status(&self) -> String {
        match self.phase {
            Phase::World => "Preparing map geometry…".into(),
            Phase::Props => self.props.as_ref().map(|props| props.progress(&self.loaded.as_ref().unwrap().level))
                .unwrap_or_else(|| "Preparing props…".into()),
            Phase::Characters => "Preparing players…".into(),
            Phase::ViewModels => "Preparing weapons…".into(),
            Phase::Ground => "Preparing world models…".into(),
            Phase::Finish | Phase::Complete => "Preparing match…".into(),
        }
    }

    fn step(&mut self, commands: &mut Commands, meshes: &mut Assets<Mesh>,
        materials: &mut Assets<WorldMaterial>, images: &mut Assets<Image>,
        bindposes: &mut Assets<SkinnedMeshInverseBindposes>) {
        let opts = &self.options;
        let level = &self.loaded.as_ref().unwrap().level;
        match self.phase {
            Phase::World => {
                self.world = Some(crate::prepare_world(commands, level, opts, meshes, materials, images));
                self.phase = Phase::Props;
            }
            Phase::Props => {
                if std::env::var_os("CSR_NO_PROPS").is_some() {
                    self.phase = Phase::Characters;
                    return;
                }
                // Building the query scratch is indivisible and happens once.
                if self.props.is_none() {
                    self.props = Some(props::PropImport::new(level,
                        collision::PropLightingQuery::owned(level.collision.clone())));
                    return;
                }
                let world = self.world.as_mut().unwrap();
                let props = self.props.as_mut().unwrap();
                if props.step(commands, level, meshes, materials, images, &mut world.textures,
                    &world.shared, opts.exposure, props::Sun::from_bsp(&level.bsp), level.bsp.sky_camera(),
                    std::time::Duration::from_millis(4)) {
                    let stats = &props.stats;
                    info!("props: {} instances, {} tris, {} draws ({} in 3D skybox), {} materials, {} failed models",
                        stats.instances, stats.triangles, stats.draws, stats.sky_draws, stats.materials, stats.failed_models);
                    self.props = None;
                    self.phase = Phase::Characters;
                }
            }
            Phase::Characters => {
                let world = self.world.as_mut().unwrap();
                self.characters = Some(characters::load(level, meshes, materials, images,
                    &mut world.textures, &world.shared, bindposes, opts.exposure));
                self.phase = Phase::ViewModels;
            }
            Phase::ViewModels => {
                let world = self.world.as_mut().unwrap();
                let cosmetics = cosmetics::Catalog::load(&level.vfs)
                    .unwrap_or_else(|error| { error!("Cosmetics: {error}"); default() });
                self.view_models = Some(viewmodel::load(level, &cosmetics,
                    &self.characters.as_ref().unwrap().weapons, meshes, materials, images,
                    &mut world.textures, &world.shared, bindposes, opts.exposure));
                self.cosmetics = Some(cosmetics);
                self.phase = Phase::Ground;
            }
            Phase::Ground => {
                let world = self.world.as_mut().unwrap();
                self.ground = Some(items::load(level, meshes, materials, images,
                    &mut world.textures, &world.shared, bindposes, opts.exposure));
                self.phase = Phase::Finish;
            }
            Phase::Finish => {
                let mut world = self.world.take().unwrap();
                doors::spawn(commands, level, &world.moving_door_builds, meshes, materials, images,
                    &mut world.textures, &world.shared, bindposes, opts.exposure);
                prop_breakables::spawn(commands, level, meshes, materials, images,
                    &mut world.textures, &world.shared, bindposes, opts.exposure);
                info!("textures: {} compressed, {} decoded, {} missing",
                    world.textures.stats_compressed, world.textures.stats_decoded, world.textures.stats_missing);
                let ground = self.ground.take().unwrap().with_load_context(world.textures, world.shared, opts.exposure);
                crate::finish_level(commands, self.loaded.take().unwrap(), opts,
                    self.characters.take().unwrap(), self.view_models.take().unwrap(), ground,
                    self.cosmetics.take().unwrap());
                self.phase = Phase::Complete;
            }
            Phase::Complete => unreachable!(),
        }
    }
}

#[cfg(test)]
pub(crate) fn fixture_level() -> level::Level {
    let mut bytes = vec![0; 8 + 64 * 16 + 4];
    bytes[..4].copy_from_slice(b"VBSP");
    bytes[4..8].copy_from_slice(&21i32.to_le_bytes());
    let mut fs = vfs::Vfs::new();
    fs.push_back("synthetic", vfs::Mount::Memory(std::collections::BTreeMap::from([
        ("maps/import_fixture.bsp".into(), std::sync::Arc::<[u8]>::from(bytes)),
    ])));
    level::Level::load_from_vfs(content::ContentId::new(content::GameOrigin::Mod,
        content::ContentKind::Map, "import_fixture"), "import_fixture", "unused".into(), fs).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import(serial: u64) -> SceneImport {
        SceneImport { serial, options: ClientOptions::default(), loaded: None,
            world: None, props: None, characters: None, view_models: None,
            ground: None, cosmetics: None, phase: Phase::Props, published: false }
    }

    fn info() -> netcode::protocol::ServerInfo {
        netcode::protocol::ServerInfo {
            version: netcode::protocol::VERSION, players: 0, max_players: 4,
            tick_rate: 128, snapshot_interval_ticks: 2, max_unlag_ticks: 128,
            interpolation_allowance_ticks: 2,
            content: netcode::protocol::ContentManifest {
                map_id: "csgo_map_import_fixture".into(), map_name: "import_fixture".into(),
                map_hash: "0".repeat(40), catalog_hash: "1".repeat(40),
            },
        }
    }

    #[test]
    fn private_staged_queue_and_cancellation_preserve_frontend_and_other_generations() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::Loading).init_resource::<Loader>()
            .init_resource::<menu::Menu>().add_observer(session::own_entity::<Transform>);
        app.world_mut().resource_mut::<Loader>().bind_host_generation(7, info());
        app.world_mut().resource_mut::<Loader>().importing = Some(7);
        app.world_mut().insert_resource(import(7));
        app.world_mut().resource_mut::<menu::Menu>().status = "Keep frontend status".into();
        // Ordinary World commands must flush before ApplyingImport is installed.
        let ordinary = app.world_mut().commands().spawn(Transform::IDENTITY).id();
        let mut queue = CommandQueue::default();
        let (staged, persistent, child) = {
            let mut commands = Commands::new(&mut queue, app.world());
            let staged = commands.spawn(Transform::IDENTITY).id();
            let persistent = commands.spawn((session::Persistent, Transform::IDENTITY)).id();
            let child = commands.spawn((Transform::IDENTITY, ChildOf(staged))).id();
            (staged, persistent, child)
        };
        let other = app.world_mut().spawn((ImportedEntity(8), session::MatchEntity)).id();
        assert!(app.world().get_entity(staged).is_err(), "work is still staged");
        assert!(apply_commands(app.world_mut(), 7, &mut queue));
        assert_eq!(app.world().get::<ImportedEntity>(staged).unwrap().0, 7);
        assert_eq!(app.world().get::<ImportedEntity>(child).unwrap().0, 7);
        assert!(app.world().get::<ImportedEntity>(ordinary).is_none());
        assert!(app.world().get::<session::MatchEntity>(persistent).is_none());
        assert!(!app.world().contains_resource::<ApplyingImport>());
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Match);
        app.world_mut().resource_mut::<Loader>().invalidate();
        cancel(app.world_mut());
        cancel(app.world_mut());
        for entity in [staged, child] { assert!(app.world().get_entity(entity).is_err()); }
        for entity in [ordinary, persistent, other] { assert!(app.world().get_entity(entity).is_ok()); }
        assert_eq!(app.world().resource::<menu::Menu>().status, "Keep frontend status");
        assert!(!app.world().contains_resource::<SceneImport>());
        assert!(matches!(app.world().resource::<NextState<AppState>>(), NextState::Pending(AppState::Menu)));
        // Cancel with another slice's commands still staged. Retired bundles
        // must never spawn, install resources, or adopt ordinary loading UI.
        app.world_mut().insert_resource(import(7));
        let retired = Commands::new(&mut queue, app.world()).spawn(Transform::IDENTITY).id();
        assert!(!apply_commands(app.world_mut(), 7, &mut queue));
        cancel(app.world_mut());
        assert!(queue.is_empty());
        assert!(app.world().get_entity(retired).is_err());
        assert!(app.world().get_entity(ordinary).is_ok());
        assert_eq!(app.world().resource::<menu::Menu>().status, "Keep frontend status");
    }

    #[test]
    fn retired_import_is_discarded_before_requiring_assets_or_publishing_readiness() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::Loading).init_resource::<Loader>()
            .insert_resource(PendingLevel(Mutex::new(None)));
        app.world_mut().resource_mut::<Loader>().bind_host_generation(8, info());
        app.world_mut().resource_mut::<Loader>().importing = Some(8);
        // A retired import never runs even if it still owns scene entities.
        app.world_mut().insert_resource(import(7));
        let retired = app.world_mut().spawn(ImportedEntity(7)).id();
        update(app.world_mut());
        assert!(app.world().get_entity(retired).is_err());
        assert!(!app.world().contains_resource::<SceneImport>());
        assert!(!app.world().contains_resource::<LoadedLevel>());
        assert!(!matches!(app.world().resource::<NextState<AppState>>(), NextState::Pending(AppState::Match)));
    }
}

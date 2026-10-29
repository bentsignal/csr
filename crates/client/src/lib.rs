//! Bevy client for CSGO Rewrite: renders a map from the player's CS:GO
//! install and plays it with CS:GO movement.
//!
//! [`CsgoPlugin`] is self-contained so the whole game can be added to
//! another Bevy app (the planned multi-game "stitch").

pub mod audio;
use bevy::audio::AddAudioSource;
pub mod audio_options;
mod menu_music;
pub mod session;
mod scene_import;
pub mod frontend;
mod frontend_display;
mod frontend_inventory;
mod frontend_game;
mod frontend_loading;
mod frontend_loading_presentation;
mod frontend_lobby;
mod frontend_options;
mod frontend_flow;
mod frontend_acceptance;
mod frontend_input;
mod frontend_pause_surface;
mod pack_flow;
mod pack_picker;
mod pack_ui;
pub mod foley;
mod sound_probe;
mod playtest;
pub mod surfaces;
pub mod tracers;
mod muzzle_flash;
pub mod inventory;
pub mod cosmetics;
pub mod cosmetic_catalog;
mod classic_cosmetics;
pub mod skin_finish;
pub mod decorations;
pub mod cosmetic_geometry;
pub mod case_ui;
pub mod menu_scene;
mod menu_blur;
pub mod settings;
mod controller;
mod console;
pub mod menu;
pub mod feedback;
mod impact_decals;
mod impact_decal_material;
pub mod utility_fx;
mod particle_data;
mod particle_material;
mod particle_depth;
mod utility_particles;
pub mod buymenu;
pub mod items;
pub mod interpolation;
mod input_latency;
#[cfg(feature = "targeted-render-trace")]
mod targeted_render_trace;
pub mod characters;
pub mod player_anim;
pub mod skinning;
mod skinning_bounds;
pub mod viewmodel;
pub mod coords;
pub mod effects;
mod kill_feed;
mod combat_hud;
mod hud_layout;
pub mod hud;
pub mod ui_assets;
pub mod hud_art;
mod radar_view;
pub mod localize;
pub mod material;
pub mod shadow_diagnostic;
pub mod player;
pub mod remote;
pub mod remote_map;
pub mod remote_render_gate;
pub mod remote_scene_assets;
pub mod remote_servers;
pub mod frontend_servers_data;
pub mod frontend_servers;
mod server_browser;
mod browser_flow;
pub mod remote_join;
pub mod hosting;
mod host_sharing;
mod host_sharing_ui;
mod menu_performance;
mod display_proof;
mod remote_flow;
mod bot_performance;
pub mod xr_input;
pub mod props;
mod doors;
mod breakables;
mod prop_breakables;
mod window_fade;
mod studio_pose;
pub mod scoreboard;
pub mod scope;
pub mod sky;
pub mod textures;
pub mod world_mesh;
pub mod visibility;

use bevy::asset::embedded_asset;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use material::WorldMaterial;
use player::{LocalPlayer, PlayerCamera, SimWorld};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;
use session::AppState;

const BOT_NAMES_T: [&str; 8] = ["Albert", "Crusher", "Dragomir", "Gunner", "Kasper", "Minh", "Rock", "Vitaliy"];
const BOT_NAMES_CT: [&str; 8] = ["Brian", "Darryl", "Elliot", "Frank", "Harold", "Jon", "Pete", "Walt"];

/// CS:GO's default 90° horizontal FOV at 4:3, as a vertical angle.
pub const VERTICAL_FOV_DEG: f32 = 73.74;

#[derive(Clone, Debug)]
pub struct ClientOptions {
    pub map: String,
    /// Take a screenshot after `frames` frames, then exit.
    pub screenshot: Option<PathBuf>,
    pub frames: u32,
    /// Start in noclip at this Source position with (pitch, yaw).
    pub camera: Option<(glam::Vec3, f32, f32)>,
    pub exposure: f32,
    pub width: u32,
    pub height: u32,
    /// Explicit session-only windowed size; defaults remain screenshot controls.
    pub size_explicit: bool,
    /// Enemy bots; the local team gets one fewer so teams are even.
    pub bots: u32,
    /// Original picker value (0..5), resolved from content; None preserves legacy defaults.
    pub bot_difficulty: Option<u8>,
    /// Play as a Terrorist instead of a Counter-Terrorist.
    pub terrorist: bool,
    /// Deathmatch (respawns, fixed loadout) instead of competitive rounds.
    pub deathmatch: bool,
    /// Offline utility sandbox: full grenade inventory on every respawn.
    pub grenade_practice: bool,
    /// Offline in-game armory and scenario controls.
    pub playtest: bool,
    pub direct: bool,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            map: "de_dust2".into(),
            screenshot: None,
            frames: 120,
            camera: None,
            exposure: 2.0,
            width: 1600,
            height: 900,
            size_explicit: false,
            bots: 5,
            bot_difficulty: None,
            terrorist: false,
            deathmatch: false,
            grenade_practice: false,
            playtest: false,
            direct: false,
        }
    }
}

#[derive(Resource)]
pub struct LevelInfo {
    pub name: String,
}

/// The loaded level, handed to the startup system once.
#[derive(Resource)]
struct PendingLevel(Mutex<Option<session::LoadedMap>>);

#[derive(Resource)]
pub struct LoadedLevel(pub level::Level);

/// Install data used by the front end without loading a map.
#[derive(Resource)]
pub struct GameData {
    pub game_dir: PathBuf,
    pub vfs: std::sync::Arc<vfs::Vfs>,
    pub catalog: std::sync::Arc<weapons::Catalog>,
}

impl GameData {
    fn load() -> Result<Self, String> {
        let (game_dir, vfs) = level::mount_game(None).map_err(|e| e.to_string())?;
        let items = vfs.read_string("scripts/items/items_game.txt").ok_or("items_game.txt not found")?;
        let catalog = std::sync::Arc::new(weapons::Catalog::from_items_game(&items).map_err(|e| e.to_string())?);
        Ok(Self { game_dir, vfs: std::sync::Arc::new(vfs), catalog })
    }
}

#[derive(Resource, Clone)]
struct Options(ClientOptions);

pub struct CsgoPlugin;
mod movement_modes;
mod camera_transition;
mod board_presentation;
mod board_audio;
mod board_hud;
mod board_stream;

impl Plugin for CsgoPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/world.wgsl");
        embedded_asset!(app, "shaders/radar.wgsl");
        embedded_asset!(app, "shaders/particle.wgsl");
        embedded_asset!(app, "shaders/impact_decal.wgsl");
        app.add_plugins(menu_blur::PostProcessPlugin)
            .add_plugins(scope::OpticBlurPlugin)
            .add_plugins(UiMaterialPlugin::<radar_view::RadarMaterial>::default())
            .add_plugins(MaterialPlugin::<WorldMaterial>::default())
            .add_plugins(MaterialPlugin::<particle_material::ParticleMaterial>::default())
            .add_plugins(MaterialPlugin::<impact_decal_material::ImpactDecalMaterial>::default())
            .add_plugins(particle_depth::ParticleDepthPlugin)
            .init_state::<AppState>()
            .init_resource::<session::Loader>()
            .init_resource::<remote_join::JoinRequest>()
            .init_resource::<hosting::HostRequest>()
            .init_resource::<hosting::Hosting>()
            .init_resource::<host_sharing_ui::Ui>()
            .init_resource::<pack_ui::PackUi>()
            .init_resource::<server_browser::Browser>()
            .add_observer(session::own_entity::<Transform>)
            .add_observer(session::own_entity::<Node>)
            .add_observer(session::own_entity::<bevy::audio::AudioPlayer>)
            .insert_resource(Time::<Fixed>::from_hz(sim::DEFAULT_TICK_RATE as f64))
            .add_systems(Startup, (ui_assets::load, case_ui::setup, menu_scene::setup, menu::setup, frontend::setup, server_browser::setup, host_sharing_ui::setup).chain())
            .add_systems(Update, (frontend::update, host_sharing_ui::update, pack_ui::update, server_browser::update, server_browser::input, server_browser::draw, pack_ui::scroll, pack_ui::draw, frontend::input, frontend::cancel_loading).chain())
            .add_systems(Last, (hosting::gate_exit, host_sharing_ui::gate_exit, server_browser::gate_exit, pack_ui::gate_exit).chain().after(bevy::window::ExitSystems))
            .add_systems(Update, (menu::apply, audio::update, menu_music::update, menu_music::update_gain).chain().after(player::update_camera))
            .add_systems(Last, audio::prune)
            .add_systems(PreUpdate, player::focus_input.after(bevy::input::InputSystems))
            .add_systems(OnEnter(AppState::Loading), session::begin_load)
            .add_systems(OnExit(AppState::Loading), session::cancel_load)
            .add_systems(OnEnter(AppState::Match), (scene_import::admit, hud::setup, hud_art::setup, scope::setup, feedback::setup, utility_fx::setup, hud::mark_roots, session::match_ready).chain())
            .add_systems(OnExit(AppState::Match), session::clear_match)
            .add_systems(OnEnter(AppState::Menu), remote_join::home)
            .add_systems(Update, (remote_join::request, hosting::request, remote::drive.run_if(resource_exists::<remote::RemoteClient>), hosting::update, remote_join::update, session::request_load, session::poll_load, scene_import::update).chain().before(frontend::update).before(player::update_camera))
            .add_systems(
                RunFixedMainLoop,
                (menu::input, buymenu::input, player::look, player::grab_cursor).chain()
                    .run_if(console::allows_input)
                    .run_if(remote_join::ready)
                    .in_set(bevy::app::RunFixedMainLoopSystems::BeforeFixedMainLoop),
            )
            // Home owns the same controls editor before any LocalPlayer or
            // match exists. Keep gameplay's readiness condition unchanged.
            .add_systems(RunFixedMainLoop, menu::input
                .run_if(console::allows_input).run_if(menu::home_controls)
                .in_set(bevy::app::RunFixedMainLoopSystems::BeforeFixedMainLoop))
            .add_systems(Update, (menu::draw.in_set(menu::ControlsDraw), menu::scroll_controls, menu::hover, ui_assets::fonts)
                .chain().after(frontend::update).run_if(menu::home_controls))
            .add_systems(Update, player::update_camera.run_if(remote_join::ready))
            .init_resource::<interpolation::RenderPoses>()
            .init_resource::<tracers::Muzzles>()
            .init_resource::<skin_finish::SkinTextures>()
            .init_resource::<decorations::Decorations>()
            .init_resource::<tracers::Tracers>()
            .init_resource::<muzzle_flash::Flashes>()
            .init_resource::<tracers::InspectRequest>()
            .init_resource::<board_presentation::HeldBoard>()
            .init_resource::<board_audio::BoardAudio>()
            .add_audio_source::<board_stream::Stream>()
            .init_resource::<menu::Menu>()
            .insert_resource(inventory::Inventory::load())
            .init_resource::<player::InputBlockFrames>()
            .init_resource::<buymenu::BuyMenu>()
            .init_resource::<scoreboard::Scoreboard>()
            .init_resource::<items::GroundInstances>()
            .init_resource::<effects::PendingEvents>()
            .init_resource::<audio::Listener>()
            .init_resource::<foley::Foley>()
            .init_resource::<sound_probe::SoundProbe>()
            .init_resource::<effects::Effects>()
            .init_resource::<characters::Characters>()
            .init_resource::<viewmodel::ViewModelState>()
            .init_resource::<viewmodel::HeldWick>()
            .add_systems(
                Update,
                (
                    (skin_finish::record, skin_finish::sync_loadout.run_if(remote_join::offline), muzzle_flash::capture, effects::process.after(foley::update)).chain(),
                    (feedback::update, utility_fx::update).chain(),
                    sky::follow_camera,
                    visibility::update,
                    effects::draw,
                    // The held board is drawn from this frame's posed knife handle.
                    (viewmodel::update, foley::inspect, utility_fx::update_wick).chain(),
                    (characters::sync, movement_modes::draw,movement_modes::full_board::draw,board_audio::update).chain(),
                    items::sync,
                    (doors::sync, breakables::sync, window_fade::update).chain(),
                    (tracers::draw, muzzle_flash::draw).chain(),
                    scope::update,
                    buymenu::draw,
                    scoreboard::draw,
                    (hud_art::layout, hud_art::update).chain().run_if(|| std::env::var_os("CSR_NO_HUD").is_none()),
                    (menu_scene::update.run_if(frontend::native_enabled),skin_finish::update,decorations::update).chain(),
                    (menu::draw.in_set(menu::ControlsDraw).after(frontend::update), (case_ui::input, case_ui::draw, case_ui::animate).chain().run_if(frontend::native_enabled)).chain(),
                    (menu::scroll_controls, menu::cards).chain().run_if(frontend::engine_menu_enabled),
                    menu::hover.run_if(frontend::engine_menu_enabled),
                    ui_assets::fonts,
                    (hud::update,board_hud::update).chain().run_if(|| std::env::var_os("CSR_NO_HUD").is_none()),
                )
                    .chain()
                    .run_if(remote_join::ready)
                    .after(player::update_camera),
            )
            .add_systems(FixedUpdate, (player::tick, foley::update).chain().run_if(remote_join::ready).run_if(remote_join::offline))
            .add_systems(Update, hud::menu_visibility.run_if(in_state(AppState::Match)).after(remote_join::update))
            .add_systems(Update, foley::update.run_if(remote_join::ready).run_if(resource_exists::<remote::RemoteClient>).after(remote_join::update));
        menu::install_controls_retirement(app);
        if let Err(error) = remote_render_gate::install(app) { error!("Remote render preparation unavailable: {error:?}"); }
        shadow_diagnostic::install(app);
        board_hud::install(app);
        skinning_bounds::install(app);
        console::install(app);
        playtest::install(app);
        kill_feed::install(app);
    }
}

/// Loads the map and runs the game window. Returns a process exit code.
pub fn run(opts: ClientOptions) -> i32 {
    let data = match GameData::load() {
        Ok(data) => data,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let video_settings = settings::Settings::load();
    let (resolution, window_mode) = frontend_display::startup(&opts, &video_settings);
    let load_request = menu::LoadRequest::default();
    let mut app = App::new();
    #[cfg(feature = "targeted-render-trace")]
    let trace_guard = match targeted_render_trace::configure(&mut app, &data.game_dir) {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("targeted render trace refused: {error}");
            return 1;
        }
    };
    let plugins = input_latency::desktop_plugins();
    #[cfg(feature = "targeted-render-trace")]
    let plugins = plugins.set(bevy::log::LogPlugin {
        custom_layer: targeted_render_trace::layer,
        ..default()
    });
    app.add_plugins(
        plugins
            .set(WindowPlugin {
                close_when_requested: false,
                primary_window: Some(Window {
                    title: format!("CSGO Rewrite — {}", opts.map),
                    resolution,
                    mode: window_mode,
                    present_mode: input_latency::present_mode(video_settings.vsync),
                    desired_maximum_frame_latency: input_latency::surface_frame_latency(),
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins(FrameTimeDiagnosticsPlugin::default())
    .insert_resource(ClearColor(Color::srgb(0.55, 0.68, 0.85)))
    .insert_resource(LevelInfo { name: opts.map.clone() })
    .insert_resource(PendingLevel(Mutex::new(None)))
    .insert_resource(inventory::Collection(data.catalog.clone()))
    .insert_resource(cosmetics::Catalog::load(&data.vfs).unwrap_or_else(|e| { error!("Cosmetics: {e}"); default() }))
    .insert_resource(audio::SoundLibrary::load(&data.vfs))
    .insert_resource(menu_music::MenuMusic::load(&data.vfs))
    .insert_resource(localize::Localization::load(&data.vfs))
    .insert_resource(data)
    .insert_resource(Options(opts.clone()))
    .insert_resource(video_settings)
    .insert_resource(load_request.clone())
    .insert_resource(player::InputEnabled(opts.screenshot.is_none()))
    .add_plugins(CsgoPlugin)
    .insert_state(if browser_flow::enabled() || remote_flow::enabled() || pack_ui::proof_enabled() || frontend_flow::enabled() || std::env::var("CSR_TEST_MENU").as_deref() == Ok("main") { AppState::Menu }
        else if opts.direct || opts.screenshot.is_some() { AppState::Loading } else { AppState::Menu });
    bot_performance::install(&mut app);
    material::install_lighting(&mut app);
    input_latency::install_frame_log(&mut app);
    #[cfg(feature = "targeted-render-trace")]
    targeted_render_trace::install(&mut app);
    if frontend_flow::enabled() {
        if frontend_acceptance::enabled() {
            frontend_acceptance::install(&mut app);
            if frontend_acceptance::match_case() {
                // Reuse the existing authored launch only. The acceptance case
                // owns every Pause/Options transition through public input.
                app.add_systems(Update, frontend_flow::update.after(frontend::update)
                    .run_if(|state: Res<State<AppState>>| *state.get() != AppState::Match));
            }
        } else {
            app.add_systems(Update, frontend_flow::update.after(frontend::update));
        }
    }
    if opts.screenshot.is_some() {
        // Automated runs never have focus; don't let the unfocused 60 Hz
        // throttle skew frame-rate readings.
        app.insert_resource(bevy::winit::WinitSettings {
            focused_mode: bevy::winit::UpdateMode::Continuous,
            unfocused_mode: bevy::winit::UpdateMode::Continuous,
        })
        .add_systems(First, pack_ui::proof.run_if(pack_ui::proof_enabled))
        .add_systems(First, remote_flow::update.run_if(remote_flow::enabled))
        .add_systems(First, browser_flow::update.run_if(browser_flow::enabled))
        .add_systems(Update, smoke_test.run_if(remote_join::ready).run_if(remote_join::offline).run_if(|| !remote_flow::enabled() && !browser_flow::enabled()))
        .add_systems(Update, frontend::capture_menu.run_if(|state: Res<State<AppState>>| {
            !browser_flow::enabled() && !remote_flow::enabled() && (*state.get() == AppState::Menu || (*state.get() == AppState::Match
                && std::env::var("CSR_TEST_MENU").as_deref() == Ok("pause")))
        }));
    }
    if menu_performance::enabled() {
        app.init_resource::<menu_performance::Proof>()
            .add_systems(First, menu_performance::motion)
            .add_systems(Update, menu_performance::measure.after(frontend::input));
    }
    if display_proof::enabled() {
        app.init_resource::<display_proof::Proof>()
            .add_systems(First, display_proof::request)
            .add_systems(Update, display_proof::measure.after(menu::apply).after(frontend::input));
    }
    let result = app.run();
    #[cfg(feature = "targeted-render-trace")]
    drop(trace_guard);
    match result { AppExit::Success=>0,AppExit::Error(code)=>code.get() as i32 }

}

#[allow(clippy::too_many_arguments)]
fn prepare_world(
    mut commands: &mut Commands,
    level: &level::Level,
    opts: &ClientOptions,
    mut meshes: &mut Assets<Mesh>,
    mut materials: &mut Assets<WorldMaterial>,
    mut images: &mut Assets<Image>,
) -> scene_import::WorldPreparation {
    let t = Instant::now();
    let mut textures = textures::TextureCache::new(&mut images);

    // World geometry, split into the playable world and the 3D skybox.
    let sky = level.bsp.sky_camera();
    let sky_faces = sky
        .map(|s| world_mesh::sky_faces(&level.bsp, &s))
        .unwrap_or_default();
    let atlas = world_mesh::LightmapAtlas::build(&level.bsp);
    // Isolate only brush models here. Studio props have independent meshes;
    // immediate Kill dependencies share the same health-driven visibility.
    let breakable_models: Vec<_> = level.collision.breakables.iter().enumerate()
        .filter(|(_, def)| def.studio.is_none())
        .map(|(index, def)| (vec![index], def.model, def.origin, true))
        .chain(level.collision.breakable_dependencies.iter().map(|dep| {
            let visible = level.bsp.entities[dep.entity].i64("rendermode") != Some(10);
            (dep.killed_by.clone(), dep.model, dep.origin, visible)
        })).collect();
    let breakable_faces: std::collections::HashSet<_> = breakable_models.iter().flat_map(|(_, model, _, _)| {
        let model = &level.bsp.models[*model];
        model.first_face.max(0) as usize..(model.first_face.max(0) as usize + model.num_faces.max(0) as usize).min(level.bsp.faces.len())
    }).collect();
    let moving_door_builds: Vec<_> = level.collision.doors.iter().enumerate()
        .filter(|(_, door)| level.bsp.entities[door.entity].i64("rendermode") != Some(10))
        .filter_map(|(index, door)| door.brush_model.map(|model|
            (index, world_mesh::build_brush_model(&level.bsp, &atlas, model, glam::Vec3::ZERO))))
        .collect();
    let moving_door_faces: std::collections::HashSet<_> = level.collision.doors.iter()
        .filter_map(|door| door.brush_model).flat_map(|index| {
            let model = &level.bsp.models[index];
            model.first_face.max(0) as usize..(model.first_face.max(0) as usize + model.num_faces.max(0) as usize).min(level.bsp.faces.len())
        }).collect();
    // Fade ownership never takes faces from sky, door or breakable lifecycles.
    let unavailable = sky_faces.iter().chain(&breakable_faces).chain(&moving_door_faces).copied().collect();
    let window_builds = window_fade::build(&level, &atlas, &unavailable, &mut textures, &mut images);
    let window_faces: std::collections::HashSet<_> = window_builds.iter().flat_map(|build| build.faces()).collect();
    let main = world_mesh::build(&level.bsp, &atlas, &|f| !sky_faces.contains(&f) && !breakable_faces.contains(&f) && !moving_door_faces.contains(&f) && !window_faces.contains(&f));
    let skybox = world_mesh::build(&level.bsp, &atlas, &|f| sky_faces.contains(&f) && !breakable_faces.contains(&f) && !moving_door_faces.contains(&f));
    let breakable_builds: Vec<_> = breakable_models.iter().filter(|(_, _, _, visible)| *visible).map(|(owners, model, origin, _)| {
        let def = &level.bsp.models[*model];
        (owners.clone(), sky_faces.contains(&(def.first_face.max(0) as usize)),
            world_mesh::build_brush_model(&level.bsp, &atlas, *model, *origin))
    }).collect();
    let main_overlays = world_mesh::build_overlays(&level.bsp, &atlas, &|f| !sky_faces.contains(&f) && !breakable_faces.contains(&f));
    let sky_overlays = world_mesh::build_overlays(&level.bsp, &atlas, &|f| sky_faces.contains(&f) && !breakable_faces.contains(&f));
    let breakable_overlay_builds: Vec<_> = breakable_models.iter().filter(|(_, _, _, visible)| *visible).map(|(owners, model, _, _)| {
        let model = &level.bsp.models[*model];
        let range = model.first_face.max(0) as usize..model.first_face.max(0) as usize + model.num_faces.max(0) as usize;
        (owners.clone(), sky_faces.contains(&range.start), world_mesh::build_overlays(&level.bsp, &atlas, &|f| range.contains(&f)))
    }).collect();
    let lightmap = images.add(atlas.to_image());
    drop(atlas);
    let world_shared = world_mesh::SharedHandles {
        lightmap: lightmap.clone(),
    };
    let window_draws = window_fade::spawn(&mut commands, &level, &window_builds, &mut meshes,
        &mut materials, &mut images, &mut textures, &world_shared, opts.exposure);
    sky::spawn_2d(&mut commands, &level, &mut meshes, &mut materials, &mut images, &mut textures, &world_shared);
    let mut world_draws = window_draws;
    let mut world_materials: std::collections::HashMap<String, Option<Handle<WorldMaterial>>> = Default::default();
    let mut world_builds = vec![(false, None, &main), (true, None, &skybox)];
    world_builds.extend(breakable_builds.iter().map(|(index, in_sky, build)| (*in_sky, Some(index), build)));
    for (in_sky, breakable, build) in world_builds {
        for (name, mesh) in &build.meshes {
            let mat = world_materials
                .entry(name.clone())
                .or_insert_with(|| {
                    let source = vmt::Material::load(name, &|path| level.vfs.read_string(path));
                    // Water needs its own reflection/refraction presentation.
                    // Until then, omit the opaque fallback at the owner's request.
                    // The simulation still retains the map's water volumes.
                    if source.as_ref().is_some_and(vmt::Material::is_water) { return None; }
                    Some(materials.add(world_mesh::material_from_vmt(source.as_ref(), &level.vfs, &mut textures, &mut images, &world_shared, opts.exposure, true)))
                })
                .clone();
            let Some(mat) = mat else { continue };
            let mut e = commands.spawn((Mesh3d(meshes.add(mesh.clone())), MeshMaterial3d(mat)));
            if let Some(index) = breakable { e.insert(breakables::BreakableMesh(index.clone())); }
            match (in_sky, sky) {
                (true, Some(s)) => {
                    e.insert((sky::sky_transform(&s), sky::sky_layer()));
                }
                _ => {
                    e.insert((Transform::IDENTITY, visibility::MapClusters::mesh(&level.bsp, mesh)));
                }
            }
            world_draws += 1;
        }
    }
    let mut overlay_materials: std::collections::HashMap<(String, u8), Option<Handle<WorldMaterial>>> = Default::default();
    let mut overlay_draws = 0;
    let mut overlay_builds = vec![(false, None, &main_overlays), (true, None, &sky_overlays)];
    overlay_builds.extend(breakable_overlay_builds.iter().map(|(index, in_sky, build)| (*in_sky, Some(index), build)));
    for (in_sky, breakable, build) in overlay_builds {
        for (name, order, mesh) in &build.meshes {
            let mat = overlay_materials.entry((name.clone(), *order)).or_insert_with(|| {
                let material = world_mesh::overlay_material(name, *order, &level.vfs, &mut textures, &mut images, &world_shared, opts.exposure);
                if material.is_none() { warn!("overlay material unavailable/unsupported: {name}"); }
                material.map(|m| materials.add(m))
            }).clone();
            let Some(mat) = mat else { continue };
            let mut e = commands.spawn((Mesh3d(meshes.add(mesh.clone())), MeshMaterial3d(mat)));
            if let Some(index) = breakable { e.insert(breakables::BreakableMesh(index.clone())); }
            match (in_sky, sky) {
                (true, Some(s)) => { e.insert((sky::sky_transform(&s), sky::sky_layer())); }
                _ => { e.insert((Transform::IDENTITY, visibility::MapClusters::mesh(&level.bsp, mesh))); }
            }
            overlay_draws += 1;
        }
    }
    info!("overlays: {overlay_draws} draws, {} triangles, {} main-world records skipped (non-affine/invalid/unsupported receivers)", main_overlays.triangles + sky_overlays.triangles + breakable_overlay_builds.iter().map(|(_, _, build)| build.triangles).sum::<usize>(), main_overlays.skipped);
    let t_world = t.elapsed().as_secs_f32();

    info!(
        "world: {} tris ({} in 3D skybox) in {world_draws} batches ({t_world:.2}s)",
        main.triangles + skybox.triangles + breakable_builds.iter().map(|(_, _, build)| build.triangles).sum::<usize>() + window_builds.iter().map(|build| build.triangles()).sum::<usize>(),
        skybox.triangles,
    );
    let board_material=world_mesh::material_from_vmt(None,&level.vfs,&mut textures,&mut images,&world_shared,opts.exposure,false);
    scene_import::WorldPreparation { board_material,textures, shared: world_shared, moving_door_builds }
}

#[allow(clippy::too_many_arguments)]
fn finish_level(
    commands: &mut Commands,
    loaded: session::LoadedMap,
    opts: &ClientOptions,
    character_models: characters::CharacterModels,
    view_models: viewmodel::ViewModels,
    ground_models: items::GroundModels,
    cosmetics: cosmetics::Catalog,
) {
    let level = loaded.level;
    let mut admission = loaded.remote;
    let remote = admission.is_some();
    commands.insert_resource(cosmetics);
    let localization = localize::Localization::load(&level.vfs);
    let sounds = audio::SoundLibrary::load(&level.vfs);
    commands.insert_resource(surfaces::SurfaceProps::load(&level.vfs));
    info!(
        "characters: T {}, CT {}, {} weapon world models; {} sound entries; {} weapons",
        character_models.terrorist.is_some(),
        character_models.counter_terrorist.is_some(),
        character_models.weapons.len(),
        sounds.len(),
        level.catalog.weapons.len()
    );
    commands.insert_resource(character_models);
    commands.insert_resource(view_models);
    commands.insert_resource(sounds);
    commands.insert_resource(ground_models);
    commands.insert_resource(localization);

    // Simulation: the local player on CT, bots on both teams.
    let mut world = if let Some(admission) = &admission {
        admission.world.as_ref().expect("prepared remote world").clone()
    } else {
        let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
        world.set_character_profiles(level.character_profiles.clone());
        world.load_map(&level.bsp, level.overview().as_deref());
        world
    };
    let (movement_modules, movement_visuals) = loaded.movement.unwrap_or_default();
    world.mechanics = movement_modules;
    commands.insert_resource(movement_visuals);
    let (mine, theirs) = if opts.terrorist {
        (sim::Team::Terrorist, sim::Team::CounterTerrorist)
    } else {
        (sim::Team::CounterTerrorist, sim::Team::Terrorist)
    };
    let id = if remote { 0 } else { world.add_player("You", mine) };
    let names = |team: sim::Team| if team == sim::Team::Terrorist { &BOT_NAMES_T } else { &BOT_NAMES_CT };
    let bot_count = if remote || opts.bot_difficulty == Some(0) { 0 } else { opts.bots };
    for i in 0..bot_count {
        world.add_bot(names(theirs)[i as usize % 8], theirs);
    }
    for i in 0..bot_count.saturating_sub(1) {
        world.add_bot(names(mine)[i as usize % 8], mine);
    }
    if !remote && !opts.deathmatch {
        world.set_mode(sim::GameMode::Competitive, competitive_rules(&level));
    }
    if !remote && opts.grenade_practice {
        let rules = sim::RulesConfig { grenade_practice: true, ..Default::default() };
        world.set_mode(sim::GameMode::Deathmatch, rules);
        world.respawn(id);
    }
    // Deterministic visual checks only; interactive sessions ignore these controls.
    if !remote && opts.screenshot.is_some() {
        if std::env::var_os("CSR_TEST_DEATH_BODY").is_some() {
            // Screenshot fixture only. Initialize a fragile target in front
            // of the spawn; CSR_AUTOFIRE drives the ordinary TickInput/fire/
            // damage/death path. Presentation receives a real death receipt.
            let shooter = world.player(id).unwrap();
            let angles = shooter.view_angles;
            let (forward, _, _) = movement::angle_vectors(angles);
            let origin = shooter.movement.origin + forward * 96.0;
            let victim = world.add_player("Death preview", theirs);
            let target = world.player_mut(victim).unwrap();
            target.movement.origin = origin;
            target.view_angles = glam::Vec3::new(0.0, angles.y + 180.0, 0.0);
            target.health = 1;
            target.armor = 0;
            target.helmet = false;
        }
        if let Ok(name) = std::env::var("CSR_TEST_WEAPON") {
            if let Some(def) = world.catalog.get(&name).cloned() {
                let p = world.player_mut(id).unwrap();
                let mut weapon = sim::weapon::WeaponState::new(std::sync::Arc::new(def));
                if let Ok(level) = std::env::var("CSR_TEST_ZOOM") { weapon.set_zoom(level.parse().unwrap_or(0), 0.0); }
                p.weapons.retain(|w| sim::weapon::slot(&w.def) != sim::weapon::slot(&weapon.def));
                p.weapons.insert(0, weapon);
                p.active = 0;
            }
        }
    }
    // Deathmatch practice weapons from the saved loadout; finishes and the
    // knife follow once the cosmetics profile is loaded (skin_finish).
    let inventory = inventory::Inventory::load();
    if !remote && opts.screenshot.is_none() {
        world.set_loadout(id, sim::Loadout { practice: inventory.practice(), ..Default::default() });
        if world.mode == sim::GameMode::Deathmatch { world.respawn(id); }
    }
    commands.insert_resource(inventory::Collection(level.catalog.clone()));
    let probe = sound_probe::SoundProbe::from_env(!remote && opts.screenshot.is_some());
    if !remote { probe.prepare(&mut world, id); }
    commands.insert_resource(probe);
    let mut local = if remote {
        LocalPlayer::new(0, glam::Vec3::ZERO, 0.0)
    } else {
        let p = world.player(id).expect("player just added");
        LocalPlayer::new(id, p.movement.eye_position(), p.view_angles.y)
    };
    if let Some((pos, pitch, yaw)) = opts.camera.filter(|_| !remote) {
        local.noclip = true;
        local.noclip_pos = pos;
        local.prev_eye = pos;
        local.curr_eye = pos;
        local.pitch = pitch;
        local.yaw = yaw;
    }

    if !remote && opts.screenshot.is_some() {
        if let Ok(look) = std::env::var("CSR_TEST_LOOK") {
            let values: Vec<f32> = look.split(',').filter_map(|v|v.parse().ok()).collect();
            if values.len()==2 {local.pitch=values[0];local.yaw=values[1];}
        }
    }

    // Sky camera first (3D skybox on its own layer, cleared to the 2D sky
    // colour), then the main camera on top with a fresh depth buffer.
    let sky_clear = sky::sky_color(&level.vfs, &level.bsp).unwrap_or(Color::srgb(0.55, 0.68, 0.85));
    let start = Transform::from_translation(coords::to_bevy(local.curr_eye));
    commands.spawn((
        Camera3d::default(),
        material::camera_clusters(),
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Custom(sky_clear),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: VERTICAL_FOV_DEG.to_radians(),
            near: 16.0,
            far: 400_000.0,
            ..default()
        }),
        Tonemapping::None,
        start,
        sky::sky_layer(),
        // Auxiliary views use direct submission without indirect draw setup.
        bevy::render::view::NoIndirectDrawing,
        PlayerCamera,
    ));
    commands.spawn((
        Camera3d::default(),
        material::camera_clusters(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: VERTICAL_FOV_DEG.to_radians(),
            near: 4.0,
            far: 40_000.0,
            ..default()
        }),
        Tonemapping::None,
        start,
        PlayerCamera,
        audio::spatial_listener(),
        player::WorldPlayerCamera,
        // Allocates separate sampled depth and grants main depth COPY_SRC.
        bevy::core_pipeline::prepass::DepthPrepass,
        particle_depth::WorldParticleDepth,
        bevy::render::view::NoIndirectDrawing,
    ));
    // Viewmodel camera: drawn last with its own depth buffer and FOV.
    commands.spawn((
        Camera3d::default(),
        material::camera_clusters(),
        Camera {
            order: 2,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: viewmodel::vertical_fov(viewmodel::VIEWMODEL_FOV),
            near: 1.0,
            far: 512.0,
            ..default()
        }),
        Tonemapping::None,
        start,
        viewmodel::layer(),
        PlayerCamera,
        player::ViewModelCamera,
        bevy::render::view::NoIndirectDrawing,
        IsDefaultUiCamera,
        menu_blur::PostProcessSettings::default(),
    ));
    commands.insert_resource(local);
    if !remote { world.bots = level.configured_bots(opts.bot_difficulty)
        .expect("offline bot settings validated by the immutable loaded level"); }
    if let Some(admission) = admission.take() {
        commands.insert_resource(admission);
    }
    commands.insert_resource(Time::<Fixed>::from_hz(world.tick_rate as f64));
    commands.insert_resource(SimWorld(world));
    commands.insert_resource(LoadedLevel(level));
}

/// Competitive rules from the install's own `gamemode_competitive.cfg`.
pub fn competitive_rules(level: &level::Level) -> sim::RulesConfig {
    let cvars = level.cfg("gamemode_competitive.cfg").map(|t| sim::rules::parse_cfg(&t)).unwrap_or_default();
    sim::RulesConfig::from_cvars(&cvars)
}

fn smoke_test(
    mut frame: Local<u32>,
    mut probe_frame: Local<Option<u32>>,
    probe: Res<sound_probe::SoundProbe>,
    mut frame_times: Local<Vec<f32>>,
    real: Res<Time<Real>>,
    options: Res<Options>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    mut world: Option<ResMut<SimWorld>>,
    local: Option<ResMut<LocalPlayer>>,
    mut menu: ResMut<buymenu::BuyMenu>,
    mut inspect: ResMut<tracers::InspectRequest>,
    mut events: ResMut<effects::PendingEvents>,
    level: Res<LoadedLevel>,
) {
    // Menu captures wait for the movie and its asynchronous fonts/widgets.
    if pack_ui::proof_enabled() || frontend_flow::enabled() || std::env::var("CSR_TEST_MENU").as_deref() == Ok("pause") { return; }
    *frame += 1;

    let opts = &options.0;
    let Some(path) = &opts.screenshot else { return };
    let capture_time=std::env::var("CSR_CAPTURE_SIM_TIME").ok().and_then(|v|v.parse::<f32>().ok())
        .filter(|v|v.is_finite()&&*v>=0.);
    if probe.active() && probe_frame.is_none() && world.as_ref().is_some_and(|w| w.0.time() >= probe.duration()) { *probe_frame = Some(*frame); }
    if !probe.active() && capture_time.is_some() && probe_frame.is_none()
        && world.as_ref().is_some_and(|w|w.0.time()>=capture_time.unwrap()) { *probe_frame=Some(*frame); }
    let capture_frame = if probe.active()||capture_time.is_some() { probe_frame.unwrap_or(u32::MAX - 30) } else { opts.frames };
    if *frame > 120 && *frame <= capture_frame { frame_times.push(real.delta_secs() * 1000.0); }
    if *frame == capture_frame.saturating_sub(90) && std::env::var_os("CSR_INSPECT").is_some(){inspect.start();}
    // This system has already required an actual screenshot request above.
    if capture_frame.checked_sub(10) == Some(*frame) && std::env::var_os("CSR_INSPECT_CANCEL").is_some(){inspect.cancel();}
    // CSR_BUY_MENU=n: show the buy menu on category n (0 = none picked).
    if let Ok(cat) = std::env::var("CSR_BUY_MENU").map(|v| v.parse::<usize>().unwrap_or(0)) {
        // The close-on-buy proof must leave subsequent state to the real
        // purchase receipt and input consumer instead of reopening each frame.
        if std::env::var("CSR_TEST_SWITCH").as_deref() != Ok("buy-close") || *frame == 1 {
            menu.open = true;
            menu.category = cat.checked_sub(1);
        }
    }
    let mut local = local;
    // Explicit screenshot-only fixture, never enabled in interactive play.
    // This checks rendering/event plumbing, not combat accuracy.
    if *frame == capture_frame.saturating_sub(8) && std::env::var_os("CSR_TEST_FEEDBACK").is_some() {
        if let Some(l)=local.as_ref() {
            let (forward,_,_)=movement::angle_vectors(l.render_angles);
            let start=l.render_eye;
            let tr=level.0.collision.trace_ray(start,start+forward*500.0,collision::contents::MASK_SOLID);
            if tr.hit() {events.0.push(sim::GameEvent::Impact{player:l.id,pos:tr.end,normal:tr.normal,surface:tr.surface,door:None});}
            if std::env::var("CSR_TEST_FEEDBACK").as_deref() != Ok("impact") {
                let pos=start+forward*70.0;
                events.0.push(sim::GameEvent::Damage{attacker:l.id,victim:l.id,health:25,group:sim::weapon::HitGroup::Chest,pos,protection:sim::weapon::HitProtection::Flesh});
            }
        }
    }
    if *frame == capture_frame.saturating_sub(12) && std::env::var_os("CSR_TEST_DROP").is_some() {
        if let Some(l)=local.as_mut(){l.pending_drop=true;}
    }
    if *frame == capture_frame.saturating_sub(10) && std::env::var_os("CSR_TEST_DROP").is_some() {
        if let Some(l)=local.as_mut(){l.pitch=45.0;}
    }
    // CSR_SLOT=n: switch to weapon slot n early in the run.
    if *frame == 10 {
        if let (Some(l), Ok(n)) = (local.as_mut(), std::env::var("CSR_SLOT").map(|v| v.parse::<u8>().unwrap_or(1))) {
            l.pending_slot = n;
        }
    }
    // CSR_TEST_RELOAD=n: empty the held gun and press reload at frame n.
    if let (Some(w), Some(l), Ok(at)) = (world.as_mut(), local.as_mut(), std::env::var("CSR_TEST_RELOAD").map(|v| v.parse::<u32>().unwrap_or(20))) {
        if *frame == at {
            if let Some(gun) = w.0.player_mut(l.id).and_then(|p| { let a = p.active; p.weapons.get_mut(a) }) {
                gun.clip = 0;
            }
            l.latched |= movement::buttons::RELOAD;
        }
    }
    // Isolation cases use a fixed field birth and spawn time, independent of
    // render-frame pacing. A sim-time capture cannot use capture_frame - 90:
    // capture_frame is unknown until the requested sim time has already arrived.
    let isolated_fire = std::env::var("CSR_TEST_UTILITY").ok()
        .is_some_and(|kind| utility_fx::isolated_fire_layer(&kind).is_some());
    let utility_fixture_due = if isolated_fire {
        world.as_ref().is_some_and(|w| w.0.time() >= 0.5
            && !w.0.utility_fields.iter().any(|f| f.id == 999999))
    } else {
        *frame == capture_frame.saturating_sub(90)
    };
    if utility_fixture_due {
        if let (Some(w),Some(l),Ok(kind))=(world.as_mut(),local.as_ref(),std::env::var("CSR_TEST_UTILITY")) {
            let (forward,_,_)=movement::angle_vectors(glam::Vec3::new(0.0,l.yaw,0.0));
            let now=if isolated_fire {0.5} else {w.0.time()};
            // A decoy shell is much smaller than the smoke/fire field fixtures.
            // Keep its snapshot-only fixture on clear nearby floor, at the
            // projectile hull's two-unit rest height, for readable inspection.
            let distance=if kind=="decoy" {80.0} else {330.0};
            let pos=w.0.collision.trace_ray(l.curr_eye+forward*distance,l.curr_eye+forward*distance-glam::Vec3::Z*200.0,collision::contents::MASK_SOLID).end
                + if kind=="decoy" {glam::Vec3::Z*2.0} else {glam::Vec3::ZERO};
            if kind=="he" {w.0.projectiles.push(sim::utility::Projectile{id:999998,owner:l.id,kind:sim::utility::Kind::He,pos:pos+glam::Vec3::Z*3.0,velocity:glam::Vec3::ZERO,settled:false,born:now-2.0,decoy_weapon:None});}
            else if kind=="flash" {if let Some(p)=w.0.player_mut(l.view_target()){p.flash_until=now+5.0;}}
            else {w.0.utility_fields.push(sim::utility::Field{id:999999,owner:l.id,kind:match kind.as_str(){"fire"|"fire-center"|"fire-fancy"|"fire-child"=>sim::utility::Kind::Molotov,"decoy"=>sim::utility::Kind::Decoy,_=>sim::utility::Kind::Smoke},pos,born:now-1.0,until:now+20.0,decoy_weapon:None});}
        }
    }
    if *frame == capture_frame {
        if let (Some(w),Some(l))=(world.as_ref(),local.as_ref()) {
            let weapon = w.0.player(l.id).and_then(|p| p.active_weapon());
            info!("capture authority: time={:.5} weapon={:?} grenade={:?} suppressor_attached={:?} suppressor_transition={:?} projectiles={} fields={} burst_mode={:?} burst_pending={:?} clip={:?} shots={:?}",
                w.0.time(), weapon.map(|g| g.def.class_name.as_str()), weapon.and_then(|g| g.grenade_throw),
                weapon.map(|g| g.suppressor_attached), weapon.and_then(|g| g.suppressor_transition),
                w.0.projectiles.len(),w.0.utility_fields.len(),
                weapon.map(|g| g.burst_mode), weapon.and_then(|g| g.burst), weapon.map(|g| g.clip),
                w.0.player(l.id).map(|p| p.shots));
            if std::env::var_os("CSR_WEAPON_TRACE").is_some() {
                if let Some(actor)=w.0.player(l.id) {
                    info!("capture local: id={} pos={:?} velocity={:?} on_ground={}",
                        actor.id, actor.movement.origin, actor.movement.velocity, actor.movement.on_ground);
                }
                for bot in w.0.players.iter().filter(|p| p.is_bot) {
                    info!("capture bot: id={} alive={} pos={:?} velocity={:?} on_ground={} {}",
                        bot.id, bot.alive, bot.movement.origin, bot.movement.velocity,
                        bot.movement.on_ground, w.0.bots.describe(&w.0, bot.id));
                }
                for projectile in &w.0.projectiles {
                    info!("capture projectile: id={} kind={:?} pos={:?} velocity={:?} settled={}",
                        projectile.id, projectile.kind, projectile.pos, projectile.velocity, projectile.settled);
                }
                for field in &w.0.utility_fields {
                    info!("capture utility field: id={} kind={:?} pos={:?} born={} until={}",
                        field.id, field.kind, field.pos, field.born, field.until);
                }
            }
            if std::env::var("CSR_TEST_SWITCH").as_deref() == Ok("buy-close") {
                let player = w.0.player(l.id);
                info!("buy-close capture: menu_open={} just_closed={} money={:?} armor={:?}",
                    menu.open, menu.just_closed, player.map(|p| p.money), player.map(|p| p.armor));
            }
        }
    }
    // CSR_WATCH_BOT=n: park the camera 120 units in front of bot n, facing it.
    if let (Some(world), Some(mut local), Ok(n)) = (world, local, std::env::var("CSR_WATCH_BOT").map(|v| v.parse::<usize>().unwrap_or(0))) {
        if let Some(bot) = world.0.players.iter().filter(|p| p.is_bot && p.alive).nth(n) {
            let dist = std::env::var("CSR_WATCH_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(120.0);
            let (fwd, _, _) = movement::angle_vectors(glam::Vec3::new(0.0, bot.view_angles.y, 0.0));
            let eye = bot.movement.origin + fwd * dist + glam::Vec3::new(0.0, 0.0, 60.0);
            let (pitch, yaw) = sim::bots::look_angles(eye, bot.movement.origin + glam::Vec3::new(0.0, 0.0, 40.0));
            local.noclip = true;
            local.noclip_pos = eye;
            local.prev_eye = eye;
            local.curr_eye = eye;
            local.pitch = pitch;
            local.yaw = yaw;
        }
    }
    if *frame == capture_frame {
        let fps = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|d| d.average())
            .unwrap_or(0.0);
        let ms = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(|d| d.average())
            .unwrap_or(0.0);
        info!("smoke test: {fps:.1} fps average ({ms:.2} ms/frame) at frame {}", *frame);
        if !frame_times.is_empty() {
            frame_times.sort_by(f32::total_cmp);
            let mean = frame_times.iter().sum::<f32>() / frame_times.len() as f32;
            let percentile = |p: f32| frame_times[((frame_times.len() - 1) as f32 * p) as usize];
            info!("frame pacing after warmup: {} frames, {:.1} fps, mean {:.2} ms, p95 {:.2} ms, p99 {:.2} ms, max {:.2} ms", frame_times.len(), 1000.0 / mean, mean, percentile(0.95), percentile(0.99), percentile(1.0));
        }
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
    }
    if *frame == capture_frame + 30 {
        exit.write(AppExit::Success);
    }
}

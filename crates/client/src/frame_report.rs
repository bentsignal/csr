//! Opt-in, bounded cadence samples and once-per-second main-world census.
//! Counts describe retained CPU assets/entities, not GPU memory or draw calls.
use bevy::{
    camera::visibility::NoFrustumCulling,
    mesh::skinning::SkinnedMesh,
    prelude::*,
    window::{PresentMode, PrimaryWindow},
};
use std::{num::NonZeroU32, time::Instant};

const MAX_SAMPLES: usize = 8192;

#[derive(Resource, Default)]
pub(super) struct ReportInterval {
    pub due: bool,
}

/// Copy-only observations: no per-frame strings, mesh scans or extra locks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrameContext {
    state: crate::session::AppState,
    page: Option<crate::menu::Page>,
    in_match: bool,
    window: Option<WindowContext>,
    screenshot: bool,
    input_enabled: bool,
    local: Option<LocalContext>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowContext {
    size: (u32, u32),
    focused: bool,
    present_requested: PresentMode,
    frame_latency_hint: Option<NonZeroU32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocalContext {
    id: sim::PlayerId,
    spectating: Option<sim::PlayerId>,
    noclip: bool,
    authoritative_local: Option<ActorContext>,
    authoritative_view_target: Option<ActorContext>,
}

/// Discrete authoritative changes belong in mixed-context detection. Continuous
/// movement/aim and interpolated camera pose are deliberately endpoint-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActorContext {
    id: sim::PlayerId,
    life_epoch: u64,
    alive: bool,
    team: sim::Team,
    active_slot: usize,
    weapon_def_index: Option<u32>,
}

impl ActorContext {
    fn observe(player: &sim::Player) -> Self {
        Self {
            id: player.id,
            life_epoch: player.life_epoch,
            alive: player.alive,
            team: player.team,
            active_slot: player.active,
            weapon_def_index: player.active_weapon().map(|weapon| weapon.def.def_index),
        }
    }
}

fn report_player(role: &str, id: sim::PlayerId, world: Option<&sim::World>) {
    let player = world.and_then(|world| world.player(id));
    info!(
        "perf player endpoint: endpoint_only=true role={role} requested_id={id} authoritative={:?} weapon_class={:?} weapon_content_id={:?} origin_source_inches={:?} view_angles_source_degrees={:?}",
        player.map(ActorContext::observe),
        player
            .and_then(|p| p.active_weapon())
            .map(|w| w.def.class_name.as_str()),
        player.and_then(|p| p.active_weapon()).map(|w| &w.def.id),
        player.map(|p| p.movement.origin.to_array()),
        player.map(|p| p.view_angles.to_array()),
    );
}

/// Bevy perspective FOV is vertical radians; horizontal FOV depends on the
/// actual aspect ratio, rather than Source's nominal 4:3 horizontal setting.
fn perspective_fov_degrees(projection: &Projection) -> Option<[f32; 2]> {
    let Projection::Perspective(p) = projection else {
        return None;
    };
    Some([
        p.fov.to_degrees(),
        (2. * ((p.fov * 0.5).tan() * p.aspect_ratio).atan()).to_degrees(),
    ])
}

#[derive(Default)]
pub(super) struct FrameWindow {
    samples: Vec<f64>,
    count: u64,
    rejected: u64,
    total: f64,
    maximum: f64,
    previous_context: Option<FrameContext>,
    context_changes: u64,
}

impl FrameWindow {
    fn observe_context(&mut self, context: FrameContext, map_changed: bool) {
        if self
            .previous_context
            .is_some_and(|previous| previous != context || map_changed)
        {
            self.context_changes = self.context_changes.saturating_add(1);
        }
        self.previous_context = Some(context);
    }

    fn record(&mut self, ms: f64) {
        if !ms.is_finite() || ms <= 0. {
            self.rejected += 1;
            return;
        }
        self.count += 1;
        self.total += ms;
        self.maximum = self.maximum.max(ms);
        if self.samples.len() < MAX_SAMPLES {
            self.samples.push(ms);
        }
    }

    /// Nearest-rank p50/p95/p99. Never label a truncated prefix as the full
    /// interval: on overflow only the all-frame mean/maximum remain valid.
    fn quantiles(&mut self) -> Option<[f64; 3]> {
        if self.count == 0 || self.count != self.samples.len() as u64 {
            return None;
        }
        self.samples.sort_unstable_by(f64::total_cmp);
        Some(
            [50, 95, 99]
                .map(|percent| self.samples[(self.samples.len() * percent).div_ceil(100) - 1]),
        )
    }

    fn clear(&mut self) {
        self.samples.clear();
        self.count = 0;
        self.rejected = 0;
        self.total = 0.;
        self.maximum = 0.;
        self.context_changes = 0;
        // Retain the endpoint: the next Time<Real> delta crosses this boundary.
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn report(
    real: Res<Time<Real>>,
    mut interval: ResMut<ReportInterval>,
    mut samples: Local<FrameWindow>,
    mut since: Local<Option<Instant>>,
    state: Res<State<crate::session::AppState>>,
    options: Res<crate::Options>,
    menu: Res<crate::menu::Menu>,
    local: Option<Res<crate::player::LocalPlayer>>,
    sim: Option<Res<crate::player::SimWorld>>,
    input: Res<crate::player::InputEnabled>,
    entities: Query<Entity>,
    level_name: Res<crate::LevelInfo>,
    windows: Query<&Window, With<PrimaryWindow>>,
    assets: (
        Res<Assets<Mesh>>,
        Res<Assets<Image>>,
        Res<Assets<crate::material::WorldMaterial>>,
        Res<Assets<StandardMaterial>>,
        Res<Assets<crate::particle_material::ParticleMaterial>>,
    ),
    scene: (
        Query<
            (
                Option<&Visibility>,
                Option<&ViewVisibility>,
                Has<NoFrustumCulling>,
            ),
            With<Mesh3d>,
        >,
        Query<Entity, With<SkinnedMesh>>,
        Query<Entity, With<Node>>,
        Query<Entity, With<bevy::audio::AudioPlayer>>,
        Query<Entity, With<crate::feedback::ImpactFx>>,
        Query<&Camera>,
        Query<
            (
                Entity,
                &Camera,
                &Transform,
                &GlobalTransform,
                &Projection,
                &bevy::camera::RenderTarget,
                Option<&bevy::camera::visibility::RenderLayers>,
                Has<crate::player::PlayerCamera>,
                Has<crate::player::ViewModelCamera>,
            ),
            With<crate::player::WorldPlayerCamera>,
        >,
    ),
    characters: Res<crate::characters::Characters>,
) {
    interval.due = false;
    samples.observe_context(
        FrameContext {
            state: *state.get(),
            page: menu.page,
            in_match: menu.in_match,
            window: windows.single().ok().map(|window| WindowContext {
                size: (window.physical_width(), window.physical_height()),
                focused: window.focused,
                present_requested: window.present_mode,
                frame_latency_hint: window.desired_maximum_frame_latency,
            }),
            screenshot: options.0.screenshot.is_some(),
            input_enabled: input.0,
            local: local.as_ref().map(|player| LocalContext {
                id: player.id,
                spectating: player.spectating,
                noclip: player.noclip,
                authoritative_local: sim
                    .as_ref()
                    .and_then(|s| s.0.player(player.id))
                    .map(ActorContext::observe),
                authoritative_view_target: sim
                    .as_ref()
                    .and_then(|s| s.0.player(player.view_target()))
                    .map(ActorContext::observe),
            }),
        },
        // Resource change detection also catches map replacement with the same
        // name, without cloning/hashing the map string on every frame.
        level_name.is_changed(),
    );
    let now = Instant::now();
    let Some(previous) = *since else {
        *since = Some(now);
        return;
    };
    samples.record(real.delta_secs_f64() * 1000.);
    let seconds = now.duration_since(previous).as_secs_f64();
    if seconds < 1. {
        return;
    }
    *since = Some(now);
    interval.due = true;
    let _scope = super::frame_scope("diagnostic.report");
    let quantiles = samples.quantiles();
    let context_changes = samples.context_changes;
    let mixed_context = context_changes != 0;
    info!(
        "perf interval: elapsed_real_s={:.3} report_wall_s={seconds:.3} sampled_real_s={:.6} frames={} mean_ms={:.4} p50_p95_p99_ms={quantiles:?} max_ms={:.4} overflow={} rejected={} mixed_context={mixed_context} context_changes={context_changes}",
        real.elapsed_secs_f64(),
        samples.total / 1000.,
        samples.count,
        samples.total / samples.count.max(1) as f64,
        samples.maximum,
        samples.count.saturating_sub(samples.samples.len() as u64),
        samples.rejected,
    );
    samples.clear();
    info!(
        "perf context: endpoint_only=true map={} mixed_context={mixed_context} observed={:?}",
        level_name.name, samples.previous_context,
    );
    if let Some(local) = &local {
        let world = sim.as_ref().map(|s| &s.0);
        report_player("local", local.id, world);
        report_player("view_target", local.view_target(), world);
        info!(
            "perf view intent endpoint: endpoint_only=true render_eye_source_inches={:?} render_angles_source_degrees={:?}",
            local.render_eye.to_array(),
            local.render_angles.to_array(),
        );
    } else {
        info!("perf player endpoint: endpoint_only=true local_resource=false");
    }
    // Read the actual main-world camera components in Last, after camera update
    // and transform propagation. Never infer the world view from camera order,
    // FOV or PlayerCamera alone (sky and viewmodel also have PlayerCamera).
    let candidates = scene.6.iter().count();
    if let Ok((
        entity,
        camera,
        transform,
        global,
        projection,
        target,
        layers,
        player_camera,
        viewmodel_camera,
    )) = scene.6.single()
    {
        let role_valid = player_camera && !viewmodel_camera;
        info!(
            "perf world camera endpoint: endpoint_only=true source=main_world_last interval_camera_stability=unproven candidates={candidates} entity={entity:?} role_valid={role_valid} player_camera={player_camera} viewmodel_camera={viewmodel_camera} is_active={} order={} transform_bevy={transform:?} global_matrix_bevy_cols={:?} projection={projection:?} fov_vertical_horizontal_degrees={:?} computed_clip_from_view_cols={:?} target={target:?} render_layers={layers:?} viewport={:?} physical_viewport_rect={:?} sub_camera_view={:?}",
            camera.is_active,
            camera.order,
            global.to_matrix().to_cols_array(),
            perspective_fov_degrees(projection),
            camera.clip_from_view().to_cols_array(),
            camera.viewport,
            camera.physical_viewport_rect(),
            camera.sub_camera_view,
        );
    } else {
        info!(
            "perf world camera endpoint: endpoint_only=true source=main_world_last interval_camera_stability=unproven candidates={candidates} selection=unavailable"
        );
    }
    if let Some(world) = sim {
        let world = &world.0;
        info!(
            "perf simulation census: tick={} tick_rate={} sim_seconds={:.3} competitive={} round={} players={} bots={} alive={} items={} projectiles={} fields={}",
            world.tick,
            world.tick_rate,
            world.time(),
            world.competitive(),
            world.round.round,
            world.players.len(),
            world.players.iter().filter(|p| p.is_bot).count(),
            world.players.iter().filter(|p| p.alive).count(),
            world.items.len(),
            world.projectiles.len(),
            world.utility_fields.len(),
        );
    }
    let mut mesh_count = 0usize;
    let mut hidden = 0usize;
    let mut view_visible = 0usize;
    let mut no_frustum = 0usize;
    for (visibility, view, uncullable) in &scene.0 {
        mesh_count += 1;
        hidden += usize::from(visibility == Some(&Visibility::Hidden));
        view_visible += usize::from(view.is_some_and(|v| v.get()));
        no_frustum += usize::from(uncullable);
    }
    info!(
        "perf scene census: entities={} mesh_entities={mesh_count} hidden_local={hidden} view_visible_any={view_visible} no_frustum={no_frustum} skinned_meshes={} ui_nodes={} audio_players={} impact_fx={} cameras={} active_cameras={}",
        entities.count(),
        scene.1.count(),
        scene.2.count(),
        scene.3.count(),
        scene.4.count(),
        scene.5.count(),
        scene.5.iter().filter(|c| c.is_active).count(),
    );
    info!(
        "perf asset census: meshes={} images={} world_materials={} standard_materials={} particle_materials={}",
        assets.0.len(),
        assets.1.len(),
        assets.2.len(),
        assets.3.len(),
        assets.4.len(),
    );
    let (instances, [bodies, pending, physical]) = characters.performance_counts();
    info!(
        "perf character census: instances={instances} bodies={bodies} pending={pending} physical={physical}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn match_context() -> FrameContext {
        FrameContext {
            state: crate::session::AppState::Match,
            page: None,
            in_match: true,
            window: Some(WindowContext {
                size: (1920, 1080),
                focused: true,
                present_requested: PresentMode::Immediate,
                frame_latency_hint: NonZeroU32::new(1),
            }),
            screenshot: false,
            input_enabled: true,
            local: Some(LocalContext {
                id: 1,
                spectating: Some(2),
                noclip: false,
                authoritative_local: None,
                authoritative_view_target: None,
            }),
        }
    }

    #[test]
    fn actor_lifecycle_and_weapon_changes_mix_context_but_motion_does_not() {
        let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
        let id = world.add_player("fixture", sim::Team::CounterTerrorist);
        let catalog = weapons::Catalog::from_items_game(
            r#"
            "items_game" { "items" {
                "7" { "name" "weapon_ak47" "item_class" "weapon_ak47" "baseitem" "1" }
                "16" { "name" "weapon_m4a1" "item_class" "weapon_m4a1" "baseitem" "1" }
            } }
        "#,
        )
        .unwrap();
        let player = world.player_mut(id).unwrap();
        player.weapons = ["weapon_ak47", "weapon_m4a1"]
            .map(|name| {
                sim::weapon::WeaponState::new(std::sync::Arc::new(
                    catalog.get(name).unwrap().clone(),
                ))
            })
            .to_vec();
        player.active = 0;
        let original = ActorContext::observe(player);
        assert_eq!(original.weapon_def_index, Some(7));
        player.movement.origin = glam::Vec3::new(128., -64., 32.);
        player.view_angles = glam::Vec3::new(-15., 90., 0.);
        assert_eq!(
            ActorContext::observe(player),
            original,
            "motion is endpoint-only"
        );
        let mut context = match_context();
        context.local.as_mut().unwrap().authoritative_local = Some(original);
        context.local.as_mut().unwrap().authoritative_view_target = Some(original);
        let mut changed = [original; 5];
        player.alive = !original.alive;
        changed[0] = ActorContext::observe(player);
        player.alive = original.alive;
        player.life_epoch += 1;
        changed[1] = ActorContext::observe(player);
        player.life_epoch = original.life_epoch;
        player.team = sim::Team::Terrorist;
        changed[2] = ActorContext::observe(player);
        player.team = original.team;
        player.active = 1;
        changed[3] = ActorContext::observe(player);
        assert_eq!(changed[3].weapon_def_index, Some(16));
        // Same-slot replacement must still be observed.
        player.weapons.swap(0, 1);
        player.active = 0;
        changed[4] = ActorContext::observe(player);
        for actor in changed.into_iter().map(Some).chain(std::iter::once(None)) {
            for view_target in [false, true] {
                let mut endpoint = context;
                let local = endpoint.local.as_mut().unwrap();
                if view_target {
                    local.authoritative_view_target = actor;
                } else {
                    local.authoritative_local = actor;
                }
                let mut window = FrameWindow::default();
                window.observe_context(context, false);
                window.observe_context(endpoint, false);
                assert_eq!(window.context_changes, 1);
            }
        }
    }

    #[test]
    fn perspective_endpoint_fov_uses_vertical_radians_and_actual_aspect() {
        let projection = Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio: 16. / 9.,
            ..default()
        });
        let [vertical, horizontal] = perspective_fov_degrees(&projection).unwrap();
        assert!((vertical - 90.).abs() < 0.0001);
        assert!((horizontal - 121.28449).abs() < 0.0001);
    }

    #[test]
    fn loading_pause_focus_and_spectator_changes_cannot_look_like_stable_match() {
        let endpoint = match_context();
        let mut loading = endpoint;
        loading.state = crate::session::AppState::Loading;
        let mut paused = endpoint;
        paused.page = Some(crate::menu::Page::Pause);
        let mut unfocused = endpoint;
        unfocused.window.as_mut().unwrap().focused = false;
        let mut other_target = endpoint;
        other_target.local.as_mut().unwrap().spectating = Some(3);
        for earlier in [loading, paused, unfocused, other_target] {
            let mut window = FrameWindow::default();
            window.observe_context(earlier, false);
            window.record(900.);
            window.observe_context(endpoint, false);
            window.record(10.);
            assert_eq!(window.previous_context, Some(endpoint));
            assert_eq!(
                window.context_changes, 1,
                "the endpoint cannot relabel earlier frames"
            );
            assert_eq!(window.count, 2);
            assert_eq!(window.total, 910.);
            assert_eq!(window.quantiles(), Some([10., 900., 900.]));
        }
    }

    #[test]
    fn returning_to_same_context_and_cross_report_boundaries_remain_observable() {
        let context = match_context();
        let mut window = FrameWindow::default();
        window.observe_context(context, true); // initial resource observation
        assert_eq!(window.context_changes, 0);
        let mut paused = context;
        paused.page = Some(crate::menu::Page::Audio);
        window.observe_context(paused, false);
        window.observe_context(context, false);
        assert_eq!(
            window.context_changes, 2,
            "returning to Match does not erase mixed history"
        );
        window.clear();
        window.observe_context(context, false);
        assert_eq!(window.context_changes, 0);
        window.clear();
        window.observe_context(paused, false);
        assert_eq!(
            window.context_changes, 1,
            "first delta after a report crosses its endpoint"
        );
        window.clear();
        window.observe_context(paused, true); // same-name map resource replacement
        assert_eq!(window.context_changes, 1);
        window.clear();
        let mut missing_window = paused;
        missing_window.window = None;
        window.observe_context(missing_window, false);
        assert_eq!(window.context_changes, 1);
    }

    #[test]
    fn interval_quantiles_use_all_valid_samples_and_reset() {
        let mut window = FrameWindow::default();
        for ms in [f64::NAN, f64::INFINITY, -1., 0.] {
            window.record(ms);
        }
        assert_eq!(window.rejected, 4);
        assert_eq!(window.quantiles(), None);
        for ms in (1..=100).rev() {
            window.record(ms as f64);
        }
        assert_eq!(window.quantiles(), Some([50., 95., 99.]));
        assert_eq!(window.total / window.count as f64, 50.5);
        assert_eq!(window.maximum, 100.);
        window.clear();
        window.record(7.);
        assert_eq!(window.quantiles(), Some([7.; 3]));
        assert_eq!(window.count, 1);
        assert_eq!(window.rejected, 0);
        assert_eq!(window.total, 7.);
        assert_eq!(window.maximum, 7.);
    }

    #[test]
    fn overflow_never_publishes_prefix_quantiles_but_retains_tail_maximum() {
        let mut window = FrameWindow::default();
        for _ in 0..MAX_SAMPLES {
            window.record(1.);
        }
        window.record(1000.);
        assert_eq!(window.samples.len(), MAX_SAMPLES);
        assert_eq!(window.count, MAX_SAMPLES as u64 + 1);
        assert_eq!(window.total, MAX_SAMPLES as f64 + 1000.);
        assert_eq!(window.maximum, 1000.);
        assert_eq!(window.quantiles(), None);
        window.clear();
        window.record(2.);
        assert_eq!(window.quantiles(), Some([2.; 3]));
    }
}

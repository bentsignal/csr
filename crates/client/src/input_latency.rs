//! Desktop rendering policy: present the current input frame without CPU pipelining.
use bevy::window::PresentMode;
use bevy::{
    app::PluginGroupBuilder, prelude::*, render::pipelined_rendering::PipelinedRenderingPlugin,
};
use std::num::NonZeroU32;
use std::{
    collections::BTreeMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

static TIMING_ENABLED: AtomicBool = AtomicBool::new(false);
static STAGES: OnceLock<Mutex<BTreeMap<&'static str, StageTiming>>> = OnceLock::new();

#[derive(Default)]
struct StageTiming {
    calls: u64,
    total_ms: f64,
    max_ms: f64,
}

/// Diagnostic wall time, including any waits inside this scope. Disabled play
/// pays one relaxed flag load and does not acquire a clock or a lock.
pub(crate) struct FrameScope {
    name: &'static str,
    start: Option<Instant>,
}

pub(crate) fn frame_scope(name: &'static str) -> FrameScope {
    FrameScope {
        name,
        start: TIMING_ENABLED.load(Ordering::Relaxed).then(Instant::now),
    }
}

impl Drop for FrameScope {
    fn drop(&mut self) {
        let Some(start) = self.start else { return };
        record_stage(self.name, start.elapsed().as_secs_f64() * 1000.);
    }
}

fn record_stage(name: &'static str, ms: f64) {
    let mut stages = STAGES.get_or_init(Default::default).lock().unwrap();
    let stage = stages.entry(name).or_default();
    stage.calls += 1;
    stage.total_ms += ms;
    stage.max_ms = stage.max_ms.max(ms);
}

#[derive(Resource, Default)]
struct ScheduleTime {
    start: Option<Instant>,
    section: Option<Instant>,
}

#[derive(Resource, Default)]
struct SurfaceTime(Option<Instant>);

fn before_surface(mut clock: ResMut<SurfaceTime>) {
    clock.0 = Some(Instant::now());
}
fn after_surface(mut clock: ResMut<SurfaceTime>) {
    if let Some(start) = clock.0.take() {
        record_stage("render.surface", start.elapsed().as_secs_f64() * 1000.);
    }
}

fn start_schedule(mut clock: ResMut<ScheduleTime>) {
    let now = Instant::now();
    clock.start = Some(now);
    clock.section = Some(now);
}
fn finish_main(mut clock: ResMut<ScheduleTime>) {
    if let Some(start) = clock.start.take() {
        record_stage("main", start.elapsed().as_secs_f64() * 1000.);
    }
}
fn finish_render(mut clock: ResMut<ScheduleTime>) {
    if let Some(start) = clock.start.take() {
        record_stage("render", start.elapsed().as_secs_f64() * 1000.);
    }
}
fn before_submit(mut clock: ResMut<ScheduleTime>) {
    let now = Instant::now();
    if let Some(start) = clock.section.replace(now) {
        record_stage(
            "render.prepare",
            now.duration_since(start).as_secs_f64() * 1000.,
        );
    }
}
fn after_submit(mut clock: ResMut<ScheduleTime>) {
    if let Some(start) = clock.section.take() {
        record_stage("render.submit", start.elapsed().as_secs_f64() * 1000.);
    }
}

/// A valid diagnostic override wins over saved preferences at startup and
/// during live preference application. It never changes the saved choice.
pub(crate) fn present_mode(vsync: bool) -> PresentMode {
    resolve_present_mode(vsync, std::env::var("CSR_PRESENT").ok().as_deref())
}

fn resolve_present_mode(vsync: bool, requested: Option<&str>) -> PresentMode {
    match requested {
        Some("immediate") => PresentMode::Immediate,
        Some("mailbox") => PresentMode::Mailbox,
        Some("fifo") => PresentMode::Fifo,
        _ if vsync => PresentMode::AutoVsync,
        _ => PresentMode::AutoNoVsync,
    }
}

/// Log app-frame intervals in ordinary interactive play as well as captures.
/// This is not a measurement of mouse-to-display or compositor latency.
pub(crate) fn install_frame_log(app: &mut App) {
    // Bounded renderer experiment: WorldMaterial supplies its own map/prop
    // lighting and does not read Bevy's clustered point/spot light lists.
    // Compare actual frames before considering any production camera policy.
    if std::env::var_os("CSR_TEST_NO_LIGHT_CLUSTERS").is_some() {
        app.add_systems(PostUpdate, disable_test_light_clusters
            .before(bevy::light::SimulationLightSystems::AssignLightsToClusters));
    }
    let limit = std::env::var("CSR_FRAME_LOG")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v >= 0.);
    if let Some(limit) = limit {
        TIMING_ENABLED.store(true, Ordering::Relaxed);
        // Use Bevy's existing pass instrumentation to separate GPU execution
        // from the wall-time/command-encoding scopes below. This allocates GPU
        // query buffers only for explicitly requested diagnostic runs.
        app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
        app.init_resource::<ScheduleTime>()
            .add_systems(First, start_schedule)
            .add_systems(Last, finish_main);
        // RenderSystems order verified against Bevy 0.19's base schedule.
        // Extraction precedes Render and is deliberately outside this timer.
        if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
            use bevy::render::{Render, RenderSystems};
            render
                .init_resource::<ScheduleTime>()
                .init_resource::<SurfaceTime>()
                .add_systems(
                    Render,
                    before_surface
                        .in_set(RenderSystems::PrepareViews)
                        .before(bevy::render::view::window::prepare_windows),
                )
                .add_systems(
                    Render,
                    after_surface
                        .in_set(RenderSystems::PrepareViews)
                        .after(bevy::render::view::window::prepare_windows),
                )
                .add_systems(
                    Render,
                    start_schedule.before(RenderSystems::ExtractCommands),
                )
                .add_systems(
                    Render,
                    before_submit
                        .after(RenderSystems::Prepare)
                        .before(RenderSystems::Render),
                )
                .add_systems(
                    Render,
                    after_submit
                        .after(RenderSystems::Render)
                        .before(RenderSystems::Cleanup),
                )
                .add_systems(Render, finish_render.after(RenderSystems::PostCleanup));
        }
        app.add_systems(
            Last,
            (
                move |real: Res<Time<Real>>, mut frame: Local<u64>| {
                    *frame += 1;
                    let ms = real.delta_secs() * 1000.;
                    if ms > limit {
                        info!("slow frame {}: {ms:.1} ms", *frame);
                    }
                },
                report_stages.after(finish_main),
            ),
        );
    }
}

fn disable_test_light_clusters(
    mut commands: Commands,
    cameras: Query<Entity, Added<Camera3d>>,
    mut settings: ResMut<bevy::light::cluster::GlobalClusterSettings>,
) {
    // In Bevy 0.19 the GPU path still allocates a dummy texture for a None
    // camera's zero-size grid. Select the documented CPU fallback as well;
    // it skips disabled cameras rather than creating an invalid GPU texture.
    if settings.gpu_clustering.is_some() {
        settings.gpu_clustering = None;
        info!("renderer experiment: disabled global GPU light clustering");
    }
    for camera in &cameras {
        commands.entity(camera).insert(bevy::light::cluster::ClusterConfig::None);
        info!("renderer experiment: disabled light clusters for camera {camera}");
    }
}

fn report_stages(
    mut last: Local<Option<Instant>>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
) {
    let now = Instant::now();
    let Some(previous) = *last else {
        *last = Some(now);
        return;
    };
    if now.duration_since(previous).as_secs_f64() < 1. {
        return;
    }
    *last = Some(now);
    let stages = std::mem::take(&mut *STAGES.get_or_init(Default::default).lock().unwrap());
    for (name, stage) in stages {
        info!(
            "frame stage {name}: calls={} total={:.3}ms mean={:.3}ms max={:.3}ms",
            stage.calls,
            stage.total_ms,
            stage.total_ms / stage.calls as f64,
            stage.max_ms
        );
    }
    // The store retains paths from cameras/passes which may no longer draw.
    // Do not attribute those stale samples to the current scene. Means cover
    // Bevy's retained history, not the full one-second logging interval.
    let mut passes: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.path().as_str().starts_with("render/"))
        .filter(|diagnostic| {
            diagnostic.measurement().is_some_and(|measurement| {
                measurement.value.is_finite() && measurement.time.elapsed().as_secs_f64() < 1.
            })
        })
        .collect();
    passes.sort_by(|a, b| a.path().as_str().cmp(b.path().as_str()));
    for diagnostic in passes {
        if let Some(mean) = diagnostic.average().filter(|mean| mean.is_finite()) {
            info!(
                "render diagnostic {}: history_mean={mean:.3}{} samples={}",
                diagnostic.path(),
                diagnostic.suffix,
                diagnostic.measurements().count(),
            );
        }
    }
}

pub(crate) fn desktop_plugins() -> PluginGroupBuilder {
    // Bevy's pipeline renders frame N while the main app advances N + 1.
    // Local look already uses this frame's mouse input; keep that pose in the
    // frame we render instead of allowing an extra CPU frame to run ahead.
    DefaultPlugins.build().disable::<PipelinedRenderingPlugin>()
}

pub(crate) fn surface_frame_latency() -> Option<NonZeroU32> {
    // This is a backend hint, not a guarantee about the OS compositor/scanout.
    NonZeroU32::new(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_present_override_wins_and_invalid_override_keeps_saved_vsync() {
        for vsync in [false, true] {
            for (name, expected) in [
                ("immediate", PresentMode::Immediate),
                ("mailbox", PresentMode::Mailbox),
                ("fifo", PresentMode::Fifo),
            ] {
                assert_eq!(resolve_present_mode(vsync, Some(name)), expected);
            }
            let saved = if vsync {
                PresentMode::AutoVsync
            } else {
                PresentMode::AutoNoVsync
            };
            for requested in [None, Some(""), Some("invalid")] {
                assert_eq!(resolve_present_mode(vsync, requested), saved);
            }
        }
    }

    #[test]
    fn desktop_retains_renderer_without_running_a_cpu_frame_ahead() {
        let plugins = desktop_plugins();
        assert!(plugins.enabled::<bevy::render::RenderPlugin>());
        assert!(!plugins.enabled::<PipelinedRenderingPlugin>());
        assert_eq!(surface_frame_latency().unwrap().get(), 1);
    }
}

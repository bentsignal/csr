//! Opt-in, bounded cadence samples and once-per-second main-world census.
//! Counts describe retained CPU assets/entities, not GPU memory or draw calls.
use bevy::{
    camera::visibility::NoFrustumCulling, mesh::skinning::SkinnedMesh, prelude::*,
    window::PrimaryWindow,
};
use std::time::Instant;

const MAX_SAMPLES: usize = 8192;

#[derive(Resource, Default)]
pub(super) struct ReportInterval {
    pub due: bool,
}

#[derive(Default)]
pub(super) struct FrameWindow {
    samples: Vec<f64>,
    count: u64,
    rejected: u64,
    total: f64,
    maximum: f64,
}

impl FrameWindow {
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
    ),
    characters: Res<crate::characters::Characters>,
) {
    interval.due = false;
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
    info!(
        "perf interval: elapsed={:.3}s window={seconds:.3}s frames={} mean_ms={:.4} p50_p95_p99_ms={quantiles:?} max_ms={:.4} overflow={} rejected={}",
        real.elapsed_secs_f64(),
        samples.count,
        samples.total / samples.count.max(1) as f64,
        samples.maximum,
        samples.count.saturating_sub(samples.samples.len() as u64),
        samples.rejected,
    );
    samples.clear();
    if let Ok(window) = windows.single() {
        info!(
            "perf context: map={} state={:?} page={:?} size={}x{} focused={} present_requested={:?} frame_latency_hint={:?} screenshot={} input_enabled={} noclip={} spectating={}",
            level_name.name,
            state.get(),
            menu.page,
            window.physical_width(),
            window.physical_height(),
            window.focused,
            window.present_mode,
            window.desired_maximum_frame_latency,
            options.0.screenshot.is_some(),
            input.0,
            local.as_ref().is_some_and(|p| p.noclip),
            local.as_ref().is_some_and(|p| p.spectating.is_some()),
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

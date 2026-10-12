//! Nonblocking render preparation before remote admission. This acknowledges
//! CPU preparation and submission, never GPU idle or future shader variants.
use bevy::{
    asset::AssetId,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        mesh::RenderMesh,
        render_asset::RenderAssets,
        render_resource::{CachedPipelineState, PipelineCache},
        texture::GpuImage,
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

/// Bound the retained ID arrays and the work performed in each render cycle.
pub const MAX_SCENE_ASSETS: usize = 65_536;
const PENDING: u8 = 0;
const READY: u8 = 1;
const CANCELLED: u8 = 2;
const FAILED: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateStatus {
    Pending,
    Ready,
    Cancelled,
    /// A pipeline failed. Bevy's renderer log retains the detailed error.
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeginError {
    TooManyAssets,
    GenerationExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallError {
    MissingRenderApp,
}

/// Each generation owns its own atomic: an old render frame cannot approve a
/// replacement, even if the main-world resource is removed and recreated.
#[derive(Clone)]
pub struct GateTicket {
    generation: u64,
    state: Arc<AtomicU8>,
}
impl GateTicket {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn status(&self) -> GateStatus {
        match self.state.load(Ordering::Acquire) {
            PENDING => GateStatus::Pending,
            READY => GateStatus::Ready,
            CANCELLED => GateStatus::Cancelled,
            _ => GateStatus::Failed,
        }
    }

    /// Revokes readiness too. A racing acknowledgment uses compare_exchange and
    /// cannot undo cancellation; a result observed before cancel must not be used.
    pub fn cancel(&self) {
        self.state.store(CANCELLED, Ordering::Release);
    }
}

#[derive(Clone)]
struct Request {
    ticket: GateTicket,
    meshes: Arc<[AssetId<Mesh>]>,
    images: Arc<[AssetId<Image>]>,
}

#[derive(Resource, Default)]
pub struct RenderGate {
    generation: u64,
    active: Option<Request>,
}
impl RenderGate {
    /// Call only after scene commands and required resources are applied. The
    /// supplied IDs must cover the currently owned scene assets, not just one
    /// sentinel mesh. Keep scene cameras active behind the loading overlay.
    pub fn begin(
        &mut self,
        meshes: &[AssetId<Mesh>],
        images: &[AssetId<Image>],
    ) -> Result<GateTicket, BeginError> {
        if meshes
            .len()
            .checked_add(images.len())
            .is_none_or(|n| n > MAX_SCENE_ASSETS)
        {
            return Err(BeginError::TooManyAssets);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(BeginError::GenerationExhausted)?;
        self.cancel();
        let ticket = GateTicket {
            generation,
            state: Arc::new(AtomicU8::new(PENDING)),
        };
        self.active = Some(Request {
            ticket: ticket.clone(),
            meshes: meshes.into(),
            images: images.into(),
        });
        self.generation = generation;
        Ok(ticket)
    }

    pub fn cancel(&mut self) {
        if let Some(request) = self.active.take() {
            request.ticket.cancel();
        }
    }
}
impl Drop for RenderGate {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Resource, Default)]
struct Installed;
#[derive(Resource, Default)]
struct ExtractedRequest(Option<Request>);

/// Install during plugin build, after RenderPlugin and before pipelined
/// rendering moves RenderApp onto its render thread. Absence fails closed.
pub fn install(app: &mut App) -> Result<(), InstallError> {
    if app.world().contains_resource::<Installed>() {
        return Ok(());
    }
    let render_app = app
        .get_sub_app_mut(RenderApp)
        .ok_or(InstallError::MissingRenderApp)?;
    render_app.add_systems(ExtractSchedule, extract_request);
    render_app.add_systems(Render, acknowledge.in_set(RenderSystems::PostCleanup));
    app.init_resource::<RenderGate>()
        .init_resource::<Installed>();
    Ok(())
}

fn extract_request(mut commands: Commands, gate: Extract<Option<Res<RenderGate>>>) {
    // Always replace with None after cancellation/removal. ExtractResource's
    // optional source behavior otherwise retains a stale render-world resource.
    commands.insert_resource(ExtractedRequest(
        gate.as_ref().and_then(|g| g.active.clone()),
    ));
}

#[derive(Default)]
struct Progress {
    state: Option<Arc<AtomicU8>>,
    complete_cycles: u8,
}
impl Progress {
    fn observe(
        &mut self,
        request: &Request,
        assets_ready: bool,
        pipelines_waiting: bool,
        pipeline_failed: bool,
    ) {
        if self
            .state
            .as_ref()
            .is_none_or(|state| !Arc::ptr_eq(state, &request.ticket.state))
        {
            self.state = Some(request.ticket.state.clone());
            self.complete_cycles = 0;
        }
        if request.ticket.status() != GateStatus::Pending {
            return;
        }
        if pipeline_failed {
            let _ = request.ticket.state.compare_exchange(
                PENDING,
                FAILED,
                Ordering::Release,
                Ordering::Relaxed,
            );
            return;
        }
        if !assets_ready || pipelines_waiting {
            self.complete_cycles = 0;
            return;
        }
        self.complete_cycles = self.complete_cycles.saturating_add(1);
        if self.complete_cycles >= 2 {
            let _ = request.ticket.state.compare_exchange(
                PENDING,
                READY,
                Ordering::Release,
                Ordering::Relaxed,
            );
        }
    }
}

fn assets_ready(
    request: &Request,
    meshes: &RenderAssets<RenderMesh>,
    images: &RenderAssets<GpuImage>,
) -> bool {
    request.meshes.iter().all(|id| meshes.get(*id).is_some())
        && request.images.iter().all(|id| images.get(*id).is_some())
}

fn acknowledge(
    request: Option<Res<ExtractedRequest>>,
    meshes: Option<Res<RenderAssets<RenderMesh>>>,
    images: Option<Res<RenderAssets<GpuImage>>>,
    pipelines: Option<Res<PipelineCache>>,
    mut progress: Local<Progress>,
) {
    let Some(request) = request.as_ref().and_then(|r| r.0.as_ref()) else {
        *progress = Progress::default();
        return;
    };
    let (Some(meshes), Some(images), Some(pipelines)) = (meshes, images, pipelines) else {
        // A missing/recovering renderer cannot count a preparation cycle.
        progress.observe(request, false, true, false);
        return;
    };
    let waiting = pipelines.waiting_pipelines().next().is_some();
    progress.observe(
        request,
        assets_ready(request, &meshes, &images),
        waiting,
        // Shader-not-yet-loaded errors remain on Bevy's retry queue. They are
        // pending work, not a terminal failure. Inspect errors once it settles.
        !waiting
            && pipelines
                .pipelines()
                .any(|p| matches!(p.state, CachedPipelineState::Err(_))),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::{Assets, RenderAssetUsages};
    use bevy::mesh::{BaseMeshPipelineKey, MeshVertexBufferLayouts, PrimitiveTopology};
    use bevy::render::mesh::RenderMeshBufferInfo;

    fn request(gate: &RenderGate) -> Request {
        gate.active.clone().unwrap()
    }

    #[test]
    fn delayed_assets_and_pipeline_work_require_two_consecutive_complete_cycles() {
        let mut gate = RenderGate::default();
        let ticket = gate.begin(&[], &[]).unwrap();
        let request = request(&gate);
        let mut progress = Progress::default();
        progress.observe(&request, false, false, false);
        progress.observe(&request, true, true, false);
        assert_eq!(ticket.status(), GateStatus::Pending);
        progress.observe(&request, true, false, false);
        progress.observe(&request, false, false, false);
        progress.observe(&request, true, false, false);
        assert_eq!(ticket.status(), GateStatus::Pending);
        progress.observe(&request, true, false, false);
        assert_eq!(ticket.status(), GateStatus::Ready);
    }

    #[test]
    fn stale_generation_and_late_cancel_never_approve_a_replacement() {
        let mut gate = RenderGate::default();
        let old = gate.begin(&[], &[]).unwrap();
        let stale = request(&gate);
        let mut progress = Progress::default();
        progress.observe(&stale, true, false, false);
        let current = gate.begin(&[], &[]).unwrap();
        assert!(current.generation() > old.generation());
        progress.observe(&stale, true, false, false);
        assert_eq!(old.status(), GateStatus::Cancelled);
        assert_eq!(current.status(), GateStatus::Pending);
        let fresh = request(&gate);
        progress.observe(&fresh, true, false, false);
        assert_eq!(current.status(), GateStatus::Pending);
        current.cancel();
        progress.observe(&fresh, true, false, false);
        assert_eq!(current.status(), GateStatus::Cancelled);
        gate.cancel();
        gate.cancel();
        assert!(gate.active.is_none());
    }

    #[test]
    fn resource_recreation_with_repeated_generation_cannot_reuse_old_progress() {
        let mut old_gate = RenderGate::default();
        let old = old_gate.begin(&[], &[]).unwrap();
        let old_request = request(&old_gate);
        let mut progress = Progress::default();
        progress.observe(&old_request, true, false, false);
        drop(old_gate);
        let mut new_gate = RenderGate::default();
        let fresh = new_gate.begin(&[], &[]).unwrap();
        assert_eq!(old.generation(), fresh.generation());
        progress.observe(&old_request, true, false, false);
        progress.observe(&request(&new_gate), true, false, false);
        assert_eq!(old.status(), GateStatus::Cancelled);
        assert_eq!(fresh.status(), GateStatus::Pending);
        progress.observe(&request(&new_gate), true, false, false);
        assert_eq!(fresh.status(), GateStatus::Ready);
    }

    #[test]
    fn failed_pipeline_is_terminal_and_cancel_revokes_ready() {
        let mut gate = RenderGate::default();
        let ticket = gate.begin(&[], &[]).unwrap();
        let mut progress = Progress::default();
        let failed = request(&gate);
        progress.observe(&failed, true, false, true);
        progress.observe(&failed, true, false, false);
        assert_eq!(ticket.status(), GateStatus::Failed);
        let ready = gate.begin(&[], &[]).unwrap();
        let current = request(&gate);
        progress.observe(&current, true, false, false);
        progress.observe(&current, true, false, false);
        assert_eq!(ready.status(), GateStatus::Ready);
        gate.cancel();
        assert_eq!(ready.status(), GateStatus::Cancelled);
    }

    #[test]
    fn asset_bound_and_generation_errors_leave_the_current_request_unchanged() {
        let mut gate = RenderGate::default();
        let current = gate.begin(&[], &[]).unwrap();
        let mut assets = Assets::<Image>::default();
        let id = assets.add(Image::default()).id();
        assert!(matches!(
            gate.begin(&[], &vec![id; MAX_SCENE_ASSETS + 1]),
            Err(BeginError::TooManyAssets)
        ));
        assert_eq!(current.status(), GateStatus::Pending);
        gate.generation = u64::MAX;
        assert!(matches!(
            gate.begin(&[], &[]),
            Err(BeginError::GenerationExhausted)
        ));
        assert_eq!(current.status(), GateStatus::Pending);
    }

    #[test]
    fn actual_render_asset_cache_presence_checks_every_supplied_id() {
        let mut source_meshes = Assets::<Mesh>::default();
        let mut source_images = Assets::<Image>::default();
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        let mut layouts = MeshVertexBufferLayouts::default();
        let rendered = RenderMesh {
            vertex_count: 0,
            aabb_center: Vec3::ZERO,
            buffer_info: RenderMeshBufferInfo::NonIndexed,
            key_bits: BaseMeshPipelineKey::empty(),
            layout: mesh.get_mesh_vertex_buffer_layout(&mut layouts),
        };
        let mesh_id = source_meshes.add(mesh).id();
        let image_id = source_images.add(Image::default()).id();
        let mut meshes = RenderAssets::<RenderMesh>::default();
        let images = RenderAssets::<GpuImage>::default();
        let mut gate = RenderGate::default();
        gate.begin(&[mesh_id], &[]).unwrap();
        assert!(!assets_ready(&request(&gate), &meshes, &images));
        meshes.insert(mesh_id, rendered);
        assert!(assets_ready(&request(&gate), &meshes, &images));
        gate.begin(&[mesh_id], &[image_id]).unwrap();
        assert!(!assets_ready(&request(&gate), &meshes, &images));
        meshes.remove(mesh_id);
        assert!(!assets_ready(&request(&gate), &meshes, &images));
    }

    #[test]
    fn missing_renderer_fails_closed_and_extraction_clears_cancelled_requests() {
        let mut empty = App::new();
        assert_eq!(install(&mut empty), Err(InstallError::MissingRenderApp));
        assert!(!empty.world().contains_resource::<RenderGate>());
        let mut app = App::new();
        app.add_plugins(bevy::render::extract_plugin::ExtractPlugin::default());
        install(&mut app).unwrap();
        install(&mut app).unwrap();
        let ticket = app
            .world_mut()
            .resource_mut::<RenderGate>()
            .begin(&[], &[])
            .unwrap();
        // A real extraction runs into the mock RenderApp, whose missing GPU
        // assets/pipeline cache must never fabricate readiness.
        app.update();
        let render = app.get_sub_app_mut(RenderApp).unwrap();
        render.world_mut().run_schedule(Render);
        assert_eq!(ticket.status(), GateStatus::Pending);
        assert!(render.world().resource::<ExtractedRequest>().0.is_some());
        app.world_mut().resource_mut::<RenderGate>().cancel();
        app.update();
        let render = app.get_sub_app_mut(RenderApp).unwrap();
        render.world_mut().run_schedule(Render);
        assert!(render.world().resource::<ExtractedRequest>().0.is_none());
        assert_eq!(ticket.status(), GateStatus::Cancelled);
    }
}

//! Optical defocus before the HUD. This uses the completed viewmodel depth,
//! including its lens discard, rather than blurring the magnified world as if
//! it were part of the gun. Pipeline plumbing follows Bevy 0.19.1's local
//! custom_post_processing example (MIT OR Apache-2.0).
use bevy::{
    core_pipeline::{schedule::Core3d, Core3dSystems, FullscreenShader},
    prelude::*,
    render::{
        extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent,
            ExtractComponentPlugin, UniformComponentPlugin},
        render_resource::{binding_types::{sampler, texture_2d, uniform_buffer}, *},
        renderer::{RenderContext, RenderDevice, ViewQuery},
        view::{ViewDepthTexture, ViewTarget},
        RenderApp, RenderStartup,
    },
};

pub struct OpticBlurPlugin;

impl Plugin for OpticBlurPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "shaders/optic_blur.wgsl");
        app.init_resource::<OpticState>()
            .add_plugins((ExtractComponentPlugin::<OpticSettings>::default(),
                UniformComponentPlugin::<OpticSettings>::default()))
            .add_systems(PostUpdate, configure_camera);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return; };
        render_app.add_systems(RenderStartup, init_pipeline)
            .add_systems(Core3d, defocus.in_set(Core3dSystems::EarlyPostProcess));
    }
}

#[derive(Component, Default, Clone, Copy, ExtractComponent, ShaderType)]
pub(crate) struct OpticSettings {
    pub aperture: Vec4,
    /// Sniper enabled, motion sigma at 1080h, rifle max sigma at 1080h, near.
    pub params: Vec4,
}

#[derive(Resource, Default)]
pub(crate) struct OpticState(pub OpticSettings);

fn configure_camera(
    mut commands: Commands,
    state: Res<OpticState>,
    mut cameras: Query<(Entity, &mut Camera3d, &Projection, Option<&mut OpticSettings>),
        With<crate::player::ViewModelCamera>>,
) {
    for (entity, mut camera, projection, settings) in &mut cameras {
        let usage = TextureUsages::from(camera.depth_texture_usages);
        if !usage.contains(TextureUsages::TEXTURE_BINDING) {
            camera.depth_texture_usages = (usage | TextureUsages::TEXTURE_BINDING).into();
        }
        let mut next = state.0;
        if let Projection::Perspective(projection) = projection { next.params.w = projection.near; }
        // Bounded render-only controls for identical still/motion captures.
        // Zero DOF removes only weapon defocus; zero scope blur removes the
        // scene/vignette pass while leaving the accepted reticle bloom alone.
        if next.params.x > 0.0 {
            if let Some(sigma) = control("CSR_TEST_SCOPE_BLUR") {
                next.params.y = sigma;
                if sigma == 0.0 { next.params.x = 0.0; }
            }
        }
        if next.params.z > 0.0 {
            if let Some(sigma) = control("CSR_TEST_OPTIC_DOF") { next.params.z = sigma; }
        }
        if let Some(mut settings) = settings { *settings = next; }
        else { commands.entity(entity).insert(next); }
    }
}

fn control(name: &str) -> Option<f32> {
    std::env::var(name).ok()?.parse::<f32>().ok()
        .filter(|value| value.is_finite()).map(|value| value.clamp(0.0, 16.0))
}

struct PipelineVariant {
    layout: BindGroupLayoutDescriptor,
    horizontal: CachedRenderPipelineId,
    composite: CachedRenderPipelineId,
}

#[derive(Resource)]
struct OpticPipeline {
    variants: [PipelineVariant; 2],
    sampler: Sampler,
}

fn init_pipeline(
    mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>,
    fullscreen: Res<FullscreenShader>, cache: Res<PipelineCache>,
) {
    let shader = assets.load("embedded://client/shaders/optic_blur.wgsl");
    let variants = [false, true].map(|multisampled| {
        let layout = BindGroupLayoutDescriptor::new("optic_blur_layout",
            &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (
                texture_2d(TextureSampleType::Float { filterable: true }),
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                BindingType::Texture { sample_type: TextureSampleType::Depth,
                    view_dimension: TextureViewDimension::D2, multisampled },
                uniform_buffer::<OpticSettings>(true),
            )));
        let queue = |entry: &'static str, format| cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some(format!("optic_blur_{entry}").into()),
            layout: vec![layout.clone()],
            vertex: fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: shader.clone(),
                shader_defs: if multisampled { vec!["MULTISAMPLED".into()] } else { vec![] },
                entry_point: Some(entry.into()),
                targets: vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })],
            }),
            ..default()
        });
        PipelineVariant { horizontal: queue("horizontal", TextureFormat::Rgba16Float),
            composite: queue("composite", TextureFormat::Rgba8UnormSrgb), layout }
    });
    commands.insert_resource(OpticPipeline { variants, sampler: device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..default()
    }) });
}

// One intermediate per final camera view, reused until its physical size changes.
// Float alpha carries filtered weapon coverage, not scene opacity.
#[derive(Default)]
struct Intermediate(Option<(Extent3d, TextureView)>);

fn defocus(
    view: ViewQuery<(&ViewTarget, &ViewDepthTexture, &OpticSettings, &DynamicUniformIndex<OpticSettings>)>,
    pipeline: Option<Res<OpticPipeline>>, cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<OpticSettings>>,
    mut intermediate: Local<Intermediate>, mut ctx: RenderContext,
) {
    let (target, depth, settings, index) = view.into_inner();
    if settings.params.x <= 0.0 && settings.params.z <= 0.0 { return; }
    let Some(pipeline) = pipeline else { return; };
    let variant = &pipeline.variants[usize::from(depth.texture.sample_count() > 1)];
    let (Some(horizontal), Some(composite), Some(binding)) = (
        cache.get_render_pipeline(variant.horizontal), cache.get_render_pipeline(variant.composite),
        uniforms.uniforms().binding(),
    ) else { return; };
    let size = target.main_texture().size();
    if intermediate.0.as_ref().is_none_or(|(old, _)| *old != size) {
        let texture = ctx.render_device().create_texture(&TextureDescriptor {
            label: Some("optic_blur_intermediate"), size, mip_level_count: 1, sample_count: 1,
            dimension: TextureDimension::D2, format: TextureFormat::Rgba16Float,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
        });
        intermediate.0 = Some((size, texture.create_view(&TextureViewDescriptor::default())));
    }
    let blurred = &intermediate.0.as_ref().unwrap().1;
    let post = target.post_process_write();
    let layout = cache.get_bind_group_layout(&variant.layout);
    // First pass never binds its own output. Second pass retains the original
    // scene for sharp world/lens pixels and writes into the other main texture.
    let horizontal_group = ctx.render_device().create_bind_group("optic_blur_horizontal", &layout,
        &BindGroupEntries::sequential((post.source, post.source, &pipeline.sampler, depth.view(), binding.clone())));
    let composite_group = ctx.render_device().create_bind_group("optic_blur_composite", &layout,
        &BindGroupEntries::sequential((post.source, blurred, &pipeline.sampler, depth.view(), binding)));
    for (pipeline, group, destination) in [(horizontal, &horizontal_group, blurred),
        (composite, &composite_group, post.destination)] {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("optic_defocus"),
            color_attachments: &[Some(RenderPassColorAttachment { view: destination, depth_slice: None,
                resolve_target: None, ops: Operations::default() })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[index.index()]);
        pass.draw(0..3, 0..1);
    }
}

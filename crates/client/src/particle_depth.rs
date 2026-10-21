//! Snapshot completed opaque world depth for soft particle intersections.
//!
//! WorldMaterial intentionally has no prepass. DepthPrepass supplies Bevy's
//! separate, correctly sized/MSAA-matched sampled texture and adds COPY_SRC to
//! main depth; this copy fills it after the world has actually drawn. Only
//! late particle sampling uses this snapshot, not early SSAO/SSR consumers.
use bevy::{
    core_pipeline::{
        core_3d::{main_opaque_pass_3d, main_transparent_pass_3d},
        prepass::ViewPrepassTextures,
        schedule::{Core3d, Core3dSystems},
    },
    pbr::main_transmissive_pass_3d,
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        renderer::{RenderContext, ViewQuery},
        view::ViewDepthTexture,
        RenderApp,
    },
};

/// Insert only beside WorldPlayerCamera, never on sky/menu/viewmodel cameras.
#[derive(Component, Clone, ExtractComponent)]
pub struct WorldParticleDepth;

pub struct ParticleDepthPlugin;

impl Plugin for ParticleDepthPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<WorldParticleDepth>::default());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.add_systems(
            Core3d,
            snapshot_opaque_depth
                .after(main_opaque_pass_3d)
                .before(main_transmissive_pass_3d)
                .before(main_transparent_pass_3d)
                .in_set(Core3dSystems::MainPass),
        );
    }
}

fn snapshot_opaque_depth(
    view: ViewQuery<(&ViewDepthTexture, &ViewPrepassTextures), With<WorldParticleDepth>>,
    mut ctx: RenderContext,
) {
    let (source, prepass) = view.into_inner();
    let Some(depth) = &prepass.depth else {
        return;
    };
    // Same whole-subresource copy as Bevy's prepass, including MSAA. Copy
    // during this view: later cameras can clear/reuse the main depth target.
    // The transparent pass keeps its writable depth attachment; particles
    // sample this separate texture through the existing view binding 20.
    ctx.command_encoder().copy_texture_to_texture(
        source.texture.as_image_copy(),
        depth.texture.texture.as_image_copy(),
        prepass.size,
    );
}

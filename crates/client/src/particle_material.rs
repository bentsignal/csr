//! Bounded SpriteCard frame interpolation and authored additive-over composition.
use bevy::{
    mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError, VertexFormat,
    },
    shader::ShaderRef,
};

pub type ParticleMaterial = ExtendedMaterial<StandardMaterial, ParticleExtension>;

/// Next sheet UV and interpolation fraction, constant in Z across each card.
pub const ATTRIBUTE_NEXT_FRAME: MeshVertexAttribute =
    MeshVertexAttribute::new("Particle_NextFrame", 190_104_105, VertexFormat::Float32x3);

/// Local card coordinates, independent of the selected texture-sheet rectangle.
pub const ATTRIBUTE_CARD_UV: MeshVertexAttribute =
    MeshVertexAttribute::new("Particle_CardUv", 190_104_106, VertexFormat::Float32x2);

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ParticleExtension {
    #[uniform(100)]
    pub additive_over: u32,
    #[uniform(101)]
    pub blend_frames: u32,
    /// Zero disables depth fade; otherwise inverse fade distance in Source units.
    #[uniform(102)]
    pub depth_fade_inverse_distance: f32,
    #[uniform(103)]
    pub soften_card_edges: u32,
}

/// Normalize the existing provisional soft-intersection settings. A positive
/// authored scale multiplies inverse distance; absent scale starts at one.
/// Field fire widens that band in its loader after rendered layer isolation;
/// this helper preserves other utility and held-wick behavior. These are not
/// measured native SpriteCard defaults. Explicit DEPTHBLEND=0 stays disabled.
pub fn depth_fade_inverse_distance(enabled: &str, scale: &str) -> f32 {
    if !enabled
        .parse::<f32>()
        .is_ok_and(|v| v.is_finite() && v > 0.0)
    {
        return 0.0;
    }
    scale
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(1.0)
}

impl MaterialExtension for ParticleExtension {
    fn vertex_shader() -> ShaderRef {
        "embedded://client/shaders/particle.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "embedded://client/shaders/particle.wgsl".into()
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(5),
            ATTRIBUTE_NEXT_FRAME.at_shader_location(8),
            ATTRIBUTE_CARD_UV.at_shader_location(9),
        ])?];
        Ok(())
    }
}

/// The installed fire VMT explicitly documents ADDOVERBLEND as ONE:INVSRCALPHA.
/// ADDSELF remains the previous approximation until its shader semantics are measured.
pub fn alpha_mode(additive_over: bool, additive: bool) -> AlphaMode {
    if additive_over {
        AlphaMode::Premultiplied
    } else if additive {
        AlphaMode::Add
    } else {
        AlphaMode::Blend
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_over_blend_takes_precedence_over_addself() {
        // Ground fire sets both flags. Treating either as plain Add loses the
        // destination attenuation and lets overlapping cards accumulate forever.
        assert_eq!(alpha_mode(true, true), AlphaMode::Premultiplied);
        assert_eq!(alpha_mode(false, true), AlphaMode::Add);
        assert_eq!(alpha_mode(false, false), AlphaMode::Blend);
    }
}

//! SpriteCard's explicitly authored additive-over blend, without changing gain.
use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

pub type ParticleMaterial = ExtendedMaterial<StandardMaterial, ParticleExtension>;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ParticleExtension {
    #[uniform(100)]
    pub additive_over: u32,
}

impl MaterialExtension for ParticleExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://client/shaders/particle.wgsl".into()
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
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

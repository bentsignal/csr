//! Plane-bounded impact artwork using the existing completed world-depth copy.
use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

pub(crate) type ImpactDecalMaterial = ExtendedMaterial<StandardMaterial, ImpactDecalExtension>;

/// Existing cosmetic stand-off, shared with attached marks so the depth slab
/// follows their actual plane. Units remain Source inches.
pub(crate) const MARK_OFFSET: f32 = 0.12;
// Collision contacts need not coincide with the visible face: the installed
// Nuke wall control differs by one unit. Retain that contact/mesh allowance,
// the existing stand-off and trace roundoff. This is a clip slab, not a fade.
const MAX_PLANE_DISTANCE: f32 = 1.0 + MARK_OFFSET + 2.0 * collision::DIST_EPSILON;
// The owner reports missing bullet marks with the one-unit control.
// Permit small collision/render-face gaps without admitting distant geometry
// through holes. Provisional four-unit allowance; native projection is pending.
// Blood retains its accepted default slab and attachment behavior.
const BULLET_PLANE_DISTANCE: f32 = 4.0 + MARK_OFFSET + 2.0 * collision::DIST_EPSILON;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(crate) struct ImpactDecalExtension {
    #[uniform(100)]
    max_plane_distance: f32,
}

impl Default for ImpactDecalExtension {
    fn default() -> Self {
        Self { max_plane_distance: MAX_PLANE_DISTANCE }
    }
}

impl MaterialExtension for ImpactDecalExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://client/shaders/impact_decal.wgsl".into()
    }
    // The base material supplies Multiply, its vertex shader and depth bias.
    // Marks must neither become receivers in the copy nor cast shadows.
    fn enable_prepass() -> bool { false }
    fn enable_shadows() -> bool { false }
}

pub(crate) fn material(texture: Handle<Image>) -> ImpactDecalMaterial {
    ImpactDecalMaterial {
        base: StandardMaterial {
            base_color: Color::linear_rgba(2., 2., 2., 1.),
            base_color_texture: Some(texture),
            unlit: true,
            alpha_mode: AlphaMode::Multiply,
            cull_mode: None,
            depth_bias: 1.,
            ..default()
        },
        extension: ImpactDecalExtension { max_plane_distance: BULLET_PLANE_DISTANCE },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::pbr::Material;

    #[test]
    fn receiver_plane_clip_preserves_authored_multiply_at_oblique_views() {
        let mat = material(default());
        assert_eq!(Material::alpha_mode(&mat), AlphaMode::Multiply);
        assert!(mat.base.unlit);
        assert_eq!(mat.base.base_color.to_linear().to_f32_array(), [2., 2., 2., 1.]);
        assert!(!<ImpactDecalMaterial as Material>::enable_prepass());
        assert!(!<ImpactDecalMaterial as Material>::enable_shadows());

        // Synthetic depth reconstruction reference for the WGSL plane test.
        // Tangential ray separation grows at grazing angles; normal distance
        // must still admit the real face and reject a farther face through a
        // hole, an off-plane face, and empty reverse-Z depth.
        let clip = Mat4::perspective_infinite_reverse_rh(1.2, 16. / 9., 4.);
        for angle in [0.0_f32, 0.8, 1.4] {
            let rotation = Quat::from_rotation_y(angle);
            let normal = rotation * Vec3::Z;
            let contact = Vec3::new(0., 0., -128.);
            let quad = contact + normal * (MARK_OFFSET + collision::DIST_EPSILON);
            let accepts = |receiver: Option<Vec3>| {
                let Some(receiver) = receiver else { return false; };
                let ndc = clip.project_point3(receiver);
                let reconstructed = clip.inverse().project_point3(ndc);
                (reconstructed - quad).dot(normal).abs() <= mat.extension.max_plane_distance
            };
            assert!(accepts(Some(contact)));
            assert!(accepts(Some(contact + rotation * Vec3::X * 20.)));
            // A real receiver can sit behind the collision contact. The old
            // trace-epsilon-only slab erased these ordinary Nuke wall marks.
            assert!(accepts(Some(contact - normal)));
            assert!(accepts(Some(contact - normal * 2.)));
            assert!(accepts(Some(contact - normal * 4.)));
            assert!(!accepts(Some(contact - normal * 8.)));
            assert!(!accepts(Some(contact + normal * 8.)));
            assert!(!accepts(None));
        }
    }

    #[test]
    fn bullet_allowance_does_not_widen_surface_blood() {
        let blood = ImpactDecalExtension::default();
        assert_eq!(blood.max_plane_distance, MAX_PLANE_DISTANCE);
        assert!(blood.max_plane_distance < 2.);
        assert!(material(default()).extension.max_plane_distance > 4.);
    }
}

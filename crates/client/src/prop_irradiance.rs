//! Directional surface lighting transported without a per-fragment texture-row ID.
//! Three integer attributes carry 21 half floats (six RGB cube faces, Sun RGB).
//! They are unpacked in the vertex shader, then interpolated as float coefficients.

use bevy::{mesh::MeshVertexAttribute, render::render_resource::VertexFormat};
use super::PropLight;

pub(crate) const ATTRIBUTES: [MeshVertexAttribute; 3] = [
    MeshVertexAttribute::new("Prop_Irradiance0", 0x43535200, VertexFormat::Uint32x4),
    MeshVertexAttribute::new("Prop_Irradiance1", 0x43535201, VertexFormat::Uint32x4),
    MeshVertexAttribute::new("Prop_Irradiance2", 0x43535202, VertexFormat::Uint32x4),
];

/// Source +X,-X,+Y,-Y,+Z,-Z to Bevy +X,-X,+Y,-Y,+Z,-Z.
pub(crate) const CUBE_ORDER: [usize; 6] = [0, 1, 4, 5, 3, 2];

pub(crate) fn pack(light: &PropLight) -> [[u32; 4]; 3] {
    let mut halves = [0u16; 24];
    for (i, value) in CUBE_ORDER.into_iter().flat_map(|i| light.cube[i].to_array())
        .chain(light.sun.to_array()).enumerate()
    {
        halves[i] = crate::textures::f32_to_f16(value);
    }
    let mut packed = [[0u32; 4]; 3];
    for (i, pair) in halves.chunks_exact(2).enumerate() {
        packed[i / 4][i % 4] = u32::from(pair[0]) | (u32::from(pair[1]) << 16);
    }
    packed
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn unpack(packed: [[u32; 4]; 3], to_sun: Vec3) -> PropLight {
        let halves: Vec<_> = packed.into_iter().flatten().flat_map(|word|
            [vtf::f16_to_f32(word as u16), vtf::f16_to_f32((word >> 16) as u16)]).collect();
        let mut cube = [Vec3::ZERO; 6];
        for (bevy_face, source_face) in CUBE_ORDER.into_iter().enumerate() {
            cube[source_face] = Vec3::from_slice(&halves[bevy_face * 3..bevy_face * 3 + 3]);
        }
        PropLight { cube, sun: Vec3::from_slice(&halves[18..21]), to_sun }
    }

    #[test]
    fn packed_cube_preserves_signed_axes_rgb_and_sun_without_extra_texture_rows() {
        let light = PropLight {
            cube: std::array::from_fn(|i| Vec3::new(i as f32 + 0.25, i as f32 + 0.5, i as f32 + 0.75)),
            sun: Vec3::new(0.125, 0.25, 0.5), to_sun: Vec3::Z,
        };
        let packed = pack(&light);
        // First two halves are +X red/green; Sun starts at word9, low half.
        assert_eq!(packed[0][0], 0x3800_3400);
        assert_eq!(packed[2][1], 0x3400_3000);
        assert_eq!(packed[2][2], 0x0000_3800);
        assert_eq!(packed[2][3], 0);
        let restored = unpack(packed, light.to_sun);
        for n in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z,
            Vec3::new(1., -2., 3.).normalize()]
        {
            assert!((restored.eval(n) - light.eval(n)).length() < 0.00001);
        }
    }

    #[test]
    fn interpolated_coefficients_retain_bump_response_and_spatial_sun_boundary() {
        let covered = PropLight {
            cube: [Vec3::new(0.125, 0.25, 0.5); 6],
            sun: Vec3::ZERO, to_sun: Vec3::Z,
        };
        let mut exposed = covered;
        exposed.cube[0] = Vec3::new(2., 1., 0.5);
        exposed.sun = Vec3::new(1., 2., 4.);
        let a = unpack(pack(&covered), Vec3::Z);
        let b = unpack(pack(&exposed), Vec3::Z);
        let middle = PropLight {
            cube: std::array::from_fn(|i| a.cube[i].lerp(b.cube[i], 0.25)),
            sun: a.sun.lerp(b.sun, 0.25), to_sun: Vec3::Z,
        };
        // A surface whose geometric normal is -Z still needs Sun coefficients:
        // a tangent-space bump can turn it toward +Z. Interpolated final vertex
        // colors would lose both this response and the directional +X variation.
        assert_eq!(a.eval(-Vec3::Z), b.eval(-Vec3::Z));
        assert_eq!(middle.eval(Vec3::Z), covered.cube[4] + exposed.sun * 0.25);
        assert_eq!(middle.eval(Vec3::X), a.cube[0].lerp(b.cube[0], 0.25));
        assert_ne!(middle.eval(Vec3::X), middle.eval(-Vec3::X));
        let perturbed = Vec3::new(1., 0., 1.).normalize();
        let expected = a.eval(perturbed).lerp(b.eval(perturbed), 0.25);
        assert!((middle.eval(perturbed) - expected).length() < 0.00001);
    }
}

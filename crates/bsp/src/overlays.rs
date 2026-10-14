//! Authored `info_overlay` records (lump 45) and receiver-triangle clipping.
//!
//! Layout: public Source BSP File Format documentation; packed basis and fade
//! fields: independently documented srctools BSP format reader/API. No engine
//! implementation is used. Geometry remains in Source coordinates.

use crate::{Bsp, lump};
use glam::{Vec2, Vec3};
use std::io;

const RECORD_SIZE: usize = 352;
const MAX_OVERLAYS: usize = 512;
const MAX_RECEIVER_TRIANGLES: usize = 65_536;
const MAX_OUTPUT_VERTICES: usize = 65_536;

#[derive(Clone, Debug)]
pub struct Overlay {
    pub id: i32,
    pub texinfo: i16,
    pub render_order: u8,
    pub faces: Vec<i32>,
    pub u: [f32; 2],
    pub v: [f32; 2],
    pub corners: [Vec2; 4],
    pub origin: Vec3,
    pub normal: Vec3,
    pub basis_u: Vec3,
    pub basis_v: Vec3,
    /// Preserved authored distance limits; rendering does not yet fade these.
    pub fade_squared: [f32; 2],
}

pub(crate) fn parse(bytes: &[u8], fades: &[u8]) -> io::Result<Vec<Overlay>> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid BSP overlay lump");
    if bytes.len() % RECORD_SIZE != 0
        || bytes.len() / RECORD_SIZE > MAX_OVERLAYS
        || (!fades.is_empty() && fades.len() != bytes.len() / RECORD_SIZE * 8)
    {
        return Err(invalid());
    }
    let mut result = Vec::with_capacity(bytes.len() / RECORD_SIZE);
    for (i, bytes) in bytes.chunks_exact(RECORD_SIZE).enumerate() {
        let r = lump::Rec(bytes);
        let packed = r.u16(6);
        let count = usize::from(packed & 0x3fff);
        if count > 64 {
            return Err(invalid());
        }
        let points: [Vec3; 4] = std::array::from_fn(|i| r.vec3(280 + i * 12));
        let normal = r.vec3(340);
        let basis_u = Vec3::new(points[0].z, points[1].z, points[2].z);
        let basis_v = normal.cross(basis_u) * if points[3].z > 0.5 { -1.0 } else { 1.0 };
        let fade_squared = if fades.is_empty() {
            [-1.0, 0.0]
        } else {
            let f = lump::Rec(&fades[i * 8..i * 8 + 8]);
            [f.f32(0), f.f32(4)]
        };
        let overlay = Overlay {
            id: r.i32(0),
            texinfo: r.i16(4),
            render_order: (packed >> 14) as u8,
            faces: (0..count).map(|i| r.i32(8 + i * 4)).collect(),
            u: [r.f32(264), r.f32(268)],
            v: [r.f32(272), r.f32(276)],
            corners: points.map(Vec3::truncate),
            origin: r.vec3(328),
            normal,
            basis_u,
            basis_v,
            fade_squared,
        };
        if !overlay.origin.is_finite()
            || !normal.is_finite()
            || !basis_u.is_finite()
            || !basis_v.is_finite()
            || !overlay.corners.iter().all(|p| p.is_finite())
            || !overlay
                .u
                .iter()
                .chain(&overlay.v)
                .chain(&fade_squared)
                .all(|v| v.is_finite())
        {
            return Err(invalid());
        }
        result.push(overlay);
    }
    Ok(result)
}

#[derive(Clone, Copy, Debug)]
pub struct OverlayVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    /// Receiver lightmap coordinates, interpolated along clipped edges.
    pub luxel: Vec2,
    pub face: usize,
}

#[derive(Default, Debug)]
pub struct OverlayMesh {
    pub vertices: Vec<OverlayVertex>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayError {
    InvalidBasis,
    NonAffineQuad,
    InvalidReceiver,
    WorkBudget,
}

impl Overlay {
    /// Affine projection of a parallelogram handle quad. Non-affine handles
    /// deliberately fail closed until their native mapping is established.
    fn project(&self, p: Vec3) -> Result<Vec2, OverlayError> {
        let x = self.corners[3] - self.corners[0];
        let y = self.corners[1] - self.corners[0];
        let scale = x.length().max(y.length());
        if scale == 0.0
            || !scale.is_finite()
            || (self.corners[0] + self.corners[2] - self.corners[1] - self.corners[3]).length()
                > scale * 0.0001
        {
            return Err(OverlayError::NonAffineQuad);
        }
        let det = x.perp_dot(y);
        if !det.is_finite()
            || det.abs() < x.length() * y.length() * 0.0001
            || (self.basis_u.length_squared() - 1.0).abs() > 0.001
            || (self.basis_v.length_squared() - 1.0).abs() > 0.001
            || self.basis_u.dot(self.basis_v).abs() > 0.001
        {
            return Err(OverlayError::InvalidBasis);
        }
        let delta = p - self.origin;
        let q = Vec2::new(delta.dot(self.basis_u), delta.dot(self.basis_v)) - self.corners[0];
        let uv = Vec2::new(q.perp_dot(y), x.perp_dot(q)) / det;
        if uv.is_finite() {
            Ok(uv)
        } else {
            Err(OverlayError::InvalidBasis)
        }
    }
}

fn clip(mut poly: Vec<OverlayVertex>) -> Vec<OverlayVertex> {
    // Sutherland-Hodgman against the four unit-quad sides. Each receiver
    // triangle produces at most seven vertices; attributes follow its plane.
    for (axis, bound, sign) in [(0, 0.0, 1.0), (0, 1.0, -1.0), (1, 0.0, 1.0), (1, 1.0, -1.0)] {
        if poly.is_empty() {
            break;
        }
        let mut out = Vec::with_capacity(8);
        let mut a = *poly.last().unwrap();
        let mut da = (a.uv[axis] - bound) * sign;
        for b in poly {
            let db = (b.uv[axis] - bound) * sign;
            if (da >= 0.0) != (db >= 0.0) {
                let t = da / (da - db);
                out.push(OverlayVertex {
                    position: a.position.lerp(b.position, t),
                    normal: a.normal.lerp(b.normal, t).normalize_or_zero(),
                    uv: a.uv.lerp(b.uv, t),
                    luxel: a.luxel.lerp(b.luxel, t),
                    face: a.face,
                });
            }
            if db >= 0.0 {
                out.push(b);
            }
            a = b;
            da = db;
        }
        out.dedup_by(|a, b| a.uv.abs_diff_eq(b.uv, 1e-6));
        if out.len() > 1
            && out
                .first()
                .unwrap()
                .uv
                .abs_diff_eq(out.last().unwrap().uv, 1e-6)
        {
            out.pop();
        }
        poly = out;
    }
    poly
}

impl Bsp {
    /// Project only onto the explicitly authored receiver faces. Displacement
    /// vertices use their actual positions and grid luxels, never a flat fan.
    /// A failed overlay returns no partial mesh. No dynamic brush transforms
    /// are applied: callers restrict receivers to the world model.
    pub fn overlay_mesh(
        &self,
        overlay: &Overlay,
        include: &dyn Fn(usize) -> bool,
    ) -> Result<OverlayMesh, OverlayError> {
        overlay.project(overlay.origin)?;
        let mut mesh = OverlayMesh::default();
        let mut work = 0;
        for &index in &overlay.faces {
            let fi = usize::try_from(index).map_err(|_| OverlayError::InvalidReceiver)?;
            if !include(fi) {
                continue;
            }
            let face = self.faces.get(fi).ok_or(OverlayError::InvalidReceiver)?;
            let ti = self
                .texinfo
                .get(usize::try_from(face.texinfo).map_err(|_| OverlayError::InvalidReceiver)?)
                .ok_or(OverlayError::InvalidReceiver)?;
            // Validate surfedges/vertex references before the existing displacement
            // builder, whose face_vertices path assumes valid mesh indices.
            let planar = self
                .face_mesh(face)
                .map_err(|_| OverlayError::InvalidReceiver)?;
            let (positions, indices, luxels, normals) = if face.dispinfo >= 0 {
                let info = self
                    .dispinfos
                    .get(face.dispinfo as usize)
                    .ok_or(OverlayError::InvalidReceiver)?;
                if !(2..=4).contains(&info.power)
                    || info.disp_vert_start < 0
                    || info.disp_tri_start < 0
                {
                    return Err(OverlayError::InvalidReceiver);
                }
                let d = self.displacement(fi).ok_or(OverlayError::InvalidReceiver)?;
                let mut normals = vec![Vec3::ZERO; d.positions.len()];
                for tri in d.indices.chunks_exact(3) {
                    let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                    let n =
                        (d.positions[b] - d.positions[a]).cross(d.positions[c] - d.positions[a]);
                    normals[a] += n;
                    normals[b] += n;
                    normals[c] += n;
                }
                for n in &mut normals {
                    *n = n.normalize_or_zero();
                }
                (d.positions, d.indices, d.luxel_coords, normals)
            } else {
                let d = planar;
                let normals = vec![
                    self.face_normal(face)
                        .ok_or(OverlayError::InvalidReceiver)?;
                    d.positions.len()
                ];
                let luxels = d
                    .positions
                    .iter()
                    .map(|p| {
                        Vec2::new(
                            ti.lightmap_vecs[0].truncate().dot(*p) + ti.lightmap_vecs[0].w
                                - face.lightmap_mins[0] as f32,
                            ti.lightmap_vecs[1].truncate().dot(*p) + ti.lightmap_vecs[1].w
                                - face.lightmap_mins[1] as f32,
                        )
                    })
                    .collect();
                (d.positions, d.indices, luxels, normals)
            };
            work += indices.len() / 3;
            if work > MAX_RECEIVER_TRIANGLES {
                return Err(OverlayError::WorkBudget);
            }
            for tri in indices.chunks_exact(3) {
                let mut vertices = Vec::with_capacity(3);
                for &i in tri {
                    let i = i as usize;
                    let p = *positions.get(i).ok_or(OverlayError::InvalidReceiver)?;
                    let v = OverlayVertex {
                        position: p,
                        normal: normals[i],
                        uv: overlay.project(p)?,
                        luxel: luxels[i],
                        face: fi,
                    };
                    if !p.is_finite() || !v.normal.is_finite() || !v.luxel.is_finite() {
                        return Err(OverlayError::InvalidReceiver);
                    }
                    vertices.push(v);
                }
                let mut poly = clip(vertices);
                if poly.len() < 3 {
                    continue;
                }
                if mesh.vertices.len() + poly.len() > MAX_OUTPUT_VERTICES {
                    return Err(OverlayError::WorkBudget);
                }
                let base = mesh.vertices.len() as u32;
                for i in 1..poly.len() - 1 {
                    if (poly[i].position - poly[0].position)
                        .cross(poly[i + 1].position - poly[0].position)
                        .length_squared()
                        > 1e-10
                    {
                        mesh.indices.extend_from_slice(&[
                            base,
                            base + i as u32,
                            base + i as u32 + 1,
                        ]);
                    }
                }
                for v in &mut poly {
                    v.uv = Vec2::new(
                        overlay.u[0] + v.uv.x * (overlay.u[1] - overlay.u[0]),
                        overlay.v[0] + v.uv.y * (overlay.v[1] - overlay.v[0]),
                    );
                }
                mesh.vertices.extend(poly);
            }
        }
        Ok(mesh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_record_decodes_basis_face_order_and_uv_bounds_and_rejects_overrun() {
        let mut bytes = vec![0; RECORD_SIZE];
        bytes[6..8].copy_from_slice(&(2u16 | (3 << 14)).to_le_bytes());
        bytes[8..12].copy_from_slice(&17i32.to_le_bytes());
        bytes[12..16].copy_from_slice(&23i32.to_le_bytes());
        for (offset, value) in [
            (268, 2f32),
            (272, 1f32),
            (288, 1f32),
            (348, 1f32),
            (324, 1f32),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let o = parse(&bytes, &[]).unwrap().remove(0);
        assert_eq!(o.faces, [17, 23]);
        assert_eq!(o.render_order, 3);
        assert_eq!(o.basis_u, Vec3::X);
        assert_eq!(o.basis_v, -Vec3::Y);
        assert_eq!(o.u, [0., 2.]);
        assert_eq!(o.v, [1., 0.]);
        bytes[6..8].copy_from_slice(&65u16.to_le_bytes());
        assert!(parse(&bytes, &[]).is_err());
        assert!(parse(&bytes[..351], &[]).is_err());
    }

    #[test]
    fn clipping_preserves_receiver_slope_luxels_and_subrect_projection() {
        let make = |x, y| OverlayVertex {
            position: Vec3::new(x, y, x + y),
            normal: Vec3::Z,
            uv: Vec2::new(x, y),
            luxel: Vec2::new(x * 4., y * 7.),
            face: 9,
        };
        let poly = clip(vec![make(-1., -1.), make(3., -1.), make(-1., 3.)]);
        assert_eq!(poly.len(), 4);
        for v in poly {
            assert!((0. ..=1.).contains(&v.uv.x) && (0. ..=1.).contains(&v.uv.y));
            assert!((v.position.z - v.position.x - v.position.y).abs() < 1e-5);
            assert!((v.luxel - Vec2::new(v.uv.x * 4., v.uv.y * 7.)).length() < 1e-5);
        }
        let o = Overlay {
            id: 0,
            texinfo: 0,
            render_order: 0,
            faces: vec![],
            u: [0.25, 0.75],
            v: [1., 0.],
            corners: [
                Vec2::new(-2., -1.),
                Vec2::new(-1., 1.),
                Vec2::new(3., 1.),
                Vec2::new(2., -1.),
            ],
            origin: Vec3::ZERO,
            normal: Vec3::Z,
            basis_u: Vec3::X,
            basis_v: Vec3::Y,
            fade_squared: [-1., 0.],
        };
        assert!((o.project(Vec3::new(0.5, 0., 20.)).unwrap() - Vec2::splat(0.5)).length() < 1e-5);
        let mut bad = o;
        bad.corners[2].x += 1.;
        assert_eq!(bad.project(Vec3::ZERO), Err(OverlayError::NonAffineQuad));
    }

    #[test]
    fn displacement_overlay_keeps_the_deformed_surface_and_grid_lightmap() {
        use crate::{DispInfo, DispVert, Face, Plane, TexInfo};
        let mut bsp = Bsp::default();
        bsp.vertices = vec![
            Vec3::ZERO,
            Vec3::Y * 2.,
            Vec3::new(2., 2., 0.),
            Vec3::X * 2.,
        ];
        bsp.edges = vec![[0, 1], [1, 2], [2, 3], [3, 0]];
        bsp.surfedges = vec![0, 1, 2, 3];
        bsp.planes = vec![Plane {
            normal: Vec3::Z,
            dist: 0.,
            kind: 2,
        }];
        bsp.texinfo = vec![TexInfo {
            texture_vecs: [glam::Vec4::X, glam::Vec4::Y],
            lightmap_vecs: [glam::Vec4::X * 100., glam::Vec4::Y * 100.],
            flags: 0,
            texdata: 0,
        }];
        bsp.faces = vec![Face {
            plane: 0,
            side: 0,
            first_edge: 0,
            num_edges: 4,
            texinfo: 0,
            dispinfo: 0,
            styles: [255; 4],
            light_offset: -1,
            lightmap_mins: [0; 2],
            lightmap_size: [8, 12],
            num_prims: 0,
            first_prim_id: 0,
        }];
        bsp.dispinfos = vec![DispInfo {
            start_position: Vec3::ZERO,
            disp_vert_start: 0,
            disp_tri_start: 0,
            power: 2,
            flags: 0,
            contents: 1,
            map_face: 0,
        }];
        bsp.disp_verts = vec![
            DispVert {
                vec: Vec3::Z,
                dist: 0.,
                alpha: 0.
            };
            25
        ];
        bsp.disp_verts[12].dist = 0.5;
        let overlay = Overlay {
            id: 0,
            texinfo: 0,
            render_order: 0,
            faces: vec![0],
            u: [0.25, 0.75],
            v: [1., 0.],
            corners: [Vec2::ZERO, Vec2::Y * 2., Vec2::splat(2.), Vec2::X * 2.],
            origin: Vec3::ZERO,
            normal: Vec3::Z,
            basis_u: Vec3::X,
            basis_v: Vec3::Y,
            fade_squared: [-1., 0.],
        };
        let mesh = bsp.overlay_mesh(&overlay, &|_| true).unwrap();
        assert_eq!(mesh.indices.len(), 32 * 3);
        assert!(mesh.vertices.iter().any(|v| v.position.z == 0.5));
        for v in &mesh.vertices {
            assert!((v.luxel - Vec2::new(v.position.x * 4., v.position.y * 6.)).length() < 1e-5);
            assert!(
                (v.uv - Vec2::new(0.25 + v.position.x * 0.25, 1. - v.position.y * 0.5)).length()
                    < 1e-5
            );
        }
        // Bad surfedges must fail closed before displacement's unchecked path.
        bsp.edges[0][0] = 99;
        assert_eq!(
            bsp.overlay_mesh(&overlay, &|_| true).unwrap_err(),
            OverlayError::InvalidReceiver
        );
    }
}

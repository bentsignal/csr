//! Compiled face tessellation from BSP lumps 37/39.
//!
//! This supports the face-local index form measured in CS:GO Legacy: a
//! primitive with zero dedicated vertices indexes the face's surfedge-order
//! vertices. Dedicated PrimVerts are deliberately unsupported until their
//! index semantics are established; invalid explicit topology never falls
//! back to a fan that could cover a hole or lose a T-junction.

use crate::{Bsp, Face, lump};
use glam::Vec3;
use std::{fmt, io};

/// Ten-byte little-endian record (one padding byte follows `kind`).
#[derive(Clone, Copy, Debug)]
pub struct Primitive {
    /// 0: triangle list; 1: triangle strip.
    pub kind: u8,
    pub first_index: u16,
    pub index_count: u16,
    pub first_vertex: u16,
    pub vertex_count: u16,
}

pub(crate) fn parse(bytes: &[u8]) -> io::Result<Vec<Primitive>> {
    if bytes.len() % 10 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "partial BSP primitive record",
        ));
    }
    Ok(lump::records(bytes, 10, |r| Primitive {
        kind: r.u8(0),
        first_index: r.u16(2),
        index_count: r.u16(4),
        first_vertex: r.u16(6),
        vertex_count: r.u16(8),
    }))
}

pub(crate) fn parse_indices(bytes: &[u8]) -> io::Result<Vec<u16>> {
    if bytes.len() % 2 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "partial BSP primitive index",
        ));
    }
    Ok(lump::records(bytes, 2, |r| r.u16(0)))
}

#[derive(Debug)]
pub struct FaceMesh {
    pub positions: Vec<Vec3>,
    /// Triangle list, counter-clockwise about the stored face normal.
    pub indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceMeshError {
    InvalidFaceVertices,
    InvalidNormal,
    PrimitiveRange,
    IndexRange,
    InvalidVertexIndex,
    UnsupportedDedicatedVertices,
    UnsupportedTopology,
    InvalidIndexCount,
    IndexBudget,
    NonFiniteTriangle,
    /// Validated triangles have zero area and the stored boundary exactly
    /// retraces itself. Compiler residue, with no enclosed surface to render.
    RetracedBoundary,
    EmptyTopology,
}

impl fmt::Display for FaceMeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FaceMeshError {}

// Bound duplicate/overlapping primitive ranges as well as individual u16
// counts. A strip can produce at most three output indices per input index.
const MAX_FACE_PRIMITIVE_INDICES: usize = 65_536;

impl Bsp {
    /// Planar render topology. Displacements retain their own tessellation.
    pub fn face_mesh(&self, face: &Face) -> Result<FaceMesh, FaceMeshError> {
        use FaceMeshError::*;
        let first = usize::try_from(face.first_edge).map_err(|_| InvalidFaceVertices)?;
        let count = usize::try_from(face.num_edges).map_err(|_| InvalidFaceVertices)?;
        if count < 3 {
            return Err(InvalidFaceVertices);
        }
        let edges = self
            .surfedges
            .get(first..first.checked_add(count).ok_or(InvalidFaceVertices)?)
            .ok_or(InvalidFaceVertices)?;
        let positions: Vec<Vec3> = edges
            .iter()
            .map(|&se| {
                let index = se.checked_abs().ok_or(InvalidFaceVertices)? as usize;
                let edge = self.edges.get(index).ok_or(InvalidFaceVertices)?;
                let pos = *self
                    .vertices
                    .get(usize::from(edge[usize::from(se < 0)]))
                    .ok_or(InvalidFaceVertices)?;
                if !pos.is_finite() {
                    return Err(InvalidFaceVertices);
                }
                Ok(pos)
            })
            .collect::<Result<_, _>>()?;
        let normal = self.face_normal(face).ok_or(InvalidNormal)?;
        if !normal.is_finite() || normal.length_squared() == 0.0 {
            return Err(InvalidNormal);
        }
        let mut indices = Vec::new();
        let mut emit = |mut tri: [usize; 3], orient: bool| -> Result<(), FaceMeshError> {
            let cross = (positions[tri[1]] - positions[tri[0]])
                .cross(positions[tri[2]] - positions[tri[0]]);
            let facing = cross.dot(normal);
            if !cross.is_finite() || !facing.is_finite() {
                return Err(NonFiniteTriangle);
            }
            // Strip connectors and collinear fan triangles carry no area.
            if cross.length_squared() == 0.0 {
                return Ok(());
            }
            if orient && facing < 0.0 {
                tri.swap(1, 2);
            }
            indices.extend(tri.map(|i| i as u32));
            Ok(())
        };
        if face.primitive_count() == 0 {
            // Preserve the previous fan's whole-polygon winding. Primitive
            // support does not invent a new triangulation for other faces.
            let mut newell = Vec3::ZERO;
            for i in 0..positions.len() {
                let (a, b) = (positions[i], positions[(i + 1) % positions.len()]);
                newell += Vec3::new(
                    (a.y - b.y) * (a.z + b.z),
                    (a.z - b.z) * (a.x + b.x),
                    (a.x - b.x) * (a.y + b.y),
                );
            }
            for i in 1..positions.len() - 1 {
                let tri = if newell.dot(normal) >= 0.0 {
                    [0, i, i + 1]
                } else {
                    [0, i + 1, i]
                };
                emit(tri, false)?;
            }
        } else {
            let first = usize::from(face.first_prim_id);
            let primitives = self
                .primitives
                .get(
                    first
                        ..first
                            .checked_add(face.primitive_count())
                            .ok_or(PrimitiveRange)?,
                )
                .ok_or(PrimitiveRange)?;
            let mut consumed = 0usize;
            for primitive in primitives {
                if primitive.vertex_count != 0 {
                    return Err(UnsupportedDedicatedVertices);
                }
                let count = usize::from(primitive.index_count);
                consumed = consumed
                    .checked_add(count)
                    .filter(|&n| n <= MAX_FACE_PRIMITIVE_INDICES)
                    .ok_or(IndexBudget)?;
                let first = usize::from(primitive.first_index);
                let input = self
                    .primitive_indices
                    .get(first..first.checked_add(count).ok_or(IndexRange)?)
                    .ok_or(IndexRange)?;
                if input.iter().any(|&i| usize::from(i) >= positions.len()) {
                    return Err(InvalidVertexIndex);
                }
                match primitive.kind {
                    0 => {
                        if count < 3 || count % 3 != 0 {
                            return Err(InvalidIndexCount);
                        }
                        for tri in input.chunks_exact(3) {
                            emit([tri[0] as usize, tri[1] as usize, tri[2] as usize], true)?;
                        }
                    }
                    1 => {
                        if count < 3 {
                            return Err(InvalidIndexCount);
                        }
                        for (i, tri) in input.windows(3).enumerate() {
                            let mut tri = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                            if i % 2 != 0 {
                                tri.swap(0, 1);
                            }
                            emit(tri, true)?;
                        }
                    }
                    _ => return Err(UnsupportedTopology),
                }
            }
        }
        if indices.is_empty() {
            if face.primitive_count() != 0 && retraced_boundary(&positions) {
                return Err(RetracedBoundary);
            }
            return Err(EmptyTopology);
        }
        Ok(FaceMesh { positions, indices })
    }
}

/// Recognize only exact out-and-back boundaries measured in Legacy archives.
/// Signed polygon area or a tolerance would also accept bow ties/thin surfaces.
/// Every nonzero edge must have its exact reverse; topology/range validation
/// and zero triangle areas have already succeeded before this is consulted.
fn retraced_boundary(positions: &[Vec3]) -> bool {
    use std::collections::BTreeMap;
    let key = |v: Vec3| v.to_array().map(|f| if f == 0. { 0 } else { f.to_bits() });
    let mut edges = BTreeMap::<([u32; 3], [u32; 3]), i32>::new();
    for i in 0..positions.len() {
        let (a, b) = (key(positions[i]), key(positions[(i + 1) % positions.len()]));
        if a == b { continue; }
        let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
        *edges.entry(edge).or_default() += sign;
    }
    !edges.is_empty() && edges.values().all(|balance| *balance == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Plane;

    fn fixture(points: &[[f32; 2]]) -> (Bsp, Face) {
        let mut bsp = Bsp::default();
        bsp.planes.push(Plane {
            normal: Vec3::Z,
            dist: 0.0,
            kind: 2,
        });
        bsp.vertices = points.iter().map(|p| Vec3::new(p[0], p[1], 0.0)).collect();
        // Alternate signed surfedges to prove indices use the face's ordered
        // vertices, not edge IDs or global vertex IDs.
        bsp.edges.push([0, 0]);
        for i in 0..points.len() {
            let next = ((i + 1) % points.len()) as u16;
            let id = bsp.edges.len() as i32;
            if i % 2 == 0 {
                bsp.edges.push([i as u16, next]);
                bsp.surfedges.push(id);
            } else {
                bsp.edges.push([next, i as u16]);
                bsp.surfedges.push(-id);
            }
        }
        let face = Face {
            plane: 0,
            side: 0,
            first_edge: 0,
            num_edges: points.len() as i16,
            texinfo: 0,
            dispinfo: -1,
            styles: [255; 4],
            light_offset: -1,
            lightmap_mins: [0; 2],
            lightmap_size: [0; 2],
            num_prims: 1,
            first_prim_id: 0,
        };
        (bsp, face)
    }
    fn primitive(kind: u8, first_index: u16, index_count: u16) -> Primitive {
        Primitive {
            kind,
            first_index,
            index_count,
            first_vertex: 0,
            vertex_count: 0,
        }
    }
    fn square() -> (Bsp, Face) {
        let (mut bsp, face) = fixture(&[[0., 0.], [2., 0.], [2., 2.], [0., 2.]]);
        bsp.primitives.push(primitive(0, 0, 6));
        bsp.primitive_indices = vec![0, 1, 2, 0, 2, 3];
        (bsp, face)
    }
    fn area(mesh: &FaceMesh) -> f32 {
        mesh.indices
            .chunks_exact(3)
            .map(|t| {
                let [a, b, c] = [
                    mesh.positions[t[0] as usize],
                    mesh.positions[t[1] as usize],
                    mesh.positions[t[2] as usize],
                ];
                let cross = (b - a).cross(c - a);
                assert!(cross.z > 0.0, "nondegenerate CCW triangles required");
                cross.z * 0.5
            })
            .sum()
    }

    #[test]
    fn compiled_concave_topology_preserves_the_notch() {
        let (mut bsp, face) =
            fixture(&[[3., 0.], [3., 1.], [1., 1.], [1., 3.], [0., 3.], [0., 0.]]);
        bsp.primitives.push(primitive(0, 0, 12));
        bsp.primitive_indices = vec![0, 1, 2, 0, 2, 5, 2, 3, 4, 2, 4, 5];
        let mesh = bsp.face_mesh(&face).unwrap();
        assert_eq!(area(&mesh), 5.0);
        let p = Vec3::new(2., 2., 0.);
        assert!(
            mesh.indices.chunks_exact(3).all(|t| {
                let v = t
                    .iter()
                    .map(|&i| mesh.positions[i as usize])
                    .collect::<Vec<_>>();
                (0..3).any(|i| (v[(i + 1) % 3] - v[i]).cross(p - v[i]).z < 0.0)
            }),
            "the missing corner must remain uncovered"
        );
    }

    #[test]
    fn compiled_t_junction_keeps_both_boundary_segments() {
        let (mut bsp, face) = fixture(&[[0., 0.], [1., 0.], [2., 0.], [2., 2.], [0., 2.]]);
        bsp.primitives.push(primitive(0, 0, 9));
        bsp.primitive_indices = vec![0, 1, 4, 1, 3, 4, 1, 2, 3];
        let mesh = bsp.face_mesh(&face).unwrap();
        assert_eq!(area(&mesh), 4.0);
        let has_edge = |a, b| {
            mesh.indices.chunks_exact(3).any(|t| {
                (0..3).any(|i| {
                    (t[i] == a && t[(i + 1) % 3] == b) || (t[i] == b && t[(i + 1) % 3] == a)
                })
            })
        };
        assert!(has_edge(0, 1) && has_edge(1, 2));
        assert_eq!(mesh.indices.len(), 9);
    }

    #[test]
    fn strip_alternation_and_degenerate_connectors_preserve_winding() {
        let (mut bsp, face) = square();
        bsp.primitives[0] = primitive(1, 0, 8);
        bsp.primitive_indices = vec![0, 1, 3, 2, 2, 2, 0, 1];
        let mesh = bsp.face_mesh(&face).unwrap();
        assert_eq!(mesh.indices, vec![0, 1, 3, 3, 1, 2, 0, 1, 2]);
        assert_eq!(area(&mesh), 6.0); // Two square triangles plus a final restarted triangle.
    }

    #[test]
    fn multiple_primitives_and_shadow_bit_use_masked_face_count() {
        let (mut bsp, mut face) = square();
        bsp.primitives = vec![primitive(99, 0, 0), primitive(0, 0, 3), primitive(0, 3, 3)];
        face.num_prims = 0x8002;
        face.first_prim_id = 1;
        assert_eq!(face.primitive_count(), 2);
        assert_eq!(area(&bsp.face_mesh(&face).unwrap()), 4.0);
        // A shadow bit alone is not a primitive reference.
        face.num_prims = 0x8000;
        face.first_prim_id = u16::MAX;
        assert_eq!(area(&bsp.face_mesh(&face).unwrap()), 4.0);
    }

    #[test]
    fn malformed_explicit_topology_never_falls_back_to_a_fan() {
        let (bsp, face) = square();
        for (primitive, expected) in [
            (primitive(0, 0, 5), FaceMeshError::InvalidIndexCount),
            (primitive(1, 0, 2), FaceMeshError::InvalidIndexCount),
            (primitive(9, 0, 6), FaceMeshError::UnsupportedTopology),
            (primitive(0, u16::MAX, 6), FaceMeshError::IndexRange),
        ] {
            let mut b = square().0;
            b.primitives[0] = primitive;
            assert_eq!(b.face_mesh(&face).unwrap_err(), expected);
        }
        let mut bad = face;
        bad.first_prim_id = 1;
        assert_eq!(
            bsp.face_mesh(&bad).unwrap_err(),
            FaceMeshError::PrimitiveRange
        );
        let mut b = square().0;
        b.primitive_indices[5] = 4;
        assert_eq!(
            b.face_mesh(&face).unwrap_err(),
            FaceMeshError::InvalidVertexIndex
        );
        b = square().0;
        b.primitives[0].vertex_count = 1;
        assert_eq!(
            b.face_mesh(&face).unwrap_err(),
            FaceMeshError::UnsupportedDedicatedVertices
        );
        b = square().0;
        b.primitive_indices.fill(0);
        assert_eq!(
            b.face_mesh(&face).unwrap_err(),
            FaceMeshError::EmptyTopology
        );
    }

    #[test]
    fn exact_retraced_compiler_boundaries_are_distinct_from_missing_surface_topology() {
        // Synthetic bent spike: a triangle fan would incorrectly create a
        // surface even though the compiled triangles and boundary enclose none.
        let (mut bsp, face) = fixture(&[[0., 0.], [1., 0.01], [2., 0.], [1., 0.01]]);
        bsp.primitives.push(primitive(0, 0, 6));
        bsp.primitive_indices = vec![1, 2, 3, 1, 3, 0];
        assert_eq!(bsp.face_mesh(&face).unwrap_err(), FaceMeshError::RetracedBoundary);

        // Near reversal still encloses a thin surface and must remain an error.
        bsp.vertices[3].y += 0.0001;
        bsp.primitive_indices.fill(0);
        assert_eq!(bsp.face_mesh(&face).unwrap_err(), FaceMeshError::EmptyTopology);
        let (mut square, face) = square();
        square.primitive_indices.fill(0);
        assert_eq!(square.face_mesh(&face).unwrap_err(), FaceMeshError::EmptyTopology);

        // Bad ranges are rejected even on a known retraced boundary.
        let (mut bsp, face) = fixture(&[[0., 0.], [1., 0.], [2., 0.], [1., 0.]]);
        bsp.primitives.push(primitive(0, 0, 6));
        bsp.primitive_indices = vec![1, 2, 4, 1, 3, 0];
        assert_eq!(bsp.face_mesh(&face).unwrap_err(), FaceMeshError::InvalidVertexIndex);
    }

    #[test]
    fn overlapping_ranges_cannot_expand_without_a_per_face_budget() {
        let (mut bsp, mut face) = square();
        bsp.primitives = vec![primitive(0, 0, 6); 10_923];
        face.num_prims = 10_923;
        assert_eq!(
            bsp.face_mesh(&face).unwrap_err(),
            FaceMeshError::IndexBudget
        );
    }

    #[test]
    fn malformed_surfedges_vertices_and_normals_fail_without_panics() {
        let (_, face) = square();
        for change in 0..6 {
            let mut b = square().0;
            match change {
                0 => b.surfedges[0] = i32::MIN,
                1 => b.edges[1][0] = u16::MAX,
                2 => b.surfedges.clear(),
                3 => b.vertices[0].x = f32::NAN,
                4 => b.planes[0].normal = Vec3::ZERO,
                _ => b.planes.clear(),
            }
            assert!(b.face_mesh(&face).is_err());
        }
        let b = square().0;
        let mut bad = face;
        bad.first_edge = -1;
        assert_eq!(
            b.face_mesh(&bad).unwrap_err(),
            FaceMeshError::InvalidFaceVertices
        );
        bad = face;
        bad.num_edges = -1;
        assert_eq!(
            b.face_mesh(&bad).unwrap_err(),
            FaceMeshError::InvalidFaceVertices
        );
    }

    #[test]
    fn binary_records_are_exact_and_partial_records_are_rejected() {
        let bytes = [1, 0, 7, 0, 4, 0, 9, 0, 0, 0];
        let p = parse(&bytes).unwrap()[0];
        assert_eq!(
            (
                p.kind,
                p.first_index,
                p.index_count,
                p.first_vertex,
                p.vertex_count
            ),
            (1, 7, 4, 9, 0)
        );
        assert!(parse(&bytes[..9]).is_err());
        assert_eq!(
            parse_indices(&[0xff, 0xff, 3, 0]).unwrap(),
            vec![u16::MAX, 3]
        );
        assert!(parse_indices(&[1]).is_err());
    }

    #[test]
    fn bsp_parser_reads_face_metadata_and_rejects_partial_primitive_lumps() {
        fn file(primitive_bytes: &[u8]) -> Vec<u8> {
            let mut bytes = vec![0; 1036];
            bytes[..4].copy_from_slice(b"VBSP");
            bytes[4..8].copy_from_slice(&21i32.to_le_bytes());
            let mut face = [0; 56];
            face[48..50].copy_from_slice(&0x8001u16.to_le_bytes());
            face[50..52].copy_from_slice(&7u16.to_le_bytes());
            for (index, data) in [
                (crate::lumps::FACES, face.as_slice()),
                (crate::lumps::PRIMITIVES, primitive_bytes),
            ] {
                let offset = bytes.len() as i32;
                bytes[8 + index * 16..12 + index * 16].copy_from_slice(&offset.to_le_bytes());
                bytes[12 + index * 16..16 + index * 16]
                    .copy_from_slice(&(data.len() as i32).to_le_bytes());
                bytes.extend_from_slice(data);
            }
            bytes
        }
        let bsp = Bsp::parse(&file(&[0; 10])).unwrap();
        assert_eq!(bsp.faces[0].num_prims, 0x8001);
        assert_eq!(bsp.faces[0].primitive_count(), 1);
        assert_eq!(bsp.faces[0].first_prim_id, 7);
        assert_eq!(bsp.primitives.len(), 1);
        assert_eq!(
            Bsp::parse(&file(&[0; 9])).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}

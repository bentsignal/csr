//! Read-only bridge from the live collision definitions, including convex
//! brushes/prop PHY hulls. Clip authored planes rather than using their AABBs.
use collision::{CollisionWorld, Convex, contents};
#[cfg(test)]
use glam::Vec3;
use mechanics_link::{Breakable, Door, Scene, Triangle};
fn convex(solid: &Convex) -> Vec<Triangle> {
    if solid.contents & contents::MASK_PLAYERSOLID == 0 {
        return vec![];
    }
    let mut out = vec![];
    for (face_index, face) in solid.planes.iter().enumerate() {
        if solid.planes[..face_index]
            .iter()
            .any(|p| p.normal.dot(face.normal) > 0.999999 && (p.dist - face.dist).abs() < 0.001)
        {
            continue;
        }
        let n = face.normal;
        let center = (solid.mins + solid.maxs) * 0.5;
        let center = center + n * (face.dist - n.dot(center));
        let u = n.any_orthonormal_vector();
        let v = n.cross(u);
        let size = (solid.maxs - solid.mins).length().max(1.) * 2.;
        let mut polygon = vec![
            center - u * size - v * size,
            center + u * size - v * size,
            center + u * size + v * size,
            center - u * size + v * size,
        ];
        for plane in &solid.planes {
            let mut clipped = vec![];
            for i in 0..polygon.len() {
                let a = polygon[i];
                let b = polygon[(i + 1) % polygon.len()];
                let da = plane.normal.dot(a) - plane.dist;
                let db = plane.normal.dot(b) - plane.dist;
                if da <= 0.001 {
                    clipped.push(a);
                }
                if (da < -0.001 && db > 0.001) || (da > 0.001 && db < -0.001) {
                    clipped.push(a + (b - a) * (da / (da - db)));
                }
            }
            polygon = clipped;
            if polygon.len() < 3 {
                break;
            }
        }
        for i in 1..polygon.len().saturating_sub(1) {
            let points = [polygon[0], polygon[i], polygon[i + 1]];
            if (points[1] - points[0])
                .cross(points[2] - points[0])
                .length_squared()
                > 1e-5
            {
                out.push(Triangle {
                    points: points.map(|p| p.to_array()),
                    surface: solid.surface,
                });
            }
        }
    }
    out
}
pub fn scene(world: &CollisionWorld) -> Scene {
    let map = world.static_world();
    let mut triangles: Vec<_> = map.convexes.iter().flat_map(convex).collect();
    triangles.extend(
        map.triangles
            .iter()
            .filter(|t| t.contents & contents::MASK_PLAYERSOLID != 0)
            .map(|t| Triangle {
                points: t.v.map(|p| p.to_array()),
                surface: t.surface,
            }),
    );
    let doors = map
        .doors
        .iter()
        .map(|d| {
            let solids = d.solids(0.);
            Door {
                origin: d.origin.to_array(),
                triangles: solids.iter().flat_map(convex).collect(),
                parts: solids.iter().map(points).collect(),
            }
        })
        .collect();
    let mut breakables: Vec<_> = map
        .breakables
        .iter()
        .enumerate()
        .map(|(i, b)| Breakable {
            killed_by: vec![i],
            triangles: b.solids.iter().flat_map(convex).collect(),
            parts: b.solids.iter().map(points).collect(),
        })
        .collect();
    breakables.extend(map.breakable_dependencies.iter().map(|b| Breakable {
        killed_by: b.killed_by.clone(),
        triangles: b.solids.iter().flat_map(convex).collect(),
        parts: b.solids.iter().map(points).collect(),
    }));
    let rails = rails::extract(map, &triangles);
    Scene {
        triangles,
        doors,
        breakables,
        rails,
    }
}
mod rails;
fn points(solid: &Convex) -> Vec<[f32; 3]> {
    let mut seen = std::collections::BTreeSet::new();
    convex(solid)
        .into_iter()
        .flat_map(|t| t.points)
        .filter(|p| seen.insert(p.map(f32::to_bits)))
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn convex_faces_keep_winding_bounds_and_real_slopes() {
        let mut world = CollisionWorld::new();
        let points = vec![
            Vec3::new(-2., -3., 0.),
            Vec3::new(2., -3., 0.),
            Vec3::new(2., 3., 0.),
            Vec3::new(-2., 3., 0.),
            Vec3::new(-2., -3., 4.),
            Vec3::new(2., -3., 4.),
        ];
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2],
            [0, 1, 5],
            [0, 5, 4],
            [0, 4, 3],
            [1, 2, 5],
            [3, 4, 5],
            [3, 5, 2],
        ];
        let solid = collision::convex_from_hull(&points, &tris, contents::SOLID, 0).unwrap();
        world.add_convex(solid.clone());
        let result = scene(&world);
        assert!(!result.triangles.is_empty());
        for t in result.triangles {
            let p = t.points.map(Vec3::from_array);
            let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
            assert!(solid.planes.iter().any(|plane| plane.normal.dot(n) > 0.999
                && (plane.normal.dot(p[0]) - plane.dist).abs() < 0.01));
            for p in p {
                assert!(
                    solid
                        .planes
                        .iter()
                        .all(|plane| plane.normal.dot(p) - plane.dist < 0.01)
                );
            }
        }
    }
}

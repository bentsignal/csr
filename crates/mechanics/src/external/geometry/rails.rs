//! CS has no authored Skate splines. Expose upper convex edges as host-authored
//! paths, then let the original grind selector/solver own every riding result.
use collision::{CollisionWorld, contents};
use glam::Vec3;
use mechanics_link::Triangle;
use std::collections::{BTreeMap, HashMap};
type Key = [i64; 3];
fn key(p: Vec3) -> Key {
    p.to_array()
        .map(|v| ((v * 0.0254) as f64 / f64::from(0.001_f32)).round() as i64)
}
struct Edge {
    points: [Vec3; 2],
    top: Option<Vec3>,
    side: Option<Vec3>,
}
pub(super) fn extract(world: &CollisionWorld, triangles: &[Triangle]) -> Vec<Vec<[f32; 3]>> {
    let mut edges: HashMap<[Key; 2], Edge> = HashMap::new();
    for triangle in triangles {
        let p = triangle.points.map(Vec3::from_array);
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        let top = n.z > 0.6;
        let side = n.z.abs() < 0.35;
        if !top && !side {
            continue;
        }
        for i in 0..3 {
            let mut points = [p[i], p[(i + 1) % 3]];
            if points[0].distance_squared(points[1]) < 16. * 16. {
                continue;
            }
            let mut keys = points.map(key);
            if keys[0] > keys[1] {
                keys.swap(0, 1);
                points.swap(0, 1);
            }
            let entry = edges.entry(keys).or_insert(Edge {
                points,
                top: None,
                side: None,
            });
            if top {
                entry.top = Some(n);
            }
            if side {
                entry.side = Some(n);
            }
        }
    }
    // Sorting makes selection order identical after process restart/replay.
    let mut edges: Vec<_> = edges
        .into_iter()
        .filter_map(|(key, edge)| {
            let (Some(top), Some(side)) = (edge.top, edge.side) else {
                return None;
            };
            let sample = (edge.points[0] + edge.points[1]) * 0.5 + top * 2. + side * 2.;
            (world.point_contents(sample, contents::MASK_PLAYERSOLID) == 0)
                .then_some((key, edge.points))
        })
        .collect();
    edges.sort_by_key(|e| e.0);
    let mut adjacency: BTreeMap<Key, Vec<(usize, usize)>> = BTreeMap::new();
    for (i, (keys, _)) in edges.iter().enumerate() {
        for (end, key) in keys.iter().enumerate() {
            adjacency.entry(*key).or_default().push((i, end));
        }
    }
    let mut used = vec![false; edges.len()];
    let mut paths = Vec::new();
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let mut path = edges[first].1.to_vec();
        for reverse in [false, true] {
            if reverse {
                path.reverse();
            }
            loop {
                let end = *path.last().unwrap();
                let direction = (end - path[path.len() - 2]).normalize_or_zero();
                let candidates: Vec<_> = adjacency
                    .get(&key(end))
                    .into_iter()
                    .flatten()
                    .filter(|(i, _)| !used[*i])
                    .filter(|(i, side)| {
                        direction.dot((edges[*i].1[1 - *side] - end).normalize_or_zero()) > 0.95
                    })
                    .copied()
                    .collect();
                if candidates.len() != 1 {
                    break;
                }
                let (i, side) = candidates[0];
                used[i] = true;
                path.push(edges[i].1[1 - side]);
            }
        }
        paths.push(path.into_iter().map(|p| p.to_array()).collect());
    }
    paths
}

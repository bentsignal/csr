//! A triangle world must expose the *union* of static brushes. Unlike Source
//! hull sweeps, Skate contacts every triangle: buried brush ends become walls
//! and the floor under a ramp becomes a second, conflicting contact surface.
use super::{convex, Convex, Triangle};
use glam::Vec3;
use std::collections::{BTreeSet, HashMap};

const CELL: f32 = 512.;
const EPS: f32 = 0.002;
fn cells(min: Vec3, max: Vec3) -> impl Iterator<Item = (i32, i32)> {
    let lo = (min / CELL).floor().as_ivec3();
    let hi = (max / CELL).floor().as_ivec3();
    (lo.x..=hi.x).flat_map(move |x| (lo.y..=hi.y).map(move |y| (x, y)))
}
fn clip(poly: &[Vec3], normal: Vec3, dist: f32, inside: bool) -> Vec<Vec3> {
    let mut out = vec![];
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let da = normal.dot(a) - dist;
        let db = normal.dot(b) - dist;
        let keep = |d: f32| if inside { d <= 0. } else { d >= 0. };
        if keep(da) { out.push(a); }
        if keep(da) != keep(db) { out.push(a + (b - a) * (da / (da - db))); }
    }
    out
}
pub(super) fn exterior(solids: &[Convex]) -> Vec<Triangle> {
    let mut grid: HashMap<_, Vec<usize>> = HashMap::new();
    for (i, s) in solids.iter().enumerate() {
        if s.contents & collision::contents::MASK_PLAYERSOLID == 0 { continue; }
        for cell in cells(s.mins - Vec3::splat(EPS), s.maxs + Vec3::splat(EPS)) {
            grid.entry(cell).or_default().push(i);
        }
    }
    let mut out = vec![];
    for (i, s) in solids.iter().enumerate() {
        for t in convex(s) {
            let points = t.points.map(Vec3::from_array);
            let n = (points[1] - points[0]).cross(points[2] - points[0]).normalize();
            let min = points.into_iter().fold(Vec3::splat(f32::INFINITY), Vec3::min);
            let max = points.into_iter().fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
            let candidates: BTreeSet<_> = cells(min - Vec3::splat(EPS), max + Vec3::splat(EPS))
                .flat_map(|cell| grid.get(&cell).into_iter().flatten().copied())
                .filter(|&j| j != i && solids[j].maxs.cmpge(min - Vec3::splat(EPS)).all()
                    && solids[j].mins.cmple(max + Vec3::splat(EPS)).all()).collect();
            let mut fragments = vec![points.to_vec()];
            for j in candidates {
                let other = &solids[j];
                // Coincident outward faces belong to the first brush only.
                if j > i && other.planes.iter().any(|p| p.normal.dot(n) > 0.999999
                    && (p.dist - n.dot(points[0])).abs() < EPS) { continue; }
                let mut next = vec![];
                for mut fragment in fragments {
                    // Probe just outside this face. Keep the actual vertices
                    // on their authored surface so adjacent ramp facets weld.
                    if other.planes.iter().any(|p| fragment.iter().all(|v|
                        p.normal.dot(*v + n * EPS * 4.) - p.dist > EPS)) {
                        next.push(fragment); continue;
                    }
                    for p in &other.planes {
                        let outside = clip(&fragment, p.normal, p.dist, false);
                        if outside.len() >= 3 && fragment.iter().any(|v| p.normal.dot(*v) - p.dist > EPS) {
                            next.push(outside);
                        }
                        fragment = clip(&fragment, p.normal, p.dist + EPS * 0.01, true);
                        if fragment.len() < 3 { break; }
                    }
                    // The remaining polygon lies inside the other brush.
                }
                fragments = next;
                if fragments.is_empty() { break; }
            }
            for poly in fragments {
                for k in 1..poly.len().saturating_sub(1) {
                    let p = [poly[0], poly[k], poly[k + 1]];
                    if (p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 1e-5 {
                        out.push(Triangle { points: p.map(|v| v.to_array()), surface: t.surface });
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn box_at(min: Vec3, max: Vec3) -> Convex {
        Convex {
            mins: min, maxs: max, contents: collision::contents::SOLID, surface: 0, plane_surfaces: vec![],
            planes: [(Vec3::X,max.x),(Vec3::NEG_X,-min.x),(Vec3::Y,max.y),(Vec3::NEG_Y,-min.y),
                (Vec3::Z,max.z),(Vec3::NEG_Z,-min.z)].into_iter().map(|(normal,dist)| collision::Plane{normal,dist}).collect(),
        }
    }
    #[test]
    fn touching_sections_have_no_internal_wall_and_overlapping_floor_is_cut() {
        let floor = box_at(Vec3::new(-100., -20., -10.), Vec3::new(100., 20., 0.));
        let a = box_at(Vec3::new(-10., -10., 0.), Vec3::new(0., 10., 10.));
        let b = box_at(Vec3::new(0., -10., 0.), Vec3::new(10., 10., 10.));
        let mesh = exterior(&[floor, a, b]);
        let mut area = 0.;
        for tri in mesh {
            let p = tri.points.map(Vec3::from_array);
            let cross = (p[1] - p[0]).cross(p[2] - p[0]);
            let center = (p[0] + p[1] + p[2]) / 3.;
            assert!(!(center.x.abs() < 0.01 && cross.x.abs() > 0.01), "internal join: {tri:?}");
            assert!(!(center.z.abs() < 0.01 && center.x.abs() < 9.99 && center.y.abs() < 9.99), "buried floor: {tri:?}");
            area += cross.length() * 0.5;
        }
        // Exterior of a 200x40x10 foundation plus a 20x20x10 cap.
        assert!((area - (2. * (8000. + 2000. + 400.) + 800.)).abs() < 1., "area={area}");
    }
}

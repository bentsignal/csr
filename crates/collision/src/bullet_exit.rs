//! Conservative exact exits for connected convex cover. No sampled stepping,
//! inferred thickness for triangles, or changes to ordinary movement traces.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BulletSurface {
    pub material: u8,
    pub penetration_modifier: f32,
    pub damage_modifier: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn slab(low: f32, high: f32) -> Convex {
        Convex { mins: Vec3::new(low, -20., -20.), maxs: Vec3::new(high, 20., 20.),
            contents: contents::SOLID, surface: 1, plane_surfaces: vec![2, 1, 1, 1, 1, 1],
            planes: vec![Plane { normal: Vec3::X, dist: high }, Plane { normal: -Vec3::X, dist: -low },
                Plane { normal: Vec3::Y, dist: 20. }, Plane { normal: -Vec3::Y, dist: 20. },
                Plane { normal: Vec3::Z, dist: 20. }, Plane { normal: -Vec3::Z, dist: 20. }] }
    }

    fn exit(world: &CollisionWorld, start: Vec3, end: Vec3) -> Option<BulletExit> {
        let hit = world.trace_ray(start, end, contents::MASK_SHOT);
        world.bullet_exit(start, end, &hit, 64.)
    }

    #[test]
    fn physical_thickness_exit_face_and_oblique_path() {
        let mut world = CollisionWorld::new();
        world.add_convex(slab(20., 28.));
        world.finalize();
        let direct = exit(&world, Vec3::ZERO, Vec3::X * 100.).unwrap();
        assert_eq!(direct.position, Vec3::X * 28.);
        assert_eq!(direct.thickness, 8.);
        assert_eq!(direct.surface, 2);
        assert_eq!(direct.normal, Vec3::X);
        assert!(direct.next_start.x > direct.position.x);
        assert!(!world.trace_ray(direct.next_start, Vec3::X * 100., contents::MASK_SHOT).hit());
        let angled = exit(&world, Vec3::new(0., -10., 0.), Vec3::new(100., 40., 0.)).unwrap();
        assert!((angled.thickness - 8. * 1.25f32.sqrt()).abs() < 0.001);
    }

    #[test]
    fn connected_intervals_keep_hidden_faces_and_count_union_once() {
        let mut world = CollisionWorld::new();
        // Out of order, with one wholly hidden convex. Overlap must not be
        // double-counted; an internal surface must not be lost either.
        world.add_convex(slab(28., 36.));
        world.add_convex(slab(20., 28.));
        let mut hidden = slab(22., 24.);
        hidden.plane_surfaces[0] = 9;
        world.add_convex(hidden);
        world.add_convex(slab(26., 30.));
        world.finalize();
        let direct = exit(&world, Vec3::ZERO, Vec3::X * 100.).unwrap();
        assert_eq!(direct.position, Vec3::X * 36.);
        assert_eq!(direct.thickness, 16.);
        assert_eq!(direct.crossed_surfaces.len(), 8);
        assert!(direct.crossed_surfaces.contains(&9));
        assert!(!world.trace_ray(direct.next_start, Vec3::X * 100., contents::MASK_SHOT).hit());
        let hit = world.trace_ray(Vec3::ZERO, Vec3::X * 100., contents::MASK_SHOT);
        assert!(world.bullet_exit(Vec3::ZERO, Vec3::X * 100., &hit, 12.).is_none());
        let angled = exit(&world, Vec3::new(0., -10., 0.), Vec3::new(100., 40., 0.)).unwrap();
        assert!((angled.thickness - 16. * 1.25f32.sqrt()).abs() < 0.001);
    }

    #[test]
    fn ties_triangle_startsolid_range_and_clearance_fail_closed() {
        for case in 0..9 {
            let mut world = CollisionWorld::new();
            world.add_convex(slab(20., 28.));
            match case {
                0 => { world.add_convex(slab(20., 28.)); }
                1 => world.add_triangle([Vec3::new(26., -10., -10.), Vec3::new(26., 10., -10.), Vec3::new(26., 0., 10.)], contents::SOLID, 1),
                5 => { world.add_convex(slab(28.02, 40.)); }
                6 => {
                    let mut tied_entry = slab(20., 32.);
                    tied_entry.plane_surfaces[1] = 9;
                    world.add_convex(tied_entry);
                }
                7 => { world.add_convex(slab(24., 28.)); }
                8 => world.add_triangle([Vec3::new(28.02, -10., -10.), Vec3::new(28.02, 10., -10.), Vec3::new(28.02, 0., 10.)], contents::SOLID, 1),
                _ => {}
            }
            world.finalize();
            let start = if case == 2 { Vec3::X * 24. } else { Vec3::ZERO };
            let end = if case == 3 { Vec3::X * 26. } else { Vec3::X * 100. };
            if case == 4 {
                let hit = world.trace_ray(start, end, contents::MASK_SHOT);
                assert!(world.bullet_exit(start, end, &hit, 4.).is_none());
            } else { assert!(exit(&world, start, end).is_none(), "case {case}"); }
        }
    }

    #[test]
    fn runtime_overlay_keeps_base_definitions_and_checks_static_backing() {
        let mut base = CollisionWorld::new();
        base.bullet_surfaces = vec![None, Some(BulletSurface { material: b'W', penetration_modifier: 0.9, damage_modifier: 0.6 })];
        base.add_convex(slab(24., 40.));
        base.finalize();
        let world = CollisionWorld { base: Some(Arc::new(base)), convexes: vec![slab(20., 28.)],
            runtime_owners: vec![Some(HitOwner::Door(3))], ..Default::default() };
        assert_eq!(world.bullet_surface(1).unwrap().material, b'W');
        let (hit, owner) = world.trace_owned_ray(Vec3::ZERO, Vec3::X * 100., contents::MASK_SHOT);
        assert_eq!(owner, Some(HitOwner::Door(3)));
        assert_eq!(hit.surface, 1);
        let connected = world.bullet_exit(Vec3::ZERO, Vec3::X * 100., &hit, 64.).unwrap();
        assert_eq!(connected.thickness, 20.);
        assert_eq!(connected.crossed_surfaces, vec![1, 2, 1, 2]);
        let mut base = CollisionWorld::new();
        base.finalize();
        let world = CollisionWorld { base: Some(Arc::new(base)), convexes: vec![slab(20., 28.)], ..Default::default() };
        assert_eq!(exit(&world, Vec3::ZERO, Vec3::X * 100.).unwrap().thickness, 8.);
    }
}

#[derive(Clone, Debug)]
pub struct BulletExit {
    pub position: Vec3,
    pub next_start: Vec3,
    pub normal: Vec3,
    pub surface: u32,
    pub thickness: f32,
    /// Entry/exit faces of EVERY convex in the connected ray interval, even
    /// solids wholly inside another. Callers must validate all of these;
    /// validating only the visible first/last faces can hide opaque backing.
    pub crossed_surfaces: Vec<u32>,
}

impl CollisionWorld {
    pub fn bullet_surface(&self, surface: u32) -> Option<BulletSurface> {
        if let Some(base) = &self.base { return base.bullet_surface(surface); }
        self.bullet_surfaces.get(surface as usize).copied().flatten()
    }

    /// Find the exact exit of the touching/overlapping convex ray intervals
    /// connected to the unique convex that produced `hit` on this same ray.
    /// Material eligibility belongs to the caller and must cover every ID in
    /// `crossed_surfaces`. Ambiguous ties, triangles, unknown backends,
    /// start-solid rays and exits beyond the finite budget fail closed.
    /// `start`/`end` must be the endpoints used to obtain `hit`.
    pub fn bullet_exit(&self, start: Vec3, end: Vec3, hit: &Trace, max_thickness: f32) -> Option<BulletExit> {
        if !hit.hit() || hit.start_solid || hit.all_solid || self.dynamic.is_some()
            || !max_thickness.is_finite() || !(0.0..=64.0).contains(&max_thickness) { return None; }
        let direction = (end - start).try_normalize()?;
        let remaining = (end - hit.end).length();
        let corridor_end = hit.end + direction * (max_thickness + 1.0).min(remaining);
        let low = hit.end.min(corridor_end) - Vec3::ONE;
        let high = hit.end.max(corridor_end) + Vec3::ONE;
        let mut convexes = Vec::new();
        let mut triangles = Vec::new();
        self.bullet_candidates(low, high, &mut convexes, &mut triangles)?;
        let mut winner = None;
        for (index, solid) in convexes.iter().enumerate() {
            let mut candidate = Trace::empty(end);
            clip_convex_surfaces(&solid.planes, start, end, Vec3::ZERO,
                solid.contents, solid.surface, &solid.plane_surfaces, &mut candidate);
            // Exact clipper result, not a proximity/owner guess. Multiple
            // indistinguishable winners are unsupported rather than skipped.
            if candidate.fraction == hit.fraction && candidate.normal == hit.normal
                && candidate.plane_dist == hit.plane_dist && candidate.surface == hit.surface
                && candidate.contents == hit.contents && !candidate.start_solid {
                if winner.replace(index).is_some() { return None; }
            }
        }
        let index = winner?;
        let mut intervals = Vec::new();
        for (other, solid) in convexes.iter().enumerate() {
            if let Some(interval) = ray_interval(solid, start, direction)? {
                intervals.push((other, interval));
            }
        }
        let entry = intervals.iter().find(|(other, _)| *other == index)?.1;
        if entry.enter < 0.0 || entry.ambiguous { return None; }
        intervals.sort_by(|a, b| a.1.enter.total_cmp(&b.1.enter));
        let mut leave = entry.leave;
        let mut last = index;
        let mut exit_face = entry.exit_face;
        let mut members = vec![false; convexes.len()];
        let mut crossed_surfaces = Vec::new();
        for (other, interval) in intervals {
            if interval.leave < entry.enter { continue; }
            // Do not silently substitute a different physical entry for the
            // exact trace winner, including an earlier grazing interval.
            if interval.enter < entry.enter { return None; }
            if interval.enter > leave { break; }
            if interval.ambiguous || (other != index
                && (interval.enter == entry.enter || interval.leave == leave)) {
                return None;
            }
            members[other] = true;
            let solid = convexes[other];
            crossed_surfaces.extend([face_surface(solid, interval.entry_face),
                face_surface(solid, interval.exit_face)]);
            if interval.leave > leave {
                leave = interval.leave;
                last = other;
                exit_face = interval.exit_face;
            }
        }
        let thickness = leave - entry.enter;
        if thickness <= 0.0 || thickness > max_thickness { return None; }
        let solid = convexes[last];
        let plane = solid.planes[exit_face];
        let clearance = 2.0 * DIST_EPSILON / plane.normal.dot(direction);
        // Very shallow exits need too much numerical clearance for this seam.
        if clearance > 0.25 || leave + clearance >= (end - start).length() { return None; }
        let next_start = start + direction * (leave + clearance);
        for (other, solid) in convexes.iter().enumerate() {
            let mut tr = Trace::empty(next_start);
            // Members must all have been left. Other solids, including ones
            // separated by a tiny air gap, must not be skipped by clearance.
            let check_start = if members[other] { next_start } else { hit.end };
            clip_convex_surfaces(&solid.planes, check_start, next_start, Vec3::ZERO,
                solid.contents, solid.surface, &solid.plane_surfaces, &mut tr);
            if tr.hit() { return None; }
        }
        for triangle in triangles {
            let mut planes = Vec::new();
            triangle_planes(triangle, &mut planes);
            let mut tr = Trace::empty(next_start);
            clip_convex(&planes, hit.end, next_start, Vec3::ZERO, triangle.contents, triangle.surface, &mut tr);
            if tr.hit() { return None; }
        }
        Some(BulletExit {
            position: start + direction * leave, next_start, normal: plane.normal,
            surface: face_surface(solid, exit_face),
            thickness, crossed_surfaces,
        })
    }

    fn bullet_candidates<'a>(&'a self, low: Vec3, high: Vec3,
        convexes: &mut Vec<&'a Convex>, triangles: &mut Vec<&'a Triangle>) -> Option<()> {
        if self.dynamic.is_some() { return None; }
        if let Some(base) = &self.base {
            base.bullet_candidates(low, high, convexes, triangles)?;
            for solid in &self.convexes {
                if solid.contents & contents::MASK_SHOT != 0 && overlaps(low, high, solid.mins, solid.maxs) {
                    convexes.push(solid);
                }
            }
        } else {
            let mut shapes = Vec::new();
            self.candidates(low, high, &mut shapes);
            for shape in shapes {
                match shape {
                    Shape::Convex(i) => {
                        let solid = &self.convexes[i as usize];
                        if solid.contents & contents::MASK_SHOT != 0 && overlaps(low, high, solid.mins, solid.maxs) { convexes.push(solid); }
                    }
                    Shape::Triangle(i) => {
                        let solid = &self.triangles[i as usize];
                        if solid.contents & contents::MASK_SHOT != 0 && overlaps(low, high, solid.mins, solid.maxs) { triangles.push(solid); }
                    }
                }
            }
        }
        (convexes.len() + triangles.len() <= 256).then_some(())
    }
}

fn face_surface(solid: &Convex, face: usize) -> u32 {
    solid.plane_surfaces.get(face).copied().filter(|id| *id != u32::MAX).unwrap_or(solid.surface)
}

#[derive(Clone, Copy)]
struct RayInterval {
    enter: f32,
    leave: f32,
    entry_face: usize,
    exit_face: usize,
    ambiguous: bool,
}

/// Physical distances along the ray, without the trace clipper's contact
/// epsilon. A miss is distinct from invalid/ambiguous geometry.
fn ray_interval(solid: &Convex, start: Vec3, direction: Vec3) -> Option<Option<RayInterval>> {
    let mut enter = f32::NEG_INFINITY;
    let mut leave = f32::INFINITY;
    let mut entry_face = None;
    let mut exit_face = None;
    let mut entry_tie = false;
    let mut exit_tie = false;
    for (face, plane) in solid.planes.iter().enumerate() {
        let speed = plane.normal.dot(direction);
        let distance = plane.dist - plane.normal.dot(start);
        if !speed.is_finite() || !distance.is_finite() { return None; }
        if speed == 0.0 {
            if distance < 0.0 { return Some(None); }
        } else {
            let t = distance / speed;
            if !t.is_finite() { return None; }
            if speed < 0.0 {
                if t > enter {
                    enter = t; entry_face = Some(face); entry_tie = false;
                } else if t == enter {
                    entry_tie |= face_surface(solid, face) != face_surface(solid, entry_face?);
                }
            } else if t < leave {
                leave = t; exit_face = Some(face); exit_tie = false;
            } else if t == leave {
                exit_tie |= face_surface(solid, face) != face_surface(solid, exit_face?);
            }
        }
    }
    if enter >= leave || leave < 0.0 { return Some(None); }
    Some(Some(RayInterval { enter, leave, entry_face: entry_face?, exit_face: exit_face?,
        ambiguous: entry_tie || exit_tie }))
}

//! Conservative exact exits for connected convex cover. No sampled stepping,
//! inferred thickness for triangles, or changes to ordinary movement traces.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BulletSurface {
    pub material: u8,
    pub penetration_modifier: f32,
    pub damage_modifier: f32,
}

/// Owned clean-room snapshot of the BSP world model's spatial partition.
/// A malformed supplied tree is distinct from no classifier: it blocks exits.
pub(super) struct BspSolidSpace {
    tree: Option<SolidSpaceTree>,
}

struct SolidSpaceTree {
    head_node: i32,
    nodes: Vec<bsp::Node>,
    planes: Vec<bsp::Plane>,
    leaf_contents: Vec<i32>,
}

const MAX_SOLID_SPACE_RECORDS: usize = 1_048_576;
const MAX_SOLID_SPACE_DEPTH: usize = 1024;

impl BspSolidSpace {
    pub(super) fn from_bsp(map: &bsp::Bsp) -> Self {
        Self { tree: Self::validated_tree(map) }
    }

    fn validated_tree(map: &bsp::Bsp) -> Option<SolidSpaceTree> {
        if [map.nodes.len(), map.planes.len(), map.leafs.len()].into_iter()
            .any(|len| len > MAX_SOLID_SPACE_RECORDS) { return None; }
        let head_node = map.models.first()?.head_node;
        // Iterative postorder validates reachable indices/cycles and computes
        // subtree heights, including shared subtrees reached at other depths.
        let mut state = vec![0u8; map.nodes.len()];
        let mut height = vec![0usize; map.nodes.len()];
        let mut stack = vec![(head_node, false)];
        while let Some((index, returning)) = stack.pop() {
            if index < 0 {
                // BSP child -1 is leaf 0, -2 is leaf 1, etc. Bitwise
                // complement also handles i32::MIN without signed overflow.
                map.leafs.get((!index) as usize)?;
                continue;
            }
            let index = index as usize;
            let node = map.nodes.get(index)?;
            if returning {
                height[index] = 1 + node.children.iter().map(|&child|
                    if child < 0 { 0 } else { height[child as usize] }).max()?;
                if height[index] > MAX_SOLID_SPACE_DEPTH { return None; }
                state[index] = 2;
                continue;
            }
            match state[index] {
                1 => return None,
                2 => continue,
                _ => {}
            }
            let plane = map.planes.get(usize::try_from(node.plane).ok()?)?;
            let length = plane.normal.length_squared();
            if !plane.normal.is_finite() || !plane.dist.is_finite()
                || !length.is_finite() || length <= 0.0 { return None; }
            state[index] = 1;
            stack.push((index as i32, true));
            stack.extend(node.children.map(|child| (child, false)));
        }
        Some(SolidSpaceTree {
            head_node, nodes: map.nodes.clone(), planes: map.planes.clone(),
            leaf_contents: map.leafs.iter().map(|leaf| leaf.contents).collect(),
        })
    }

    fn is_nonsolid(&self, point: Vec3) -> bool {
        let Some(tree) = &self.tree else { return false; };
        if !point.is_finite() { return false; }
        let mut index = tree.head_node;
        for _ in 0..=MAX_SOLID_SPACE_DEPTH {
            if index < 0 {
                return tree.leaf_contents.get((!index) as usize)
                    .is_some_and(|value| value & contents::SOLID == 0);
            }
            let Some(node) = tree.nodes.get(index as usize) else { return false; };
            let Some(plane) = tree.planes.get(node.plane as usize) else { return false; };
            let distance = plane.normal.dot(point) - plane.dist;
            if !distance.is_finite() { return false; }
            index = node.children[usize::from(distance < 0.0)];
        }
        false
    }
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

    fn partition(sealed: bool) -> bsp::Bsp {
        let node = |plane, children| bsp::Node { plane, children,
            mins: [-100; 3], maxs: [100; 3], first_face: 0, num_faces: 0 };
        let leaf = |contents| bsp::Leaf { contents, cluster: -1, area: 0,
            mins: [-100; 3], maxs: [100; 3], first_leaf_face: 0,
            num_leaf_faces: 0, first_leaf_brush: 0, num_leaf_brushes: 0 };
        bsp::Bsp {
            planes: vec![bsp::Plane { normal: Vec3::X, dist: 28., kind: 0 },
                bsp::Plane { normal: Vec3::X, dist: 20., kind: 0 }],
            // Deliberately use a nonzero world headnode; node 0 describes
            // unrelated all-solid space and must never be used for this world.
            nodes: vec![node(0, [-2, -2]), node(0, [if sealed { -2 } else { -1 }, 2]),
                node(1, [-2, -1])],
            leafs: vec![leaf(contents::WATER), leaf(contents::SOLID)],
            models: vec![bsp::Model { head_node: 1, mins: Vec3::splat(-100.),
                maxs: Vec3::splat(100.), origin: Vec3::ZERO, first_face: 0, num_faces: 0 }],
            ..Default::default()
        }
    }

    #[test]
    fn sealed_void_blocks_but_actual_thin_cover_and_synthetic_world_exit() {
        let mut world = CollisionWorld::new();
        world.add_convex(slab(20., 28.));
        world.finalize();
        let start = Vec3::ZERO;
        let end = Vec3::X * 100.;
        let synthetic = exit(&world, start, end).unwrap();
        for sealed in [false, true] {
            let map = partition(sealed);
            world.bullet_solid_space = Some(Arc::new(BspSolidSpace::from_bsp(&map)));
            drop(map); // No borrowed map data or filesystem lookup per pellet.
            assert!(world.bullet_exit_space_is_clear(start));
            assert!(!world.bullet_exit_space_is_clear(Vec3::X * 24.));
            assert_eq!(world.bullet_exit_space_is_clear(synthetic.next_start), !sealed);
            assert_eq!(world.bullet_exit_space_is_clear(Vec3::X * 28.), !sealed);
            let result = exit(&world, start, end);
            assert_eq!(result.is_some(), !sealed);
            if let Some(result) = result {
                assert_eq!(result.thickness, synthetic.thickness);
                assert_eq!(result.next_start, synthetic.next_start);
                assert_eq!(result.crossed_surfaces, synthetic.crossed_surfaces);
            }
            // The BSP classifier must not change brush contact or movement.
            let trace = world.trace_ray(start, end, contents::MASK_SHOT);
            assert_eq!(trace.normal, -Vec3::X);
            assert!((trace.end - Vec3::X * (20. - DIST_EPSILON)).length() < 1e-5);
            assert_eq!(world.point_contents(synthetic.next_start, contents::MASK_SHOT), 0);
            assert!(!world.trace_ray(synthetic.next_start, end, contents::MASK_SHOT).hit());
            let hull = world.trace_box(start, end, Vec3::splat(-1.), Vec3::ONE, contents::MASK_PLAYERSOLID);
            assert!((hull.end - Vec3::X * (19. - DIST_EPSILON)).length() < 1e-5);
        }
    }

    #[test]
    fn runtime_overlay_exit_checks_immutable_base_leaf_contents() {
        for sealed in [false, true] {
            let map = partition(sealed);
            let mut base = CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &|_| None);
            assert!(base.bullet_solid_space.is_some());
            base.finalize();
            let overlay = CollisionWorld { base: Some(Arc::new(base)),
                convexes: vec![slab(20., 28.)], ..Default::default() };
            let nested = CollisionWorld { base: Some(Arc::new(overlay)), ..Default::default() };
            assert_eq!(exit(&nested, Vec3::ZERO, Vec3::X * 100.).is_some(), !sealed);
        }
    }

    #[test]
    fn malformed_supplied_partitions_fail_closed_with_bounded_traversal() {
        for case in 0..12 {
            let mut map = partition(false);
            match case {
                0 => map.models.clear(),
                1 => map.models[0].head_node = i32::MAX,
                2 => map.models[0].head_node = i32::MIN,
                3 => map.nodes[1].children[0] = 3,
                4 => map.nodes[1].children[0] = i32::MIN,
                5 => map.nodes[2].children[1] = 1,
                6 => map.nodes[1].plane = -1,
                7 => map.nodes[1].plane = 2,
                8 => map.planes[0].dist = f32::NAN,
                9 => map.planes[0].normal = Vec3::splat(f32::INFINITY),
                10 => map.planes[0].normal = Vec3::ZERO,
                11 => map.planes[0].normal = Vec3::splat(f32::MAX),
                _ => unreachable!(),
            }
            let space = BspSolidSpace::from_bsp(&map);
            assert!(space.tree.is_none(), "case {case}");
            let mut world = CollisionWorld::new();
            world.add_convex(slab(20., 28.));
            world.finalize();
            world.bullet_solid_space = Some(Arc::new(space));
            assert!(exit(&world, Vec3::ZERO, Vec3::X * 100.).is_none(), "case {case}");
        }
        let mut map = partition(false);
        map.models[0].head_node = -1;
        let space = BspSolidSpace::from_bsp(&map);
        assert!(space.is_nonsolid(Vec3::ZERO));
        assert!(!space.is_nonsolid(Vec3::splat(f32::NAN)));
        map.models[0].head_node = -2;
        assert!(!BspSolidSpace::from_bsp(&map).is_nonsolid(Vec3::ZERO));
        // Acyclic but over-deep input is rejected before any pellet query.
        map.models[0].head_node = 0;
        map.nodes = (0..=MAX_SOLID_SPACE_DEPTH).map(|i| bsp::Node {
            plane: 0, children: [-1, if i == MAX_SOLID_SPACE_DEPTH { -1 } else { (i + 1) as i32 }],
            mins: [-100; 3], maxs: [100; 3], first_face: 0, num_faces: 0,
        }).collect();
        assert!(BspSolidSpace::from_bsp(&map).tree.is_none());
        map.nodes.pop();
        map.nodes.last_mut().unwrap().children = [-1, -1];
        let space = BspSolidSpace::from_bsp(&map);
        assert!(space.tree.is_some());
        assert!(space.is_nonsolid(Vec3::ZERO));
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
        // Leaving the last convex does not prove that there is playable space
        // behind it: sealed BSP solid leaves can have no containing brush.
        if !self.bullet_exit_space_is_clear(next_start) { return None; }
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

    fn bullet_exit_space_is_clear(&self, point: Vec3) -> bool {
        if let Some(base) = &self.base { return base.bullet_exit_space_is_clear(point); }
        self.bullet_solid_space.as_ref().is_none_or(|space| space.is_nonsolid(point))
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

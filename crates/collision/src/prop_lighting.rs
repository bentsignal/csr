//! Temporary broad phase for load-time static-prop lighting, not gameplay.
//!
//! The grid duplicates large shapes across XY cells and long rays repeatedly
//! gather/sort those entries. A 3D BVH stores each solid shape once. Retaining
//! the original segment and shape order is deliberate: equal-distance hits,
//! start-solid contents, and early all-solid termination are order-sensitive.

use super::*;

const LEAF_SHAPES: usize = 8;

struct Node {
    mins: Vec3,
    maxs: Vec3,
    shapes: std::ops::Range<usize>,
    /// First node beyond this subtree; preorder traversal needs no stack.
    escape: usize,
}

/// Prepared exact static-prop lighting queries, borrowing an immutable map.
/// Storage is linear in shape count and reused for every ray. No geometry or
/// triangle planes are duplicated. Forks share immutable acceleration and own
/// independent scratch. This object is released after prop baking.
pub struct PropLightingQuery<'a> {
    world: LightingWorld<'a>,
    shapes: std::sync::Arc<Vec<Shape>>,
    nodes: std::sync::Arc<Vec<Node>>,
    seen: Vec<u32>,
    generation: u32,
    candidates: Vec<usize>,
    planes: Vec<Plane>,
}

#[derive(Clone)]
enum LightingWorld<'a> {
    Borrowed(&'a CollisionWorld),
    Owned(std::sync::Arc<CollisionWorld>),
}

impl std::ops::Deref for LightingWorld<'_> {
    type Target = CollisionWorld;
    fn deref(&self) -> &CollisionWorld {
        match self { Self::Borrowed(world) => world, Self::Owned(world) => world }
    }
}

fn key(shape: Shape) -> (u8, u32) {
    match shape {
        Shape::Convex(i) => (0, i),
        Shape::Triangle(i) => (1, i),
    }
}

fn bounds(world: &CollisionWorld, shape: Shape) -> (Vec3, Vec3) {
    match shape {
        Shape::Convex(i) => {
            let c = &world.convexes[i as usize];
            (c.mins, c.maxs)
        }
        Shape::Triangle(i) => {
            let t = &world.triangles[i as usize];
            (t.mins, t.maxs)
        }
    }
}

fn node_count(n: usize) -> usize {
    if n <= LEAF_SHAPES {
        1
    } else {
        1 + node_count(n / 2) + node_count(n - n / 2)
    }
}

fn build(world: &CollisionWorld, shapes: &mut [Shape], first: usize, nodes: &mut Vec<Node>) {
    let mut mins = Vec3::splat(f32::MAX);
    let mut maxs = Vec3::splat(f32::MIN);
    for &shape in shapes.iter() {
        let (low, high) = bounds(world, shape);
        mins = mins.min(low);
        maxs = maxs.max(high);
    }
    let index = nodes.len();
    nodes.push(Node {
        mins,
        maxs,
        shapes: first..first + shapes.len(),
        escape: 0,
    });
    if shapes.len() > LEAF_SHAPES {
        let size = maxs - mins;
        let axis = if size.x >= size.y && size.x >= size.z {
            0
        } else if size.y >= size.z {
            1
        } else {
            2
        };
        let middle = shapes.len() / 2;
        shapes.select_nth_unstable_by(middle, |&a, &b| {
            let (a0, a1) = bounds(world, a);
            let (b0, b1) = bounds(world, b);
            (a0[axis] + a1[axis])
                .total_cmp(&(b0[axis] + b1[axis]))
                .then_with(|| key(a).cmp(&key(b)))
        });
        let (left, right) = shapes.split_at_mut(middle);
        build(world, left, first, nodes);
        build(world, right, first + middle, nodes);
    }
    nodes[index].escape = nodes.len();
}

impl<'a> PropLightingQuery<'a> {
    pub(super) fn new(world: &'a CollisionWorld) -> Self {
        Self::with_world(LightingWorld::Borrowed(world))
    }

    /// Retain the immutable map and exact query scratch across loading frames.
    pub fn owned(world: std::sync::Arc<CollisionWorld>) -> PropLightingQuery<'static> {
        PropLightingQuery::with_world(LightingWorld::Owned(world))
    }

    /// Independent scratch for scoped parallel lighting of the same map.
    pub fn fork(&self) -> Self {
        Self {
            world: self.world.clone(), shapes: self.shapes.clone(), nodes: self.nodes.clone(),
            seen: vec![0; self.shapes.len()], generation: 0,
            candidates: Vec::with_capacity(self.shapes.len()),
            planes: Vec::with_capacity(26),
        }
    }

    fn with_world(world: LightingWorld<'a>) -> Self {
        // Dynamic worlds keep their existing backend and exclusion behavior.
        let static_map = world.dynamic.is_none() && world.grid.dims[0] != 0;
        let count = if static_map {
            world
                .convexes
                .iter()
                .filter(|c| c.contents & contents::MASK_SOLID != 0)
                .count()
                + world
                    .triangles
                    .iter()
                    .filter(|t| t.contents & contents::MASK_SOLID != 0)
                    .count()
        } else {
            0
        };
        let mut shapes = Vec::with_capacity(count);
        if static_map {
            shapes.extend(
                world
                    .convexes
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.contents & contents::MASK_SOLID != 0)
                    .map(|(i, _)| Shape::Convex(i as u32)),
            );
            shapes.extend(
                world
                    .triangles
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.contents & contents::MASK_SOLID != 0)
                    .map(|(i, _)| Shape::Triangle(i as u32)),
            );
        }
        let mut nodes = Vec::with_capacity(if shapes.is_empty() {
            0
        } else {
            node_count(shapes.len())
        });
        if !shapes.is_empty() {
            build(&world, &mut shapes, 0, &mut nodes);
        }
        Self {
            world,
            seen: vec![0; shapes.len()],
            candidates: Vec::with_capacity(shapes.len()),
            shapes: std::sync::Arc::new(shapes),
            nodes: std::sync::Arc::new(nodes),
            generation: 0,
            // triangle_planes emits at most 2 + 2 * (3 + 3 * 3) planes.
            planes: Vec::with_capacity(26),
        }
    }

    pub fn is_sky(&self, surface: u32) -> bool {
        self.world.is_sky(surface)
    }

    /// Same mask, owner exclusion and exact clipper as
    /// `CollisionWorld::trace_prop_lighting_ray`, with reusable query storage.
    pub fn trace_prop_lighting_ray(
        &mut self,
        start: Vec3,
        end: Vec3,
        prop_ordinal: usize,
    ) -> Trace {
        if self.world.dynamic.is_some() || !start.is_finite() || !end.is_finite() {
            return self.world.trace_prop_lighting_ray(start, end, prop_ordinal);
        }
        if self.nodes.is_empty() {
            return Trace::empty(end);
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen.fill(0);
            self.generation = 1;
        }
        let excluded = self.world.static_prop_convexes.get(prop_ordinal);
        let mut tr = Trace::empty(end);
        // Match trace_box_excluding's zero-offset point sweep arithmetic too.
        let s = start + Vec3::ZERO;
        let e = end + Vec3::ZERO;
        let segments = (((e - s).length() / (GRID_CELL * 2.0)).ceil() as usize).clamp(1, 1024);
        for seg in 0..segments {
            let f0 = seg as f32 / segments as f32;
            let f1 = (seg + 1) as f32 / segments as f32;
            let a = s.lerp(e, f0);
            let b = s.lerp(e, f1);
            let low = a.min(b) - Vec3::ONE;
            let high = a.max(b) + Vec3::ONE;
            self.candidates.clear();
            let mut node_index = 0;
            while let Some(node) = self.nodes.get(node_index) {
                if !overlaps(node.mins, node.maxs, low, high) {
                    node_index = node.escape;
                } else if node.shapes.len() > LEAF_SHAPES {
                    node_index += 1;
                } else {
                    for index in node.shapes.clone() {
                        if self.seen[index] == self.generation {
                            continue;
                        }
                        let shape = self.shapes[index];
                        if let Shape::Convex(i) = shape {
                            if excluded.is_some_and(|range| range.contains(&(i as usize))) {
                                continue;
                            }
                        }
                        let (mins, maxs) = bounds(&self.world, shape);
                        if overlaps(mins, maxs, low, high) {
                            self.seen[index] = self.generation;
                            self.candidates.push(index);
                        }
                    }
                    node_index = node.escape;
                }
            }
            self.candidates
                .sort_unstable_by_key(|&i| key(self.shapes[i]));
            for &index in &self.candidates {
                match self.shapes[index] {
                    Shape::Convex(i) => {
                        let c = &self.world.convexes[i as usize];
                        clip_convex(&c.planes, s, e, Vec3::ZERO, c.contents, c.surface, &mut tr);
                    }
                    Shape::Triangle(i) => {
                        let t = &self.world.triangles[i as usize];
                        triangle_planes(t, &mut self.planes);
                        clip_convex(
                            &self.planes,
                            s,
                            e,
                            Vec3::ZERO,
                            t.contents,
                            t.surface,
                            &mut tr,
                        );
                    }
                }
                if tr.all_solid {
                    break;
                }
            }
            if tr.all_solid || tr.fraction <= f1 {
                break;
            }
        }
        tr.end = if tr.all_solid {
            start
        } else if tr.fraction < 1.0 {
            start + (end - start) * tr.fraction
        } else {
            end
        };
        tr
    }
}

#[cfg(test)]
#[path = "prop_lighting_tests.rs"]
mod tests;

#[cfg(test)]
mod ownership_tests {
    use super::*;

    #[test]
    fn owned_forks_keep_the_same_map_and_acceleration_with_independent_ray_scratch() {
        let mut world = CollisionWorld::new();
        let surface = world.surface_id("synthetic");
        assert!(world.add_convex(Convex {
            planes: vec![Plane { normal: Vec3::Z, dist: 4. }, Plane { normal: -Vec3::Z, dist: 4. }],
            mins: Vec3::splat(-4.), maxs: Vec3::splat(4.), contents: contents::SOLID, surface,
        }));
        world.finalize();
        let world = std::sync::Arc::new(world);
        let borrowed = world.prepare_prop_lighting();
        let mut owned = PropLightingQuery::owned(world.clone());
        let mut fork = owned.fork();
        assert!(std::ptr::eq(&*borrowed.world, &*owned.world));
        assert!(std::sync::Arc::ptr_eq(&owned.nodes, &fork.nodes));
        assert!(std::sync::Arc::ptr_eq(&owned.shapes, &fork.shapes));
        tests::exact(&mut owned, Vec3::Z * 8., -Vec3::Z * 8., usize::MAX);
        assert_eq!(fork.generation, 0);
        tests::exact(&mut fork, Vec3::Z * 8., -Vec3::Z * 8., usize::MAX);
        drop(borrowed);
        drop(world);
        drop(owned);
        tests::exact(&mut fork, -Vec3::Z * 8., Vec3::Z * 8., usize::MAX);
    }
}

//! Collision world: swept axis-aligned boxes and rays against convex solids.
//!
//! Every solid is either
//! * a [`Convex`] — a set of bounding planes (map brushes with their bevel
//!   planes, static prop physics hulls), or
//! * a triangle (displacement surfaces), clipped on the fly against the
//!   separating-axis planes of the box/triangle Minkowski sum.
//!
//! Sweeps use the classic plane-clipping algorithm from Quake-lineage
//! engines: each plane is pushed out by the box's support distance, and the
//! latest entry / earliest exit fractions decide the hit. All shapes live in
//! a uniform XY grid for broad-phase culling.
//!
//! Units and axes are Source's (inches, Z up).

use glam::{Mat3, Vec3};

pub use bsp::contents;

mod prop_lighting;
pub use prop_lighting::PropLightingQuery;
mod ladders;
pub mod doors;
pub mod breakables;

/// Distance kept between a sweep's end position and the surface it hit.
pub const DIST_EPSILON: f32 = 0.03125;
const GRID_CELL: f32 = 128.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub normal: Vec3,
    pub dist: f32,
}

/// Result of a sweep.
#[derive(Clone, Copy, Debug)]
pub struct Trace {
    /// 0..1 portion of the move completed.
    pub fraction: f32,
    pub end: Vec3,
    /// Surface normal of what was hit (zero if nothing).
    pub normal: Vec3,
    pub plane_dist: f32,
    /// The start position was inside a solid.
    pub start_solid: bool,
    /// The whole move was inside a solid.
    pub all_solid: bool,
    /// Contents of what was hit.
    pub contents: i32,
    /// Surface id of the hit shape (see [`CollisionWorld::surface_name`]).
    pub surface: u32,
}

impl Trace {
    pub fn empty(end: Vec3) -> Self {
        Self {
            fraction: 1.0,
            end,
            normal: Vec3::ZERO,
            plane_dist: 0.0,
            start_solid: false,
            all_solid: false,
            contents: 0,
            surface: 0,
        }
    }

    pub fn hit(&self) -> bool {
        self.fraction < 1.0 || self.start_solid
    }
}

#[derive(Clone, Debug)]
pub struct Convex {
    pub planes: Vec<Plane>,
    pub mins: Vec3,
    pub maxs: Vec3,
    pub contents: i32,
    pub surface: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub v: [Vec3; 3],
    pub normal: Vec3,
    pub mins: Vec3,
    pub maxs: Vec3,
    pub contents: i32,
    pub surface: u32,
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    Convex(u32),
    Triangle(u32),
}

#[derive(Default)]
struct Grid {
    origin: [f32; 2],
    dims: [usize; 2],
    cells: Vec<Vec<Shape>>,
}

impl Grid {
    fn cell_range(&self, mins: Vec3, maxs: Vec3) -> Option<(usize, usize, usize, usize)> {
        if self.dims[0] == 0 {
            return None;
        }
        let to_cell = |v: f32, axis: usize| ((v - self.origin[axis]) / GRID_CELL).floor();
        let x0 = to_cell(mins.x, 0).max(0.0) as usize;
        let y0 = to_cell(mins.y, 1).max(0.0) as usize;
        let x1 = to_cell(maxs.x, 0);
        let y1 = to_cell(maxs.y, 1);
        if x1 < 0.0 || y1 < 0.0 {
            return None;
        }
        let x1 = (x1 as usize).min(self.dims[0] - 1);
        let y1 = (y1 as usize).min(self.dims[1] - 1);
        if x0 > x1 || y0 > y1 {
            return None;
        }
        Some((x0, y0, x1, y1))
    }
}

// Retain only query storage, never a world, shape reference or live geometry.
// Larger queries still run in full; only their allocations are discarded afterward.
const TRACE_CANDIDATE_CAPACITY_LIMIT: usize = 4096;
const TRACE_SEEN_CAPACITY_LIMIT: usize = 4096;
const TRACE_PLANE_CAPACITY_LIMIT: usize = 32;

struct TraceScratch {
    cands: Vec<Shape>,
    planes: Vec<Plane>,
    seen: std::collections::HashSet<(u8, u32)>,
}

impl Default for TraceScratch {
    fn default() -> Self {
        Self {
            cands: Vec::with_capacity(64),
            planes: Vec::with_capacity(32),
            seen: std::collections::HashSet::new(),
        }
    }
}

impl TraceScratch {
    fn reset(&mut self) {
        if self.cands.capacity() > TRACE_CANDIDATE_CAPACITY_LIMIT {
            self.cands = Vec::new();
        } else {
            self.cands.clear();
        }
        if self.planes.capacity() > TRACE_PLANE_CAPACITY_LIMIT {
            self.planes = Vec::new();
        } else {
            self.planes.clear();
        }
        if self.seen.capacity() > TRACE_SEEN_CAPACITY_LIMIT {
            self.seen = std::collections::HashSet::new();
        } else {
            self.seen.clear();
        }
    }
}

std::thread_local! {
    static TRACE_SCRATCH: std::cell::RefCell<TraceScratch> =
        std::cell::RefCell::new(TraceScratch::default());
}

// Drop also clears copied planes/indices and bounds retained capacity on unwind.
struct TraceScratchReset<'a>(&'a mut TraceScratch);

impl Drop for TraceScratchReset<'_> {
    fn drop(&mut self) { self.0.reset(); }
}

fn with_trace_scratch<R>(trace: impl FnOnce(&mut TraceScratch) -> R) -> R {
    TRACE_SCRATCH.with(|slot| {
        if let Ok(mut scratch) = slot.try_borrow_mut() {
            let reset = TraceScratchReset(&mut scratch);
            trace(&mut *reset.0)
        } else {
            // A nested query must neither alias nor clear the outer query's data.
            trace(&mut TraceScratch::default())
        }
    })
}

/// A live world backend. BSP levels leave this unset.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum Climbable { #[default] None, Ladder, Scaffolding }
pub trait DynamicWorld: Send + Sync {
    fn voxel_scale(&self)->f32 {0.}
    fn climbable(&self,_origin:Vec3,_mins:Vec3,_maxs:Vec3)->Climbable {Climbable::None}
    fn trace_player(&self,start:Vec3,end:Vec3,mins:Vec3,maxs:Vec3,mask:i32,_drop_scaffold:bool)->Trace {
        self.trace_box(start,end,mins,maxs,mask)
    }
    fn hit_target(&self, _start:Vec3, _end:Vec3)->Option<(u64,f32)>{None}
    fn damage_target(&self, _key:u64, _damage:f32, _from:Vec3){}
    /// Exact ray contact, with a fallback for worlds that only need damage.
    fn damage_target_at(&self, key:u64, damage:f32, from:Vec3, _point:Vec3) { self.damage_target(key, damage, from); }
    fn damage_block(&self, _point:Vec3, _normal:Vec3, _damage:f32){}

    fn trace_box(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32) -> Trace;
    fn point_contents(&self, point: Vec3, mask: i32) -> i32;
}

/// Clip a hull against one axis-aligned solid, accumulating the earliest hit.
pub fn clip_bounds(start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, low: Vec3, high: Vec3, contents: i32, surface: u32, tr: &mut Trace) {
    let planes = [Plane{normal:Vec3::X,dist:high.x}, Plane{normal:-Vec3::X,dist:-low.x},
        Plane{normal:Vec3::Y,dist:high.y}, Plane{normal:-Vec3::Y,dist:-low.y},
        Plane{normal:Vec3::Z,dist:high.z}, Plane{normal:-Vec3::Z,dist:-low.z}];
    let offset=(mins+maxs)*0.5;
    clip_convex(&planes,start+offset,end+offset,(maxs-mins)*0.5,contents,surface,tr);
    tr.end=if tr.all_solid {start} else {start.lerp(end,tr.fraction)};
}

/// Map ordinal of the runtime solid that won the ordinary masked clip query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitOwner { Door(usize), Breakable(usize) }

impl HitOwner {
    pub fn breakable(self) -> Option<usize> {
        match self { Self::Breakable(index) => Some(index), Self::Door(_) => None }
    }
}

/// Names for surface ids (materials / surface props), index = surface id.
#[derive(Default)]
pub struct CollisionWorld {
    pub dynamic: Option<std::sync::Arc<dyn DynamicWorld>>,
    pub convexes: Vec<Convex>,
    pub triangles: Vec<Triangle>,
    pub surfaces: Vec<String>,
    /// Authored prop doors, separate from the immutable map solids.
    pub doors: std::sync::Arc<[doors::Door]>,
    pub breakables: std::sync::Arc<[breakables::Breakable]>,
    pub breakable_dependencies: std::sync::Arc<[breakables::Dependency]>,
    /// A session-owned, immutable pose overlay shares the heavy map/grid.
    base: Option<std::sync::Arc<CollisionWorld>>,
    /// Per-overlay convex ownership, in the same order as `convexes`.
    /// Static geometry has no runtime owner.
    runtime_owners: Vec<Option<HitOwner>>,
    grid: Grid,
    /// Convex indices per BSP static-prop ordinal, including empty entries.
    /// Built alongside the placed hulls; world geometry is never in a range.
    static_prop_convexes: Vec<std::ops::Range<usize>>,
    /// Authored ladder brushes only. Keeps ordinary movement queries free of
    /// allocations and avoids scanning world geometry for this small subset.
    ladder_convexes: Vec<usize>,
    pub mins: Vec3,
    pub maxs: Vec3,
    /// Shapes dropped for invalid geometry.
    pub rejected: usize,
}

impl CollisionWorld {
    pub fn new() -> Self {
        Self {
            surfaces: vec![String::new()],
            ..Default::default()
        }
    }

    pub fn surface_id(&mut self, name: &str) -> u32 {
        if let Some(i) = self.surfaces.iter().position(|s| s == name) {
            return i as u32;
        }
        self.surfaces.push(name.to_string());
        (self.surfaces.len() - 1) as u32
    }

    pub fn surface_name(&self, id: u32) -> &str {
        if let Some(base) = &self.base { return base.surface_name(id); }
        self.surfaces.get(id as usize).map_or("", String::as_str)
    }

    /// True for skybox brushes (sunlight and sky traces pass "through" them).
    pub fn is_sky(&self, id: u32) -> bool {
        self.surface_name(id).contains("skybox")
    }

    /// Adds a convex solid. Shapes with non-finite or absurd bounds are
    /// dropped (returns false) so one bad input cannot poison the grid.
    pub fn add_convex(&mut self, c: Convex) -> bool {
        let sane = |v: Vec3| v.is_finite() && v.abs().max_element() < 131_072.0;
        if !sane(c.mins) || !sane(c.maxs) || c.planes.iter().any(|p| !p.normal.is_finite() || !p.dist.is_finite()) {
            self.rejected += 1;
            return false;
        }
        self.convexes.push(c);
        true
    }

    pub fn add_triangle(&mut self, v: [Vec3; 3], contents: i32, surface: u32) {
        let n = (v[1] - v[0]).cross(v[2] - v[0]);
        let len = n.length();
        if len < 1e-6 || !len.is_finite() || v.iter().any(|p| p.abs().max_element() >= 131_072.0) {
            self.rejected += 1;
            return;
        }
        let mins = v[0].min(v[1]).min(v[2]);
        let maxs = v[0].max(v[1]).max(v[2]);
        self.triangles.push(Triangle {
            v,
            normal: n / len,
            mins,
            maxs,
            contents,
            surface,
        });
    }

    /// Builds the broad-phase grid. Call after adding all shapes.
    pub fn finalize(&mut self) {
        self.ladder_convexes = self.convexes.iter().enumerate()
            .filter_map(|(index, brush)| (brush.contents & contents::LADDER != 0).then_some(index))
            .collect();
        let mut mn = Vec3::splat(f32::MAX);
        let mut mx = Vec3::splat(f32::MIN);
        for c in &self.convexes {
            mn = mn.min(c.mins);
            mx = mx.max(c.maxs);
        }
        for t in &self.triangles {
            mn = mn.min(t.mins);
            mx = mx.max(t.maxs);
        }
        if mn.x > mx.x {
            self.grid = Grid::default();
            return;
        }
        self.mins = mn;
        self.maxs = mx;
        let dims = [
            (((mx.x - mn.x) / GRID_CELL).floor() as usize + 1).min(4096),
            (((mx.y - mn.y) / GRID_CELL).floor() as usize + 1).min(4096),
        ];
        let mut grid = Grid {
            origin: [mn.x, mn.y],
            dims,
            cells: vec![Vec::new(); dims[0] * dims[1]],
        };
        let insert = |shape: Shape, mins: Vec3, maxs: Vec3, grid: &mut Grid| {
            if let Some((x0, y0, x1, y1)) = grid.cell_range(mins, maxs) {
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        grid.cells[y * grid.dims[0] + x].push(shape);
                    }
                }
            }
        };
        for (i, c) in self.convexes.iter().enumerate() {
            insert(Shape::Convex(i as u32), c.mins, c.maxs, &mut grid);
        }
        for (i, t) in self.triangles.iter().enumerate() {
            insert(Shape::Triangle(i as u32), t.mins, t.maxs, &mut grid);
        }
        self.grid = grid;
    }

    fn candidates(&self, mins: Vec3, maxs: Vec3, out: &mut Vec<Shape>) {
        out.clear();
        let Some((x0, y0, x1, y1)) = self.grid.cell_range(mins, maxs) else {
            return;
        };
        let single = x0 == x1 && y0 == y1;
        for y in y0..=y1 {
            for x in x0..=x1 {
                out.extend_from_slice(&self.grid.cells[y * self.grid.dims[0] + x]);
            }
        }
        if !single {
            out.sort_unstable_by_key(|s| match *s {
                Shape::Convex(i) => (0u8, i),
                Shape::Triangle(i) => (1u8, i),
            });
            out.dedup_by_key(|s| match *s {
                Shape::Convex(i) => (0u8, i),
                Shape::Triangle(i) => (1u8, i),
            });
        }
    }

    /// Sweeps a box `mins..maxs` (relative to the position) from `start` to `end`.
    pub fn trace_box(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32) -> Trace {
        self.trace_box_excluding(start, end, mins, maxs, mask, None)
    }

    fn trace_box_excluding(
        &self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32,
        excluded_convexes: Option<&std::ops::Range<usize>>,
    ) -> Trace {
        self.trace_box_owned(start, end, mins, maxs, mask, excluded_convexes).0
    }

    fn trace_box_owned(
        &self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32,
        excluded_convexes: Option<&std::ops::Range<usize>>,
    ) -> (Trace, Option<HitOwner>) {
        if let Some(base) = &self.base {
            let mut tr = base.trace_box_excluding(start, end, mins, maxs, mask, excluded_convexes);
            let mut owner = None;
            let offset = (mins + maxs) * 0.5;
            let ext = (maxs - mins) * 0.5;
            let s = start + offset;
            let e = end + offset;
            for (index, solid) in self.convexes.iter().enumerate() {
                if solid.contents & mask != 0 && swept_bounds_overlap(s, e, ext, solid.mins, solid.maxs) {
                    if clip_convex(&solid.planes, s, e, ext, solid.contents, solid.surface, &mut tr) {
                        owner = self.runtime_owners.get(index).copied().flatten();
                    }
                }
            }
            tr.end = if tr.all_solid { start } else { start.lerp(end, tr.fraction) };
            return (tr, owner);
        }
        if let Some(world)=&self.dynamic { return (world.trace_box(start,end,mins,maxs,mask), None); }
        let offset = (mins + maxs) * 0.5;
        let ext = (maxs - mins) * 0.5;
        let s = start + offset;
        let e = end + offset;
        let mut tr = Trace::empty(end);

        // Long sweeps are split into short segments walked in order, so a
        // bullet or shadow ray only visits the grid cells it passes and can
        // stop at the first segment that contains a hit.
        let length = (e - s).length();
        let segments = ((length / (GRID_CELL * 2.0)).ceil() as usize).clamp(1, 1024);
        with_trace_scratch(|TraceScratch { cands, planes: scratch, seen }| {
            for seg in 0..segments {
                let f0 = seg as f32 / segments as f32;
                let f1 = (seg + 1) as f32 / segments as f32;
                let a = s.lerp(e, f0);
                let b = s.lerp(e, f1);
                let sweep_min = a.min(b) - ext - Vec3::ONE;
                let sweep_max = a.max(b) + ext + Vec3::ONE;
                self.candidates(sweep_min, sweep_max, cands);
                for &shape in &*cands {
                    if segments > 1 {
                        let key = match shape {
                            Shape::Convex(i) => (0u8, i),
                            Shape::Triangle(i) => (1u8, i),
                        };
                        if !seen.insert(key) {
                            continue;
                        }
                    }
                    match shape {
                        Shape::Convex(i) => {
                            if excluded_convexes.is_some_and(|range| range.contains(&(i as usize))) {
                                continue;
                            }
                            let c = &self.convexes[i as usize];
                            if c.contents & mask == 0 || !overlaps(c.mins, c.maxs, sweep_min, sweep_max) {
                                if segments > 1 {
                                    seen.remove(&(0, i));
                                }
                                continue;
                            }
                            clip_convex(&c.planes, s, e, ext, c.contents, c.surface, &mut tr);
                        }
                        Shape::Triangle(i) => {
                            let t = &self.triangles[i as usize];
                            if t.contents & mask == 0 || !overlaps(t.mins, t.maxs, sweep_min, sweep_max) {
                                if segments > 1 {
                                    seen.remove(&(1, i));
                                }
                                continue;
                            }
                            triangle_planes(t, scratch);
                            clip_convex(scratch, s, e, ext, t.contents, t.surface, &mut tr);
                        }
                    }
                    if tr.all_solid {
                        break;
                    }
                }
                // Anything later in the sweep cannot beat a hit inside this segment.
                if tr.all_solid || tr.fraction <= f1 {
                    break;
                }
            }
            if tr.fraction < 1.0 {
                tr.end = start + (end - start) * tr.fraction;
            } else {
                tr.end = end;
            }
            if tr.all_solid {
                tr.end = start;
            }
            (tr, None)
        })
    }

    /// Sweeps a point.
    pub fn trace_ray(&self, start: Vec3, end: Vec3, mask: i32) -> Trace {
        self.trace_box(start, end, Vec3::ZERO, Vec3::ZERO, mask)
    }

    /// The exact first-hit owner from the same ordered, masked collision query.
    /// Combat must not infer ownership by approximately matching ray fractions.
    pub fn trace_breakable_ray(&self, start: Vec3, end: Vec3, mask: i32) -> (Trace, Option<usize>) {
        let (tr, owner) = self.trace_owned_ray(start, end, mask);
        (tr, owner.and_then(HitOwner::breakable))
    }

    /// Shares the same winner, ordering, masks and start-solid semantics as
    /// `trace_ray` and `trace_breakable_ray`; never a second proximity query.
    pub fn trace_owned_ray(&self, start: Vec3, end: Vec3, mask: i32) -> (Trace, Option<HitOwner>) {
        self.trace_box_owned(start, end, Vec3::ZERO, Vec3::ZERO, mask, None)
    }

    /// Render-only solid visibility ray from a static prop's lighting sample.
    /// Ignores only that placed prop's hulls, indexed by BSP `sprp` ordinal.
    /// World brushes, displacements and other instances (even of the same
    /// model) still occlude. An absent ordinal excludes nothing. Gameplay
    /// traces use `trace_ray` / `trace_box` without this exclusion.
    pub fn trace_prop_lighting_ray(&self, start: Vec3, end: Vec3, prop_ordinal: usize) -> Trace {
        self.trace_box_excluding(
            start, end, Vec3::ZERO, Vec3::ZERO, contents::MASK_SOLID,
            self.static_prop_convexes.get(prop_ordinal),
        )
    }

    /// Prepare temporary render-only acceleration for many exact lighting rays.
    /// Call after `finalize`; drop it after baking the map's prop lighting.
    pub fn prepare_prop_lighting(&self) -> PropLightingQuery<'_> {
        PropLightingQuery::new(self)
    }

    /// Contents at a point (union of all shapes containing it).
    pub fn point_contents(&self, p: Vec3, mask: i32) -> i32 {
        if let Some(base) = &self.base {
            let mut out = base.point_contents(p, mask);
            for solid in &self.convexes {
                if solid.contents & mask != 0 && solid.planes.iter().all(|pl| pl.normal.dot(p) <= pl.dist) {
                    out |= solid.contents;
                }
            }
            return out;
        }
        if let Some(world)=&self.dynamic {return world.point_contents(p,mask);}
        let mut cands = Vec::new();
        self.candidates(p - Vec3::ONE, p + Vec3::ONE, &mut cands);
        let mut out = 0;
        for shape in cands {
            if let Shape::Convex(i) = shape {
                let c = &self.convexes[i as usize];
                if c.contents & mask != 0 && c.planes.iter().all(|pl| pl.normal.dot(p) - pl.dist <= 0.0) {
                    out |= c.contents;
                }
            }
        }
        out
    }
}

fn overlaps(amin: Vec3, amax: Vec3, bmin: Vec3, bmax: Vec3) -> bool {
    amin.x <= bmax.x && amax.x >= bmin.x && amin.y <= bmax.y && amax.y >= bmin.y && amin.z <= bmax.z && amax.z >= bmin.z
}

/// Continuous slab rejection only; the original convex clipper decides hits.
/// Use the static grid query's one-unit bounds guard (larger than the clipper's
/// contact epsilon), and f64 interval arithmetic to avoid rounding a grazing
/// sweep out. Retain the full segment: later solids can still add start-solid
/// contents even when an earlier solid already set the hit fraction.
fn swept_bounds_overlap(s: Vec3, e: Vec3, ext: Vec3, low: Vec3, high: Vec3) -> bool {
    let mut enter = 0.0f64;
    let mut leave = 1.0f64;
    for axis in 0..3 {
        let a = s[axis] as f64;
        let delta = e[axis] as f64 - a;
        let lo = low[axis] as f64 - ext[axis] as f64 - 1.0;
        let hi = high[axis] as f64 + ext[axis] as f64 + 1.0;
        if delta == 0.0 {
            if a < lo || a > hi { return false; }
        } else {
            let first = (lo - a) / delta;
            let last = (hi - a) / delta;
            enter = enter.max(first.min(last));
            leave = leave.min(first.max(last));
            if enter > leave { return false; }
        }
    }
    true
}

/// Clips a sweep of a centred box with half-extents `ext` against the convex
/// region `n·x <= d` for every plane, updating `tr` when it hits earlier.
// Returns true only when this shape replaces the hit surface. Start-solid
// contents alone do not change ownership; all-solid and strict earlier hits do.
fn clip_convex(planes: &[Plane], s: Vec3, e: Vec3, ext: Vec3, contents: i32, surface: u32, tr: &mut Trace) -> bool {
    let mut enter = -1.0f32;
    let mut leave = 1.0f32;
    let mut clip: Option<Plane> = None;
    let mut get_out = false;
    let mut start_out = false;
    for p in planes {
        let n = p.normal;
        let dist = p.dist + n.x.abs() * ext.x + n.y.abs() * ext.y + n.z.abs() * ext.z;
        let d1 = n.dot(s) - dist;
        let d2 = n.dot(e) - dist;
        if d2 > 0.0 {
            get_out = true;
        }
        if d1 > 0.0 {
            start_out = true;
        }
        // Reconsider only an entry whose f32 distance ordering would clamp
        // to zero. The same stored geometry can provide a separating plane
        // in higher precision; this rejects this convex, not other solids.
        if d1 > 0.0 && d1 <= DIST_EPSILON && d2 < d1 {
            let signed = |point: Vec3| {
                (n.x as f64 * point.x as f64)
                    + (n.y as f64 * point.y as f64)
                    + (n.z as f64 * point.z as f64)
                    - dist as f64
            };
            let precise_start = signed(s);
            let precise_end = signed(e);
            if precise_start > 0.0 && precise_end >= precise_start {
                return false;
            }
        }
        // Entirely in front of this plane: no contact with the solid.
        if d1 > 0.0 && (d2 >= DIST_EPSILON || d2 >= d1) {
            return false;
        }
        if d1 <= 0.0 && d2 <= 0.0 {
            continue;
        }
        if d1 > d2 {
            let f = ((d1 - DIST_EPSILON) / (d1 - d2)).max(0.0);
            if f > enter {
                enter = f;
                clip = Some(Plane { normal: n, dist });
            }
        } else {
            let f = ((d1 + DIST_EPSILON) / (d1 - d2)).min(1.0);
            if f < leave {
                leave = f;
            }
        }
    }
    if !start_out {
        tr.start_solid = true;
        tr.contents |= contents;
        if !get_out {
            tr.all_solid = true;
            tr.fraction = 0.0;
            tr.surface = surface;
            return true;
        }
        return false;
    }
    if enter < leave && enter > -1.0 && enter < tr.fraction {
        if let Some(p) = clip {
            tr.fraction = enter.max(0.0);
            tr.normal = p.normal;
            tr.plane_dist = p.dist;
            tr.contents = contents;
            tr.surface = surface;
            return true;
        }
    }
    false
}

/// Thickness given to triangles behind their front face, so rays and thin
/// sweeps cannot slip through a zero-volume surface.
const TRIANGLE_THICKNESS: f32 = 1.0;

/// Separating-axis planes of a triangle for box sweeps: its two faces, the
/// three box axes, and the nine edge × axis directions (both signs each).
fn triangle_planes(t: &Triangle, out: &mut Vec<Plane>) {
    out.clear();
    let d = t.normal.dot(t.v[0]);
    out.push(Plane { normal: t.normal, dist: d });
    out.push(Plane { normal: -t.normal, dist: -d + TRIANGLE_THICKNESS });
    let push_axis = |axis: Vec3, out: &mut Vec<Plane>| {
        let len_sq = axis.length_squared();
        if len_sq < 1e-8 {
            return;
        }
        let a = axis / len_sq.sqrt();
        // Already covered by the (thickened) face planes.
        if a.dot(t.normal).abs() > 0.9999 {
            return;
        }
        let d = t.v.iter().map(|v| a.dot(*v));
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for x in d {
            lo = lo.min(x);
            hi = hi.max(x);
        }
        out.push(Plane { normal: a, dist: hi });
        out.push(Plane { normal: -a, dist: -lo });
    };
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        push_axis(axis, out);
    }
    for i in 0..3 {
        let edge = t.v[(i + 1) % 3] - t.v[i];
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            push_axis(edge.cross(axis), out);
        }
    }
}

/// Builds a convex from hull points and triangles: face planes, axial
/// bevels and edge bevels, each placed on the hull's supporting plane.
pub fn convex_from_hull(points: &[Vec3], triangles: &[[u32; 3]], contents: i32, surface: u32) -> Option<Convex> {
    if points.len() < 4 {
        return None;
    }
    let mut mins = Vec3::splat(f32::MAX);
    let mut maxs = Vec3::splat(f32::MIN);
    for p in points {
        if !p.is_finite() {
            return None;
        }
        mins = mins.min(*p);
        maxs = maxs.max(*p);
    }
    // A convex solid needs volume. A malformed PHY ledge can have several
    // distinct point indices which all decode to the same position. Adding
    // axial bevels to that input would manufacture a point collider: the
    // player's expanded hull then hits it in an otherwise open doorway.
    // Reject zero-width inputs before bevel construction, retaining every
    // positive thickness (including thin authored walls and floors).
    if !mins.cmplt(maxs).all() {
        return None;
    }
    let mut planes: Vec<Plane> = Vec::new();
    let add = |dir: Vec3, planes: &mut Vec<Plane>| {
        let len = dir.length();
        if len < 1e-6 {
            return;
        }
        let n = dir / len;
        let d = points.iter().map(|p| n.dot(*p)).fold(f32::MIN, f32::max);
        if planes
            .iter()
            .any(|p| p.normal.dot(n) > 0.9999 && (p.dist - d).abs() < 0.01)
        {
            return;
        }
        planes.push(Plane { normal: n, dist: d });
    };
    let centroid = points.iter().copied().sum::<Vec3>() / points.len() as f32;
    for tri in triangles {
        let [a, b, c] = tri.map(|i| points[i as usize]);
        let mut n = (b - a).cross(c - a);
        // Orient outward.
        if n.dot(a - centroid) < 0.0 {
            n = -n;
        }
        add(n, &mut planes);
    }
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        add(axis, &mut planes);
        add(-axis, &mut planes);
    }
    let mut edges: Vec<(u32, u32)> = Vec::new();
    for tri in triangles {
        for i in 0..3 {
            let (a, b) = (tri[i], tri[(i + 1) % 3]);
            let key = (a.min(b), a.max(b));
            if !edges.contains(&key) {
                edges.push(key);
            }
        }
    }
    for (a, b) in edges {
        let e = points[b as usize] - points[a as usize];
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let n = e.cross(axis);
            add(n, &mut planes);
            add(-n, &mut planes);
        }
    }
    Some(Convex {
        planes,
        mins,
        maxs,
        contents,
        surface,
    })
}

#[cfg(test)]
mod prop_volume_tests;

/// Options for building a collision world from a map.
pub struct MapCollisionOptions {
    /// Brush entities whose brushes are solid (func_brush, func_wall, ...).
    pub solid_brush_classes: Vec<&'static str>,
}

impl Default for MapCollisionOptions {
    fn default() -> Self {
        Self {
            solid_brush_classes: vec![
                "func_brush",
                "func_wall",
                "func_detail_blocker",
                "func_door",
                "func_door_rotating",
                "func_breakable",
                "func_breakable_surf",
                "func_physbox",
                "func_movelinear",
                "func_rotating",
                "func_tracktrain",
                "func_clip_vphysics",
            ],
        }
    }
}

/// Loads a static prop's hulls; returns `None` if the prop is not solid.
pub type PropHullLoader<'a> = dyn Fn(&str) -> Option<Vec<mdl::ConvexHull>> + 'a;

impl CollisionWorld {
    /// Adds the world brushes, solid brush entities, displacements and solid
    /// static props of a map. `prop_hulls` loads a model's `.phy` hulls.
    pub fn from_bsp(map: &bsp::Bsp, opts: &MapCollisionOptions, prop_hulls: &PropHullLoader) -> Self {
        Self::from_bsp_with_model_data(map, opts, prop_hulls, &|_| None, &kv::Block::default())
    }

    /// Mounted MDL declarations opt supported props into the same exact-owner
    /// health/collision lifecycle as brushes. Missing declarations stay unchanged.
    pub fn from_bsp_with_model_data(map: &bsp::Bsp, opts: &MapCollisionOptions,
        prop_hulls: &PropHullLoader, models: &breakables::ModelDataLoader<'_>, presets: &kv::Block) -> Self {
        let mut w = CollisionWorld::new();
        let props = breakables::load_props(map, prop_hulls, models, presets);
        // Collect edges before brush loading so linked static brushes are never
        // left in the immutable grid. Rebase owner ordinals after brush admission.
        let mut dependencies = breakables::dependencies(map, &props);

        // Brushes of model 0 (the world) and of solid brush entities.
        let add_model_brushes = |w: &mut CollisionWorld, model: usize, origin: Vec3| {
            let Some(m) = map.models.get(model) else { return };
            let mut brushes: Vec<u16> = Vec::new();
            collect_leaf_brushes(map, m.head_node, &mut brushes);
            brushes.sort_unstable();
            brushes.dedup();
            for bi in brushes {
                let b = &map.brushes[bi as usize];
                if b.contents & contents::MASK_SOLID == 0 && b.contents & contents::PLAYERCLIP == 0 && b.contents & contents::LADDER == 0 {
                    continue;
                }
                let mut planes = Vec::with_capacity(b.num_sides as usize);
                let mut surface_name = String::new();
                for s in 0..b.num_sides.max(0) as usize {
                    let side = &map.brush_sides[b.first_side as usize + s];
                    let p = &map.planes[side.plane as usize];
                    planes.push(Plane {
                        normal: p.normal,
                        dist: p.dist + p.normal.dot(origin),
                    });
                    if surface_name.is_empty() && !side.bevel && side.texinfo >= 0 {
                        if let Some(name) = map.texinfo_material(side.texinfo as usize) {
                            surface_name = name.to_ascii_lowercase();
                        }
                    }
                }
                let Some((mins, maxs)) = brush_bounds(&planes) else { continue };
                let surface = w.surface_id(&surface_name);
                w.add_convex(Convex {
                    planes,
                    mins,
                    maxs,
                    contents: b.contents,
                    surface,
                });
            }
        };
        add_model_brushes(&mut w, 0, Vec3::ZERO);
        let mut breakables = Vec::new();
        for (entity, ent) in map.entities.iter().enumerate() {
            let Some(class) = ent.str("classname") else { continue };
            let dependency = dependencies.iter().position(|dep| dep.entity == entity);
            if dependency.is_none() && !opts.solid_brush_classes.iter().any(|c| c.eq_ignore_ascii_case(class)) {
                continue;
            }
            // func_brush with "Solidity: never solid" (1).
            if class.eq_ignore_ascii_case("func_brush") && ent.i64("solidity") == Some(1) {
                continue;
            }
            // Disabled until a game mode turns them on (e.g. the Retakes
            // blockers on dust2): neither solid nor visible in normal play.
            if ent.i64("startdisabled") == Some(1) {
                continue;
            }
            // CS:GO's authored SF_BUTTON_NOTSOLID applies even when the
            // button is retained as an OnBreak/Kill dependency. Its interaction
            // volume must not become a player or bullet collision barrier.
            if class.eq_ignore_ascii_case("func_button")
                && ent.i64("spawnflags").unwrap_or(0) & 16384 != 0 {
                continue;
            }
            let Some(model) = ent.str("model").and_then(|m| m.strip_prefix('*')).and_then(|n| n.parse::<usize>().ok()) else {
                continue;
            };
            let origin = ent.str("origin").and_then(bsp::parse_origin).unwrap_or(Vec3::ZERO);
            let start = w.convexes.len();
            add_model_brushes(&mut w, model, origin);
            if let Some(index) = dependency {
                dependencies[index].solids = w.convexes.drain(start..).collect();
                continue;
            }
            if breakables.len() < breakables::MAX_BREAKABLES.saturating_sub(props.len()) {
                if let Some(mut def) = breakables::Breakable::load(entity, ent, map) {
                    if w.convexes.len() > start {
                        def.solids = w.convexes.drain(start..).collect();
                        breakables.push(def);
                    }
                }
            }
        }
        for dep in &mut dependencies {
            for index in &mut dep.killed_by { *index += breakables.len(); }
        }
        breakables.extend(props);
        for def in &mut breakables {
            if let Some(studio) = &def.studio {
                let surface = w.surface_id(&studio.model.to_ascii_lowercase());
                for solid in &mut def.solids { solid.surface = surface; }
            }
        }
        w.breakables = breakables.into();
        w.breakable_dependencies = dependencies.into();

        // Displacements.
        for (fi, face) in map.faces.iter().enumerate() {
            if face.dispinfo < 0 {
                continue;
            }
            let Some(mesh) = map.displacement(fi) else { continue };
            let surface_name = map
                .texinfo_material(face.texinfo as usize)
                .unwrap_or("")
                .to_ascii_lowercase();
            let surface = w.surface_id(&surface_name);
            for tri in mesh.indices.chunks_exact(3) {
                w.add_triangle(
                    [
                        mesh.positions[tri[0] as usize],
                        mesh.positions[tri[1] as usize],
                        mesh.positions[tri[2] as usize],
                    ],
                    contents::SOLID,
                    surface,
                );
            }
        }

        // Static props (solid 6 = vphysics).
        let mut cache: std::collections::HashMap<u16, Option<Vec<mdl::ConvexHull>>> = Default::default();
        for (prop_ordinal, prop) in map.static_props.props.iter().enumerate() {
            let first_convex = w.convexes.len();
            // Record every ordinal before any early continue: nonsolid,
            // missing and rejected hulls must not shift later ownership.
            w.static_prop_convexes.push(first_convex..first_convex);
            if prop.solid != 6 {
                continue;
            }
            let hulls = cache.entry(prop.model).or_insert_with(|| {
                map.static_props
                    .models
                    .get(prop.model as usize)
                    .and_then(|name| prop_hulls(name))
            });
            let Some(hulls) = hulls else { continue };
            let rot: Mat3 = mdl::angles_to_mat3(prop.angles);
            let scale = if prop.scale > 0.0 { prop.scale } else { 1.0 };
            let name = map
                .static_props
                .models
                .get(prop.model as usize)
                .cloned()
                .unwrap_or_default();
            let surface = w.surface_id(&name);
            for hull in hulls.iter() {
                let pts: Vec<Vec3> = hull.points.iter().map(|p| prop.origin + rot * (*p * scale)).collect();
                if let Some(c) = convex_from_hull(&pts, &hull.triangles, contents::SOLID, surface) {
                    w.add_convex(c);
                }
            }
            w.static_prop_convexes[prop_ordinal].end = w.convexes.len();
        }
        w.doors = doors::load(map, prop_hulls, &mut w.surfaces).into();
        w.finalize();
        w
    }
}

fn collect_leaf_brushes(map: &bsp::Bsp, node: i32, out: &mut Vec<u16>) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n < 0 {
            let leaf = &map.leafs[(-1 - n) as usize];
            let first = leaf.first_leaf_brush as usize;
            let count = leaf.num_leaf_brushes as usize;
            out.extend_from_slice(&map.leaf_brushes[first..first + count]);
        } else {
            let node = &map.nodes[n as usize];
            stack.push(node.children[0]);
            stack.push(node.children[1]);
        }
    }
}

/// Bounds of a brush from its axial planes (Source brushes always carry all
/// six axial planes as sides or bevels).
fn brush_bounds(planes: &[Plane]) -> Option<(Vec3, Vec3)> {
    let mut mins = Vec3::splat(f32::MIN);
    let mut maxs = Vec3::splat(f32::MAX);
    let mut found = [false; 6];
    for p in planes {
        for axis in 0..3 {
            let n = p.normal[axis];
            let others = (0..3).filter(|&a| a != axis).all(|a| p.normal[a].abs() < 1e-5);
            if !others {
                continue;
            }
            if n > 0.9999 {
                maxs[axis] = p.dist;
                found[axis * 2] = true;
            } else if n < -0.9999 {
                mins[axis] = -p.dist;
                found[axis * 2 + 1] = true;
            }
        }
    }
    found.iter().all(|&f| f).then_some((mins, maxs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_convex(mins: Vec3, maxs: Vec3) -> Convex {
        Convex {
            planes: vec![
                Plane { normal: Vec3::X, dist: maxs.x },
                Plane { normal: -Vec3::X, dist: -mins.x },
                Plane { normal: Vec3::Y, dist: maxs.y },
                Plane { normal: -Vec3::Y, dist: -mins.y },
                Plane { normal: Vec3::Z, dist: maxs.z },
                Plane { normal: -Vec3::Z, dist: -mins.z },
            ],
            mins,
            maxs,
            contents: contents::SOLID,
            surface: 0,
        }
    }

    fn floor_world() -> CollisionWorld {
        let mut w = CollisionWorld::new();
        w.add_convex(box_convex(Vec3::new(-512.0, -512.0, -16.0), Vec3::new(512.0, 512.0, 0.0)));
        w.add_convex(box_convex(Vec3::new(100.0, -512.0, 0.0), Vec3::new(132.0, 512.0, 128.0)));
        w.finalize();
        w
    }

    const HULL_MIN: Vec3 = Vec3::new(-16.0, -16.0, 0.0);
    const HULL_MAX: Vec3 = Vec3::new(16.0, 16.0, 72.0);

    #[test]
    fn box_lands_on_floor() {
        let w = floor_world();
        let tr = w.trace_box(Vec3::new(0.0, 0.0, 64.0), Vec3::new(0.0, 0.0, -64.0), HULL_MIN, HULL_MAX, contents::MASK_PLAYERSOLID);
        assert!(tr.fraction < 1.0);
        assert!((tr.end.z - 0.0).abs() < 0.1, "end {:?}", tr.end);
        assert_eq!(tr.normal, Vec3::Z);
        assert!(!tr.start_solid);
    }

    #[test]
    fn box_stops_at_wall() {
        let w = floor_world();
        let tr = w.trace_box(Vec3::new(0.0, 0.0, 1.0), Vec3::new(200.0, 0.0, 1.0), HULL_MIN, HULL_MAX, contents::MASK_PLAYERSOLID);
        assert!((tr.end.x - 84.0).abs() < 0.1, "end {:?}", tr.end);
        assert_eq!(tr.normal, -Vec3::X);
    }

    #[test]
    fn start_solid_detected() {
        let w = floor_world();
        let tr = w.trace_box(Vec3::new(116.0, 0.0, 10.0), Vec3::new(116.0, 0.0, 20.0), HULL_MIN, HULL_MAX, contents::MASK_PLAYERSOLID);
        assert!(tr.start_solid && tr.all_solid);
    }

    #[test]
    fn triangle_ramp() {
        let mut w = CollisionWorld::new();
        // 45-degree ramp rising along +X.
        w.add_triangle([Vec3::new(0.0, -100.0, 0.0), Vec3::new(100.0, -100.0, 100.0), Vec3::new(0.0, 100.0, 0.0)], contents::SOLID, 0);
        w.add_triangle([Vec3::new(100.0, -100.0, 100.0), Vec3::new(100.0, 100.0, 100.0), Vec3::new(0.0, 100.0, 0.0)], contents::SOLID, 0);
        w.finalize();
        let tr = w.trace_ray(Vec3::new(25.0, 10.0, 200.0), Vec3::new(25.0, 10.0, -200.0), contents::MASK_SOLID);
        assert!((tr.end.z - 25.0).abs() < 0.2, "end {:?}", tr.end);
        assert!(tr.normal.z > 0.7 && tr.normal.x < 0.0);
        // A box resting on the ramp is held up by its lower edge.
        let tr = w.trace_box(Vec3::new(50.0, 0.0, 200.0), Vec3::new(50.0, 0.0, -200.0), HULL_MIN, HULL_MAX, contents::MASK_SOLID);
        assert!((tr.end.z - 66.0).abs() < 0.2, "end {:?}", tr.end);
    }

    #[test]
    fn hull_convex_blocks() {
        // Unit-ish tetrahedron scaled up.
        let pts = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(64.0, 0.0, 0.0), Vec3::new(0.0, 64.0, 0.0), Vec3::new(0.0, 0.0, 64.0)];
        let tris = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let c = convex_from_hull(&pts, &tris, contents::SOLID, 0).unwrap();
        let mut w = CollisionWorld::new();
        w.add_convex(c);
        w.finalize();
        let tr = w.trace_ray(Vec3::new(10.0, 10.0, 100.0), Vec3::new(10.0, 10.0, -10.0), contents::MASK_SOLID);
        assert!((tr.end.z - 44.0).abs() < 0.2, "end {:?}", tr.end);
    }

    #[test]
    fn bsp_prop_ordinals_keep_empty_and_multi_hull_ranges() {
        let prop = |model, solid, x| bsp::StaticProp {
            origin: Vec3::new(x, 0.0, 0.0), angles: Vec3::ZERO, model, solid,
            flags: 0, skin: 0, fade_min: 0.0, fade_max: 0.0, scale: 1.0,
            diffuse_modulation: None,
        };
        let mut map = bsp::Bsp {
            static_props: bsp::StaticProps {
                models: ["box", "missing", "empty", "invalid"].map(String::from).to_vec(),
                props: vec![
                    prop(0, 0, 0.0), prop(0, 6, 0.0), prop(1, 6, 0.0),
                    prop(u16::MAX, 6, 0.0), prop(2, 6, 0.0), prop(3, 6, 0.0),
                    prop(0, 6, 100.0),
                ],
                ..Default::default()
            },
            ..Default::default()
        };
        // A world brush precedes all prop hulls in the real builder.
        let world_brush = box_convex(Vec3::new(-32.0, -16.0, -16.0), Vec3::new(-16.0, 16.0, 16.0));
        map.models.push(bsp::Model {
            mins: world_brush.mins, maxs: world_brush.maxs, origin: Vec3::ZERO,
            head_node: -1, first_face: 0, num_faces: 0,
        });
        map.leafs.push(bsp::Leaf {
            contents: 0, cluster: -1, area: 0, mins: [-32, -16, -16], maxs: [-16, 16, 16],
            first_leaf_face: 0, num_leaf_faces: 0, first_leaf_brush: 0, num_leaf_brushes: 1,
        });
        map.leaf_brushes.push(0);
        map.brushes.push(bsp::Brush { first_side: 0, num_sides: 6, contents: contents::SOLID });
        for (index, plane) in world_brush.planes.iter().enumerate() {
            map.planes.push(bsp::Plane { normal: plane.normal, dist: plane.dist, kind: 0 });
            map.brush_sides.push(bsp::BrushSide { plane: index as u16, texinfo: -1, dispinfo: -1, bevel: false });
        }
        let hull = mdl::ConvexHull {
            points: vec![Vec3::ZERO, Vec3::X * 8.0, Vec3::Y * 8.0, Vec3::Z * 8.0],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        };
        let mut far_hull = hull.clone();
        for point in &mut far_hull.points { point.x += 200_000.0; }
        let loader = |name: &str| match name {
            "box" => Some(vec![hull.clone(), hull.clone()]),
            "empty" => Some(Vec::new()),
            "invalid" => Some(vec![mdl::ConvexHull::default(), far_hull.clone()]),
            _ => None,
        };
        let w = CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &loader);
        assert_eq!(w.static_prop_convexes, vec![1..1, 1..3, 3..3, 3..3, 3..3, 3..3, 3..5]);
        assert_eq!(w.rejected, 1);
        assert_eq!(w.convexes.len(), 5);
        assert_eq!(w.convexes[0].mins, world_brush.mins);
        assert_eq!(w.convexes[1].mins, Vec3::ZERO);
        assert_eq!(w.convexes[3].mins, Vec3::X * 100.0);

        let start = Vec3::new(1.0, 1.0, 1.0);
        let end = Vec3::new(20.0, 1.0, 1.0);
        assert!(w.trace_ray(start, end, contents::MASK_SOLID).start_solid);
        // Both hulls of this instance are excluded; the empty ordinal and
        // an out-of-range ordinal must leave the normal solid trace intact.
        assert!(!w.trace_prop_lighting_ray(start, end, 1).hit());
        assert!(w.trace_prop_lighting_ray(start, end, 0).start_solid);
        assert!(w.trace_prop_lighting_ray(start, end, usize::MAX).start_solid);
        // Identical model and surface, different placed instance: still hit.
        let tr = w.trace_prop_lighting_ray(start, Vec3::new(120.0, 1.0, 1.0), 1);
        assert!(!tr.start_solid);
        assert!(tr.hit());
        assert!((tr.end.x - 100.0).abs() < 0.1, "{tr:?}");
        let tr = w.trace_prop_lighting_ray(start, Vec3::new(-40.0, 1.0, 1.0), 1);
        assert!(!tr.start_solid && tr.hit());
        assert!((tr.end.x + 16.0).abs() < 0.1, "world brush must remain: {tr:?}");
    }
}

#[cfg(test)]
mod long_trace_tests {
    use super::*;

    #[test]
    fn long_ray_hits_far_wall_and_near_wall_first() {
        let mut w = CollisionWorld::new();
        let wall = |x0: f32| Convex {
            planes: vec![
                Plane { normal: Vec3::X, dist: x0 + 16.0 },
                Plane { normal: -Vec3::X, dist: -x0 },
                Plane { normal: Vec3::Y, dist: 512.0 },
                Plane { normal: -Vec3::Y, dist: 512.0 },
                Plane { normal: Vec3::Z, dist: 512.0 },
                Plane { normal: -Vec3::Z, dist: 512.0 },
            ],
            mins: Vec3::new(x0, -512.0, -512.0),
            maxs: Vec3::new(x0 + 16.0, 512.0, 512.0),
            contents: contents::SOLID,
            surface: 0,
        };
        w.add_convex(wall(3000.0));
        w.add_convex(wall(9000.0));
        w.finalize();
        let tr = w.trace_ray(Vec3::ZERO, Vec3::new(12000.0, 0.0, 0.0), contents::MASK_SOLID);
        assert!((tr.end.x - 3000.0).abs() < 0.1, "{:?}", tr.end);
        let tr = w.trace_ray(Vec3::new(3100.0, 0.0, 0.0), Vec3::new(12000.0, 0.0, 0.0), contents::MASK_SOLID);
        assert!((tr.end.x - 9000.0).abs() < 0.1, "{:?}", tr.end);
        let tr = w.trace_ray(Vec3::new(12000.0, 0.0, 0.0), Vec3::new(3100.0, 0.0, 0.0), contents::MASK_SOLID);
        assert!((tr.end.x - 9016.0).abs() < 0.1, "{:?}", tr.end);
    }
}

#[cfg(test)]
mod separating_plane_tests;

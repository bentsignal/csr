//! Clean-room rotating prop doors from mounted BSP entities and PHY hulls.
//! No geometry or map coordinates are supplied by the implementation.

use crate::{CollisionWorld, Convex, PropHullLoader, Trace, contents, convex_from_hull, clip_convex};
use glam::{Mat3, Vec3};
use std::sync::Arc;

pub const MAX_DOORS: usize = 256;

/// Immutable definition. Its ordinal is scoped by the session's map ContentId;
/// model/sound paths resolve only through that map's mounted content VFS.
#[derive(Debug)]
pub struct Door {
    pub entity: usize,
    pub model: String,
    /// A bounded rotating brush may carry one studio visual, positioned in the
    /// closed hinge frame. Collision remains the authored brush, not its AABB.
    pub brush_model: Option<usize>,
    pub visual_entity: usize,
    pub visual_offset: Vec3,
    pub visual_angles: Vec3,
    pub origin: Vec3,
    pub angles: Vec3,
    pub distance: f32,
    pub speed: f32,
    pub return_delay: f32,
    pub initial_angle: f32,
    pub open_direction: i64,
    pub locked: bool,
    pub group: usize,
    pub move_sound: String,
    pub unlock_sound: String,
    pub open_sound: String,
    pub close_sound: String,
    pub surface: u32,
    hulls: Vec<DoorHull>,
    pub center: Vec3,
    pub radius: f32,
}

#[derive(Debug)]
struct DoorHull {
    geometry: mdl::ConvexHull,
    contents: i32,
    surface: u32,
}

impl Door {
    pub fn rotation(&self, angle: f32) -> Mat3 {
        Mat3::from_rotation_z(angle.to_radians()) * mdl::angles_to_mat3(self.angles)
    }

    pub fn visual_pose(&self, angle: f32) -> (Vec3, Mat3) {
        let swing = Mat3::from_rotation_z(angle.to_radians());
        (self.origin + swing * self.visual_offset,
            swing * mdl::angles_to_mat3(self.visual_angles))
    }

    pub fn solids(&self, angle: f32) -> Vec<Convex> {
        let rotation = self.rotation(angle);
        self.hulls.iter().filter_map(|hull| {
            let points: Vec<_> = hull.geometry.points.iter().map(|p| self.origin + rotation * *p).collect();
            // Rebuild the axial/edge bevels for this rotation. Reusing rotated
            // bevels alone is insufficient for swept world-axis player hulls.
            convex_from_hull(&points, &hull.geometry.triangles, hull.contents, hull.surface)
        }).collect()
    }

    pub fn center_at(&self, angle: f32) -> Vec3 { self.origin + self.rotation(angle) * self.center }

    pub fn opening_angle(&self, user: Vec3) -> f32 {
        let sign = match self.open_direction {
            1 => 1.0,
            2 => -1.0,
            _ => {
                let arm = self.rotation(0.0) * self.center;
                if Vec3::Z.cross(arm).dot(user - self.center_at(0.0)) > 0.0 { -1.0 } else { 1.0 }
            }
        };
        sign * self.distance
    }
}

pub(super) fn load(map: &bsp::Bsp, loader: &PropHullLoader<'_>, surfaces: &mut Vec<String>) -> Vec<Door> {
    let mut doors = Vec::new();
    let mut names = Vec::new();
    for (entity, ent) in map.entities.iter().enumerate() {
        if doors.len() >= MAX_DOORS { break; }
        let brush = ent.str("classname") == Some("func_door_rotating");
        if (!brush && ent.str("classname") != Some("prop_door_rotating"))
            || ent.i64("startdisabled") == Some(1) { continue; }
        // Parented/script-driven machinery needs an entity I/O path; do not
        // make it independently usable at an incorrect world-space hinge.
        if ent.str("parentname").is_some_and(|s| !s.is_empty()) { continue; }
        let Some(authored_model) = ent.str("model") else { continue; };
        let Some(origin) = ent.str("origin").and_then(bsp::parse_origin) else { continue; };
        let authored_angles = ent.str("angles").and_then(bsp::parse_origin).unwrap_or(Vec3::ZERO);
        if !origin.is_finite() || origin.abs().max_element() >= 131_072.0 || !authored_angles.is_finite() { continue; }
        let (model, brush_model, visual_entity, visual_offset, visual_angles, angles, mut hulls) = if brush {
            // Admit the observed vertical-hinge, one-way, use-operated brush
            // with exactly one directly parented studio leaf. Other machinery
            // retains its previous handling until its I/O/axis is supported.
            let flags = ent.i64("spawnflags").unwrap_or(0);
            if flags & (64 | 128 | 4 | 1) != 0 || flags & 16 == 0 || flags & 256 == 0
                || authored_angles != Vec3::ZERO { continue; }
            let Some(index) = authored_model.strip_prefix('*').and_then(|n| n.parse::<usize>().ok()) else { continue; };
            if map.models.get(index).is_none() { continue; }
            let Some(name) = ent.str("targetname").filter(|s| !s.is_empty()) else { continue; };
            let children: Vec<_> = map.entities.iter().enumerate().filter(|(_, child)|
                child.str("parentname").is_some_and(|p| p.eq_ignore_ascii_case(name))).collect();
            let [(child_index, child)] = children.as_slice() else { continue; };
            if child.str("classname") != Some("prop_dynamic") || child.i64("startdisabled") == Some(1)
                || child.i64("solid") != Some(6) || child.i64("rendermode").unwrap_or(0) != 0
                || child.f32("modelscale").unwrap_or(1.0) != 1.0
                || child.i64("setbodygroup").unwrap_or(0) != 0 { continue; }
            let Some(model) = child.str("model").filter(|s| !s.starts_with('*')) else { continue; };
            let Some(child_origin) = child.str("origin").and_then(bsp::parse_origin).filter(|v| v.is_finite()) else { continue; };
            let child_angles = child.str("angles").and_then(bsp::parse_origin).unwrap_or(Vec3::ZERO);
            if !child_angles.is_finite() || child_origin.distance(origin) > 4096.0 { continue; }
            let Some(hulls) = brush_hulls(map, index) else { continue; };
            (model, Some(index), *child_index, child_origin - origin, child_angles, Vec3::ZERO, hulls)
        } else {
            if authored_model.starts_with('*') { continue; }
            let Some(hulls) = loader(authored_model) else { continue; };
            (authored_model, None, entity, Vec3::ZERO, authored_angles, authored_angles,
                hulls.into_iter().map(|hull| (hull, contents::SOLID, String::new())).collect())
        };
        // Preserve the shared malformed/zero-volume PHY rejection.
        hulls.retain(|(h, mask, _)| convex_from_hull(&h.points, &h.triangles, *mask, 0).is_some());
        if hulls.is_empty() { continue; }
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        let mut radius = 0.0f32;
        for p in hulls.iter().flat_map(|(h, _, _)| &h.points) {
            low = low.min(*p); high = high.max(*p); radius = radius.max(p.length());
        }
        if radius > 4096.0 { continue; }
        let finite = |key: &str, default: f32| ent.f32(key).filter(|v| v.is_finite()).unwrap_or(default);
        let distance = finite("distance", 90.0).abs().clamp(1.0, 180.0);
        let initial_angle = match ent.i64("spawnpos").unwrap_or(0) {
            1 => distance,
            2 => ent.str("ajarangles").and_then(bsp::parse_origin).filter(|a| a.is_finite())
                .map_or(0.0, |a| (a.y - angles.y).clamp(-distance, distance)),
            _ => 0.0,
        };
        let surface_name = model.to_ascii_lowercase();
        let surface = match surfaces.iter().position(|s| s == &surface_name) {
            Some(i) => i as u32,
            None => { surfaces.push(surface_name); (surfaces.len() - 1) as u32 }
        };
        let hulls = hulls.into_iter().map(|(geometry, contents, name)| {
            let surface = if name.is_empty() { surface } else {
                match surfaces.iter().position(|s| s == &name) {
                    Some(index) => index as u32,
                    None => { surfaces.push(name); (surfaces.len() - 1) as u32 }
                }
            };
            DoorHull { geometry, contents, surface }
        }).collect();
        let cue = |key| ent.str(key).filter(|s| !s.eq_ignore_ascii_case("none")).unwrap_or("").to_owned();
        names.push((ent.str("targetname").unwrap_or("").to_owned(), ent.str("slavename").unwrap_or("").to_owned()));
        doors.push(Door {
            entity, model: model.to_owned(), brush_model, visual_entity, visual_offset, visual_angles,
            origin, angles, distance,
            speed: finite("speed", 100.0).clamp(1.0, 720.0),
            return_delay: if brush && ent.i64("spawnflags").unwrap_or(0) & 32 != 0 { -1.0 }
                else { finite(if brush { "wait" } else { "returndelay" }, -1.0).clamp(-1.0, 3600.0) },
            initial_angle, open_direction: if brush {
                if ent.i64("spawnflags").unwrap_or(0) & 2 != 0 { 2 } else { 1 }
            } else { ent.i64("opendir").unwrap_or(0) },
            locked: ent.i64("spawnflags").unwrap_or(0) & 2048 != 0,
            group: doors.len(), move_sound: cue("soundmoveoverride"),
            unlock_sound: cue("soundunlockedoverride"),
            open_sound: cue("soundopenoverride"), close_sound: cue("soundcloseoverride"),
            surface, hulls, center: (low + high) * 0.5, radius,
        });
    }
    // Connected slave/master pairs share a use action, independent of which
    // leaf the player selects. Stable minimum ordinal avoids ordering effects.
    for _ in 0..doors.len() {
        for (i, (_, slave)) in names.iter().enumerate() {
            if slave.is_empty() { continue; }
            for (j, (name, _)) in names.iter().enumerate() {
                if slave.eq_ignore_ascii_case(name) {
                    let group = doors[i].group.min(doors[j].group);
                    let old = [doors[i].group, doors[j].group];
                    for door in &mut doors { if old.contains(&door.group) { door.group = group; } }
                }
            }
        }
    }
    doors
}

/// Recover the bounded compiled brush convexes in their local hinge frame.
/// Plane triplets supply vertices; each plane's ordered polygon supplies edges
/// for swept-box bevels. Do not substitute a visible mesh or model bounds.
fn brush_hulls(map: &bsp::Bsp, model: usize) -> Option<Vec<(mdl::ConvexHull, i32, String)>> {
    let model = map.models.get(model)?;
    let mut brushes = Vec::new();
    crate::collect_leaf_brushes(map, model.head_node, &mut brushes);
    brushes.sort_unstable(); brushes.dedup();
    if brushes.is_empty() || brushes.len() > 64 { return None; }
    let mut hulls = Vec::new();
    for index in brushes {
        let brush = map.brushes.get(index as usize)?;
        if brush.contents & (contents::MASK_SOLID | contents::PLAYERCLIP | contents::LADDER) == 0 { continue; }
        let first = usize::try_from(brush.first_side).ok()?;
        let count = usize::try_from(brush.num_sides).ok()?;
        if !(4..=64).contains(&count) { return None; }
        let mut planes = Vec::new();
        let mut material = String::new();
        for side in map.brush_sides.get(first..first.checked_add(count)?)? {
            if side.bevel { continue; }
            if material.is_empty() && side.texinfo >= 0 {
                material = map.texinfo_material(side.texinfo as usize).unwrap_or("").to_ascii_lowercase();
            }
            let plane = map.planes.get(side.plane as usize)?;
            if !plane.normal.is_finite() || !plane.dist.is_finite() { return None; }
            planes.push((plane.normal, plane.dist));
        }
        let mut points: Vec<Vec3> = Vec::new();
        for a in 0..planes.len() { for b in a+1..planes.len() { for c in b+1..planes.len() {
            let (na, da) = planes[a]; let (nb, db) = planes[b]; let (nc, dc) = planes[c];
            let det = na.dot(nb.cross(nc));
            if det.abs() < 1e-6 { continue; }
            let point = (nb.cross(nc) * da + nc.cross(na) * db + na.cross(nb) * dc) / det;
            if !point.is_finite() { return None; }
            if planes.iter().any(|(n, d)| n.dot(point) - d > 0.01) { continue; }
            // Only after containment is established is this a genuine vertex.
            // Reject an unsupported whole volume, never manufacture its subset.
            if point.abs().max_element() > 4096.0 { return None; }
            if !points.iter().any(|p| p.distance_squared(point) < 1e-6) { points.push(point); }
        } } }
        if points.len() < 4 || points.len() > 256 { return None; }
        let mut triangles = Vec::new();
        for (normal, dist) in planes {
            let mut face: Vec<usize> = points.iter().enumerate()
                .filter(|(_, p)| (normal.dot(**p) - dist).abs() < 0.01).map(|(i, _)| i).collect();
            if face.len() < 3 { continue; }
            let center = face.iter().map(|&i| points[i]).sum::<Vec3>() / face.len() as f32;
            let u = (points[face[0]] - center).normalize_or_zero();
            let v = normal.cross(u);
            face.sort_by(|&a, &b| {
                let pa = points[a] - center; let pb = points[b] - center;
                pa.dot(v).atan2(pa.dot(u)).total_cmp(&pb.dot(v).atan2(pb.dot(u)))
            });
            for i in 1..face.len()-1 { triangles.push([face[0] as u32, face[i] as u32, face[i+1] as u32]); }
        }
        // An unbounded/incomplete half-space set can have some finite
        // intersections. It is not an admitted closed collider: every triangle
        // edge, including triangulation diagonals, must have two incident faces.
        let mut edges = std::collections::BTreeMap::new();
        for triangle in &triangles {
            for edge in 0..3 {
                let (a, b) = (triangle[edge], triangle[(edge + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_insert(0usize) += 1;
            }
        }
        if edges.is_empty() || edges.values().any(|&count| count != 2) { return None; }
        convex_from_hull(&points, &triangles, brush.contents, 0)?;
        hulls.push((mdl::ConvexHull { points, triangles }, brush.contents, material));
    }
    (!hulls.is_empty()).then_some(hulls)
}

/// Exact stationary player-hull contact against one proposed door pose.
pub fn obstructed(solids: &[Convex], origin: Vec3, mins: Vec3, maxs: Vec3) -> bool {
    let offset = (mins + maxs) * 0.5;
    let ext = (maxs - mins) * 0.5;
    solids.iter().any(|solid| {
        if (origin + maxs).cmplt(solid.mins).any() || (origin + mins).cmpgt(solid.maxs).any() { return false; }
        let mut tr = Trace::empty(origin);
        clip_convex(&solid.planes, origin + offset, origin + offset, ext, solid.contents, solid.surface, &mut tr);
        tr.start_solid
    })
}

pub fn ray_fraction(solids: &[Convex], start: Vec3, end: Vec3) -> Option<f32> {
    let mut tr = Trace::empty(end);
    for solid in solids {
        clip_convex(&solid.planes, start, end, Vec3::ZERO, solid.contents, solid.surface, &mut tr);
    }
    tr.hit().then_some(tr.fraction)
}

impl CollisionWorld {
    /// Original map for authored brush queries (e.g. BSP ladders). Traces on
    /// the outer world still include moving doors and occlusion correctly.
    pub fn static_world(&self) -> &Self { self.base.as_deref().unwrap_or(self) }

    pub fn map_arc(world: &Arc<Self>) -> Arc<Self> {
        world.base.clone().unwrap_or_else(|| world.clone())
    }

    /// Pose-owned collision for one simulation copy. No shared mutable state,
    /// whole-map clone, or map-grid rebuild during prediction/correction.
    /// `solids` contains current doors AND intact brush breakables. Keeping
    /// one overlay avoids losing either owner when the other changes state.
    pub fn with_door_solids(base: Arc<Self>, solids: Vec<Convex>) -> Arc<Self> {
        let owners = vec![None; solids.len()];
        Self::with_runtime_solids(base, solids, owners)
    }

    /// Preserve each intact breakable's owner through the ordinary clipper,
    /// including masks and static/door/brush ties. Owners are map ordinals.
    pub fn with_runtime_solids(base: Arc<Self>, solids: Vec<Convex>, owners: Vec<Option<usize>>) -> Arc<Self> {
        Self::with_runtime_owners(base, solids, owners.into_iter()
            .map(|owner| owner.map(crate::HitOwner::Breakable)).collect())
    }

    /// Door and breakable owners travel with their convexes through one
    /// ordered clip. The legacy breakable-only constructor remains unchanged.
    pub fn with_runtime_owners(base: Arc<Self>, solids: Vec<Convex>, owners: Vec<Option<crate::HitOwner>>) -> Arc<Self> {
        assert_eq!(solids.len(), owners.len());
        if base.doors.is_empty() && base.breakables.is_empty() { return base; }
        Arc::new(Self {
            mins: base.mins, maxs: base.maxs, doors: base.doors.clone(),
            breakables: base.breakables.clone(),
            breakable_dependencies: base.breakable_dependencies.clone(),
            breakable_break_links: base.breakable_break_links.clone(),
            dynamic: base.dynamic.clone(),
            convexes: solids, runtime_owners: owners, base: Some(base), ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests;

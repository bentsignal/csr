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
    hulls: Vec<mdl::ConvexHull>,
    pub center: Vec3,
    pub radius: f32,
}

impl Door {
    pub fn rotation(&self, angle: f32) -> Mat3 {
        Mat3::from_rotation_z(angle.to_radians()) * mdl::angles_to_mat3(self.angles)
    }

    pub fn solids(&self, angle: f32) -> Vec<Convex> {
        let rotation = self.rotation(angle);
        self.hulls.iter().filter_map(|hull| {
            let points: Vec<_> = hull.points.iter().map(|p| self.origin + rotation * *p).collect();
            // Rebuild the axial/edge bevels for this rotation. Reusing rotated
            // bevels alone is insufficient for swept world-axis player hulls.
            convex_from_hull(&points, &hull.triangles, contents::SOLID, self.surface)
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
        if ent.str("classname") != Some("prop_door_rotating") || ent.i64("startdisabled") == Some(1) { continue; }
        // Parented/script-driven machinery needs an entity I/O path; do not
        // make it independently usable at an incorrect world-space hinge.
        if ent.str("parentname").is_some_and(|s| !s.is_empty()) { continue; }
        let Some(model) = ent.str("model").filter(|s| !s.starts_with('*')) else { continue; };
        let Some(origin) = ent.str("origin").and_then(bsp::parse_origin) else { continue; };
        let angles = ent.str("angles").and_then(bsp::parse_origin).unwrap_or(Vec3::ZERO);
        if !origin.is_finite() || origin.abs().max_element() >= 131_072.0 || !angles.is_finite() { continue; }
        let Some(mut hulls) = loader(model) else { continue; };
        // Preserve the shared malformed/zero-volume PHY rejection.
        hulls.retain(|h| convex_from_hull(&h.points, &h.triangles, contents::SOLID, 0).is_some());
        if hulls.is_empty() { continue; }
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        let mut radius = 0.0f32;
        for p in hulls.iter().flat_map(|h| &h.points) {
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
        let cue = |key| ent.str(key).filter(|s| !s.eq_ignore_ascii_case("none")).unwrap_or("").to_owned();
        names.push((ent.str("targetname").unwrap_or("").to_owned(), ent.str("slavename").unwrap_or("").to_owned()));
        doors.push(Door {
            entity, model: model.to_owned(), origin, angles, distance,
            speed: finite("speed", 100.0).clamp(1.0, 720.0),
            return_delay: finite("returndelay", -1.0).clamp(-1.0, 3600.0),
            initial_angle, open_direction: ent.i64("opendir").unwrap_or(0),
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
        assert_eq!(solids.len(), owners.len());
        if base.doors.is_empty() && base.breakables.is_empty() { return base; }
        Arc::new(Self {
            mins: base.mins, maxs: base.maxs, doors: base.doors.clone(),
            breakables: base.breakables.clone(),
            breakable_dependencies: base.breakable_dependencies.clone(),
            dynamic: base.dynamic.clone(),
            convexes: solids, runtime_breakables: owners, base: Some(base), ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests;

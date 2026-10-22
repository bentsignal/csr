//! Contact with authored BSP ladder brushes, independent of visual ladder props.
use super::*;

/// Reach from the player's collision hull, in map units. This includes the
/// ordinary sweep separation epsilon; it never expands solid movement hulls.
pub const REACH: f32 = 2.0;

impl CollisionWorld {
    /// Outward normal of a nearby ladder face. Use all convex planes rather
    /// than its bounds: slanted ladders and brush bevels need exact contact.
    /// Caps do not attach, so walking over the top returns to normal movement.
    pub fn ladder_normal(&self, origin: Vec3, mins: Vec3, maxs: Vec3) -> Option<Vec3> {
        // Dynamic backends retain their own climbable policy.
        let brushes = self.static_world();
        if self.dynamic.is_some() || brushes.ladder_convexes.is_empty() { return None; }
        let center = origin + (mins + maxs) * 0.5;
        let ext = (maxs - mins) * 0.5;
        let low = origin + mins - Vec3::splat(REACH);
        let high = origin + maxs + Vec3::splat(REACH);
        let mut closest: Option<(f32, Vec3)> = None;
        for &index in &brushes.ladder_convexes {
            let brush = &brushes.convexes[index];
            if !overlaps(low, high, brush.mins, brush.maxs) { continue; }
            let mut face: Option<(f32, Vec3)> = None;
            let mut outside = false;
            for plane in &brush.planes {
                let gap = plane.normal.dot(center) - plane.dist - plane.normal.abs().dot(ext);
                // No vertical reach above/below a ladder; otherwise its cap
                // can recapture a player who just climbed onto the floor.
                let reach = if plane.normal.z.abs() >= 0.7 { 0.0 } else { REACH };
                if gap > reach { outside = true; break; }
                if face.is_none_or(|(best, _)| gap > best) { face = Some((gap, plane.normal)); }
            }
            if outside { continue; }
            let Some((gap, normal)) = face else { continue };
            if normal.z.abs() >= 0.7 || gap < -REACH { continue; }
            // A nearby ladder behind another solid must not catch the player.
            // Exclude only this static ladder brush, retaining floors, other
            // walls and the outer world's current door poses.
            let excluded = index..index + 1;
            // Stop at the ordinary hull clearance, not inside the face.
            // Authored clip can share the ladder plane along its sides or
            // between segments. Penetrating that plane would falsely make
            // this flush clip an obstruction (especially on narrow ladders).
            // A solid in front of the face still stops this sweep earlier.
            let contact = origin - normal * (gap - DIST_EPSILON).max(0.0);
            let obstruction = self.trace_box_excluding(origin, contact, mins, maxs,
                contents::MASK_PLAYERSOLID, Some(&excluded));
            // Adjacent authored ladder segments can overlap the same player
            // hull. They are a continuous climbing face, not an occluder.
            if obstruction.hit() && obstruction.contents & contents::LADDER == 0 { continue; }
            // Prefer the nearest surface, with stable brush order at ties.
            if closest.is_none_or(|(best, _)| gap.abs() < best) {
                closest = Some((gap.abs(), normal));
            }
        }
        closest.map(|(_, normal)| normal)
    }
}

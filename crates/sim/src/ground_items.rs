//! Minimal support-loss fall, owned by the same tick as damage and door motion.
//! This preserves the existing placed drop, with no rigid-body toss or bounce.

use crate::World;
use collision::contents;
use glam::Vec3;
use std::sync::Arc;

/// Existing throw_point clearance above its downward contact trace.
pub(crate) const SUPPORT_OFFSET: f32 = 1.0;

impl World {
    pub(crate) fn step_ground_items(&mut self) {
        if self.items.is_empty() {
            self.ground_support_collision = None;
            return;
        }
        let collision_changed = self.ground_support_collision.as_ref()
            .is_none_or(|previous| !Arc::ptr_eq(previous, &self.collision));
        let dt = self.dt();
        for item in &mut self.items {
            if item.settled && !collision_changed { continue; }

            if item.settled || item.vertical_velocity == 0.0 {
                let support = self.collision.trace_ray(item.pos,
                    item.pos - Vec3::Z * (SUPPORT_OFFSET + 2.0 * collision::DIST_EPSILON),
                    contents::MASK_PLAYERSOLID);
                // Preserve the exact original placement, including its trace
                // skin. Any upward-facing contact supports this vertical-only
                // fall, just as the existing downward drop-placement ray does.
                if support.hit() && (support.start_solid || support.normal.z > 0.0) {
                    item.vertical_velocity = 0.0;
                    item.settled = true;
                    continue;
                }
            }
            item.settled = false;
            // Match movement's two half-gravity steps and configured velocity
            // bound. Sweep the existing support anchor, not a guessed gun hull.
            let half_gravity = self.config.gravity * 0.5 * dt;
            item.vertical_velocity = (item.vertical_velocity - half_gravity)
                .clamp(-self.config.max_velocity, self.config.max_velocity);
            let start = item.pos - Vec3::Z * SUPPORT_OFFSET;
            let end = start + Vec3::Z * (item.vertical_velocity * dt);
            let contact = self.collision.trace_ray(start, end, contents::MASK_PLAYERSOLID);
            if contact.start_solid || contact.all_solid {
                // A moving solid can overlap the old anchor. Do not invent a
                // push/depenetration law; recheck when collision next changes.
                item.vertical_velocity = 0.0;
                item.settled = true;
            } else {
                item.pos = contact.end + Vec3::Z * SUPPORT_OFFSET;
                if contact.hit() && contact.normal.z > 0.0 {
                    item.vertical_velocity = 0.0;
                    item.settled = true;
                } else {
                    item.vertical_velocity = (item.vertical_velocity - half_gravity)
                        .clamp(-self.config.max_velocity, self.config.max_velocity);
                }
            }
        }
        self.ground_support_collision = Some(self.collision.clone());
    }
}

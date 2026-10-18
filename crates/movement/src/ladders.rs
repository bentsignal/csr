//! BSP ladders run inside the same tick and sweep solver as ordinary movement.
//! The geometry is authored map data; no map names or prop coordinates appear.
use super::*;

// Clean-room initial movement policy. Native speed/diagonal/crouch and
// dismount parity still require measurement; these are not a fidelity receipt.
pub const CLIMB_SPEED: f32 = 200.0;
const JUMP_AWAY_SPEED: f32 = 270.0;

impl<W: TraceWorld + ?Sized> Mover<'_, W> {
    /// Returns true when ladder movement (including a detach command) consumed
    /// the tick. Voxel climbing remains on its existing independent path.
    pub(super) fn ladder_move(&mut self, input: &MoveInput, events: &mut MoveEvents) -> bool {
        let previous = self.pm.ladder_normal.take();
        let (mins, maxs) = self.pm.hull();
        let Some(normal) = self.world.ladder_normal(self.pm.origin, mins, maxs) else { return false };
        // A jump launches away from the face. Do not immediately catch the
        // same face while the jump key is held or that launch is moving out.
        // Crossing a side edge can expose that brush's narrow end face. Keep
        // the outward travel instead of converting it into another climb on
        // the end face, which would trap a player strafing off the ladder.
        if (previous.is_none() && input.buttons & buttons::JUMP != 0)
            || (previous != Some(normal) && self.pm.velocity.dot(normal) > 1.0)
        { return false; }

        self.pm.on_ground = false;
        self.pm.ground_ticks = 0;
        self.pm.last_fall_speed = 0.0;
        events.landed = None;
        if input.buttons & buttons::JUMP != 0 && self.pm.old_buttons & buttons::JUMP == 0 {
            self.pm.velocity = normal * JUMP_AWAY_SPEED;
            events.jumped = true;
            self.try_player_move();
            self.categorize(events);
            return true;
        }

        self.pm.ladder_normal = Some(normal);
        let (forward, right, _) = angle_vectors(input.view_angles);
        let mut wish = forward * input.forward_move + right * input.side_move;
        let mut speed = CLIMB_SPEED.min(self.max_speed);
        if self.pm.ducked { speed *= self.cfg.duck_speed_modifier; }
        else if input.buttons & buttons::WALK != 0 { speed *= self.cfg.walk_speed_modifier; }
        wish = wish.clamp_length_max(speed);
        let up = (Vec3::Z - normal * normal.z).normalize_or_zero();
        let inward = wish.dot(normal);
        // Convert input into the ladder to upward travel, preserving lateral
        // and look-directed vertical motion on the face. Backward input descends.
        self.pm.velocity = (wish - normal * inward - up * inward).clamp_length_max(speed);
        self.try_player_move();
        self.check_velocity();
        // Leaving an edge/top restores gravity next tick. Never snap position
        // or bypass collision to reach a dismount point.
        let (mins, maxs) = self.pm.hull();
        if self.world.ladder_normal(self.pm.origin, mins, maxs).is_none() {
            self.pm.ladder_normal = None;
        }
        true
    }
}

#[cfg(test)]
mod tests;

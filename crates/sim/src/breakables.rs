//! Damage comes only from the ordinary TickInput combat path. Prediction
//! owns an independent health vector and reconstructs collision on correction.
use crate::World;
use glam::Vec3;
use serde::{Deserialize, Serialize};
pub use collision::breakables::MAX_BREAKABLES;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub health: f32,
    /// Health zero in a declaration means indestructible, not already broken.
    pub destroyed: bool,
}
impl State {
    pub fn broken(&self) -> bool { self.destroyed }
    pub fn valid(&self) -> bool {
        self.health.is_finite() && (0.0..=collision::breakables::MAX_HEALTH).contains(&self.health)
            && (!self.destroyed || self.health == 0.0)
    }
}

pub(crate) enum DamageKind { Bullet, Club }

impl World {
    pub fn validate_breakable_state(&self, states: &[State]) -> Result<(), String> {
        if states.len() != self.collision.breakables.len()
            || states.iter().zip(self.collision.breakables.iter()).any(|(state, def)|
                !state.valid() || state.health > def.health
                    || (!state.destroyed && state.health == 0.0 && def.health > 0.0)
                    || ((def.health == 0.0 || def.material == 7 || def.trigger_only)
                        && (state.destroyed || state.health != def.health))) {
            return Err("snapshot breakables differ from mounted map".into());
        }
        Ok(())
    }

    pub(crate) fn reset_breakables(&mut self) {
        self.breakables = self.collision.breakables.iter()
            .map(|def| State { health: def.health, destroyed: false }).collect();
        // The caller resets doors immediately afterwards, rebuilding the one
        // shared runtime overlay with both authored door poses and intact vents.
    }

    pub(crate) fn damage_breakable(&mut self, start: Vec3, end: Vec3, tr: &collision::Trace,
        damage: f32, kind: DamageKind) {
        if !damage.is_finite() || damage <= 0.0 || tr.start_solid || tr.all_solid || !tr.hit() { return; }
        // Trace the actual authored convexes, never their AABBs. The ordinary
        // trace already includes static world/doors/other breakables, so only
        // a leaf at that first surface may take damage. Player hits resolve
        // before this call; no damage passes through players or solid walls.
        let hit = self.collision.breakables.iter().zip(&self.breakables).enumerate()
            .filter(|(_, (_, state))| !state.broken())
            .filter_map(|(i, (def, _))| collision::doors::ray_fraction(&def.solids, start, end)
                .filter(|f| (*f - tr.fraction).abs() <= 0.000001).map(|f| (i, f)))
            .min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i);
        let Some(index) = hit else { return; };
        let def = &self.collision.breakables[index];
        let state = &mut self.breakables[index];
        if def.trigger_only || def.material == 7 || state.health <= 0.0 { return; }
        let amount = damage * match kind { DamageKind::Bullet => def.bullet_scale, DamageKind::Club => def.club_scale };
        if !amount.is_finite() || amount < def.minimum_damage || amount <= 0.0 { return; }
        state.health = (state.health - amount).max(0.0);
        if state.health == 0.0 {
            state.destroyed = true;
            // Only the small runtime geometry is rebuilt, never the map/grid.
            // Subsequent pellets and later players this tick see the opening.
            self.rebuild_door_collision();
        }
    }
}

#[cfg(test)]
mod tests;

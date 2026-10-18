//! Tick-owned map doors. Both authority and prediction select and step the
//! same mounted definition; render frames only consume the resulting pose.

use crate::{GameEvent, TickInput, World};
use collision::{contents, doors::obstructed};
use glam::Vec3;
use movement::buttons;
use serde::{Deserialize, Serialize};

pub const USE_DISTANCE: f32 = 96.0;
pub use collision::doors::MAX_DOORS;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Cue { Move, Open, Close, Unlock }

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// Degrees about the authored world-Z hinge, relative to closed angles.
    pub angle: f32,
    pub target: f32,
    pub return_at: Option<f32>,
}

impl State {
    pub fn valid(&self) -> bool {
        self.angle.is_finite() && self.angle.abs() <= 180.0
            && self.target.is_finite() && self.target.abs() <= 180.0
            && self.return_at.is_none_or(|at| at.is_finite() && at >= 0.0)
    }
}

impl World {
    /// Map-dependent admission before restoring a remote snapshot. A peer
    /// cannot remove closed doors or invent a pose outside the mounted swing.
    pub fn validate_door_state(&self, states: &[State]) -> Result<(), String> {
        if states.len() != self.collision.doors.len() || states.iter().zip(self.collision.doors.iter())
            .any(|(state, def)| !state.valid() || state.angle.abs() > def.distance
                || state.target.abs() > def.distance) {
            return Err("snapshot doors differ from mounted map".into());
        }
        Ok(())
    }

    pub(crate) fn reset_doors(&mut self) {
        self.doors = self.collision.doors.iter().map(|def| State {
            angle: def.initial_angle, target: def.initial_angle, return_at: None,
        }).collect();
        self.rebuild_door_collision();
    }

    pub(crate) fn rebuild_door_collision(&mut self) {
        let mut solids: Vec<_> = self.collision.doors.iter().zip(&self.doors)
            .flat_map(|(def, state)| def.solids(state.angle)).collect();
        let mut owners = vec![None; solids.len()];
        for (index, (def, state)) in self.collision.breakables.iter().zip(&self.breakables).enumerate() {
            if state.broken() { continue; }
            solids.extend(def.solids.iter().cloned());
            owners.resize(solids.len(), Some(index));
        }
        for dep in self.collision.breakable_dependencies.iter() {
            if dep.killed_by.iter().any(|&index| self.breakables[index].broken()) { continue; }
            solids.extend(dep.solids.iter().cloned());
            owners.resize(solids.len(), None);
        }
        self.collision = collision::CollisionWorld::with_runtime_solids(
            collision::CollisionWorld::map_arc(&self.collision), solids, owners);
    }

    fn door_cue(&mut self, index: usize, cue: Cue) {
        self.events.push(GameEvent::DoorSound {
            door: index as u32, cue,
            pos: self.collision.doors[index].center_at(self.doors[index].angle),
        });
    }

    /// Door selection uses the current input's aim, bounded against the eye,
    /// independently of the display camera. USE is edge-triggered: holding it
    /// never oscillates an open leaf or repeatedly restarts its sound.
    fn used_door(&self, player: &crate::Player, input: &TickInput) -> Option<usize> {
        if !player.alive || player.team == crate::Team::Spectator || input.buttons & buttons::USE == 0
            || player.last_input.buttons & buttons::USE != 0 { return None; }
        let (mut from, mut direction) = (player.eye(), movement::angle_vectors(input.view_angles).0);
        if let Some(aim) = input.aim.filter(|a| a.origin.is_finite() && a.direction.is_finite()) {
            let offset = Vec3::Z * player.movement.view_offset();
            let hand = player.movement.origin + offset + (aim.origin - offset).clamp_length_max(crate::MAX_AIM_OFFSET);
            if !self.collision.trace_box(from, hand, Vec3::splat(-crate::AIM_WALL_MARGIN),
                Vec3::splat(crate::AIM_WALL_MARGIN), contents::MASK_SHOT).hit() {
                from = hand;
            }
            direction = aim.direction.try_normalize().unwrap_or(direction);
        }
        if !from.is_finite() || !direction.is_finite() { return None; }
        let end = from + direction * USE_DISTANCE;
        let world_hit = self.collision.trace_ray(from, end, contents::MASK_SHOT);
        if world_hit.start_solid || world_hit.all_solid { return None; }
        let mut nearest = world_hit.fraction + 0.0001;
        let mut selected = None;
        for (index, (def, state)) in self.collision.doors.iter().zip(&self.doors).enumerate() {
            let solids = def.solids(state.angle);
            let fraction = collision::doors::ray_fraction(&solids, from, end);
            if let Some(fraction) = fraction.filter(|f| *f < nearest) {
                selected = Some(index); nearest = fraction;
            }
        }
        selected
    }

    pub(crate) fn step_doors(&mut self, inputs: &[(crate::PlayerId, TickInput)], frozen: bool) {
        if self.doors.is_empty() { return; }
        let now = self.time();
        let dt = self.dt();
        // Stable player order, same first-input policy as the ordinary player
        // loop. One group changes at most once per tick with simultaneous use.
        let mut used = Vec::new();
        if !frozen {
            for player in &self.players {
                let Some((_, input)) = inputs.iter().find(|(id, _)| *id == player.id) else { continue; };
                if let Some(index) = self.used_door(player, input) {
                    let group = self.collision.doors[index].group;
                    if !used.iter().any(|(g, _)| *g == group) { used.push((group, player.movement.origin)); }
                }
            }
        }
        for (group, user) in used {
            if self.collision.doors.iter().any(|d| d.group == group && d.locked) { continue; }
            let leader = self.collision.doors.iter().position(|d| d.group == group).unwrap();
            let close = self.doors[leader].target != 0.0;
            for index in 0..self.doors.len() {
                let def = &self.collision.doors[index];
                if def.group != group { continue; }
                self.doors[index].target = if close { 0.0 } else { def.opening_angle(user) };
                self.doors[index].return_at = None;
                if !close && self.doors[index].angle == 0.0 { self.door_cue(index, Cue::Unlock); }
                self.door_cue(index, Cue::Move);
            }
        }
        let mut changed = false;
        for index in 0..self.doors.len() {
            let mut state = self.doors[index];
            if state.return_at.is_some_and(|at| now >= at) {
                state.target = 0.0; state.return_at = None;
                self.door_cue(index, Cue::Move);
            }
            let def = &self.collision.doors[index];
            let delta = (state.target - state.angle).clamp(-def.speed * dt, def.speed * dt);
            if delta != 0.0 {
                // Bound tip travel to 0.5 units and expand occupants by that
                // amount so a rotating leaf cannot skip through a thin hull.
                let steps = (delta.to_radians().abs() * def.radius / 0.5).ceil().max(1.0) as usize;
                let start = state.angle;
                let previous_solids = def.solids(start);
                for step in 1..=steps {
                    let angle = start + delta * step as f32 / steps as f32;
                    let solids = def.solids(angle);
                    if self.players.iter().filter(|p| p.alive).any(|p| {
                        let (mins, maxs) = p.movement.hull();
                        let padded = |solids: &[_]| obstructed(solids, p.movement.origin,
                            mins - Vec3::splat(0.5), maxs + Vec3::splat(0.5));
                        padded(&solids) && (!padded(&previous_solids)
                            || obstructed(&solids, p.movement.origin, mins, maxs))
                    }) { break; }
                    state.angle = angle;
                }
                changed |= state.angle != self.doors[index].angle;
                if (state.angle - state.target).abs() < 0.0001 {
                    state.angle = state.target;
                    if state.target != 0.0 && def.return_delay >= 0.0 { state.return_at = Some(now + def.return_delay); }
                    self.doors[index] = state;
                    self.door_cue(index, if state.target == 0.0 { Cue::Close } else { Cue::Open });
                }
            }
            self.doors[index] = state;
        }
        if changed { self.rebuild_door_collision(); }
    }
}

#[cfg(test)]
mod tests;

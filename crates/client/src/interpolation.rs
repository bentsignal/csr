//! One render timeline for cameras, characters and first-person weapons.
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub origin: glam::Vec3,
    pub velocity: glam::Vec3,
    pub on_ground: bool,
    pub eye: glam::Vec3,
    pub angles: glam::Vec3,
    pub punch: glam::Vec2,
    alive: bool,
}
impl Pose {
    pub fn player(p: &sim::Player) -> Self {
        Self { velocity: p.movement.velocity, on_ground: p.movement.on_ground, origin: p.movement.origin, eye: p.eye(), angles: p.view_angles, punch: p.view_punch().into(), alive: p.alive }
    }
    pub fn remote(p: &netcode::remote_pose::RenderPose) -> Self {
        Self { origin: p.origin, velocity: p.velocity, on_ground: p.on_ground,
            eye: p.eye, angles: p.view_angles, punch: p.punch, alive: p.alive }
    }
    fn lerp(self, next: Self, alpha: f32) -> Self {
        Self {
            velocity: self.velocity.lerp(next.velocity, alpha), on_ground: next.on_ground,
            origin: self.origin.lerp(next.origin, alpha), eye: self.eye.lerp(next.eye, alpha),
            angles: glam::Vec3::new(angle_lerp(self.angles.x, next.angles.x, alpha), angle_lerp(self.angles.y, next.angles.y, alpha), 0.0),
            punch: self.punch.lerp(next.punch, alpha), alive: next.alive,
        }
    }
}
fn angle_lerp(a: f32, b: f32, alpha: f32) -> f32 { a + ((b - a + 180.0).rem_euclid(360.0) - 180.0) * alpha }

#[derive(Resource, Default)]
pub struct RenderPoses(HashMap<sim::PlayerId, (Pose, Pose)>, HashMap<sim::PlayerId, f32>, HashMap<sim::PlayerId, Pose>, Option<PredictedLocal>);

struct PredictedLocal {
    id: sim::PlayerId,
    tick: u64,
    life: u64,
    raw: Pose,
    stair: f32,
}

/// Eases a stair step out of the camera at 140 units/s (`dt`: seconds per tick).
fn stair_offset(offset: f32, rise: f32, grounded: bool, reset: bool, dt: f32) -> f32 {
    if reset { return 0.0; }
    let mut offset = offset;
    if grounded && (2.0..=18.1).contains(&rise.abs()) { offset = (offset - rise).clamp(-24.0,24.0); }
    offset - offset.signum() * offset.abs().min(140.0 * dt)
}
impl RenderPoses {
    /// Already-sampled render poses; never write these into gameplay state.
    pub fn set_overrides(&mut self, poses: impl IntoIterator<Item = (sim::PlayerId, Pose)>) {
        self.2.clear();
        self.2.extend(poses);
    }
    pub fn clear_overrides(&mut self) { self.2.clear(); self.3 = None; }
    /// Preserve the existing stair-eye easing for local prediction without
    /// capturing or changing any authoritative/remote actor state.
    pub fn capture_prediction(&mut self, player: &sim::Player, tick: u64, dt: f32) -> Pose {
        let raw = Pose::player(player);
        let mut stair = 0.;
        if let Some(previous) = &self.3 {
            let reset = previous.id != player.id || previous.life != player.life_epoch
                || previous.raw.alive != raw.alive || tick < previous.tick
                || previous.raw.origin.distance(raw.origin) > 128.
                || (tick == previous.tick && previous.raw.origin != raw.origin);
            stair = stair_offset(previous.stair, raw.origin.z - previous.raw.origin.z,
                raw.on_ground && previous.raw.on_ground, reset,
                tick.saturating_sub(previous.tick) as f32 * dt);
        }
        self.3 = Some(PredictedLocal { id: player.id, tick, life: player.life_epoch, raw, stair });
        let mut render = raw;
        render.eye.z += stair;
        render
    }
    pub fn capture(&mut self, world: &sim::World) {
        let round_start = world.events.iter().any(|e| matches!(e, sim::GameEvent::RoundStart { .. }));
        self.0.retain(|id, _| world.player(*id).is_some());
        self.1.retain(|id, _| world.player(*id).is_some());
        for p in &world.players {
            let mut current = Pose::player(p);
            let pair = self.0.entry(p.id).or_insert((current, current));
            let reset = round_start || pair.1.alive != current.alive || pair.1.origin.distance(current.origin) > 128.0;
            let offset = self.1.entry(p.id).or_default();
            *offset = stair_offset(*offset, current.origin.z - pair.1.origin.z, current.on_ground && pair.1.on_ground, reset, world.dt());
            current.eye.z += *offset;
            *pair = (if reset { current } else { pair.1 }, current);
        }
    }
    pub fn sample(&self, player: &sim::Player, alpha: f32) -> Pose {
        if let Some(pose) = self.2.get(&player.id) { return *pose; }
        self.0.get(&player.id).map_or_else(|| Pose::player(player), |&(a, b)| a.lerp(b, alpha.clamp(0.0, 1.0)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn predicted_stairs_match_offline_easing_and_snap_new_lives_and_corrections() {
        for rate in [64, 128] {
            let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
            world.set_tick_rate(rate);
            let id = world.add_player("Local", sim::Team::CounterTerrorist);
            world.events.clear();
            world.player_mut(id).unwrap().movement.on_ground = true;
            let mut offline = RenderPoses::default();
            let mut predicted = RenderPoses::default();
            offline.capture(&world);
            predicted.capture_prediction(world.player(id).unwrap(), world.tick, world.dt());
            world.player_mut(id).unwrap().movement.origin.z += 18.;
            for tick in 1..=32 {
                world.tick = tick;
                let before = world.state();
                offline.capture(&world);
                let pose = predicted.capture_prediction(world.player(id).unwrap(), tick, world.dt());
                assert_eq!(pose.eye, offline.sample(world.player(id).unwrap(), 1.).eye);
                assert_eq!(world.state(), before);
                if tick == 1 { assert!(pose.eye.z < world.player(id).unwrap().eye().z - 15.); }
            }
            world.respawn(id);
            let pose = predicted.capture_prediction(world.player(id).unwrap(), 33, world.dt());
            assert_eq!(pose.eye, world.player(id).unwrap().eye());
            world.player_mut(id).unwrap().movement.origin.z += 4.;
            let corrected = predicted.capture_prediction(world.player(id).unwrap(), 33, world.dt());
            assert_eq!(corrected.eye, world.player(id).unwrap().eye());
            world.player_mut(id).unwrap().movement.origin.x += 200.;
            let teleport = predicted.capture_prediction(world.player(id).unwrap(), 34, world.dt());
            assert_eq!(teleport.eye, world.player(id).unwrap().eye());
            predicted.clear_overrides();
            world.player_mut(id).unwrap().movement.origin.z += 4.;
            assert_eq!(predicted.capture_prediction(world.player(id).unwrap(), 35, world.dt()).eye, world.player(id).unwrap().eye());
        }
    }
    #[test]
    fn stairs_ease_out_but_jumps_and_teleports_do_not_create_offsets() {
        for rate in [64, 128] {
            let dt = 1.0 / rate as f32;
            let first = stair_offset(0.0, 18.0, true, false, dt);
            assert!(first < -15.0 && first > -18.0);
            let mut offset = first;
            // 3/16 s of level ground eases the full step out.
            for _ in 0..12 * rate / 64 { offset = stair_offset(offset, 0.0, true, false, dt); }
            assert_eq!(offset, 0.0);
            assert_eq!(stair_offset(0.0, 18.0, false, false, dt), 0.0);
            assert_eq!(stair_offset(-16.0, 150.0, true, true, dt), 0.0);
        }
    }
    #[test]
    fn turns_cross_zero_by_the_short_path() {
        assert!((angle_lerp(359.0, 1.0, 0.5) - 360.0).abs() < 0.001);
        assert!((angle_lerp(1.0, 359.0, 0.5)).abs() < 0.001);
    }
    #[test]
    fn frames_between_ticks_have_continuous_position_and_rotation() {
        let a = Pose { velocity: glam::Vec3::ZERO, on_ground: true, origin: glam::Vec3::ZERO, eye: glam::Vec3::Z * 64.0, angles: glam::Vec3::ZERO, punch: glam::Vec2::ZERO, alive: true };
        let b = Pose { origin: glam::Vec3::X * 4.0, eye: a.eye + glam::Vec3::X * 4.0, angles: glam::Vec3::Y * 8.0, ..a };
        for i in 0..=4 {
            let p = a.lerp(b, i as f32 / 4.0);
            assert_eq!(p.eye.x, i as f32);
            assert_eq!(p.origin.x, p.eye.x);
            assert_eq!(p.angles.y, i as f32 * 2.0);
        }
    }
}

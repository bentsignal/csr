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
    /// Character animation sampled on the same timeline as this transform.
    pub animation: sim::model_pose::Sample,
    /// Feet-relative offsets in Source world axes; already-sampled remote
    /// tracking is copied directly, never read back from raw authority.
    pub tracked: Option<sim::TrackedPose>,
    /// Independent admitted weapon ray; never inferred from the head/camera.
    pub aim: Option<sim::AimRay>,
    alive: bool,
    life: Option<u64>,
}
impl Pose {
    /// Compatibility for callers without a clock; capture paths use player_at.
    pub fn player(p: &sim::Player) -> Self {
        Self::player_at(p, 0, sim::DEFAULT_TICK_RATE)
    }
    pub fn player_at(p: &sim::Player, tick: u64, rate: u32) -> Self {
        Self { velocity: p.movement.velocity, on_ground: p.movement.on_ground, origin: p.movement.origin, eye: p.eye(), angles: p.view_angles, punch: p.view_punch().into(), alive: p.alive, life:Some(p.life_epoch),tracked:p.tracked,aim:p.aim,
            animation:sim::model_pose::sample(p,tick,rate) }
    }
    pub fn remote(p: &netcode::remote_pose::RenderPose) -> Self {
        Self { origin: p.origin, velocity: p.velocity, on_ground: p.on_ground,
            eye: p.eye, angles: p.view_angles, punch: p.punch, alive: p.alive,
            tracked:p.tracked,aim:p.aim,life:None,animation:p.animation }
    }
    fn lerp(self, next: Self, alpha: f32) -> Self {
        let same_life=self.life==next.life && self.alive==next.alive;
        // A respawn is a new camera/weapon timeline even when no rendered
        // frame observed the dead actor. Never blend its body or eye with
        // the previous life; tracking/aim still pass their validation below.
        let previous = if same_life { self } else { next };
        Self {
            velocity: previous.velocity.lerp(next.velocity, alpha), on_ground: next.on_ground,
            origin: previous.origin.lerp(next.origin, alpha), eye: previous.eye.lerp(next.eye, alpha),
            angles: glam::Vec3::new(angle_lerp(previous.angles.x, next.angles.x, alpha), angle_lerp(previous.angles.y, next.angles.y, alpha), 0.0),
            punch: previous.punch.lerp(next.punch, alpha), alive: next.alive,
            animation: if same_life { self.animation.interpolate(next.animation,alpha) } else { next.animation },
            tracked:blend_tracked(self.tracked,next.tracked,alpha,same_life),
            aim:blend_aim(self.aim,next.aim,alpha,same_life),life:next.life,
        }
    }
}
fn angle_lerp(a: f32, b: f32, alpha: f32) -> f32 { a + ((b - a + 180.0).rem_euclid(360.0) - 180.0) * alpha }

fn valid_tracking(mut tracked:sim::TrackedPose)->Option<sim::TrackedPose> {
    for pose in [&mut tracked.head,&mut tracked.left_hand,&mut tracked.right_hand] {
        let norm=pose.rotation.length_squared();
        if !pose.position.is_finite() || !pose.position.length_squared().is_finite()
            || !pose.rotation.is_finite() || !norm.is_finite() || norm<0.0001 {return None;}
        pose.rotation=pose.rotation.normalize();
    }
    Some(tracked)
}
fn blend_tracked(a:Option<sim::TrackedPose>,b:Option<sim::TrackedPose>,t:f32,same_life:bool)->Option<sim::TrackedPose> {
    let b=valid_tracking(b?)?;
    let Some(a)=a.and_then(valid_tracking).filter(|_|same_life) else {return Some(b)};
    let blend=|a:sim::Pose,b:sim::Pose|sim::Pose {position:a.position.lerp(b.position,t),rotation:a.rotation.slerp(b.rotation,t).normalize()};
    Some(sim::TrackedPose {head:blend(a.head,b.head),left_hand:blend(a.left_hand,b.left_hand),right_hand:blend(a.right_hand,b.right_hand)})
}
fn valid_aim(mut aim:sim::AimRay)->Option<sim::AimRay> {
    if !aim.origin.is_finite() || !aim.origin.length_squared().is_finite() || !aim.direction.is_finite(){return None;}
    aim.direction=aim.direction.try_normalize()?; Some(aim)
}
fn blend_aim(a:Option<sim::AimRay>,b:Option<sim::AimRay>,t:f32,same_life:bool)->Option<sim::AimRay> {
    let b=valid_aim(b?)?;
    let Some(a)=a.and_then(valid_aim).filter(|_|same_life) else {return Some(b)};
    let arc=glam::Quat::from_rotation_arc(a.direction,b.direction);
    Some(sim::AimRay {origin:a.origin.lerp(b.origin,t),direction:(glam::Quat::IDENTITY.slerp(arc,t)*a.direction).normalize()})
}

#[cfg(test)]
#[path="interpolation_tracked_tests.rs"]
mod tracked_tests;

#[derive(Resource, Default)]
pub struct RenderPoses(HashMap<sim::PlayerId, (Pose, Pose)>, HashMap<sim::PlayerId, f32>, HashMap<sim::PlayerId, Pose>, Option<PredictedLocal>, TrackingSource);

#[derive(Default, Debug, PartialEq, Eq)]
enum TrackingSource {
    #[default]
    Offline,
    Sampled,
    // A cancelled remote scene may still retain authoritative actors. Only
    // fresh offline capture or an explicit sampled override may restore data.
    Cleared,
}

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
        self.4=TrackingSource::Sampled;
    }
    pub fn clear_overrides(&mut self) {
        self.2.clear(); self.3=None; self.4=TrackingSource::Cleared;
        // Old offline tracking must not cross a remote/session cancellation.
        // Preserve the established flat eye/body interpolation and stair data.
        for (a,b) in self.0.values_mut() {a.tracked=None;a.aim=None;b.tracked=None;b.aim=None;}
    }
    /// Preserve the existing stair-eye easing for local prediction without
    /// capturing or changing any authoritative/remote actor state.
    pub fn capture_prediction(&mut self, player: &sim::Player, tick: u64, dt: f32) -> Pose {
        // Existing callers pass the predictor's fixed dt; recover its tick rate.
        self.capture_prediction_at(player,tick,(1.0/dt).round() as u32)
    }
    pub fn capture_prediction_at(&mut self, player: &sim::Player, tick: u64, rate: u32) -> Pose {
        let dt = 1.0 / rate as f32;
        let raw = Pose::player_at(player,tick,rate);
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
        if self.4==TrackingSource::Cleared {self.4=TrackingSource::Offline;}
        let round_start = world.events.iter().any(|e| matches!(e, sim::GameEvent::RoundStart { .. }));
        self.0.retain(|id, _| world.player(*id).is_some());
        self.1.retain(|id, _| world.player(*id).is_some());
        for p in &world.players {
            let mut current = Pose::player_at(p,world.tick,world.tick_rate);
            let pair = self.0.entry(p.id).or_insert((current, current));
            let reset = round_start || pair.1.life != current.life
                || pair.1.alive != current.alive || pair.1.origin.distance(current.origin) > 128.0;
            let offset = self.1.entry(p.id).or_default();
            *offset = stair_offset(*offset, current.origin.z - pair.1.origin.z, current.on_ground && pair.1.on_ground, reset, world.dt());
            current.eye.z += *offset;
            *pair = (if reset { current } else { pair.1 }, current);
        }
    }
    /// Compatibility for callers without a clock; sampled pairs retain theirs.
    pub fn sample(&self, player: &sim::Player, alpha: f32) -> Pose {
        self.sample_at(player,alpha,0,sim::DEFAULT_TICK_RATE)
    }
    /// Provide the real clock for an actor not yet present in the render history.
    pub fn sample_at(&self, player: &sim::Player, alpha: f32, tick: u64, rate: u32) -> Pose {
        if let Some(pose) = self.2.get(&player.id) { return *pose; }
        let mut pose=self.0.get(&player.id).map_or_else(|| Pose::player_at(player,tick,rate), |&(a, b)| a.lerp(b, alpha.clamp(0.0, 1.0)));
        // Newly joined actors may be absent from the sampled timeline, and
        // cancellation may leave authority alive until scene teardown. Neither
        // gap permits raw tracking; only fresh capture restores that fallback.
        if self.4!=TrackingSource::Offline {pose.tracked=None;pose.aim=None;}
        pose
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nearby_respawn_discards_previous_eye_motion_without_a_dead_frame() {
        for rate in [64, 128] {
            let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
            world.set_tick_rate(rate);
            let id = world.add_player("Local", sim::Team::CounterTerrorist);
            world.events.clear();
            world.player_mut(id).unwrap().movement.on_ground = true;
            let mut history = RenderPoses::default();
            history.capture(&world);

            // Leave a real stair-eye offset and a moving/turned old pose.
            world.tick += 1;
            let actor = world.player_mut(id).unwrap();
            actor.movement.origin.z += 18.;
            actor.movement.velocity = glam::Vec3::X * 117.;
            actor.view_angles.y = 179.;
            history.capture(&world);
            let actor = world.player(id).unwrap();
            let previous = history.sample(actor, 1.);
            assert!(previous.eye.z < actor.eye().z - 15.);

            // An alive-to-alive respawn nearby is neither a large teleport nor
            // a RoundStart event. The life epoch is the only reset signal.
            world.spawns = vec![sim::SpawnPoint {
                origin: glam::Vec3::new(4., 0., 18.), yaw: -179.,
                team: sim::Team::CounterTerrorist,
            }];
            world.respawn(id);
            world.tick += 1;
            let actor = world.player_mut(id).unwrap();
            actor.movement.on_ground = true;
            let next = Pose::player_at(world.player(id).unwrap(), world.tick, rate);
            assert!(previous.alive && next.alive);
            assert_ne!(previous.life, next.life);
            assert!(previous.origin.distance(next.origin) < 128.);
            let before = world.state();
            history.capture(&world);
            for alpha in [0., 0.25, 0.5, 1.] {
                for pose in [previous.lerp(next, alpha), history.sample(world.player(id).unwrap(), alpha)] {
                    assert_eq!(pose.origin, next.origin);
                    assert_eq!(pose.eye, next.eye);
                    assert_eq!(pose.velocity, next.velocity);
                    assert_eq!(pose.angles, next.angles);
                    assert_eq!(pose.punch, next.punch);
                    assert_eq!(pose.animation, next.animation);
                }
            }
            assert_eq!(world.state(), before);
        }
    }

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
    fn offline_and_prediction_capture_animation_with_their_actual_fixed_clock() {
        for rate in [64, 128] {
            let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
            world.set_tick_rate(rate); let id = world.add_player("Clock", sim::Team::CounterTerrorist);
            world.events.clear(); world.tick = rate as u64;
            let actor = world.player_mut(id).unwrap(); actor.movement.on_ground = true;
            actor.movement.velocity = glam::Vec3::X * 130.; actor.animation.move_cycle = 0.2;
            actor.animation.last_fire_tick = Some(rate as u64 - rate as u64 / 8);
            let mut poses = RenderPoses::default(); poses.capture(&world);
            let first = poses.sample(world.player(id).unwrap(), 1.);
            assert_eq!(first.animation.input.time_since_fire, 0.125);
            world.tick += 1; world.player_mut(id).unwrap().animation.move_cycle = 0.3;
            poses.capture(&world);
            let before = world.state();
            let middle = poses.sample(world.player(id).unwrap(), 0.5);
            assert!((middle.animation.move_cycle - 0.25).abs() < 0.00001);
            let predicted = poses.capture_prediction_at(world.player(id).unwrap(), world.tick, rate);
            assert_eq!(predicted.animation, sim::model_pose::sample(world.player(id).unwrap(), world.tick, rate));
            assert_eq!(world.state(), before);
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
        let a = Pose { velocity: glam::Vec3::ZERO, on_ground: true, origin: glam::Vec3::ZERO, eye: glam::Vec3::Z * 64.0, angles: glam::Vec3::ZERO, punch: glam::Vec2::ZERO, alive: true,life:Some(1),tracked:None,aim:None,
            animation:sim::model_pose::Sample { input:character_pose::AnimInput { speed:0.,move_yaw:0.,on_ground:true,ducked:false,pitch:0.,class:weapons::WeaponClass::Knife,time_since_fire:10.,reload_progress:None,air_time:0. },move_cycle:0.,last_fire_tick:None } };
        let b = Pose { origin: glam::Vec3::X * 4.0, eye: a.eye + glam::Vec3::X * 4.0, angles: glam::Vec3::Y * 8.0, ..a };
        for i in 0..=4 {
            let p = a.lerp(b, i as f32 / 4.0);
            assert_eq!(p.eye.x, i as f32);
            assert_eq!(p.origin.x, p.eye.x);
            assert_eq!(p.angles.y, i as f32 * 2.0);
        }
    }
}

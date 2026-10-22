//! Read-only remote rendering poses. Sampling never steps or restores sim.
use content::ContentId;
use glam::{Quat, Vec2, Vec3};
use sim::{AimRay, Player, PlayerId, Pose, Team, TrackedPose, WorldState};
use std::collections::{BTreeMap, VecDeque};

pub const MAX_HISTORY_FRAMES: usize = 128;

/// Source axes/units. Head/hands and the aim ray remain independent of a camera.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderPose {
    pub origin: Vec3,
    pub velocity: Vec3,
    pub eye: Vec3,
    pub view_angles: Vec3,
    pub punch: Vec2,
    /// Bone animation uses this same delayed snapshot timeline.
    pub animation: sim::model_pose::Sample,
    pub on_ground: bool,
    pub alive: bool,
    pub team: Team,
    pub weapon: Option<ContentId>,
    pub tracked: Option<TrackedPose>,
    pub aim: Option<AimRay>,
    life: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SampleMode {
    Held,
    Interpolated,
    Discontinuity,
    Extrapolated { ticks: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderSample {
    pub pose: RenderPose,
    /// The sampled timeline, capped if requested extrapolation was too large.
    /// Commands must still obey the protocol's last-authoritative-tick bound.
    pub used_tick: f64,
    pub mode: SampleMode,
}

struct Frame {
    tick: u64,
    round: u32,
    players: BTreeMap<PlayerId, RenderPose>,
}

pub struct PoseHistory {
    rate: u32,
    capacity: usize,
    extrapolation: u32,
    teleport_distance: f32,
    frames: VecDeque<Frame>,
}

impl PoseHistory {
    /// Rendering delay and extrapolation are caller policy; no unmeasured
    /// CS:GO timing default is invented here. Extrapolation may be disabled.
    pub fn new(
        rate: u32,
        capacity: usize,
        max_extrapolation_ticks: u32,
        teleport_distance: f32,
    ) -> Result<Self, String> {
        if !sim::TICK_RATE_RANGE.contains(&rate)
            || !(2..=MAX_HISTORY_FRAMES).contains(&capacity)
            || max_extrapolation_ticks > rate
            || !teleport_distance.is_finite()
            || !(0. ..=1e6).contains(&teleport_distance)
            || teleport_distance == 0.
        {
            return Err("invalid pose history bounds".into());
        }
        Ok(Self {
            rate,
            capacity,
            extrapolation: max_extrapolation_ticks,
            teleport_distance,
            frames: VecDeque::new(),
        })
    }
    pub fn len(&self) -> usize {
        self.frames.len()
    }
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    pub fn oldest_tick(&self) -> Option<u64> {
        self.frames.front().map(|f| f.tick)
    }
    pub fn latest_tick(&self) -> Option<u64> {
        self.frames.back().map(|f| f.tick)
    }
    pub fn clear(&mut self) {
        self.frames.clear();
    }

    /// Capture only render data, not inventories, rules, events or bot brains.
    /// Stale/duplicate snapshots are ignored; invalid new data is atomic.
    pub fn push(&mut self, state: &WorldState) -> Result<bool, String> {
        if self.latest_tick().is_some_and(|tick| state.tick <= tick) {
            return Ok(false);
        }
        if state.tick > crate::protocol::MAX_PROTOCOL_COUNTER
            || state.tick_rate != self.rate
            || state.players.len() > 64
        {
            return Err("incompatible remote pose snapshot".into());
        }
        let mut players = BTreeMap::new();
        for player in &state.players {
            if player.id == 0 || players.contains_key(&player.id) {
                return Err("invalid remote player id".into());
            }
            players.insert(player.id, RenderPose::capture(player, state.tick, state.tick_rate)?);
        }
        if self
            .frames
            .back()
            .is_some_and(|f| f.round != state.round.round)
        {
            self.clear();
        }
        self.frames.push_back(Frame {
            tick: state.tick,
            round: state.round.round,
            players,
        });
        while self.frames.len() > self.capacity {
            self.frames.pop_front();
        }
        Ok(true)
    }

    /// Sample at a caller-chosen server/render tick. It may be fractional.
    pub fn sample(
        &self,
        player: PlayerId,
        render_tick: f64,
    ) -> Result<Option<RenderSample>, String> {
        if !render_tick.is_finite()
            || render_tick < 0.
            || render_tick > crate::protocol::MAX_LOCAL_TICK as f64
        {
            return Err("invalid render timeline".into());
        }
        let Some(first) = self.frames.front() else {
            return Ok(None);
        };
        if render_tick <= first.tick as f64 {
            return Ok(first
                .players
                .get(&player)
                .cloned()
                .map(|pose| RenderSample {
                    pose,
                    used_tick: first.tick as f64,
                    mode: SampleMode::Held,
                }));
        }
        let mut older = first;
        for newer in self.frames.iter().skip(1) {
            if render_tick <= newer.tick as f64 {
                let a = older.players.get(&player);
                let b = newer.players.get(&player);
                let (Some(a), Some(b)) = (a, b) else {
                    let pose = if render_tick == newer.tick as f64 {
                        b
                    } else {
                        a
                    };
                    return Ok(pose.cloned().map(|pose| RenderSample {
                        pose,
                        used_tick: render_tick,
                        mode: SampleMode::Held,
                    }));
                };
                if a.alive != b.alive
                    || a.team != b.team
                    || a.life != b.life
                    || a.origin.distance_squared(b.origin)
                        > self.teleport_distance * self.teleport_distance
                {
                    return Ok(Some(RenderSample {
                        pose: b.clone(),
                        used_tick: newer.tick as f64,
                        mode: SampleMode::Discontinuity,
                    }));
                }
                let alpha =
                    ((render_tick - older.tick as f64) / (newer.tick - older.tick) as f64) as f32;
                return Ok(Some(RenderSample {
                    pose: a.interpolate(b, alpha),
                    used_tick: render_tick,
                    mode: SampleMode::Interpolated,
                }));
            }
            older = newer;
        }
        let Some(mut pose) = older.players.get(&player).cloned() else {
            return Ok(None);
        };
        let extra = (render_tick - older.tick as f64).min(self.extrapolation as f64);
        if extra == 0. || !pose.alive {
            return Ok(Some(RenderSample {
                pose,
                used_tick: older.tick as f64,
                mode: SampleMode::Held,
            }));
        }
        let offset = pose.velocity * (extra as f32 / self.rate as f32);
        pose.origin += offset;
        pose.eye += offset;
        if !pose.origin.is_finite() || !pose.eye.is_finite() {
            return Err("remote extrapolation exceeds finite coordinates".into());
        }
        Ok(Some(RenderSample {
            pose,
            used_tick: older.tick as f64 + extra,
            mode: SampleMode::Extrapolated { ticks: extra },
        }))
    }
}

impl RenderPose {
    fn capture(player: &Player, tick: u64, rate: u32) -> Result<Self, String> {
        let mut pose = Self {
            origin: player.movement.origin,
            velocity: player.movement.velocity,
            eye: player.eye(),
            view_angles: player.view_angles.map(wrap_angle),
            punch: player.view_punch().into(),
            animation: sim::model_pose::sample(player, tick, rate),
            on_ground: player.movement.on_ground,
            alive: player.alive,
            team: player.team,
            weapon: player.active_weapon().map(|w| w.def.id.clone()),
            tracked: player.tracked,
            aim: player.aim,
            life: player.life_epoch,
        };
        if !pose.origin.is_finite()
            || !pose.origin.length_squared().is_finite()
            || !pose.velocity.is_finite()
            || !pose.velocity.length_squared().is_finite()
            || !pose.eye.is_finite()
            || !pose.eye.length_squared().is_finite()
            || !pose.view_angles.is_finite()
            || !pose.punch.is_finite()
            || !pose.animation.move_cycle.is_finite()
            || !(0.0..1.0).contains(&pose.animation.move_cycle)
            || !pose.animation.input.speed.is_finite()
            || !pose.animation.input.move_yaw.is_finite()
            || !pose.animation.input.pitch.is_finite()
            || !pose.animation.input.time_since_fire.is_finite()
            || !pose.animation.input.air_time.is_finite()
            || !(0.0..=60.0).contains(&pose.animation.input.air_time)
            || pose.animation.input.reload_progress.is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            || pose.animation.last_fire_tick.is_some_and(|at| at > tick)
            || (pose.alive && pose.life == 0)
        {
            return Err("nonfinite remote pose".into());
        }
        if let Some(tracked) = &mut pose.tracked {
            for p in [
                &mut tracked.head,
                &mut tracked.left_hand,
                &mut tracked.right_hand,
            ] {
                if !p.position.is_finite()
                    || !p.position.length_squared().is_finite()
                    || !p.rotation.is_finite()
                    || !p.rotation.length_squared().is_finite()
                    || p.rotation.length_squared() < 0.0001
                {
                    return Err("invalid tracked render pose".into());
                }
                p.rotation = p.rotation.normalize();
            }
        }
        if let Some(aim) = &mut pose.aim {
            if !aim.origin.is_finite()
                || !aim.origin.length_squared().is_finite()
                || !aim.direction.is_finite()
            {
                return Err("invalid render aim ray".into());
            }
            aim.direction = aim.direction.try_normalize().unwrap_or(Vec3::X);
        }
        Ok(pose)
    }
    fn interpolate(&self, next: &Self, alpha: f32) -> Self {
        let tracked = match (self.tracked, next.tracked) {
            (Some(a), Some(b)) => Some(TrackedPose {
                head: interpolate_pose(a.head, b.head, alpha),
                left_hand: interpolate_pose(a.left_hand, b.left_hand, alpha),
                right_hand: interpolate_pose(a.right_hand, b.right_hand, alpha),
            }),
            _ => next.tracked,
        };
        let aim = match (self.aim, next.aim) {
            (Some(a), Some(b)) => {
                let rotation = Quat::from_rotation_arc(a.direction, b.direction);
                Some(AimRay {
                    origin: a.origin.lerp(b.origin, alpha),
                    direction: (Quat::IDENTITY.slerp(rotation, alpha) * a.direction).normalize(),
                })
            }
            _ => next.aim,
        };
        Self {
            origin: self.origin.lerp(next.origin, alpha),
            velocity: self.velocity.lerp(next.velocity, alpha),
            eye: self.eye.lerp(next.eye, alpha),
            view_angles: Vec3::new(
                angle_lerp(self.view_angles.x, next.view_angles.x, alpha),
                angle_lerp(self.view_angles.y, next.view_angles.y, alpha),
                angle_lerp(self.view_angles.z, next.view_angles.z, alpha),
            ),
            punch: if self.weapon == next.weapon {
                self.punch.lerp(next.punch, alpha)
            } else {
                next.punch
            },
            tracked,
            aim,
            animation: self.animation.interpolate(next.animation, alpha),
            ..next.clone()
        }
    }
}

fn wrap_angle(angle: f32) -> f32 {
    (angle + 180.).rem_euclid(360.) - 180.
}
fn angle_lerp(a: f32, b: f32, alpha: f32) -> f32 {
    wrap_angle(a + wrap_angle(b - a) * alpha)
}
fn interpolate_pose(a: Pose, b: Pose, alpha: f32) -> Pose {
    Pose {
        position: a.position.lerp(b.position, alpha),
        rotation: a.rotation.slerp(b.rotation, alpha).normalize(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::World;
    use std::sync::Arc;
    fn world() -> (World, PlayerId) {
        let mut world = World::new(Arc::new(collision::CollisionWorld::new()));
        let id = world.add_player("Remote", Team::CounterTerrorist);
        (world, id)
    }
    fn history(rate: u32) -> PoseHistory {
        PoseHistory::new(rate, 4, 8, 128.).unwrap()
    }
    #[test]
    fn samples_fractional_position_eye_and_shortest_angle_without_changing_authority() {
        let (mut world, id) = world();
        let mut history = history(128);
        world.players[0].movement.origin = Vec3::ZERO;
        world.players[0].view_angles.y = 359.;
        history.push(&world.state()).unwrap();
        world.tick = 4;
        world.players[0].movement.origin = Vec3::X * 8.;
        world.players[0].view_angles.y = 1.;
        let before = world.state();
        history.push(&before).unwrap();
        let sample = history.sample(id, 2.).unwrap().unwrap();
        assert_eq!(sample.pose.origin, Vec3::X * 4.);
        assert_eq!(sample.pose.eye.x, 4.);
        assert!(sample.pose.view_angles.y.abs() < 0.001);
        assert_eq!(sample.mode, SampleMode::Interpolated);
        assert_eq!(world.state(), before);
    }
    #[test]
    fn animation_uses_delayed_transform_timeline_and_holds_later_stance_and_shots() {
        for rate in [64, 128] {
            let (mut world, id) = world(); world.set_tick_rate(rate); world.tick = rate as u64;
            let actor = &mut world.players[0];
            actor.movement.origin = Vec3::ZERO; actor.movement.on_ground = true;
            actor.movement.velocity = Vec3::X * 130.; actor.view_angles.x = 0.;
            actor.animation.move_cycle = 0.95;
            let mut history = history(rate); history.push(&world.state()).unwrap();
            world.tick += 4;
            let actor = &mut world.players[0];
            actor.movement.origin = Vec3::X * 8.; actor.view_angles.x = 40.;
            actor.animation.move_cycle = 0.05;
            history.push(&world.state()).unwrap();
            let before = world.state();
            let middle = history.sample(id, (world.tick - 2) as f64).unwrap().unwrap().pose;
            assert_eq!(middle.origin.x, 4.);
            assert!(middle.animation.move_cycle.min(1. - middle.animation.move_cycle) < 0.00001);
            assert_eq!(middle.animation.input.pitch, middle.view_angles.x);
            assert_eq!(middle.animation.input.pitch, 20.);
            assert_ne!(middle.animation.move_cycle, world.players[0].animation.move_cycle);
            assert_eq!(world.state(), before);
            world.tick += 4;
            let actor = &mut world.players[0]; actor.movement.ducked = true;
            actor.animation.move_cycle = 0.4; actor.animation.last_fire_tick = Some(world.tick - 1);
            history.push(&world.state()).unwrap();
            let held = history.sample(id, (world.tick - 2) as f64).unwrap().unwrap().pose;
            assert!(!held.animation.input.ducked);
            assert_eq!(held.animation.last_fire_tick, None);
            let exact = history.sample(id, world.tick as f64).unwrap().unwrap().pose;
            assert!(exact.animation.input.ducked);
            assert_eq!(exact.animation.last_fire_tick, Some(world.tick - 1));
            assert_eq!(exact.animation.input.time_since_fire, 1. / rate as f32);
        }
    }

    #[test]
    fn airborne_shot_keeps_pose_clock_on_the_interpolated_transform_timeline() {
        for rate in [64, 128] {
            let (mut world, id) = world();
            world.set_tick_rate(rate);
            world.tick = rate as u64;
            let actor = &mut world.players[0];
            actor.movement.on_ground = false;
            actor.movement.origin = Vec3::ZERO;
            actor.animation.air_time = 0.25;
            let mut history = history(rate);
            history.push(&world.state()).unwrap();

            world.tick += 4;
            let actor = &mut world.players[0];
            actor.movement.origin = Vec3::new(8., 0., 4.);
            actor.animation.air_time += 4. / rate as f32;
            actor.animation.last_fire_tick = Some(world.tick);
            let before = world.state();
            history.push(&before).unwrap();
            for offset in [1., 2., 3.] {
                let sample = history
                    .sample(id, (world.tick - 4) as f64 + offset)
                    .unwrap()
                    .unwrap();
                assert_eq!(sample.mode, SampleMode::Interpolated);
                assert_eq!(
                    sample.pose.origin,
                    Vec3::new(2., 0., 1.) * offset as f32
                );
                assert_eq!(
                    sample.pose.animation.input.air_time,
                    0.25 + offset as f32 / rate as f32
                );
                assert_eq!(sample.pose.animation.last_fire_tick, None);
            }
            assert_eq!(
                history.sample(id, world.tick as f64).unwrap().unwrap()
                    .pose.animation.last_fire_tick,
                Some(world.tick)
            );
            assert_eq!(world.state(), before);
        }
    }

    #[test]
    fn successive_remote_shots_keep_locomotion_on_the_transform_timeline() {
        for rate in [64, 128] {
            let (mut world, id) = world();
            world.set_tick_rate(rate);
            let mut history = history(rate);
            let mut previous: Option<RenderPose> = None;
            for shot in 0..3 {
                world.tick = rate as u64 + shot * 8;
                let actor = &mut world.players[0];
                actor.movement.on_ground = true;
                actor.movement.origin = Vec3::X * (shot as f32 * 8.);
                let yaw = 170. + shot as f32 * 20.;
                let speed = 100. + shot as f32 * 40.;
                actor.movement.velocity =
                    Vec3::new(yaw.to_radians().cos(), yaw.to_radians().sin(), 0.) * speed;
                actor.view_angles = Vec3::new(-20. + shot as f32 * 20., 0., 0.);
                actor.animation.move_cycle = [0.95, 0.05, 0.15][shot as usize];
                actor.animation.last_fire_tick = Some(world.tick - [2, 0, 1][shot as usize]);
                let next = RenderPose::capture(actor, world.tick, rate).unwrap();
                let before = world.state();
                history.push(&before).unwrap();
                if let Some(older) = previous {
                    for alpha in [0.25, 0.5, 0.75] {
                        let tick = (world.tick - 8) as f64 + 8. * alpha as f64;
                        let sample = history.sample(id, tick).unwrap().unwrap();
                        assert_eq!(sample.mode, SampleMode::Interpolated);
                        assert_eq!(sample.pose.origin, older.origin.lerp(next.origin, alpha));
                        let animation = sample.pose.animation;
                        let cycle = (older.animation.move_cycle + 0.1 * alpha).rem_euclid(1.);
                        assert!((animation.move_cycle - cycle).abs() < 0.00001);
                        assert!(
                            (animation.input.speed - (older.animation.input.speed + 40. * alpha))
                                .abs() < 0.0001
                        );
                        let expected_yaw = 170. + (shot - 1) as f32 * 20. + 20. * alpha;
                        assert!(wrap_angle(animation.input.move_yaw - expected_yaw).abs() < 0.0001);
                        assert_eq!(animation.input.pitch, sample.pose.view_angles.x);
                        assert_eq!(
                            animation.input.pitch,
                            older.animation.input.pitch + 20. * alpha
                        );
                        assert_eq!(animation.last_fire_tick, older.animation.last_fire_tick);
                        assert_eq!(
                            animation.input.time_since_fire,
                            older.animation.input.time_since_fire
                        );
                    }
                    assert_eq!(
                        history.sample(id, world.tick as f64).unwrap().unwrap()
                            .pose.animation,
                        next.animation
                    );
                }
                assert_eq!(world.state(), before);
                previous = Some(next);
            }
        }
    }

    #[test]
    fn remote_shots_preserve_stance_and_class_pose_boundaries() {
        let (world, _) = world();
        let older = RenderPose::capture(&world.players[0], world.tick, 128).unwrap();
        for stance in [false, true] {
            let mut newer = older.clone();
            newer.origin.x += 8.;
            newer.animation.move_cycle = 0.25;
            newer.animation.input.speed = 200.;
            newer.animation.input.move_yaw = 90.;
            newer.animation.input.pitch = 40.;
            newer.animation.last_fire_tick = Some(4);
            newer.animation.input.time_since_fire = 0.;
            if stance {
                newer.animation.input.ducked = !older.animation.input.ducked;
            } else {
                newer.animation.input.class = weapons::WeaponClass::Rifle;
                assert_ne!(newer.animation.input.class, older.animation.input.class);
            }
            for alpha in [0., 0.25, 0.5, 0.75] {
                let held = older.interpolate(&newer, alpha);
                assert_eq!(held.animation, older.animation);
                assert_eq!(held.origin.x, older.origin.x + 8. * alpha);
            }
            assert_eq!(older.interpolate(&newer, 1.).animation, newer.animation);
        }
    }

    #[test]
    fn invalid_animation_snapshots_do_not_replace_the_sampled_history() {
        let (mut world, _) = world(); let mut history = history(128);
        history.push(&world.state()).unwrap(); world.tick = 4;
        for invalid in 0..3 {
            world.players[0].animation = sim::model_pose::PoseState::default();
            match invalid {
                0 => world.players[0].animation.move_cycle = f32::NAN,
                1 => world.players[0].animation.air_time = f32::INFINITY,
                _ => world.players[0].animation.last_fire_tick = Some(world.tick + 1),
            }
            assert!(history.push(&world.state()).is_err());
            assert_eq!(history.latest_tick(), Some(0));
            assert_eq!(history.len(), 1);
        }
    }

    #[test]
    fn extrapolation_is_capped_by_ticks_and_scales_at_64_and_128() {
        for rate in [64, 128] {
            let (mut world, id) = world();
            world.set_tick_rate(rate);
            world.players[0].movement.origin = Vec3::ZERO;
            world.players[0].movement.velocity = Vec3::X * 128.;
            let mut history = history(rate);
            history.push(&world.state()).unwrap();
            let sample = history.sample(id, 1000.).unwrap().unwrap();
            assert_eq!(sample.used_tick, 8.);
            assert_eq!(sample.pose.origin.x, 128. * 8. / rate as f32);
            assert_eq!(sample.mode, SampleMode::Extrapolated { ticks: 8. });
        }
    }
    #[test]
    fn teleport_respawn_and_team_changes_snap_instead_of_blending_lives() {
        for change in 0..3 {
            let (mut world, id) = world();
            let mut history = history(128);
            world.players[0].movement.origin = Vec3::ZERO;
            history.push(&world.state()).unwrap();
            world.tick = 4;
            world.players[0].movement.origin = Vec3::X * if change == 0 { 512. } else { 10. };
            if change == 1 {
                world.players[0].life_epoch += 1;
            }
            if change == 2 {
                world.players[0].team = Team::Terrorist;
            }
            history.push(&world.state()).unwrap();
            let sample = history.sample(id, 2.).unwrap().unwrap();
            assert_eq!(sample.pose.origin, world.players[0].movement.origin);
            assert_eq!(sample.mode, SampleMode::Discontinuity);
        }
    }
    #[test]
    fn lost_death_snapshot_still_detects_the_real_respawn_epoch() {
        let (mut world, id) = world();
        let mut history = history(128);
        history.push(&world.state()).unwrap();
        world.damage_environment(id, 1000, "fixture");
        assert!(!world.player(id).unwrap().alive);
        world.tick = 256;
        world.step(&[]);
        assert!(world.player(id).unwrap().alive);
        history.push(&world.state()).unwrap();
        assert_eq!(
            history.sample(id, 128.).unwrap().unwrap().mode,
            SampleMode::Discontinuity
        );
    }
    #[test]
    fn dead_players_are_held_and_actor_removal_is_not_invented() {
        let (mut world, id) = world();
        let mut history = history(128);
        world.players[0].alive = false;
        world.players[0].movement.velocity = Vec3::X * 100.;
        history.push(&world.state()).unwrap();
        let sample = history.sample(id, 100.).unwrap().unwrap();
        assert_eq!(sample.mode, SampleMode::Held);
        assert_eq!(sample.pose.origin, world.players[0].movement.origin);
        world.tick = 4;
        world.remove_player(id);
        history.push(&world.state()).unwrap();
        assert!(history.sample(id, 4.).unwrap().is_none());
        assert!(history.sample(999, 2.).unwrap().is_none());
    }
    #[test]
    fn new_round_clears_old_poses_and_old_packets_cannot_rewind_history() {
        let (mut world, _) = world();
        let mut history = history(128);
        let old = world.state();
        history.push(&old).unwrap();
        for tick in 1..=8 {
            world.tick = tick;
            history.push(&world.state()).unwrap();
        }
        assert_eq!(history.len(), 4);
        assert_eq!(history.oldest_tick(), Some(5));
        assert!(!history.push(&old).unwrap());
        world.tick = 9;
        world.round.round += 1;
        history.push(&world.state()).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history.oldest_tick(), Some(9));
    }
    #[test]
    fn tracked_quaternion_and_independent_opposite_aim_interpolate_without_nan() {
        let (mut world, id) = world();
        let mut history = history(128);
        let pose = Pose {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        };
        world.players[0].tracked = Some(TrackedPose {
            head: pose,
            left_hand: pose,
            right_hand: pose,
        });
        world.players[0].aim = Some(AimRay {
            origin: Vec3::ZERO,
            direction: Vec3::X,
        });
        history.push(&world.state()).unwrap();
        world.tick = 4;
        let rotated = Pose {
            position: Vec3::X * 10.,
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        };
        world.players[0].tracked = Some(TrackedPose {
            head: rotated,
            left_hand: rotated,
            right_hand: rotated,
        });
        world.players[0].aim = Some(AimRay {
            origin: Vec3::Y * 4.,
            direction: -Vec3::X,
        });
        history.push(&world.state()).unwrap();
        let sample = history.sample(id, 2.).unwrap().unwrap();
        let head = sample.pose.tracked.unwrap().head;
        assert_eq!(head.position.x, 5.);
        assert!((head.rotation * Vec3::X).x > 0.7);
        assert!((head.rotation * Vec3::X).y > 0.7);
        let aim = sample.pose.aim.unwrap();
        assert_eq!(aim.origin, Vec3::Y * 2.);
        assert!(aim.direction.is_finite());
        assert!((aim.direction.length() - 1.).abs() < 0.001);
    }
    #[test]
    fn malformed_quaternions_and_new_snapshots_are_rejected_atomically() {
        let (mut world, id) = world();
        let mut history = history(128);
        history.push(&world.state()).unwrap();
        world.tick = 4;
        let zero = Pose {
            position: Vec3::ZERO,
            rotation: Quat::from_xyzw(0., 0., 0., 0.),
        };
        world.players[0].tracked = Some(TrackedPose {
            head: zero,
            left_hand: zero,
            right_hand: zero,
        });
        assert!(history.push(&world.state()).is_err());
        assert_eq!(history.len(), 1);
        assert_eq!(history.latest_tick(), Some(0));
        assert!(history.sample(id, f64::NAN).is_err());
        assert!(history.sample(id, -1.).is_err());
        assert!(history.sample(id, u64::MAX as f64).is_err());
        world.tick = u64::MAX;
        world.players[0].tracked = None;
        assert!(history.push(&world.state()).is_err());
        assert_eq!(history.latest_tick(), Some(0));
    }
    #[test]
    fn antipodal_quaternions_do_not_spin_equivalent_head_orientations() {
        let (mut world, id) = world();
        let mut history = history(128);
        let pose = Pose {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        };
        world.players[0].tracked = Some(TrackedPose {
            head: pose,
            left_hand: pose,
            right_hand: pose,
        });
        history.push(&world.state()).unwrap();
        world.tick = 4;
        let pose = Pose {
            rotation: -Quat::IDENTITY,
            ..pose
        };
        world.players[0].tracked = Some(TrackedPose {
            head: pose,
            left_hand: pose,
            right_hand: pose,
        });
        history.push(&world.state()).unwrap();
        let q = history
            .sample(id, 2.)
            .unwrap()
            .unwrap()
            .pose
            .tracked
            .unwrap()
            .head
            .rotation;
        assert!(q.dot(Quat::IDENTITY).abs() > 0.9999);
    }

    #[test]
    fn finite_coordinates_that_overflow_vector_arithmetic_are_rejected() {
        let (mut world, _) = world();
        let mut history = history(128);
        history.push(&world.state()).unwrap();
        world.tick = 4;
        world.players[0].movement.origin.x = f32::MAX;
        assert!(history.push(&world.state()).is_err());
        assert_eq!(history.latest_tick(), Some(0));
        world.players[0].movement.origin.x = 0.;
        world.players[0].movement.velocity.x = f32::MAX;
        assert!(history.push(&world.state()).is_err());
        assert_eq!(history.len(), 1);
    }
}

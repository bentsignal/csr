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
    pub on_ground: bool,
    pub alive: bool,
    pub team: Team,
    pub weapon: Option<ContentId>,
    pub tracked: Option<TrackedPose>,
    pub aim: Option<AimRay>,
    life: f32,
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
        if state.tick_rate != self.rate || state.players.len() > 64 {
            return Err("incompatible remote pose snapshot".into());
        }
        let mut players = BTreeMap::new();
        for player in &state.players {
            if player.id == 0 || players.contains_key(&player.id) {
                return Err("invalid remote player id".into());
            }
            players.insert(player.id, RenderPose::capture(player)?);
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
        if !render_tick.is_finite() || render_tick < 0. {
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
    fn capture(player: &Player) -> Result<Self, String> {
        let mut pose = Self {
            origin: player.movement.origin,
            velocity: player.movement.velocity,
            eye: player.eye(),
            view_angles: player.view_angles.map(wrap_angle),
            punch: player.view_punch().into(),
            on_ground: player.movement.on_ground,
            alive: player.alive,
            team: player.team,
            weapon: player.active_weapon().map(|w| w.def.id.clone()),
            tracked: player.tracked,
            aim: player.aim,
            life: player.respawn_at,
        };
        if !pose.origin.is_finite()
            || !pose.origin.length_squared().is_finite()
            || !pose.velocity.is_finite()
            || !pose.velocity.length_squared().is_finite()
            || !pose.eye.is_finite()
            || !pose.eye.length_squared().is_finite()
            || !pose.view_angles.is_finite()
            || !pose.punch.is_finite()
            || !pose.life.is_finite()
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
                world.players[0].respawn_at = 2.;
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

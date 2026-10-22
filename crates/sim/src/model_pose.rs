//! Tick-owned character poses and installed-model hit geometry.
//! Assets are shared map data; only PoseState travels in player snapshots.
use crate::{Player, Team, World};
use character_pose::{AnimInput, AnimState, PlayerAnimSet};
use content::ContentId;
use glam::{Mat4, Vec3};
use mdl::hitbox::{HitboxSet, PosedHitbox};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PoseState {
    pub move_cycle: f32,
    pub air_time: f32,
    pub last_fire_tick: Option<u64>,
}

pub struct Profile {
    pub id: ContentId,
    pub animations: Arc<PlayerAnimSet>,
    pub hitboxes: HitboxSet,
}

#[derive(Default)]
pub struct Profiles {
    pub terrorist: Option<Profile>,
    pub counter_terrorist: Option<Profile>,
}
impl Profiles {
    pub fn for_team(&self, team: Team) -> Option<&Profile> {
        match team {
            Team::Terrorist => self.terrorist.as_ref(),
            Team::CounterTerrorist => self.counter_terrorist.as_ref(),
            Team::Spectator => None,
        }
    }
}

/// Identical input for the authoritative evaluator and the drawn character.
/// This preserves the current animation controller; native animation parity is separate.
pub fn input(player: &Player, tick: u64, rate: u32) -> AnimInput {
    let velocity = player.movement.velocity;
    let speed = velocity.truncate().length();
    let yaw = velocity.y.atan2(velocity.x).to_degrees();
    let weapon = player.active_weapon();
    AnimInput {
        speed,
        move_yaw: if speed > 1. {
            (yaw - player.view_angles.y + 180.).rem_euclid(360.) - 180.
        } else {
            0.
        },
        on_ground: player.movement.on_ground,
        ducked: player.movement.ducked,
        pitch: player.view_angles.x,
        class: weapon.map_or(weapons::WeaponClass::Knife, |w| w.def.class),
        time_since_fire: player.animation.last_fire_tick.map_or(10., |at| {
            (tick.saturating_sub(at) as f32 / rate as f32).min(0.6)
        }),
        reload_progress: weapon.and_then(|w| w.reload_fraction(tick as f32 / rate as f32)),
        air_time: player.animation.air_time,
    }
}

/// Animation data retained on the same timeline as interpolated actor transforms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub input: AnimInput,
    pub move_cycle: f32,
    pub last_fire_tick: Option<u64>,
}
pub fn sample(player: &Player, tick: u64, rate: u32) -> Sample {
    Sample {
        input: input(player, tick, rate),
        move_cycle: player.animation.move_cycle,
        last_fire_tick: player.animation.last_fire_tick,
    }
}
impl Sample {
    pub fn interpolate(self, next: Self, alpha: f32) -> Self {
        if alpha >= 1. {
            return next;
        }
        let t = alpha.clamp(0., 1.);
        // Stance, class, reload and ground transitions hold the locomotion pose.
        if self.input.on_ground != next.input.on_ground
            || self.input.ducked != next.input.ducked
            || self.input.class != next.input.class
            || self.input.reload_progress.is_some() != next.input.reload_progress.is_some()
        {
            let mut held = self;
            // Gesture/stance changes do not stop time within the same flight.
            // Holding this clock while the actor's transform interpolates makes
            // airborne legs pause on every shot (or crouch/reload transition).
            // Keep takeoff/landing discrete so no later flight is shown early.
            if !self.input.on_ground && !next.input.on_ground {
                held.input.air_time =
                    self.input.air_time + (next.input.air_time - self.input.air_time) * t;
            }
            return held;
        }
        let mut value = self;
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        value.input.speed = lerp(self.input.speed, next.input.speed);
        value.input.move_yaw = self.input.move_yaw
            + ((next.input.move_yaw - self.input.move_yaw + 180.).rem_euclid(360.) - 180.) * t;
        value.input.pitch = lerp(self.input.pitch, next.input.pitch);
        // A new shot holds only its receipt/gesture clock until its frame;
        // locomotion must keep following the interpolated actor transform.
        if self.last_fire_tick == next.last_fire_tick {
            value.input.time_since_fire = lerp(self.input.time_since_fire, next.input.time_since_fire);
        }
        value.input.air_time = lerp(self.input.air_time, next.input.air_time);
        value.input.reload_progress = self
            .input
            .reload_progress
            .zip(next.input.reload_progress)
            .map(|(a, b)| lerp(a, b));
        let delta = (next.move_cycle - self.move_cycle + 0.5).rem_euclid(1.) - 0.5;
        value.move_cycle = (self.move_cycle + delta * t).rem_euclid(1.);
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn airborne() -> Sample {
        Sample {
            input: AnimInput {
                speed: 130.,
                move_yaw: 0.,
                on_ground: false,
                ducked: false,
                pitch: 0.,
                class: weapons::WeaponClass::Rifle,
                time_since_fire: 0.2,
                reload_progress: None,
                air_time: 0.25,
            },
            move_cycle: 0.3,
            last_fire_tick: Some(1),
        }
    }

    #[test]
    fn airborne_clock_continues_while_later_animation_events_are_held() {
        for rate in [64, 128] {
            let older = airborne();
            for event in 0..4 {
                let mut newer = older;
                newer.input.air_time += 4. / rate as f32;
                match event {
                    0 => {
                        newer.last_fire_tick = Some(5);
                        newer.input.time_since_fire = 0.;
                    }
                    1 => newer.input.ducked = true,
                    2 => newer.input.class = weapons::WeaponClass::Pistol,
                    _ => newer.input.reload_progress = Some(0.),
                }
                for alpha in [0., 0.25, 0.5, 0.75] {
                    let mut expected = older;
                    expected.input.air_time += (4. / rate as f32) * alpha;
                    assert_eq!(older.interpolate(newer, alpha), expected);
                }
                assert_eq!(older.interpolate(newer, 1.), newer);
            }
        }
    }

    #[test]
    fn ground_transitions_hold_the_clock_until_their_frame() {
        let air = airborne();
        let mut ground = air;
        ground.input.on_ground = true;
        ground.input.air_time = 0.;
        for (older, newer) in [(ground, air), (air, ground)] {
            for alpha in [0., 0.25, 0.5, 0.75] {
                assert_eq!(older.interpolate(newer, alpha), older);
            }
            assert_eq!(older.interpolate(newer, 1.), newer);
        }
    }

    #[test]
    fn successive_shots_interpolate_locomotion_but_hold_the_fire_receipt() {
        for rate in [64, 128] {
            for grounded in [false, true] {
                let mut older = airborne();
                older.input.on_ground = grounded;
                if grounded {
                    older.input.air_time = 0.;
                }
                older.input.speed = 100.;
                older.input.move_yaw = 170.;
                older.input.pitch = -20.;
                older.move_cycle = 0.95;
                older.last_fire_tick = Some(rate as u64);
                for shot in 1..=2 {
                    let mut newer = older;
                    newer.last_fire_tick = Some(rate as u64 + shot * 8);
                    newer.input.time_since_fire = 0.;
                    newer.input.speed += 40.;
                    newer.input.move_yaw = if shot == 1 { -170. } else { -150. };
                    newer.input.pitch += 20.;
                    if !grounded {
                        newer.input.air_time += 8. / rate as f32;
                    }
                    newer.move_cycle = if shot == 1 { 0.05 } else { 0.15 };
                    for alpha in [0., 0.25, 0.5, 0.75] {
                        let value = older.interpolate(newer, alpha);
                        let cycle = (older.move_cycle + 0.1 * alpha).rem_euclid(1.);
                        assert!((value.move_cycle - cycle).abs() < 0.00001);
                        assert_eq!(value.input.speed, older.input.speed + 40. * alpha);
                        assert_eq!(value.input.move_yaw, older.input.move_yaw + 20. * alpha);
                        assert_eq!(value.input.pitch, older.input.pitch + 20. * alpha);
                        assert_eq!(value.last_fire_tick, older.last_fire_tick);
                        assert_eq!(value.input.time_since_fire, older.input.time_since_fire);
                        assert_eq!(value.input.on_ground, grounded);
                        assert_eq!(value.input.ducked, older.input.ducked);
                        assert_eq!(value.input.class, older.input.class);
                        assert_eq!(value.input.reload_progress, older.input.reload_progress);
                    }
                    assert_eq!(older.interpolate(newer, 1.), newer);
                    older = newer;
                    older.input.time_since_fire = 7. / rate as f32;
                }
            }
        }
    }

    #[test]
    fn simultaneous_shots_do_not_blend_across_pose_boundaries() {
        for boundary in 0..6 {
            let mut older = airborne();
            older.input.on_ground = boundary != 5;
            if boundary == 3 {
                older.input.reload_progress = Some(0.5);
            }
            let mut newer = older;
            newer.last_fire_tick = Some(5);
            newer.input.time_since_fire = 0.;
            newer.move_cycle = 0.45;
            newer.input.speed = 200.;
            newer.input.move_yaw = 90.;
            newer.input.pitch = 40.;
            match boundary {
                0 => newer.input.ducked = true,
                1 => newer.input.class = weapons::WeaponClass::Pistol,
                2 => newer.input.reload_progress = Some(0.),
                3 => newer.input.reload_progress = None,
                _ => newer.input.on_ground = !older.input.on_ground,
            }
            for alpha in [0., 0.25, 0.5, 0.75] {
                assert_eq!(older.interpolate(newer, alpha), older);
            }
            assert_eq!(older.interpolate(newer, 1.), newer);
        }
    }
}

pub(crate) fn posed(
    profile: &Profile,
    player: &Player,
    tick: u64,
    rate: u32,
) -> Option<Arc<[PosedHitbox]>> {
    if !player.alive {
        return None;
    }
    let mut state = AnimState {
        move_cycle: player.animation.move_cycle,
    };
    let pose = profile
        .animations
        .evaluate_prepared(&mut state, &input(player, tick, rate), 0.);
    let transform = Mat4::from_rotation_translation(
        glam::Quat::from_rotation_z(player.view_angles.y.to_radians()),
        player.movement.origin,
    );
    let world_bones = pose
        .into_iter()
        .map(|bone| transform * bone)
        .collect::<Vec<_>>();
    Some(Arc::from(profile.hitboxes.pose(&world_bones).expect(
        "validated character hitboxes require finite rigid simulation poses",
    )))
}

#[derive(Clone)]
pub(crate) struct CachedHitboxes {
    input: AnimInput,
    cycle: f32,
    origin: Vec3,
    yaw: f32,
    team: Team,
    pub shapes: Arc<[PosedHitbox]>,
}

impl World {
    #[cfg(test)]
    pub(crate) fn set_test_hitboxes(&mut self, id: crate::PlayerId, shapes: Vec<PosedHitbox>) {
        let player = self.player(id).unwrap();
        let value = CachedHitboxes {
            input: input(player, self.tick, self.tick_rate),
            cycle: player.animation.move_cycle,
            origin: player.movement.origin,
            yaw: player.view_angles.y,
            team: player.team,
            shapes: shapes.into(),
        };
        self.character_hitboxes.insert(id, value);
    }
    /// Attach runtime map/content-pack profiles. Geometry never comes from a client packet.
    pub fn set_character_profiles(&mut self, profiles: Arc<Profiles>) {
        self.character_profiles = profiles;
        self.character_hitboxes.clear();
        self.rewind = None;
    }
    pub fn character_profiles(&self) -> Arc<Profiles> {
        self.character_profiles.clone()
    }
    pub fn character_profile(&self, team: Team) -> Option<&Profile> {
        self.character_profiles.for_team(team)
    }
    pub(crate) fn refresh_character_hitboxes(&mut self, index: usize) {
        let player = &self.players[index];
        if !player.alive || self.character_profiles.for_team(player.team).is_none() {
            self.character_hitboxes.remove(&player.id);
            return;
        }
        let input = input(player, self.tick, self.tick_rate);
        if self
            .character_hitboxes
            .get(&player.id)
            .is_some_and(|cached| {
                cached.input == input
                    && cached.cycle == player.animation.move_cycle
                    && cached.origin == player.movement.origin
                    && cached.yaw == player.view_angles.y
                    && cached.team == player.team
            })
        {
            return;
        }
        let profile = self.character_profiles.for_team(player.team).unwrap();
        let shapes = posed(profile, player, self.tick, self.tick_rate).unwrap();
        self.character_hitboxes.insert(
            player.id,
            CachedHitboxes {
                input,
                cycle: player.animation.move_cycle,
                origin: player.movement.origin,
                yaw: player.view_angles.y,
                team: player.team,
                shapes,
            },
        );
    }
    pub(crate) fn advance_character_clocks(&mut self) {
        let dt = self.dt();
        for player in &mut self.players {
            if !player.alive {
                continue;
            }
            player.animation.air_time = if player.movement.on_ground {
                0.
            } else {
                (player.animation.air_time + dt).min(60.)
            };
            let mut state = AnimState {
                move_cycle: player.animation.move_cycle,
            };
            state.advance(&input(player, self.tick, self.tick_rate), dt);
            player.animation.move_cycle = state.move_cycle;
        }
    }
}

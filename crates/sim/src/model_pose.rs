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
        // Do not expose a later shot, stance, class or reload before its frame.
        if self.input.on_ground != next.input.on_ground
            || self.input.ducked != next.input.ducked
            || self.input.class != next.input.class
            || self.last_fire_tick != next.last_fire_tick
            || self.input.reload_progress.is_some() != next.input.reload_progress.is_some()
        {
            return self;
        }
        let mut value = self;
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        value.input.speed = lerp(self.input.speed, next.input.speed);
        value.input.move_yaw = self.input.move_yaw
            + ((next.input.move_yaw - self.input.move_yaw + 180.).rem_euclid(360.) - 180.) * t;
        value.input.pitch = lerp(self.input.pitch, next.input.pitch);
        value.input.time_since_fire = lerp(self.input.time_since_fire, next.input.time_since_fire);
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

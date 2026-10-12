//! Immutable authoritative death context. This is not a ragdoll solver or force.
use crate::{Player, PlayerId, Team, TrackedPose, weapon::HitGroup};
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeathKey {
    pub victim: PlayerId,
    pub life_epoch: u64,
    /// World tick at the actual producer. A kill during N -> N+1 is stamped N;
    /// the enclosing post-step event batch may be stamped N+1.
    pub tick: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeathPose {
    /// Current authoritative feet/angles, never a rewound hit hull or render pose.
    pub origin: Vec3,
    pub view_angles: Vec3,
    pub velocity: Vec3,
    pub ducked: bool,
    pub duck_amount: f32,
    pub on_ground: bool,
    /// Already admitted head/hands, retaining their feet-relative convention.
    pub tracked: Option<TrackedPose>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeathActor {
    pub id: PlayerId,
    pub name: String,
    pub team: Team,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeathHit {
    /// Actual firearm post-spread ray or melee ray. Not an inferred impulse.
    pub direction: Vec3,
    /// Actual hit point; lag compensation may place it on a historical hull.
    pub position: Vec3,
    pub group: HitGroup,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeathReceipt {
    pub key: DeathKey,
    pub round: u32,
    pub pose: DeathPose,
    pub victim_name: String,
    pub victim_team: Team,
    /// None for WORLD damage. Identity is independent of future actor lookup.
    pub attacker: Option<DeathActor>,
    /// None when the damage producer has no actual hit ray (bomb/environment).
    pub hit: Option<DeathHit>,
}

impl DeathReceipt {
    pub(crate) fn capture(
        victim: &Player,
        attacker: Option<&Player>,
        tick: u64,
        round: u32,
        hit: Option<DeathHit>,
    ) -> Self {
        Self {
            key: DeathKey {
                victim: victim.id,
                life_epoch: victim.life_epoch,
                tick,
            },
            round,
            pose: DeathPose {
                origin: victim.movement.origin,
                view_angles: victim.view_angles,
                velocity: victim.movement.velocity,
                ducked: victim.movement.ducked,
                duck_amount: victim.movement.duck_amount,
                on_ground: victim.movement.on_ground,
                tracked: victim.tracked,
            },
            victim_name: victim.name.clone(),
            victim_team: victim.team,
            attacker: attacker.map(|p| DeathActor {
                id: p.id,
                name: p.name.clone(),
                team: p.team,
            }),
            hit,
        }
    }
}

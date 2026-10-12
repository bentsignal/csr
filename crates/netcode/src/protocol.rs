//! Versioned messages: clients select inputs, never player ids or world state.
use serde::{Deserialize, Serialize};
use sim::{GameEvent, PlayerId, Snapshot, Team, TickInput};

pub const VERSION: u32 = 1;
pub const PROTOCOL_ID: u64 = 0x4353_5257_0000_0001;
pub const CONTROL: u8 = 0;
pub const INPUT: u8 = 1;
pub const SNAPSHOT: u8 = 2;
pub const EVENTS: u8 = 3;
pub const MAX_INPUT_BATCH: usize = 8;
pub const MAX_MESSAGE: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentManifest {
    /// A stable ContentId string; the map filename is separate.
    pub map_id: String,
    pub map_name: String,
    /// SHA-1 of the installed BSP bytes (including its embedded content).
    pub map_hash: String,
    /// SHA-1 of the installed weapon definitions used by the sim.
    pub catalog_hash: String,
}

impl ContentManifest {
    /// Metadata only: never interpret a peer's string as a filesystem path.
    pub fn validate(&self) -> Result<content::ContentId, String> {
        if self.map_id.len() > 256 {
            return Err("map content id is too long".into());
        }
        let id: content::ContentId = self
            .map_id
            .parse()
            .map_err(|e| format!("invalid map content id: {e}"))?;
        if id.kind != content::ContentKind::Map {
            return Err("content id is not a map".into());
        }
        if self.map_name.is_empty()
            || self.map_name.len() > 128
            || !self
                .map_name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err("unsafe map filename".into());
        }
        for hash in [&self.map_hash, &self.catalog_hash] {
            if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("invalid content SHA-1".into());
            }
        }
        Ok(id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientControl {
    Hello {
        version: u32,
        name: String,
        team: Team,
        content: ContentManifest,
    },
    Leave,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerControl {
    Welcome {
        version: u32,
        player: PlayerId,
        tick_rate: u32,
        server_tick: u64,
        content: ContentManifest,
    },
    Rejected {
        reason: String,
    },
    ContentRequired {
        version: u32,
        content: ContentManifest,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputCommand {
    /// Strictly increasing, starting at one. Not a requested server tick.
    pub sequence: u64,
    /// Last authoritative/render interpolation tick seen by the shooter;
    /// never the client's predicted future tick. Reserved for hit rewind.
    pub view_tick: u64,
    pub input: TickInput,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputBatch(pub Vec<InputCommand>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StateUpdate {
    pub acknowledged_input: u64,
    pub snapshot: Snapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventBatch {
    pub tick: u64,
    pub events: Vec<GameEvent>,
}

pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>, String> {
    let bytes = postcard::to_allocvec(message).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_MESSAGE {
        return Err("network message exceeds limit".into());
    }
    Ok(bytes)
}

pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    if bytes.len() > MAX_MESSAGE {
        return Err("network message exceeds limit".into());
    }
    let (message, trailing) = postcard::take_from_bytes(bytes).map_err(|e| e.to_string())?;
    if !trailing.is_empty() {
        return Err("trailing network message bytes".into());
    }
    Ok(message)
}

pub fn channels() -> renet::ConnectionConfig {
    use renet::{ChannelConfig, SendType};
    let server_channels = vec![
        ChannelConfig {
            channel_id: CONTROL,
            max_memory_usage_bytes: 16 * 1024,
            send_type: SendType::ReliableOrdered {
                resend_time: std::time::Duration::from_millis(100),
            },
        },
        ChannelConfig {
            channel_id: SNAPSHOT,
            max_memory_usage_bytes: MAX_MESSAGE * 2,
            send_type: SendType::Unreliable,
        },
        ChannelConfig {
            channel_id: EVENTS,
            max_memory_usage_bytes: 128 * 1024,
            send_type: SendType::ReliableOrdered {
                resend_time: std::time::Duration::from_millis(100),
            },
        },
    ];
    let client_channels = vec![
        server_channels[0].clone(),
        ChannelConfig {
            channel_id: INPUT,
            max_memory_usage_bytes: 64 * 1024,
            send_type: SendType::Unreliable,
        },
    ];
    renet::ConnectionConfig {
        available_bytes_per_tick: (MAX_MESSAGE + 128 * 1024) as u64,
        client_channels_config: client_channels,
        server_channels_config: server_channels,
    }
}

pub(crate) fn validate_state(
    state: &StateUpdate,
    welcome: Option<&ServerControl>,
    sent: u64,
    last_ack: u64,
) -> Result<(), String> {
    let Some(ServerControl::Welcome {
        player, tick_rate, ..
    }) = welcome
    else {
        return Err("state before admission".into());
    };
    let world = &state.snapshot.state;
    if world.tick_rate != *tick_rate
        || state.acknowledged_input > sent
        || state.acknowledged_input < last_ack
    {
        return Err("invalid snapshot rate or input acknowledgement".into());
    }
    if world.players.len() > 64
        || world.items.len() > 4096
        || world.projectiles.len() > 4096
        || world.utility_fields.len() > 4096
    {
        return Err("snapshot entity limit exceeded".into());
    }
    let c = &world.config;
    let r = &world.rules;
    if [
        c.gravity,
        c.friction,
        c.stop_speed,
        c.accelerate,
        c.air_accelerate,
        c.air_max_wishspeed,
        c.max_speed,
        c.max_velocity,
        c.step_size,
        c.jump_impulse,
        c.stamina_max,
        c.stamina_jump_cost,
        c.stamina_land_cost,
        c.stamina_recovery_rate,
        c.walk_speed_modifier,
        c.duck_speed_modifier,
        c.duck_speed,
        r.freeze_time,
        r.buy_time,
        r.round_time,
        r.restart_delay,
        r.c4_timer,
        r.plant_time,
        r.defuse_time,
        r.defuse_time_kit,
        r.kill_award_factor,
        world.round.phase_start,
        world.round.live_start,
    ]
    .iter()
    .any(|n| !n.is_finite())
    {
        return Err("nonfinite snapshot physics or rules".into());
    }
    let mut ids = std::collections::HashSet::new();
    for actor in &world.players {
        if actor.id == 0
            || !ids.insert(actor.id)
            || !actor.movement.origin.is_finite()
            || !actor.movement.velocity.is_finite()
            || !actor.view_angles.is_finite()
            || [
                actor.movement.duck_amount,
                actor.movement.stamina,
                actor.movement.last_fall_speed,
                actor.respawn_at,
                actor.flash_until,
            ]
            .iter()
            .any(|n| !n.is_finite())
            || actor.weapons.len() > 64
            || (!actor.weapons.is_empty() && actor.active >= actor.weapons.len())
        {
            return Err("invalid snapshot player state".into());
        }
        for weapon in &actor.weapons {
            validate_weapon(weapon)?;
        }
        crate::session::validate_input(actor.last_input)?;
        let mut input = actor.last_input;
        input.tracked = actor.tracked;
        input.aim = actor.aim;
        crate::session::validate_input(input)?;
    }
    for item in &world.items {
        if !item.pos.is_finite() || !item.yaw.is_finite() || !item.pickup_after.is_finite() {
            return Err("invalid dropped item state".into());
        }
        if let sim::GroundKind::Weapon(weapon) = &item.kind {
            validate_weapon(weapon)?;
        }
    }
    if world
        .projectiles
        .iter()
        .any(|p| !p.pos.is_finite() || !p.velocity.is_finite() || !p.born.is_finite())
        || world
            .utility_fields
            .iter()
            .any(|f| !f.pos.is_finite() || !f.born.is_finite() || !f.until.is_finite())
    {
        return Err("invalid snapshot utility state".into());
    }
    if world.bomb.is_some_and(|b| {
        !b.pos.is_finite()
            || !b.planted_at.is_finite()
            || !b.explode_at.is_finite()
            || b.defuser.is_some_and(|(_, started)| !started.is_finite())
    }) {
        return Err("invalid snapshot bomb state".into());
    }
    if *player == 0 || !ids.contains(player) {
        return Err("snapshot omits assigned player".into());
    }
    Ok(())
}

fn validate_weapon(weapon: &sim::weapon::WeaponState) -> Result<(), String> {
    if [
        weapon.next_attack,
        weapon.reload_start,
        weapon.next_secondary_attack,
        weapon.zoom_changed_at,
        weapon.zoom_from_fov,
        weapon.accuracy_penalty,
        weapon.recoil_index,
    ]
    .iter()
    .any(|n| !n.is_finite())
        || weapon.reload_end.is_some_and(|n| !n.is_finite())
        || weapon.last_melee_attack.is_some_and(|n| !n.is_finite())
    {
        return Err("invalid snapshot weapon timing".into());
    }
    if weapon.cosmetic.is_some_and(|c| {
        !c.wear.is_finite()
            || c.stickers
                .iter()
                .flatten()
                .any(|(_, wear)| !wear.is_finite())
    }) {
        return Err("invalid snapshot cosmetic state".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vr_input_round_trips_without_camera_assumptions() {
        let input = TickInput {
            tracked: Some(sim::TrackedPose::default()),
            aim: Some(sim::AimRay {
                origin: glam::Vec3::new(4., 2., 64.),
                direction: glam::Vec3::X,
            }),
            ..Default::default()
        };
        let batch = InputBatch(vec![InputCommand {
            sequence: 12,
            view_tick: 321,
            input,
        }]);
        assert_eq!(
            decode::<InputBatch>(&encode(&batch).unwrap()).unwrap(),
            batch
        );
    }
    #[test]
    fn oversized_or_invalid_messages_are_rejected() {
        assert!(decode::<InputBatch>(&vec![0; MAX_MESSAGE + 1]).is_err());
        assert!(decode::<InputBatch>(&[255, 255]).is_err());
        let mut bytes = encode(&InputBatch(Vec::new())).unwrap();
        bytes.push(0);
        assert!(decode::<InputBatch>(&bytes).is_err());
    }
    #[test]
    fn manifest_never_accepts_host_paths_or_non_map_content() {
        let mut manifest = ContentManifest {
            map_id: "csgo_map_de_dust2".into(),
            map_name: "de_dust2".into(),
            map_hash: "0".repeat(40),
            catalog_hash: "1".repeat(40),
        };
        assert!(manifest.validate().is_ok());
        for path in [
            "../secret",
            "C:/map",
            "\\\\host\\map",
            "maps/de_dust2",
            "de_dust2.bsp",
        ] {
            manifest.map_name = path.into();
            assert!(manifest.validate().is_err());
        }
        manifest.map_name = "de_dust2".into();
        manifest.map_id = "csgo_weapon_ak47".into();
        assert!(manifest.validate().is_err());
    }
}

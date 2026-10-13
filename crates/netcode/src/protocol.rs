//! Versioned messages: clients select inputs, never player ids or world state.
use serde::{Deserialize, Serialize};
use sim::{GameEvent, PlayerId, Snapshot, Team, TickInput};

// RulesConfig carries the authoritative grenade-practice setting.
pub const VERSION: u32 = 10;
pub const PROTOCOL_ID: u64 = 0x4353_5257_0000_000a;
pub const CONTROL: u8 = 0;
pub const INPUT: u8 = 1;
pub const SNAPSHOT: u8 = 2;
pub const EVENTS: u8 = 3;
pub const MAX_INPUT_BATCH: usize = 8;
/// Shared aggregate event budget, including direct transport users.
pub const MAX_EVENTS: usize = 4096;
pub const MAX_MESSAGE: usize = 256 * 1024;
pub(crate) const CONTROL_MEMORY: usize = 16 * 1024;
/// Exact f64 tick representation, with room for bounded local replay.
pub const MAX_LOCAL_TICK: u64 = (1_u64 << 53) - 1;
pub const MAX_PROTOCOL_COUNTER: u64 = MAX_LOCAL_TICK - crate::prediction::MAX_PENDING_INPUTS as u64;

pub(crate) fn validate_events(batch: &EventBatch) -> Result<(), String> {
    if batch.tick > MAX_PROTOCOL_COUNTER || batch.events.len() > MAX_EVENTS {
        return Err("invalid event batch bounds".into());
    }
    let text = |value: &str| {
        !value.is_empty()
            && value.len() <= 128
            && !value.chars().any(char::is_control)
            && !value.contains(['/', '\\', ':'])
    };
    let position = |value: glam::Vec3| value.is_finite() && value.length_squared().is_finite();
    for event in &batch.events {
        use GameEvent::*;
        let valid = match event {
            Fire {
                player,
                weapon,
                origin,
            }
            | Melee {
                player,
                weapon,
                origin,
                ..
            }
            | Scope {
                player,
                weapon,
                origin,
                ..
            } => *player != 0 && text(weapon) && position(*origin),
            Tracer { player, start, end } => *player != 0 && position(*start) && position(*end),
            Impact {
                player,
                pos,
                normal,
                ..
            } => *player != 0 && position(*pos) && position(*normal),
            Damage {
                victim,
                health,
                pos,
                ..
            } => *victim != 0 && *health >= 0 && position(*pos),
            Kill { attacker, victim, weapon, headshot, death } => {
                *victim != 0 && text(weapon)
                    && validate_death(death, *attacker, *victim, *headshot, batch.tick)
            }
            Land { player, speed } => *player != 0 && speed.is_finite() && *speed >= 0.,
            Pickup { player, item } => *player != 0 && text(item),
            Dropped { player, item, pos } => *player != 0 && text(item) && position(*pos),
            DecoyShot { player, pos }
            | UtilityThrown { player, pos, .. }
            | UtilityDetonated { player, pos, .. } => *player != 0 && position(*pos),
            BombPlanted { player, site, pos } => {
                *player != 0 && site.is_ascii_uppercase() && position(*pos)
            }
            BombExploded { pos } => position(*pos),
            StatTrakKill { owner: player, .. }
            | Reload { player }
            | Jump { player }
            | Purchase { player, .. }
            | PlantStart { player }
            | DefuseStart { player, .. }
            | BombDefused { player } => *player != 0,
            RoundStart { .. } | FreezeEnd | RoundEnd { .. } | Halftime | MatchEnd { .. } => true,
        };
        if !valid {
            return Err("invalid authoritative event payload".into());
        }
    }
    Ok(())
}

/// Check the whole immutable receipt before enqueueing any event. Actor lookup
/// is deliberately absent: a newer snapshot may have respawned/removed them.
fn validate_death(
    death: &sim::death::DeathReceipt,
    attacker: PlayerId,
    victim: PlayerId,
    headshot: bool,
    batch_tick: u64,
) -> bool {
    let name = |value: &str| value.len() <= 64 && !value.chars().any(char::is_control);
    let vector = |value: glam::Vec3| value.is_finite() && value.length_squared().is_finite();
    let unit = |length: f32| length.is_finite() && (length - 1.).abs() <= 0.001;
    if death.key.victim != victim || death.key.life_epoch == 0
        || death.key.tick > batch_tick || death.key.tick > MAX_PROTOCOL_COUNTER
        || !name(&death.victim_name)
        || !vector(death.pose.origin) || !vector(death.pose.view_angles)
        || !vector(death.pose.velocity) || !death.pose.duck_amount.is_finite()
        || !(0. ..=1.).contains(&death.pose.duck_amount)
    { return false; }
    match &death.attacker {
        Some(actor) if attacker != sim::WORLD && actor.id == attacker && name(&actor.name) => {},
        None if attacker == sim::WORLD => {},
        _ => return false,
    }
    if let Some(tracked) = &death.pose.tracked {
        for (pose, maximum) in [
            (&tracked.head, crate::session::TRACKED_HEAD_LIMIT),
            (&tracked.left_hand, crate::session::TRACKED_HAND_LIMIT),
            (&tracked.right_hand, crate::session::TRACKED_HAND_LIMIT),
        ] {
            if !vector(pose.position) || pose.position.length() > maximum + 0.001
                || !pose.rotation.is_finite() || !unit(pose.rotation.length_squared())
            { return false; }
        }
    }
    death.hit.is_none_or(|hit| {
        vector(hit.direction) && unit(hit.direction.length_squared()) && vector(hit.position)
            && headshot == (hit.group == sim::weapon::HitGroup::Head)
    })
}

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
    Inspect {
        version: u32,
    },
    Hello {
        version: u32,
        name: String,
        team: Team,
        content: ContentManifest,
    },
    Leave,
    /// Receipt of a fully reconstructed and validated transport frame, not an
    /// input acknowledgement or a request to change authoritative sim state.
    SnapshotReceipt {
        frame: u64,
        hash: u64,
    },
    SnapshotRecovery,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerControl {
    Info(ServerInfo),
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

/// Bounded discovery metadata only. Frontends resolve installed definitions;
/// receiving it never selects a host path, URL, pack mount or download.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub version: u32,
    pub tick_rate: u32,
    pub snapshot_interval_ticks: u32,
    pub max_unlag_ticks: u32,
    pub interpolation_allowance_ticks: u32,
    pub content: ContentManifest,
}
impl ServerInfo {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION
            || !sim::TICK_RATE_RANGE.contains(&self.tick_rate)
            || self.snapshot_interval_ticks == 0
            || self.snapshot_interval_ticks > self.tick_rate
            || self.max_unlag_ticks == 0
            || self.max_unlag_ticks > self.tick_rate
            || self.interpolation_allowance_ticks > self.max_unlag_ticks
        {
            return Err("invalid server discovery bounds".into());
        }
        self.content.validate()?;
        Ok(())
    }
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
            max_memory_usage_bytes: CONTROL_MEMORY,
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
        player,
        tick_rate,
        server_tick,
        ..
    }) = welcome
    else {
        return Err("state before admission".into());
    };
    let world = &state.snapshot.state;
    if world.tick_rate != *tick_rate
        || *server_tick > MAX_PROTOCOL_COUNTER
        || world.tick > MAX_PROTOCOL_COUNTER
        || state.acknowledged_input > MAX_PROTOCOL_COUNTER
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
            || actor.name.len() > 64
            || actor.name.chars().any(char::is_control)
            || !ids.insert(actor.id)
            || (actor.alive && actor.life_epoch == 0)
            || actor
                .last_input
                .rewind_tick
                .is_some_and(|tick| tick > world.tick)
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
pub(crate) fn test_death_event() -> GameEvent {
    let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
    let victim = world.add_player("victim <old> & name", Team::CounterTerrorist);
    world.damage_environment(victim, 1000, "environment");
    world.events.into_iter().find(|event| matches!(event, GameEvent::Kill { .. })).unwrap()
}

#[cfg(test)]
#[path = "death_protocol_tests.rs"]
mod death_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authoritative_effects_reject_nonfinite_and_unbounded_payloads() {
        use glam::Vec3;
        let mut invalid_kill = test_death_event();
        if let GameEvent::Kill { weapon, .. } = &mut invalid_kill { *weapon = "../host/file".into(); }
        let events = vec![
            GameEvent::Fire {
                player: 1,
                weapon: "weapon_test".into(),
                origin: Vec3::NAN,
            },
            GameEvent::Impact {
                player: 1,
                pos: Vec3::ZERO,
                normal: Vec3::INFINITY,
                surface: 0,
            },
            GameEvent::Tracer {
                player: 1,
                start: Vec3::ZERO,
                end: Vec3::splat(f32::MAX),
            },
            GameEvent::Land {
                player: 1,
                speed: f32::NAN,
            },
            invalid_kill,
            GameEvent::Dropped {
                player: 1,
                item: "x".repeat(129),
                pos: Vec3::ZERO,
            },
        ];
        for event in events {
            assert!(
                validate_events(&EventBatch {
                    tick: 1,
                    events: vec![event]
                })
                .is_err()
            );
        }
        assert!(
            validate_events(&EventBatch {
                tick: 1,
                events: vec![
                    GameEvent::Fire {
                        player: 1,
                        weapon: "weapon_test".into(),
                        origin: Vec3::ZERO
                    },
                    GameEvent::Land {
                        player: 1,
                        speed: 100.
                    },
                ]
            })
            .is_ok()
        );
    }
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

    #[test]
    fn snapshot_life_and_rewind_metadata_are_validated_before_exposure() {
        let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
        let id = world.add_player("player", sim::Team::CounterTerrorist);
        let welcome = ServerControl::Welcome {
            version: VERSION,
            player: id,
            tick_rate: 128,
            server_tick: 0,
            content: ContentManifest {
                map_id: "csgo_map_de_dust2".into(),
                map_name: "de_dust2".into(),
                map_hash: "0".repeat(40),
                catalog_hash: "1".repeat(40),
            },
        };
        let state = StateUpdate {
            acknowledged_input: 0,
            snapshot: world.snapshot(),
        };
        assert!(validate_state(&state, Some(&welcome), 0, 0).is_ok());
        let mut invalid = state.clone();
        invalid.snapshot.state.players[0].life_epoch = 0;
        assert!(validate_state(&invalid, Some(&welcome), 0, 0).is_err());
        invalid = state;
        invalid.snapshot.state.players[0].last_input.rewind_tick = Some(1);
        assert!(validate_state(&invalid, Some(&welcome), 0, 0).is_err());
        invalid.snapshot.state.players[0].last_input.rewind_tick = None;
        invalid.snapshot.state.tick = u64::MAX;
        assert!(validate_state(&invalid, Some(&welcome), 0, 0).is_err());
        invalid.snapshot.state.tick = 0;
        invalid.acknowledged_input = u64::MAX;
        assert!(validate_state(&invalid, Some(&welcome), u64::MAX, 0).is_err());
        let mut invalid_welcome = welcome;
        if let ServerControl::Welcome { server_tick, .. } = &mut invalid_welcome {
            *server_tick = u64::MAX;
        }
        invalid.acknowledged_input = 0;
        assert!(validate_state(&invalid, Some(&invalid_welcome), 0, 0).is_err());
        assert!(
            validate_events(&EventBatch {
                tick: u64::MAX,
                events: vec![]
            })
            .is_err()
        );
    }
}

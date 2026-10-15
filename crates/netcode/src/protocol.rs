//! Versioned messages: clients select inputs, never player ids or world state.
use serde::{Deserialize, Serialize};
use sim::{GameEvent, PlayerId, Snapshot, Team, TickInput};

// v22 preserves held automatic-fire phase and released-empty idle reloads.
// Wire fields are unchanged, but older peers predict different weapon state.
pub const VERSION: u32 = 22;
pub const PROTOCOL_ID: u64 = 0x4353_5257_0000_0016;
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
                ..
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
            }
            | BurstModeChanged {
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
            DecoyShot { player, pos, weapon } => *player != 0 && position(*pos)
                && valid_decoy_weapon(sim::utility::Kind::Decoy, Some(weapon)),
            UtilityThrown { player, pos, .. }
            | UtilityDetonated { player, pos, .. }
            | DecoyExpired { player, pos } => *player != 0 && position(*pos),
            UtilityBounced { player, kind, pos } => *player != 0 && position(*pos)
                && matches!(kind, sim::utility::Kind::Smoke | sim::utility::Kind::Flash
                    | sim::utility::Kind::He | sim::utility::Kind::Molotov
                    | sim::utility::Kind::Incendiary | sim::utility::Kind::Decoy),
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
    /// Occupied roster slots, including bots, spectators and dead players.
    pub players: u16,
    /// Initial roster plus this authority's configured transport admission slots.
    pub max_players: u16,
    pub tick_rate: u32,
    pub snapshot_interval_ticks: u32,
    pub max_unlag_ticks: u32,
    pub interpolation_allowance_ticks: u32,
    pub content: ContentManifest,
}
impl ServerInfo {
    /// Admission/render timing compatibility excludes changing roster metadata.
    pub(crate) fn same_session_policy(&self, other: &Self) -> bool {
        self.version == other.version
            && self.tick_rate == other.tick_rate
            && self.snapshot_interval_ticks == other.snapshot_interval_ticks
            && self.max_unlag_ticks == other.max_unlag_ticks
            && self.interpolation_allowance_ticks == other.interpolation_allowance_ticks
            && self.content == other.content
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION
            || self.max_players == 0
            || self.max_players > 64
            || self.players > self.max_players
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
                actor.animation.move_cycle,
                actor.animation.air_time,
            ]
            .iter()
            .any(|n| !n.is_finite())
            || !(0.0..1.0).contains(&actor.animation.move_cycle)
            || !(0.0..=60.0).contains(&actor.animation.air_time)
            || actor.animation.last_fire_tick.is_some_and(|tick| tick > world.tick)
            || actor.weapons.len() > 64
            || (!actor.weapons.is_empty() && actor.active >= actor.weapons.len())
        {
            return Err("invalid snapshot player state".into());
        }
        for (slot, weapon) in actor.weapons.iter().enumerate() {
            validate_weapon(weapon,world.tick)?;
            if weapon.burst.is_some() && (!actor.alive || slot != actor.active) {
                return Err("inactive weapon retains pending burst".into());
            }
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
            validate_weapon(weapon,world.tick)?;
            if weapon.grenade_throw.is_some() || weapon.suppressor_transition.is_some() || weapon.burst.is_some(){return Err("dropped weapon retains active state".into());}
        }
    }
    if world
        .projectiles
        .iter()
        .any(|p| !p.pos.is_finite() || !p.velocity.is_finite() || !p.born.is_finite()
            || !valid_decoy_weapon(p.kind, p.decoy_weapon.as_deref()))
        || world
            .utility_fields
            .iter()
            .any(|f| !f.pos.is_finite() || !f.born.is_finite() || !f.until.is_finite()
                || !valid_decoy_weapon(f.kind, f.decoy_weapon.as_deref()))
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

fn valid_decoy_weapon(kind: sim::utility::Kind, weapon: Option<&str>) -> bool {
    weapon.is_none_or(|weapon| kind == sim::utility::Kind::Decoy && weapon.len() <= 128
        && weapon.parse::<content::ContentId>().is_ok_and(|id| id.kind == content::ContentKind::Weapon))
}

fn validate_weapon(weapon: &sim::weapon::WeaponState, tick:u64) -> Result<(), String> {
    if weapon.burst_mode && weapon.burst_profile().is_none() {
        return Err("weapon has no valid burst profile".into());
    }
    if let Some(burst) = weapon.burst {
        if !weapon.burst_mode || weapon.burst_profile().is_none() || weapon.reload_end.is_some()
            || !(1..=2).contains(&burst.remaining) || i32::from(burst.remaining) > weapon.clip
            || !burst.next_shot.is_finite() || burst.next_shot < 0. || burst.next_shot > weapon.next_attack {
            return Err("invalid snapshot burst state".into());
        }
    }
    if weapon.suppressor_attached && !weapon.def.attr_f32("has silencer").is_some_and(|v| v > 0.) {
        return Err("weapon has no suppressor".into());
    }
    if let Some(state) = weapon.suppressor_transition {
        let Some(animation) = weapon.suppressor_animation() else { return Err("suppressor timing profile missing".into()); };
        if state.started_tick > tick || weapon.reload_end.is_some()
            || !animation.duration.is_finite() || animation.duration <= 0. || animation.duration > 30.
            || !animation.complete_time.is_finite() || !(0. ..=animation.duration).contains(&animation.complete_time) {
            return Err("invalid snapshot suppressor transition".into());
        }
    }
    if let Some(state)=weapon.grenade_throw {
        use sim::utility::ThrowPhase;
        let Some(profile)=weapon.def.grenade_animations.as_ref() else {return Err("grenade timing profile missing".into());};
        if weapon.def.class!=weapons::WeaponClass::Grenade || state.pull_started_tick>tick
            || state.throw_started_tick.is_some_and(|start|start<state.pull_started_tick||start>tick)
            || match state.phase {
                ThrowPhase::Throwing=>state.throw_started_tick.is_none()||!state.release_requested,
                _=>state.throw_started_tick.is_some()||state.spawned,
            }
            || [profile.pull_pin_duration,profile.overhand.duration,profile.underhand.duration]
                .iter().any(|n|!n.is_finite()||*n<=0.||*n>30.)
            || [&profile.overhand,&profile.underhand].iter().any(|a|
                !a.release_time.is_finite()||a.release_time<0.||a.release_time>a.duration
                || a.command_release_time.is_some_and(|delay| !delay.is_finite() || delay < 0. || delay > a.duration))
        {return Err("invalid snapshot grenade throw phase".into());}
    }
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
    fn burst_snapshot_roundtrips_and_rejects_impossible_pending_bullets() {
        let catalog = weapons::Catalog::from_items_game(r##"
            items_game {
                prefabs { burst_gun { item_class weapon_pack_burst item_type_name "#CSGO_Type_Rifle"
                    attributes { "primary clip size" 20 "has burst mode" 1
                        "cycletime when in burst mode" 0.5 "time between burst shots" 0.05 } } }
                items { 900 { name weapon_pack_burst prefab burst_gun baseitem 1 } }
            }
        "##).unwrap();
        let mut world = sim::World::with_catalog(std::sync::Arc::new(collision::CollisionWorld::new()), std::sync::Arc::new(catalog));
        world.add_player("burst", sim::Team::CounterTerrorist);
        let mut gun = sim::weapon::WeaponState::new(std::sync::Arc::new(world.catalog.get("weapon_pack_burst").unwrap().clone()));
        gun.burst_mode = true;
        gun.clip = 19;
        gun.next_attack = 0.5;
        gun.burst = Some(sim::weapon::BurstState { remaining: 2, next_shot: 0.05 });
        assert!(validate_weapon(&gun, 0).is_ok());
        world.players[0].weapons = vec![gun.clone()];
        let snapshot = world.snapshot();
        assert_eq!(world.decode::<sim::Snapshot>(&sim::state::encode(&snapshot)).unwrap(), snapshot);
        for case in 0..8 {
            let mut invalid = gun.clone();
            match case {
                0 => invalid.burst_mode = false,
                1 => invalid.burst.as_mut().unwrap().remaining = 0,
                2 => invalid.burst.as_mut().unwrap().remaining = 3,
                3 => invalid.burst.as_mut().unwrap().next_shot = f32::NAN,
                4 => invalid.clip = 1,
                5 => invalid.reload_end = Some(0.3),
                6 => invalid.next_attack = 0.01,
                _ => { std::sync::Arc::make_mut(&mut invalid.def).attributes.insert("time between burst shots".into(), "NaN".into()); },
            }
            assert!(validate_weapon(&invalid, 0).is_err(), "case {case}");
        }
        let welcome = ServerControl::Welcome { version: VERSION, player: world.players[0].id,
            tick_rate: world.tick_rate, server_tick: 0,
            content: ContentManifest { map_id: "csgo_map_test".into(), map_name: "test".into(),
                map_hash: "0".repeat(40), catalog_hash: "1".repeat(40) } };
        let valid = StateUpdate { acknowledged_input: 0, snapshot };
        assert!(validate_state(&valid, Some(&welcome), 0, 0).is_ok());
        for case in 0..3 {
            let mut invalid = valid.clone();
            match case {
                0 => invalid.snapshot.state.players[0].alive = false,
                1 => {
                    let mut other = gun.clone(); other.burst = None;
                    invalid.snapshot.state.players[0].weapons.push(other);
                    invalid.snapshot.state.players[0].active = 1;
                }
                _ => {
                    invalid.snapshot.state.players[0].weapons.clear();
                    invalid.snapshot.state.items.push(sim::GroundItem { id: 1,
                        kind: sim::GroundKind::Weapon(gun.clone()), pos: glam::Vec3::ZERO,
                        yaw: 0., dropped_by: world.players[0].id, pickup_after: 0. });
                }
            }
            assert!(validate_state(&invalid, Some(&welcome), 0, 0).is_err(), "inactive case {case}");
        }
    }
    #[test]
    fn burst_mode_cue_roundtrip_and_admission_preserve_authority() {
        let event = GameEvent::BurstModeChanged {
            player: 7, weapon: "weapon_glock".into(), origin: glam::Vec3::new(12., 4., 64.), burst: true,
        };
        let batch = EventBatch { tick: 200, events: vec![event.clone()] };
        validate_events(&batch).unwrap();
        assert_eq!(decode::<EventBatch>(&encode(&batch).unwrap()).unwrap(), batch);
        for case in 0..3 {
            let mut invalid = event.clone();
            if let GameEvent::BurstModeChanged { player, weapon, origin, .. } = &mut invalid {
                match case { 0 => *player = 0, 1 => *weapon = "../host/file".into(), _ => *origin = glam::Vec3::NAN }
            }
            assert!(validate_events(&EventBatch { tick: 200, events: vec![invalid] }).is_err());
        }
    }

    #[test]
    fn decoy_expiry_roundtrip_and_admission_keep_authoritative_position() {
        let pos = glam::Vec3::new(-100., 20., 2.);
        let batch = EventBatch { tick: 960, events: vec![GameEvent::DecoyExpired { player: 7, pos }] };
        validate_events(&batch).unwrap();
        assert_eq!(decode::<EventBatch>(&encode(&batch).unwrap()).unwrap(), batch);
        for (player, pos) in [(0, pos), (7, glam::Vec3::NAN), (7, glam::Vec3::splat(f32::MAX))] {
            let invalid = EventBatch { tick: 960, events: vec![GameEvent::DecoyExpired { player, pos }] };
            assert!(validate_events(&invalid).is_err());
        }
    }

    #[test]
    fn grenade_contact_batch_round_trips_and_rejects_invalid_authority() {
        let event = GameEvent::UtilityBounced {
            player: 7, kind: sim::utility::Kind::Smoke, pos: glam::Vec3::new(100., 20., 2.),
        };
        let batch = EventBatch { tick: 200, events: vec![event] };
        assert!(validate_events(&batch).is_ok());
        assert_eq!(decode::<EventBatch>(&encode(&batch).unwrap()).unwrap(), batch);
        for (player, pos) in [(0, glam::Vec3::ZERO), (7, glam::Vec3::NAN)] {
            let invalid = EventBatch { tick: 200, events: vec![GameEvent::UtilityBounced {
                player, kind: sim::utility::Kind::He, pos,
            }] };
            assert!(validate_events(&invalid).is_err());
        }
    }

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
                suppressed: false,
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
            GameEvent::DecoyShot { player: 1, pos: Vec3::ZERO, weapon: "csgo_map_de_dust2".into() },
            GameEvent::DecoyShot { player: 1, pos: Vec3::ZERO, weapon: "../weapon_ak47".into() },
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
                        origin: Vec3::ZERO,
                        suppressed: false,
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

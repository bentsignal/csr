//! The replicated game state, serialisable with serde.
//!
//! [`WorldState`] is everything in a [`World`] that changes while playing:
//! the round, every player (pose, health, money, inventory, loadout,
//! tracked VR pose), dropped items, grenades in flight, smoke and fire, the
//! bomb, door poses/lifecycle, and the rules and movement settings in force. It leaves out what
//! both sides load from the map and the install (collision, spawns,
//! objectives, the weapon catalog), server-only bot brains and hit history.
//!
//! A [`Snapshot`] is one tick as a client receives it: the state after the
//! tick plus the [`GameEvent`]s it produced. Restoring a snapshot into a
//! world built from the same map gives a world that steps identically when
//! given the same player inputs (`tests::restored_state_steps_identically`).
//! Bot brains are server-only: replaying a match with bots also requires
//! their authoritative inputs or a clone of the server's brains. A snapshot
//! alone does not reproduce their decisions.
//!
//! Encoding is [postcard](https://docs.rs/postcard) (compact, varint
//! integers, raw IEEE floats, so the bytes are deterministic). Weapon
//! definitions travel as their [`content::ContentId`] and are resolved
//! against the receiver's catalog.

use crate::rules::{GameMode, RoundState, RulesConfig};
use crate::utility::{Field, Projectile};
use crate::{GameEvent, GroundItem, PlantedBomb, Player, PlayerId, World};
use movement::MoveConfig;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use weapons::{Catalog, WeaponDefinition};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldState {
    pub tick: u64,
    pub tick_rate: u32,
    pub mode: GameMode,
    pub rules: RulesConfig,
    pub config: MoveConfig,
    pub round: RoundState,
    pub players: Vec<Player>,
    pub items: Vec<GroundItem>,
    pub projectiles: Vec<Projectile>,
    pub utility_fields: Vec<Field>,
    pub bomb: Option<PlantedBomb>,
    pub doors: Vec<crate::doors::State>,
    pub next_id: PlayerId,
    pub next_item: u32,
}

/// One tick for a remote client: the state after it and what happened.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub state: WorldState,
    pub events: Vec<GameEvent>,
}

/// Why bytes could not be turned back into a state.
#[derive(Debug)]
pub struct DecodeError(pub String);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot decode game state: {}", self.0)
    }
}

impl std::error::Error for DecodeError {}

impl World {
    /// A copy of the replicated state.
    pub fn state(&self) -> WorldState {
        WorldState {
            tick: self.tick,
            tick_rate: self.tick_rate,
            mode: self.mode,
            rules: self.rules.clone(),
            config: self.config.clone(),
            round: self.round.clone(),
            players: self.players.clone(),
            items: self.items.clone(),
            projectiles: self.projectiles.clone(),
            utility_fields: self.utility_fields.clone(),
            bomb: self.bomb,
            doors: self.doors.clone(),
            next_id: self.next_id,
            next_item: self.next_item,
        }
    }

    /// Replaces the replicated state (map data and bot brains stay).
    pub fn restore(&mut self, s: WorldState) {
        self.tick = s.tick;
        self.tick_rate = s.tick_rate;
        self.mode = s.mode;
        self.rules = s.rules;
        self.config = s.config;
        self.round = s.round;
        self.players = s.players;
        self.items = s.items;
        self.projectiles = s.projectiles;
        self.utility_fields = s.utility_fields;
        self.bomb = s.bomb;
        self.doors = s.doors;
        self.rebuild_door_collision();
        self.next_id = s.next_id;
        self.next_item = s.next_item;
        self.rewind = None;
        self.character_hitboxes.clear();
    }

    /// The state after the last step plus that step's events.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot { state: self.state(), events: self.events.clone() }
    }

    /// Serialises the replicated state.
    pub fn encode_state(&self) -> Vec<u8> {
        encode(&self.state())
    }

    /// FNV-1a hash of the encoded state. Equal hashes after equal inputs
    /// show the step is deterministic.
    pub fn state_hash(&self) -> u64 {
        fnv1a(&self.encode_state())
    }

    /// Decodes bytes from [`encode`] against this world's weapon catalog.
    pub fn decode<T: serde::de::DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, DecodeError> {
        decode(&self.catalog, bytes)
    }
}

/// Serialises any replicated value (state, snapshot, input).
pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_allocvec(value).expect("game state always serialises")
}

/// Deserialises a replicated value, resolving weapon definitions against
/// `catalog`.
pub fn decode<T: serde::de::DeserializeOwned>(catalog: &Arc<Catalog>, bytes: &[u8]) -> Result<T, DecodeError> {
    let previous = DECODE_CATALOG.with(|c| c.replace(Some(DefTable { catalog: catalog.clone(), resolved: HashMap::new() })));
    let result = postcard::from_bytes(bytes).map_err(|e| DecodeError(e.to_string()));
    DECODE_CATALOG.with(|c| *c.borrow_mut() = previous);
    result
}

pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

struct DefTable {
    catalog: Arc<Catalog>,
    /// One shared definition per id within a decode.
    resolved: HashMap<String, Arc<WeaponDefinition>>,
}

thread_local! {
    static DECODE_CATALOG: RefCell<Option<DefTable>> = const { RefCell::new(None) };
}

/// `#[serde(with)]` for `Arc<WeaponDefinition>`: written as the content id,
/// read back from the catalog passed to [`decode`].
pub(crate) mod weapon_def {
    use super::*;
    use serde::de::Error;

    pub fn serialize<S: serde::Serializer>(def: &Arc<WeaponDefinition>, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&def.id)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Arc<WeaponDefinition>, D::Error> {
        let id = String::deserialize(d)?;
        DECODE_CATALOG.with(|table| {
            let mut table = table.borrow_mut();
            let table = table.as_mut().ok_or_else(|| D::Error::custom("weapon outside sim::state::decode"))?;
            if let Some(def) = table.resolved.get(&id) {
                return Ok(def.clone());
            }
            let def = table
                .catalog
                .weapons
                .iter()
                .find(|w| w.id.to_string() == id)
                .ok_or_else(|| D::Error::custom(format!("unknown weapon {id}")))?;
            let def = Arc::new(def.clone());
            table.resolved.insert(id, def.clone());
            Ok(def)
        })
    }
}

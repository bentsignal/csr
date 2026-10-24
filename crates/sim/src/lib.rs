//! Fixed-tick simulation.
//!
//! One funnel for authority, prediction and replay:
//! `World` + per-player [`TickInput`] → [`World::step`] → [`Snapshot`] and
//! [`GameEvent`]s. The step is deterministic for the same inputs (all
//! randomness is seeded from the tick, player and shot), so a client can
//! predict by running the very same code the server runs.
//!
//! Two rule sets ([`GameMode`]): deathmatch (respawn with a fixed loadout)
//! and competitive (rounds, money, buy zones, the bomb; see [`rules`]).
//!
//! Everything that decides the game runs here, including the bots
//! ([`World::bots`]) and each player's loadout choices ([`Loadout`]), so a
//! dedicated server, a listen server and client prediction all run the same
//! code. A client only produces [`TickInput`]s and draws.
//!
//! The tick rate is a per-world setting ([`World::tick_rate`], default
//! [`DEFAULT_TICK_RATE`], as on 128-tick CS:GO community servers). Code that
//! counts ticks derives the count from it, so 64 and 128 tick play the same.
//!
//! [`state`] makes the replicated state serialisable: it is the network
//! snapshot, and [`World::state_hash`] checks determinism.

pub mod bots;
pub mod model_pose;
pub mod death;
pub mod doors;
pub mod breakables;
mod ground_items;
pub mod loadout;
pub mod objectives;
pub mod rewind;
pub mod state;
pub mod rng;
pub mod rules;
pub mod weapon;
pub mod recoil_motion;
pub mod utility;
pub mod tactics;
mod penetration;
mod player_contact;

use collision::{contents, CollisionWorld};
use glam::Vec3;
use movement::{buttons, MoveConfig, MoveEvents, MoveInput, PlayerMove};
pub use loadout::Loadout;
pub use objectives::Objectives;
pub use state::{Snapshot, WorldState};
use serde::{Deserialize, Serialize};
use rng::Rng;
pub use rules::{BuyError, GameMode, Phase, Purchase, RoundEndReason, RoundState, RulesConfig};
use std::sync::Arc;
use weapon::{AccuracyContext, HitGroup, WeaponState, C4};

/// Ticks per second unless a server chooses otherwise (128-tick, as on
/// CS:GO community servers).
pub const DEFAULT_TICK_RATE: u32 = 128;
/// The tick rates a world accepts.
pub const TICK_RATE_RANGE: std::ops::RangeInclusive<u32> = 16..=256;
/// Seconds before a dead player respawns (deathmatch rules).
pub const RESPAWN_DELAY: f32 = 2.0;
/// Knife reach, units.
pub const MELEE_RANGE: f32 = 48.0;
/// The "attacker" of world damage (the bomb).
pub const WORLD: PlayerId = 0;
/// `weapon_slot` value that switches to the previously used weapon.
pub const SLOT_LAST: u8 = 10;
/// One inventory step in the installed default `invnext` direction.
pub const SLOT_NEXT: u8 = 11;
/// One inventory step in the installed default `invprev` direction.
pub const SLOT_PREVIOUS: u8 = 12;
/// How close a defuser must be to the bomb, units.
pub const DEFUSE_RANGE: f32 = 72.0;
/// Pickup radius for items on the ground, units.
pub const PICKUP_RANGE: f32 = 40.0;
/// Bomb blast: damage at the centre and radius (`500 * 3.5`).
pub const BOMB_DAMAGE: f32 = 500.0;
pub const BOMB_RADIUS: f32 = 1750.0;

pub type PlayerId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Team {
    Terrorist,
    CounterTerrorist,
    Spectator,
}

impl Team {
    pub fn enemy_of(self, other: Team) -> bool {
        self != other && self != Team::Spectator && other != Team::Spectator
    }

    pub fn other(self) -> Team {
        match self {
            Team::Terrorist => Team::CounterTerrorist,
            Team::CounterTerrorist => Team::Terrorist,
            Team::Spectator => Team::Spectator,
        }
    }
}

/// A rigid transform: position and orientation, Source axes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub position: Vec3,
    pub rotation: glam::Quat,
}

/// Tracked head and hands of a VR player. Positions are offsets from the
/// player's origin (the feet) in world axes, so they ride along with
/// server-side movement. Flat clients never send one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackedPose {
    pub head: Pose,
    pub left_hand: Pose,
    pub right_hand: Pose,
}

/// Where a weapon points, separate from where the player looks (a VR gun in
/// the hand). `origin` is an offset from the player's origin, like
/// [`TrackedPose`]; `direction` is a world-space vector (normalised by the
/// sim).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AimRay {
    pub origin: Vec3,
    pub direction: Vec3,
}

/// How far a tracked aim origin may be from the current hull eye, units.
/// Further offsets are pulled back before current-world muzzle admission.
pub const MAX_AIM_OFFSET: f32 = 48.0;
/// Conservative tracked-muzzle admission radius, matching collision's distance
/// epsilon. It catches adjacent thin walls that a biased point trace can miss.
/// Eyes within this margin of a solid suppress tracked attacks for that tick.
pub const AIM_WALL_MARGIN: f32 = 0.03125;

/// Input for one player for one tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TickInput {
    /// Pitch, yaw, roll in degrees.
    pub view_angles: Vec3,
    /// -450..450
    pub forward_move: f32,
    /// -450..450, positive is right.
    pub side_move: f32,
    /// Held `movement::buttons` bits (JUMP, DUCK, WALK, ATTACK, ATTACK2,
    /// RELOAD, USE...). The sim derives press edges for actions that need them.
    pub buttons: u32,
    /// Inventory slot to switch to (1 primary, 2 pistol, 3 knife, 5 bomb;
    /// pressing the current slot again cycles within it), [`SLOT_LAST`] for
    /// the previous weapon, [`SLOT_NEXT`]/[`SLOT_PREVIOUS`] for one inventory
    /// step resolved against current authority, 0 for none.
    pub weapon_slot: u8,
    /// Something to buy this tick.
    pub buy: Purchase,
    /// Drop the active weapon.
    pub drop: bool,
    /// Select a carried grenade by item definition; never grants an item.
    pub utility: u32,
    /// Tracked head and hands (VR). `None` for flat players.
    pub tracked: Option<TrackedPose>,
    /// Weapon aim, when it differs from the view (VR). Shots, knife swings
    /// and throws use it instead of [`TickInput::view_angles`].
    pub aim: Option<AimRay>,
    /// Server-validated historical target tick. Clients do not choose this
    /// field directly; the session derives it from the bounded view timeline.
    pub rewind_tick: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub team: Team,
    pub is_bot: bool,
    pub movement: PlayerMove,
    pub view_angles: Vec3,
    pub health: i32,
    pub armor: i32,
    pub helmet: bool,
    pub defuse_kit: bool,
    pub money: i32,
    pub alive: bool,
    /// Replicated spawn/placement epoch. Historical hits cannot cross lives.
    pub life_epoch: u64,
    pub respawn_at: f32,
    pub flash_until: f32,
    /// Ordered by inventory slot.
    pub weapons: Vec<WeaponState>,
    pub active: usize,
    pub last_active: usize,
    pub kills: u32,
    pub deaths: u32,
    pub shots: u64,
    /// Authoritative animation clock, shared with model hit registration.
    pub animation: model_pose::PoseState,
    pub in_buy_zone: bool,
    /// Sim time a bomb plant started, while planting.
    pub plant_start: Option<f32>,
    pub last_input: TickInput,
    pub last_events: MoveEvents,
    /// Tracked head and hands from the latest input (VR), for other
    /// players to draw.
    pub tracked: Option<TrackedPose>,
    /// Weapon aim from the latest input, already clamped (VR).
    pub aim: Option<AimRay>,
    /// What this player chose to carry and how it looks.
    pub loadout: Loadout,
}

impl Player {
    pub fn active_weapon_mut(&mut self) -> Option<&mut WeaponState> {
        self.weapons.get_mut(self.active)
    }
    pub fn active_weapon(&self) -> Option<&WeaponState> {
        self.weapons.get(self.active)
    }

    pub fn eye(&self) -> Vec3 {
        self.movement.eye_position()
    }

    /// Where shots start and the angles they leave at: the eye and view
    /// angles, or the tracked aim ray when there is one.
    pub fn aim_origin_angles(&self) -> (Vec3, Vec3) {
        match self.aim {
            Some(aim) => {
                let d = aim.direction;
                let yaw = d.y.atan2(d.x).to_degrees();
                let pitch = -d.z.atan2(d.truncate().length()).to_degrees();
                (self.movement.origin + aim.origin, Vec3::new(pitch, yaw, 0.0))
            }
            None => (self.eye(), self.view_angles),
        }
    }

    /// Recoil offset the camera shows (degrees, pitch/yaw).
    pub fn view_punch(&self) -> [f32; 2] {
        let p = self.active_weapon().map_or([0.0, 0.0], WeaponState::aim_punch);
        let k = weapon::RECOIL_SCALE * weapon::VIEW_PUNCH_FRACTION;
        [p[0] * k, p[1] * k]
    }

    /// The hull the player occupies (mins, maxs) in world space.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let (mins, maxs) = self.movement.hull();
        (self.movement.origin + mins, self.movement.origin + maxs)
    }

    pub fn has(&self, class_name: &str) -> bool {
        self.weapons.iter().any(|w| w.def.class_name == class_name)
    }

    /// Shared inventory capacity for buying and collecting grenades.
    fn can_carry_grenade(&self, def: &weapons::WeaponDefinition) -> bool {
        let count = self.weapons.iter().filter(|w| w.def.class_name == def.class_name).count();
        let total = self.weapons.iter().filter(|w| w.def.class == weapons::WeaponClass::Grenade).count();
        total < 4 && count < if def.class_name == "weapon_flashbang" { 2 } else { 1 }
            && !(utility::Kind::from_name(&def.class_name).is_some_and(|k| k.fire())
                && self.weapons.iter().any(|w| utility::Kind::from_name(&w.def.class_name).is_some_and(|k| k.fire())))
    }

    pub fn has_bomb(&self) -> bool {
        self.has(C4)
    }

    /// Index of the first weapon in an inventory slot.
    pub fn weapon_in_slot(&self, slot: u8) -> Option<usize> {
        self.weapons.iter().position(|w| weapon::slot(&w.def) == slot)
    }

    /// The weapon to hold by default: primary, else pistol, else knife.
    pub fn best_weapon(&self) -> usize {
        (1..=5).find_map(|s| self.weapon_in_slot(s)).unwrap_or(0)
    }

    /// Adds a weapon in slot order, keeping the active selection.
    fn give(&mut self, w: WeaponState) -> usize {
        let slot = weapon::slot(&w.def);
        let at = self.weapons.iter().position(|x| weapon::slot(&x.def) > slot).unwrap_or(self.weapons.len());
        self.weapons.insert(at, w);
        if self.weapons.len() > 1 {
            if self.active >= at {
                self.active += 1;
            }
            if self.last_active >= at {
                self.last_active += 1;
            }
        }
        at
    }

    /// Removes a weapon; if it was held, switches to the best remaining.
    fn take(&mut self, index: usize, time: f32) -> WeaponState {
        let mut w = self.weapons.remove(index);
        // A dropped gun cannot finish its owner's reload on the ground or
        // in another player's hands.
        w.reload_end = None;
        w.grenade_throw = None;
        w.cancel_burst();
        w.cancel_suppressor(time);
        w.reset_zoom(time);
        let was_active = self.active == index;
        if self.active > index {
            self.active -= 1;
        }
        if self.last_active > index {
            self.last_active -= 1;
        }
        self.last_active = self.last_active.min(self.weapons.len().saturating_sub(1));
        if was_active {
            self.active = 0;
            let best = self.best_weapon();
            self.select(best, time);
        }
        self.active = self.active.min(self.weapons.len().saturating_sub(1));
        w
    }

    fn select(&mut self, index: usize, time: f32) {
        if index >= self.weapons.len() {
            return;
        }
        if index!=self.active && self.active_weapon().is_some_and(|w| w.grenade_throw.is_some_and(|s|s.phase==utility::ThrowPhase::Throwing)) {return;}
        if let Some(old) = self.weapons.get_mut(self.active) {
            if index!=self.active {old.grenade_throw=None; old.cancel_suppressor(time); old.cancel_burst();}
            old.reset_zoom(time);
            old.reload_end = None;
        }
        if index != self.active {
            self.last_active = self.active;
        }
        self.active = index;
        self.plant_start = None;
        let w = &mut self.weapons[index];
        w.reload_end = None;
        w.reset_zoom(time);
        w.next_attack = w.next_attack.max(time + weapon::deploy_time(&w.def));
    }
}

/// A spawn point from the map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnPoint {
    pub origin: Vec3,
    pub yaw: f32,
    pub team: Team,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GroundKind {
    Weapon(WeaponState),
    DefuseKit,
}

/// Something lying on the ground that players pick up by walking over it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroundItem {
    pub id: u32,
    pub kind: GroundKind,
    pub pos: Vec3,
    pub yaw: f32,
    pub dropped_by: PlayerId,
    /// The dropper cannot pick it back up before this time.
    pub pickup_after: f32,
    /// Support-loss motion only; initial drop placement still uses throw_point.
    pub vertical_velocity: f32,
    /// Resting anchor; collision changes recheck support before the next fall.
    pub settled: bool,
}

impl GroundItem {
    pub fn class_name(&self) -> &str {
        match &self.kind {
            GroundKind::Weapon(w) => &w.def.class_name,
            GroundKind::DefuseKit => "item_defuser",
        }
    }
}

/// The ticking bomb.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlantedBomb {
    pub pos: Vec3,
    /// Index into [`Objectives::bomb_sites`].
    pub site: usize,
    pub planter: PlayerId,
    pub planted_at: f32,
    pub explode_at: f32,
    /// The CT defusing it and when the defuse completes.
    pub defuser: Option<(PlayerId, f32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeleeHit { Miss, World, Player }

/// Things that happened during a tick, for effects, sounds and the HUD.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GameEvent {
    StatTrakKill { owner:PlayerId, serial:u64 },
    /// `weapon` is the throw-time weapon ContentId, independent of current inventory.
    DecoyShot { player: PlayerId, pos: Vec3, weapon: String },
    UtilityThrown { player: PlayerId, kind: utility::Kind, pos: Vec3 },
    UtilityDetonated { player: PlayerId, kind: utility::Kind, pos: Vec3 },
    Melee { player: PlayerId, weapon: String, origin: Vec3, secondary: bool, hit: MeleeHit },
    Scope { player: PlayerId, weapon: String, origin: Vec3, level: u8 },
    /// Shot-time mode for presentation; later input or weapon state may differ.
    Fire { player: PlayerId, weapon: String, origin: Vec3, suppressed: bool, secondary: bool },
    Tracer { player: PlayerId, start: Vec3, end: Vec3 },
    /// A bullet hit the world. `player` fired it.
    Impact { player: PlayerId, pos: Vec3, normal: Vec3, surface: u32, door: Option<doors::Impact> },
    Damage { attacker: PlayerId, victim: PlayerId, health: i32, group: HitGroup, pos: Vec3, protection: weapon::HitProtection },
    /// `attacker` is [`WORLD`] for the bomb.
    Kill { attacker: PlayerId, victim: PlayerId, weapon: String, headshot: bool, death: death::DeathReceipt },
    Reload { player: PlayerId },
    Jump { player: PlayerId },
    Land { player: PlayerId, speed: f32 },
    RoundStart { round: u32 },
    FreezeEnd,
    RoundEnd { reason: RoundEndReason },
    Halftime,
    MatchEnd { winner: Option<Team> },
    Purchase { player: PlayerId, item: Purchase },
    Pickup { player: PlayerId, item: String },
    Dropped { player: PlayerId, item: String, pos: Vec3 },
    PlantStart { player: PlayerId },
    BombPlanted { player: PlayerId, site: char, pos: Vec3 },
    DefuseStart { player: PlayerId, kit: bool },
    BombDefused { player: PlayerId },
    BombExploded { pos: Vec3 },
    /// A moving grenade contacted the world; the cue stays at the contact.
    /// Appended to preserve all earlier event discriminants.
    UtilityBounced { player: PlayerId, kind: utility::Kind, pos: Vec3 },
    /// Timed decoy expiry, distinct from activation and round cleanup.
    DecoyExpired { player: PlayerId, pos: Vec3 },
    /// An admitted mode change, separate from a shot or an authored animation.
    /// Appended to preserve earlier event discriminants.
    BurstModeChanged { player: PlayerId, weapon: String, origin: Vec3, burst: bool },
    /// Fully depleted primary-trigger feedback; no live shot or ammo change.
    /// Appended so earlier event discriminants retain their meaning.
    EmptyTrigger { player: PlayerId, weapon: String, origin: Vec3 },
    DoorSound { door: u32, cue: doors::Cue, pos: Vec3 },
}

/// The whole game: map data shared by every copy (`Arc`s), the replicated
/// state ([`WorldState`]) and the server-side bot brains. `Clone` is cheap
/// enough to snapshot every tick for rewinds and tests.
#[derive(Clone)]
pub struct World {
    pub collision: Arc<CollisionWorld>,
    /// Replicated pose/lifecycle; immutable definitions stay with the map.
    pub doors: Vec<doors::State>,
    pub breakables: Vec<breakables::State>,
    pub catalog: Arc<weapons::Catalog>,
    pub config: MoveConfig,
    pub mode: GameMode,
    pub rules: RulesConfig,
    pub round: RoundState,
    pub objectives: Objectives,
    pub players: Vec<Player>,
    pub spawns: Vec<SpawnPoint>,
    pub items: Vec<GroundItem>,
    pub projectiles: Vec<utility::Projectile>,
    pub utility_fields: Vec<utility::Field>,
    pub bomb: Option<PlantedBomb>,
    pub tick: u64,
    /// Ticks per second ([`World::set_tick_rate`]).
    pub tick_rate: u32,
    /// Events from the most recent step.
    pub events: Vec<GameEvent>,
    /// AI for the players with `is_bot`. They think at the start of every
    /// [`World::step`] on whoever runs the authoritative world.
    pub bots: bots::Bots,
    /// Optional server-only hit hull history, never part of WorldState.
    rewind: Option<rewind::History>,
    character_profiles: Arc<model_pose::Profiles>,
    character_hitboxes: std::collections::HashMap<PlayerId, model_pose::CachedHitboxes>,
    next_id: PlayerId,
    next_item: u32,
    /// Derived support-query cache, invalidated on restore. Runtime door and
    /// breakable changes replace the collision Arc; unchanged floors need no
    /// per-item stationary traces. GroundItem carries the replicated rest/motion.
    ground_support_collision: Option<Arc<CollisionWorld>>,
}

/// Slab test: entry distance of a ray into an axis-aligned box.
fn ray_box(origin: Vec3, dir: Vec3, mins: Vec3, maxs: Vec3) -> Option<f32> {
    let mut t0 = 0.0f32;
    let mut t1 = f32::MAX;
    for a in 0..3 {
        if dir[a].abs() < 1e-8 {
            if origin[a] < mins[a] || origin[a] > maxs[a] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / dir[a];
        let (mut near, mut far) = ((mins[a] - origin[a]) * inv, (maxs[a] - origin[a]) * inv);
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        t0 = t0.max(near);
        t1 = t1.min(far);
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

impl World {
    pub fn new(collision: Arc<CollisionWorld>) -> Self {
        Self::with_catalog(collision, Arc::new(weapons::Catalog::default()))
    }

    pub fn with_catalog(collision: Arc<CollisionWorld>, catalog: Arc<weapons::Catalog>) -> Self {
        let rules = RulesConfig { solid_teammates: 0, ..RulesConfig::default() };
        let mut world = Self {
            collision,
            doors: Vec::new(),
            breakables: Vec::new(),
            catalog,
            config: MoveConfig::default(),
            mode: GameMode::Deathmatch,
            round: RoundState::new(&rules, 0.0),
            rules,
            objectives: Objectives::default(),
            players: Vec::new(),
            spawns: Vec::new(),
            items: Vec::new(),
            projectiles: Vec::new(),
            utility_fields: Vec::new(),
            bomb: None,
            tick: 0,
            tick_rate: DEFAULT_TICK_RATE,
            events: Vec::new(),
            bots: bots::Bots::default(),
            rewind: None,
            character_profiles: Arc::default(),
            character_hitboxes: Default::default(),
            next_id: 1,
            next_item: 1,
            ground_support_collision: None,
        };
        world.reset_breakables();
        world.reset_doors();
        world
    }

    /// Sim time in seconds.
    pub fn time(&self) -> f32 {
        self.tick as f32 / self.tick_rate as f32
    }

    /// Seconds per tick.
    pub fn dt(&self) -> f32 {
        1.0 / self.tick_rate as f32
    }

    /// Whole ticks in a duration (at least one for any positive duration).
    pub fn ticks(&self, seconds: f32) -> u64 {
        ((seconds * self.tick_rate as f32).round() as u64).max((seconds > 0.0) as u64)
    }

    /// Changes the tick rate (clamped to [`TICK_RATE_RANGE`]), keeping the
    /// current sim time, so timers already running still end on time.
    pub fn set_tick_rate(&mut self, rate: u32) {
        let rate = rate.clamp(*TICK_RATE_RANGE.start(), *TICK_RATE_RANGE.end());
        let now = self.time() as f64;
        let old_rate = self.tick_rate;
        for player in &mut self.players {
            player.animation.last_fire_tick = player.animation.last_fire_tick
                .map(|tick| ((tick as f64 / old_rate as f64) * rate as f64).round() as u64);
            for weapon in &mut player.weapons { weapon.retime_recoil_motion(rate); }
        }
        for item in &mut self.items {
            if let GroundKind::Weapon(weapon) = &mut item.kind { weapon.retime_recoil_motion(rate); }
        }
        self.character_hitboxes.clear();
        self.tick_rate = rate;
        self.tick = (now * rate as f64).round() as u64;
        self.rewind = None;
    }

    pub fn competitive(&self) -> bool {
        self.mode == GameMode::Competitive
    }

    /// Reads initially enabled `info_player_terrorist` / `info_player_counterterrorist`.
    pub fn load_spawns(&mut self, map: &bsp::Bsp) {
        self.spawns.clear();
        for (class, team) in [
            ("info_player_terrorist", Team::Terrorist),
            ("info_player_counterterrorist", Team::CounterTerrorist),
        ] {
            for ent in map.entities_of_class(class) {
                // Alternative mode spawns can live in the same BSP (for
                // example Guardian and 2v2). They require an explicit Enable
                // input; merely loading the map must not activate them.
                if ent.str("enabled").and_then(kv::parse_i64) == Some(0) {
                    continue;
                }
                let Some(origin) = ent.str("origin").and_then(bsp::parse_origin) else {
                    continue;
                };
                let yaw = ent.str("angles").map(kv_angles).map(|a| a.y).unwrap_or(0.0);
                self.spawns.push(SpawnPoint { origin, yaw, team });
            }
        }
    }

    /// Spawns plus buy zones and bomb sites. `overview` is the map's radar
    /// overview file text, used to name the sites.
    pub fn load_map(&mut self, map: &bsp::Bsp, overview: Option<&str>) {
        self.load_spawns(map);
        self.objectives = Objectives::from_bsp(map, &self.spawns, overview);
    }

    /// Switches rule sets. Competitive starts a fresh match: call it after
    /// adding the players so the bomb goes to one of the Terrorists.
    pub fn set_mode(&mut self, mode: GameMode, rules: RulesConfig) {
        self.mode = mode;
        self.rules = rules;
        self.reset_breakables();
        self.reset_doors();
        if mode == GameMode::Special { self.players.retain(|p|!p.is_bot); for p in &mut self.players {p.money=0;} }
        match mode {
            GameMode::Competitive => self.start_match(),
            GameMode::Deathmatch | GameMode::Special => {
                self.items.clear();
        self.projectiles.clear();
        self.utility_fields.clear();
        for p in &mut self.players { p.flash_until=0.0; }
                self.bomb = None;
                let ids: Vec<PlayerId> = self.players.iter().map(|p| p.id).collect();
                for id in ids {
                    self.respawn(id);
                }
            }
        }
    }

    /// Drops a point onto the floor below it (as the game does for spawns).
    pub fn drop_to_floor(&self, origin: Vec3, ducked: bool) -> Vec3 {
        let (mins, maxs) = if ducked {
            (movement::DUCK_MINS, movement::DUCK_MAXS)
        } else {
            (movement::STAND_MINS, movement::STAND_MAXS)
        };
        let start = origin + Vec3::new(0.0, 0.0, 2.0);
        let tr = self
            .collision
            .trace_box(start, origin - Vec3::new(0.0, 0.0, 256.0), mins, maxs, contents::MASK_PLAYERSOLID);
        if tr.start_solid || tr.fraction == 1.0 { origin } else { tr.end }
    }

    pub fn add_player(&mut self, name: &str, team: Team) -> PlayerId {
        self.add_player_ex(name, team, false)
    }

    pub fn add_bot(&mut self, name: &str, team: Team) -> PlayerId {
        self.add_player_ex(name, team, true)
    }

    fn add_player_ex(&mut self, name: &str, team: Team, is_bot: bool) -> PlayerId {
        let id = self.next_id;
        self.next_id += 1;
        self.players.push(Player {
            id,
            name: name.to_string(),
            team,
            is_bot,
            movement: PlayerMove::default(),
            view_angles: Vec3::ZERO,
            health: 100,
            armor: 0,
            helmet: false,
            defuse_kit: false,
            money: if self.mode == GameMode::Special { 0 } else { self.rules.start_money },
            alive: false,
            life_epoch: 0,
            respawn_at: 0.0,
            flash_until: 0.0,
            weapons: Vec::new(),
            active: 0,
            last_active: 0,
            kills: 0,
            deaths: 0,
            shots: 0,
            animation: model_pose::PoseState::default(),
            in_buy_zone: false,
            plant_start: None,
            last_input: TickInput::default(),
            last_events: MoveEvents::default(),
            tracked: None,
            aim: None,
            loadout: Loadout::default(),
        });
        match self.mode {
            GameMode::Deathmatch | GameMode::Special => self.respawn(id),
            // Joining mid-round waits for the next one.
            GameMode::Competitive if self.round.phase == Phase::Freeze => {
                self.spawn_for_round(id, false);
                let bomb_out = self.bomb.is_some() || self.players.iter().any(Player::has_bomb) || self.items.iter().any(|i| i.class_name() == C4);
                if team == Team::Terrorist && !bomb_out {
                    self.give_weapon(id, C4);
                }
            }
            GameMode::Competitive => {}
        }
        id
    }

    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.players.iter().find(|p| p.id == id)
    }

    /// Removes a departed player under server authority. Competitive gear,
    /// especially the C4, stays available instead of vanishing on disconnect.
    pub fn remove_player(&mut self, id: PlayerId) -> bool {
        let Some(i) = self.index_of(id) else { return false; };
        self.death_drops(i);
        if let Some(bomb) = &mut self.bomb {
            if bomb.defuser.is_some_and(|(defuser, _)| defuser == id) {
                bomb.defuser = None;
            }
        }
        let removed = self.players.remove(i);
        if removed.is_bot {
            self.bots.forget_player(id);
        }
        true
    }

    pub fn player_mut(&mut self, id: PlayerId) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.id == id)
    }

    fn index_of(&self, id: PlayerId) -> Option<usize> {
        self.players.iter().position(|p| p.id == id)
    }

    fn weapon(&self, class_name: &str) -> Option<WeaponState> {
        self.catalog.get(class_name).map(|d| WeaponState::new(Arc::new(d.clone())))
    }

    fn give_weapon(&mut self, id: PlayerId, class_name: &str) {
        if let Some(w) = self.weapon(class_name) {
            if let Some(p) = self.player_mut(id) {
                p.give(w);
            }
        }
    }

    /// Deathmatch loadout: a rifle, a pistol and a knife.
    fn loadout(&self, team: Team) -> Vec<WeaponState> {
        let names: &[&str] = match team {
            Team::Terrorist => &["weapon_ak47", "weapon_glock", "weapon_knife_t"],
            _ => &["weapon_m4a1_silencer", "weapon_usp_silencer", "weapon_knife"],
        };
        names.iter().filter_map(|n| self.weapon(n)).collect()
    }

    /// Competitive starting inventory: knife and the default pistol.
    fn default_inventory(&self, team: Team) -> Vec<WeaponState> {
        let (pistol, knife) = match team {
            Team::Terrorist => (&self.rules.t_default_secondary, &self.rules.t_default_melee),
            _ => (&self.rules.ct_default_secondary, &self.rules.ct_default_melee),
        };
        [pistol, knife].iter().filter_map(|n| self.weapon(n)).collect()
    }

    /// A free spawn point for a team (not within 40 units of a live player).
    fn pick_spawn(&self, id: PlayerId, team: Team) -> SpawnPoint {
        let taken: Vec<Vec3> = self
            .players
            .iter()
            .filter(|p| p.id != id && p.alive)
            .map(|p| p.movement.origin)
            .collect();
        let offset = (self.tick as usize + id as usize * 7 + self.round.round as usize * 3) % self.spawns.len().max(1);
        let team_spawns: Vec<SpawnPoint> = self.spawns.iter().copied().filter(|s| s.team == team).collect();
        (0..team_spawns.len())
            .map(|i| team_spawns[(i + offset) % team_spawns.len()])
            .find(|s| taken.iter().all(|t| t.distance(s.origin) > 40.0))
            .or_else(|| team_spawns.first().copied())
            .or_else(|| self.spawns.first().copied())
            .unwrap_or(SpawnPoint { origin: Vec3::ZERO, yaw: 0.0, team })
    }

    fn place(&mut self, id: PlayerId) {
        let Some(team) = self.player(id).map(|p| p.team) else { return };
        let spawn = self.pick_spawn(id, team);
        let contacts = player_contact::PlayerWorld::new(self.collision.as_ref(), &self.players, id, self.rules.solid_teammates);
        let hit=movement::TraceWorld::trace_hull(&contacts,spawn.origin+Vec3::Z*2.,spawn.origin-Vec3::Z*256.,
            movement::STAND_MINS,movement::STAND_MAXS,contents::MASK_PLAYERSOLID);
        let origin=if hit.start_solid || hit.fraction==1. {spawn.origin} else {hit.end};
        if let Some(p) = self.player_mut(id) {
            p.movement = PlayerMove::spawn_at(origin);
            p.view_angles = Vec3::new(0.0, spawn.yaw, 0.0);
            p.health = 100;
            p.flash_until = 0.0;
            p.alive = true;
            p.life_epoch = p.life_epoch.saturating_add(1);
            p.animation = model_pose::PoseState::default();
            p.plant_start = None;
        }
    }

    /// Deathmatch respawn: a free spawn point, fully equipped.
    pub fn respawn(&mut self, id: PlayerId) {
        let Some(team) = self.player(id).map(|p| p.team) else { return };
        if team == Team::Spectator {
            return;
        }
        self.place(id);
        let special = self.mode == GameMode::Special;
        let loadout = if special {
            self.default_inventory(team)
        } else {
            let mut guns = self.loadout(team);
            if let Some(p) = self.player(id) {
                p.loadout.practice_weapons(&self.catalog, team, &mut guns, self.rules.grenade_practice);
                if self.mode == GameMode::Deathmatch && self.rules.grenade_practice && !p.is_bot {
                    for name in ["weapon_hegrenade", "weapon_flashbang", "weapon_smokegrenade",
                        "weapon_molotov", "weapon_incgrenade", "weapon_decoy"] {
                        if let Some(grenade) = self.weapon(name) { guns.push(grenade); }
                    }
                }
            }
            guns
        };
        if let Some(p) = self.player_mut(id) {
            p.armor = if special { 0 } else { 100 };
            p.helmet = !special;
            p.defuse_kit = false;
            p.weapons = loadout;
            p.active = p.best_weapon();
            p.last_active = p.active;
        }
    }

    /// Round start placement. Survivors (`keep`) keep their equipment.
    fn spawn_for_round(&mut self, id: PlayerId, keep: bool) {
        let Some(team) = self.player(id).map(|p| p.team) else { return };
        if team == Team::Spectator {
            return;
        }
        self.place(id);
        let fresh = self.default_inventory(team);
        let now = self.time();
        if let Some(p) = self.player_mut(id) {
            if !keep {
                p.weapons = fresh;
                p.armor = 0;
                p.helmet = false;
                p.defuse_kit = false;
            }
            for w in &mut p.weapons {
                w.reset_zoom(now);
                w.reload_end = None;
                w.recoil_index = 0.0;
                w.reset_recoil_motion();
                w.accuracy_penalty = 0.0;
            }
            p.active = p.best_weapon();
            p.last_active = p.weapon_in_slot(3).unwrap_or(p.active);
        }
    }

    fn start_match(&mut self) {
        self.round = RoundState::new(&self.rules, self.time());
        for p in &mut self.players {
            p.money = self.rules.start_money;
            p.kills = 0;
            p.deaths = 0;
        }
        self.start_round(false);
    }

    fn start_round(&mut self, keep_survivors: bool) {
        let now = self.time();
        self.reset_breakables();
        self.reset_doors();
        self.items.clear();
        self.projectiles.clear();
        self.utility_fields.clear();
        for p in &mut self.players { p.flash_until=0.0; }
        self.bomb = None;
        self.round.phase = Phase::Freeze;
        self.round.phase_start = now;
        self.round.live_start = now + self.rules.freeze_time;
        self.round.bomb_planted = false;
        self.round.last_result = None;
        let survivors: Vec<PlayerId> = if keep_survivors {
            self.players.iter().filter(|p| p.alive).map(|p| p.id).collect()
        } else {
            Vec::new()
        };
        for p in &mut self.players {
            p.alive = false;
            // A bomb carried over from last round is handed out afresh.
            if let Some(i) = p.weapons.iter().position(|w| w.def.class_name == C4) {
                p.weapons.remove(i);
                p.active = 0;
                p.last_active = 0;
            }
        }
        let ids: Vec<PlayerId> = self.players.iter().map(|p| p.id).collect();
        for id in ids {
            self.spawn_for_round(id, survivors.contains(&id));
        }
        // The bomb goes to a random Terrorist.
        let ts: Vec<PlayerId> = self.players.iter().filter(|p| p.team == Team::Terrorist && p.alive).map(|p| p.id).collect();
        if !ts.is_empty() {
            let mut rng = Rng::from_parts(&[self.round.round as u64, self.tick, 0xC4]);
            let carrier = ts[(rng.next_u64() % ts.len() as u64) as usize];
            self.give_weapon(carrier, C4);
        }
        self.events.push(GameEvent::RoundStart { round: self.round.round });
    }

    fn next_round(&mut self) {
        let halftime = self.rules.halftime && self.round.round == self.rules.max_rounds / 2;
        self.round.round += 1;
        if halftime {
            for p in &mut self.players {
                p.team = p.team.other();
                p.money = self.rules.start_money;
            }
            let r = &mut self.round;
            std::mem::swap(&mut r.score_t, &mut r.score_ct);
            r.losses_t = self.rules.starting_losses;
            r.losses_ct = self.rules.starting_losses;
            self.events.push(GameEvent::Halftime);
        }
        self.start_round(!halftime);
    }

    fn end_round(&mut self, reason: RoundEndReason) {
        let now = self.time();
        self.round.phase = Phase::Over;
        self.round.phase_start = now;
        self.round.last_result = Some(reason);
        let winner = reason.winner();
        match winner {
            Some(Team::Terrorist) => {
                self.round.score_t += 1;
                self.round.losses_t = self.round.losses_t.saturating_sub(1);
                self.round.losses_ct = (self.round.losses_ct + 1).min(5);
            }
            Some(Team::CounterTerrorist) => {
                self.round.score_ct += 1;
                self.round.losses_ct = self.round.losses_ct.saturating_sub(1);
                self.round.losses_t = (self.round.losses_t + 1).min(5);
            }
            _ => {}
        }
        let win_cash = match reason {
            RoundEndReason::TargetBombed => self.rules.win_bomb,
            RoundEndReason::BombDefused => self.rules.win_defuse,
            RoundEndReason::TargetSaved => self.rules.win_time,
            RoundEndReason::TerroristsEliminated | RoundEndReason::CounterTerroristsEliminated => self.rules.win_elimination,
            RoundEndReason::Draw => 0,
        };
        let planted = self.round.bomb_planted;
        let (losses_t, losses_ct) = (self.round.losses_t, self.round.losses_ct);
        for p in &mut self.players {
            if p.team == Team::Spectator {
                continue;
            }
            let cash = if Some(p.team) == winner {
                win_cash
            } else if winner.is_none() {
                0
            } else if reason == RoundEndReason::TargetSaved && p.team == Team::Terrorist && p.alive {
                // Terrorists who hide out the clock get nothing.
                0
            } else {
                let losses = if p.team == Team::Terrorist { losses_t } else { losses_ct };
                let mut c = self.rules.loss_bonus(losses);
                if p.team == Team::Terrorist && planted {
                    c += self.rules.planted_but_defused;
                }
                c
            };
            p.money = (p.money + cash).min(self.rules.max_money);
        }
        self.events.push(GameEvent::RoundEnd { reason });

        let to_win = self.rules.rounds_to_win();
        let (t, ct) = (self.round.score_t, self.round.score_ct);
        let clinched = self.rules.can_clinch && (t >= to_win || ct >= to_win);
        if clinched || self.round.round >= self.rules.max_rounds {
            let winner = match t.cmp(&ct) {
                std::cmp::Ordering::Greater => Some(Team::Terrorist),
                std::cmp::Ordering::Less => Some(Team::CounterTerrorist),
                std::cmp::Ordering::Equal => None,
            };
            self.round.phase = Phase::MatchOver;
            self.round.match_winner = winner;
            self.events.push(GameEvent::MatchEnd { winner });
        }
    }

    /// Timers that run before players move: freeze time, round clock,
    /// bomb, next round.
    fn update_round(&mut self, time: f32) {
        if let Some(b) = self.bomb {
            if time >= b.explode_at {
                self.explode(b.pos);
                if self.round.phase == Phase::Live {
                    self.end_round(RoundEndReason::TargetBombed);
                }
            } else if let Some((defuser, _)) = b.defuser {
                if !self.player(defuser).is_some_and(|p| p.alive) {
                    if let Some(b) = &mut self.bomb {
                        b.defuser = None;
                    }
                }
            }
        }
        let r = &self.round;
        match r.phase {
            Phase::Freeze if time >= r.phase_start + self.rules.freeze_time => {
                self.round.phase = Phase::Live;
                self.round.live_start = time;
                self.events.push(GameEvent::FreezeEnd);
            }
            Phase::Live if self.bomb.is_none() && time >= r.live_start + self.rules.round_time => {
                self.end_round(RoundEndReason::TargetSaved);
            }
            Phase::Over if time >= r.phase_start + self.rules.restart_delay => self.next_round(),
            Phase::MatchOver if time >= r.phase_start + self.rules.restart_delay * 2.0 => self.start_match(),
            _ => {}
        }
    }

    fn check_elimination(&mut self) {
        if self.round.phase != Phase::Live {
            return;
        }
        let count = |team: Team, alive_only: bool| {
            self.players.iter().filter(|p| p.team == team && (p.alive || !alive_only)).count()
        };
        if count(Team::CounterTerrorist, false) > 0 && count(Team::CounterTerrorist, true) == 0 {
            self.end_round(RoundEndReason::CounterTerroristsEliminated);
        } else if count(Team::Terrorist, false) > 0 && count(Team::Terrorist, true) == 0 && self.bomb.is_none() {
            self.end_round(RoundEndReason::TerroristsEliminated);
        }
    }

    fn explode(&mut self, pos: Vec3) {
        self.bomb = None;
        self.events.push(GameEvent::BombExploded { pos });
        let sigma = BOMB_RADIUS / 3.0;
        for i in 0..self.players.len() {
            let p = &self.players[i];
            if !p.alive {
                continue;
            }
            let d = (p.movement.origin + Vec3::Z * 36.0).distance(pos);
            let dmg = BOMB_DAMAGE * (-(d * d) / (2.0 * sigma * sigma)).exp();
            if dmg < 1.0 {
                continue;
            }
            let (health_dmg, armor_dmg) = if p.armor > 0 {
                let absorbed = (dmg * 0.5).min(p.armor as f32 * 2.0);
                (dmg - absorbed, absorbed * 0.5)
            } else {
                (dmg, 0.0)
            };
            let health_dmg = health_dmg.round().max(1.0) as i32;
            let p = &mut self.players[i];
            p.health -= health_dmg;
            p.armor = (p.armor - armor_dmg as i32).max(0);
            let victim = p.id;
            let at = p.movement.origin;
            self.events.push(GameEvent::Damage {
                attacker: WORLD,
                victim,
                health: health_dmg,
                group: HitGroup::Chest,
                pos: at,
                protection: if armor_dmg > 0.0 { weapon::HitProtection::Kevlar } else { weapon::HitProtection::Flesh },
            });
            if self.players[i].health <= 0 {
                self.kill(i, None, "planted_c4", false, None);
            }
        }
    }

    fn new_item(&mut self, kind: GroundKind, pos: Vec3, yaw: f32, dropped_by: PlayerId, pickup_after: f32) {
        let id = self.next_item;
        self.next_item += 1;
        self.items.push(GroundItem { id, kind, pos, yaw, dropped_by, pickup_after,
            vertical_velocity: 0.0, settled: false });
    }

    /// Initial drop anchor: a little ahead, on the floor when one is found.
    fn throw_point(&self, p: &Player, distance: f32) -> Vec3 {
        let (fwd, _, _) = movement::angle_vectors(Vec3::new(0.0, p.view_angles.y, 0.0));
        let start = p.movement.origin + Vec3::Z * 36.0;
        let tr = self.collision.trace_ray(start, start + fwd * distance, contents::MASK_PLAYERSOLID);
        let ahead = start + (tr.end - start) * 0.8;
        let down = self.collision.trace_ray(ahead, ahead - Vec3::Z * 512.0, contents::MASK_PLAYERSOLID);
        if down.hit() {
            down.end + Vec3::Z * ground_items::SUPPORT_OFFSET
        } else {
            // No support in range: start here and let the tick-owned fall
            // advance the item, rather than teleporting it to the ray's end.
            ahead
        }
    }

    /// Drops one inventory item on the ground.
    fn drop_weapon(&mut self, i: usize, index: usize, distance: f32) {
        if self.players[i].weapons.get(index).is_some_and(|w|w.grenade_throw.is_some_and(|s|s.phase==utility::ThrowPhase::Throwing)) {return;}
        let time = self.time();
        let pos = self.throw_point(&self.players[i], distance);
        let p = &mut self.players[i];
        let id = p.id;
        let yaw = p.view_angles.y;
        let mut w = p.take(index, time);
        // A gun can leave its holder in the same tick it was bought or
        // spawned. Assign its finish before it enters the ground inventory.
        if w.cosmetic_owner.is_none() {
            p.loadout.dress(id, p.team, &self.catalog, &mut w);
        }
        let name = w.def.class_name.clone();
        self.new_item(GroundKind::Weapon(w), pos, yaw, id, time + 1.0);
        self.events.push(GameEvent::Dropped { player: id, item: name, pos });
    }

    /// What a dying player leaves behind: best gun, the bomb, a kit.
    fn death_drops(&mut self, i: usize) {
        if !self.competitive() {
            return;
        }
        let gun = self.players[i].weapon_in_slot(1).or_else(|| self.players[i].weapon_in_slot(2));
        let bomb = self.players[i].weapons.iter().position(|w| w.def.class_name == C4);
        let mut drops: Vec<usize> = gun.into_iter().chain(bomb).collect();
        drops.sort_unstable_by(|a, b| b.cmp(a));
        for (n, index) in drops.into_iter().enumerate() {
            self.drop_weapon(i, index, 8.0 + n as f32 * 20.0);
        }
        if std::mem::take(&mut self.players[i].defuse_kit) {
            let p = &self.players[i];
            let pos = self.throw_point(p, 24.0);
            let (id, yaw, time) = (p.id, p.view_angles.y, self.time());
            self.new_item(GroundKind::DefuseKit, pos, yaw, id, time);
        }
    }

    fn pickups(&mut self, i: usize, time: f32) {
        let p = &self.players[i];
        let origin = p.movement.origin;
        let mut taken = None;
        for (n, item) in self.items.iter().enumerate() {
            let d = item.pos - origin;
            if d.truncate().length() > PICKUP_RANGE || d.z < -24.0 || d.z > 80.0 {
                continue;
            }
            if item.dropped_by == p.id && time < item.pickup_after {
                continue;
            }
            let ok = match &item.kind {
                GroundKind::DefuseKit => p.team == Team::CounterTerrorist && !p.defuse_kit,
                GroundKind::Weapon(w) if w.def.class_name == C4 => p.team == Team::Terrorist,
                GroundKind::Weapon(w) if w.def.class == weapons::WeaponClass::Grenade => p.can_carry_grenade(&w.def),
                GroundKind::Weapon(w) => p.weapon_in_slot(weapon::slot(&w.def)).is_none() && !p.has(&w.def.class_name),
            };
            if ok {
                // Nearby items on the other side of a wall are not reachable.
                let trace = self.collision.trace_ray(p.eye(), item.pos + Vec3::Z * 4.0, contents::MASK_PLAYERSOLID);
                if trace.hit() {
                    continue;
                }
                taken = Some(n);
                break;
            }
        }
        let Some(n) = taken else { return };
        let item = self.items.remove(n);
        let name = item.class_name().to_string();
        let p = &mut self.players[i];
        match item.kind {
            GroundKind::DefuseKit => p.defuse_kit = true,
            GroundKind::Weapon(w) => {
                let had_gun = p.weapon_in_slot(1).is_some() || p.weapon_in_slot(2).is_some();
                let at = p.give(w);
                if !had_gun && weapon::slot(&p.weapons[at].def) <= 2 {
                    p.select(at, time);
                }
            }
        }
        let id = p.id;
        self.events.push(GameEvent::Pickup { player: id, item: name });
    }

    /// What a purchase would cost, or why it is not allowed right now.
    pub fn buy_cost(&self, id: PlayerId, item: Purchase) -> Result<i32, BuyError> {
        let time = self.time();
        let p = self.player(id).ok_or(BuyError::Unknown)?;
        // The authoritative offline practice rules opt into a free armory.
        // Reuse Purchase in TickInput; clients cannot grant outside this mode.
        if self.mode == GameMode::Deathmatch && self.rules.grenade_practice && !p.is_bot && p.alive {
            if let Purchase::Weapon(index) = item {
                let def = self.catalog.weapons.iter().find(|w| w.def_index == index).ok_or(BuyError::Unknown)?;
                return if matches!(def.class, weapons::WeaponClass::Knife | weapons::WeaponClass::Pistol
                    | weapons::WeaponClass::Smg | weapons::WeaponClass::Rifle | weapons::WeaponClass::Sniper
                    | weapons::WeaponClass::Shotgun | weapons::WeaponClass::MachineGun | weapons::WeaponClass::Grenade) {
                    Ok(0)
                } else { Err(BuyError::Unknown) };
            }
        }
        if !p.alive || (self.mode != GameMode::Special && (!self.competitive() || !self.round.can_buy(&self.rules, time))) {
            return Err(BuyError::NotAllowedNow);
        }
        if self.mode != GameMode::Special && !p.in_buy_zone {
            return Err(BuyError::NotInBuyZone);
        }
        let cost = match item {
            Purchase::None => return Err(BuyError::Unknown),
            Purchase::Weapon(index) => {
                let def = self.catalog.weapons.iter().find(|w| w.def_index == index).ok_or(BuyError::Unknown)?;
                if def.class == weapons::WeaponClass::Knife || def.class_name == C4 || def.price <= 0 {
                    return Err(BuyError::Unknown);
                }
                let side_ok = match def.side {
                    weapons::Side::Both => true,
                    weapons::Side::Terrorist => p.team == Team::Terrorist,
                    weapons::Side::CounterTerrorist => p.team == Team::CounterTerrorist,
                };
                if !side_ok {
                    return Err(BuyError::WrongTeam);
                }
                if if def.class == weapons::WeaponClass::Grenade {
                    !p.can_carry_grenade(def)
                } else { p.has(&def.class_name) } {
                    return Err(BuyError::AlreadyOwned);
                }
                def.price
            }
            Purchase::Kevlar => {
                if p.armor >= 100 {
                    return Err(BuyError::AlreadyOwned);
                }
                rules::KEVLAR_PRICE
            }
            Purchase::KevlarHelmet => {
                let c = if p.armor < 100 { rules::KEVLAR_PRICE } else { 0 } + if p.helmet { 0 } else { rules::HELMET_PRICE };
                if c == 0 {
                    return Err(BuyError::AlreadyOwned);
                }
                c
            }
            Purchase::DefuseKit => {
                if p.team != Team::CounterTerrorist {
                    return Err(BuyError::WrongTeam);
                }
                if p.defuse_kit {
                    return Err(BuyError::AlreadyOwned);
                }
                rules::DEFUSE_KIT_PRICE
            }
        };
        if cost > p.money {
            return Err(BuyError::NotEnoughMoney);
        }
        Ok(cost)
    }

    /// Applies a purchase if the rules allow it.
    pub fn buy(&mut self, id: PlayerId, item: Purchase) -> Result<(), BuyError> {
        let time = self.time();
        let cost = self.buy_cost(id, item)?;
        let i = self.index_of(id).ok_or(BuyError::Unknown)?;
        self.players[i].money -= cost;
        match item {
            Purchase::Weapon(index) => {
                let def = self.catalog.weapons.iter().find(|w| w.def_index == index).cloned().ok_or(BuyError::Unknown)?;
                let slot = weapon::slot(&def);
                let practice = self.mode == GameMode::Deathmatch && self.rules.grenade_practice && !self.players[i].is_bot;
                if practice {
                    let p = &mut self.players[i];
                    // Replace, rather than accumulate guns or knives on each click.
                    p.weapons.retain(|w| if slot <= 3 { weapon::slot(&w.def) != slot }
                        else { w.def.id != def.id });
                    p.active = p.active.min(p.weapons.len().saturating_sub(1));
                    p.last_active = p.last_active.min(p.weapons.len().saturating_sub(1));
                    if slot == 3 { p.loadout.knife = Some(def.class_name.clone()); }
                    if slot <= 2 {
                        let base = if p.team == Team::CounterTerrorist { 2 } else { 0 };
                        p.loadout.practice[base + slot as usize - 1] = Some(def.class_name.clone());
                    }
                } else if slot <= 2 {
                    if let Some(old) = self.players[i].weapon_in_slot(slot) {
                        self.drop_weapon(i, old, 16.0);
                    }
                }
                let p = &mut self.players[i];
                let at = p.give(WeaponState::new(Arc::new(def)));
                if practice || slot == 1 || (slot == 2 && p.weapon_in_slot(1).is_none()) {
                    p.select(at, time);
                }
            }
            Purchase::Kevlar => self.players[i].armor = 100,
            Purchase::KevlarHelmet => {
                self.players[i].armor = 100;
                self.players[i].helmet = true;
            }
            Purchase::DefuseKit => self.players[i].defuse_kit = true,
            Purchase::None => {}
        }
        self.events.push(GameEvent::Purchase { player: id, item });
        Ok(())
    }

    /// Bomb planting. Returns true while the player is planting (movement
    /// is locked).
    fn update_plant(&mut self, i: usize, input: &TickInput, time: f32) -> bool {
        let p = &self.players[i];
        let holding = p.active_weapon().is_some_and(|w| w.def.class_name == C4);
        let attack = input.buttons & buttons::ATTACK != 0;
        let (mn, mx) = p.bounds();
        let site = self.objectives.site_at(mn, mx);
        let able = self.competitive() && self.round.phase == Phase::Live && holding && attack && p.movement.on_ground;
        let (Some(site), true) = (site, able) else {
            self.players[i].plant_start = None;
            return false;
        };
        let id = p.id;
        match p.plant_start {
            None => {
                self.players[i].plant_start = Some(time);
                self.events.push(GameEvent::PlantStart { player: id });
            }
            Some(start) if time - start >= self.rules.plant_time => {
                let p = &mut self.players[i];
                p.plant_start = None;
                let index = p.active;
                p.take(index, time);
                p.money = (p.money + self.rules.player_planted).min(self.rules.max_money);
                let pos = p.movement.origin + Vec3::Z * 1.0;
                self.bomb = Some(PlantedBomb {
                    pos,
                    site,
                    planter: id,
                    planted_at: time,
                    explode_at: time + self.rules.c4_timer,
                    defuser: None,
                });
                self.round.bomb_planted = true;
                let name = self.objectives.bomb_sites[site].name;
                self.events.push(GameEvent::BombPlanted { player: id, site: name, pos });
                return false;
            }
            Some(_) => {}
        }
        true
    }

    /// Bomb defusing. Returns true while the player is defusing.
    fn update_defuse(&mut self, i: usize, input: &TickInput, time: f32) -> bool {
        let Some(bomb) = self.bomb else { return false };
        let p = &self.players[i];
        if p.team != Team::CounterTerrorist || self.round.phase != Phase::Live {
            return false;
        }
        let id = p.id;
        let to = bomb.pos - p.eye();
        let near = to.length() <= DEFUSE_RANGE + 40.0 && (bomb.pos - p.movement.origin).truncate().length() <= DEFUSE_RANGE;
        let holding = input.buttons & buttons::USE != 0
            && near
            && p.movement.on_ground
            && !self.collision.trace_ray(p.eye(), bomb.pos + Vec3::Z * 4.0, contents::MASK_PLAYERSOLID).hit();
        match bomb.defuser {
            Some((d, done)) if d == id => {
                if !holding {
                    if let Some(b) = &mut self.bomb {
                        b.defuser = None;
                    }
                    return false;
                }
                if time >= done {
                    self.bomb = None;
                    let p = &mut self.players[i];
                    p.money = (p.money + self.rules.player_defused).min(self.rules.max_money);
                    self.events.push(GameEvent::BombDefused { player: id });
                    self.end_round(RoundEndReason::BombDefused);
                    return false;
                }
                true
            }
            None if holding => {
                let (fwd, _, _) = movement::angle_vectors(input.view_angles);
                if fwd.dot(to.normalize_or_zero()) < 0.5 {
                    return false;
                }
                let kit = p.defuse_kit;
                let done = time + self.rules.defuse_duration(kit);
                if let Some(b) = &mut self.bomb {
                    b.defuser = Some((id, done));
                }
                self.events.push(GameEvent::DefuseStart { player: id, kit });
                true
            }
            _ => false,
        }
    }

    /// Advances the world by one tick. Players without an input this tick
    /// repeat their last one (as a server does for a late packet), minus
    /// one-shot buttons and commands.
    pub fn step(&mut self, inputs: &[(PlayerId, TickInput)]) {
        self.step_internal(inputs, true);
    }

    /// Replays client inputs through the same simulation without inventing
    /// server-only bot decisions. Replicated bot metadata remains intact.
    /// Prediction never exposes GameEvents: authoritative reliable events
    /// drive effects, so a correction cannot replay a sound or notification.
    pub fn step_prediction(&mut self, inputs: &[(PlayerId, TickInput)]) {
        self.step_internal(inputs, false);
        self.events.clear();
    }

    fn step_internal(&mut self, inputs: &[(PlayerId, TickInput)], think_bots: bool) {
        self.character_hitboxes.retain(|id, _| self.players.iter().any(|p| p.id == *id));
        for index in 0..self.players.len() { self.refresh_character_hitboxes(index); }
        if think_bots {
            if let Some(history) = &mut self.rewind {
                history.capture(self.tick, self.round.round, &self.players, &self.character_hitboxes);
            }
        }
        // Bots decide first, from last tick's state and events (what they
        // heard). Explicit inputs win over a bot's own.
        let with_bots;
        let inputs = if think_bots && self.players.iter().any(|p| p.is_bot) {
            let mut bots = std::mem::take(&mut self.bots);
            let mut all = bots.think(self);
            self.bots = bots;
            all.retain(|(id, _)| !inputs.iter().any(|(h, _)| h == id));
            all.extend_from_slice(inputs);
            with_bots = all;
            &with_bots[..]
        } else {
            inputs
        };
        self.events.clear();
        let time = self.time();
        let dt = self.dt();

        match self.mode {
            GameMode::Deathmatch | GameMode::Special => {
                let due: Vec<PlayerId> = self
                    .players
                    .iter()
                    .filter(|p| !p.alive && p.team != Team::Spectator && time >= p.respawn_at)
                    .map(|p| p.id)
                    .collect();
                for id in due {
                    self.respawn(id);
                }
            }
            GameMode::Competitive => self.update_round(time),
        }
        // Newly spawned knives must use the selected definition on their
        // very first attack, before processing this tick's input.
        self.dress_weapons();
        let frozen = self.competitive() && self.round.phase == Phase::Freeze;
        self.step_doors(inputs, frozen);
        let collision = self.collision.clone();

        // Motion continues on release, while holstered, and on the ground;
        // render frames never advance authoritative recoil.
        for player in &mut self.players {
            for weapon in &mut player.weapons { weapon.advance_recoil_motion(self.tick); }
        }
        for item in &mut self.items {
            if let GroundKind::Weapon(weapon) = &mut item.kind { weapon.advance_recoil_motion(self.tick); }
        }

        for i in 0..self.players.len() {
            if !self.players[i].alive {
                continue;
            }
            let id = self.players[i].id;
            let mut input = inputs.iter().find(|(pid, _)| *pid == id).map(|(_, inp)| *inp).unwrap_or(TickInput {
                buttons: self.players[i].last_input.buttons & !(buttons::JUMP | buttons::RELOAD),
                weapon_slot: 0,
                buy: Purchase::None,
                drop: false,
                utility: 0,
                rewind_tick: None,
                ..self.players[i].last_input
            });
            if !think_bots || self.players[i].is_bot {
                input.rewind_tick = None;
            }
            for weapon in &mut self.players[i].weapons { weapon.advance_suppressor(self.tick, dt); }
            // Buying and dropping.
            {
                let (mn, mx) = self.players[i].bounds();
                let team = self.players[i].team;
                self.players[i].in_buy_zone = self.objectives.in_buy_zone(team, mn, mx);
            }
            if input.buy != Purchase::None {
                let _ = self.buy(id, input.buy);
            }
            if input.drop {
                let p = &self.players[i];
                if let Some(w) = p.active_weapon() {
                    if w.def.class != weapons::WeaponClass::Knife {
                        let index = p.active;
                        self.drop_weapon(i, index, 48.0);
                    }
                }
            }

            // Weapon switching and reloads.
            {
                let p = &mut self.players[i];
                let slot = input.weapon_slot;
                if input.utility!=0 {
                    if let Some(index)=p.weapons.iter().position(|w| w.def.def_index==input.utility && w.def.class==weapons::WeaponClass::Grenade) {
                        if index!=p.active { p.select(index,time); }
                    }
                }
                if input.utility!=0 { /* selected above */ } else if slot == SLOT_LAST {
                    let last = p.last_active;
                    if last != p.active {
                        p.select(last, time);
                    }
                } else if matches!(slot, SLOT_NEXT | SLOT_PREVIOUS) {
                    // A wheel command carries direction, not a client-derived
                    // inventory index or slot. Drops/buys above may have changed
                    // the inventory since the input was sampled.
                    if !p.weapons.is_empty() {
                        let count = p.weapons.len();
                        let target = if slot == SLOT_NEXT { (p.active + 1) % count }
                            else { (p.active + count - 1) % count };
                        if target != p.active { p.select(target, time); }
                    }
                } else if slot >= 1 {
                    let in_slot: Vec<usize> = (0..p.weapons.len()).filter(|&w| weapon::slot(&p.weapons[w].def) == slot).collect();
                    let pressed = p.last_input.weapon_slot != slot;
                    let target = match in_slot.iter().position(|&w| w == p.active) {
                        Some(k) if pressed && in_slot.len() > 1 => Some(in_slot[(k + 1) % in_slot.len()]),
                        Some(_) => None,
                        None => in_slot.first().copied(),
                    };
                    if let Some(t) = target {
                        p.select(t, time);
                    }
                }
                let pressed_reload = input.buttons & buttons::RELOAD != 0;
                if let Some(w) = p.weapons.get_mut(p.active) {
                    if pressed_reload && w.can_reload() {
                        w.reset_zoom(time);
                        w.begin_reload(time);
                        self.events.push(GameEvent::Reload { player: id });
                    }
                }
            }

            // Objectives lock movement while planting or defusing.
            let busy = self.update_plant(i, &input, time) || self.update_defuse(i, &input, time);
            if !self.players[i].alive {
                continue;
            }
            if frozen || busy {
                input.forward_move = 0.0;
                input.side_move = 0.0;
                input.buttons &= !buttons::JUMP;
            }

            // Movement.
            let max_speed = self.players[i].active_weapon().map_or(250.0, |w| w.max_speed());
            let move_input = MoveInput {
                view_angles: input.view_angles,
                forward_move: input.forward_move,
                side_move: input.side_move,
                buttons: input.buttons,
                weapon_max_speed: max_speed,
            };
            {
                let contacts = player_contact::PlayerWorld::new(collision.as_ref(), &self.players, id, self.rules.solid_teammates);
                let p = &mut self.players[i];
                p.view_angles = input.view_angles;
                p.tracked = input.tracked;
                // Dynamic-world fluids use the same collision sweeps as CS movement.
                let swimming=collision.dynamic.as_ref().is_some_and(|d|d.point_contents(p.movement.origin+Vec3::Z*32.,contents::WATER)!=0);
                if swimming {
                    let mut config=self.config.clone();config.gravity=0.;config.air_accelerate=8.;config.air_max_wishspeed=160.;config.max_speed=160.;
                    p.movement.velocity*=0.97;
                    let vertical=if input.buttons&buttons::JUMP!=0{140.}else if input.buttons&buttons::DUCK!=0{-100.}else{-input.view_angles.x.to_radians().sin()*input.forward_move*0.3-12.};
                    p.movement.velocity.z=vertical;
                    p.last_events=contacts.move_player(&mut p.movement,&move_input,&config,dt);
                } else {
                    p.last_events = contacts.move_player(&mut p.movement, &move_input, &self.config, dt);
                }
                if p.last_events.jumped {
                    self.events.push(GameEvent::Jump { player: id });
                }
                if let Some(speed) = p.last_events.landed {
                    self.events.push(GameEvent::Land { player: id, speed });
                }
            }

            self.refresh_character_hitboxes(i);

            // Admit the tracked muzzle against current geometry and the current
            // post-movement eye. A nearby hand must not begin beyond a wall.
            let mut muzzle_blocked = false;
            if let Some(aim) = &mut input.aim {
                let p = &self.players[i];
                let eye_offset = Vec3::Z * p.movement.view_offset();
                aim.direction = aim.direction.try_normalize().unwrap_or(Vec3::X);
                aim.origin = eye_offset + (aim.origin - eye_offset).clamp_length_max(MAX_AIM_OFFSET);
                if !aim.origin.is_finite() || !aim.direction.is_finite() {
                    input.aim = None;
                } else {
                    let trace = collision.trace_box(
                        p.eye(), p.movement.origin + aim.origin,
                        Vec3::splat(-AIM_WALL_MARGIN), Vec3::splat(AIM_WALL_MARGIN),
                        contents::MASK_SHOT,
                    );
                    if trace.hit() || trace.all_solid {
                        aim.origin = eye_offset;
                    }
                    // Leaving a brush may be start-solid with fraction 1, so
                    // falling back to that eye alone cannot safely fire.
                    if trace.start_solid || trace.all_solid {
                        muzzle_blocked = true;
                        input.buttons &= !(buttons::ATTACK | buttons::ATTACK2);
                    }
                }
            }
            self.players[i].aim = input.aim;

            // Firing.
            let attack = input.buttons & buttons::ATTACK != 0 && !frozen && !busy;
            let pressed = attack && self.players[i].last_input.buttons & buttons::ATTACK == 0;
            let secondary = input.buttons & buttons::ATTACK2 != 0 && !frozen && !busy;
            let secondary_pressed = secondary && self.players[i].last_input.buttons & buttons::ATTACK2 == 0;
            let mut fire = false;
            let mut stab = false;
            let mut secondary_cycle = None;
            {
                let p = &mut self.players[i];
                let ducked = p.movement.ducked;
                let eye = p.eye();
                if let Some(w) = p.weapons.get_mut(p.active) {
                    // Scope cycles repeat from the held command at secondary
                    // readiness. Keep edge-only suppressor/burst actions below
                    // and never change the primary/bolt deadline here.
                    if secondary && w.zoom_levels() > 0 && w.reload_end.is_none()
                        && time >= w.next_attack && time >= w.next_secondary_attack {
                        w.resume_zoom = None;
                        w.set_zoom((w.zoom_level + 1) % (w.zoom_levels() + 1), time);
                        w.next_secondary_attack = time + 0.3;
                        self.events.push(GameEvent::Scope { player: id, weapon: w.def.class_name.clone(), origin: eye, level: w.zoom_level });
                    }
                    if secondary_pressed { w.begin_suppressor(self.tick, dt); }
                    if secondary_pressed && w.switch_burst(time) {
                        self.events.push(GameEvent::BurstModeChanged {
                            player: id, weapon: w.def.class_name.clone(), origin: eye, burst: w.burst_mode,
                        });
                    }
                    if frozen || busy || muzzle_blocked { w.cancel_burst(); }
                    // Do not consume a fresh press merely because its first
                    // tick lands inside the previous shot's cooldown. Retain
                    // it until ready while held; release and handling locks
                    // cancel it, so this cannot queue a later surprise shot.
                    if !attack || w.reload_end.is_some() || w.suppressor_transition.is_some() {
                        w.primary_pending = false;
                    } else if pressed && w.burst.is_none() {
                        w.primary_pending = true;
                    }
                    stab = secondary && w.is_melee();
                    let can_fire = w.def.class_name != C4 && w.def.class!=weapons::WeaponClass::Grenade;
                    // Pulling the trigger stops a shell-by-shell reload once
                    // a shell is in, as in the game.
                    if attack && (pressed || w.def.full_auto) && w.reload_end.is_some() && w.shell_reload().is_some() && w.clip > 0 {
                        w.reload_end = None;
                        w.primary_pending = pressed;
                    }
                    // Authored R8 fan-fire shares primary readiness and ammo;
                    // resolve its mode only for the admitted shot below.
                    let fan_cycle = if secondary && time >= w.next_secondary_attack { w.secondary_fire_cycle() } else { None };
                    if w.burst_due(time) {
                        fire = true;
                    } else if can_fire && (attack || stab || fan_cycle.is_some()) && w.reload_end.is_none() && time >= w.next_attack
                        && ((w.def.full_auto && !w.burst_mode) || w.primary_pending || w.is_melee() || fan_cycle.is_some()) {
                        let trigger_press = if fan_cycle.is_some() { secondary_pressed } else { w.primary_pending };
                        w.primary_pending = false;
                        if w.uses_ammo() && w.clip <= 0 {
                            if trigger_press && w.can_reload() {
                                w.reset_zoom(time);
                                w.begin_reload(time);
                                self.events.push(GameEvent::Reload { player: id });
                            } else if w.reserve <= 0 && w.burst.is_none() && w.suppressor_transition.is_none() {
                                if let Some(profile) = w.def.empty_trigger.as_ref().filter(|profile|
                                    !profile.sound.is_empty() && profile.repeat_interval.is_finite()
                                        && profile.repeat_interval >= 0.01) {
                                    // Shares ordinary deployment/attack readiness. A
                                    // dry cue is not a live shot or recoil impulse.
                                    w.next_attack = time + profile.repeat_interval;
                                    self.events.push(GameEvent::EmptyTrigger {
                                        player: id, weapon: w.def.class_name.clone(), origin: eye,
                                    });
                                }
                            }
                        } else {
                            fire = true;
                            secondary_cycle = fan_cycle;
                        }
                    }
                    // Native AK and Glock/FAMAS burst cases wait while an
                    // empty trigger is held. AK/M4 and both burst guns start
                    // idle reload on release, without another click. Keep
                    // the existing weapon readiness/reload admission guards.
                    if !frozen && !busy && !muzzle_blocked && !attack && !secondary && can_fire
                        && w.clip == 0 && w.can_reload() && time >= w.next_attack {
                        w.reset_zoom(time);
                        w.begin_reload(time);
                        self.events.push(GameEvent::Reload { player: id });
                    }
                    // A loaded automatic trigger remains a burst during its
                    // shot cooldown. `fire` alone is false on those ticks and
                    // would recover the entire recoil step between AK shots.
                    // Reloads, dry weapons and a held semiautomatic trigger
                    // still recover normally when they cannot sustain fire.
                    let recoil_firing = fire || w.burst.is_some() || (can_fire && ((attack && w.def.full_auto && !w.burst_mode) || fan_cycle.is_some())
                        && w.reload_end.is_none() && w.clip > 0);
                    weapon::think(w, time, dt, ducked, recoil_firing);
                }
            }
            if fire {
                self.players[i].animation.last_fire_tick = Some(self.tick);
                if self.players[i].active_weapon().is_some_and(|w| w.is_melee()) {
                    self.melee_attack(i, time, stab);
                } else {
                    self.fire(i, time, input.rewind_tick, secondary_cycle);
                    // At low tick rates several imported intervals can become
                    // due together. A burst has at most two follow-up bullets.
                    for _ in 0..2 {
                        if !self.players[i].active_weapon().is_some_and(|w| w.burst_due(time)) { break; }
                        self.fire(i, time, input.rewind_tick, None);
                    }
                }
            }
            if !frozen && !busy && self.players[i].active_weapon().is_some_and(|w|w.def.class==weapons::WeaponClass::Grenade) {
                self.step_grenade(i,input.buttons);
            }
            if self.players[i].alive && !self.items.is_empty() {
                self.pickups(i, time);
            }
            self.players[i].last_input = input;
        }
        self.step_utility();
        self.step_ground_items();
        if self.competitive() {
            self.check_elimination();
        }
        self.dress_weapons();
        self.advance_character_clocks();
        self.tick += 1;
    }

    fn melee_attack(&mut self, shooter: usize, time: f32, secondary: bool) {
        let p = &self.players[shooter];
        let id = p.id;
        let team = p.team;
        let (eye, aim_angles) = p.aim_origin_angles();
        let (dir, _, _) = movement::angle_vectors(aim_angles);
        let w = p.active_weapon().unwrap();
        let mut def = (*w.def).clone();
        let first = w.last_melee_attack.is_none_or(|t| time - t > 0.9);
        let range = if secondary { 32.0 } else { MELEE_RANGE };
        let (tr, breakable) = self.collision.trace_breakable_ray(eye, eye + dir * range, contents::MASK_SHOT);
        let mut best = None;
        for (j, other) in self.players.iter().enumerate() {
            if j == shooter || !other.alive || !team.enemy_of(other.team) { continue; }
            let (mn, mx) = other.bounds();
            if let Some(t) = ray_box(eye, dir, mn, mx) {
                if t <= tr.fraction * range && best.is_none_or(|(_, bt)| t < bt) { best = Some((j, t)); }
            }
        }
        let external = self.collision.dynamic.as_ref().and_then(|d| d.hit_target(eye, tr.end))
            .filter(|(_, fraction)| best.is_none_or(|(_, t)| fraction * tr.fraction * range < t));
        let hit = if best.is_some() || external.is_some() { MeleeHit::Player } else if tr.fraction < 1.0 { MeleeHit::World } else { MeleeHit::Miss };
        self.events.push(GameEvent::Melee { player: id, weapon: def.class_name.clone(), origin: eye, secondary, hit });
        if let Some((key, fraction)) = external {
            if let Some(dynamic) = &self.collision.dynamic {
                dynamic.damage_target_at(key, if secondary { 65.0 } else if first { 40.0 } else { 25.0 }, eye, eye + (tr.end-eye)*fraction);
            }
        } else if let Some((victim, distance)) = best {
            let v = &self.players[victim];
            let (forward, _, _) = movement::angle_vectors(Vec3::new(0.0, v.view_angles.y, 0.0));
            let toward = (v.movement.origin - self.players[shooter].movement.origin).with_z(0.0).normalize_or_zero();
            let behind = forward.dot(toward) > 0.475;
            def.damage = match (secondary, behind, first) {
                (true, true, _) => 180.0, (true, false, _) => 65.0,
                (false, true, _) => 90.0, (false, false, true) => 40.0, _ => 25.0,
            };
            def.armor_ratio = 1.7;
            def.range_modifier = 1.0;
            self.apply_damage(shooter, victim, &def, distance, HitGroup::Chest, eye + dir * distance, Some(dir));
        }
        if external.is_none() && best.is_none() && tr.hit() {
            self.damage_breakable(breakable, &tr,
                if secondary { 65.0 } else if first { 40.0 } else { 25.0 }, breakables::DamageKind::Club);
            if let Some(dynamic) = &self.collision.dynamic {
                dynamic.damage_block(tr.end, tr.normal, if secondary { 30.0 } else { 15.0 });
            }
        }
        let active = self.players[shooter].active;
        let w = &mut self.players[shooter].weapons[active];
        w.last_melee_attack = Some(time);
        w.next_attack = time + match (secondary, hit == MeleeHit::Miss) {
            (true, true) => 1.0, (true, false) => 1.1, (false, true) => 0.4, (false, false) => 0.5,
        };
    }

    fn fire(&mut self, shooter: usize, time: f32, rewind_tick: Option<u64>, secondary_cycle: Option<f32>) {
        let dt = self.dt();
        let p = &self.players[shooter];
        let secondary = secondary_cycle.is_some();
        let trigger = if secondary { buttons::ATTACK2 } else { buttons::ATTACK };
        let continued_trigger = p.last_input.buttons & trigger != 0;
        let id = p.id;
        let team = p.team;
        let (eye, aim_angles) = p.aim_origin_angles();
        let Some(w) = p.weapons.get(p.active) else { return };
        let def = w.def.clone();
        let suppressed = w.suppressor_attached;
        let punch = w.aim_punch();
        let ctx = AccuracyContext {
            speed: p.movement.speed_2d(),
            vertical_speed: p.movement.velocity.z,
            on_ground: p.movement.on_ground,
            ducked: p.movement.ducked,
        };
        let inacc = weapon::inaccuracy_for_shot(w, ctx, secondary) * 0.001;
        let spread = w.shot_stat("spread", def.accuracy.spread, secondary) * 0.001;
        let angles = aim_angles
            + Vec3::new(punch[0] * weapon::RECOIL_SCALE, punch[1] * weapon::RECOIL_SCALE, 0.0);
        let (fwd, right, up) = movement::angle_vectors(angles);
        let shot = p.shots;
        let range = def.range.max(1.0);

        self.events.push(GameEvent::Fire {
            player: id,
            weapon: def.class_name.clone(),
            origin: eye,
            suppressed,
            secondary,
        });

        // Seeded by the shooter's shot count, not the tick, so a predicting
        // client draws the same spread the server computes.
        let mut rng = Rng::from_parts(&[id as u64, shot]);
        for _ in 0..def.bullets.max(1) {
            let r1 = rng.next_f32();
            let a1 = rng.range(0.0, std::f32::consts::TAU);
            let r2 = rng.next_f32();
            let a2 = rng.range(0.0, std::f32::consts::TAU);
            let ox = a1.cos() * r1 * inacc + a2.cos() * r2 * spread;
            let oy = a1.sin() * r1 * inacc + a2.sin() * r2 * spread;
            let dir = (fwd + right * ox + up * oy).normalize();
            let end = eye + dir * range;
            let mut segment_start = eye;
            let mut damage_scale = 1.0;
            for slab in 0..=penetration::MAX_SLABS {
                let traveled = (segment_start - eye).dot(dir).max(0.0);
                let segment_range = (range - traveled).max(0.0);
                let (tr, owner) = self.collision.trace_owned_ray(segment_start, end, contents::MASK_SHOT);
                // A ray leaving an enclosing solid can have fraction=1 while
                // still marked start-solid. It must not admit a target beyond
                // that wall as if the whole segment were clear.
                if tr.start_solid || tr.all_solid {
                    self.events.push(GameEvent::Tracer { player: id, start: eye, end: segment_start });
                    break;
                }
                let world_dist = tr.fraction * segment_range;

                // The movement hull is broad phase only when authored shapes exist.
                let mut best: Option<(usize, f32, Option<HitGroup>)> = None;
                for (j, other) in self.players.iter().enumerate() {
                    if j == shooter || !other.alive || !team.enemy_of(other.team) { continue; }
                    let hull = self.target_hull(other, rewind_tick);
                    if ray_box(segment_start, dir, hull.mins, hull.maxs).is_none_or(|d| d > world_dist) { continue; }
                    let candidate = if let Some(shapes) = &hull.hitboxes {
                        let mut intersections = shapes.iter().filter_map(|shape|
                            shape.ray_intersection(segment_start, dir, world_dist).map(|d| (d, shape.group)))
                            .collect::<Vec<_>>();
                        intersections.sort_by(|a,b| a.0.total_cmp(&b.0));
                        intersections.first().copied().map(|(distance, raw)| {
                            // Valve's May 2018 notes explicitly admit neck -> head.
                            // Do not invent neck-only damage or skip an intervening body part.
                            if raw == 8 {
                                if let Some(&(d, 1)) = intersections.iter().find(|(_,group)| *group != 8) {
                                    return (d, Some(HitGroup::Head));
                                }
                            }
                            (distance, HitGroup::from_model(raw))
                        })
                    } else { ray_box(segment_start, dir, hull.mins, hull.maxs).map(|d| (d,None)) };
                    if let Some((distance, group)) = candidate {
                        if distance <= world_dist && best.is_none_or(|(_,d,_)| distance < d) {
                            best = Some((j,distance,group));
                        }
                    }
                }
                if let Some(dynamic)=&self.collision.dynamic {
                    if let Some((key,fraction))=dynamic.hit_target(segment_start,tr.end) {
                        let distance=fraction*world_dist;
                        if best.is_none_or(|(_,t,_)|distance<t) {
                            let hit=segment_start+dir*distance;
                            dynamic.damage_target_at(key,def.damage * damage_scale * def.range_modifier.powf((traveled+distance)/500.0),eye,hit);
                            self.events.push(GameEvent::Tracer{player:id,start:eye,end:hit});
                            break;
                        }
                    }
                }
                match best {
                    Some((j, t, authored_group)) => {
                        let hit = segment_start + dir * t;
                        self.events.push(GameEvent::Tracer { player: id, start: eye, end: hit });
                        // Range falloff after the last exit can exhaust a pellet
                        // that passed the exit budget. Recheck before the shared
                        // damage path can restore one health through its minimum.
                        if damage_scale < 1.0
                            && def.damage * damage_scale * def.range_modifier.powf((traveled + t) / 500.) < 1. {
                            break;
                        }
                        let victim = &self.players[j];
                        let hull = self.target_hull(victim, rewind_tick);
                        let (mn, mx) = (hull.movement_mins, hull.movement_maxs);
                        let frac = ((hit.z - mn.z) / (mx.z - mn.z)).clamp(0.0, 1.0);
                        let (_, vright, _) = movement::angle_vectors(Vec3::new(0.0, hull.yaw, 0.0));
                        let center = (mn + mx) * 0.5;
                        let side = ((hit - center).dot(vright) / 16.0).clamp(-1.0, 1.0);
                        let group = authored_group.unwrap_or_else(|| HitGroup::from_height(frac, side));
                        if damage_scale == 1.0 {
                            self.apply_damage(shooter, j, &def, traveled + t, group, hit, Some(dir));
                        } else {
                            let mut attenuated = (*def).clone();
                            attenuated.damage *= damage_scale;
                            self.apply_damage(shooter, j, &attenuated, traveled + t, group, hit, Some(dir));
                        }
                        break;
                    }
                    None => {
                        if tr.fraction < 1.0 && !self.collision.is_sky(tr.surface) {
                            // Query the intact current geometry before breakable damage
                            // can replace this simulation copy's collision overlay.
                            let continuation = if slab < penetration::MAX_SLABS {
                                penetration::through_exit(&self.collision, segment_start, end, &tr, def.penetration)
                            } else { None };
                            // Retain the actual exit receiver before breakable
                            // damage can remove its current collision owner.
                            let exit_door = continuation.as_ref().and_then(|(exit, _)| {
                                let (back, owner) = self.collision.trace_owned_ray(exit.next_start,
                                    exit.position - dir * 0.125, contents::MASK_SHOT);
                                self.door_impact(owner, &back)
                            });
                            let door = self.door_impact(owner, &tr);
                            self.damage_breakable(owner.and_then(collision::HitOwner::breakable), &tr,
                                def.damage * damage_scale * def.range_modifier.powf((traveled + world_dist) / 500.0), breakables::DamageKind::Bullet);
                            if let Some(dynamic)=&self.collision.dynamic {dynamic.damage_block(tr.end,tr.normal,def.damage * damage_scale * def.range_modifier.powf((traveled+world_dist)/500.0));}
                            self.events.push(GameEvent::Impact {
                                player: id,
                                pos: tr.end,
                                normal: tr.normal,
                                surface: tr.surface,
                                door,
                            });
                            if let Some((exit, retention)) = continuation {
                                let next_scale = damage_scale * retention;
                                let distance = (exit.next_start - eye).dot(dir);
                                // apply_damage has a one-health minimum; do not let
                                // an exhausted pellet acquire damage through rounding.
                                if def.damage * next_scale * def.range_modifier.powf(distance / 500.) >= 1. {
                                    self.events.push(GameEvent::Impact {
                                        player: id, pos: exit.position, normal: exit.normal,
                                        surface: exit.surface, door: exit_door,
                                    });
                                    segment_start = exit.next_start;
                                    damage_scale = next_scale;
                                    continue;
                                }
                            }
                        }
                        // Presentation anchors tracers to the rendered muzzle.
                        // Emit only the completed pellet path, not a new muzzle
                        // trail for every segment behind penetrated cover.
                        self.events.push(GameEvent::Tracer { player: id, start: eye, end: tr.end });
                    }
                }
                break;
            }
        }

        let p = &mut self.players[shooter];
        p.shots += 1;
        if let Some(w) = p.weapons.get_mut(p.active) {
            if w.uses_ammo() {
                w.clip -= 1;
            }
            w.shot_cooldown(time, dt, continued_trigger, secondary_cycle);
            w.accuracy_penalty += w.shot_stat("inaccuracy fire", def.accuracy.inaccuracy_fire, secondary);
            w.recoil_shot(self.tick, self.tick_rate);
            w.recoil_index += 1.0;
            if w.zoom_level > 0 && def.attr_f32("unzoom after shot").is_some_and(|v| v != 0.0) {
                w.resume_zoom = (w.clip > 0).then_some(w.zoom_level);
                w.set_zoom(0, time);
            }
        }
    }

    fn apply_damage(&mut self, attacker: usize, victim: usize, def: &weapons::WeaponDefinition, distance: f32, group: HitGroup, pos: Vec3, direction: Option<Vec3>) {
        let v = &self.players[victim];
        let protection = weapon::hit_protection(group, v.armor, v.helmet);
        let (health_dmg, armor_dmg) = weapon::damage(def, distance, group, v.armor, v.helmet);
        let attacker_id = self.players[attacker].id;
        let v = &mut self.players[victim];
        v.health -= health_dmg.max(1);
        v.armor = (v.armor - armor_dmg).max(0);
        let victim_id = v.id;
        self.events.push(GameEvent::Damage {
            attacker: attacker_id,
            victim: victim_id,
            health: health_dmg,
            group,
            pos,
            protection,
        });
        if self.players[victim].health <= 0 {
            let hit = direction.map(|direction| death::DeathHit { direction, position: pos, group });
            self.kill(victim, Some(attacker), &def.class_name, group == HitGroup::Head, hit);
            if self.competitive() && self.players[attacker].team.enemy_of(self.players[victim].team) {
                let award = (def.kill_award as f32 * self.rules.kill_award_factor) as i32;
                let a = &mut self.players[attacker];
                a.money = (a.money + award).min(self.rules.max_money);
            }
        }
    }

    /// Damage from the host world's creatures or environmental hazards.
    pub fn damage_environment(&mut self, id: PlayerId, amount: i32, cause: &str) {
        let Some(victim) = self.players.iter().position(|p| p.id == id && p.alive) else { return };
        let amount = amount.max(0);
        if amount == 0 { return; }
        self.players[victim].health -= amount;
        self.events.push(GameEvent::Damage { attacker: WORLD, victim: id, health: amount, group: HitGroup::Chest, pos: self.players[victim].eye(), protection: weapon::HitProtection::Flesh });
        if self.players[victim].health <= 0 { self.kill(victim, None, cause, false, None); }
    }

    fn kill(&mut self, victim: usize, attacker: Option<usize>, weapon: &str, headshot: bool, hit: Option<death::DeathHit>) {
        let death = death::DeathReceipt::capture(&self.players[victim], attacker.map(|a| &self.players[a]), self.tick, self.round.round, hit);
        let now = self.time();
        let v = &mut self.players[victim];
        v.health = 0;
        v.alive = false;
        v.movement.ladder_normal = None;
        for w in &mut v.weapons { w.reset_zoom(now); w.cancel_burst(); }
        v.deaths += 1;
        v.plant_start = None;
        v.respawn_at = now + RESPAWN_DELAY;
        let victim_id = v.id;
        let attacker_id = match attacker {
            Some(a) => {
                self.players[a].kills += 1;
                self.players[a].id
            }
            None => WORLD,
        };
        if let Some(a)=attacker {
            let enemy=self.players[a].team!=self.players[victim].team;
            if enemy {
                if let Some(w)=self.players[a].active_weapon_mut() {
                    if w.def.class_name==weapon && w.cosmetic_owner==Some(attacker_id) {
                        if let Some(c)=w.cosmetic.as_mut() {
                            if let Some(count)=c.stattrak.as_mut() {
                                *count=count.saturating_add(1);
                                self.events.push(GameEvent::StatTrakKill{owner:attacker_id,serial:c.serial});
                            }
                        }
                    }
                }
            }
        }
        self.events.push(GameEvent::Kill {
            attacker: attacker_id,
            victim: victim_id,
            weapon: weapon.to_string(),
            headshot,
            death,
        });
        self.death_drops(victim);
    }

    /// True if nothing solid blocks the line between two points.
    pub fn visible(&self, from: Vec3, to: Vec3) -> bool {
        let tr = self.collision.trace_ray(from, to, contents::MASK_SHOT);
        tr.fraction >= 1.0 || self.collision.is_sky(tr.surface)
    }

    /// The player carrying the bomb, if any.
    pub fn bomb_carrier(&self) -> Option<&Player> {
        self.players.iter().find(|p| p.alive && p.has_bomb())
    }

    /// The bomb lying on the ground, if any.
    pub fn dropped_bomb(&self) -> Option<&GroundItem> {
        self.items.iter().find(|i| i.class_name() == C4)
    }
}

fn kv_angles(s: &str) -> Vec3 {
    let v: Vec<f32> = s.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    Vec3::new(
        v.first().copied().unwrap_or(0.0),
        v.get(1).copied().unwrap_or(0.0),
        v.get(2).copied().unwrap_or(0.0),
    )
}

#[cfg(test)]
mod tests;

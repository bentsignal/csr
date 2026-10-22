//! Bots: produce [`TickInput`]s for AI players, so bots obey exactly the
//! same movement, weapon, buying and objective rules as humans.
//!
//! * Navigation: with the map's nav mesh, bots follow A* paths through area
//!   portals, jump/crouch where the mesh says so, and re-path when stuck.
//!   Without a mesh they wander and turn away from walls.
//! * Combat: perception runs ~8 times a second per bot (staggered); on
//!   seeing an enemy a bot reacts after a delay, turns at a limited rate
//!   with aim error, stops and fires controlled bursts.
//! * Deathmatch: bots roam between hot places (bombsites, main routes).
//! * Competitive: bots buy in the buy zone (rifle + armour when they can
//!   afford it, save otherwise, armour or a pistol on pistol rounds, a kit
//!   for CTs). Terrorists pick a site each round; the carrier walks there
//!   and plants, the others escort, and the nearest one fetches a dropped
//!   bomb. Counter-Terrorists split between the sites and hold them, and
//!   everyone converges on a planted bomb: CTs to defuse, Ts to guard.

use crate::objectives::Zone;
use crate::rng::Rng;
use crate::rules::{Phase, Purchase};
use crate::{GameMode, Player, PlayerId, Team, TickInput, World};
use glam::Vec3;
use movement::buttons;
use nav::NavMesh;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Ticks between events that happen `per_second` times a second.
fn interval(world: &World, per_second: u32) -> u64 {
    (world.tick_rate / per_second).max(1) as u64
}
/// Perception updates per second.
const PERCEPTION_RATE: u32 = 8;
/// Utility-throw decisions per second.
const UTILITY_RATE: u32 = 4;
/// Checks for fire on the planned path per second.
const FIRE_CHECK_RATE: u32 = 2;
/// A wandering bot that has not moved for this long turns away (1/8 s).
const STUCK_SECONDS: f32 = 0.125;
/// A failed crossing is retried after a bounded recovery window: a temporary
/// player obstruction must not permanently disconnect the bot's navigation.
const FAILED_TRANSITION_SECONDS: f32 = 8.0;
const MAX_FAILED_TRANSITIONS: usize = 8;

#[derive(Clone, Debug)]
struct FailedTransition {
    from: u32,
    to: u32,
    retry_at: f32,
}

#[derive(Clone, Debug)]
struct LadderRoute {
    from: u32,
    to: u32,
    id: u32,
    attached: bool,
    exiting: bool,
}

#[derive(Clone, Debug, Default)]
struct Brain {
    target: Option<PlayerId>,
    target_pos: Vec3,
    seen_at: f32,
    last_seen: f32,
    pitch: f32,
    yaw: f32,
    aim_error: glam::Vec2,
    aim_goal: glam::Vec2,
    next_aim_change: f32,
    wander_yaw: f32,
    last_pos: Vec3,
    stuck_ticks: u32,
    burst_left: u32,
    next_burst: f32,
    strafe: f32,
    next_strafe_change: f32,
    path: Vec<u32>,
    path_index: usize,
    progress_pos: Vec3,
    progress_time: f32,
    last_navigation_tick: Option<u64>,
    /// Exact breakable and crossing last attacked for navigation, not a timer.
    route_obstruction: Option<(usize, u32, u32)>,
    failed_transitions: Vec<FailedTransition>,
    /// An unreachable query under the current temporary edge exclusions.
    failed_route: Option<(u32, u32)>,
    route_searches: u64,
    ladder_route: Option<LadderRoute>,
    /// This decision followed a ladder; do not add walking recovery/yield jumps.
    ladder_navigation: bool,
    /// Round the bot finished buying in, and failed attempts this round.
    bought_round: u32,
    buy_attempts: u32,
    /// The area the current path leads to, when following an objective.
    goal_area: Option<u32>,
    /// (site index, chosen area) for site objectives.
    site_area: Option<(usize, u32)>,
    /// When to pick a new spot while holding a site.
    repick_at: f32,
    hold: Option<(Vec3, u32, Vec3, Vec3)>, // objective, area, position, angle
    visible: bool,
    turn_velocity: glam::Vec2,
    escape_until: f32,
    escape_side: f32,
    duck_jump: bool,
    last_shots: u64,
    reload_cover: Option<(u32, Vec3)>,
    throw: Option<(u32, Vec3, f32)>,
    next_utility: f32,
    last_health: i32,
    hurt_at: f32,

}

impl Brain {
    fn fresh(bot: &Player, bought_round: u32) -> Self {
        Brain {
            yaw: bot.view_angles.y,
            wander_yaw: bot.view_angles.y,
            last_pos: bot.movement.origin,
            bought_round,
            last_shots: bot.shots,
            last_health: bot.health,
            hurt_at: -10.0,
            ..Default::default()
        }
    }
}

/// Authored configuration, independent of any original engine AI algorithm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub skill: f32,
    pub reaction_time: f32,
    pub shooting: bool,
}

/// Brains are kept in id order so every copy of a world thinks in the same
/// order (determinism).
#[derive(Clone, Debug)]
pub struct Bots {
    brains: BTreeMap<PlayerId, Brain>,
    /// 0 (least skilled) .. 1 (sharp).
    pub skill: f32,
    /// Explicit admitted profile delay; None retains the existing default behavior.
    pub reaction_time: Option<f32>,
    /// Harmless bots still navigate and think, but cannot request an attack.
    pub shooting: bool,
    /// If false, bots stand still and never shoot (target practice).
    pub aggressive: bool,
    pub nav: Option<Arc<NavMesh>>,
    /// Nav areas inside each bomb site (computed on first use).
    site_areas: Vec<Vec<u32>>,
    team: crate::tactics::Coordinator,
}

impl Default for Bots {
    fn default() -> Self {
        Self {
            brains: BTreeMap::new(),
            skill: 0.5,
            reaction_time: None,
            shooting: true,
            aggressive: true,
            nav: None,
            site_areas: Vec::new(),
            team: Default::default(),
        }
    }
}

/// Pitch/yaw (degrees) looking from `from` to `to`, Source conventions.
pub fn look_angles(from: Vec3, to: Vec3) -> (f32, f32) {
    let d = to - from;
    let yaw = d.y.atan2(d.x).to_degrees();
    let pitch = -(d.z.atan2(d.truncate().length())).to_degrees();
    (pitch, yaw)
}

fn angle_diff(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

fn approach(current: f32, target: f32, max_step: f32) -> f32 {
    let d = angle_diff(target, current);
    current + d.clamp(-max_step, max_step)
}

/// Places bots like to go, with weights.
const HOT_PLACES: &[(&str, u32)] = &[
    ("BombsiteA", 6),
    ("BombsiteB", 6),
    ("Middle", 4),
    ("LongA", 4),
    ("Catwalk", 3),
    ("TopofMid", 3),
    ("UpperTunnel", 3),
    ("Short", 2),
    ("CTSpawn", 2),
    ("TSpawn", 2),
];

fn pick_goal(nav: &NavMesh, rng: &mut Rng) -> Option<u32> {
    if nav.areas.is_empty() {
        return None;
    }
    // 70 %: a hot place, else any area.
    if rng.next_f32() < 0.7 {
        let total: u32 = HOT_PLACES.iter().map(|(_, w)| w).sum();
        let mut roll = (rng.next_f32() * total as f32) as u32;
        for (name, w) in HOT_PLACES {
            if roll < *w {
                let candidates: Vec<u32> = nav
                    .areas
                    .iter()
                    .filter(|a| nav.place_name(a.place) == Some(name) && a.size() > 1500.0)
                    .map(|a| a.id)
                    .collect();
                if !candidates.is_empty() {
                    return Some(candidates[(rng.next_u64() as usize) % candidates.len()]);
                }
                break;
            }
            roll -= w;
        }
    }
    let big: Vec<u32> = nav.areas.iter().filter(|a| a.size() > 2500.0).map(|a| a.id).collect();
    let pool = if big.is_empty() { nav.areas.iter().map(|a| a.id).collect() } else { big };
    Some(pool[(rng.next_u64() as usize) % pool.len()])
}

/// Nav areas whose centre lies inside a bomb site's trigger volume.
fn areas_in(nav: &NavMesh, zone: &Zone) -> Vec<u32> {
    let mut v: Vec<u32> = nav
        .areas
        .iter()
        .filter(|a| {
            let c = a.center();
            zone.contains(Vec3::new(c.x, c.y, (c.z + 8.0).clamp(zone.mins.z, zone.maxs.z))) && c.z > zone.mins.z - 32.0 && c.z < zone.maxs.z
        })
        .map(|a| a.id)
        .collect();
    if v.is_empty() {
        v.extend(nav.nearest_area(zone.center()).map(|a| a.id));
    }
    v
}

/// What a bot is trying to do right now (besides fighting).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Task {
    /// Roam (deathmatch, or nothing better to do).
    Roam,
    /// Walk to a bomb site and hold a spot there.
    HoldSite(usize),
    /// Carry the bomb to a site and plant it.
    Plant(usize),
    /// Walk to a point (a dropped bomb, a planted bomb to guard).
    GoTo(Vec3),
    /// Walk to the planted bomb and defuse it.
    Defuse(Vec3),
    Guard(Vec3),
}

/// Prefer the usual gun, but do not keep returning to an exhausted one.
fn gun_slot(bot: &Player, time: f32) -> u8 {
    let preferred = if bot.weapon_in_slot(1).is_some() { 1 } else { 2 };
    let Some(index) = bot.weapon_in_slot(preferred) else { return preferred; };
    let w = &bot.weapons[index];
    if !w.uses_ammo() || w.clip > 0 || w.reserve > 0 {
        return preferred;
    }
    let fallback = (1..=3).find(|&slot| bot.weapon_in_slot(slot).is_some_and(|i| {
        let w = &bot.weapons[i];
        w.is_melee() || (slot <= 2 && w.uses_ammo() && (w.clip > 0 || w.reserve > 0))
    }));
    let Some(slot) = fallback else { return preferred; };
    // Let the current action finish; selection would cancel these states.
    // Zero means retain the active weapon, including during fallback deploy.
    if bot.plant_start.is_some() || bot.active_weapon().is_some_and(|w| {
        w.reload_end.is_some() || w.burst.is_some() || w.suppressor_transition.is_some()
            || w.grenade_throw.is_some() || time < w.next_attack || time < w.next_secondary_attack
    }) {
        return 0;
    }
    slot
}

/// The next thing a bot buys, given what it has (one item per tick).
fn next_purchase(world: &World, bot: &Player, team: &crate::tactics::Coordinator) -> Option<Purchase> {
    use crate::tactics::Economy;
    let money=bot.money-team.reserve.get(&bot.id).copied().unwrap_or(0);
    let economy=team.plans.get(&bot.team).map_or(Economy::Full,|p|p.economy);
    let ct=bot.team==Team::CounterTerrorist;
    let rank=world.players.iter().filter(|p|p.team==bot.team&&p.id<bot.id).count();
    let weapon=|name:&str|world.catalog.get(name).filter(|d|!bot.has(name)&&d.price<=money).map(|d|Purchase::Weapon(d.def_index));
    if economy==Economy::Pistol {
        if rank<3 && bot.armor==0 && money>=650 {return Some(Purchase::Kevlar)}
        if ct && rank==3 && !bot.defuse_kit && money>=400 {return Some(Purchase::DefuseKit)}
    } else if bot.weapon_in_slot(1).is_none() {
        if economy==Economy::Eco {return weapon("weapon_p250")}
        let sniper=rank==1 && !world.players.iter().any(|p|p.team==bot.team&&p.has("weapon_awp"));
        let choices=if ct {[if sniper {"weapon_awp"}else{"weapon_m4a1_silencer"},"weapon_m4a1_silencer","weapon_famas","weapon_mp9"]}
            else {[if sniper {"weapon_awp"}else{"weapon_ak47"},"weapon_ak47","weapon_galilar","weapon_mac10"]};
        for name in choices {if let Some(d)=world.catalog.get(name) {if d.price+if bot.armor<50{650}else{0}<=money {return Some(Purchase::Weapon(d.def_index))}}}
        if economy==Economy::Force {return weapon("weapon_deagle").or_else(||weapon("weapon_p250"))}
        return None;
    }
    if economy==Economy::Eco {return None}
    if economy!=Economy::Pistol {
        if bot.armor<50 && money>=650 {return Some(Purchase::Kevlar)}
        if !bot.helmet && money>=350 {return Some(Purchase::KevlarHelmet)}
        if ct && rank%2==0 && !bot.defuse_kit && money>=400 {return Some(Purchase::DefuseKit)}
    }
    if bot.weapons.iter().filter(|w|w.def.class==weapons::WeaponClass::Grenade).count()<4 {
        for name in ["weapon_smokegrenade","weapon_flashbang",if ct{"weapon_incgrenade"}else{"weapon_molotov"},"weapon_hegrenade"] {
            if let Some(p)=weapon(name) {return Some(p)}
        }
    }
    None
}

impl Bots {
    /// Release only the departed bot's server-only bookkeeping. Surviving
    /// brains, navigation, skill and the team's current plan remain intact.
    pub(crate) fn forget_player(&mut self, id: PlayerId) {
        self.brains.remove(&id);
        self.team.forget_player(id);
    }

    pub fn with_skill(skill: f32) -> Self {
        Self {
            skill: skill.clamp(0.0, 1.0),
            ..Default::default()
        }
    }

    pub fn with_profile(profile: Profile) -> Result<Self, &'static str> {
        if !profile.skill.is_finite() || !(0.0..=1.0).contains(&profile.skill)
            || !profile.reaction_time.is_finite() || profile.reaction_time < 0.0 {
            return Err("Invalid authored bot skill or reaction time.");
        }
        Ok(Self { skill: profile.skill, reaction_time: Some(profile.reaction_time),
            shooting: profile.shooting, ..Default::default() })
    }

    pub fn with_nav(mut self, nav: Option<Arc<NavMesh>>) -> Self {
        self.nav = nav;
        self
    }

    /// A one-line description of a bot's state, for debugging.
    pub fn describe(&self, world: &World, id: PlayerId) -> String {
        let (Some(b), Some(p)) = (self.brains.get(&id), world.player(id)) else { return String::new() };
        format!(
            "task {:?} target {:?} path {}/{} goal {:?} site {:?} role {:?} economy {:?} failed_edges {:?} route_searches {}",
            Self::task(world, p),
            b.target,
            b.path_index,
            b.path.len(),
            b.goal_area,
            b.site_area, self.team.role(p), self.team.plans.get(&p.team).map(|p|p.economy),
            b.failed_transitions,
            b.route_searches,
        )
    }

    /// The Terrorists' target site this round.
    fn t_site(world: &World) -> usize {
        let n = world.objectives.bomb_sites.len().max(1);
        (Rng::from_parts(&[world.round.round as u64, 0x517E]).next_u64() % n as u64) as usize
    }

    fn task(world: &World, bot: &Player) -> Task {
        if world.mode != GameMode::Competitive || world.objectives.bomb_sites.is_empty() || world.round.phase != Phase::Live {
            return Task::Roam;
        }
        if let Some(b) = world.bomb {
            // Keep an active defuser, otherwise choose one reachable teammate
            // by estimated travel + kit duration. The others cover approaches.
            let defuser = b.defuser.map(|d| d.0).filter(|id| world.player(*id).is_some_and(|p| p.alive))
                .or_else(|| world.players.iter().filter(|p| p.alive && p.team == Team::CounterTerrorist)
                    .min_by(|a, c| defuse_cost(a, b.pos).total_cmp(&defuse_cost(c, b.pos)).then(a.id.cmp(&c.id))).map(|p| p.id));
            return if defuser == Some(bot.id) { Task::Defuse(b.pos) } else { Task::Guard(b.pos) };
        }
        let sites = world.objectives.bomb_sites.len();
        match bot.team {
            Team::Terrorist => {
                if bot.has_bomb() {
                    return Task::Plant(Self::t_site(world));
                }
                if let Some(item) = world.dropped_bomb() {
                    // The nearest living Terrorist fetches it.
                    let nearest = world
                        .players
                        .iter()
                        .filter(|p| p.alive && p.team == Team::Terrorist)
                        .min_by(|a, b| a.movement.origin.distance(item.pos).total_cmp(&b.movement.origin.distance(item.pos)))
                        .map(|p| p.id);
                    if nearest == Some(bot.id) {
                        return Task::GoTo(item.pos);
                    }
                }
                Task::HoldSite(Self::t_site(world))
            }
            Team::CounterTerrorist => {
                let rank = world.players.iter().filter(|p| p.team == Team::CounterTerrorist && p.id < bot.id).count();
                Task::HoldSite((rank + world.round.round as usize) % sites)
            }
            Team::Spectator => Task::Roam,
        }
    }

    fn planned_task(&self, world: &World, bot: &Player) -> Task {
        use crate::tactics::Role;
        let fallback=Self::task(world,bot);
        if world.mode!=GameMode::Competitive || world.round.phase!=Phase::Live || world.objectives.bomb_sites.is_empty() {return fallback}
        let now=world.time();
        let role=self.team.role(bot);
        if let Some(b)=world.bomb {
            if bot.team==Team::CounterTerrorist {
                // Don't throw away saved equipment when no living CT can finish in time.
                let duration=world.rules.defuse_duration(bot.defuse_kit);
                if b.defuser.is_none() && b.explode_at-now<duration+bot.movement.origin.distance(b.pos)/250.0+0.25 {
                    if let Some(spawn)=world.spawns.iter().filter(|s|s.team==bot.team).max_by(|a,c|a.origin.distance_squared(b.pos).total_cmp(&c.origin.distance_squared(b.pos))) {return Task::GoTo(spawn.origin)}
                }
                if self.team.defuser==Some(bot.id) {return Task::Defuse(b.pos)}
                // Reach the site before taking a covering angle; don't camp outside a retake.
                if bot.movement.origin.distance(b.pos)>420.0 {return Task::GoTo(b.pos)}
            }
            return Task::Guard(b.pos)
        }
        let plan=&self.team.plans[&bot.team];
        if bot.team==Team::Terrorist {
            if matches!(fallback,Task::GoTo(_)) {return fallback}
            let site=plan.site;
            if !plan.executing && role!=Role::Lurk {
                if let Some(stage)=plan.staging {return Task::GoTo(stage+Vec3::new((bot.id%3) as f32*32.0-32.0,0.0,0.0))}
            }
            if bot.has_bomb() {return Task::Plant(site)}
            // Support follows the carrier along the actual route and can trade an entry.
            if role==Role::Support {
                if let Some(carrier)=world.players.iter().find(|p|p.alive&&p.has_bomb()) {
                    if carrier.movement.origin.distance(bot.movement.origin)>280.0 {return Task::GoTo(carrier.movement.origin)}
                }
            }
            if role==Role::Lurk && now<plan.execute_at+12.0 && world.objectives.bomb_sites.len()>1 {return Task::HoldSite((site+1)%world.objectives.bomb_sites.len())}
            Task::HoldSite(site)
        } else {
            if let Some(c)=self.team.contact(bot.team,now,bot.movement.origin) {
                if role==Role::Rotator || now-world.round.live_start>28.0 {
                    let site=world.objectives.bomb_sites.iter().enumerate().min_by(|a,b|a.1.zone.center().distance_squared(c.pos).total_cmp(&b.1.zone.center().distance_squared(c.pos))).map(|s|s.0).unwrap_or(0);
                    if c.pos.distance(world.objectives.bomb_sites[site].zone.center())<1100.0 {return Task::HoldSite(site)}
                }
            }
            fallback
        }
    }

    pub fn think(&mut self, world: &World) -> Vec<(PlayerId, TickInput)> {
        if world.mode == GameMode::Special { return Vec::new(); }
        let time = world.time();
        let dt = world.dt();
        let nav = self.nav.clone();
        self.team.update(world,nav.as_deref());
        if let (Some(mesh), true) = (nav.as_deref(), self.site_areas.len() != world.objectives.bomb_sites.len()) {
            self.site_areas = world.objectives.bomb_sites.iter().map(|s| areas_in(mesh, &s.zone)).collect();
        }
        let competitive = world.mode == GameMode::Competitive;
        let frozen = competitive && world.round.phase == Phase::Freeze;
        let t_side: Vec3 = average(world.spawns.iter().filter(|s| s.team == Team::Terrorist).map(|s| s.origin));
        let ct_side: Vec3 = average(world.spawns.iter().filter(|s| s.team == Team::CounterTerrorist).map(|s| s.origin));
        let reservations: Vec<_> = self.brains.iter().filter_map(|(id, b)| {
            let p = world.player(*id).filter(|p| p.alive)?;
            Some((*id, p.team, b.hold?.2))
        }).collect();
        let mut out = Vec::new();
        for bot in world.players.iter().filter(|p| p.is_bot) {
            let task=self.planned_task(world,bot);
            let contact=self.team.contact(bot.team,time,bot.movement.origin);
            let purchase=next_purchase(world,bot,&self.team);
            let brain = self.brains.entry(bot.id).or_insert_with(|| Brain::fresh(bot, 0));
            brain.ladder_navigation = false;
            if !bot.alive {
                // Forget everything; start fresh after respawning.
                *brain = Brain::fresh(bot, 0);
                continue;
            }
            if brain.progress_time == 0.0 {
                brain.progress_time = time;
                brain.progress_pos = bot.movement.origin;
                brain.yaw = bot.view_angles.y;
            }
            let mut rng = Rng::from_parts(&[world.tick, bot.id as u64, 0xB07]);
            let eye = bot.eye();
            if bot.health<brain.last_health {brain.hurt_at=time;}
            brain.last_health=bot.health;
            let mut input = TickInput {
                weapon_slot: gun_slot(bot, time),
                ..Default::default()
            };

            // Buying, one item per tick, while allowed.
            if competitive && bot.in_buy_zone && brain.bought_round != world.round.round && world.round.can_buy(&world.rules, time) {
                match purchase {
                    Some(item) if brain.buy_attempts < 12 => {
                        input.buy = item;
                        brain.buy_attempts += 1;
                    }
                    _ => {
                        brain.bought_round = world.round.round;
                        brain.buy_attempts = 0;
                    }
                }
            }
            // Clearing the actual blocker is real route progress. Give the
            // newly opened crossing its normal traversal window exactly once;
            // merely damaging an uncleared object never renews recovery.
            if let Some((index, from, to)) = brain.route_obstruction.take() {
                if brain.path.get(brain.path_index) == Some(&from)
                    && brain.path.get(brain.path_index + 1) == Some(&to) {
                    if world.breakables.get(index).is_some_and(|state| state.broken()) {
                        brain.progress_time = time;
                        brain.progress_pos = bot.movement.origin;
                    } else {
                        brain.route_obstruction = Some((index, from, to));
                    }
                }
            }
            if frozen {
                brain.route_obstruction = None;
                brain.ladder_route = None;
                // New round: drop the old path and objectives.
                brain.path.clear();
                brain.failed_transitions.clear();
                brain.failed_route = None;
                brain.goal_area = None;
                brain.site_area = None;
                brain.hold = None;
                brain.turn_velocity = glam::Vec2::ZERO;
                brain.duck_jump = false;
                brain.target = None;
                brain.yaw = bot.view_angles.y;
                brain.pitch = 0.0;
                brain.progress_time = time;
                brain.progress_pos = bot.movement.origin;
                input.view_angles = Vec3::new(0.0, brain.yaw, 0.0);
                out.push((bot.id, input));
                continue;
            }

            if bot.active_weapon().is_none_or(|w| w.reload_end.is_none()) { brain.reload_cover = None; }
            if bot.shots > brain.last_shots {
                 brain.burst_left = brain.burst_left.saturating_sub((bot.shots - brain.last_shots) as u32);
                 if brain.burst_left == 0 { brain.next_burst = time + 0.25 + rng.next_f32() * 0.3; }
             }
            brain.last_shots = bot.shots;
            let old_angles = glam::Vec2::new(brain.pitch, brain.yaw);
            // Perception (throttled and staggered).
            if self.aggressive && world.flash_amount(bot.id)<0.25 && (world.tick + bot.id as u64) % interval(world, PERCEPTION_RATE) == 0 {
                let mut best: Option<(PlayerId, Vec3, f32)> = None;
                let (fwd, _, _) = movement::angle_vectors(Vec3::new(0.0, brain.yaw, 0.0));
                for other in world.players.iter().filter(|o| o.alive && bot.team.enemy_of(o.team)) {
                    let chest = other.movement.origin + Vec3::new(0.0, 0.0, other.movement.view_offset() - 12.0);
                    let to = chest - eye;
                    let dist = to.length();
                    let score=dist*if brain.target==Some(other.id){0.7}else{1.0};
                    if dist > 3500.0 || best.is_some_and(|(_, _, d)| d < score) {
                        continue;
                    }
                    // Field of view ~140°, but notice anyone very close.
                    let facing = to.truncate().normalize_or_zero().dot(fwd.truncate());
                    if facing < 0.342 && dist > 160.0 {
                        continue;
                    }
                    if let Some(spot) = visible_target(world, eye, other) {
                        best = Some((other.id, spot, score));
                    }
                }
                match best {
                    Some((id, pos, _)) => {
                        if brain.target != Some(id) {
                            brain.seen_at = time;
                        }
                        brain.target = Some(id);
                        brain.target_pos = pos;
                        brain.last_seen = time;
                    }
                    None => {},
                }
            }
            // A single tracked target gets a current LOS check. Never update
            // its remembered position through walls or fire at stale visibility.
            brain.visible = false;
            if let Some(t) = brain.target.and_then(|id| world.player(id)) {
                if !t.alive { brain.target = None; }
                else if let Some(spot) = (world.flash_amount(bot.id)<0.25).then(|| visible_target(world, eye, t)).flatten() {
                    brain.target_pos = spot;
                    brain.last_seen = time;
                    brain.visible = true;
                } else if time - brain.last_seen > 1.25 { brain.target = None; }
            }

            if brain.visible {
                self.team.report(crate::tactics::Contact { team:bot.team,enemy:brain.target.unwrap(),pos:brain.target_pos,observed:time,available:time+0.3,visual:true });
            }
            // A defuse close to done is worth finishing under fire.
            let finishing = world.bomb.and_then(|b| b.defuser).is_some_and(|(d, done)| d == bot.id && (done - time < 2.5 || world.smoke_occludes(eye,brain.target_pos) || world.bomb.is_some_and(|b|b.explode_at-done<2.0)));
            let turn_rate = (240.0 + 480.0 * self.skill) * dt;
            if brain.visible && !finishing {
                let err = (1.0 - self.skill) * 3.0;
                let (mut tp, mut ty) = look_angles(eye, brain.target_pos);
                // Slowly drifting error preserves imperfect aim without a new
                // camera shake every 1/64 second when this bot is spectated.
                if time >= brain.next_aim_change {
                    brain.aim_goal = glam::Vec2::new(rng.range(-err, err), rng.range(-err, err));
                    brain.next_aim_change = time + 0.25;
                }
                brain.aim_error = brain.aim_error.lerp(brain.aim_goal, 1.0 - (-8.0 * dt).exp());
                let punch=bot.active_weapon().map_or([0.0;2],|w|w.aim_punch());
                tp -= punch[0]*crate::weapon::RECOIL_SCALE*self.skill;
                ty -= punch[1]*crate::weapon::RECOIL_SCALE*self.skill;
                tp += brain.aim_error.x;
                ty += brain.aim_error.y;
                brain.pitch = approach(brain.pitch, tp, turn_rate);
                brain.yaw = approach(brain.yaw, ty, turn_rate);
                let on_target = angle_diff(ty, brain.yaw).abs() < 2.5 && angle_diff(tp, brain.pitch).abs() < 2.5;
                let reaction = self.reaction_time.unwrap_or(0.45 - 0.3 * self.skill);
                if bot.active_weapon().is_some_and(|w| w.is_melee()) {
                    let target = world.player(brain.target.unwrap()).unwrap();
                    let (mn, mx) = target.bounds();
                    let in_range = crate::ray_box(eye, (brain.target_pos - eye).normalize_or_zero(), mn, mx)
                        .is_some_and(|distance| distance <= crate::MELEE_RANGE);
                    if in_range {
                        if on_target && time - brain.seen_at > reaction {
                            input.buttons |= buttons::ATTACK;
                        }
                    } else if let Some(mesh) = nav.as_deref() {
                        if let Some(area) = mesh.nearest_area(target.movement.origin) {
                            // Keep the existing planner's recovery, portals and
                            // movement speeds, while aiming at the visible enemy.
                            let aim = (brain.pitch, brain.yaw);
                            follow_path(mesh, world, bot, brain, &mut input, &mut rng, time, turn_rate,
                                Some(area.id), Some(target.movement.origin));
                            let (forward, right, _) = movement::angle_vectors(Vec3::new(0.0, brain.yaw, 0.0));
                            let wish = forward * input.forward_move + right * input.side_move;
                            (brain.pitch, brain.yaw) = aim;
                            move_towards(&mut input, brain.yaw, wish, wish.length());
                        }
                    } else {
                        let delta = (target.movement.origin - bot.movement.origin).with_z(0.0);
                        if can_walk(world, bot, delta.normalize_or_zero() * crate::MELEE_RANGE) {
                            move_towards(&mut input, brain.yaw, delta, 450.0);
                        }
                    }
                } else if on_target && brain.visible && time - brain.seen_at > reaction {
                    if brain.burst_left == 0 && time >= brain.next_burst {
                        brain.burst_left = if eye.distance(brain.target_pos)>1000.0 {2} else if eye.distance(brain.target_pos)<250.0 {7} else {3};
                    }
                    if brain.burst_left > 0 {
                        input.buttons |= buttons::ATTACK;
                        // A burst counts actual shots, not simulation ticks.
                        if let Some(w) = bot.active_weapon() {
                            if !w.def.full_auto && bot.last_input.buttons & buttons::ATTACK != 0 {
                                input.buttons &= !buttons::ATTACK;
                            }
                        }
                    }
                } else if brain.visible {
                    if time >= brain.next_strafe_change {
                        brain.strafe = if rng.next_f32() < 0.5 { -450.0 } else { 450.0 };
                        brain.next_strafe_change = time + 0.3 + rng.next_f32() * 0.5;
                    }
                    let (_, right, _) = movement::angle_vectors(Vec3::new(0.0, brain.yaw, 0.0));
                    if can_walk(world, bot, right * brain.strafe.signum() * 48.0) {
                        input.side_move = brain.strafe * 0.45;
                    }
                }
                if let Some(w) = bot.active_weapon() {
                    if w.zoom_levels() > 0 && w.zoom_level == 0 && brain.visible && eye.distance(brain.target_pos) > 450.0
                        && bot.last_input.buttons & buttons::ATTACK2 == 0 {
                        input.buttons |= buttons::ATTACK2;
                        input.buttons &= !buttons::ATTACK;
                    }
                    // Current visibility refreshes last_seen every tick. An
                    // empty automatic can otherwise hold ATTACK mid-burst
                    // forever, preventing idle reload and the move to cover.
                    let empty_reload = bot.movement.on_ground && w.clip == 0 && w.can_reload();
                    if w.reload_end.is_some() || empty_reload || (w.clip < (w.def.clip_size / 4).max(1) && time - brain.last_seen > 0.4) {
                        if w.can_reload() { input.buttons |= buttons::RELOAD; }
                        input.buttons &= !buttons::ATTACK;
                        if let Some(mesh) = nav.as_deref() {
                            if brain.reload_cover.is_none() {
                                brain.reload_cover = reload_cover(mesh, world, bot, brain.target_pos);
                            }
                            if let Some((area, point)) = brain.reload_cover {
                                follow_path(mesh, world, bot, brain, &mut input, &mut rng, time, turn_rate, Some(area), Some(point));
                            }
                        }
                    }
                }
            } else if self.aggressive {
                let (mn, mx) = bot.bounds();
                match task {
                    Task::Plant(site) if world.objectives.site_at(mn, mx) == Some(site) || (world.objectives.site_at(mn, mx).is_some() && bot.plant_start.is_some()) => {
                        input.weapon_slot = 5;
                        input.buttons |= buttons::ATTACK;
                        brain.pitch = approach(brain.pitch, 30.0, turn_rate);
                    }
                    Task::Defuse(pos) if (pos - bot.movement.origin).truncate().length() < 40.0 => {
                        let (p, y) = look_angles(eye, pos);
                        brain.pitch = approach(brain.pitch, p, turn_rate);
                        brain.yaw = approach(brain.yaw, y, turn_rate);
                        input.buttons |= buttons::USE | buttons::DUCK;
                    }
                    _ => match nav.as_deref() {
                        Some(mesh) => {
                            let (goal_area, point, hold_dir) = match task {
                                Task::Roam => (None, None, None),
                                Task::Plant(site) => {
                                    let areas = self.site_areas.get(site).cloned().unwrap_or_default();
                                    if brain.site_area.map(|s| s.0) != Some(site) || (matches!(task, Task::HoldSite(_)) && time > brain.repick_at && brain.repick_at > 0.0) {
                                        if !areas.is_empty() {
                                            brain.site_area = Some((site, areas[(rng.next_u64() as usize) % areas.len()]));
                                            brain.repick_at = 0.0;
                                        }
                                    }
                                    let face = if bot.team == Team::Terrorist { ct_side } else { t_side };
                                    (brain.site_area.map(|s| s.1), None, Some(face))
                                }
                                Task::HoldSite(_) | Task::Guard(_) => {
                                    let anchor = match task {
                                        Task::HoldSite(site) => world.objectives.bomb_sites[site].zone.center(),
                                        Task::Guard(pos) => pos,
                                        _ => unreachable!(),
                                    };
                                    let enemy_side = if bot.team == Team::Terrorist { ct_side } else { t_side };
                                    if brain.hold.is_none_or(|h| h.0.distance(anchor) > 48.0) || (brain.repick_at > 0.0 && time > brain.repick_at) {
                                        brain.hold = tactical_spot(mesh, world, bot, anchor, enemy_side, &reservations)
                                            .map(|(area, point, look)| (anchor, area, point, look));
                                        let covered=brain.hold.is_some();
                                        // Cache a reachable fallback too. An occluded bomb must not
                                        // trigger a full cover/LOS search every simulation tick.
                                        if !covered {brain.hold=mesh.nearest_area(anchor).map(|a|(anchor,a.id,a.center(),enemy_side));}
                                        brain.repick_at = time + if covered {12.0 + (bot.id % 5) as f32}else{3.0};
                                    }
                                    brain.hold.map_or((mesh.nearest_area(anchor).map(|a| a.id), Some(anchor), Some(enemy_side)),
                                        |(_, area, pos, look)| (Some(area), Some(pos), Some(look)))
                                }
                                Task::GoTo(pos) | Task::Defuse(pos) => {
                                    let face = if bot.team == Team::Terrorist { ct_side } else { t_side };
                                    (mesh.nearest_area(pos).map(|a| a.id), Some(pos), Some(face))
                                }
                            };
                            let arrived = follow_path(mesh, world, bot, brain, &mut input, &mut rng, time, turn_rate, goal_area, point);
                            if arrived {
                                // Holding a spot: face the enemy's side, now and then move.
                                if let Some(face) = hold_dir {
                                    let (_, y) = look_angles(bot.movement.origin, face);
                                    brain.yaw = approach(brain.yaw, y, turn_rate * 0.5);
                                    brain.pitch = approach(brain.pitch, 0.0, turn_rate);
                                }
                                if brain.repick_at == 0.0 {
                                    brain.repick_at = time + 8.0 + rng.next_f32() * 10.0;
                                }
                            }
                        }
                        None => wander(world, bot, brain, &mut input, &mut rng, turn_rate),
                    },
                }
            }
            // Reload while out of contact, and watch reported positions without aiming through walls.
            if !brain.visible && !matches!(task,Task::Plant(_)|Task::Defuse(_)) {
                if let Some(w)=bot.active_weapon() {if w.can_reload() && w.clip<w.def.clip_size/2 && time-brain.last_seen>1.0 {input.buttons|=buttons::RELOAD;}}
            }
            if self.shooting && brain.throw.is_none() && self.aggressive && !finishing && world.flash_amount(bot.id)<0.2
                && time>=brain.next_utility && time>=self.team.utility_after.get(&bot.team).copied().unwrap_or(0.0)
                && (world.tick+bot.id as u64)%interval(world, UTILITY_RATE)==0 && time-brain.hurt_at>1.5 && bot.plant_start.is_none()
                && (!brain.visible || (eye.distance(brain.target_pos)>550.0 && bot.health>60
                    && world.players.iter().any(|p|p.id!=bot.id&&p.alive&&p.team==bot.team&&p.eye().distance(eye)<500.0&&world.visible(p.eye(),brain.target_pos)))) {
                if let Some((def,angles))=choose_utility(world,bot,contact,task) {
                    brain.throw=Some((def,angles,time+3.5));
                    self.team.utility_after.insert(bot.team,time+3.0);
                }
                brain.next_utility=time+1.0;
            }
            // Reuse this decision's exact actor/definition/target solution at
            // the release gate below; no cache survives a think/tick.
            let mut planned_throw = None;
            if let Some((def,target,until))=brain.throw {
                let angles=world.bot_lob_angles(bot,def,target).unwrap_or(bot.view_angles);
                planned_throw = Some((def, target, angles));
                if time>until || !bot.weapons.iter().any(|w|w.def.def_index==def) || time-brain.hurt_at<0.6 || (brain.visible&&eye.distance(brain.target_pos)<300.0) {
                    brain.throw=None;brain.next_utility=time+5.0;
                } else {
                    input.utility=def;input.weapon_slot=0;input.buttons=0;
                    input.forward_move=0.0;input.side_move=0.0;
                    brain.pitch=approach(brain.pitch,angles.x,turn_rate);brain.yaw=approach(brain.yaw,angles.y,turn_rate);
                }
            }
            // Preserve the path's world-space direction while the head turns.
            // Stable yielding also keeps teammates from occupying one point.
            let (forward, right, _) = movement::angle_vectors(Vec3::new(0.0, brain.yaw, 0.0));
            let mut wish = forward * input.forward_move + right * input.side_move;
            if !brain.visible && brain.throw.is_none() && input.buttons & (buttons::USE|buttons::ATTACK)==0 {
                if let Some(report)=contact.filter(|c| c.pos.distance(eye)<1200.0) {
                    let (pitch,yaw)=look_angles(eye,report.pos);
                    brain.yaw=approach(old_angles.y,yaw,turn_rate*0.7);
                    brain.pitch=approach(old_angles.x,pitch.clamp(-25.0,25.0),turn_rate*0.7);
                    if world.bomb.is_none() && self.team.role(bot)==crate::tactics::Role::Lurk && report.pos.distance(eye)<700.0 {
                        input.buttons|=buttons::WALK;
                    }
                }
            }
            if wish.length_squared() > 1.0 && !brain.ladder_navigation {
                for other in world.players.iter().filter(|p| p.alive && p.id != bot.id && p.team == bot.team) {
                    let delta = (bot.movement.origin - other.movement.origin).truncate();
                    let distance = delta.length();
                    if distance < 62.0 && (bot.movement.origin.z - other.movement.origin.z).abs() < 48.0 {
                        let away = if distance > 1.0 { delta / distance } else { glam::Vec2::new(0.0, if bot.id > other.id { 1.0 } else { -1.0 }) };
                        wish += away.extend(0.0) * (62.0 - distance) * 5.0;
                        if bot.id > other.id && wish.dot((other.movement.origin - bot.movement.origin).normalize_or_zero()) > 0.0 { wish *= 0.55; }
                    }
                }
            }
            wish = wish.clamp_length_max(450.0);
            // A closed use-only leaf is not an unreachable nav crossing. Look
            // along the actual movement command, then let ordinary player use
            // selection admit the final smoothed aim below. Do not borrow aim
            // from combat, bomb work, utility or an unfinished weapon action.
            let route_action = !brain.ladder_navigation && !brain.visible && brain.throw.is_none()
                && bot.plant_start.is_none()
                && !world.bomb.is_some_and(|b| b.defuser.is_some_and(|(id, _)| id == bot.id))
                && input.buttons & (buttons::USE | buttons::ATTACK | buttons::ATTACK2 | buttons::RELOAD) == 0
                && input.utility == 0 && input.weapon_slot != 5
                && bot.active_weapon().is_none_or(|w| {
                    w.reload_end.is_none() && w.burst.is_none() && w.suppressor_transition.is_none()
                        && w.grenade_throw.is_none() && time >= w.next_attack && time >= w.next_secondary_attack
                });
            let route_door = if route_action {
                closed_route_door(world, bot, wish)
            } else { None };
            let route_breakable = if route_action && route_door.is_none() && self.shooting
                && brain.target.is_none() && !finishing && world.flash_amount(bot.id) < 0.2
                && brain.last_navigation_tick == Some(world.tick)
                && brain.path_index + 1 < brain.path.len()
            {
                blocking_route_breakable(world, bot, wish)
            } else { None };
            if route_door.is_some() {
                let (pitch, yaw) = look_angles(eye, eye + wish);
                brain.pitch = approach(brain.pitch, pitch, turn_rate);
                brain.yaw = approach(brain.yaw, yaw, turn_rate);
            }
            if let Some((_, point)) = route_breakable {
                let (pitch, yaw) = look_angles(eye, point);
                let punch = bot.active_weapon().filter(|w| !w.is_melee()).map_or([0.; 2], |w| w.aim_punch());
                brain.pitch = approach(brain.pitch, pitch - punch[0] * crate::weapon::RECOIL_SCALE, turn_rate);
                brain.yaw = approach(brain.yaw, yaw - punch[1] * crate::weapon::RECOIL_SCALE, turn_rate);
            }
            brain.pitch = smooth_angle(old_angles.x, brain.pitch, &mut brain.turn_velocity.x, dt, 300.0);
            brain.yaw = smooth_angle(old_angles.y, brain.yaw, &mut brain.turn_velocity.y, dt, 420.0);
            // The surface converts level inward/outward input into climbing.
            // Keep that command independent of a previous search/contact pitch;
            // combat and utility retain their existing aim ownership.
            if brain.ladder_navigation && !brain.visible && brain.throw.is_none() {
                brain.pitch = 0.0;
                brain.turn_velocity.x = 0.0;
            }
            let (forward, right, _) = movement::angle_vectors(Vec3::new(0.0, brain.yaw, 0.0));
            input.forward_move = wish.dot(forward);
            input.side_move = wish.dot(right);
            if brain.visible && !finishing {
                let (p, y) = look_angles(eye, brain.target_pos);
                let punch=bot.active_weapon().map_or([0.0;2],|w|w.aim_punch());
                let blocked=world.players.iter().filter(|p|p.alive&&p.team==bot.team&&p.id!=bot.id).any(|p|{
                    let delta=brain.target_pos-eye;let t=((p.eye()-eye).dot(delta)/delta.length_squared().max(1.0)).clamp(0.0,1.0);
                    t>0.0 && t<1.0 && (eye+delta*t).distance(p.eye()-Vec3::Z*10.0)<30.0
                });
                let melee = bot.active_weapon().is_some_and(|w| w.is_melee());
                let moving=!melee && bot.movement.velocity.truncate().length()>bot.active_weapon().map_or(55.0,|w|w.def.max_player_speed*0.28);
                // Melee authority permits movement and uses the final aim ray
                // against the enemy hull, rather than a gun's accuracy posture.
                let melee_miss = melee && brain.target.and_then(|id| world.player(id)).is_none_or(|target| {
                    let (mn, mx) = target.bounds();
                    let (dir, _, _) = movement::angle_vectors(Vec3::new(brain.pitch, brain.yaw, 0.0));
                    crate::ray_box(eye, dir, mn, mx).is_none_or(|distance| distance > crate::MELEE_RANGE)
                });
                if blocked || moving || (!melee && !bot.movement.on_ground) || melee_miss || !brain.visible || angle_diff(y, brain.yaw+punch[1]*crate::weapon::RECOIL_SCALE).abs() > 4.0 || angle_diff(p, brain.pitch+punch[0]*crate::weapon::RECOIL_SCALE).abs() > 4.0 {
                    input.buttons &= !buttons::ATTACK;
                }
            }
            if let Some((def,target,_))=brain.throw {
                let angles = match planned_throw {
                    Some((planned_def, planned_target, angles)) if planned_def == def && planned_target == target => angles,
                    _ => world.bot_lob_angles(bot,def,target).unwrap_or(bot.view_angles),
                };
                if input.utility==def && angle_diff(angles.x,brain.pitch).abs()<1.5 && angle_diff(angles.y,brain.yaw).abs()<1.5
                    && bot.movement.velocity.length()<35.0 && bot.active_weapon().is_some_and(|w|w.def.def_index==def && time>=w.next_attack)
                    && world.clear_bot_lob(bot,def,target,angles) {input.buttons|=buttons::ATTACK;}
                if bot.active_weapon().and_then(|w|w.grenade_throw).is_some_and(|s|s.phase==crate::utility::ThrowPhase::Holding) {
                    input.buttons&=!(buttons::ATTACK|buttons::ATTACK2);
                }
            }
            if world.flash_amount(bot.id)>0.4 { input.buttons &= !buttons::ATTACK; input.buttons|=buttons::WALK; }
            prepare_jump(bot, brain, &mut input);
            input.view_angles = Vec3::new(brain.pitch.clamp(-89.0, 89.0), brain.yaw, 0.0);
            if let Some((index, _)) = route_breakable {
                if clear_route_attack(world, bot, &input, index) {
                    brain.route_obstruction = Some((index, brain.path[brain.path_index], brain.path[brain.path_index + 1]));
                    input.buttons |= buttons::ATTACK;
                }
            }
            if !self.shooting { input.buttons &= !(buttons::ATTACK | buttons::ATTACK2); }
            if let Some(index) = route_door {
                let use_input = TickInput { buttons: input.buttons | buttons::USE, ..input };
                if world.used_door(bot, &use_input) == Some(index) {
                    input = use_input;
                }
            }
            out.push((bot.id, input));
        }
        out
    }
}

fn average(points: impl Iterator<Item = Vec3>) -> Vec3 {
    let (sum, n) = points.fold((Vec3::ZERO, 0), |(s, n), p| (s + p, n + 1));
    if n == 0 { Vec3::ZERO } else { sum / n as f32 }
}

fn choose_utility(world: &World, bot: &Player, contact: Option<crate::tactics::Contact>, task: Task) -> Option<(u32,Vec3)> {
    use crate::utility::Kind;
    let eye=bot.eye();
    let recent=contact.filter(|c|world.time()-c.observed<3.0);
    let defending=bot.team==Team::CounterTerrorist && world.bomb.is_none();
    let retaking=bot.team==Team::CounterTerrorist && world.bomb.is_some();
    for w in &bot.weapons {
        let Some(kind)=Kind::from_name(&w.def.class_name) else {continue};
        let target=match kind {
            Kind::Smoke if retaking => world.bomb?.pos+Vec3::Z*6.0,
            Kind::Smoke if recent.is_some() && (bot.health<45 || !defending) => {
                let c=recent?;eye.lerp(c.pos,0.7)-Vec3::Z*40.0
            }
            Kind::Flash if recent.is_some() => recent?.pos+Vec3::Z*20.0,
            Kind::He if recent.is_some() => recent?.pos-Vec3::Z*36.0,
            Kind::Molotov|Kind::Incendiary if recent.is_some() && !matches!(task,Task::Defuse(_)) => recent?.pos-Vec3::Z*48.0,
            _ => continue,
        };
        let distance=eye.distance(target);
        if !(180.0..1000.0).contains(&distance) {continue}
        if world.utility_fields.iter().any(|f| f.pos.distance(target)<200.0 && (f.kind==kind || (f.kind.fire()&&kind.fire()))) {continue}
        if world.projectiles.iter().any(|p| p.kind==kind&&world.player(p.owner).is_some_and(|p|p.team==bot.team)) {continue}
        if kind!=Kind::Smoke && world.players.iter().filter(|p|p.alive&&p.team==bot.team).any(|p|{
            let predicted=p.eye()+p.movement.velocity*0.5;
            predicted.distance(target)<if kind==Kind::Flash {650.0}else{260.0} && world.visible(predicted,target)
        }) {continue}
        let Some(angles)=world.bot_lob_angles(bot,w.def.def_index,target) else {continue};
        if world.clear_bot_lob(bot,w.def.def_index,target,angles) {return Some((w.def.def_index,target))}
    }
    None
}

/// The eye-height movement ray identifies a leaf, and the next commanded hull
/// position must overlap it. Proximity alone must not open doors beyond a
/// stopping point. Use the existing interaction reach, hull and collision
/// owner; walls and other runtime solids in front reject the query.
/// Every linked leaf must be closed and idle: USE would otherwise close the
/// group, and a partially closing leaf must finish before we request opening.
fn closed_route_door(world: &World, bot: &Player, wish: Vec3) -> Option<usize> {
    if world.doors.is_empty() { return None; }
    let direction = wish.try_normalize()?;
    let (trace, owner) = world.collision.trace_owned_ray(bot.eye(),
        bot.eye() + direction * crate::doors::USE_DISTANCE, collision::contents::MASK_PLAYERSOLID);
    if !trace.hit() || trace.start_solid || trace.all_solid { return None; }
    let collision::HitOwner::Door(index) = owner? else { return None; };
    let def = &world.collision.doors[index];
    let (mins, maxs) = bot.movement.hull();
    if !collision::doors::obstructed(&def.solids(world.doors[index].angle),
        bot.movement.origin + wish * world.dt(), mins, maxs)
    { return None; }
    let group = def.group;
    world.collision.doors.iter().zip(&world.doors)
        .filter(|(def, _)| def.group == group)
        .all(|(def, state)| !def.locked && state.angle == 0.0 && state.target == 0.0)
        .then_some(index)
}

/// A deliberately local obstruction response, not a search for nearby glass.
/// Only an eye-height shot owner overlapping the next commanded player hull
/// qualifies. Static geometry and other runtime blockers must leave that move
/// clear. Recovery's existing jump/replan deadlines keep owning failed routes;
/// attacking never renews progress or edits health/collision here.
fn blocking_route_breakable(world: &World, bot: &Player, wish: Vec3) -> Option<(usize, Vec3)> {
    let w = bot.active_weapon()?;
    // A follow-up burst/pellet could continue beyond a newly cleared vent.
    // Leave those weapons to ordinary recovery rather than queue blind fire.
    if w.burst_mode || (!w.is_melee() && w.def.bullets.max(1) != 1) { return None; }
    let range = if w.is_melee() { crate::MELEE_RANGE } else if w.uses_ammo() && w.clip > 0
        && !matches!(w.def.class, weapons::WeaponClass::Grenade | weapons::WeaponClass::Equipment)
    { w.def.range.max(1.0) } else { return None; };
    let direction = wish.try_normalize()?;
    let (trace, owner) = world.collision.trace_owned_ray(bot.eye(), bot.eye() + direction * range,
        collision::contents::MASK_SHOT);
    if !trace.hit() || trace.start_solid || trace.all_solid { return None; }
    let collision::HitOwner::Breakable(index) = owner? else { return None; };
    let def = &world.collision.breakables[index];
    if world.breakables[index].broken() || def.trigger_only || def.material == 7
        || world.breakables[index].health <= 0.0 { return None; }
    let (mins, maxs) = bot.movement.hull();
    let next = bot.movement.origin + wish * world.dt();
    let blocks = |solids: &[collision::Convex], pos| solids.iter()
        .filter(|solid| solid.contents & collision::contents::MASK_PLAYERSOLID != 0)
        .any(|solid| collision::doors::obstructed(std::slice::from_ref(solid), pos, mins, maxs));
    if !blocks(&def.solids, next) || blocks(&def.solids, bot.movement.origin)
        || world.collision.static_world().trace_box(bot.movement.origin, next, mins, maxs,
            collision::contents::MASK_PLAYERSOLID).hit()
        || world.collision.doors.iter().zip(&world.doors).any(|(def, state)| blocks(&def.solids(state.angle), next))
        || world.collision.breakables.iter().zip(&world.breakables).enumerate()
            .any(|(other, (def, state))| other != index && !state.broken() && blocks(&def.solids, next))
        || world.collision.breakable_dependencies.iter().any(|dep|
            !dep.killed_by.iter().any(|&index| world.breakables[index].broken()) && blocks(&dep.solids, next))
    { return None; }
    Some((index, trace.end))
}

/// Admission only: reproduce the active attack's post-movement, post-think
/// rays (including recoil/seeded spread), then send a normal primary trigger.
/// World::fire/melee_attack still resolve every hit and apply all damage.
fn clear_route_attack(world: &World, bot: &Player, input: &TickInput, index: usize) -> bool {
    let Some(active) = bot.active_weapon() else { return false; };
    let time = world.time();
    if active.reload_end.is_some() || active.burst.is_some() || active.burst_mode
        || active.suppressor_transition.is_some() || active.grenade_throw.is_some()
        || time < active.next_attack || time < active.next_secondary_attack
        || (!active.is_melee() && (!active.uses_ammo() || active.clip <= 0 || active.def.bullets.max(1) != 1))
        || (!active.is_melee() && !active.def.full_auto && bot.last_input.buttons & buttons::ATTACK != 0)
        // Do not predict a slot change/cycle, a purchase, or a swimming move.
        || (input.weapon_slot != 0 && (input.weapon_slot != crate::weapon::slot(&active.def)
            || input.weapon_slot != bot.last_input.weapon_slot))
        || input.buy != Purchase::None || input.drop || input.utility != 0
        || world.collision.dynamic.as_ref().is_some_and(|d|
            d.point_contents(bot.movement.origin + Vec3::Z * 32., collision::contents::WATER) != 0)
    { return false; }
    let mut movement = bot.movement;
    movement::player_move(&mut movement, &movement::MoveInput {
        view_angles: input.view_angles, forward_move: input.forward_move, side_move: input.side_move,
        buttons: input.buttons, weapon_max_speed: active.max_speed(),
    }, &world.config, world.collision.as_ref(), world.dt());
    let eye = movement.eye_position();
    let mut w = active.clone();
    w.advance_recoil_motion(world.tick);
    crate::weapon::think(&mut w, time, world.dt(), movement.ducked, true);
    let def = &world.collision.breakables[index];
    let range = if w.is_melee() { crate::MELEE_RANGE } else { w.def.range.max(1.0) };
    let admitted = |direction: Vec3| {
        let (trace, owner) = world.collision.trace_owned_ray(eye, eye + direction * range,
            collision::contents::MASK_SHOT);
        if owner != Some(collision::HitOwner::Breakable(index)) || !trace.hit()
            || trace.start_solid || trace.all_solid || (!w.is_melee() && world.collision.is_sky(trace.surface))
            || world.players.iter().filter(|p| p.id != bot.id && p.alive).any(|p| {
                let (mins, maxs) = p.bounds();
                (eye.cmpge(mins).all() && eye.cmple(maxs).all())
                    || crate::ray_box(eye, direction, mins, maxs).is_some_and(|d| d <= trace.fraction * range)
            })
            || world.collision.dynamic.as_ref().is_some_and(|d| d.hit_target(eye, trace.end).is_some())
        { return false; }
        // Exactly the primary club/bullet amounts and authored damage gates
        // used by melee_attack/fire -> damage_breakable. Zero damage must not
        // consume ammo or keep slashing an invulnerable navigation obstacle.
        let amount = if w.is_melee() {
            (if w.last_melee_attack.is_none_or(|t| time - t > 0.9) { 40.0 } else { 25.0 }) * def.club_scale
        } else {
            w.def.damage * w.def.range_modifier.powf(trace.fraction * range / 500.0) * def.bullet_scale
        };
        amount.is_finite() && amount > 0.0 && amount >= def.minimum_damage
    };
    if w.is_melee() {
        return admitted(movement::angle_vectors(input.view_angles).0);
    }
    let punch = w.aim_punch();
    let (forward, right, up) = movement::angle_vectors(input.view_angles
        + Vec3::new(punch[0], punch[1], 0.0) * crate::weapon::RECOIL_SCALE);
    let inaccuracy = crate::weapon::inaccuracy(&w, crate::weapon::AccuracyContext {
        speed: movement.speed_2d(), on_ground: movement.on_ground, ducked: movement.ducked,
    }) * 0.001;
    let spread = w.alt_stat("spread", w.def.accuracy.spread) * 0.001;
    let mut rng = Rng::from_parts(&[bot.id as u64, bot.shots]);
    let r1 = rng.next_f32();
    let a1 = rng.range(0.0, std::f32::consts::TAU);
    let r2 = rng.next_f32();
    let a2 = rng.range(0.0, std::f32::consts::TAU);
    let ox = a1.cos() * r1 * inaccuracy + a2.cos() * r2 * spread;
    let oy = a1.sin() * r1 * inaccuracy + a2.sin() * r2 * spread;
    admitted((forward + right * ox + up * oy).normalize())
}

fn wander(world: &World, bot: &crate::Player, brain: &mut Brain, input: &mut TickInput, rng: &mut Rng, turn_rate: f32) {
    let moved = (bot.movement.origin - brain.last_pos).truncate().length();
    brain.last_pos = bot.movement.origin;
    // Under 64 units/s (half a unit per tick at 128 tick) counts as stuck.
    let still = 64.0 / world.tick_rate as f32;
    brain.stuck_ticks = if moved < still { brain.stuck_ticks + 1 } else { 0 };
    if brain.stuck_ticks as u64 > world.ticks(STUCK_SECONDS) {
        brain.wander_yaw += rng.range(90.0, 270.0);
        brain.stuck_ticks = 0;
    } else if rng.next_f32() < 0.004 * 64.0 / world.tick_rate as f32 {
        brain.wander_yaw += rng.range(-60.0, 60.0);
    }
    brain.yaw = approach(brain.yaw, brain.wander_yaw, turn_rate * 0.5);
    brain.pitch = approach(brain.pitch, 0.0, turn_rate);
    input.forward_move = 450.0;
}

fn prepare_jump(bot: &Player, brain: &mut Brain, input: &mut TickInput) {
    if brain.ladder_navigation {
        brain.duck_jump = false;
        // Only an attached bottom dismount may request a fresh jump-away.
        if bot.movement.ladder_normal.is_none() { input.buttons &= !buttons::JUMP; }
        return;
    }
    if bot.movement.on_ground {
        // Only a fresh grounded press starts a duck-jump. Falling off an
        // edge must not turn a recovery request into an airborne crouch.
        brain.duck_jump = input.buttons & buttons::JUMP != 0
            && bot.last_input.buttons & buttons::JUMP == 0;
        if !brain.duck_jump { input.buttons &= !buttons::JUMP; }
    } else {
        input.buttons &= !buttons::JUMP;
        // Keep the feet tucked for the whole jump, even after rising above
        // the portal removes its jump request. Repeated duck/unduck changes
        // the airborne hull origin and makes both the pose and target rise
        // oscillate. Landing clears this latch on the next grounded input.
        if brain.duck_jump { input.buttons |= buttons::DUCK; }
    }
}

/// Follows (or plans) a path. With `goal` the path leads to that area and,
/// with `point`, on to that exact spot; without, to a random hot place.
/// Returns true when the goal has been reached (the bot stands still).
#[allow(clippy::too_many_arguments)]
fn follow_path(
    mesh: &NavMesh,
    world: &World,
    bot: &crate::Player,
    brain: &mut Brain,
    input: &mut TickInput,
    rng: &mut Rng,
    time: f32,
    turn_rate: f32,
    goal: Option<u32>,
    point: Option<Vec3>,
) -> bool {
    let pos = bot.movement.origin;
    // Feet may be above the mesh during a jump or on a prop, but being
    // underneath a ledge is not arrival on its upper floor. Match the small
    // below-floor tolerance used by nearest_area's floor preference.
    let in_area = |id: u32, margin: f32| {
        mesh.area(id).is_some_and(|a| {
            let dz = pos.z - a.z_at(pos.x, pos.y);
            a.contains_xy(pos, margin) && (-8.0..120.0).contains(&dz)
        })
    };
    let current_ladder = brain.path.get(brain.path_index).zip(brain.path.get(brain.path_index + 1))
        .and_then(|(&from, &to)| mesh.ladder_transition(from, to));
    let old_failed_count = brain.failed_transitions.len();
    brain.failed_transitions.retain(|edge| time < edge.retry_at);
    if brain.failed_transitions.len() != old_failed_count {
        brain.failed_route = None;
    }
    // A cached route may pause during combat. Time spent not following it
    // must not turn the first resumed command into a failed crossing.
    let resumed = brain.last_navigation_tick.is_some_and(|tick| world.tick.saturating_sub(tick) > 1);
    if resumed {
        brain.progress_time = time;
        brain.progress_pos = pos;
    }
    // Displacement can leave the cached route even while navigation runs
    // every tick. Its old portal is no longer an approach from this position;
    // retrying it can blacklist a crossing we never attempted. Outside the
    // normal look-ahead, continuous navigation needs grounded membership
    // in another area to establish displacement: linked areas may have gaps,
    // and airborne feet may leave their floor bounds. Preserve combat resume.
    if !brain.path.iter().skip(brain.path_index).take(4).any(|&id| in_area(id, 0.0))
        && !current_ladder.is_some_and(|(ladder, _)| ladder_contact(world, bot, ladder).1)
        && (resumed || (bot.movement.on_ground
            && mesh.nearest_area(pos).is_some_and(|a| in_area(a.id, 0.0))))
    {
        brain.path.clear();
        brain.path_index = 0;
    }
    brain.last_navigation_tick = Some(world.tick);
    if goal != brain.goal_area {
        brain.goal_area = goal;
        brain.path.clear();
        brain.path_index = 0;
        brain.failed_route = None;
    }
    if (world.tick+bot.id as u64)%interval(world, FIRE_CHECK_RATE)==0 && brain.path.iter().skip(brain.path_index).take(3).any(|id|mesh.area(*id).is_some_and(|a|world.fire_at(a.center()))) {brain.path.clear();}
    let need_path = brain.path_index + 1 >= brain.path.len();
    if need_path {
        brain.ladder_route = None;
        let start = mesh.nearest_area(pos).map(|a| a.id);
        if let (Some(g), Some(s)) = (goal, start) {
            if s == g && in_area(g, -2.0) {
                // In the goal area: walk to the exact point, if any.
                if let Some(p) = point {
                    if (p - pos).truncate().length() > 16.0 {
                        let (_, ty) = look_angles(pos, p);
                        brain.yaw = approach(brain.yaw, ty, turn_rate);
                        brain.pitch = approach(brain.pitch, 0.0, turn_rate);
                        move_towards(input, brain.yaw, p - pos, ((p - pos).truncate().length() * 6.0).min(250.0));
                        return false;
                    }
                }
                brain.path.clear();
                return true;
            }
        }
        brain.path.clear();
        brain.path_index = 0;
        let target = goal.or_else(|| pick_goal(mesh, rng));
        if let (Some(start), Some(target)) = (start, target) {
            // A disconnected fixed objective must not trigger a full A*
            // search every tick while the bot wanders. The key changes as
            // soon as it enters another area; changing/expiring the edge
            // exclusions also invalidates this result, without a new timer.
            if brain.failed_route != Some((start, target)) {
                brain.route_searches = brain.route_searches.wrapping_add(1);
                let route = mesh.find_path_with_traversal(
                    start, target,
                    |a| if world.fire_at(a.center()) {5000.0}else{0.0},
                    |from, to| !brain.failed_transitions.iter().any(|e| e.from == from && e.to == to),
                );
                brain.failed_route = if route.is_none() && !brain.failed_transitions.is_empty() {
                    Some((start, target))
                } else { None };
                if let Some(p) = route {
                    brain.path = p;
                }
            }
        }
        brain.progress_time = time;
        brain.progress_pos = pos;
        if brain.path.len() < 2 {
            wander(world, bot, brain, input, rng, turn_rate);
            return false;
        }
    }
    // Advance when inside one of the next few areas (a bot may stand above
    // the mesh on a prop, or drop past a small area).
    for ahead in (1..=3).rev() {
        let k = brain.path_index + ahead;
        // Never look ahead across a ladder or call an airborne/climbing bot
        // arrived on a lower floor merely because it is within 120 units.
        let ladder_crossing = (brain.path_index..k.min(brain.path.len().saturating_sub(1)))
            .any(|i| mesh.ladder_transition(brain.path[i], brain.path[i + 1]).is_some());
        let ladder_arrival = ahead == 1 && bot.movement.on_ground && bot.movement.ladder_normal.is_none();
        if k < brain.path.len() && (!ladder_crossing || ladder_arrival) && in_area(brain.path[k], -2.0) {
            brain.path_index = k;
            brain.ladder_route = None;
            brain.progress_pos = pos;
            brain.progress_time = time;
            break;
        }
    }
    if brain.path_index + 1 >= brain.path.len() {
        return false;
    }
    let cur = brain.path[brain.path_index];
    let next = brain.path[brain.path_index + 1];
    if let Some((ladder, up)) = mesh.ladder_transition(cur, next) {
        follow_ladder(mesh, world, bot, brain, input, ladder, up, cur, next, time, turn_rate);
        return false;
    }
    brain.ladder_route = None;
    let Some(mut target) = mesh.portal(cur, next, 20.0) else {
        brain.path.clear();
        return false;
    };
    // The shared portal itself can lie exactly on the area boundary. Aim
    // inside the destination so a bot that stops at the seam still has a
    // forward command and can satisfy the inset arrival test above.
    if let Some(a) = mesh.area(next) {
        let inset_x = ((a.se.x - a.nw.x) * 0.5).min(20.0);
        let inset_y = ((a.se.y - a.nw.y) * 0.5).min(20.0);
        target.x = target.x.clamp(a.nw.x + inset_x, a.se.x - inset_x);
        target.y = target.y.clamp(a.nw.y + inset_y, a.se.y - inset_y);
        target.z = a.z_at(target.x, target.y);
    }
    if world.fire_at(target) && !world.fire_at(pos) && pos.distance(target)<180.0 {
        // If every route crosses a burning choke, hold outside it until it clears.
        input.forward_move=0.0;input.side_move=0.0;
        brain.progress_time=time;
        return false;
    }
    let (_, ty) = look_angles(pos, target);
    brain.yaw = approach(brain.yaw, ty, turn_rate);
    brain.pitch = approach(brain.pitch, 0.0, turn_rate);
    // Move along the path; slow down for sharp turns.
    move_towards(input, brain.yaw, target - pos, 450.0);
    if let Some(a) = mesh.area(next) {
        let rise = target.z - pos.z;
        let near = (target - pos).truncate().length() < 96.0;
        if near && a.flags & nav::attr::NO_JUMP == 0 && (rise > 20.0 || (a.flags & nav::attr::JUMP != 0 && rise > 8.0)) {
            input.buttons |= buttons::JUMP;
        }
        if a.flags & nav::attr::CROUCH != 0 {
            input.buttons |= buttons::DUCK;
        }
    }
    // Airborne height alone is not path progress: a bot jumping against a
    // box can rise farther than this threshold every attempt. Counting that
    // rise keeps resetting the recovery clock, so the route never replans.
    // A completed step onto a different floor still counts once grounded.
    let displacement = pos - brain.progress_pos;
    // Escape strafing around the same corner is not progress toward its
    // portal. Otherwise a sideways oscillation renews the timer forever.
    let progressed = (target - brain.progress_pos).truncate().length()
        - (target - pos).truncate().length() > 48.0
        || (bot.movement.on_ground && displacement.z.abs() > 48.0);
    // Stuck handling: jump after 0.75 s without progress, re-path after 2 s.
    if progressed {
        brain.progress_pos = pos;
        brain.progress_time = time;
    } else if time - brain.progress_time > 2.0 {
        // Replanning the identical deterministic A* route immediately would
        // retry this same inaccessible crossing indefinitely. Remember only
        // the attempted directed edge, leaving other approaches available.
        if cur != next && mesh.area(cur).is_some_and(|a| a.connections.iter().any(|d| d.contains(&next))) {
            remember_failed_transition(brain, cur, next, time);
        }
        brain.path.clear();
        brain.progress_time = time;
        input.forward_move = 0.0;
        input.side_move = 0.0;
        input.buttons &= !buttons::JUMP;
    } else if time - brain.progress_time > 0.75 {
        if mesh.area(next).is_some_and(|a| a.flags & nav::attr::NO_JUMP == 0) {
            input.buttons |= buttons::JUMP;
        }
        if time >= brain.escape_until {
            let (_, right, _) = movement::angle_vectors(Vec3::new(0.0, brain.yaw, 0.0));
            let preferred = if bot.id % 2 == 0 { 1.0 } else { -1.0 };
            brain.escape_side = if can_walk(world, bot, right * preferred * 48.0) { preferred } else { -preferred };
            brain.escape_until = time + 0.65;
        }
        input.side_move = brain.escape_side * 300.0;
    }
    false
}

fn ladder_face(world: &World, bot: &Player, ladder: &nav::Ladder, point: Vec3) -> Option<collision::Trace> {
    let (mins, maxs) = bot.movement.hull();
    let normal = ladder.normal();
    let reach = ladder.width + ((maxs - mins) * 0.5).dot(normal.abs());
    let start = point + normal * reach;
    let end = point - normal * reach;
    if !start.is_finite() || !end.is_finite() || !(end - start).is_finite() { return None; }
    let tr = world.collision.trace_ray(start, end, collision::contents::LADDER);
    (tr.hit() && !tr.start_solid && !tr.all_solid && tr.normal.dot(normal) > 0.7).then_some(tr)
}

/// Confirm this selected NAV ribbon, not merely some ladder in movement state.
fn ladder_contact(world: &World, bot: &Player, ladder: &nav::Ladder) -> (Option<Vec3>, bool) {
    let pos = bot.movement.origin;
    let (mins, maxs) = bot.movement.hull();
    let extent = (maxs - mins) * 0.5;
    let normal = ladder.normal();
    let t = ((pos.z - ladder.bottom.z) / (ladder.top.z - ladder.bottom.z)).clamp(0.0, 1.0);
    let line = ladder.bottom.lerp(ladder.top, t);
    let lateral = Vec3::Z.cross(normal);
    let actual = world.collision.ladder_normal(pos, mins, maxs);
    let face = ladder_face(world, bot, ladder, line);
    let matches = actual.is_some_and(|n| n.dot(normal) > 0.7 && face.is_some_and(|face| {
        let gap = (pos + (mins + maxs) * 0.5 - face.end).dot(n) - extent.dot(n.abs());
        // Match CollisionWorld::ladder_normal's two-unit attachment reach.
        n.dot(face.normal) > 0.99 && gap.abs() <= collision::LADDER_REACH + collision::DIST_EPSILON
    }))
        && (pos - line).dot(lateral).abs() <= ladder.width * 0.5 + extent.dot(lateral.abs())
        && pos.z + maxs.z >= ladder.bottom.z && pos.z <= ladder.top.z + maxs.z;
    (actual, matches)
}

/// A local input policy, not an off-mesh movement system. NAV selects a mount;
/// the authoritative collision query must confirm its face and authored ribbon.
/// Actual cap/side departure, not an elapsed climb duration, starts dismounting.
#[allow(clippy::too_many_arguments)]
fn follow_ladder(mesh: &NavMesh, world: &World, bot: &Player, brain: &mut Brain,
    input: &mut TickInput, ladder: &nav::Ladder, up: bool, from: u32, to: u32,
    time: f32, turn_rate: f32)
{
    brain.ladder_navigation = true;
    let pos = bot.movement.origin;
    let Some(destination) = mesh.area(to) else { brain.path.clear(); return };
    let normal = ladder.normal();
    let (mins, maxs) = bot.movement.hull();
    let extent = (maxs - mins) * 0.5;
    let t = ((pos.z - ladder.bottom.z) / (ladder.top.z - ladder.bottom.z)).clamp(0.0, 1.0);
    let line = ladder.bottom.lerp(ladder.top, t);
    let lateral = Vec3::Z.cross(normal);
    let offset = pos - line;
    // NAV is a guide to the mount, not an exact collision plane. Probe the
    // authored ribbon and derive hull clearance from the real ladder face.
    // This also keeps a parallel nearby ladder from impersonating this mount.
    let (actual, on_mount) = ladder_contact(world, bot, ladder);
    if brain.ladder_route.as_ref().is_none_or(|r| r.from != from || r.to != to || r.id != ladder.id) {
        brain.ladder_route = Some(LadderRoute { from, to, id: ladder.id, attached: false, exiting: false });
    }
    let route = brain.ladder_route.as_mut().unwrap();
    // Geometric reach alone is not accepted attachment. A descending bot
    // must first move into the face; fresh outward motion intentionally does
    // not catch a ladder in the human movement solver.
    if on_mount && bot.movement.ladder_normal.is_some_and(|held| held.dot(normal) > 0.7) { route.attached = true; }
    if route.attached && ((up && actual.is_none() && pos.z >= ladder.top.z)
        || (!up && pos.z <= destination.z_at(pos.x, pos.y) + world.config.step_size)) {
        route.exiting = true;
    }
    let entry = if up { ladder.bottom } else { ladder.top };
    let mount = ladder_face(world, bot, ladder, entry).map(|face| {
        let clearance = extent.dot(face.normal.abs()) - ((mins + maxs) * 0.5).dot(face.normal)
            + collision::DIST_EPSILON
            // At the top, move fully past floor support while remaining in
            // the existing attachment band. Stopping exactly on hull clearance
            // can leave a fraction of the hull supported indefinitely.
            + if up { 0.0 } else { collision::LADDER_REACH };
        face.end + face.normal * clearance
    });
    let target = if route.exiting {
        destination.center()
    } else if route.attached {
        line
    } else { mount.unwrap_or(entry) };
    let mut direction = target - pos;
    if on_mount && !route.exiting {
        // Level input into the actual face climbs; backward input descends.
        // Center laterally while climbing instead of strafing off its edge.
        direction = normal * (if up || !route.attached { -1.0 } else { 1.0 })
            - lateral * (offset.dot(lateral) / ladder.width.max(1.0)).clamp(-0.5, 0.5);
    } else if route.exiting && !up && on_mount && bot.movement.ladder_normal.is_some()
        && bot.last_input.buttons & buttons::JUMP == 0 {
        // Backward motion on a face descends rather than stepping away. Use
        // the human jump-away command at the bottom; movement owns the impulse.
        input.buttons |= buttons::JUMP;
    } else if ((actual.is_some() && !on_mount) || (!route.attached && mount.is_none())) && !route.exiting {
        // Missing/wrong face: hold and let the existing failure deadline recover.
        direction = Vec3::ZERO;
    }
    let (_, yaw) = look_angles(pos, pos + direction);
    brain.yaw = approach(brain.yaw, yaw, turn_rate);
    brain.pitch = approach(brain.pitch, 0.0, turn_rate);
    let speed = if !route.attached { (direction.truncate().length() * 6.0).min(450.0) } else { 450.0 };
    move_towards(input, brain.yaw, direction, speed);
    let delta = pos - brain.progress_pos;
    let progressed = if on_mount && !route.exiting {
        delta.z * (if up { 1.0 } else { -1.0 }) > 48.0
    } else {
        (target - brain.progress_pos).truncate().length() - (target - pos).truncate().length() > 48.0
    };
    if progressed {
        brain.progress_pos = pos;
        brain.progress_time = time;
    } else if time - brain.progress_time > 2.0 {
        remember_failed_transition(brain, from, to, time);
        brain.path.clear();
        brain.ladder_route = None;
        brain.progress_time = time;
        input.forward_move = 0.0;
        input.side_move = 0.0;
        input.buttons &= !buttons::JUMP;
    }
}

fn remember_failed_transition(brain: &mut Brain, from: u32, to: u32, time: f32) {
    brain.failed_transitions.retain(|e| e.from != from || e.to != to);
    if brain.failed_transitions.len() == MAX_FAILED_TRANSITIONS { brain.failed_transitions.remove(0); }
    brain.failed_transitions.push(FailedTransition { from, to, retry_at: time + FAILED_TRANSITION_SECONDS });
    brain.failed_route = None;
}

 fn reload_cover(mesh: &NavMesh, world: &World, bot: &Player, threat: Vec3) -> Option<(u32, Vec3)> {
     let start = mesh.nearest_area(bot.movement.origin)?.id;
     let mut choices: Vec<_> = mesh.areas.iter().flat_map(|a| a.hiding_spots.iter().map(move |p| (a.id, *p + Vec3::Z * 2.0)))
         .filter(|(_, p)| p.distance(bot.movement.origin) < 360.0 && (p.z - bot.movement.origin.z).abs() < 64.0)
         .collect();
     choices.sort_by(|a,b| a.1.distance_squared(bot.movement.origin).total_cmp(&b.1.distance_squared(bot.movement.origin)));
     choices.into_iter().take(16).find(|(area, p)| {
         !world.visible(threat, *p + Vec3::Z * 52.0)
             && !world.collision.trace_box(*p,*p,movement::STAND_MINS,movement::STAND_MAXS,collision::contents::MASK_PLAYERSOLID).start_solid
             && (*area == start || mesh.find_path(start,*area).is_some())
     })
 }

fn visible_target(world: &World, eye: Vec3, target: &Player) -> Option<Vec3> {
    let head = target.eye() - Vec3::Z * 2.0;
    [head - Vec3::Z * 12.0, head].into_iter().find(|&spot| world.visible(eye, spot) && !world.smoke_occludes(eye,spot))
}

fn defuse_cost(p: &Player, bomb: Vec3) -> f32 {
    p.movement.origin.distance(bomb) / 220.0 + if p.defuse_kit { 5.0 } else { 10.0 }
}

fn move_towards(input: &mut TickInput, yaw: f32, delta: Vec3, speed: f32) {
    let desired = delta.truncate().normalize_or_zero().extend(0.0) * speed;
    let (f, r, _) = movement::angle_vectors(Vec3::new(0.0, yaw, 0.0));
    input.forward_move = desired.dot(f);
    input.side_move = desired.dot(r);
}

fn can_walk(world: &World, bot: &Player, delta: Vec3) -> bool {
    let pos = bot.movement.origin;
    let (mn, mx) = bot.bounds();
    !world.collision.trace_box(pos + Vec3::Z * 2.0, pos + delta + Vec3::Z * 2.0,
        mn - pos, mx - pos, collision::contents::MASK_PLAYERSOLID).hit()
}

/// Bounded acceleration and braking remove instant angular velocity changes
/// without smoothing player mouse input or changing the simulation tick rate.
fn smooth_angle(current: f32, target: f32, velocity: &mut f32, dt: f32, max_speed: f32) -> f32 {
    let error = angle_diff(target, current);
    let acceleration = 1800.0;
    let desired = error.signum() * max_speed.min((2.0 * acceleration * error.abs()).sqrt()).min(error.abs() * 14.0);
    *velocity += (desired - *velocity).clamp(-acceleration * dt, acceleration * dt);
    let step = *velocity * dt;
    if step.signum() == error.signum() && step.abs() >= error.abs() {
        *velocity = 0.0;
        current + error
    } else { current + step }
}

/// Choose reachable cover around an objective, with room for teammates and
/// a visible nearby approach to watch. Only static map knowledge is used.
fn tactical_spot(mesh: &NavMesh, world: &World, bot: &Player, anchor: Vec3, enemy_side: Vec3,
    reservations: &[(PlayerId, Team, Vec3)]) -> Option<(u32, Vec3, Vec3)> {
    let start = mesh.nearest_area(bot.movement.origin)?.id;
    let mut candidates = Vec::new();
    for a in &mesh.areas {
        let center = a.center();
        if center.truncate().distance(anchor.truncate()) > 620.0 || (center.z - anchor.z).abs() > 140.0 { continue; }
        for (i, point) in a.hiding_spots.iter().copied().chain(std::iter::once(center)).enumerate() {
            let p = Vec3::new(point.x, point.y, a.z_at(point.x, point.y) + 2.0);
            let distance = p.truncate().distance(anchor.truncate());
            if distance < 90.0 || !a.contains_xy(p, -12.0) { continue; }
            let tr = world.collision.trace_box(p, p, Vec3::new(-16.0,-16.0,0.0), Vec3::new(16.0,16.0,72.0), collision::contents::MASK_PLAYERSOLID);
            if tr.start_solid { continue; }
            let occupied = reservations.iter().filter(|(id, team, _)| *id != bot.id && *team == bot.team)
                .map(|(_,_,q)| (170.0 - p.distance(*q)).max(0.0) * 5.0).sum::<f32>();
            let allies = world.players.iter().filter(|q| q.alive && q.team == bot.team && q.id != bot.id)
                .map(|q| (110.0 - p.distance(q.movement.origin)).max(0.0) * 3.0).sum::<f32>();
            let cover = if i < a.hiding_spots.len() { 100.0 } else { 0.0 };
            let diversity = ((a.id.wrapping_mul(31).wrapping_add(bot.id.wrapping_mul(127))) % 97) as f32;
            let objective_sight=world.visible(p+Vec3::Z*52.0,anchor+Vec3::Z*12.0);
            if world.bomb.is_some() && !objective_sight {continue}
            if world.fire_at(p) {continue}
            let score = cover + if objective_sight {80.0}else{0.0} + diversity - (distance - 260.0).abs() * 0.3 - occupied - allies;
            candidates.push((score, a.id, p));
        }
    }
    candidates.sort_by(|a,b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, area, pos) in candidates.into_iter().take(24) {
        if area != start && mesh.find_path(start, area).is_none() { continue; }
        let eye = pos + Vec3::Z * 62.0;
        let towards_enemy = (enemy_side - pos).truncate().normalize_or_zero();
        let look = mesh.areas.iter().filter_map(|a| {
            let target = a.center() + Vec3::Z * 52.0;
            let d = target - eye;
            let distance = d.length();
            if !(140.0..900.0).contains(&distance) || !world.visible(eye, target) { return None; }
            Some((d.truncate().normalize_or_zero().dot(towards_enemy) * 300.0 + distance * 0.2, target))
        }).max_by(|a,b| a.0.total_cmp(&b.0)).map(|v| v.1);
        if let Some(look) = look { return Some((area, pos, look)); }
    }
    None
}

#[cfg(test)]
#[path = "bots/route_breakable_tests.rs"]
mod route_breakable_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn look_angles_conventions() {
        let (p, y) = look_angles(Vec3::ZERO, Vec3::new(0.0, 10.0, 0.0));
        assert!((y - 90.0).abs() < 1e-4 && p.abs() < 1e-4);
        let (p, _) = look_angles(Vec3::ZERO, Vec3::new(10.0, 0.0, -10.0));
        assert!((p - 45.0).abs() < 1e-3, "looking down is positive pitch: {p}");
    }

    #[test]
    fn approach_wraps() {
        assert_eq!(approach(350.0, 10.0, 5.0), 355.0);
        assert!((approach(10.0, 350.0, 5.0) - 5.0).abs() < 1e-4);
    }
}

#[cfg(test)] mod regression_tests {
    use super::*;

    /// Own synthetic NAV bytes and collision geometry; no private asset fixture.
    fn ladder_nav() -> NavMesh {
        let mut bytes = Vec::new();
        let word = |bytes: &mut Vec<u8>, value: u32| bytes.extend(value.to_le_bytes());
        for value in [0xFEED_FACE, 16, 1, 0] { word(&mut bytes, value); }
        bytes.push(1); bytes.extend(0u16.to_le_bytes()); bytes.push(1);
        word(&mut bytes, 2);
        for (id, x0, x1, z) in [(1, -160f32, -16f32, 0f32), (2, 16f32, 160f32, 256f32)] {
            word(&mut bytes, id); word(&mut bytes, 0);
            for value in [x0, -64.0, z, x1, 64.0, z, z, z] { word(&mut bytes, value.to_bits()); }
            for _ in 0..4 { word(&mut bytes, 0); }
            bytes.push(0); word(&mut bytes, 0); bytes.extend(0u16.to_le_bytes());
            for active in [id == 1, id == 2] {
                word(&mut bytes, u32::from(active));
                if active { word(&mut bytes, 7); }
            }
            bytes.extend([0; 24]); word(&mut bytes, 0); word(&mut bytes, 0); bytes.push(0);
        }
        word(&mut bytes, 1); word(&mut bytes, 7); word(&mut bytes, 48f32.to_bits());
        for value in [0f32, 0.0, 256.0, 0.0, 0.0, 0.0, 256.0] { word(&mut bytes, value.to_bits()); }
        for value in [3, 2, 0, 0, 0, 1] { word(&mut bytes, value); }
        NavMesh::parse(&bytes).unwrap()
    }

    fn ladder_box(low: Vec3, high: Vec3, contents: i32) -> collision::Convex {
        collision::Convex { planes: vec![
            collision::Plane { normal: Vec3::X, dist: high.x },
            collision::Plane { normal: -Vec3::X, dist: -low.x },
            collision::Plane { normal: Vec3::Y, dist: high.y },
            collision::Plane { normal: -Vec3::Y, dist: -low.y },
            collision::Plane { normal: Vec3::Z, dist: high.z },
            collision::Plane { normal: -Vec3::Z, dist: -low.z },
        ], mins: low, maxs: high, contents, surface: 0 }
    }

    fn ladder_route_fixture(rate: u32, up: bool, ladder: bool, cap: f32) -> (World, Bots, PlayerId) {
        let mut collision = collision::CollisionWorld::new();
        collision.add_convex(ladder_box(Vec3::new(-512., -512., -16.), Vec3::new(512., 512., 0.), collision::contents::SOLID));
        collision.add_convex(ladder_box(Vec3::new(0., -128., 240.), Vec3::new(512., 128., 256.), collision::contents::SOLID));
        if ladder {
            collision.add_convex(ladder_box(Vec3::new(0., -24., 0.), Vec3::new(1., 24., cap),
                collision::contents::LADDER | collision::contents::GRATE));
        }
        collision.finalize();
        let mut world = World::with_catalog(Arc::new(collision), Arc::new(weapons::Catalog { weapons: vec![] }));
        world.set_tick_rate(rate); world.tick = world.ticks(2.0);
        let id = world.add_bot("ladder route", Team::CounterTerrorist);
        let start = if up { Vec3::new(-100., 0., 0.03125) } else { Vec3::new(100., 0., 256.03125) };
        let bot = world.player_mut(id).unwrap();
        bot.movement = movement::PlayerMove::spawn_at(start); bot.movement.on_ground = true;
        world.mode = GameMode::Competitive; world.round.phase = Phase::Live;
        let goal = if up { Vec3::new(120., 0., 256.) } else { Vec3::new(-120., 0., 0.) };
        world.objectives.bomb_sites.push(crate::objectives::BombSite { name: 'A',
            zone: Zone { mins: goal - Vec3::splat(16.), maxs: goal + Vec3::splat(16.) } });
        world.bomb = Some(crate::PlantedBomb { pos: goal, site: 0, planter: id,
            planted_at: 0., explode_at: 200., defuser: None });
        let bots = Bots::with_skill(1.).with_nav(Some(Arc::new(ladder_nav())));
        (world, bots, id)
    }

    // All ladder regressions are intentionally UNRUN in this source-only slot.
    #[test]
    fn parsed_nav_think_and_world_movement_climb_and_dismount_at_both_rates() {
        for rate in [64, 128] {
            for (up, cap) in [(true, 256.), (true, 280.), (false, 256.)] {
                let (mut world, mut bots, id) = ladder_route_fixture(rate, up, true, cap);
                let mut attached = false;
                let mut detached = false;
                let mut max_height = 0f32;
                let mut approach = Vec::new();
                for _ in 0..rate * 8 {
                    let before = world.player(id).unwrap().movement;
                    let inputs = bots.think(&world);
                    assert_eq!(world.player(id).unwrap().movement, before, "think emits input only");
                    assert_eq!(inputs[0].1.buttons & buttons::ATTACK, 0);
                    if !up && before.origin.z > 200. && world.tick % 4 == 0 {
                        let mount = ladder_contact(&world, world.player(id).unwrap(), bots.nav.as_ref().unwrap().ladder(7).unwrap());
                        approach.push((world.tick, before.origin, before.velocity, before.ladder_normal,
                            mount, inputs[0].1.forward_move, inputs[0].1.side_move, inputs[0].1.view_angles,
                            bots.brains[&id].ladder_route.clone()));
                    }
                    world.step_prediction(&inputs);
                    let pm = &world.player(id).unwrap().movement;
                    max_height = max_height.max(pm.origin.z);
                    attached |= pm.ladder_normal == Some(-Vec3::X);
                    detached |= attached && pm.ladder_normal.is_none();
                    let arrived = if up { pm.origin.x > 32. && pm.origin.z > 248. }
                        else { pm.origin.x < -40. && pm.origin.z < 8. };
                    if pm.on_ground && pm.ladder_normal.is_none() && arrived { break; }
                }
                let pm = &world.player(id).unwrap().movement;
                assert!(attached && detached, "{rate} Hz up={up} cap={cap}: {pm:?}; approach={approach:?}");
                assert!(pm.on_ground && pm.ladder_normal.is_none());
                assert!(if up { pm.origin.x > 32. && pm.origin.z > 248. } else { pm.origin.x < -40. && pm.origin.z < 8. }, "{pm:?}");
                if up { assert!(max_height > cap, "actual cap, not NAV top, ends climbing"); }
                assert!(bots.brains[&id].failed_transitions.is_empty());
            }
        }
    }

    #[test]
    fn ladder_missing_contact_wrong_mount_and_incomplete_descent_fail_boundedly() {
        for rate in [64, 128] {
            let (mut world, mut bots, id) = ladder_route_fixture(rate, true, false, 256.);
            for _ in 0..rate * 4 {
                let inputs = bots.think(&world);
                assert_eq!(inputs[0].1.buttons & buttons::JUMP, 0, "no invented mounting jump");
                world.step_prediction(&inputs);
            }
            assert!(world.player(id).unwrap().movement.origin.z < 8.);
            assert!(bots.brains[&id].failed_transitions.iter().any(|e| e.from == 1 && e.to == 2));

            let (mut world, mut bots, id) = ladder_route_fixture(rate, true, true, 256.);
            let mut collision = collision::CollisionWorld::new();
            collision.add_convex(ladder_box(Vec3::new(-80., -24., 0.), Vec3::new(-79., 24., 256.),
                collision::contents::LADDER | collision::contents::GRATE));
            collision.finalize(); world.collision = Arc::new(collision);
            let started = world.time();
            let bot = world.player_mut(id).unwrap();
            bot.movement.origin = Vec3::new(-96.03125, 0., 64.);
            bot.movement.on_ground = false;
            bot.movement.ladder_normal = Some(-Vec3::X);
            // Seed the actual parsed route, while standing on a different,
            // parallel collision ladder. A normal alone is not mount identity.
            bots.brains.insert(id, Brain { path: vec![1, 2], goal_area: Some(2),
                progress_time: started, progress_pos: bot.movement.origin,
                ..Brain::fresh(bot, 0) });
            let inputs = bots.think(&world);
            assert_eq!((inputs[0].1.forward_move, inputs[0].1.side_move), (0., 0.));
            assert!(!bots.brains[&id].ladder_route.as_ref().unwrap().attached);
            world.step_prediction(&inputs);
            assert_eq!(world.player(id).unwrap().movement.origin.z, 64., "wrong mount receives no climbing command");

            let (mut world, mut bots, id) = ladder_route_fixture(rate, false, true, 256.);
            let started = world.time();
            let bot = world.player_mut(id).unwrap();
            bot.movement.origin = Vec3::new(-16.03125, 0., 96.);
            bot.movement.on_ground = false; bot.movement.ladder_normal = Some(-Vec3::X);
            bots.brains.insert(id, Brain { path: vec![2, 1], goal_area: Some(1),
                progress_time: started, progress_pos: bot.movement.origin,
                ..Brain::fresh(bot, 0) });
            let _ = bots.think(&world);
            assert_eq!(bots.brains[&id].path_index, 0, "120-unit floor tolerance cannot finish a ladder descent");
        }
    }

    fn door_route_fixture(rate: u32, locked_leaf: Option<usize>) -> (World, Bots, PlayerId) {
        let hull = mdl::ConvexHull {
            points: vec![Vec3::new(-2., 0., 0.), Vec3::new(2., 0., 0.),
                Vec3::new(2., 48., 0.), Vec3::new(-2., 48., 0.),
                Vec3::new(-2., 0., 96.), Vec3::new(2., 0., 96.),
                Vec3::new(2., 48., 96.), Vec3::new(-2., 48., 96.)],
            triangles: vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7],
                [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5],
                [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]],
        };
        let mut map = bsp::Bsp::default();
        for index in 0..2 {
            let flags = if locked_leaf == Some(index) { 2048 } else { 0 };
            let link = if index == 0 { "targetname route slavename partner" } else { "targetname partner" };
            let doc = kv::parse(&format!(r#"door {{ classname prop_door_rotating
                model models/fixture_leaf.mdl origin "0 {} 0" angles "0 0 0"
                distance 90 speed 180 spawnflags {flags} {link} }}"#, index * 200)).unwrap();
            map.entities.push(doc.block("door").unwrap().clone());
        }
        let mut collision = collision::CollisionWorld::from_bsp(&map,
            &collision::MapCollisionOptions::default(), &|_| Some(vec![hull.clone()]));
        let a = Vec3::new(-1024., -1024., 0.);
        let b = Vec3::new(1024., -1024., 0.);
        let c = Vec3::new(1024., 1024., 0.);
        let d = Vec3::new(-1024., 1024., 0.);
        collision.add_triangle([a, b, c], collision::contents::SOLID, 0);
        collision.add_triangle([a, c, d], collision::contents::SOLID, 0);
        collision.finalize();
        let mut world = World::with_catalog(Arc::new(collision),
            Arc::new(weapons::Catalog { weapons: vec![] }));
        world.set_tick_rate(rate);
        world.tick = world.ticks(2.);
        let id = world.add_bot("door route", Team::CounterTerrorist);
        let bot = world.player_mut(id).unwrap();
        bot.movement = movement::PlayerMove::spawn_at(Vec3::new(-60., 24., 1.));
        bot.movement.on_ground = true;
        world.mode = GameMode::Competitive;
        world.round.phase = Phase::Live;
        world.objectives.bomb_sites.push(crate::objectives::BombSite {
            name: 'A', zone: Zone { mins: Vec3::new(550., -40., 0.), maxs: Vec3::new(650., 88., 128.) },
        });
        world.bomb = Some(crate::PlantedBomb { pos: Vec3::new(600., 24., 1.), site: 0,
            planter: id, planted_at: 0., explode_at: 40., defuser: None });
        let area = |id, min_x, max_x| nav::Area {
            id, flags: 0, nw: Vec3::new(min_x, -40., 0.), se: Vec3::new(max_x, 88., 0.),
            ne_z: 0., sw_z: 0., connections: Default::default(), place: 0, hiding_spots: vec![],
        };
        let mut start = area(1, -128., 0.);
        start.connections[1].push(2);
        let mesh = NavMesh::from_areas(vec![start, area(2, 0., 768.)]);
        let bots = Bots::with_skill(1.).with_nav(Some(Arc::new(mesh)));
        (world, bots, id)
    }

    // Source-only regression: intentionally UNRUN in this implementation slot.
    #[test]
    fn route_door_use_opens_once_and_preserves_locked_open_and_bomb_actions() {
        for rate in [64, 128] {
            let (mut world, mut bots, id) = door_route_fixture(rate, None);
            let blocked = |world: &World| world.collision.trace_box(
                Vec3::new(-60., 24., 1.), Vec3::new(80., 24., 1.),
                movement::STAND_MINS, movement::STAND_MAXS, collision::contents::MASK_PLAYERSOLID).hit();
            assert!(blocked(&world));
            let inputs = bots.think(&world);
            assert_eq!(inputs[0].1.buttons & buttons::USE, 0, "being within use reach alone is insufficient");
            world.step_prediction(&inputs);
            let mut pressed = false;
            for _ in 0..rate {
                let inputs = bots.think(&world);
                assert!(inputs[0].1.forward_move > 0.);
                assert_eq!(world.doors[0].target, 0., "thinking must not mutate doors");
                pressed = inputs[0].1.buttons & buttons::USE != 0;
                world.step_prediction(&inputs);
                if pressed { break; }
            }
            assert!(pressed, "closed door blocking the command at {rate} Hz");
            assert_ne!(world.doors[0].target, 0.);
            assert_ne!(world.doors[1].target, 0., "ordinary use opens the linked leaf too");
            let target = world.doors[0].target;
            for _ in 0..rate * 2 {
                let inputs = bots.think(&world);
                assert_eq!(inputs[0].1.buttons & buttons::USE, 0, "opening/open group must never be toggled closed");
                world.step_prediction(&inputs);
                assert_eq!(world.doors[0].target, target);
                if world.player(id).unwrap().movement.origin.x > 80. { break; }
            }
            assert!(!blocked(&world), "the authoritative door pose clears the route");
            assert!(world.player(id).unwrap().movement.origin.x > 80., "ordinary movement traverses the leaf");
            assert!(bots.brains[&id].failed_transitions.is_empty());

            for locked in [Some(0), Some(1)] {
                let (mut world, mut bots, id) = door_route_fixture(rate, locked);
                world.player_mut(id).unwrap().movement.origin.x = -19.;
                for _ in 0..4 {
                    let inputs = bots.think(&world);
                    assert_eq!(inputs[0].1.buttons & buttons::USE, 0, "no futile use on a locked group");
                    world.step_prediction(&inputs);
                    assert!(world.doors.iter().all(|s| s.angle == 0. && s.target == 0.));
                }
            }
            // The selected leaf is closed but its linked partner is already
            // opening/open/closing. Using it would toggle the whole group.
            for (angle, target) in [(0., 90.), (90., 90.), (45., 0.)] {
                let (mut world, mut bots, id) = door_route_fixture(rate, None);
                world.player_mut(id).unwrap().movement.origin.x = -19.;
                world.doors[1].angle = angle;
                world.doors[1].target = target;
                world.rebuild_door_collision();
                let inputs = bots.think(&world);
                assert_eq!(inputs[0].1.buttons & buttons::USE, 0);
                world.step_prediction(&inputs);
                assert_eq!(world.doors[0].target, 0.);
                assert_eq!(world.doors[1].target, target);
            }
            // The final smoothed player aim, not the route query's aim,
            // decides when a USE edge can actually select this door.
            let (mut world, mut bots, id) = door_route_fixture(rate, None);
            world.player_mut(id).unwrap().movement.origin.x = -19.;
            world.player_mut(id).unwrap().view_angles.y = 90.;
            let inputs = bots.think(&world);
            assert_eq!(inputs[0].1.buttons & buttons::USE, 0);
            assert!(inputs[0].1.view_angles.y > 80.);
            world.step_prediction(&inputs);
            assert_eq!(world.doors[0].target, 0.);

            let (mut world, mut bots, id) = door_route_fixture(rate, None);
            let bot = world.player_mut(id).unwrap();
            bot.movement.origin.x = -19.;
            bot.last_input.buttons = buttons::USE;
            let inputs = bots.think(&world);
            assert_eq!(inputs[0].1.buttons & buttons::USE, 0, "release an old use before a fresh edge");
            world.step_prediction(&inputs);
            assert_eq!(world.doors[0].target, 0.);
            let inputs = bots.think(&world);
            assert_ne!(inputs[0].1.buttons & buttons::USE, 0);
            world.step_prediction(&inputs);
            assert_ne!(world.doors[0].target, 0.);

            let (mut world, mut bots, id) = door_route_fixture(rate, None);
            let bomb = world.bomb.as_mut().unwrap();
            bomb.pos = Vec3::new(-40., 24., 1.);
            bomb.defuser = Some((id, 10.));
            world.player_mut(id).unwrap().last_input.buttons = buttons::USE;
            let inputs = bots.think(&world);
            assert_ne!(inputs[0].1.buttons & buttons::USE, 0, "keep holding defuse");
            assert_eq!((inputs[0].1.forward_move, inputs[0].1.side_move), (0., 0.));
            world.step_prediction(&inputs);
            assert_eq!(world.bomb.unwrap().defuser, Some((id, 10.)));
            assert_eq!(world.doors[0].target, 0.);

            let (mut world, mut bots, id) = door_route_fixture(rate, None);
            world.catalog = Arc::new(weapons::Catalog::from_items_game(r#"items_game { items {
                49 { name weapon_c4 item_class weapon_c4 baseitem 1 }
            } }"#).unwrap());
            world.bomb = None;
            world.objectives.bomb_sites[0].zone = Zone {
                mins: Vec3::new(-128., -40., 0.), maxs: Vec3::new(-20., 88., 128.),
            };
            let started = world.time();
            let c4 = crate::WeaponState::new(Arc::new(world.catalog.get("weapon_c4").unwrap().clone()));
            let bot = world.player_mut(id).unwrap();
            bot.team = Team::Terrorist;
            bot.weapons = vec![c4];
            bot.active = 0;
            bot.plant_start = Some(started);
            let inputs = bots.think(&world);
            assert_eq!(inputs[0].1.weapon_slot, 5);
            assert_ne!(inputs[0].1.buttons & buttons::ATTACK, 0);
            assert_eq!(inputs[0].1.buttons & buttons::USE, 0, "planting retains its input");
            world.step_prediction(&inputs);
            assert_eq!(world.player(id).unwrap().plant_start, Some(started));
            assert_eq!(world.doors[0].target, 0.);
        }
    }

    fn stuck_path_fixture() -> (NavMesh, World, PlayerId, Brain) {
        let area = |id, min_x, max_x| nav::Area {
            id, flags: 0,
            nw: Vec3::new(min_x, -64., 0.),
            se: Vec3::new(max_x, 64., 0.),
            ne_z: 0., sw_z: 0.,
            connections: Default::default(), place: 0, hiding_spots: vec![],
        };
        let mesh = NavMesh::from_areas(vec![area(1, -64., 64.), area(2, 64., 256.)]);
        let mut collision = collision::CollisionWorld::new();
        collision.finalize();
        let mut world = World::with_catalog(Arc::new(collision),
            Arc::new(weapons::Catalog { weapons: vec![] }));
        let id = world.add_bot("blocked jumper", Team::Terrorist);
        world.player_mut(id).unwrap().movement.origin = Vec3::ZERO;
        let brain = Brain { path: vec![1, 2], goal_area: Some(2),
            progress_pos: Vec3::ZERO, progress_time: 0.,
            ..Brain::fresh(world.player(id).unwrap(), 0) };
        (mesh, world, id, brain)
    }

    #[test]
    fn duck_jump_stays_tucked_after_the_portal_stops_requesting_jump() {
        for rate in [64, 128] {
            let (mesh, mut world, id, mut brain) = stuck_path_fixture();
            world.set_tick_rate(rate);
            // A raised destination initially asks for a jump.
            let mut areas = mesh.areas;
            let a = &mut areas[1];
            a.nw.z = 32.; a.se.z = 32.; a.ne_z = 32.; a.sw_z = 32.;
            let mesh = NavMesh::from_areas(areas);
            world.player_mut(id).unwrap().movement.on_ground = true;
            let mut rng = Rng::from_parts(&[17]);
            let mut input = TickInput::default();
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut input, &mut rng, 0., 4., Some(2), None);
            prepare_jump(world.player(id).unwrap(), &mut brain, &mut input);
            assert_ne!(input.buttons & buttons::JUMP, 0);
            world.player_mut(id).unwrap().last_input = input;
            for tick in 1..=world.ticks(0.5) {
                world.tick = tick;
                let p = world.player_mut(id).unwrap();
                p.movement.on_ground = false;
                p.movement.origin.z = 56.;
                let mut input = TickInput::default();
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut input, &mut rng, world.time(), 4., Some(2), None);
                assert_eq!(input.buttons & buttons::JUMP, 0, "above the ledge needs no jump request");
                prepare_jump(world.player(id).unwrap(), &mut brain, &mut input);
                assert_eq!(input.buttons & buttons::JUMP, 0, "release the jump key in flight");
                assert_ne!(input.buttons & buttons::DUCK, 0, "keep the airborne hull stable at {rate} Hz");
                world.player_mut(id).unwrap().last_input = input;
            }
            world.player_mut(id).unwrap().movement.on_ground = true;
            let mut input = TickInput::default();
            prepare_jump(world.player(id).unwrap(), &mut brain, &mut input);
            assert_eq!(input.buttons & buttons::DUCK, 0);
            assert!(!brain.duck_jump);
            // A later fall/recovery request must not inherit that jump.
            world.player_mut(id).unwrap().movement.on_ground = false;
            input.buttons = buttons::JUMP;
            prepare_jump(world.player(id).unwrap(), &mut brain, &mut input);
            assert_eq!(input.buttons & (buttons::JUMP | buttons::DUCK), 0);
            // Explicit crouch navigation still applies during a fall.
            input.buttons = buttons::DUCK;
            prepare_jump(world.player(id).unwrap(), &mut brain, &mut input);
            assert_ne!(input.buttons & buttons::DUCK, 0);
        }
    }

    #[test]
    fn a_stopped_bot_drives_through_the_portal_seam() {
        for rate in [64, 128] {
            for north in [false, true] {
                let (mesh, mut world, id, mut brain) = stuck_path_fixture();
                world.set_tick_rate(rate);
                let mut areas = mesh.areas;
                if north {
                    for a in &mut areas {
                        a.nw = Vec3::new(a.nw.y, a.nw.x, a.nw.z);
                        a.se = Vec3::new(a.se.y, a.se.x, a.se.z);
                    }
                }
                let mesh = NavMesh::from_areas(areas);
                let axis = if north { Vec3::Y } else { Vec3::X };
                let p = world.player_mut(id).unwrap();
                p.movement.origin = axis * 64.;
                p.movement.velocity = Vec3::ZERO;
                p.movement.on_ground = true;
                let mut input = TickInput::default();
                let mut rng = Rng::from_parts(&[17]);
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut input, &mut rng, 0., 4., Some(2), None);
                let (f, r, _) = movement::angle_vectors(Vec3::new(0., brain.yaw, 0.));
                assert!((f * input.forward_move + r * input.side_move).dot(axis) > 449.);
                assert_eq!(brain.path_index, 0, "the seam itself is not arrival");
                world.player_mut(id).unwrap().movement.origin = axis * 84.;
                world.tick += 1;
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut TickInput::default(), &mut rng, world.time(), 4., Some(2), None);
                assert_eq!(brain.path_index, 1);
                world.tick += 1;
                assert!(follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut TickInput::default(), &mut rng, world.time(), 4., Some(2), None));
            }
        }
    }

    #[test]
    fn being_below_the_goal_floor_does_not_complete_its_crossing() {
        for rate in [64, 128] {
            let (mesh, mut world, id, mut brain) = stuck_path_fixture();
            world.set_tick_rate(rate);
            world.player_mut(id).unwrap().movement.origin = Vec3::new(96., 0., -32.);
            let mut rng = Rng::from_parts(&[17]);
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut TickInput::default(), &mut rng, 0., 4., Some(2), None);
            assert_eq!(brain.path_index, 0, "feet below a ledge have not crossed it");
            // Even if the closest area is the objective, replanning must not
            // report arrival while under it or outside its XY bounds.
            for pos in [Vec3::new(96., 0., -32.), Vec3::X * 300.] {
                brain.path.clear();
                world.player_mut(id).unwrap().movement.origin = pos;
                world.tick += 1;
                assert!(!follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut TickInput::default(), &mut rng, world.time(), 4., Some(2), None));
            }
            // Preserve advancement while jumping above a valid floor.
            brain.path = vec![1, 2]; brain.path_index = 0;
            world.player_mut(id).unwrap().movement.origin = Vec3::new(96., 0., 56.);
            world.player_mut(id).unwrap().movement.on_ground = false;
            world.tick += 1;
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut TickInput::default(), &mut rng, world.time(), 4., Some(2), None);
            assert_eq!(brain.path_index, 1);
        }
    }

    #[test]
    fn jumping_in_place_does_not_keep_a_blocked_route_alive() {
        for rate in [64, 128] {
            let (mesh, mut world, id, mut brain) = stuck_path_fixture();
            world.set_tick_rate(rate);
            let mut rng = Rng::from_parts(&[17]);
            for tick in 1..=world.ticks(2.1) {
                world.tick = tick;
                let now = world.time();
                let p = world.player_mut(id).unwrap();
                // Repeated jumps with no horizontal progress: the previous
                // 3D distance test refreshed the timer at every 56-unit peak.
                p.movement.origin.z = if tick % (rate as u64 / 2) < rate as u64 / 4 { 56. } else { 0. };
                p.movement.on_ground = p.movement.origin.z == 0.;
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut TickInput::default(), &mut rng, now, 4., Some(2), None);
                if now <= 2. { assert!(!brain.path.is_empty()); }
                else { assert!(brain.path.is_empty(), "blocked route must expire at {rate} Hz"); break; }
            }
        }
    }

    #[test]
    fn landed_height_change_still_counts_as_navigation_progress() {
        let (mesh, mut world, id, mut brain) = stuck_path_fixture();
        let p = world.player_mut(id).unwrap();
        p.movement.origin.z = 56.;
        p.movement.on_ground = true;
        follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
            &mut TickInput::default(), &mut Rng::from_parts(&[17]), 1.9, 4., Some(2), None);
        assert_eq!(brain.progress_time, 1.9);
        assert_eq!(brain.progress_pos.z, 56.);
        assert!(!brain.path.is_empty());
    }

    fn blocked_shortcut_fixture(detour: bool) -> (NavMesh, World, PlayerId, Brain) {
        let area = |id, x0, y0, x1, y1, connections| nav::Area {
            id, flags: 0, nw: Vec3::new(x0, y0, 0.), se: Vec3::new(x1, y1, 0.),
            ne_z: 0., sw_z: 0., connections, place: 0, hiding_spots: vec![],
        };
        let mesh = NavMesh::from_areas(vec![
            area(1, -64., -64., 64., 64., [vec![], vec![2], if detour { vec![3] } else { vec![] }, vec![]]),
            area(2, 64., -64., 128., 64., [vec![], vec![4], vec![], vec![1]]),
            area(3, -64., 64., 64., 128., [vec![], vec![4], vec![], vec![]]),
            area(4, 128., -64., 192., 64., [vec![], vec![], vec![], vec![]]),
        ]);
        let (_, mut world, id, mut brain) = stuck_path_fixture();
        world.player_mut(id).unwrap().movement.on_ground = true;
        brain.goal_area = Some(4);
        brain.path = mesh.find_path(1, 4).unwrap();
        assert_eq!(brain.path, [1, 2, 4]);
        (mesh, world, id, brain)
    }

    #[test]
    fn navigation_resumes_from_the_area_reached_during_combat() {
        let area = |id, x0, y0, x1, y1, connections| nav::Area {
            id, flags: 0, nw: Vec3::new(x0, y0, 0.), se: Vec3::new(x1, y1, 0.),
            ne_z: 0., sw_z: 0., connections, place: 0, hiding_spots: vec![],
        };
        // Two northern rooms lead east, then south to the goal. Combat may
        // instead strafe the bot into the southwest room, whose exit is east.
        let mesh = NavMesh::from_areas(vec![
            area(1, -128., -128., 0., 0., [vec![], vec![2], vec![], vec![]]),
            area(2, 0., -128., 128., 0., [vec![], vec![], vec![4], vec![]]),
            area(3, -128., 0., 0., 128., [vec![], vec![4], vec![], vec![]]),
            area(4, 0., 0., 128., 128., Default::default()),
        ]);
        for rate in [64, 128] {
            for resumed_pos in [
                Vec3::new(-64., -64., 0.), // unchanged: reuse the cached path
                Vec3::new(-1., -64., 0.), // near a seam: the arrival inset is irrelevant
                Vec3::new(64., -64., 0.), // on the next area: normal advancement still works
                Vec3::new(-64., 64., 0.), // off-route: replan before moving
            ] {
                let (_, mut world, id, mut brain) = stuck_path_fixture();
                world.set_tick_rate(rate);
                let start = Vec3::new(-64., -64., 0.);
                let bot = world.player_mut(id).unwrap();
                bot.movement.origin = start;
                bot.movement.on_ground = true;
                brain.path = mesh.find_path(1, 4).unwrap();
                brain.goal_area = Some(4);
                brain.progress_pos = start;
                let mut rng = Rng::from_parts(&[17]);
                world.tick = 1;
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
                world.tick = world.ticks(3.0);
                world.player_mut(id).unwrap().movement.origin = resumed_pos;
                let mut input = TickInput::default();
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut input, &mut rng, world.time(), 4., Some(4), None);
                let off_route = resumed_pos.y > 0.;
                assert_eq!(brain.path, if off_route { vec![3, 4] } else { vec![1, 2, 4] },
                    "resume at {resumed_pos:?}, {rate} Hz");
                assert_eq!(brain.path_index, if resumed_pos.x > 0. { 1 } else { 0 });
                assert_eq!(brain.route_searches, if off_route { 1 } else { 0 });
                assert!(brain.failed_transitions.is_empty(), "combat is not a failed crossing");
                assert_eq!(input.buttons & buttons::JUMP, 0, "combat is not stuck time");
                assert_eq!(brain.progress_time, world.time());
                if off_route {
                    let (forward, right, _) = movement::angle_vectors(Vec3::new(0., brain.yaw, 0.));
                    let wish = forward * input.forward_move + right * input.side_move;
                    assert!(wish.x > 0. && wish.y.abs() < 0.001,
                        "use the current room's east exit, not the old room's portal: {wish:?}");
                }
            }
        }
    }

    #[test]
    fn continuous_navigation_keeps_a_route_across_gaps_and_airborne_motion() {
        for rate in [64, 128] {
            for (pos, grounded) in [
                (Vec3::new(80., 0., 0.), true), // uncovered XY gap, even with ground contact
                (Vec3::new(80., 0., 56.), false), // airborne across the same gap
                (Vec3::new(-32., 96., 56.), false), // airborne above another approach
                (Vec3::new(0., 0., 128.), false), // above the route's height tolerance
            ] {
                let (mesh, mut world, id, mut brain) = blocked_shortcut_fixture(true);
                let mut areas = mesh.areas;
                // Keep the authored 1 -> 2 link while separating its XY bounds.
                areas.iter_mut().find(|a| a.id == 2).unwrap().nw.x = 96.;
                let mesh = NavMesh::from_areas(areas);
                assert_eq!(mesh.find_path(1, 4), Some(vec![1, 2, 4]));
                world.set_tick_rate(rate);
                let mut rng = Rng::from_parts(&[17]);
                world.tick = 1;
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
                world.tick += 1;
                let bot = world.player_mut(id).unwrap();
                bot.movement.origin = pos;
                bot.movement.on_ground = grounded;
                let mut input = TickInput::default();
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut input, &mut rng, world.time(), 4., Some(4), None);
                assert_eq!(brain.path, [1, 2, 4], "keep linked traversal at {pos:?}, {rate} Hz");
                assert_eq!(brain.path_index, 0);
                assert_eq!(brain.route_searches, 0, "absence of route membership is not displacement");
                assert!(brain.failed_transitions.is_empty());
                assert!(input.forward_move != 0. || input.side_move != 0., "keep steering on the route");
            }
        }
    }

    #[test]
    fn continuous_navigation_replans_before_blacklisting_a_displaced_crossing() {
        for rate in [64, 128] {
            let (mesh, mut world, id, mut brain) = blocked_shortcut_fixture(true);
            world.set_tick_rate(rate);
            let mut rng = Rng::from_parts(&[17]);
            // Reach the recovery deadline with consecutive navigation calls.
            // The blocked portal remains valid while still in its approach.
            for tick in 1..=world.ticks(2.0) {
                world.tick = tick;
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
            }
            assert_eq!(brain.path, [1, 2, 4]);
            assert!(brain.failed_transitions.is_empty());
            // Collision/knockback can move the bot into the other approach
            // without pausing navigation. The old 1 -> 2 portal must not be
            // steered toward or blamed when its recovery clock expires.
            world.tick += 1;
            world.player_mut(id).unwrap().movement.origin = Vec3::new(-32., 96., 0.);
            let mut input = TickInput::default();
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut input, &mut rng, world.time(), 4., Some(4), None);
            assert_eq!(brain.path, [3, 4], "replan immediately at {rate} Hz");
            assert_eq!(brain.path_index, 0);
            assert_eq!(brain.route_searches, 1);
            assert!(brain.failed_transitions.is_empty(), "displacement is not a failed crossing");
            assert_eq!(brain.progress_time, world.time());
            assert_eq!(input.buttons & buttons::JUMP, 0, "new approach has no stuck time");
            assert!(input.forward_move != 0. || input.side_move != 0., "steer on the replacement route");
            world.tick += 1;
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
            assert_eq!(brain.path, [3, 4]);
            assert_eq!(brain.route_searches, 1, "reuse the replacement route");
        }
    }

    #[test]
    fn blocked_jump_and_sideways_loop_replans_via_another_approach() {
        for rate in [64, 128] {
            let (mut mesh, mut world, id, mut brain) = blocked_shortcut_fixture(true);
            world.set_tick_rate(rate);
            let mut rng = Rng::from_parts(&[17]);
            for tick in 1..=world.ticks(2.1) {
                world.tick = tick;
                let p = world.player_mut(id).unwrap();
                // Motion is large enough to defeat displacement-only recovery,
                // but it never gets closer to the blocked eastern crossing.
                p.movement.origin.y = if tick % (rate as u64) < rate as u64 / 2 { 56. } else { -56. };
                p.movement.origin.z = if tick % (rate as u64 / 2) < rate as u64 / 4 { 56. } else { 0. };
                p.movement.on_ground = p.movement.origin.z == 0.;
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
                if brain.path.is_empty() { break; }
            }
            assert!(brain.path.is_empty(), "corner loop must expire at {rate} Hz");
            assert_eq!(brain.failed_transitions.len(), 1);
            let edge = &brain.failed_transitions[0];
            assert_eq!((edge.from, edge.to), (1, 2));
            world.tick += 1;
            let mut input = TickInput::default();
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut input, &mut rng, world.time(), 4., Some(4), None);
            assert_eq!(brain.path, [1, 3, 4], "replanning must not immediately retry the failed crossing");
            assert_eq!(input.buttons & buttons::JUMP, 0);
            // Only 1 -> 2 was rejected, not area 2 or the reverse crossing.
            assert_eq!(mesh.find_path_with_traversal(2, 1, |_| 0.,
                |from, to| !(from == 1 && to == 2)), Some(vec![2, 1]));
            // Recovery must not override the same explicit NO_JUMP flag
            // already honored by normal portal-height jumping.
            mesh.areas.iter_mut().find(|a| a.id == 3).unwrap().flags |= nav::attr::NO_JUMP;
            let p = world.player_mut(id).unwrap();
            p.movement.origin.z = 0.;
            p.movement.on_ground = true;
            let end = world.tick + world.ticks(1.0);
            for tick in world.tick + 1..=end {
                world.tick = tick;
                let mut input = TickInput::default();
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut input, &mut rng, world.time(), 4., Some(4), None);
                assert_eq!(input.buttons & buttons::JUMP, 0);
            }
            assert!(world.time() - brain.progress_time > 0.75, "exercise stuck recovery, not only normal movement");
        }
    }

    #[test]
    fn failed_crossing_falls_back_and_expires_without_blocking_goal_area() {
        for rate in [64, 128] {
            let (mesh, mut world, id, mut brain) = blocked_shortcut_fixture(false);
            world.set_tick_rate(rate);
            let mut rng = Rng::from_parts(&[17]);
            world.tick = 1;
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
            // Combat can suspend an existing route: that pause is not an
            // actual attempt to cross its next portal.
            world.tick = world.ticks(3.0);
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
            assert!(brain.failed_transitions.is_empty());
            assert_eq!(brain.path, [1, 2, 4]);
            let end = world.tick + world.ticks(2.1);
            for tick in world.tick + 1..=end {
                world.tick = tick;
                follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                    &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
                if !brain.failed_transitions.is_empty() { break; }
            }
            let retry_at = brain.failed_transitions[0].retry_at;
            world.tick += 1;
            let mut input = TickInput::default();
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut input, &mut rng, world.time(), 4., Some(4), None);
            assert!(brain.path.is_empty(), "no alternate crossing must use existing wander fallback");
            assert_eq!(input.forward_move, 450.);
            assert_eq!(input.buttons & buttons::JUMP, 0);
            assert_eq!(brain.failed_route, Some((1, 4)));
            let searches = brain.route_searches;
            world.tick += 1;
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
            assert_eq!(brain.route_searches, searches, "same disconnected query must not repeat A*");
            // Entering the excluded edge's destination through another
            // approach remains valid, and changes the query immediately.
            world.player_mut(id).unwrap().movement.origin = Vec3::X * 96.;
            world.tick += 1;
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
            assert_eq!(brain.route_searches, searches + 1);
            assert_eq!(brain.path, [2, 4]);
            assert_eq!(brain.failed_route, None);
            world.player_mut(id).unwrap().movement.origin = Vec3::ZERO;
            brain.path.clear();
            world.tick += 1;
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
            assert_eq!(brain.failed_route, Some((1, 4)));
            let searches = brain.route_searches;
            world.tick = world.ticks(retry_at + 0.01);
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut TickInput::default(), &mut rng, world.time(), 4., Some(4), None);
            assert!(brain.failed_transitions.is_empty());
            assert_eq!(brain.route_searches, searches + 1, "expiry invalidates the failed query");
            assert_eq!(brain.path, [1, 2, 4], "a temporary obstruction can be retried after expiry");
            // An unreachable point within the current area is not an edge.
            world.tick += 1;
            follow_path(&mesh, &world, world.player(id).unwrap(), &mut brain,
                &mut TickInput::default(), &mut rng, world.time(), 4., Some(1), Some(Vec3::X * 40.));
            assert!(brain.failed_transitions.is_empty());
        }
    }
    #[test]
    fn removed_bot_bookkeeping_stays_bounded_through_replacement_churn() {
        for rate in [64, 128] {
            let mut collision = collision::CollisionWorld::new();
            collision.finalize();
            let mut world = World::with_catalog(
                Arc::new(collision),
                Arc::new(weapons::Catalog { weapons: Vec::new() }),
            );
            world.set_tick_rate(rate);
            let human = world.add_player("remaining human", Team::CounterTerrorist);
            let mut previous = human;
            for _ in 0..128 {
                let bot = world.add_bot("replacement", Team::Terrorist);
                assert!(bot > previous, "replacement must receive a fresh actor id");
                previous = bot;
                world.step(&[]);
                assert_eq!(world.bots.brains.len(), 1);
                // Team roles may be assigned at a round boundary and contacts
                // can arrive during combat, independently of brain creation.
                world.bots.team.roles.insert(bot, crate::tactics::Role::Entry);
                world.bots.team.reserve.insert(bot, 1000);
                let now = world.time();
                world.bots.team.report(crate::tactics::Contact {
                    team: Team::CounterTerrorist,
                    enemy: bot,
                    pos: Vec3::X,
                    observed: now,
                    available: now,
                    visual: true,
                });
                world.bots.team.defuser = Some(bot);
                assert!(world.remove_player(bot));
                assert!(world.bots.brains.is_empty());
                assert!(!world.bots.team.roles.contains_key(&bot));
                assert!(!world.bots.team.reserve.contains_key(&bot));
                assert!(world.bots.team.contacts.iter().all(|contact| world.player(contact.enemy).is_some()));
                assert!(world.bots.team.contacts.len() <= world.players.len() * 2);
                assert!(world.bots.team.roles.len() <= world.players.len());
                assert!(world.bots.team.reserve.len() <= world.players.len());
                assert!(world.bots.team.defuser.is_none());
                assert_eq!(world.players.len(), 1);
                assert!(world.player(human).is_some());
                assert!(!world.remove_player(bot));
            }
        }
    }

    #[test]
    fn bot_removal_preserves_surviving_brain_and_team_plan() {
        let mut collision = collision::CollisionWorld::new();
        collision.finalize();
        let mut world = World::with_catalog(
            Arc::new(collision),
            Arc::new(weapons::Catalog { weapons: Vec::new() }),
        );
        let removed = world.add_bot("departing", Team::Terrorist);
        let survivor = world.add_bot("remaining", Team::CounterTerrorist);
        world.step(&[]);
        world.bots.brains.get_mut(&survivor).unwrap().next_utility = 123.;
        let now = world.time();
        world.bots.team.report(crate::tactics::Contact {
            team: Team::Terrorist,
            enemy: survivor,
            pos: Vec3::Y,
            observed: now,
            available: now,
            visual: true,
        });
        world.bots.team.defuser = Some(survivor);
        let brain = format!("{:?}", world.bots.brains[&survivor]);
        let plans = format!("{:?}", world.bots.team.plans);
        let skill = world.bots.skill;
        assert!(world.remove_player(removed));
        assert_eq!(world.bots.brains.len(), 1);
        assert_eq!(format!("{:?}", world.bots.brains[&survivor]), brain);
        assert_eq!(format!("{:?}", world.bots.team.plans), plans);
        assert_eq!(world.bots.team.defuser, Some(survivor));
        assert_eq!(world.bots.team.contacts.len(), 1);
        assert_eq!(world.bots.team.contacts[0].enemy, survivor);
        assert!(world.bots.team.roles.contains_key(&survivor));
        assert!(world.bots.team.reserve.contains_key(&survivor));
        assert_eq!(world.bots.skill, skill);
    }

    #[test]
    fn exhausted_combat_gun_selects_usable_inventory_without_reverting() {
        for rate in [64, 128] {
            for (active, pistol_clip, pistol_reserve, expected_slot) in [
                (0, 12, 0, 2),
                (0, 0, 24, 2), // A reloadable pistol is still usable.
                (0, 0, 0, 3),
                (1, 0, 0, 3), // The active pistol can also exhaust its reserve.
            ] {
                let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
                    "7" { "name" "weapon_ak47" "baseitem" "1" "attributes" {
                        "primary clip size" "30" "is full auto" "1"
                    } }
                    "4" { "name" "weapon_glock" "baseitem" "1" "attributes" {
                        "primary clip size" "20"
                    } }
                    "42" { "name" "weapon_knife" "baseitem" "1" }
                } }"#).unwrap();
                let mut collision = collision::CollisionWorld::new();
                collision.finalize();
                let mut world = World::with_catalog(Arc::new(collision), Arc::new(catalog));
                world.set_tick_rate(rate);
                world.tick = world.ticks(2.);
                let id = world.add_bot("exhausted burst", Team::Terrorist);
                let enemy = world.add_player("visible target", Team::CounterTerrorist);
                world.player_mut(enemy).unwrap().movement.origin = Vec3::X * 100.;
                let mut inventory: Vec<_> = ["weapon_ak47", "weapon_glock", "weapon_knife"]
                    .map(|name| crate::WeaponState::new(Arc::new(world.catalog.get(name).unwrap().clone())))
                    .into_iter().collect();
                inventory[0].clip = 0;
                inventory[0].reserve = 0;
                inventory[1].clip = pistol_clip;
                inventory[1].reserve = pistol_reserve;
                let bot = world.player_mut(id).unwrap();
                bot.weapons = inventory;
                bot.active = active;
                bot.movement.origin = Vec3::ZERO;
                bot.movement.on_ground = true;
                bot.last_input.buttons = buttons::ATTACK;
                let (pitch, yaw) = look_angles(bot.eye(), Vec3::new(100., 0., 50.));
                let mut brain = Brain::fresh(bot, 0);
                brain.target = Some(enemy);
                brain.pitch = pitch;
                brain.yaw = yaw;
                brain.burst_left = 2;
                let mut bots = Bots::with_skill(1.);
                bots.brains.insert(id, brain);
                let inputs = bots.think(&world);
                let input = &inputs.iter().find(|(actor, _)| *actor == id).unwrap().1;
                assert!(bots.brains[&id].visible);
                assert_eq!(input.weapon_slot, expected_slot);
                world.step_prediction(&inputs);
                let bot = world.player(id).unwrap();
                assert_eq!(crate::weapon::slot(&bot.active_weapon().unwrap().def), expected_slot);
                assert_eq!((bot.weapons[0].clip, bot.weapons[0].reserve), (0, 0));
                assert_eq!((bot.weapons[1].clip, bot.weapons[1].reserve), (pistol_clip, pistol_reserve));
                // Repeated decisions must retain the fallback through deploy
                // and after readiness, rather than select the empty primary.
                assert_eq!(gun_slot(bot, world.time()), 0);
                assert_eq!(gun_slot(bot, world.time() + 2.), expected_slot);
            }
        }
    }

    #[test]
    fn exhausted_melee_fallback_pursues_without_distant_slashes() {
        for rate in [64, 128] {
            for with_nav in [false, true] {
                let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
                    "7" { "name" "weapon_ak47" "baseitem" "1" "attributes" {
                        "primary clip size" "30" "is full auto" "1"
                    } }
                    "42" { "name" "weapon_knife" "baseitem" "1" }
                } }"#).unwrap();
                let mut collision = collision::CollisionWorld::new();
                collision.finalize();
                let mut world = World::with_catalog(Arc::new(collision), Arc::new(catalog));
                world.set_tick_rate(rate);
                world.tick = world.ticks(2.);
                let id = world.add_bot("melee fallback", Team::Terrorist);
                let enemy = world.add_player("visible target", Team::CounterTerrorist);
                world.player_mut(enemy).unwrap().movement.origin = Vec3::X * 200.;
                let inventory = ["weapon_ak47", "weapon_knife"]
                    .map(|name| crate::WeaponState::new(Arc::new(world.catalog.get(name).unwrap().clone())))
                    .into_iter().collect();
                let bot = world.player_mut(id).unwrap();
                bot.weapons = inventory;
                bot.active = 0;
                bot.weapons[0].clip = 0;
                bot.weapons[0].reserve = 0;
                bot.movement.origin = Vec3::ZERO;
                bot.movement.on_ground = true;
                let mut brain = Brain::fresh(bot, 0);
                brain.target = Some(enemy);
                (brain.pitch, brain.yaw) = look_angles(bot.eye(), Vec3::new(200., 0., 50.));
                brain.burst_left = 2;
                let mut bots = Bots::with_skill(1.);
                if with_nav {
                    let area = |id, nw, se| nav::Area {
                        id, nw, se, flags: 0, ne_z: 0., sw_z: 0.,
                        connections: Default::default(), place: 0, hiding_spots: vec![],
                    };
                    // The nav route bends north before reaching the enemy
                    // east of us. Pursuit must preserve that world direction
                    // while the combat aim continues to face the enemy.
                    let mut start = area(1, Vec3::new(-64., -64., 0.), Vec3::new(64., 64., 0.));
                    let mut bend = area(2, Vec3::new(-64., 64., 0.), Vec3::new(256., 192., 0.));
                    let goal = area(3, Vec3::new(64., -64., 0.), Vec3::new(256., 64., 0.));
                    start.connections[2].push(2);
                    bend.connections[0].push(3);
                    bots.nav = Some(Arc::new(NavMesh::from_areas(vec![start, bend, goal])));
                }
                bots.brains.insert(id, brain);
                let inputs = bots.think(&world);
                assert_eq!(inputs[0].1.weapon_slot, 3);
                world.step_prediction(&inputs);
                assert!(world.player(id).unwrap().active_weapon().unwrap().is_melee());
                // Finish ordinary fallback deployment before testing pursuit.
                world.tick += world.ticks(2.);
                let now = world.time();
                let bot = world.player_mut(id).unwrap();
                bot.movement.origin = Vec3::ZERO;
                bot.movement.on_ground = true;
                bots.brains.get_mut(&id).unwrap().progress_time = now;
                let inputs = bots.think(&world);
                let input = inputs[0].1;
                assert!(bots.brains[&id].visible);
                assert_eq!(input.buttons & (buttons::ATTACK | buttons::ATTACK2), 0);
                let (forward, right, _) = movement::angle_vectors(input.view_angles.with_x(0.));
                let wish = forward * input.forward_move + right * input.side_move;
                if with_nav {
                    assert_eq!(bots.brains[&id].path, [1, 2, 3]);
                    assert!(wish.y > 400., "follow the portal, not a direct charge east");
                    assert!(angle_diff(input.view_angles.y, 0.).abs() < 1.);
                } else {
                    assert!(wish.x > 400.);
                }
                world.step_prediction(&inputs);
                let bot = world.player(id).unwrap();
                assert!(if with_nav { bot.movement.origin.y > 0. } else { bot.movement.origin.x > 0. });
                assert!(bot.active_weapon().unwrap().last_melee_attack.is_none());
                assert_eq!((bot.weapons[0].clip, bot.weapons[0].reserve), (0, 0));

                // A moving knife can hit a hull even when its centre is
                // farther than MELEE_RANGE. Gun bursts/accuracy gates must
                // neither suppress the slash nor determine its damage.
                for grounded in [true, false] {
                    world.player_mut(enemy).unwrap().movement.origin = Vec3::X * 60.;
                    let bot = world.player_mut(id).unwrap();
                    bot.movement.origin = Vec3::ZERO;
                    bot.movement.velocity = Vec3::X * 200.;
                    bot.movement.on_ground = grounded;
                    let target = world.player(enemy).unwrap();
                    let (pitch, yaw) = look_angles(world.player(id).unwrap().eye(), target.eye() - Vec3::Z * 14.);
                    let brain = bots.brains.get_mut(&id).unwrap();
                    brain.pitch = pitch; brain.yaw = yaw;
                    brain.turn_velocity = glam::Vec2::ZERO;
                    brain.burst_left = 0;
                    brain.next_burst = world.time() + 10.;
                    let inputs = bots.think(&world);
                    assert_ne!(inputs[0].1.buttons & buttons::ATTACK, 0);
                    assert_eq!(inputs[0].1.weapon_slot, if grounded { 3 } else { 0 });
                    assert_eq!((inputs[0].1.forward_move, inputs[0].1.side_move), (0., 0.));
                    let health = world.player(enemy).unwrap().health;
                    let ready = world.player(id).unwrap().active_weapon().unwrap().next_attack;
                    world.step_prediction(&inputs);
                    if grounded {
                        assert!(world.player(enemy).unwrap().health < health, "ordinary melee authority must apply the hit");
                    } else {
                        assert_eq!(world.player(enemy).unwrap().health, health, "pending melee cooldown must still apply");
                        assert_eq!(world.player(id).unwrap().active_weapon().unwrap().next_attack, ready);
                    }
                }
            }
        }
    }

    #[test]
    fn exhausted_fallback_preserves_pending_actions_and_reloadable_primary() {
        let (_, mut world, id, _) = stuck_path_fixture();
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            "7" { "name" "weapon_ak47" "baseitem" "1" "attributes" { "primary clip size" "30" } }
            "42" { "name" "weapon_knife" "baseitem" "1" }
        } }"#).unwrap();
        let bot = world.player_mut(id).unwrap();
        bot.weapons = ["weapon_ak47", "weapon_knife"]
            .map(|name| crate::WeaponState::new(Arc::new(catalog.get(name).unwrap().clone())))
            .into_iter().collect();
        bot.active = 0;
        bot.weapons[0].clip = 0;
        bot.weapons[0].reserve = 1;
        assert_eq!(gun_slot(bot, 2.), 1, "R6 reload must retain the primary");
        bot.weapons[0].reserve = 0;
        assert_eq!(gun_slot(bot, 2.), 3);
        bot.weapons[0].reload_end = Some(3.);
        assert_eq!(gun_slot(bot, 2.), 0);
        bot.weapons[0].reload_end = None;
        bot.weapons[0].next_attack = 3.;
        assert_eq!(gun_slot(bot, 2.), 0, "do not interrupt deployment/attack lockout");
        assert_eq!(gun_slot(bot, 3.), 3);
        bot.weapons[0].burst = Some(crate::weapon::BurstState { remaining: 1, next_shot: 3. });
        assert_eq!(gun_slot(bot, 3.), 0);
        bot.weapons[0].burst = None;
        bot.weapons[0].suppressor_transition = Some(crate::weapon::SuppressorTransition {
            started_tick: 0, attached: false,
        });
        assert_eq!(gun_slot(bot, 3.), 0);
        bot.weapons[0].suppressor_transition = None;
        bot.weapons.pop();
        assert_eq!(gun_slot(bot, 3.), 1, "no fallback without an owned usable weapon");
    }

    #[test]
    fn grounded_empty_combat_burst_reloads_and_moves_to_cover() {
        for rate in [64, 128] {
            for (grounded, clip, reserve, should_reload) in [
                (true, 0, 60, true),
                (true, 1, 60, false),
                (true, 0, 0, false),
                (false, 0, 60, false),
            ] {
                let mut collision = collision::CollisionWorld::new();
                let surf = collision.surface_id("wall");
                // The target is visible east of the bot; the northern
                // hiding spot is behind this wall from the target's eye.
                let a = Vec3::new(50., 40., 0.);
                let b = Vec3::new(50., 300., 0.);
                let c = Vec3::new(50., 300., 200.);
                let d = Vec3::new(50., 40., 200.);
                collision.add_triangle([a, b, c], collision::contents::SOLID, surf);
                collision.add_triangle([a, c, d], collision::contents::SOLID, surf);
                collision.finalize();
                let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
                    "7" { "name" "weapon_ak47" "baseitem" "1" "attributes" {
                        "primary clip size" "30" "is full auto" "1"
                    } }
                } }"#).unwrap();
                let mut world = World::with_catalog(Arc::new(collision), Arc::new(catalog));
                world.set_tick_rate(rate);
                world.tick = world.ticks(2.);
                let id = world.add_bot("empty burst", Team::Terrorist);
                let enemy = world.add_player("visible target", Team::CounterTerrorist);
                world.player_mut(enemy).unwrap().movement.origin = Vec3::X * 100.;
                let mut weapon = crate::WeaponState::new(Arc::new(world.catalog.get("weapon_ak47").unwrap().clone()));
                weapon.clip = clip;
                weapon.reserve = reserve;
                let bot = world.player_mut(id).unwrap();
                bot.weapons = vec![weapon];
                bot.active = 0;
                bot.movement.origin = Vec3::ZERO;
                bot.movement.on_ground = grounded;
                bot.last_input.buttons = buttons::ATTACK;
                let target_pos = world.player(enemy).unwrap().eye() - Vec3::Z * 14.;
                let (pitch, yaw) = look_angles(world.player(id).unwrap().eye(), target_pos);
                let mut brain = Brain::fresh(world.player(id).unwrap(), 0);
                brain.target = Some(enemy);
                brain.pitch = pitch;
                brain.yaw = yaw;
                brain.progress_time = world.time();
                brain.burst_left = 2; // Magazine emptied before the authored burst ended.
                let area = |id, min_y, max_y| nav::Area {
                    id, flags: 0,
                    nw: Vec3::new(-64., min_y, 0.), se: Vec3::new(32., max_y, 0.),
                    ne_z: 0., sw_z: 0., connections: Default::default(),
                    place: 0, hiding_spots: vec![],
                };
                let mut start = area(1, -64., 64.);
                start.connections[0].push(2);
                let mut cover = area(2, 64., 192.);
                cover.hiding_spots.push(Vec3::new(0., 128., 0.));
                let mut bots = Bots::with_skill(1.);
                bots.nav = Some(Arc::new(NavMesh::from_areas(vec![start, cover])));
                bots.brains.insert(id, brain);
                let input = bots.think(&world).into_iter().find(|(actor, _)| *actor == id).unwrap().1;
                let brain = &bots.brains[&id];
                assert!(brain.visible);
                assert_eq!(brain.last_seen, world.time());
                assert_eq!(input.buttons & buttons::RELOAD != 0, should_reload);
                assert_eq!(brain.reload_cover.is_some(), should_reload);
                if should_reload {
                    assert_eq!(input.buttons & buttons::ATTACK, 0);
                    let (forward, right, _) = movement::angle_vectors(input.view_angles);
                    let wish = forward * input.forward_move + right * input.side_move;
                    assert!(wish.y > 400., "empty burst must hand movement to the cover route");
                    assert_eq!(brain.path, [1, 2]);
                } else {
                    assert!(brain.path.is_empty());
                    assert_eq!(input.forward_move, 0.);
                    assert_eq!(input.side_move, 0.);
                }
            }
        }
    }

    #[test] fn unseen_targets_remain_at_last_seen_position_and_cannot_be_fired_at() {
        let mut collision = collision::CollisionWorld::new();
        let surf = collision.surface_id("wall");
        let a=Vec3::new(50.0,40.0,0.0); let b=Vec3::new(50.0,300.0,0.0);
        let c=Vec3::new(50.0,300.0,200.0); let d=Vec3::new(50.0,40.0,200.0);
        collision.add_triangle([a,b,c],collision::contents::SOLID,surf);
        collision.add_triangle([a,c,d],collision::contents::SOLID,surf);
        collision.finalize();
        let mut w=World::with_catalog(Arc::new(collision), Arc::new(weapons::Catalog { weapons: Vec::new() }));
        let id=w.add_bot("observer",Team::Terrorist);
        let enemy=w.add_player("target",Team::CounterTerrorist);
        w.player_mut(id).unwrap().movement.origin=Vec3::ZERO;
        w.player_mut(id).unwrap().view_angles=Vec3::ZERO;
        w.player_mut(enemy).unwrap().movement.origin=Vec3::new(100.0,0.0,0.0);
        let every=interval(&w,PERCEPTION_RATE);
        w.tick=(every-id as u64%every)%every;
        let mut bots=Bots::with_skill(0.7);
        bots.think(&w);
        assert_eq!(bots.brains[&id].target,Some(enemy));
        let seen=bots.brains[&id].target_pos;
        w.player_mut(enemy).unwrap().movement.origin=Vec3::new(100.0,200.0,0.0);
        w.tick+=1;
        let input=bots.think(&w)[0].1.clone();
        assert_eq!(bots.brains[&id].target_pos,seen);
        assert!(!bots.brains[&id].visible);
        assert_eq!(input.buttons & buttons::ATTACK,0);
        w.tick+=100*w.tick_rate as u64/64;
        bots.think(&w);
        assert_eq!(bots.brains[&id].target,None);
    }
    #[test] fn angular_motion_is_bounded_and_converges_across_wrap() {
        let mut angle=350.0; let mut velocity=0.0;
        for _ in 0..160 {
            let dt=1.0/crate::DEFAULT_TICK_RATE as f32;
            let next=smooth_angle(angle,10.0,&mut velocity,dt,420.0);
            assert!((next-angle).abs()<=420.0*dt+0.001);
            assert!(next<=370.001);
            angle=next;
        }
        assert!(angle_diff(angle,10.0).abs()<0.01);
    }
    #[test] fn path_movement_is_independent_of_head_direction() {
        for yaw in [0.0,45.0,90.0,180.0,270.0] {
            let mut input=TickInput::default();
            move_towards(&mut input,yaw,Vec3::X,250.0);
            let (f,r,_)=movement::angle_vectors(Vec3::new(0.0,yaw,0.0));
            assert!((f*input.forward_move+r*input.side_move-Vec3::X*250.0).length()<0.001);
        }
    }
}

 #[cfg(test)] mod objective_tests {
     use super::*;
     #[test] fn one_defuser_others_cover_and_active_defuser_is_preserved() {
         let mut collision=collision::CollisionWorld::new(); collision.finalize();
         let mut world=World::with_catalog(Arc::new(collision),Arc::new(weapons::Catalog {weapons:Vec::new()}));
         let t=world.add_bot("T",Team::Terrorist);
         let a=world.add_bot("CT1",Team::CounterTerrorist);
         let b=world.add_bot("CT2",Team::CounterTerrorist);
         world.mode=GameMode::Competitive; world.round.phase=Phase::Live;
         world.objectives.bomb_sites.push(crate::objectives::BombSite { name:'A',zone:Zone { mins:Vec3::splat(-100.0),maxs:Vec3::splat(100.0) } });
         world.player_mut(a).unwrap().movement.origin=Vec3::X*200.0;
         world.player_mut(b).unwrap().movement.origin=Vec3::X*250.0;
         world.player_mut(b).unwrap().defuse_kit=true;
         world.bomb=Some(crate::PlantedBomb {pos:Vec3::ZERO,site:0,planter:t,planted_at:0.0,explode_at:40.0,defuser:None});
         assert!(matches!(Bots::task(&world,world.player(b).unwrap()),Task::Defuse(_)));
         assert!(matches!(Bots::task(&world,world.player(a).unwrap()),Task::Guard(_)));
         assert!(matches!(Bots::task(&world,world.player(t).unwrap()),Task::Guard(_)));
         world.bomb.as_mut().unwrap().defuser=Some((a,5.0));
         assert!(matches!(Bots::task(&world,world.player(a).unwrap()),Task::Defuse(_)));
         assert!(matches!(Bots::task(&world,world.player(b).unwrap()),Task::Guard(_)));
     }
 }

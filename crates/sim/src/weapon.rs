//! Weapon behaviour: firing rate, ammo, reloads, CS:GO-style inaccuracy and
//! a deterministic per-weapon recoil pattern.
//!
//! Stats come from [`weapons::WeaponDefinition`] (the game's own
//! `items_game.txt`). Values the item schema does not carry (reload and
//! deploy times) are approximations listed in [`reload_time`].
//!
//! Inaccuracy follows the public description of CS:GO's model: a base value
//! (standing/crouched), plus a movement term that grows between 34 % and
//! 95 % of max speed, plus a jump term in the air, plus a firing penalty
//! that decays to 10 % over the weapon's recovery time. Inaccuracy and
//! spread values are in milliradians.

use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use weapons::{WeaponClass, WeaponDefinition};

/// Degrees of aim punch per unit of `recoil magnitude`.
pub const RECOIL_DEGREES_PER_UNIT: f32 = 0.02;
/// CS:GO `weapon_recoil_scale`: bullets follow twice the aim punch.
pub const RECOIL_SCALE: f32 = 2.0;
/// Fraction of the (scaled) aim punch shown by the camera.
pub const VIEW_PUNCH_FRACTION: f32 = 0.5;
/// Fallback phase recovery without an independently observed content policy.
pub const RECOIL_RECOVERY_RATE: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cosmetic { pub serial:u64, pub paint: u32, pub seed: u32, pub wear: f32, pub stattrak:Option<u32>, pub souvenir:Option<u32>, pub stickers:[Option<(u32,f32)>;4] }

impl Cosmetic {
    /// Wear values in range (a client's choice is never trusted to be).
    pub fn sanitized(&self) -> Self {
        let wear = |w: f32| if w.is_finite() { w.clamp(0.0, 1.0) } else { 0.0 };
        Self { wear: wear(self.wear), stickers: self.stickers.map(|s| s.map(|(kit, w)| (kit, wear(w)))), ..*self }
    }
}

/// Derived data is excluded from both the wire state and state equality.
/// Reading aim punch may fill the cache without changing the weapon.
#[derive(Clone, Debug, Default)]
struct RecoilPatternCache(OnceLock<Arc<Vec<[f32; 2]>>>);

impl PartialEq for RecoilPatternCache {
    fn eq(&self, _other: &Self) -> bool { true }
}

#[derive(Clone, Debug, Default)]
struct RecoilImpulseCache(OnceLock<Option<Arc<Vec<[f32; 2]>>>>);
impl PartialEq for RecoilImpulseCache {
    fn eq(&self, _other: &Self) -> bool { true }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuppressorTransition {
    pub started_tick: u64,
    pub attached: bool,
}

/// Three-round option declared by the installed item schema/localization.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BurstProfile {
    pub cycle: f32,
    pub interval: f32,
}
impl BurstProfile {
    pub fn from_def(def: &WeaponDefinition) -> Option<Self> {
        if def.attr_f32("has burst mode") != Some(1.) || def.clip_size <= 0
            || matches!(def.class, WeaponClass::Knife | WeaponClass::Grenade | WeaponClass::Equipment) { return None; }
        let cycle = def.attr_f32("cycletime when in burst mode")?;
        let interval = def.attr_f32("time between burst shots")?;
        (cycle.is_finite() && interval.is_finite() && interval > 0.
            && interval * 2. <= cycle && cycle <= 30.).then_some(Self { cycle, interval })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BurstState {
    /// Follow-up bullets still due, excluding the bullet already emitted.
    pub remaining: u8,
    pub next_shot: f32,
}

impl SuppressorTransition {
    pub fn activity(self) -> &'static str {
        if self.attached { "ACT_VM_ATTACH_SILENCER" } else { "ACT_VM_DETACH_SILENCER" }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeaponState {
    #[serde(with = "crate::state::weapon_def")]
    pub def: Arc<WeaponDefinition>,
    pub cosmetic: Option<Cosmetic>,
    pub cosmetic_owner: Option<crate::PlayerId>,
    pub grenade_throw: Option<crate::utility::ThrowState>,
    pub suppressor_attached: bool,
    pub suppressor_transition: Option<SuppressorTransition>,
    pub burst_mode: bool,
    pub burst: Option<BurstState>,
    pub clip: i32,
    pub reserve: i32,
    /// Sim time at which the next shot may fire.
    pub next_attack: f32,
    /// Sim time at which an in-progress reload completes.
    pub reload_end: Option<f32>,
    /// Sim time the current (or last) reload began.
    pub reload_start: f32,
    /// Shells loaded so far in a shell-by-shell reload.
    pub shells_loaded: i32,
    /// 0 is unscoped; other levels come from the installed item schema.
    pub zoom_level: u8,
    pub next_secondary_attack: f32,
    pub zoom_changed_at: f32,
    pub zoom_from_fov: f32,
    pub resume_zoom: Option<u8>,
    pub last_melee_attack: Option<f32>,
    /// Firing inaccuracy, milliradians; decays with recovery time.
    pub accuracy_penalty: f32,
    /// Position in the recoil pattern (shots, fractional while recovering).
    pub recoil_index: f32,
    /// Angle and angular velocity evolve independently of the spray phase.
    /// Present only for a content profile with separate recovery evidence.
    pub recoil_motion: Option<crate::recoil_motion::RecoilMotion>,
    /// Cumulative recoil offsets (pitch, yaw) in degrees, computed from
    /// `def` on first use (not replicated: both sides derive it).
    #[serde(skip)]
    pattern: RecoilPatternCache,
    #[serde(skip)]
    impulses: RecoilImpulseCache,
}

impl WeaponState {
    pub fn new(def: Arc<WeaponDefinition>) -> Self {
        let clip = def.clip_size.max(0);
        let reserve = def.reserve_ammo.max(0);
        let suppressor_attached = def.attr_f32("has silencer").is_some_and(|v| v > 0.);
        Self {
            suppressor_attached,
            suppressor_transition: None,
            burst_mode: false,
            burst: None,
            def,
            cosmetic: None,
            cosmetic_owner: None,
            grenade_throw: None,
            clip,
            reserve,
            next_attack: 0.0,
            reload_end: None,
            reload_start: 0.0,
            shells_loaded: 0,
            zoom_level: 0,
            next_secondary_attack: 0.0,
            zoom_changed_at: 0.0,
            zoom_from_fov: 90.0,
            resume_zoom: None,
            last_melee_attack: None,
            accuracy_penalty: 0.0,
            recoil_index: 0.0,
            recoil_motion: None,
            pattern: RecoilPatternCache::default(),
            impulses: RecoilImpulseCache::default(),
        }
    }

    /// Swaps the definition (a knife model choice), keeping the state.
    pub fn set_def(&mut self, def: Arc<WeaponDefinition>) {
        self.def = def;
        self.pattern = RecoilPatternCache::default();
        self.reset_recoil_motion();
    }

    pub fn burst_profile(&self) -> Option<BurstProfile> { BurstProfile::from_def(&self.def) }

    /// Toggle only on an admitted secondary press while the weapon is ready.
    /// Native switch lockout is unmeasured; no invented animation delay is added.
    pub fn switch_burst(&mut self, time: f32) -> bool {
        if self.burst_profile().is_none() || self.burst.is_some() || self.reload_end.is_some()
            || self.suppressor_transition.is_some() || time < self.next_attack || time < self.next_secondary_attack { return false; }
        self.burst_mode = !self.burst_mode;
        self.pattern = RecoilPatternCache::default();
        self.reset_recoil_motion();
        true
    }

    pub fn cancel_burst(&mut self) { self.burst = None; }

    pub fn burst_due(&self, time: f32) -> bool {
        self.burst_mode && self.burst.is_some_and(|burst| time >= burst.next_shot && burst.remaining > 0)
            && self.reload_end.is_none() && self.clip > 0
    }

    /// Called after every ordinary authoritative bullet. Recovery is measured
    /// from the first bullet; native final-shot recovery remains unverified.
    pub(crate) fn shot_cooldown(&mut self, time: f32, dt: f32, continued_trigger: bool) {
        if self.burst_mode {
            if let Some(profile) = self.burst_profile() {
                self.burst = if let Some(burst) = self.burst {
                    (burst.remaining > 1 && self.clip > 0).then_some(BurstState {
                        remaining: burst.remaining.saturating_sub(1), next_shot: burst.next_shot + profile.interval,
                    })
                } else {
                    self.next_attack = time + profile.cycle;
                    (self.clip > 0).then_some(BurstState { remaining: self.clip.min(2) as u8, next_shot: time + profile.interval })
                };
                return;
            }
        }
        self.cancel_burst();
        let cycle = self.alt_stat("cycletime", self.def.cycle_time).max(0.01);
        let overdue = time - self.next_attack;
        // A fixed tick may observe a deadline slightly late. Re-anchoring
        // every held automatic shot at that tick accumulates the rounding
        // error across the magazine. Carry only this one-tick overrun; an
        // admitted new press or stale deadline starts a fresh interval.
        // Keep one bullet per tick, including faster custom-pack weapons.
        let carry = self.def.full_auto && continued_trigger && cycle >= dt
            && (0.0..=dt + 0.00001).contains(&overdue);
        self.next_attack = if carry { self.next_attack + cycle } else { time + cycle };
    }

    pub fn draw_activity(&self) -> &str {
        self.def.suppressor_animations.as_ref().map_or("ACT_VM_DRAW", |profile|
            if self.suppressor_attached { profile.draw_attached.as_str() }
            else { profile.draw_detached.as_str() })
    }

    pub fn suppressor_animation(&self) -> Option<&weapons::SuppressorAnimation> {
        let state = self.suppressor_transition?;
        let profile = self.def.suppressor_animations.as_ref()?;
        Some(if state.attached { &profile.attach } else { &profile.detach })
    }

    pub fn begin_suppressor(&mut self, tick: u64, dt: f32) -> bool {
        let time = tick as f32 * dt;
        if self.reload_end.is_some() || self.suppressor_transition.is_some()
            || time < self.next_attack || time < self.next_secondary_attack { return false; }
        let Some(profile) = &self.def.suppressor_animations else { return false; };
        let attached = !self.suppressor_attached;
        let animation = if attached { &profile.attach } else { &profile.detach };
        self.next_attack = time + animation.duration;
        self.next_secondary_attack = self.next_attack;
        self.suppressor_transition = Some(SuppressorTransition { started_tick: tick, attached });
        true
    }

    pub fn advance_suppressor(&mut self, tick: u64, dt: f32) {
        let Some(state) = self.suppressor_transition else { return; };
        let Some(animation) = self.suppressor_animation() else { return; };
        let elapsed = tick.saturating_sub(state.started_tick) as f32 * dt;
        let (complete, finished) = (elapsed >= animation.complete_time, elapsed >= animation.duration);
        if complete && self.suppressor_attached != state.attached {
            self.suppressor_attached = state.attached;
            self.pattern = RecoilPatternCache::default();
            self.reset_recoil_motion();
        }
        if finished { self.suppressor_transition = None; }
    }

    /// Holstering interrupts the animation; only a completed attachment change
    /// persists. The transition's lockout does not become an unrelated deploy delay.
    pub fn cancel_suppressor(&mut self, time: f32) {
        if self.suppressor_transition.take().is_some() {
            self.next_attack = time;
            self.next_secondary_attack = time;
        }
    }

    pub fn suppressor_visible(&self, time: f32, dt: f32) -> bool {
        let Some(state) = self.suppressor_transition else { return self.suppressor_attached; };
        let Some(animation) = self.suppressor_animation() else { return self.suppressor_attached; };
        let elapsed = (time - state.started_tick as f32 * dt).max(0.);
        animation.visibility.iter().filter(|(at, _)| *at <= elapsed)
            .last().map_or(!state.attached, |(_, show)| *show)
    }

    fn pattern(&self) -> &[[f32; 2]] {
        self.pattern.0.get_or_init(|| {
            let mut def = (*self.def).clone();
            if def.suppressor_animations.is_some() && !self.suppressor_attached {
                // A default-equipped observation cannot establish detached spray.
                def.recoil_trajectory = def.recoil_trajectory_unsuppressed.clone();
            }
            if self.burst_mode {
                // A normal-mode observation does not establish burst recoil.
                def.recoil_trajectory = None;
                let alt = |key, base| def.attr_f32(key).filter(|v| v.is_finite()).unwrap_or(base);
                def.recoil = weapons::Recoil { angle: alt("recoil angle alt", def.recoil.angle),
                    angle_variance: alt("recoil angle variance alt", def.recoil.angle_variance),
                    magnitude: alt("recoil magnitude alt", def.recoil.magnitude),
                    magnitude_variance: alt("recoil magnitude variance alt", def.recoil.magnitude_variance),
                    seed: def.recoil.seed };
            }
            Arc::new(recoil_pattern(&def))
        })
    }

    fn motion_profile(&self) -> Option<&weapons::RecoilTrajectory> {
        if self.burst_mode { return None; }
        let profile = if self.def.suppressor_animations.is_some() && !self.suppressor_attached {
            self.def.recoil_trajectory_unsuppressed.as_ref()
        } else { self.def.recoil_trajectory.as_ref() }?;
        (profile.bullet_angles.len() == self.def.clip_size.max(0) as usize
            && profile.bullet_angles.first() == Some(&[0., 0.])
            && crate::recoil_motion::valid(profile)).then_some(profile)
    }

    pub fn has_recoil_response(&self) -> bool { self.motion_profile().is_some() }

    fn phase_recovery(&self) -> Option<&weapons::RecoilPhaseRecovery> {
        self.motion_profile()?.response.as_ref()?.phase_recovery.as_ref()
    }

    pub fn reset_recoil_motion(&mut self) {
        self.recoil_motion = None;
        self.impulses = RecoilImpulseCache::default();
    }

    /// Called for all carried and ground weapons, including a holstered gun.
    pub(crate) fn advance_recoil_motion(&mut self, tick: u64) {
        if let (Some(profile), Some(mut state)) = (self.motion_profile(), self.recoil_motion) {
            state.advance(profile.response.as_ref().unwrap(), tick);
            self.recoil_motion = Some(state);
        }
    }

    pub(crate) fn retime_recoil_motion(&mut self, rate: u32) {
        if let Some(state) = &mut self.recoil_motion {
            let old_rate = state.tick_rate;
            let convert = |tick: u64| ((tick as f64 / old_rate as f64) * rate as f64).round() as u64;
            state.updated_tick = convert(state.updated_tick);
            state.anchor_tick = convert(state.anchor_tick);
            state.tick_rate = rate;
            self.impulses = RecoilImpulseCache::default();
        }
    }

    /// An impulse is added after sampling the actual authoritative ray.
    pub(crate) fn recoil_shot(&mut self, tick: u64, rate: u32) {
        if self.motion_profile().is_none() { return; }
        let discrete_phase = self.phase_recovery().is_some();
        if self.recoil_motion.is_none() {
            self.recoil_motion = Some(crate::recoil_motion::RecoilMotion::new(tick, rate));
        }
        if self.recoil_index <= 0. || (discrete_phase && self.recoil_index < 1.) {
            self.recoil_motion.as_mut().unwrap().anchor_tick = tick;
            self.impulses = RecoilImpulseCache::default();
        }
        let Some(impulses) = self.impulses.0.get_or_init(|| self.derive_recoil_impulses()).as_ref() else { return; };
        let index = if discrete_phase { self.recoil_index.floor() } else { self.recoil_index }
            .clamp(0., (impulses.len() - 1) as f32);
        let lo = index.floor() as usize;
        let hi = (lo + 1).min(impulses.len() - 1);
        let impulse = [
            impulses[lo][0] + (impulses[hi][0] - impulses[lo][0]) * index.fract(),
            impulses[lo][1] + (impulses[hi][1] - impulses[lo][1]) * index.fract(),
        ];
        let motion = self.recoil_motion.as_mut().unwrap();
        for axis in 0..2 { motion.velocity[axis] += impulse[axis]; }
    }

    fn derive_recoil_impulses(&self) -> Option<Arc<Vec<[f32; 2]>>> {
        let profile = self.motion_profile()?;
        let response = profile.response.as_ref()?;
        let state = self.recoil_motion?;
        let rate = state.tick_rate;
        let dt = 1. / rate as f32;
        let mut tick = state.anchor_tick;
        // Use the authoritative deadline carry policy itself, including its
        // f32 rounding and first-fire anchor. A nominal cycle rounded once
        // does not reproduce a full held magazine at either 64 or 128 Hz.
        let mut clock = Self::new(self.def.clone());
        clock.suppressor_attached = self.suppressor_attached;
        clock.next_attack = tick as f32 * dt;
        let mut angle = [0.; 2];
        let mut velocity = [0.; 2];
        let mut impulses = Vec::with_capacity(profile.bullet_angles.len());
        for index in 0..profile.bullet_angles.len() - 1 {
            clock.shot_cooldown(tick as f32 * dt, dt, true);
            let previous = tick;
            // A definition's admitted cycle is bounded by its ordinary
            // fire policy. This loop is at most one magazine of deadlines.
            let limit = (rate as f32 * 30.).ceil() as u64;
            while (tick as f32 * dt) < clock.next_attack {
                tick += 1;
                if tick.saturating_sub(previous) > limit { return None; }
            }
            let seconds = (tick - previous) as f64 / rate as f64;
            let target = profile.bullet_angles[index + 1].map(|x| x / RECOIL_SCALE);
            let total = if index == 0 { response.first_impulse } else {
                crate::recoil_motion::velocity_for_target(angle, velocity, target, response, seconds)?
            };
            impulses.push([total[0] - velocity[0], total[1] - velocity[1]]);
            velocity = total;
            crate::recoil_motion::advance(&mut angle, &mut velocity, response, seconds);
        }
        impulses.push(response.final_impulse);
        Some(Arc::new(impulses))
    }

    pub fn zoom_levels(&self) -> u8 {
        self.def.attr_f32("zoom levels").unwrap_or(0.0).clamp(0.0, 2.0) as u8
    }

    pub fn fov(&self) -> f32 {
        if self.zoom_level == 0 { 90.0 } else {
            self.def.attr_f32(&format!("zoom fov {}", self.zoom_level)).unwrap_or(90.0)
        }
    }

    pub fn rendered_fov(&self, time: f32) -> f32 {
        let duration = self.def.attr_f32(&format!("zoom time {}", self.zoom_level)).unwrap_or(0.05).max(0.001);
        let t = ((time - self.zoom_changed_at) / duration).clamp(0.0, 1.0);
        self.zoom_from_fov + (self.fov() - self.zoom_from_fov) * t
    }

    pub fn set_zoom(&mut self, level: u8, time: f32) {
        self.zoom_from_fov = self.rendered_fov(time);
        self.zoom_level = level.min(self.zoom_levels());
        self.zoom_changed_at = time;
    }

    pub fn reset_zoom(&mut self, time: f32) {
        self.set_zoom(0, time);
        self.resume_zoom = None;
    }

    pub fn alt_stat(&self, key: &str, base: f32) -> f32 {
        if self.zoom_level > 0 || self.burst_mode { self.def.attr_f32(&format!("{key} alt")).unwrap_or(base) } else { base }
    }

    pub fn max_speed(&self) -> f32 { self.alt_stat("max player speed", self.def.max_player_speed) }

    pub fn hides_viewmodel(&self) -> bool {
        self.zoom_level > 0 && self.def.attr_f32("hide view model zoomed").is_some_and(|v| v != 0.0)
    }

    pub fn is_melee(&self) -> bool {
        self.def.class == WeaponClass::Knife
    }

    pub fn uses_ammo(&self) -> bool {
        self.def.clip_size > 0
    }

    pub fn can_reload(&self) -> bool {
        self.uses_ammo() && self.burst.is_none() && self.suppressor_transition.is_none() && self.reload_end.is_none() && self.clip < self.def.clip_size && self.reserve > 0
    }

    /// Shell-by-shell timing if this weapon loads one shell at a time.
    pub fn shell_reload(&self) -> Option<ShellReload> {
        shell_reload(&self.def)
    }

    /// Starts a reload: magazines finish after [`reload_time`]; shotguns
    /// that load shells take a start, then one insert per missing shell (up
    /// to the reserve), loading each shell as its insert completes.
    pub fn begin_reload(&mut self, time: f32) {
        self.cancel_burst();
        self.reload_start = time;
        self.shells_loaded = 0;
        let total = match self.shell_reload() {
            Some(s) => {
                let shells = (self.def.clip_size - self.clip).min(self.reserve).max(1);
                s.start + s.insert * shells as f32
            }
            None => reload_time(&self.def),
        };
        self.reload_end = Some(time + total);
    }

    /// 0..1 through the current reload.
    pub fn reload_fraction(&self, time: f32) -> Option<f32> {
        let end = self.reload_end?;
        Some(((time - self.reload_start) / (end - self.reload_start).max(0.01)).clamp(0.0, 1.0))
    }

    /// Where a shell-by-shell reload is: the start (`None`) or the insert
    /// of shell `n` (0-based), with 0..1 progress through that stage.
    pub fn shell_stage(&self, time: f32) -> Option<(Option<i32>, f32)> {
        let s = self.shell_reload()?;
        let end = self.reload_end?;
        let t = time - self.reload_start;
        if t < s.start {
            return Some((None, (t / s.start.max(0.01)).clamp(0.0, 1.0)));
        }
        // Rendering interpolates slightly past the last tick: stay on the
        // final insert rather than start one that will not happen.
        let shells = ((end - self.reload_start - s.start) / s.insert.max(0.01)).round().max(1.0);
        let k = ((t - s.start) / s.insert.max(0.01)).clamp(0.0, shells - 1e-3);
        Some((Some(k.floor() as i32), k.fract()))
    }

    /// Current aim punch (pitch, yaw) in degrees from the recoil pattern,
    /// interpolated at the fractional recoil index. Negative pitch is up.
    pub fn aim_punch(&self) -> [f32; 2] {
        if self.motion_profile().is_some() {
            return self.recoil_motion.map_or([0.; 2], |state| state.angle);
        }
        if self.recoil_index <= 0.0 {
            return [0.0, 0.0];
        }
        let pattern = self.pattern();
        if pattern.is_empty() {
            return [0.0, 0.0];
        }
        let max = (pattern.len() - 1) as f32;
        let i = self.recoil_index.min(max);
        let lo = i.floor() as usize;
        let hi = (lo + 1).min(pattern.len() - 1);
        let t = i - lo as f32;
        let a = pattern[lo];
        let b = pattern[hi];
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }
}

/// A fixed pattern per weapon, generated from its recoil stats and seed:
/// each shot kicks by `magnitude` along `angle ± variance` (0° = up).
pub fn recoil_pattern(def: &WeaponDefinition) -> Vec<[f32; 2]> {
    if let Some(profile) = &def.recoil_trajectory {
        let angles = &profile.bullet_angles;
        if angles.len() == def.clip_size.max(0) as usize
            && (2..=256).contains(&angles.len())
            && angles[0] == [0., 0.]
            && angles.iter().all(|angle| angle[0].is_finite() && angle[1].is_finite()
                && angle[0].abs() < 89. && angle[1].abs() <= 180.)
        {
            // fire() samples this before increasing recoil_index and applies
            // RECOIL_SCALE once. Preserve the measured bullet-space angles.
            // After the observed magazine, aim_punch clamps to its last point;
            // do not invent additional shots from a different generator.
            return angles.iter().map(|angle|
                [angle[0] / RECOIL_SCALE, angle[1] / RECOIL_SCALE]).collect();
        }
    }
    let shots = (def.clip_size.max(1) as usize + 1).min(256);
    let mut rng = Rng::from_parts(&[def.recoil.seed as u64, def.def_index as u64]);
    let mut out = Vec::with_capacity(shots + 1);
    let (mut pitch, mut yaw) = (0.0f32, 0.0f32);
    out.push([0.0, 0.0]);
    for shot in 0..shots {
        let angle = def.recoil.angle + rng.range(-def.recoil.angle_variance, def.recoil.angle_variance);
        let magnitude = def.recoil.magnitude + rng.range(-def.recoil.magnitude_variance, def.recoil.magnitude_variance);
        // The first shots of a spray climb almost straight up.
        let straight = if shot < 3 { 0.25 } else { 1.0 };
        let a = (angle * straight).to_radians();
        let deg = magnitude.max(0.0) * RECOIL_DEGREES_PER_UNIT;
        pitch -= deg * a.cos();
        yaw += deg * a.sin();
        out.push([pitch, yaw]);
    }
    out
}

/// Seconds to reload. The item schema does not store this (it comes from
/// the view-model animations), so these are measured approximations.
pub fn reload_time(def: &WeaponDefinition) -> f32 {
    match def.class_name.as_str() {
        "weapon_ak47" => 2.43,
        "weapon_m4a1" => 3.07,
        "weapon_m4a1_silencer" => 3.07,
        "weapon_awp" => 3.67,
        "weapon_ssg08" => 3.7,
        "weapon_deagle" => 2.2,
        "weapon_glock" => 2.27,
        "weapon_usp_silencer" | "weapon_hkp2000" => 2.17,
        "weapon_famas" => 3.3,
        "weapon_galilar" => 3.0,
        "weapon_aug" => 3.8,
        "weapon_sg556" => 2.83,
        "weapon_negev" | "weapon_m249" => 5.7,
        _ => match def.class {
            WeaponClass::Pistol => 2.2,
            WeaponClass::Smg => 2.6,
            WeaponClass::Rifle => 2.9,
            WeaponClass::Sniper => 3.7,
            WeaponClass::Shotgun => 3.0,
            WeaponClass::MachineGun => 5.7,
            _ => 2.5,
        },
    }
}

/// Stages of a shell-by-shell reload, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellReload {
    /// Bringing the gun up to the loading port.
    pub start: f32,
    /// One shell.
    pub insert: f32,
    /// Racking it afterwards (cosmetic; the gun can fire).
    pub finish: f32,
}

/// Shotguns without a magazine (the item schema gives the MAG-7 a
/// `magazine model`; the Nova, XM1014 and Sawed-Off have none) load one
/// shell at a time. Stage lengths are those of the view-model sequences
/// `start_reload`, `insert` and `after_reload`.
pub fn shell_reload(def: &WeaponDefinition) -> Option<ShellReload> {
    if def.class != WeaponClass::Shotgun || def.attributes.contains_key("magazine model") {
        return None;
    }
    let (start, insert, finish) = match def.class_name.as_str() {
        "weapon_nova" => (12.0 / 30.0, 14.0 / 30.0, 25.0 / 30.0),
        "weapon_xm1014" => (21.0 / 30.0, 13.0 / 20.0, 13.0 / 30.0),
        "weapon_sawedoff" => (12.0 / 30.0, 15.0 / 30.0, 26.0 / 30.0),
        _ => (0.4, 0.5, 0.8),
    };
    Some(ShellReload { start, insert, finish })
}

/// Seconds after switching before the weapon can fire.
pub fn deploy_time(def: &WeaponDefinition) -> f32 {
    match def.class {
        WeaponClass::Knife => 0.5,
        WeaponClass::Pistol => 0.8,
        _ => 1.0,
    }
}

/// The bomb's class name.
pub const C4: &str = "weapon_c4";

/// CS:GO inventory slot: 1 primary, 2 pistol, 3 knife/zeus, 4 grenades,
/// 5 bomb.
pub fn slot(def: &WeaponDefinition) -> u8 {
    match def.class {
        WeaponClass::Pistol => 2,
        WeaponClass::Knife => 3,
        WeaponClass::Grenade => 4,
        WeaponClass::Equipment if def.class_name == C4 => 5,
        WeaponClass::Equipment => 3,
        _ => 1,
    }
}

/// Inputs to the inaccuracy model for one shot.
#[derive(Clone, Copy, Debug)]
pub struct AccuracyContext {
    pub speed: f32,
    pub on_ground: bool,
    pub ducked: bool,
}

/// Total inaccuracy (milliradians) for the next shot.
pub fn inaccuracy(w: &WeaponState, ctx: AccuracyContext) -> f32 {
    let a = &w.def.accuracy;
    let base = if ctx.ducked { w.alt_stat("inaccuracy crouch", a.inaccuracy_crouch) } else { w.alt_stat("inaccuracy stand", a.inaccuracy_stand) };
    let max_speed = w.max_speed().max(1.0);
    let lo = max_speed * 0.34;
    let hi = max_speed * 0.95;
    let move_frac = ((ctx.speed - lo) / (hi - lo)).clamp(0.0, 1.0);
    let air = if ctx.on_ground { 0.0 } else { w.alt_stat("inaccuracy jump", a.inaccuracy_jump) };
    base + w.alt_stat("inaccuracy move", a.inaccuracy_move) * move_frac + air + w.accuracy_penalty
}

/// Advances timers every tick. `firing` includes a loaded automatic trigger
/// held through its shot cooldown, not just a shot emitted on this tick.
pub fn think(w: &mut WeaponState, time: f32, dt: f32, ducked: bool, firing: bool) {
    if time >= w.next_attack && w.reload_end.is_none() {
        if let Some(level) = w.resume_zoom.take() { w.set_zoom(level, time); }
    }
    let recovery = if ducked {
        w.alt_stat("recovery time crouch", w.def.accuracy.recovery_time_crouch)
    } else {
        w.alt_stat("recovery time stand", w.def.accuracy.recovery_time_stand)
    }
    .max(0.05);
    // Decays to 10 % over the recovery time.
    w.accuracy_penalty *= 0.1f32.powf(dt / recovery);
    if !firing {
        if let Some(phase) = w.phase_recovery() {
            // Direct weapon-state observations establish a plateau followed
            // by this multiplicative decay. The ordinary attack deadline
            // supplies the cooldown gate; receive-time captures do not prove
            // its exact native boundary tick.
            if time > w.next_attack {
                w.recoil_index *= 0.1f32.powf(phase.decay_decades_per_second * dt);
            }
        } else {
            w.recoil_index = (w.recoil_index - RECOIL_RECOVERY_RATE * dt).max(0.0);
        }
    }
    if let Some(end) = w.reload_end {
        if let Some(s) = shell_reload(&w.def) {
            // One shell each time an insert completes (all of them by the end).
            let due = if time >= end {
                i32::MAX
            } else {
                (((time - w.reload_start - s.start) / s.insert.max(0.01)).floor() as i32).max(0)
            };
            while w.shells_loaded < due && w.clip < w.def.clip_size && w.reserve > 0 {
                w.clip += 1;
                w.reserve -= 1;
                w.shells_loaded += 1;
            }
            if time >= end || w.clip >= w.def.clip_size || w.reserve <= 0 {
                w.reload_end = None;
            }
        } else if time >= end {
            let need = (w.def.clip_size - w.clip).max(0);
            let take = need.min(w.reserve);
            w.clip += take;
            w.reserve -= take;
            w.reload_end = None;
        }
    }
}

/// Damage after range falloff, hit group and armour, as CS:GO computes it.
/// Returns (health damage, armour damage).
pub fn damage(def: &WeaponDefinition, distance: f32, group: HitGroup, armor: i32, helmet: bool) -> (i32, i32) {
    let mut dmg = def.damage * def.range_modifier.powf(distance / 500.0);
    dmg *= group.multiplier();
    let armored = armor > 0 && match group {
        HitGroup::Head => helmet,
        HitGroup::LeftLeg | HitGroup::RightLeg => false,
        _ => true,
    };
    if !armored {
        return (dmg as i32, 0);
    }
    let ratio = def.armor_ratio * 0.5;
    let mut health = dmg * ratio;
    let mut armor_dmg = (dmg - health) * 0.5;
    if armor_dmg > armor as f32 {
        armor_dmg = armor as f32;
        health = dmg - armor_dmg * 2.0;
    }
    (health as i32, armor_dmg as i32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HitGroup {
    Head,
    Chest,
    Stomach,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

impl HitGroup {
    /// Known authored Source1 body groups. Unknown/neck-only behavior retains
    /// the prior coarse classification until separately measured.
    pub fn from_model(group: i32) -> Option<Self> {
        Some(match group { 1 => Self::Head, 2 => Self::Chest, 3 => Self::Stomach,
            4 => Self::LeftArm, 5 => Self::RightArm, 6 => Self::LeftLeg,
            7 => Self::RightLeg, _ => return None })
    }

    pub fn multiplier(self) -> f32 {
        match self {
            HitGroup::Head => 4.0,
            HitGroup::Stomach => 1.25,
            HitGroup::LeftLeg | HitGroup::RightLeg => 0.75,
            _ => 1.0,
        }
    }

    /// Hit group from where a bullet entered a standing or crouched hull,
    /// as a fraction of the hull height (until player hitboxes are loaded
    /// from the character models).
    pub fn from_height(frac: f32, side: f32) -> HitGroup {
        if frac > 0.86 {
            HitGroup::Head
        } else if frac > 0.62 {
            if side.abs() > 0.7 {
                if side > 0.0 { HitGroup::RightArm } else { HitGroup::LeftArm }
            } else {
                HitGroup::Chest
            }
        } else if frac > 0.46 {
            HitGroup::Stomach
        } else if side >= 0.0 {
            HitGroup::RightLeg
        } else {
            HitGroup::LeftLeg
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::{ContentId, ContentKind};

    fn ak() -> WeaponDefinition {
        WeaponDefinition {
            recoil_trajectory: None,
            recoil_trajectory_unsuppressed: None,
            grenade_animations: None,
            suppressor_animations: None,
            id: ContentId::csgo(ContentKind::Weapon, "ak47"),
            class_name: "weapon_ak47".into(),
            def_index: 7,
            name_token: String::new(),
            class: WeaponClass::Rifle,
            side: weapons::Side::Terrorist,
            price: 2700,
            damage: 36.0,
            armor_ratio: 1.55,
            penetration: 2.0,
            range: 8192.0,
            range_modifier: 0.98,
            cycle_time: 0.1,
            full_auto: true,
            bullets: 1,
            clip_size: 30,
            reserve_ammo: 90,
            max_player_speed: 215.0,
            accuracy: weapons::Accuracy {
                spread: 0.6,
                inaccuracy_stand: 6.41,
                inaccuracy_crouch: 4.81,
                inaccuracy_move: 175.06,
                inaccuracy_jump: 140.76,
                inaccuracy_land: 0.242,
                inaccuracy_fire: 7.8,
                recovery_time_stand: 0.368,
                recovery_time_crouch: 0.305,
            },
            recoil: weapons::Recoil {
                angle: 0.0,
                angle_variance: 70.0,
                magnitude: 30.0,
                magnitude_variance: 0.0,
                seed: 223,
            },
            view_model: String::new(),
            world_model: String::new(),
            unsuppressed_fire_sound: String::new(),
            fire_sound: String::new(),
            empty_trigger: None,
            zoom_in_sound: String::new(),
            zoom_out_sound: String::new(),
            kill_award: 300,
            attributes: Default::default(),
        }
    }

    fn shotgun(name: &str, magazine: bool) -> WeaponDefinition {
        let mut d = ak();
        d.class_name = name.into();
        d.class = WeaponClass::Shotgun;
        d.clip_size = 8;
        d.reserve_ammo = 32;
        if magazine {
            d.attributes.insert("magazine model".into(), "models/weapons/w_shot_mag7_mag.mdl".into());
        }
        d
    }

    fn run_reload(w: &mut WeaponState, from: f32, until: f32) {
        let mut t = from;
        while t < until {
            think(w, t, 1.0 / 128.0, false, false);
            t += 1.0 / 128.0;
        }
    }

    #[test]
    fn shotguns_load_shell_by_shell() {
        let mut w = WeaponState::new(Arc::new(shotgun("weapon_nova", false)));
        let s = w.shell_reload().expect("the Nova loads shells");
        w.clip = 0;
        w.begin_reload(0.0);
        // Eight missing shells: the start plus eight inserts.
        let end = w.reload_end.unwrap();
        assert!((end - (s.start + 8.0 * s.insert)).abs() < 1e-4);
        assert_eq!(w.shell_stage(0.1).unwrap().0, None);
        assert_eq!(w.shell_stage(s.start + 2.5 * s.insert).unwrap().0, Some(2));
        // Shells arrive one per insert.
        run_reload(&mut w, 0.0, s.start + 2.5 * s.insert);
        assert_eq!(w.clip, 2);
        assert_eq!(w.reserve, 30);
        run_reload(&mut w, s.start + 2.5 * s.insert, end + 0.1);
        assert_eq!(w.clip, 8);
        assert_eq!(w.reserve, 24);
        assert!(w.reload_end.is_none());
        // Only as many inserts as the reserve allows.
        w.clip = 0;
        w.reserve = 3;
        w.begin_reload(10.0);
        assert!((w.reload_end.unwrap() - 10.0 - (s.start + 3.0 * s.insert)).abs() < 1e-4);
        // The MAG-7 has a magazine.
        assert!(WeaponState::new(Arc::new(shotgun("weapon_mag7", true))).shell_reload().is_none());
    }

    #[test]
    fn ak_damage_matches_csgo() {
        // Known CS:GO values at close range: head 111 with a helmet, 143
        // without; chest 27 through armour, 35 without.
        let (hp, _) = damage(&ak(), 100.0, HitGroup::Head, 100, true);
        assert_eq!(hp, 111);
        let (hp, _) = damage(&ak(), 100.0, HitGroup::Head, 100, false);
        assert_eq!(hp, 143);
        let (hp, armor) = damage(&ak(), 100.0, HitGroup::Chest, 100, false);
        assert_eq!(hp, 27);
        assert!(armor > 0);
        let (hp, _) = damage(&ak(), 100.0, HitGroup::Chest, 0, false);
        assert_eq!(hp, 35);
    }

    #[test]
    fn running_is_inaccurate() {
        let w = WeaponState::new(Arc::new(ak()));
        let still = inaccuracy(&w, AccuracyContext { speed: 0.0, on_ground: true, ducked: false });
        let walking = inaccuracy(&w, AccuracyContext { speed: 70.0, on_ground: true, ducked: false });
        let running = inaccuracy(&w, AccuracyContext { speed: 215.0, on_ground: true, ducked: false });
        assert_eq!(still, walking);
        assert!(running > still * 20.0);
    }

    #[test]
    fn recoil_pattern_is_fixed_and_climbs() {
        let a = recoil_pattern(&ak());
        let b = recoil_pattern(&ak());
        assert_eq!(a, b);
        assert!(a[10][0] < -3.0, "pitch after 10 shots {}", a[10][0]);
    }
}

//! Game sounds from CS:GO soundscripts.
//!
//! `scripts/game_sounds_manifest.txt` lists soundscript files; each maps an
//! entry name (`Weapon_AK47.Single`) to one `wave` or several `rndwave`
//! files under `sound/`, plus its volume, pitch, soundlevel, channel and
//! (for "version 2" entries) operator stacks. Wave paths may start with flag
//! characters (`)`, `^`, `*`, ...) that select mixing behaviour in the
//! original engine; they are stripped here.
//!
//! Loudness follows measurements of the real game (docs/FIDELITY.md, "Sound
//! audit"): each entry falls into a category with a measured gain at the
//! listener and in the field, and a measured distance curve chosen by the
//! operator stack it imports (or by its soundlevel when it has none).
//! Distances are recomputed every frame, so a sound fades as the listener
//! walks away from it.

use crate::coords::to_bevy;
use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, SpatialAudioSink, SpatialListener, SpatialScale, Volume};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

mod classic;
mod utility;
#[cfg(test)]
mod combat_tests;
pub use classic::SoundEra;

/// Rodio applies inverse-square attenuation in spatial-scale units; scaling
/// the world down this far keeps every in-map distance under one unit, so
/// rodio only pans and the measured curves below set the loudness.
const PAN_ONLY_SCALE: f32 = 1.0 / 100_000.0;
/// Rodio gives a source straight ahead 0.75 per ear; the curves below are
/// per-ear gains measured in the real game, so spatial gains are divided
/// by this.
const RODIO_FRONT: f32 = 0.75;

/// Adapter for rodio's distance-difference panner. With attenuation disabled,
/// rodio 0.22 boosts the *farther* ear; exchange its virtual ears so a source
/// on the right is heard on the right. Positions stay in Source inches.
pub fn spatial_listener() -> SpatialListener {
    SpatialListener { left_ear_offset: Vec3::X * 8.0, right_ear_offset: Vec3::X * -8.0 }
}

/// How an entry is mixed: which measured gains and distance curve apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// `CS_update_weapon`: gunfire and explosions.
    Weapon,
    /// `CS_update_foley`: reloads, deploys, damage, C4 handling.
    Foley,
    /// `CS_update_footsteps`.
    Footsteps,
    /// `CS_update_suit`: gear rustle with each step.
    Suit,
    /// `CS_update_rics` / `CS_update_physics`: bullet impacts.
    Impacts,
    /// Announcer and radio lines (`radio/`).
    Announcer,
    /// Menu and HUD sounds (`ui/`).
    Ui,
    /// Player voice lines (`vo/`).
    Voice,
    /// Map ambience from soundscapes (`ambient/`, the "Ambient" mix group).
    Ambient,
    /// Everything else: attenuated by soundlevel.
    Plain,
}

impl Category {
    /// Gain per unit of soundscript volume when heard at the listener,
    /// relative to the AK-47 shot (lvol 16.83 at volume 0.9).
    fn near(self, local: bool) -> f32 {
        match self {
            Category::Weapon | Category::Foley => 1.0,
            // The local player's own steps use the quiet "PlayerFootsteps" mix.
            Category::Footsteps | Category::Suit => if local { 0.18 } else { 1.0 },
            // Land_<surface> uses the physics stack, but the local player's
            // landing measures 10.93 / (16.83 / 0.9), rather than impact gain.
            Category::Impacts => if local { 0.58 } else { 0.29 },
            Category::Announcer | Category::Ui => 0.58,
            Category::Voice => 0.29,
            // soundmixers.txt: Ambient 0.25 against Weapons1 0.7.
            Category::Ambient => 0.36,
            Category::Plain => 1.6,
        }
    }

    /// Gain per unit volume for a source 30-400 u in front, before the
    /// distance curve, divided by rodio's own front-source panning.
    fn field(self) -> f32 {
        let measured = match self {
            Category::Weapon => 0.58,
            Category::Foley => 0.55,
            Category::Footsteps | Category::Suit => 0.73,
            Category::Impacts => 0.16,
            Category::Announcer | Category::Ui => 0.58 * 0.57,
            Category::Voice => 0.29 * 0.57,
            Category::Ambient => 0.36 * 0.57,
            Category::Plain => 1.08,
        };
        measured / RODIO_FRONT
    }

    /// Measured distance curve (`calc_falloff_curve` min/max of the stack).
    fn falloff(self, soundlevel: f32, distance: f32) -> f32 {
        match self {
            Category::Weapon => stack_curve(distance, 400.0, 3937.0).max(0.05),
            Category::Foley => stack_curve(distance, 400.0, 1100.0),
            Category::Footsteps | Category::Suit => stack_curve(distance, 120.0, 1100.0),
            Category::Impacts => {
                if distance <= 120.0 { 1.0 } else if distance >= 600.0 { 0.0 } else { (-1.3 * (distance - 120.0) / 480.0).exp() }
            }
            _ => soundlevel_curve(soundlevel, distance),
        }
    }
}

/// Weapon, foley and footstep stacks (curve amount 0.7): flat to `min`,
/// then about `0.78 e^(-2.5x)` with `x = (d - min) / (max - min)`, silent
/// past `max`. Fitted to `snd_showstart` gains along Dust II's Mid.
pub fn stack_curve(distance: f32, min: f32, max: f32) -> f32 {
    if distance <= min {
        1.0
    } else if distance >= max {
        0.0
    } else {
        0.78 * (-2.5 * (distance - min) / (max - min)).exp()
    }
}

/// Entries without an operator stack: inverse distance past a radius of
/// 32 u at 60 dB that grows 10x per 22 dB (fitted to 60/65/75/80 dB).
/// Soundlevel 0 (`SNDLVL_NONE`) does not attenuate.
pub fn soundlevel_curve(soundlevel: f32, distance: f32) -> f32 {
    if soundlevel <= 0.0 {
        return 1.0;
    }
    let radius = 32.0 * 10f32.powf((soundlevel - 60.0) / 22.0);
    (radius / distance.max(1.0)).min(1.0)
}

/// Gunfire layers: the near entry fades out and its `…SingleDistant`
/// partner fades in between 800 and 2800 units (the stack's
/// `math_remap_float` and `pow 1.2` operators).
pub fn gunfire_crossfade(distance: f32, distant: bool) -> f32 {
    let t = ((distance - 800.0) / 2000.0).clamp(0.0, 1.0);
    if distant { 1.0 - (1.0 - t).powf(1.2) } else { 1.0 - t.powf(1.2) }
}

/// Retained literal weapon bounds. The existing fitted stack shape remains;
/// an authored curve_amount is metadata, not a calibrated operator formula.
#[derive(Clone, Copy, Debug, PartialEq)]
struct WeaponFalloff { min: f32, max: f32, floor: f32, curve_amount: f32 }
impl Default for WeaponFalloff {
    fn default() -> Self { Self { min: 400.0, max: 3937.0, floor: 0.05, curve_amount: 0.7 } }
}
impl WeaponFalloff {
    fn gain(self, distance: f32) -> f32 { stack_curve(distance, self.min, self.max).max(self.floor) }
}
#[derive(Clone, Copy)]
struct GunfireFade { start: f32, end: f32, distant: bool }
impl GunfireFade {
    fn gain(self, distance: f32) -> f32 {
        let t = ((distance - self.start) / (self.end - self.start)).clamp(0.0, 1.0);
        if self.distant { 1.0 - (1.0 - t).powf(1.2) } else { 1.0 - t.powf(1.2) }
    }
}
// A device which never creates sinks must not retain unbounded gun requests.
const MAX_PENDING_GUN_VOICES: usize = 64;

/// Source sound channels that hold one sound per entity: a new sound on the
/// same channel stops the old one. `CHAN_AUTO` and `CHAN_STATIC` overlap.
fn channel_number(name: &str) -> u8 {
    match name.trim().to_ascii_uppercase().as_str() {
        "CHAN_WEAPON" => 1,
        "CHAN_VOICE" => 2,
        "CHAN_ITEM" => 3,
        "CHAN_BODY" => 4,
        "CHAN_STREAM" => 5,
        "CHAN_STATIC" => 6,
        "CHAN_VOICE2" => 7,
        _ => 0,
    }
}

/// Soundlevel in dB from `SNDLVL_*` / `ATTN_*` names or a number.
pub fn parse_soundlevel(text: &str) -> f32 {
    let t = text.trim().to_ascii_uppercase();
    match t.as_str() {
        "SNDLVL_NONE" | "ATTN_NONE" => 0.0,
        "SNDLVL_NORM" | "ATTN_NORM" => 75.0,
        "SNDLVL_IDLE" | "ATTN_IDLE" => 60.0,
        "SNDLVL_STATIC" | "ATTN_STATIC" => 66.0,
        "SNDLVL_TALKING" => 80.0,
        "ATTN_RICOCHET" => 65.0,
        "SNDLVL_GUNFIRE" | "ATTN_GUNFIRE" => 140.0,
        _ => {
            let digits: String = t.trim_start_matches("SNDLVL_").chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            digits.parse().unwrap_or(75.0)
        }
    }
}

/// A value or a `"low, high"` range, with named constants resolved.
fn parse_range(text: &str, named: &dyn Fn(&str) -> Option<f32>) -> (f32, f32) {
    let one = |s: &str| named(s.trim()).or_else(|| kv::parse_f32(s.trim()));
    let mut parts = text.split(',');
    let low = parts.next().and_then(one);
    let high = parts.next().and_then(one);
    match (low, high) {
        (Some(l), Some(h)) => (l.min(h), l.max(h)),
        (Some(l), None) => (l, l),
        _ => (1.0, 1.0),
    }
}

#[derive(Clone, Debug)]
struct Entry {
    waves: Vec<String>,
    volume: (f32, f32),
    /// Percent; 100 = normal speed.
    pitch: (f32, f32),
    soundlevel: f32,
    channel: u8,
    category: Category,
    weapon_falloff: Option<WeaponFalloff>,
    /// Literal overrides of the imported foley stack's distance bounds.
    foley_bounds: Option<(f32, f32)>,
    xfade_bounds: (f32, f32),
    explicit_near_fade: bool,
    /// `play_distant_version_*` partner entry.
    distant: Option<String>,
    /// Older instances of this entry an entity may keep when it starts
    /// another (`sys_stop_entries` in the `CS_limit_*` start stacks).
    limit: Option<usize>,
    /// This is a `…Distant` layer (cross-faded in with distance).
    distant_layer: bool,
    /// Era-profile bytes verified and decoded once, independent of later mounts.
    frozen_wave: Option<classic::FrozenWave>,
}

#[derive(Component)]
pub struct SoundGain(pub f32);

/// A sound placed in the world; its gain follows the listener's distance.
#[derive(Component, Clone, Copy)]
pub struct Emitter {
    /// Bevy-space position.
    pub pos: Vec3,
    base: f32,
    category: Category,
    soundlevel: f32,
    gunfire: Option<GunfireFade>,
    weapon_falloff: Option<WeaponFalloff>,
    foley_bounds: Option<(f32, f32)>,
}

impl Emitter {
    fn gain(&self, distance: f32) -> f32 {
        let fade = self.gunfire.map_or(1.0, |fade| fade.gain(distance));
        let falloff = self.weapon_falloff.map_or_else(|| {
            self.foley_bounds.map_or_else(|| self.category.falloff(self.soundlevel, distance),
                |(min, max)| stack_curve(distance, min, max))
        }, |curve| curve.gain(distance));
        self.base * falloff * fade
    }
}

/// Only remote steps and their gear rustle use this bounded acoustic pass.
/// Queue them paused until Update has the current listener/collision snapshot,
/// so their attack cannot briefly play at the unobstructed volume.
#[derive(Component, Default)]
pub struct FootstepOcclusion {
    gain: f32,
    next_check: f64,
}

fn footstep_occlusion(world: &collision::CollisionWorld, source: Vec3, listener: Vec3) -> f32 {
    use collision::contents;
    let source = crate::coords::to_source(source);
    let listener = crate::coords::to_source(listener);
    // Foley emits at the feet. Lift off the supporting surface, then sample
    // slightly higher too: a low crate/step must not act like a sealed wall.
    // Exclude playerclip, water and other invisible movement-only volumes.
    let mask = contents::SOLID | contents::MOVEABLE | contents::WINDOW | contents::GRATE;
    let low = world.trace_ray(source + glam::Vec3::Z * 4.0, listener, mask);
    if !low.hit() { return 1.0; }
    let high = world.trace_ray(source + glam::Vec3::Z * 32.0, listener, mask);
    if !high.hit() { return 0.65; }
    // Provisional transmission gains, not a calibrated Legacy acoustic model.
    // Compare eye height to an upright source, not to its feet: ordinary
    // same-floor footsteps already sit roughly 64 units below the listener.
    let separate_floor = (listener.z - (source.z + 64.0)).abs() > 96.0;
    let transmission = |start: glam::Vec3, hit: collision::Trace| -> f32 {
        let porous = hit.contents & (contents::WINDOW | contents::GRATE) != 0
            && hit.contents & (contents::SOLID | contents::MOVEABLE) == 0;
        // A nearby grate/window must not conceal a solid wall or floor later
        // on the same ray. Query structural contents separately only when the
        // first hit is porous; keep the existing open/low-obstacle paths.
        if porous && !world.trace_ray(start, listener, contents::SOLID | contents::MOVEABLE).hit() {
            0.65
        } else if separate_floor { 0.18 } else { 0.32 }
    };
    transmission(source + glam::Vec3::Z * 4.0, low)
        .max(transmission(source + glam::Vec3::Z * 32.0, high))
}

/// Where and on whose behalf a sound plays.
#[derive(Clone, Copy, Debug, Default)]
pub struct Emit {
    /// Source-space position; `None` plays at the listener (own sounds).
    pub at: Option<glam::Vec3>,
    /// Entity that owns the sound, for channel stealing and limits.
    pub owner: Option<u32>,
    /// The local player's own sound (selects the "local player" mixes).
    pub local: bool,
}

impl Emit {
    pub fn at(pos: glam::Vec3) -> Self {
        Emit { at: Some(pos), ..default() }
    }
}

/// Listener position (Bevy space), updated every frame.
#[derive(Resource, Default)]
pub struct Listener(pub Vec3);

struct Playing {
    entity: Entity,
    owner: u32,
    channel: u8,
    entry: String,
    request_clock: f64,
    player_observed: bool,
    sink_observed: bool,
    deferred_limit: Option<usize>,
}

/// Opt-in diagnostic of a deferred playback request. A sink observed in a
/// later Update proves only that Bevy created it, never the shot tick or PCM onset.
#[derive(Component)]
pub struct SoundTrace {
    requested_at: Instant,
    request_clock: f64,
    sink_observed: bool,
}

impl SoundTrace {
    fn observe_sink(&mut self, present: bool) -> bool {
        if !present || self.sink_observed { return false; }
        self.sink_observed = true;
        true
    }
}

#[derive(Resource, Default)]
pub struct SoundLibrary {
    entries: HashMap<String, Entry>,
    loaded: HashMap<String, Option<(Handle<AudioSource>, u16)>>,
    playing: Vec<Playing>,
    rng: u64,
    clock: f64,
    listener: Vec3,
    classic: classic::Profile,
    /// `CSR_SOUND_LOG=1`: log every sound start (compare with the real
    /// game's `snd_showstart`, see docs/FIDELITY.md).
    log: bool,
    trace: bool,
    pub master_volume: f32,
}

/// Unwraps supported audio containers, or returns `None`.
///
/// Bevy panics on undecodable audio, so every file is checked first:
/// PCM (8/16/24/32-bit int, 32-bit float) WAVs pass through, MP3-in-WAV
/// (format 0x55, common in Valve content) is unwrapped to its raw MP3
/// frames, bare MP3s pass through, anything else is skipped.
/// This identifies the container; `source` also validates it with the decoder.
pub fn playable(bytes: Vec<u8>) -> Option<Vec<u8>> {
    if bytes.starts_with(b"ID3") || (bytes.len() > 2 && bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0) {
        return Some(bytes);
    }
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let u16_at = |o: usize| bytes.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let mut format = None;
    let mut data = None;
    let mut p = 12;
    while p + 8 <= bytes.len() {
        let id = &bytes[p..p + 4];
        let size = u32_at(p + 4)? as usize;
        let body = p + 8;
        match id {
            b"fmt " => {
                let mut tag = u16_at(body)?;
                let bits = u16_at(body + 14)?;
                // WAVE_FORMAT_EXTENSIBLE: the real tag is the sub-format GUID's first word.
                if tag == 0xFFFE {
                    tag = u16_at(body + 24)?;
                }
                format = Some((tag, bits));
            }
            b"data" => data = Some((body, size.min(bytes.len().saturating_sub(body)))),
            _ => {}
        }
        p = body + size + (size & 1);
    }
    match (format?, data?) {
        ((1, 8 | 16 | 24 | 32), _) | ((3, 32), _) => Some(bytes),
        ((0x55, _), (start, len)) => Some(bytes[start..start + len].to_vec()),
        _ => None,
    }
}

fn decoded_audio(bytes: Vec<u8>) -> Option<(Vec<u8>, u16)> {
    use rodio::Source;
    let bytes = playable(bytes)?;
    let decoder = rodio::Decoder::builder().with_byte_len(bytes.len() as u64)
        .with_data(std::io::Cursor::new(bytes.clone())).build().ok()?;
    let channels = decoder.channels().get();
    Some((bytes, channels))
}

// rodio 0.22 ChannelVolume sums input channels without applying its intended
// average. Compensate only for spatial playback; local stereo stays intact.
fn spatial_normalization(channels: u16) -> f32 { 1.0 / channels.max(1) as f32 }

fn strip_flags(wave: &str) -> String {
    let w = wave.trim_start_matches(|c: char| "*#@><^)}$!?&~`+%(".contains(c));
    let normalized = w.replace('\\', "/").to_ascii_lowercase();
    format!("sound/{}", normalized.split('/').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("/"))
}

fn parse_entry(b: &kv::Block) -> Option<Entry> {
    let mut waves = Vec::new();
    if let Some(w) = b.str("wave") {
        waves.push(strip_flags(w));
    }
    if let Some(r) = b.block("rndwave") {
        for (_, w) in r.iter() {
            if let Some(w) = w.as_str() {
                waves.push(strip_flags(w));
            }
        }
    }
    if waves.is_empty() {
        return None;
    }
    let volume = b.str("volume").map_or((1.0, 1.0), |v| {
        parse_range(v, &|s| s.eq_ignore_ascii_case("VOL_NORM").then_some(1.0))
    });
    let pitch = b.str("pitch").map_or((100.0, 100.0), |v| {
        parse_range(v, &|s| match s.to_ascii_uppercase().as_str() {
            "PITCH_NORM" => Some(100.0),
            "PITCH_LOW" => Some(95.0),
            "PITCH_HIGH" => Some(120.0),
            _ => None,
        })
    });
    // Operator stacks: the imported stacks pick the distance curve and the
    // start-stack limit; `play_distant_version_*` names the distant layer.
    let mut imports = Vec::new();
    let mut distant = None;
    if let Some(stacks) = b.block("operator_stacks") {
        for (_, stack) in stacks.iter() {
            let Some(stack) = stack.as_block() else { continue };
            for (key, value) in stack.iter() {
                if key.eq_ignore_ascii_case("import_stack") {
                    if let Some(name) = value.as_str() {
                        imports.push(name.to_ascii_lowercase());
                    }
                } else if key.to_ascii_lowercase().starts_with("play_distant_version") {
                    distant = value.as_block().and_then(|v| v.str("entry_name")).map(str::to_ascii_lowercase);
                }
            }
        }
    }
    let has = |s: &str| imports.iter().any(|i| i == s);
    let first = waves[0].trim_start_matches("sound/");
    let category = if has("cs_update_weapon") {
        Category::Weapon
    } else if has("cs_update_footsteps") {
        Category::Footsteps
    } else if has("cs_update_suit") {
        Category::Suit
    } else if has("cs_update_rics") || has("cs_update_physics") {
        Category::Impacts
    } else if has("cs_update_foley") {
        Category::Foley
    } else if first.starts_with("radio/") {
        Category::Announcer
    } else if first.starts_with("ui/") {
        Category::Ui
    } else if first.contains("/vo/") || first.starts_with("vo/") {
        Category::Voice
    } else if first.starts_with("ambient/") {
        Category::Ambient
    } else {
        Category::Plain
    };
    let limit = if has("cs_distance_weapon_start") {
        Some(2)
    } else if has("cs_limit_start") || has("cs_limit_foley_start") {
        Some(1)
    } else if has("cs_limit_footsteps_start") {
        Some(3)
    } else if has("cs_limit_bullet_impact") {
        Some(5)
    } else if has("cs_limit_physics_start") {
        Some(7)
    } else if has("cs_limit_suit_start") {
        Some(0)
    } else {
        None
    };
    let mut weapon_falloff = (category == Category::Weapon).then(WeaponFalloff::default);
    let mut foley_bounds = None;
    let mut xfade_bounds = (800.0, 2800.0);
    let mut explicit_near_fade = false;
    if let Some(update) = b.block("operator_stacks").and_then(|s| s.block("update_stack")) {
        let literal = |block: Option<&kv::Block>, field: &str| block.and_then(|v| v.str(field))
            .and_then(kv::parse_f32).filter(|v| v.is_finite());
        if category == Category::Foley {
            // The installed helmet cue overrides CS_update_foley's bounds.
            // Retain those declarations without changing the fitted curve,
            // local gain, mixer or the defaults of other foley entries.
            if let Some(falloff) = update.block("volume_falloff") {
                let bound = |field, default| falloff.get(field).map_or(Some(default), |value|
                    value.as_str().and_then(kv::parse_f32).filter(|value| value.is_finite()));
                if let (Some(min), Some(max)) = (bound("input_min", 400.0), bound("input_max", 1100.0)) {
                    if min >= 0.0 && max > min { foley_bounds = Some((min, max)); }
                }
            }
        }
        if let Some(curve) = &mut weapon_falloff {
            let falloff = update.block("volume_falloff");
            let min = literal(falloff, "input_min").or_else(|| literal(update.block("volume_falloff_min"), "input2")).unwrap_or(curve.min);
            let max = literal(falloff, "input_max").or_else(|| literal(update.block("volume_falloff_max"), "input2")).unwrap_or(curve.max);
            if min >= 0.0 && max > min { curve.min = min; curve.max = max; }
            if let Some(floor) = literal(falloff, "input_volume_min").filter(|v| (0.0..=1.0).contains(v)) { curve.floor = floor; }
            if let Some(amount) = literal(falloff, "input_curve_amount").filter(|v| *v >= 0.0) { curve.curve_amount = amount; }
            let start = literal(update.block("volume_xfade_start_dist"), "input2").unwrap_or(xfade_bounds.0);
            let end = literal(update.block("volume_xfade_end_dist"), "input2").unwrap_or(xfade_bounds.1);
            if start >= 0.0 && end > start { xfade_bounds = (start, end); }
            let remap = update.block("volume_distance_xfade");
            explicit_near_fade = literal(remap, "input_map_min") == Some(0.0) && literal(remap, "input_map_max") == Some(1.0);
        }
    }
    Some(Entry {
        waves,
        volume,
        pitch,
        soundlevel: b.str("soundlevel").map_or(75.0, parse_soundlevel),
        channel: b.str("channel").map_or(0, channel_number),
        category,
        weapon_falloff, foley_bounds, xfade_bounds, explicit_near_fade,
        distant,
        limit,
        distant_layer: false,
        frozen_wave: None,
    })
}

impl SoundLibrary {
    pub fn load(fs: &vfs::Vfs) -> Self {
        Self::load_with_era(fs, SoundEra::default())
    }

    /// Explicit installed-era mode supports measured comparisons; players use
    /// the classic default without configuring files or acquiring extra assets.
    pub fn load_with_era(fs: &vfs::Vfs, era: SoundEra) -> Self {
        let mut lib = SoundLibrary {
            master_volume: 0.5,
            rng: 0x9E37_79B9_7F4A_7C15,
            log: std::env::var_os("CSR_SOUND_LOG").is_some(),
            trace: std::env::var_os("CSR_AUDIO_TRACE").is_some(),
            ..Default::default()
        };
        let mut files: Vec<String> = Vec::new();
        if let Some(manifest) = fs.read_string("scripts/game_sounds_manifest.txt").and_then(|t| kv::parse(&t).ok()) {
            for (_, v) in manifest.iter() {
                if let Some(b) = v.as_block() {
                    for (k, f) in b.iter() {
                        if k.to_ascii_lowercase().starts_with("precache_file") || k.eq_ignore_ascii_case("preload_file") {
                            if let Some(f) = f.as_str() {
                                files.push(f.to_string());
                            }
                        }
                    }
                }
            }
        }
        if files.is_empty() {
            files.push("scripts/game_sounds_weapons.txt".into());
        }
        for file in files {
            let Some(doc) = fs.read_string(&file).and_then(|t| kv::parse(&t).ok()) else { continue };
            for (name, v) in doc.iter() {
                let Some(entry) = v.as_block().and_then(parse_entry) else { continue };
                lib.entries.insert(name.to_ascii_lowercase(), entry);
            }
        }
        let distant: Vec<String> = lib.entries.values().filter_map(|e| e.distant.clone()).collect();
        for name in distant {
            if let Some(e) = lib.entries.get_mut(&name) {
                e.distant_layer = true;
            }
        }
        lib.classic = classic::Profile::load(fs, &mut lib.entries, era);
        utility::import(&mut lib);
        lib
    }

    pub fn era(&self) -> SoundEra { self.classic.era() }

    /// Only stock canonical definitions opt into verified classic waveforms.
    /// Catalog bindings and public soundscript entries remain unchanged.
    pub fn weapon_shot_entry<'a>(&'a self, weapon: &'a weapons::WeaponDefinition) -> &'a str {
        self.classic.select(&weapon.id, &weapon.fire_sound).unwrap_or(&weapon.fire_sound)
    }

    /// Reliable shot events carry the mode at fire time, independent of a later
    /// snapshot's active weapon. Classic overrides apply only to matching bindings.
    pub fn weapon_shot_mode_entry<'a>(&'a self, weapon: &'a weapons::WeaponDefinition, suppressed: bool) -> &'a str {
        let binding = if suppressed || !weapon.attr_f32("has silencer").is_some_and(|v| v > 0.) {
            &weapon.fire_sound
        } else { &weapon.unsuppressed_fire_sound };
        self.classic.select(&weapon.id, binding).unwrap_or(binding)
    }

    /// Soundscript entry names (lower case).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// The files an entry picks from.
    pub fn waves(&self, name: &str) -> Option<&[String]> {
        self.entries.get(&name.to_ascii_lowercase()).map(|e| e.waves.as_slice())
    }

    /// Distant partner identity for waveform audits; no entry is inferred.
    pub fn distant_entry(&self, name: &str) -> Option<&str> {
        self.entries.get(&name.to_ascii_lowercase()).and_then(|entry| entry.distant.as_deref())
    }

    /// The mixing category an entry falls into (for audits and tests).
    pub fn category(&self, name: &str) -> Option<Category> {
        self.entries.get(&name.to_ascii_lowercase()).map(|e| e.category)
    }

    pub fn has(&self, name: &str) -> bool {
        self.entries.contains_key(&name.to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn random(&mut self) -> f32 {
        // xorshift64*: cheap, and sounds need no reproducibility.
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    }

    pub(crate) fn source(&mut self, fs: &vfs::Vfs, sources: &mut Assets<AudioSource>, path: &str) -> Option<(Handle<AudioSource>, u16)> {
        self.loaded
            .entry(path.to_string())
            .or_insert_with(|| {
                let (bytes, channels) = decoded_audio(fs.read(path)?)?;
                Some((sources.add(AudioSource { bytes: Arc::from(bytes) }), channels))
            })
            .clone()
    }

    /// Plays a soundscript entry. `at` is a Source-space position for
    /// spatial sounds, `None` for sounds heard at the listener (own gun).
    pub fn play(
        &mut self,
        commands: &mut Commands,
        fs: &vfs::Vfs,
        sources: &mut Assets<AudioSource>,
        name: &str,
        at: Option<glam::Vec3>,
    ) {
        self.play_with(commands, fs, sources, name, Emit { at, ..default() });
    }

    /// Play the decoy's captured weapon cue with the same verified era selection
    /// as a normal shot. Missing classic data and InstalledLegacy keep the
    /// public cue; the decoy remains spatial and has no player voice owner.
    pub fn play_decoy(
        &mut self,
        commands: &mut Commands,
        fs: &vfs::Vfs,
        sources: &mut Assets<AudioSource>,
        weapon: &weapons::WeaponDefinition,
        pos: glam::Vec3,
    ) {
        let entry = self.weapon_shot_entry(weapon).to_string();
        self.play(commands, fs, sources, &entry, Some(pos));
    }

    /// Decoy expiry shares the observed installed HE-style near/distant cue.
    pub fn play_decoy_expiry(
        &mut self, commands: &mut Commands, fs: &vfs::Vfs,
        sources: &mut Assets<AudioSource>, pos: glam::Vec3,
    ) {
        self.play(commands, fs, sources, &utility::decoy_expiry_id().to_string(), Some(pos));
    }

    /// Contact audio is spatial even for a grenade thrown by the listener.
    /// Imported entries retain authored gain, pitch and soundlevel unchanged.
    pub fn play_grenade_bounce(
        &mut self, commands: &mut Commands, fs: &vfs::Vfs,
        sources: &mut Assets<AudioSource>, kind: sim::utility::Kind, pos: glam::Vec3,
    ) {
        if let Some(id) = utility::bounce_id(kind) {
            self.play(commands, fs, sources, &id.to_string(), Some(pos));
        }
    }

    /// Plays an entry for an owner: a new sound on a single-sound channel
    /// replaces the owner's previous one, and stacks with a limit stop the
    /// oldest instances. Gunfire also starts its distant layer.
    pub fn play_with(
        &mut self,
        commands: &mut Commands,
        fs: &vfs::Vfs,
        sources: &mut Assets<AudioSource>,
        name: &str,
        emit: Emit,
    ) {
        let key = name.to_ascii_lowercase();
        let Some(entry) = self.entries.get(&key).cloned() else { return };
        let defer_limit = entry.category == Category::Weapon && matches!(entry.channel, 0 | 6) && entry.limit.is_some();
        if defer_limit && emit.owner.is_some_and(|owner| self.playing.iter().filter(|p| p.owner == owner && p.entry == key).count() >= MAX_PENDING_GUN_VOICES) {
            if self.trace { info!("audio_trace pending_gun_voice_bound owner {:?} entry {key:?}", emit.owner); }
            return;
        }
        if let Some(distant) = entry.distant.as_deref().filter(|_| emit.at.is_some()) {
            self.play_with(commands, fs, sources, distant, emit);
        }
        let pick = ((self.random() * entry.waves.len() as f32) as usize).min(entry.waves.len() - 1);
        let audio = if let Some(wave) = &entry.frozen_wave {
            // This reserved key cannot collide with a VFS path. Keep verified
            // bytes even if the selected BSP/pack later shadows the old path.
            self.loaded.entry(format!("classic2015:{key}"))
                .or_insert_with(|| Some((sources.add(AudioSource { bytes: wave.bytes.clone() }), wave.channels)))
                .clone()
        } else {
            self.source(fs, sources, &entry.waves[pick])
        };
        let Some((handle, channels)) = audio else { return };
        let mut lerp = |(low, high): (f32, f32)| low + (high - low) * self.random();
        let volume = lerp(entry.volume).clamp(0.0, 1.0);
        let speed = (lerp(entry.pitch) / 100.0).clamp(0.25, 4.0);

        if let Some(owner) = emit.owner {
            self.steal(commands, owner, &entry, &key);
        }
        if self.log {
            let distance = emit.at.map_or(0.0, |p| to_bevy(p).distance(self.listener));
            info!(
                "sound {:.3} {name} {} owner {:?} {:?} vol {volume:.2} pitch {:.0} dist {distance:.0}",
                self.clock, entry.waves[pick], emit.owner, entry.category, speed * 100.0
            );
        }
        let initial_gain;
        let entity = match emit.at {
            Some(pos) => {
                let emitter = Emitter {
                    pos: to_bevy(pos),
                    base: volume * entry.category.field() * spatial_normalization(channels),
                    category: entry.category,
                    soundlevel: entry.soundlevel,
                    gunfire: (entry.category == Category::Weapon && (entry.distant.is_some() || entry.distant_layer || entry.explicit_near_fade))
                        .then_some(GunfireFade { start: entry.xfade_bounds.0, end: entry.xfade_bounds.1, distant: entry.distant_layer }),
                    weapon_falloff: entry.weapon_falloff,
                    foley_bounds: entry.foley_bounds,
                };
                let occluded = matches!(entry.category, Category::Footsteps | Category::Suit);
                let gain = if occluded { 0.0 } else { emitter.gain(emitter.pos.distance(self.listener)) };
                initial_gain = gain * self.master_volume;
                let mut entity = commands
                    .spawn((
                        AudioPlayer(handle),
                        SoundGain(emitter.base),
                        emitter,
                        PlaybackSettings { paused: occluded, ..PlaybackSettings::DESPAWN }
                            .with_volume(Volume::Linear(gain * self.master_volume))
                            .with_speed(speed)
                            .with_spatial(true)
                            .with_spatial_scale(SpatialScale::new(PAN_ONLY_SCALE)),
                        Transform::from_translation(emitter.pos),
                    ));
                if occluded { entity.insert(FootstepOcclusion::default()); }
                entity.id()
            }
            None => {
                // Measured gains top out at lvol 20 (1.07 of the AK shot).
                let gain = (if emit.local { self.classic.local_gain(&key) } else { None })
                    .unwrap_or_else(|| volume * entry.category.near(emit.local)).min(1.07);
                initial_gain = gain * self.master_volume;
                commands
                    .spawn((
                        AudioPlayer(handle),
                        SoundGain(gain),
                        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain * self.master_volume)).with_speed(speed),
                    ))
                    .id()
            }
        };
        if self.trace {
            commands.entity(entity).insert(SoundTrace {
                requested_at: Instant::now(), request_clock: self.clock, sink_observed: false,
            });
            info!("audio_trace request entity {entity:?} clock {:.6} entry {key:?} wave {:?} owner {:?} channel {} pitch {:.3} cue_volume {volume:.6} master {:.6} initial_gain {initial_gain:.6} spatial {}",
                self.clock, entry.waves[pick], emit.owner, entry.channel, speed * 100.0,
                self.master_volume, emit.at.is_some());
        }
        if let Some(owner) = emit.owner {
            self.playing.push(Playing { entity, owner, channel: entry.channel, entry: key,
                request_clock: self.clock, player_observed: false, sink_observed: false,
                deferred_limit: defer_limit.then(|| entry.limit.unwrap().max(1)) });
        }
    }

    /// Channel stealing and per-entry instance limits for one owner.
    fn steal(&mut self, commands: &mut Commands, owner: u32, entry: &Entry, key: &str) {
        let single_channel = !matches!(entry.channel, 0 | 6);
        let mut same: Vec<usize> = Vec::new();
        let mut stop: Vec<usize> = Vec::new();
        for (i, p) in self.playing.iter().enumerate() {
            if p.owner != owner {
                continue;
            }
            if single_channel && p.channel == entry.channel {
                stop.push(i);
            } else if p.entry == key {
                same.push(i);
            }
        }
        if let Some(limit) = entry.limit.filter(|_| entry.category != Category::Weapon || !matches!(entry.channel, 0 | 6)) {
            // `limit` counts the new sound too: stop the oldest until it fits.
            let excess = (same.len() + 1).saturating_sub(limit.max(1));
            stop.extend(same.into_iter().take(excess));
        }
        stop.sort_unstable();
        for i in stop.into_iter().rev() {
            let p = self.playing.remove(i);
            if self.trace {
                info!("audio_trace steal_command entity {:?} clock {:.6} entry {:?} owner {} channel {} replacement {key:?}",
                    p.entity, self.clock, p.entry, p.owner, p.channel);
            }
            commands.entity(p.entity).try_despawn();
        }
    }
}

/// Follows the listener and re-applies distance gains to world sounds.
/// A volume ramp (soundscape cross-fades). Fading out despawns at the end.
#[derive(Component, Clone, Copy)]
pub struct Fade {
    start: f64,
    duration: f32,
    out: bool,
}

impl Fade {
    fn value(&self, clock: f64) -> f32 {
        let t = (((clock - self.start) as f32) / self.duration.max(0.001)).clamp(0.0, 1.0);
        if self.out { 1.0 - t } else { t }
    }
}

/// A sound described directly (not by a soundscript entry), as soundscapes
/// do: one wave with its own volume, pitch and soundlevel.
#[derive(Clone, Copy, Debug)]
pub struct Raw {
    pub at: Option<glam::Vec3>,
    pub volume: f32,
    pub pitch: f32,
    pub soundlevel: f32,
    pub looping: bool,
    /// Fade in over this many seconds (0 = start at full volume).
    pub fade_in: f32,
}

impl SoundLibrary {
    /// Plays one wave file as ambience; returns the entity so the caller
    /// can fade it out later.
    pub fn play_raw(&mut self, commands: &mut Commands, fs: &vfs::Vfs, sources: &mut Assets<AudioSource>, wave: &str, raw: Raw) -> Option<Entity> {
        self.play_wave(commands, fs, sources, wave, raw, Category::Ambient)
    }

    pub fn play_ui_wave(&mut self, commands: &mut Commands, fs: &vfs::Vfs, sources: &mut Assets<AudioSource>, wave: &str) -> Option<Entity> {
        self.play_wave(commands, fs, sources, wave, Raw { at: None, volume: 1.0, pitch: 100.0, soundlevel: 0.0, looping: false, fade_in: 0.0 }, Category::Ui)
    }

    fn play_wave(&mut self, commands: &mut Commands, fs: &vfs::Vfs, sources: &mut Assets<AudioSource>, wave: &str, raw: Raw, category: Category) -> Option<Entity> {
        let path = strip_flags(wave);
        let (handle, channels) = self.source(fs, sources, &path)?;
        let base = PlaybackSettings { mode: if raw.looping { bevy::audio::PlaybackMode::Loop } else { bevy::audio::PlaybackMode::Despawn }, ..PlaybackSettings::DESPAWN }
            .with_speed((raw.pitch / 100.0).clamp(0.25, 4.0));
        let fade = (raw.fade_in > 0.0).then_some(Fade { start: self.clock, duration: raw.fade_in, out: false });
        let start = if fade.is_some() { 0.0 } else { 1.0 };
        if self.log {
            info!("ambient {:.3} {path} vol {:.2} at {:?}", self.clock, raw.volume, raw.at);
        }
        let mut e = match raw.at {
            Some(pos) => {
                let emitter = Emitter { pos: to_bevy(pos), base: raw.volume * category.field() * spatial_normalization(channels), category, soundlevel: raw.soundlevel, gunfire: None, weapon_falloff: None, foley_bounds: None };
                let gain = emitter.gain(emitter.pos.distance(self.listener)) * start;
                commands.spawn((
                    AudioPlayer(handle),
                    SoundGain(emitter.base),
                    emitter,
                    base.with_volume(Volume::Linear(gain * self.master_volume)).with_spatial(true).with_spatial_scale(SpatialScale::new(PAN_ONLY_SCALE)),
                    Transform::from_translation(emitter.pos),
                ))
            }
            None => {
                let gain = raw.volume * category.near(false);
                commands.spawn((AudioPlayer(handle), SoundGain(gain), base.with_volume(Volume::Linear(gain * start * self.master_volume))))
            }
        };
        if let Some(fade) = fade {
            e.insert(fade);
        }
        Some(e.id())
    }

    /// Fades a sound out over `seconds`, then despawns it.
    pub fn fade_out(&self, commands: &mut Commands, entity: Entity, seconds: f32) {
        if let Ok(mut e) = commands.get_entity(entity) {
            e.insert(Fade { start: self.clock, duration: seconds, out: true });
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn update(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut sounds: ResMut<SoundLibrary>,
    mut listener: ResMut<Listener>,
    world: Option<Res<crate::player::SimWorld>>,
    ears: Query<&GlobalTransform, With<SpatialListener>>,
    mut spatial: Query<(&Emitter, Option<&Fade>, Option<&mut SpatialAudioSink>,
        &mut PlaybackSettings, Option<&mut FootstepOcclusion>)>,
    mut faded: Query<(Entity, &SoundGain, &Fade, Option<&mut AudioSink>, Has<Emitter>)>,
    mut traces: Query<(Entity, &mut SoundTrace, Has<AudioSink>, Has<SpatialAudioSink>)>,
) {
    let _frame_scope = crate::input_latency::frame_scope("audio.update");
    sounds.clock += time.delta_secs_f64();
    if let Some(ear) = ears.iter().next() {
        listener.0 = ear.translation();
    }
    sounds.listener = listener.0;
    let master = sounds.master_volume;
    let clock = sounds.clock;
    for (entity, mut trace, ordinary, spatial) in &mut traces {
        if trace.observe_sink(ordinary || spatial) {
            info!("audio_trace sink_first_observed entity {entity:?} clock {clock:.6} request_clock {:.6} request_elapsed_ms {:.3} spatial {spatial}",
                trace.request_clock, trace.requested_at.elapsed().as_secs_f64() * 1000.0);
        }
    }
    for (emitter, fade, sink, mut settings, occlusion) in &mut spatial {
        if sink.is_none() && occlusion.is_none() { continue; }
        let mut first_check = false;
        let transmission = if let Some(mut occlusion) = occlusion {
            first_check = occlusion.next_check == 0.0;
            if clock >= occlusion.next_check {
                occlusion.gain = world.as_ref().map_or(1.0, |world|
                    footstep_occlusion(&world.0.collision, emitter.pos, listener.0));
                occlusion.next_check = clock + 0.1;
            }
            occlusion.gain
        } else { 1.0 };
        let gain = emitter.gain(emitter.pos.distance(listener.0)) * transmission
            * fade.map_or(1.0, |f| f.value(clock));
        if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(gain * master));
            if first_check { sink.play(); }
        } else {
            settings.volume = Volume::Linear(gain * master);
        }
        if first_check { settings.paused = false; }
    }
    for (entity, gain, fade, sink, is_spatial) in &mut faded {
        if fade.out && fade.value(clock) <= 0.0 {
            commands.entity(entity).try_despawn();
            continue;
        }
        if let (Some(mut sink), false) = (sink, is_spatial) {
            sink.set_volume(Volume::Linear(gain.0 * fade.value(clock) * master));
        }
    }
}

/// Reap tracked voices after the frame's producer and playback commands flush.
///
/// FixedUpdate/Update can reserve an AudioPlayer before it is query-visible.
/// Last runs after their final deferred flush and PostUpdate's audio cleanup.
/// Do not retain missing, never-observed players: cancelled requests must also
/// disappear. A future Last audio producer must flush before this system.
pub fn prune(mut commands: Commands, mut sounds: ResMut<SoundLibrary>,
    active: Query<(Has<AudioSink>, Has<SpatialAudioSink>), With<AudioPlayer>>) {
    let _frame_scope = crate::input_latency::frame_scope("audio.prune");
    let trace_enabled = sounds.trace;
    let prune_clock = sounds.clock;
    let mut eligible = HashSet::new();
    sounds.playing.retain_mut(|p| {
        let present = active.get(p.entity).ok();
        if trace_enabled && present.is_none() {
            info!("audio_trace prune_missing_player entity {:?} clock {prune_clock:.6} request_clock {:.6} entry {:?} owner {} channel {} player_previously_observed {} stage last",
                p.entity, p.request_clock, p.entry, p.owner, p.channel, p.player_observed);
        }
        p.player_observed |= present.is_some();
        if let Some((ordinary, spatial)) = present {
            // Never delete an unresolved or newly observed start in this turn.
            // A sink is not a measured PCM onset; catch-up can still bunch starts.
            if p.sink_observed && (ordinary || spatial) { eligible.insert(p.entity); }
            p.sink_observed = ordinary || spatial;
        }
        present.is_some()
    });
    let mut counts: HashMap<(u32, String), (usize, usize)> = HashMap::new();
    for p in &sounds.playing {
        if let Some(limit) = p.deferred_limit.filter(|_| p.sink_observed) {
            let group = counts.entry((p.owner, p.entry.clone())).or_insert((0, limit));
            group.0 += 1;
        }
    }
    let mut stopped = Vec::new();
    for (index, p) in sounds.playing.iter().enumerate() {
        if !eligible.contains(&p.entity) { continue; }
        if let Some((count, limit)) = counts.get_mut(&(p.owner, p.entry.clone())) {
            if *count > *limit { *count -= 1; stopped.push(index); }
        }
    }
    for index in stopped.into_iter().rev() {
        let p = sounds.playing.remove(index);
        if trace_enabled { info!("audio_trace limit_after_sink entity {:?} clock {prune_clock:.6} entry {:?} owner {}", p.entity, p.entry, p.owner); }
        commands.entity(p.entity).try_despawn();
    }
}

#[cfg(test)]
mod tests {
    fn acoustic_box(mins: glam::Vec3, maxs: glam::Vec3, contents: i32) -> collision::Convex {
        use collision::Plane;
        use glam::Vec3;
        collision::Convex {
            planes: vec![
                Plane { normal: Vec3::X, dist: maxs.x }, Plane { normal: -Vec3::X, dist: -mins.x },
                Plane { normal: Vec3::Y, dist: maxs.y }, Plane { normal: -Vec3::Y, dist: -mins.y },
                Plane { normal: Vec3::Z, dist: maxs.z }, Plane { normal: -Vec3::Z, dist: -mins.z },
            ], mins, maxs, contents, surface: 0,
        }
    }

    #[test]
    fn footsteps_distinguish_solid_floors_walls_and_open_vertical_paths() {
        use collision::{CollisionWorld, contents};
        use glam::Vec3;
        let gain = |world: &CollisionWorld, feet, eye| super::footstep_occlusion(world, to_bevy(feet), to_bevy(eye));
        let mut world = CollisionWorld::new();
        let lower = Vec3::ZERO;
        let upper = Vec3::Z * 256.0;
        assert_eq!(gain(&world, upper, lower + Vec3::Z * 64.0), 1.0);
        world.add_convex(acoustic_box(Vec3::new(-512.0, -512.0, 128.0),
            Vec3::new(512.0, 512.0, 144.0), contents::SOLID));
        world.finalize();
        assert_eq!(gain(&world, upper, lower + Vec3::Z * 64.0), 0.18);
        assert_eq!(gain(&world, lower, upper + Vec3::Z * 64.0), 0.18);
        assert_eq!(gain(&world, lower, Vec3::new(100.0, 0.0, 64.0)), 1.0);
        // Same-floor walls muffle steps; movement-only clips do not. A small
        // obstacle and a grate transmit more than a full-height solid wall.
        for (contents, top, expected) in [(contents::SOLID, 128.0, 0.32),
            (contents::PLAYERCLIP, 128.0, 1.0), (contents::GRATE, 128.0, 0.65),
            (contents::SOLID, 44.0, 0.65)] {
            let mut world = CollisionWorld::new();
            world.add_convex(acoustic_box(Vec3::new(48.0, -64.0, -16.0),
                Vec3::new(56.0, 64.0, top), contents));
            world.finalize();
            assert_eq!(gain(&world, lower, Vec3::new(100.0, 0.0, 64.0)), expected);
        }
    }

    #[test]
    fn porous_surfaces_do_not_hide_solid_walls_or_floors() {
        use collision::{CollisionWorld, contents};
        use glam::Vec3;
        let gain = |world: &CollisionWorld, feet, eye|
            super::footstep_occlusion(world, to_bevy(feet), to_bevy(eye));
        for porous in [contents::GRATE, contents::WINDOW] {
            let mut wall = CollisionWorld::new();
            wall.add_convex(acoustic_box(Vec3::new(24., -64., -16.),
                Vec3::new(28., 64., 128.), porous));
            wall.add_convex(acoustic_box(Vec3::new(48., -64., -16.),
                Vec3::new(56., 64., 128.), contents::SOLID));
            wall.finalize();
            assert_eq!(gain(&wall, Vec3::ZERO, Vec3::new(100., 0., 64.)), 0.32);
            assert_eq!(gain(&wall, Vec3::new(100., 0., 0.), Vec3::Z * 64.), 0.32);

            let mut floor = CollisionWorld::new();
            floor.add_convex(acoustic_box(Vec3::new(-512., -512., 220.),
                Vec3::new(512., 512., 224.), porous));
            floor.add_convex(acoustic_box(Vec3::new(-512., -512., 128.),
                Vec3::new(512., 512., 144.), contents::SOLID));
            floor.finalize();
            assert_eq!(gain(&floor, Vec3::Z * 256., Vec3::Z * 64.), 0.18);
            assert_eq!(gain(&floor, Vec3::ZERO, Vec3::Z * 320.), 0.18);
        }
    }

    #[test]
    fn queued_steps_get_occlusion_before_playback_without_changing_combat_or_local_audio() {
        use collision::contents;
        use glam::Vec3 as SourceVec;
        let mut collision = collision::CollisionWorld::new();
        collision.add_convex(acoustic_box(SourceVec::new(-512.0, -512.0, 128.0),
            SourceVec::new(512.0, 512.0, 144.0), contents::SOLID));
        collision.finalize();
        let mut app = App::new();
        app.init_resource::<Time<Real>>().init_resource::<SoundLibrary>()
            .init_resource::<Assets<AudioSource>>()
            .insert_resource(Listener(to_bevy(SourceVec::Z * 64.0)))
            .insert_resource(crate::player::SimWorld(sim::World::new(Arc::new(collision))));
        app.world_mut().resource_mut::<SoundLibrary>().master_volume = 1.0;
        for (name, category) in [("step", Category::Footsteps), ("suit", Category::Suit),
            ("shot", Category::Weapon), ("body", Category::Foley), ("head_feedback", Category::Foley)] {
            let mut entry = synthetic_entry(8, None);
            entry.category = category;
            entry.weapon_falloff = None;
            app.world_mut().resource_mut::<SoundLibrary>().entries.insert(name.into(), entry);
        }
        app.add_systems(FixedUpdate, |mut commands: Commands, mut sounds: ResMut<SoundLibrary>,
            mut sources: ResMut<Assets<AudioSource>>| {
            for (owner, name) in [(1, "step"), (2, "suit"), (3, "shot"), (4, "body"),
                (5, "head_feedback"), (6, "step")] {
                sounds.play_with(&mut commands, &vfs::Vfs::new(), &mut sources, name,
                    Emit { at: (owner < 5).then_some(SourceVec::Z * 256.0), owner: Some(owner), local: owner >= 5 });
            }
        }).add_systems(Update, update);
        app.world_mut().run_schedule(FixedUpdate);
        let entity = |app: &App, owner| app.world().resource::<SoundLibrary>().playing.iter()
            .find(|voice| voice.owner == owner).unwrap().entity;
        for owner in [1, 2] {
            let settings = app.world().get::<PlaybackSettings>(entity(&app, owner)).unwrap();
            assert!(settings.paused);
            assert_eq!(settings.volume.to_linear(), 0.0);
        }
        app.update(); // No AudioPlugin/device: inspect actual queued settings.
        for owner in [1, 2, 3, 4] {
            let id = entity(&app, owner);
            let emitter = app.world().get::<Emitter>(id).unwrap();
            let settings = app.world().get::<PlaybackSettings>(id).unwrap();
            assert!(!settings.paused);
            let transmission = if owner <= 2 { 0.18 } else { 1.0 };
            let expected = emitter.gain(192.0) * transmission;
            assert!((settings.volume.to_linear() - expected).abs() < 1e-6);
            assert_eq!(app.world().get::<FootstepOcclusion>(id).is_some(), owner <= 2);
        }
        for owner in [5, 6] {
            let id = entity(&app, owner);
            assert!(app.world().get::<Emitter>(id).is_none());
            assert!(app.world().get::<FootstepOcclusion>(id).is_none());
            let settings = app.world().get::<PlaybackSettings>(id).unwrap();
            assert!(!settings.paused);
            assert!(settings.volume.to_linear() > 0.0);
        }
    }

    #[test]
    fn sink_observation_requires_an_actual_sink_and_is_reported_once() {
        let mut trace = super::SoundTrace {
            requested_at: std::time::Instant::now(), request_clock: 3.25, sink_observed: false,
        };
        // A queued AudioPlayer alone cannot establish a sink or audible onset.
        assert!(!trace.observe_sink(false));
        assert!(!trace.sink_observed);
        assert!(trace.observe_sink(true));
        assert!(!trace.observe_sink(true));
        assert!(!trace.observe_sink(false));
        // A new request has its own independent observation lifecycle.
        let mut next = super::SoundTrace {
            requested_at: std::time::Instant::now(), request_clock: 3.5, sink_observed: false,
        };
        assert!(next.observe_sink(true));
        assert_eq!(trace.request_clock, 3.25);
    }

    #[test]
    fn queued_player_is_not_reported_as_an_observed_sink() {
        use super::*;
        let mut app = App::new();
        app.init_resource::<Time<Real>>()
            .init_resource::<SoundLibrary>()
            .init_resource::<Listener>()
            .add_systems(Update, update);
        let entity = app.world_mut().spawn((
            AudioPlayer::<AudioSource>(Handle::default()),
            SoundTrace { requested_at: Instant::now(), request_clock: 1.0, sink_observed: false },
        )).id();
        app.update();
        assert!(!app.world().get::<SoundTrace>(entity).unwrap().sink_observed);
        // A rodio queue is sufficient; this test never opens an audio device.
        let (player, _queue) = rodio::Player::new();
        app.world_mut().entity_mut(entity).insert(AudioSink::new(player));
        app.update();
        assert!(app.world().get::<SoundTrace>(entity).unwrap().sink_observed);
        app.world_mut().entity_mut(entity).remove::<AudioSink>();
        app.update();
        assert!(app.world().get::<SoundTrace>(entity).unwrap().sink_observed);
    }

    // Actual play_with, deferred Commands and Main schedule; no device/assets.
    #[derive(Resource, Default)]
    struct VoiceFixture {
        requests: Vec<Emit>,
        cancel: bool,
        pending_seen_in_update: usize,
    }

    fn synthetic_entry(limit: usize, distant: Option<&str>) -> Entry {
        Entry {
            waves: vec!["sound/synthetic.wav".into()], volume: (1.0, 1.0),
            pitch: (100.0, 100.0), soundlevel: 79.0, channel: 6,
            category: Category::Weapon, weapon_falloff: Some(WeaponFalloff::default()), foley_bounds: None, xfade_bounds: (800.0, 2800.0), explicit_near_fade: false, distant: distant.map(str::to_owned),
            limit: Some(limit), distant_layer: distant.is_none(),
            // No AudioPlugin is installed; only entity lifecycle is exercised.
            frozen_wave: Some(classic::FrozenWave { bytes: Arc::from([0u8; 4]), channels: 1 }),
        }
    }

    fn request_voices(mut commands: Commands, mut sounds: ResMut<SoundLibrary>,
        mut sources: ResMut<Assets<AudioSource>>, mut fixture: ResMut<VoiceFixture>) {
        for emit in std::mem::take(&mut fixture.requests) {
            sounds.play_with(&mut commands, &vfs::Vfs::new(), &mut sources, "synthetic.near", emit);
        }
        if fixture.cancel {
            for voice in &sounds.playing { commands.entity(voice.entity).try_despawn(); }
        }
    }

    fn witness_pending(sounds: Res<SoundLibrary>, active: Query<(), With<AudioPlayer>>,
        mut fixture: ResMut<VoiceFixture>) {
        fixture.pending_seen_in_update += sounds.playing.iter()
            .filter(|voice| !active.contains(voice.entity)).count();
    }

    fn voices() -> App {
        let mut app = App::new();
        app.init_resource::<Time<Real>>().init_resource::<SoundLibrary>()
            .init_resource::<Listener>().init_resource::<Assets<AudioSource>>()
            .init_resource::<VoiceFixture>();
        let mut sounds = app.world_mut().resource_mut::<SoundLibrary>();
        sounds.entries.insert("synthetic.near".into(), synthetic_entry(2, Some("synthetic.far")));
        sounds.entries.insert("synthetic.far".into(), synthetic_entry(1, None));
        drop(sounds);
        // This forces the production-observed emit -> Update observer gap:
        // the request is in Playing before Commands spawn becomes query-visible.
        app.add_systems(Update, (request_voices, update, witness_pending).chain_ignore_deferred())
            .add_systems(Last, prune);
        app
    }

    fn start_pending_voices(app: &mut App) {
        let ids: Vec<Entity> = app.world().resource::<SoundLibrary>().playing.iter()
            .filter(|p| !p.sink_observed).map(|p| p.entity).collect();
        for id in ids {
            // An actual rodio queue, without a device or a claim of PCM onset.
            let (player, _queue) = rodio::Player::new();
            app.world_mut().entity_mut(id).insert(AudioSink::new(player));
        }
    }

    fn local_voice(owner: u32) -> Emit { Emit { owner: Some(owner), local: true, ..default() } }

    #[test]
    fn pending_voice_survives_update_gap_and_is_reaped_after_real_removal() {
        let mut app = voices();
        app.world_mut().resource_mut::<VoiceFixture>().requests.push(local_voice(7));
        app.update();
        assert_eq!(app.world().resource::<VoiceFixture>().pending_seen_in_update, 1);
        let voice = &app.world().resource::<SoundLibrary>().playing[0];
        assert!(voice.player_observed, "Last saw the flushed AudioPlayer");
        let entity = voice.entity;
        assert!(app.world().get::<AudioPlayer>(entity).is_some());
        app.world_mut().entity_mut(entity).remove::<AudioPlayer>();
        app.update();
        assert!(app.world().resource::<SoundLibrary>().playing.is_empty());
    }

    #[test]
    fn actual_local_bursts_keep_two_voices_across_update_gaps() {
        let mut app = voices();
        for shot in 1..=5 {
            app.world_mut().resource_mut::<VoiceFixture>().requests.push(local_voice(7));
            app.update();
            start_pending_voices(&mut app);
            app.update();
            assert_eq!(app.world().resource::<SoundLibrary>().playing.len(), shot.min(2));
            let count = app.world_mut().query::<&AudioPlayer>().iter(app.world()).count();
            assert_eq!(count, shot.min(2), "Steal despawns are applied, not just forgotten");
        }
        assert_eq!(app.world().resource::<VoiceFixture>().pending_seen_in_update, 5);
    }

    #[test]
    fn actual_world_bursts_limit_near_and_far_per_owner() {
        let mut app = voices();
        for _ in 0..5 {
            for owner in [7, 8] {
                app.world_mut().resource_mut::<VoiceFixture>().requests.push(Emit {
                    at: Some(glam::Vec3::ZERO), owner: Some(owner), local: false,
                });
            }
            app.update();
            start_pending_voices(&mut app);
            app.update();
        }
        let sounds = app.world().resource::<SoundLibrary>();
        for owner in [7, 8] {
            assert_eq!(sounds.playing.iter().filter(|v| v.owner == owner && v.entry == "synthetic.near").count(), 2);
            assert_eq!(sounds.playing.iter().filter(|v| v.owner == owner && v.entry == "synthetic.far").count(), 1);
        }
        let count = app.world_mut().query::<&AudioPlayer>().iter(app.world()).count();
        assert_eq!(count, 6);
    }

    #[test]
    fn three_deferred_shots_keep_each_start_before_active_limit() {
        let mut app = voices();
        app.world_mut().resource_mut::<VoiceFixture>().requests.extend([local_voice(7); 3]);
        app.update();
        let ids: Vec<Entity> = app.world().resource::<SoundLibrary>().playing.iter().map(|p| p.entity).collect();
        assert_eq!(ids.len(), 3, "No third-shot request may erase an unresolved onset");
        assert!(ids.iter().all(|id| app.world().get::<AudioPlayer>(*id).is_some()));
        assert!(app.world().resource::<SoundLibrary>().playing.iter().all(|p| !p.sink_observed));
        start_pending_voices(&mut app);
        app.update();
        assert_eq!(app.world().resource::<SoundLibrary>().playing.len(), 3, "Newly observed starts receive one turn");
        app.update();
        assert_eq!(app.world().resource::<SoundLibrary>().playing.len(), 2);
        assert!(app.world().get::<AudioPlayer>(ids[0]).is_none(), "Oldest active voice retires");
        assert!(app.world().get::<AudioPlayer>(ids[1]).is_some());
        assert!(app.world().get::<AudioPlayer>(ids[2]).is_some());
    }

    #[test]
    fn a_device_without_sinks_has_bounded_pending_gunfire() {
        let mut app = voices();
        app.world_mut().resource_mut::<VoiceFixture>().requests.extend([local_voice(7); MAX_PENDING_GUN_VOICES + 1]);
        app.update();
        assert_eq!(app.world().resource::<SoundLibrary>().playing.len(), MAX_PENDING_GUN_VOICES);
        app.update();
        assert_eq!(app.world().resource::<SoundLibrary>().playing.len(), MAX_PENDING_GUN_VOICES);
    }

    #[test]
    fn suppressed_entry_keeps_literal_bounds_and_fades_without_distant_partner() {
        let doc = kv::parse(r#""Shot" { "wave" "synthetic.wav"
            "operator_stacks" { "update_stack" { "import_stack" "CS_update_weapon"
                "volume_falloff" { "input_max" "800" "input_curve_amount" "0.9" }
                "volume_xfade_end_dist" { "input2" "1500" }
                "volume_distance_xfade" { "input_map_min" "0" "input_map_max" "1" } } } }"#).unwrap();
        let entry = parse_entry(doc.block("Shot").unwrap()).unwrap();
        assert_eq!(entry.distant, None);
        assert!(entry.explicit_near_fade);
        let curve = entry.weapon_falloff.unwrap();
        assert_eq!((curve.min, curve.max, curve.floor, curve.curve_amount), (400.0, 800.0, 0.05, 0.9));
        let emitter = Emitter { pos: Vec3::ZERO, base: 1.0, category: entry.category, soundlevel: entry.soundlevel,
            weapon_falloff: entry.weapon_falloff, foley_bounds: entry.foley_bounds, gunfire: Some(GunfireFade { start: entry.xfade_bounds.0, end: entry.xfade_bounds.1, distant: false }) };
        assert_eq!(emitter.gain(100.0), 1.0);
        assert_eq!(emitter.gain(800.0), 0.05);
        assert!(emitter.gain(1100.0) < 0.05);
        assert_eq!(emitter.gain(1500.0), 0.0);
        assert_eq!(emitter.gain(5000.0), 0.0);
    }

    #[test]
    fn malformed_literal_bounds_do_not_produce_nonfinite_gain() {
        let doc = kv::parse(r#""Shot" { "wave" "synthetic.wav"
            "operator_stacks" { "update_stack" { "import_stack" "CS_update_weapon"
                "volume_falloff" { "input_min" "900" "input_max" "800" "input_volume_min" "NaN" }
                "volume_xfade_end_dist" { "input2" "-5" } } } }"#).unwrap();
        let entry = parse_entry(doc.block("Shot").unwrap()).unwrap();
        assert_eq!(entry.weapon_falloff, Some(WeaponFalloff::default()));
        assert_eq!(entry.xfade_bounds, (800.0, 2800.0));
    }

    #[test]
    fn authored_foley_bounds_reach_spatial_playback_without_changing_default_cues() {
        let doc = kv::parse(r#"
            Custom { wave synthetic.wav channel CHAN_STATIC volume 0.7
                operator_stacks { update_stack { import_stack CS_update_foley
                    volume_falloff { input_min 120 input_max 1400 } } } }
            Default { wave synthetic.wav channel CHAN_STATIC
                operator_stacks { update_stack { import_stack CS_update_foley } } }
        "#).unwrap();
        let mut app = App::new();
        app.init_resource::<SoundLibrary>().init_resource::<Assets<AudioSource>>();
        for name in ["Custom", "Default"] {
            let mut entry = parse_entry(doc.block(name).unwrap()).unwrap();
            entry.frozen_wave = Some(classic::FrozenWave { bytes: Arc::from([0u8; 4]), channels: 1 });
            app.world_mut().resource_mut::<SoundLibrary>().entries.insert(name.to_ascii_lowercase(), entry);
        }
        app.add_systems(Update, |mut commands: Commands, mut sounds: ResMut<SoundLibrary>,
            mut sources: ResMut<Assets<AudioSource>>| {
            for name in ["Custom", "Default"] {
                sounds.play_with(&mut commands, &vfs::Vfs::new(), &mut sources, name,
                    Emit { at: Some(glam::Vec3::X * 1250.0), owner: Some(7), local: false });
            }
        });
        // No AudioPlugin/device: exercise production parsing, play_with and
        // deferred entity creation, then inspect the actual spatial emitters.
        app.update();
        let sounds = app.world().resource::<SoundLibrary>();
        let emitter = |name| {
            let voice = sounds.playing.iter().find(|p| p.entry == name).unwrap();
            app.world().get::<Emitter>(voice.entity).unwrap()
        };
        let custom = emitter("custom");
        assert_eq!(custom.foley_bounds, Some((120.0, 1400.0)));
        assert_eq!(custom.gain(120.0), custom.base);
        assert!(custom.gain(1250.0) > 0.0, "Authored range must survive the old 1100-unit cutoff");
        assert_eq!(custom.gain(1400.0), 0.0);
        let default = emitter("default");
        assert_eq!(default.foley_bounds, None);
        assert_eq!(default.gain(400.0), default.base);
        assert_eq!(default.gain(1100.0), 0.0);
        assert_eq!(sounds.entries["custom"].volume, (0.7, 0.7));
        assert_eq!(sounds.entries["custom"].channel, 6);
    }

    #[test]
    fn invalid_foley_bounds_keep_default_attenuation() {
        for bounds in ["input_min 900 input_max 800", "input_min -1 input_max 1400",
            "input_min NaN input_max inf", "input_min NaN input_max 1400",
            "input_min 120 input_max inf", "input_min invalid input_max 1400",
            "input_min 120 input_max invalid", "input_min {} input_max 1400"] {
            let text = format!("Cue {{ wave synthetic.wav operator_stacks {{ update_stack {{
                import_stack CS_update_foley volume_falloff {{ {bounds} }} }} }} }}");
            let doc = kv::parse(&text).unwrap();
            let entry = parse_entry(doc.block("Cue").unwrap()).unwrap();
            let emitter = Emitter { pos: Vec3::ZERO, base: 1.0, category: entry.category,
                soundlevel: entry.soundlevel, weapon_falloff: entry.weapon_falloff,
                foley_bounds: entry.foley_bounds, gunfire: None };
            assert_eq!(emitter.gain(400.0), 1.0);
            assert_eq!(emitter.gain(1100.0), 0.0);
        }
    }

    #[test]
    fn attacker_head_feedback_reaches_listener_without_affecting_positional_body_hits() {
        use sim::{GameEvent, weapon::{HitGroup, HitProtection}};
        #[derive(Resource)]
        struct Hits(Vec<GameEvent>);
        fn app(events: Vec<GameEvent>, viewer: u32, noclip: bool) -> App {
            let doc = kv::parse(r#"
                Player.DamageHeadShotFeedback { wave synthetic.wav volume 0.7 channel CHAN_STATIC
                    operator_stacks { start_stack { import_stack CS_limit_foley_start }
                    update_stack { import_stack CS_update_foley
                        volume_falloff { input_min 120 input_max 4000 } } } }
                Player.DamageHelmetFeedback { wave synthetic.wav volume 0.7 channel CHAN_STATIC
                    operator_stacks { start_stack { import_stack CS_limit_foley_start }
                    update_stack { import_stack CS_update_foley
                        volume_falloff { input_min 120 input_max 4000 } } } }
                Player.DamageHeadShot { wave synthetic.wav channel CHAN_STATIC
                    operator_stacks { update_stack { import_stack CS_update_foley } } }
                Player.DamageHelmet { wave synthetic.wav channel CHAN_STATIC
                    operator_stacks { update_stack { import_stack CS_update_foley } } }
                Player.DamageKevlar { wave synthetic.wav channel CHAN_STATIC
                    operator_stacks { update_stack { import_stack CS_update_foley } } }
                Flesh.BulletImpact { wave synthetic.wav }
            "#).unwrap();
            let mut app = App::new();
            app.init_resource::<SoundLibrary>().init_resource::<Assets<AudioSource>>()
                .insert_resource(Hits(events));
            let mut local = crate::player::LocalPlayer::new(viewer, glam::Vec3::ZERO, 0.0);
            local.noclip = noclip;
            app.insert_resource(local);
            for (name, block) in doc.iter().filter_map(|(name, value)| value.as_block().map(|block| (name, block))) {
                let mut entry = parse_entry(block).unwrap();
                entry.frozen_wave = Some(classic::FrozenWave { bytes: Arc::from([0u8; 4]), channels: 1 });
                app.world_mut().resource_mut::<SoundLibrary>().entries.insert(name.to_ascii_lowercase(), entry);
            }
            app.add_systems(Update, |mut commands: Commands, mut sounds: ResMut<SoundLibrary>,
                mut sources: ResMut<Assets<AudioSource>>, mut hits: ResMut<Hits>,
                local: Res<crate::player::LocalPlayer>| {
                for event in hits.0.drain(..) {
                    crate::effects::play_combat_cue(&event, &local, &mut commands,
                        &mut sounds, &mut sources, &vfs::Vfs::new());
                }
            });
            app.update();
            app
        }
        // Beyond both ordinary damage and Feedback stack cutoffs. A map's
        // geometry/range and the attacker's gun must not mute confirmation.
        let contact = glam::Vec3::new(5000.0, 0.0, 64.0);
        let hit = |group, protection| GameEvent::Damage { attacker: 1, victim: 2, health: 20,
            group, pos: contact, protection };
        // Capture a death independently of any later player lookup, then use
        // the authoritative hit fields carried by a firearm headshot receipt.
        let mut world = sim::World::new(Arc::new(collision::CollisionWorld::new()));
        let victim = world.add_player("old life", sim::Team::Terrorist);
        world.damage_environment(victim, 1000, "environment");
        let mut death = world.events.into_iter().find_map(|event| match event {
            GameEvent::Kill { death, .. } => Some(death), _ => None,
        }).unwrap();
        death.key.victim = 2;
        death.hit = Some(sim::death::DeathHit { position: contact, direction: glam::Vec3::X, group: HitGroup::Head });
        let kill = GameEvent::Kill { attacker: 1, victim: 2, weapon: "weapon_awp".into(), headshot: true, death };
        for (protection, feedback) in [(HitProtection::Helmet, "player.damagehelmetfeedback"),
            (HitProtection::Flesh, "player.damageheadshotfeedback")] {
            for fatal in [false, true] {
                let mut events = vec![hit(HitGroup::Head, protection)];
                if fatal { events.push(kill.clone()); }
                let app = app(events, 1, false);
                let sounds = app.world().resource::<SoundLibrary>();
                assert_eq!(sounds.playing.len(), if fatal { 2 } else { 1 });
                let voice = sounds.playing.iter().find(|voice| voice.entry == feedback).unwrap();
                assert_eq!(voice.owner, 2, "Feedback remains owned by the struck player");
                assert!(app.world().get::<Emitter>(voice.entity).is_none());
                assert_eq!(app.world().get::<SoundGain>(voice.entity).unwrap().0, 0.7);
                if fatal {
                    let voice = sounds.playing.iter().find(|voice| voice.entry == "player.damageheadshot").unwrap();
                    assert!(app.world().get::<Emitter>(voice.entity).is_none());
                    assert_eq!(app.world().get::<SoundGain>(voice.entity).unwrap().0, 1.0);
                }
            }
        }
        // Body impacts stay ordinary; unrelated listeners get no attacker cue.
        for (event, viewer, noclip, expected) in [
            (hit(HitGroup::Chest, HitProtection::Kevlar), 1, false, "player.damagekevlar"),
            (hit(HitGroup::LeftLeg, HitProtection::Flesh), 1, false, "flesh.bulletimpact"),
            (hit(HitGroup::Head, HitProtection::Helmet), 3, false, "player.damagehelmet"),
            (hit(HitGroup::Head, HitProtection::Helmet), 1, true, "player.damagehelmet"),
        ] {
            let app = app(vec![event], viewer, noclip);
            let sounds = app.world().resource::<SoundLibrary>();
            assert_eq!(sounds.playing.len(), 1);
            assert_eq!(sounds.playing[0].entry, expected);
            let emitter = app.world().get::<Emitter>(sounds.playing[0].entity).unwrap();
            assert_eq!(emitter.pos, to_bevy(contact));
            assert!(emitter.gain(5000.0) < emitter.gain(0.0));
        }
        assert!(app(vec![kill], 3, false).world().resource::<SoundLibrary>().playing.is_empty());
    }

    #[test]
    fn cancelled_never_observed_players_are_not_retained_forever() {
        let mut app = voices();
        let mut fixture = app.world_mut().resource_mut::<VoiceFixture>();
        fixture.requests.push(local_voice(7)); fixture.cancel = true;
        drop(fixture);
        app.update();
        assert_eq!(app.world().resource::<VoiceFixture>().pending_seen_in_update, 1);
        assert!(app.world().resource::<SoundLibrary>().playing.is_empty());
        assert_eq!(app.world_mut().query::<&AudioPlayer>().iter(app.world()).count(), 0);
        app.update();
        assert!(app.world().resource::<SoundLibrary>().playing.is_empty());
    }

    #[test]
    fn fixed_and_late_postupdate_producers_are_visible_before_prune() {
        let mut app = voices();
        fn request_late(mut fixture: ResMut<VoiceFixture>, mut emitted: Local<bool>) {
            if !*emitted { fixture.requests.push(local_voice(8)); *emitted = true; }
        }
        app.add_systems(FixedUpdate, request_voices)
            .add_systems(PostUpdate, (request_late, request_voices).chain_ignore_deferred());
        app.world_mut().resource_mut::<VoiceFixture>().requests.push(local_voice(7));
        app.world_mut().run_schedule(FixedUpdate);
        // The second producer queues after Update's observer, during the actual
        // frame's PostUpdate. Its final command flush still precedes Last.
        app.update();
        let sounds = app.world().resource::<SoundLibrary>();
        assert_eq!(sounds.playing.len(), 2);
        assert!(sounds.playing.iter().all(|voice| voice.player_observed));
    }
    #[test]
    fn movie_wave_paths_resolve_doubled_separators() {
        assert_eq!(super::strip_flags(r"UI\\buttonrollover.wav"), "sound/ui/buttonrollover.wav");
        assert_eq!(super::strip_flags("*weapons/ak47/ak47_01.wav"), "sound/weapons/ak47/ak47_01.wav");
    }
    use super::*;

    #[test]
    fn malformed_mp3_is_rejected_before_bevy_decodes_it() {
        assert!(decoded_audio(vec![0xFF, 0xFB, 1, 2]).is_none());
    }

    #[test]
    fn stereo_and_mono_spatial_samples_have_the_same_gain() {
        let ears = spatial_listener();
        let front = [0.0, 0.0, -0.01];
        let samples = |channels: u16| {
            let input = rodio::buffer::SamplesBuffer::new(channels.try_into().unwrap(), 44100.try_into().unwrap(), vec![1.0; channels as usize]);
            let mut source = rodio::source::Spatial::new(input, front,
                (ears.left_ear_offset * PAN_ONLY_SCALE).to_array(), (ears.right_ear_offset * PAN_ONLY_SCALE).to_array());
            source.next().unwrap() * spatial_normalization(channels)
        };
        assert_eq!(samples(1), samples(2));
        assert_eq!(samples(1), samples(4));
    }

    #[test]
    fn actual_spatial_samples_pan_to_the_correct_ear_without_extra_falloff() {
        let ears = spatial_listener();
        let samples = |position: Vec3| {
            let input = rodio::buffer::SamplesBuffer::new(1.try_into().unwrap(), 44100.try_into().unwrap(), vec![1.0]);
            let mut source = rodio::source::Spatial::new(input, (position * PAN_ONLY_SCALE).to_array(),
                (ears.left_ear_offset * PAN_ONLY_SCALE).to_array(), (ears.right_ear_offset * PAN_ONLY_SCALE).to_array());
            (source.next().unwrap(), source.next().unwrap())
        };
        let front = samples(Vec3::new(0.0, 0.0, -4000.0));
        assert!((front.0 - RODIO_FRONT).abs() < 0.001 && (front.1 - RODIO_FRONT).abs() < 0.001);
        let right = samples(Vec3::new(1000.0, 0.0, 0.0));
        assert!(right.1 > right.0 * 1.9, "right-hand source has wrong pan: {right:?}");
        let left = samples(Vec3::new(-1000.0, 0.0, 0.0));
        assert!(left.0 > left.1 * 1.9, "left-hand source has wrong pan: {left:?}");
    }

    fn wav(tag: u16, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&tag.to_le_bytes());
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&44100u32.to_le_bytes());
        fmt.extend_from_slice(&88200u32.to_le_bytes());
        fmt.extend_from_slice(&2u16.to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        let mut out = b"RIFF\0\0\0\0WAVE".to_vec();
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        out.extend_from_slice(&fmt);
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn accepts_pcm_unwraps_mp3_rejects_adpcm() {
        assert!(playable(wav(1, 16, &[0, 0, 1, 0])).is_some());
        assert_eq!(playable(wav(0x55, 0, &[0xFF, 0xFB, 1, 2])).unwrap(), vec![0xFF, 0xFB, 1, 2]);
        assert!(playable(wav(2, 4, &[0, 0])).is_none());
        assert!(playable(b"not audio".to_vec()).is_none());
    }

    #[test]
    fn soundscript_fields_parse() {
        let doc = kv::parse(r#"
            "Weapon_X.Single" { "channel" "CHAN_STATIC" "volume" "0.9" "pitch" "PITCH_NORM" "soundlevel" "SNDLVL_79dB"
                "wave" "`)weapons/x/x_01.wav" "soundentry_version" "2"
                "operator_stacks" { "start_stack" { "import_stack" "CS_distance_weapon_start"
                    "play_distant_version_1" { "entry_name" "Weapon_X.SingleDistant" } }
                    "update_stack" { "import_stack" "CS_update_weapon" } } }
            "Weapon_Knife.Slash" { "channel" "CHAN_WEAPON" "volume" "0.1, 0.6" "pitch" "97, 105" "soundlevel" "SNDLVL_65dB"
                "rndwave" { "wave" ")weapons/knife/a.wav" "wave" ")weapons/knife/b.wav" } }
            "Event.TERWin" { "wave" "+radio/terwin.wav" "soundlevel" "SNDLVL_GUNFIRE" }
        "#).unwrap();
        let shot = parse_entry(doc.block("Weapon_X.Single").unwrap()).unwrap();
        assert_eq!(shot.waves, ["sound/weapons/x/x_01.wav"]);
        assert_eq!((shot.channel, shot.category, shot.limit), (6, Category::Weapon, Some(2)));
        assert_eq!(shot.distant.as_deref(), Some("weapon_x.singledistant"));
        assert_eq!((shot.volume, shot.pitch, shot.soundlevel), ((0.9, 0.9), (100.0, 100.0), 79.0));
        let slash = parse_entry(doc.block("Weapon_Knife.Slash").unwrap()).unwrap();
        assert_eq!((slash.volume, slash.pitch, slash.channel, slash.waves.len()), ((0.1, 0.6), (97.0, 105.0), 1, 2));
        assert_eq!(slash.category, Category::Plain);
        let radio = parse_entry(doc.block("Event.TERWin").unwrap()).unwrap();
        assert_eq!((radio.category, radio.soundlevel), (Category::Announcer, 140.0));
    }

    #[test]
    fn curves_match_measurements() {
        // docs/FIDELITY.md, "Distance curves" (measured, ±0.1).
        let near = |measured: f32, model: f32| assert!((measured - model).abs() < 0.1, "{measured} vs {model}");
        near(1.0, stack_curve(400.0, 400.0, 3937.0));
        near(0.48, stack_curve(1000.0, 400.0, 3937.0) * gunfire_crossfade(1000.0, false));
        near(0.13, stack_curve(2000.0, 400.0, 3937.0) * gunfire_crossfade(2000.0, false));
        near(0.42, stack_curve(400.0, 120.0, 1100.0));
        near(0.14, stack_curve(800.0, 120.0, 1100.0));
        near(0.40, stack_curve(600.0, 400.0, 1100.0));
        near(0.33, soundlevel_curve(60.0, 100.0));
        near(0.30, soundlevel_curve(65.0, 200.0));
        near(0.44, soundlevel_curve(75.0, 400.0));
        near(0.41, soundlevel_curve(80.0, 600.0));
        assert_eq!(soundlevel_curve(0.0, 5000.0), 1.0);
        assert_eq!(gunfire_crossfade(500.0, true), 0.0);
        assert_eq!(gunfire_crossfade(3000.0, false), 0.0);
    }
}

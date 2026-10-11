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
use std::collections::HashMap;
use std::sync::Arc;

/// Rodio applies inverse-square attenuation in spatial-scale units; scaling
/// the world down this far keeps every in-map distance under one unit, so
/// rodio only pans and the measured curves below set the loudness.
const PAN_ONLY_SCALE: f32 = 1.0 / 100_000.0;
/// Rodio gives a source straight ahead 0.75 per ear; the curves below are
/// per-ear gains measured in the real game, so spatial gains are divided
/// by this.
const RODIO_FRONT: f32 = 0.75;

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
            Category::Impacts => 0.29,
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
    /// `play_distant_version_*` partner entry.
    distant: Option<String>,
    /// Older instances of this entry an entity may keep when it starts
    /// another (`sys_stop_entries` in the `CS_limit_*` start stacks).
    limit: Option<usize>,
    /// This is a `…Distant` layer (cross-faded in with distance).
    distant_layer: bool,
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
    gunfire: Option<bool>,
}

impl Emitter {
    fn gain(&self, distance: f32) -> f32 {
        let fade = self.gunfire.map_or(1.0, |distant| gunfire_crossfade(distance, distant));
        self.base * self.category.falloff(self.soundlevel, distance) * fade
    }
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
    started: f64,
}

#[derive(Resource, Default)]
pub struct SoundLibrary {
    entries: HashMap<String, Entry>,
    loaded: HashMap<String, Option<Handle<AudioSource>>>,
    playing: Vec<Playing>,
    rng: u64,
    clock: f64,
    listener: Vec3,
    /// `CSR_SOUND_LOG=1`: log every sound start (compare with the real
    /// game's `snd_showstart`, see docs/FIDELITY.md).
    log: bool,
    pub master_volume: f32,
}

/// Returns bytes Bevy's decoder is guaranteed to accept, or `None`.
///
/// Bevy panics on undecodable audio, so every file is checked first:
/// PCM (8/16/24/32-bit int, 32-bit float) WAVs pass through, MP3-in-WAV
/// (format 0x55, common in Valve content) is unwrapped to its raw MP3
/// frames, bare MP3s pass through, anything else is skipped.
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

fn strip_flags(wave: &str) -> String {
    let w = wave.trim_start_matches(|c: char| "*#@><^)}$!?&~`+%(".contains(c));
    format!("sound/{}", w.replace('\\', "/").to_ascii_lowercase())
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
    Some(Entry {
        waves,
        volume,
        pitch,
        soundlevel: b.str("soundlevel").map_or(75.0, parse_soundlevel),
        channel: b.str("channel").map_or(0, channel_number),
        category,
        distant,
        limit,
        distant_layer: false,
    })
}

impl SoundLibrary {
    pub fn load(fs: &vfs::Vfs) -> Self {
        let mut lib = SoundLibrary {
            master_volume: 0.5,
            rng: 0x9E37_79B9_7F4A_7C15,
            log: std::env::var_os("CSR_SOUND_LOG").is_some(),
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
        lib
    }

    /// Soundscript entry names (lower case).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// The files an entry picks from.
    pub fn waves(&self, name: &str) -> Option<&[String]> {
        self.entries.get(&name.to_ascii_lowercase()).map(|e| e.waves.as_slice())
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

    fn source(&mut self, fs: &vfs::Vfs, sources: &mut Assets<AudioSource>, path: &str) -> Option<Handle<AudioSource>> {
        self.loaded
            .entry(path.to_string())
            .or_insert_with(|| {
                let bytes = playable(fs.read(path)?)?;
                Some(sources.add(AudioSource { bytes: Arc::from(bytes) }))
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
        if let Some(distant) = entry.distant.as_deref().filter(|_| emit.at.is_some()) {
            self.play_with(commands, fs, sources, distant, emit);
        }
        let pick = ((self.random() * entry.waves.len() as f32) as usize).min(entry.waves.len() - 1);
        let Some(handle) = self.source(fs, sources, &entry.waves[pick]) else { return };
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
        let entity = match emit.at {
            Some(pos) => {
                let emitter = Emitter {
                    pos: to_bevy(pos),
                    base: volume * entry.category.field(),
                    category: entry.category,
                    soundlevel: entry.soundlevel,
                    gunfire: (entry.category == Category::Weapon && (entry.distant.is_some() || entry.distant_layer)).then_some(entry.distant_layer),
                };
                let gain = emitter.gain(emitter.pos.distance(self.listener));
                commands
                    .spawn((
                        AudioPlayer(handle),
                        SoundGain(emitter.base),
                        emitter,
                        PlaybackSettings::DESPAWN
                            .with_volume(Volume::Linear(gain * self.master_volume))
                            .with_speed(speed)
                            .with_spatial(true)
                            .with_spatial_scale(SpatialScale::new(PAN_ONLY_SCALE)),
                        Transform::from_translation(emitter.pos),
                    ))
                    .id()
            }
            None => {
                // Measured gains top out at lvol 20 (1.07 of the AK shot).
                let gain = (volume * entry.category.near(emit.local)).min(1.07);
                commands
                    .spawn((
                        AudioPlayer(handle),
                        SoundGain(gain),
                        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain * self.master_volume)).with_speed(speed),
                    ))
                    .id()
            }
        };
        if let Some(owner) = emit.owner {
            self.playing.push(Playing { entity, owner, channel: entry.channel, entry: key, started: self.clock });
        }
    }

    /// Channel stealing and per-entry instance limits for one owner.
    fn steal(&mut self, commands: &mut Commands, owner: u32, entry: &Entry, key: &str) {
        let clock = self.clock;
        // Sounds this old have finished (and despawned) in any case.
        self.playing.retain(|p| clock - p.started < 15.0);
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
        if let Some(limit) = entry.limit {
            // `limit` counts the new sound too: stop the oldest until it fits.
            let excess = (same.len() + 1).saturating_sub(limit.max(1));
            stop.extend(same.into_iter().take(excess));
        }
        stop.sort_unstable();
        for i in stop.into_iter().rev() {
            let p = self.playing.remove(i);
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
        let path = strip_flags(wave);
        let handle = self.source(fs, sources, &path)?;
        let category = Category::Ambient;
        let base = PlaybackSettings { mode: if raw.looping { bevy::audio::PlaybackMode::Loop } else { bevy::audio::PlaybackMode::Despawn }, ..PlaybackSettings::DESPAWN }
            .with_speed((raw.pitch / 100.0).clamp(0.25, 4.0));
        let fade = (raw.fade_in > 0.0).then_some(Fade { start: self.clock, duration: raw.fade_in, out: false });
        let start = if fade.is_some() { 0.0 } else { 1.0 };
        if self.log {
            info!("ambient {:.3} {path} vol {:.2} at {:?}", self.clock, raw.volume, raw.at);
        }
        let mut e = match raw.at {
            Some(pos) => {
                let emitter = Emitter { pos: to_bevy(pos), base: raw.volume * category.field(), category, soundlevel: raw.soundlevel, gunfire: None };
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
    time: Res<Time>,
    mut sounds: ResMut<SoundLibrary>,
    mut listener: ResMut<Listener>,
    ears: Query<&GlobalTransform, With<SpatialListener>>,
    mut spatial: Query<(&Emitter, Option<&Fade>, &mut SpatialAudioSink)>,
    mut faded: Query<(Entity, &SoundGain, &Fade, Option<&mut AudioSink>, Has<Emitter>)>,
) {
    sounds.clock += time.delta_secs_f64();
    if let Some(ear) = ears.iter().next() {
        listener.0 = ear.translation();
    }
    sounds.listener = listener.0;
    let master = sounds.master_volume;
    let clock = sounds.clock;
    for (emitter, fade, mut sink) in &mut spatial {
        let gain = emitter.gain(emitter.pos.distance(listener.0)) * fade.map_or(1.0, |f| f.value(clock));
        sink.set_volume(Volume::Linear(gain * master));
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

#[cfg(test)]
mod tests {
    use super::*;

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

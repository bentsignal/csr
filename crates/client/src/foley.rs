//! Player foley: footsteps, jump and landing sounds, and the weapon sounds
//! that the original plays from animation events (deploy, reload).
//!
//! Timings were measured in CS:GO Legacy with `snd_showstart`
//! (docs/FIDELITY.md, "Sound audit"). Everything here is derived from the
//! simulation's player state, so it works the same for the local player,
//! bots and (later) networked players.

use crate::audio::{Emit, SoundLibrary};
use crate::player::{LocalPlayer, SimWorld};
use crate::surfaces::SurfaceProps;
use crate::viewmodel::ViewModels;
use crate::LoadedLevel;
use bevy::audio::AudioSource;
use bevy::prelude::*;
use std::collections::HashMap;

/// Horizontal speeds at or below this are silent (walk speed 130 is
/// silent; 140 already makes steps).
pub const AUDIBLE_SPEED: f32 = 135.0;
/// Step period above `FAST_SPEED` (measured 0.297 s at 250 u/s) and below it
/// (0.390 s at 140-215 u/s).
pub const FAST_STEP: f32 = 0.297;
pub const SLOW_STEP: f32 = 0.390;
pub const FAST_SPEED: f32 = 230.0;
/// Landings slower than this make no landing sound (stepping off a kerb).
pub const LAND_SPEED: f32 = 150.0;

pub fn step_period(speed: f32) -> f32 {
    if speed > FAST_SPEED { FAST_STEP } else { SLOW_STEP }
}

#[derive(Default)]
struct PlayerFoley {
    seen: bool,
    weapon: String,
    reloading: bool,
    shell_stage: Option<Option<i32>>,
    moving: bool,
    step_timer: f32,
    left: bool,
}

struct Queued {
    at: f32,
    player: sim::PlayerId,
    weapon: String,
    entry: String,
    reload: bool,
}

#[derive(Resource, Default)]
pub struct Foley {
    players: HashMap<sim::PlayerId, PlayerFoley>,
    queue: Vec<Queued>,
}

impl Foley {
    fn schedule(&mut self, player: sim::PlayerId, weapon: &str, now: f32, duration: f32, events: &[(f32, String)], reload: bool) {
        for (cycle, entry) in events {
            self.queue.push(Queued { at: now + cycle * duration, player, weapon: weapon.to_string(), entry: entry.clone(), reload });
        }
    }
}

/// How a player's sounds are emitted: at the listener for the player we
/// view in first person, in the world for everyone else.
pub fn emit_for(p: &sim::Player, local: &LocalPlayer, at: glam::Vec3) -> Emit {
    let own = p.id == local.view_target() && !local.noclip;
    Emit { at: (!own).then_some(at), owner: Some(p.id), local: own }
}

fn team_prefix(team: sim::Team) -> &'static str {
    if team == sim::Team::Terrorist { "T_" } else { "CT_" }
}

/// The surface's step entry under `origin` (`stepleft`/`stepright` of its
/// surface property), e.g. `Gravel.StepLeft`.
fn surface_step(level: &level::Level, props: &SurfaceProps, origin: glam::Vec3, left: bool) -> String {
    let tr = level.collision.trace_ray(origin + glam::Vec3::Z, origin - glam::Vec3::Z * 32.0, collision::contents::MASK_SOLID);
    let sounds = (tr.fraction < 1.0).then(|| props.sounds(&level.vfs, level.collision.surface_name(tr.surface))).flatten();
    let step = sounds.and_then(|s| if left { s.step_left.clone() } else { s.step_right.clone() });
    step.unwrap_or_else(|| if left { "Default.StepLeft".into() } else { "Default.StepRight".into() })
}

/// Footstep entry for a team: `CT_Gravel.StepLeft`, else the plain entry,
/// else the team default.
fn step_entry(sounds: &SoundLibrary, team: sim::Team, step: &str, left: bool) -> String {
    let prefixed = format!("{}{step}", team_prefix(team));
    if sounds.has(&prefixed) {
        prefixed
    } else if sounds.has(step) {
        step.to_string()
    } else {
        format!("{}Default.Step{}", team_prefix(team), if left { "Left" } else { "Right" })
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    world: Res<SimWorld>,
    local: Res<LocalPlayer>,
    views: Res<ViewModels>,
    props: Res<SurfaceProps>,
    level: Res<LoadedLevel>,
    time: Res<Time>,
    mut sounds: ResMut<SoundLibrary>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut foley: ResMut<Foley>,
) {
    let now = world.0.time();
    let dt = time.delta_secs();
    let fs = &level.0.vfs;
    let foley = &mut *foley;
    for p in &world.0.players {
        let st = foley.players.entry(p.id).or_default();
        let weapon = p.active_weapon().map(|w| w.def.class_name.clone()).unwrap_or_default();
        if !p.alive {
            *st = PlayerFoley::default();
            foley.queue.retain(|q| q.player != p.id);
            continue;
        }
        let first = !st.seen;
        st.seen = true;

        // Deploy: the draw sequence's sounds, at its own speed.
        if weapon != st.weapon {
            st.weapon = weapon.clone();
            st.reloading = false;
            st.shell_stage = None;
            foley.queue.retain(|q| q.player != p.id);
            if !first {
                if let Some((duration, events)) = views.activity_sounds(&weapon, "ACT_VM_DRAW") {
                    foley.schedule(p.id, &weapon, now, duration, &events, false);
                }
            }
        }

        // Reload: the reload sequence stretched over the simulated reload,
        // as the viewmodel plays it; shotguns per stage.
        let st = foley.players.get_mut(&p.id).unwrap();
        let w = p.active_weapon();
        let reloading = w.is_some_and(|w| w.reload_end.is_some());
        let shell = w.and_then(|w| w.shell_reload()).is_some();
        let stage = w.and_then(|w| w.shell_stage(now)).map(|s| s.0);
        let mut plan: Option<(&str, Option<f32>)> = None;
        if shell {
            if reloading && stage.is_some() && stage != st.shell_stage {
                plan = Some((if stage == Some(None) { "ACT_SHOTGUN_RELOAD_START" } else { "ACT_VM_RELOAD" }, None));
            } else if !reloading && st.reloading {
                plan = Some(("ACT_SHOTGUN_RELOAD_FINISH", None));
            }
        } else if reloading && !st.reloading {
            plan = Some(("ACT_VM_RELOAD", w.and_then(|w| w.reload_end).map(|end| (end - now).max(0.05))));
        }
        let cancelled = st.reloading && !reloading && !shell;
        st.reloading = reloading;
        st.shell_stage = stage;
        if cancelled {
            // Finished or interrupted: nothing of it may still play.
            foley.queue.retain(|q| !(q.player == p.id && q.reload));
        }
        if let Some((activity, stretch)) = plan {
            if std::env::var_os("CSR_SOUND_LOG").is_some() {
                info!("foley: {} {weapon} {activity} over {stretch:?}: {:?}", p.id, views.activity_sounds(&weapon, activity));
            }
            if let Some((duration, events)) = views.activity_sounds(&weapon, activity) {
                foley.schedule(p.id, &weapon, now, stretch.unwrap_or(duration), &events, true);
            }
        }

        // Footsteps, alternating feet, with the gear rustle on each.
        let st = foley.players.get_mut(&p.id).unwrap();
        let v = p.movement.velocity;
        let speed = (v.x * v.x + v.y * v.y).sqrt();
        if p.movement.on_ground && speed > AUDIBLE_SPEED {
            if !st.moving {
                st.moving = true;
                st.step_timer = step_period(speed);
            }
            st.step_timer -= dt;
            if st.step_timer <= 0.0 {
                st.step_timer = (st.step_timer + step_period(speed)).max(0.0);
                st.left = !st.left;
                let left = st.left;
                let step = surface_step(&level.0, &props, p.movement.origin, left);
                let entry = step_entry(&sounds, p.team, &step, left);
                let emit = emit_for(p, &local, p.movement.origin);
                sounds.play_with(&mut commands, fs, &mut sources, &entry, emit);
                sounds.play_with(&mut commands, fs, &mut sources, &format!("{}Default.Suit", team_prefix(p.team)), emit);
            }
        } else {
            st.moving = false;
        }
    }

    // Animation-event sounds whose time has come, if the weapon is still out.
    let due: Vec<Queued> = {
        let (due, keep) = foley.queue.drain(..).partition(|q| q.at <= now);
        foley.queue = keep;
        due
    };
    for q in due {
        let Some(p) = world.0.player(q.player) else { continue };
        if p.active_weapon().map(|w| w.def.class_name.as_str()) != Some(q.weapon.as_str()) {
            continue;
        }
        let emit = emit_for(p, &local, p.movement.origin + glam::Vec3::Z * 48.0);
        sounds.play_with(&mut commands, fs, &mut sources, &q.entry, emit);
    }
}

/// Take-off sound (`Default.WalkJump`).
pub fn jump(commands: &mut Commands, sounds: &mut SoundLibrary, sources: &mut Assets<AudioSource>, level: &level::Level, p: &sim::Player, local: &LocalPlayer) {
    let emit = emit_for(p, local, p.movement.origin);
    sounds.play_with(commands, &level.vfs, sources, "Default.WalkJump", emit);
}

/// Landing: the surface's `Land_` step entry plus the suit, e.g.
/// `Land_Gravel.StepLeft` and `CT_Default.SuitLand`.
#[allow(clippy::too_many_arguments)]
pub fn land(commands: &mut Commands, sounds: &mut SoundLibrary, sources: &mut Assets<AudioSource>, level: &level::Level, props: &SurfaceProps, p: &sim::Player, local: &LocalPlayer, speed: f32) {
    if speed < LAND_SPEED {
        return;
    }
    let step = surface_step(level, props, p.movement.origin, true);
    let land = format!("Land_{step}");
    let entry = if sounds.has(&land) { land } else { "Default.Land".into() };
    let emit = emit_for(p, local, p.movement.origin);
    sounds.play_with(commands, &level.vfs, sources, &entry, emit);
    sounds.play_with(commands, &level.vfs, sources, &format!("{}Default.SuitLand", team_prefix(p.team)), emit);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_cadence_matches_measurements() {
        assert_eq!(step_period(250.0), FAST_STEP);
        assert_eq!(step_period(215.0), SLOW_STEP);
        assert!(130.0 <= AUDIBLE_SPEED && AUDIBLE_SPEED < 140.0);
    }
}

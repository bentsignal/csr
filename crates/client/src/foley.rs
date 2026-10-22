//! Player foley: footsteps, jump and landing sounds, and the weapon sounds
//! that the original plays from animation events (deploy, reload, AWP bolt).
//!
//! Footstep timings were measured in CS:GO Legacy with `snd_showstart`
//! (docs/FIDELITY.md, "Sound audit"); weapon events use the installed clips.
//! Tick foley is derived from simulation state. Local knife inspection instead
//! follows the accepted viewmodel playback on its presentation clock.

use crate::audio::{Emit, SoundLibrary};
use crate::player::{LocalPlayer, SimWorld};
use crate::surfaces::SurfaceProps;
use crate::viewmodel::{ViewModels, ViewModelState};
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
    life_epoch: Option<u64>,
    weapon: String,
    reloading: bool,
    shell_stage: Option<Option<i32>>,
    grenade_pull: Option<u64>,
    grenade_release: Option<u64>,
    suppressor_transition: Option<sim::weapon::SuppressorTransition>,
    moving: bool,
    climbing: bool,
    step_timer: f32,
    left: bool,
}

#[derive(Clone, Copy)]
struct Elapsed {
    seconds: f32,
    tick_seconds: f32,
}

impl PlayerFoley {
    fn footstep(&mut self, grounded: bool, speed: f32, elapsed: Elapsed) -> Option<bool> {
        if !grounded || speed <= AUDIBLE_SPEED {
            self.moving = false;
            return None;
        }
        let period = step_period(speed);
        let seconds = if self.moving {
            elapsed.seconds
        } else {
            // Movement/life onset was only just observed. Do not infer that
            // this player was moving throughout the preceding snapshot gap.
            self.moving = true;
            self.step_timer = period;
            elapsed.tick_seconds
        };
        self.step_timer -= seconds;
        if self.step_timer > 0.0 { return None; }
        // Emit at most one current step. Missing intermediate movement and
        // surfaces cannot be reconstructed; discard whole-period overruns.
        self.step_timer = if self.step_timer <= -period {
            period
        } else {
            (self.step_timer + period).max(0.0)
        };
        self.left = !self.left;
        Some(self.left)
    }
}

struct Queued {
    at: f32,
    player: sim::PlayerId,
    weapon: String,
    entry: String,
    deploy: bool,
    reload: bool,
    suppressor: Option<sim::weapon::SuppressorTransition>,
    grenade_pull: Option<u64>,
    primary_attack: bool,
}

#[derive(Default)]
struct InspectFoley {
    key: Option<crate::viewmodel::KnifeInspectKey>,
    queue: Vec<(f32, String)>,
    last_render: Option<f32>,
}

impl InspectFoley {
    fn due(&mut self, receipt: Option<&crate::viewmodel::KnifeInspect>, now: f32)
        -> Vec<(f32, String)> {
        if self.last_render.replace(now).is_some_and(|previous| now < previous) {
            // An old absolute playback cannot survive a presentation reset.
            // Keep its consumed identity; do not replay it at the new time.
            self.queue.clear();
        }
        let key = receipt.map(|receipt| &receipt.key);
        if self.key.as_ref() != key {
            self.queue.clear();
            self.key = key.cloned();
            if let Some(receipt) = receipt {
                self.queue.extend(receipt.sounds.iter().map(|(cycle, entry)|
                    (receipt.key.started + cycle * receipt.duration, entry.clone())));
            }
        }
        let (due, keep) = self.queue.drain(..).partition(|(at, _)| *at <= now);
        self.queue = keep;
        due
    }
}

#[derive(Resource, Default)]
pub struct Foley {
    players: HashMap<sim::PlayerId, PlayerFoley>,
    queue: Vec<Queued>,
    last_tick: Option<(u64, u32)>,
    inspect: InspectFoley,
    /// Test-only witness of the existing scheduler, including cycle-zero cues
    /// which leave the queue in the same update. No audio device is required.
    #[cfg(test)]
    pub(crate) scheduled: Vec<(sim::PlayerId, String, String, f32)>,
    /// Witness at the actual due-event dispatch, after interruption filtering.
    #[cfg(test)]
    pub(crate) dispatched: Vec<(sim::PlayerId, String, String, f32)>,
    #[cfg(test)]
    pub(crate) inspected: Vec<(String, f32)>,
}

impl Foley {
    fn advance(&mut self, tick: u64, rate: u32) -> Option<Elapsed> {
        let tick_seconds = 1.0 / rate as f32;
        let seconds = match self.last_tick.replace((tick, rate)) {
            Some((old_tick, old_rate)) if old_rate == rate && tick == old_tick => return None,
            Some((old_tick, old_rate)) if old_rate == rate && tick > old_tick => {
                (tick - old_tick) as f32 * tick_seconds
            }
            Some(_) => {
                // A new clock cannot continue old absolute animation times.
                self.players.clear();
                self.queue.clear();
                self.inspect.queue.clear();
                tick_seconds
            }
            None => tick_seconds,
        };
        Some(Elapsed { seconds, tick_seconds })
    }

    fn retain_lives(&mut self, players: &[sim::Player]) {
        self.players.retain(|id, state| players.iter().any(|player| {
            player.id == *id && player.alive && state.life_epoch == Some(player.life_epoch)
        }));
        self.queue.retain(|sound| self.players.contains_key(&sound.player)
            && (!sound.primary_attack || players.iter().any(|player|
                player.id == sound.player && player.active_weapon().is_some_and(|w|
                    w.def.class_name == sound.weapon && w.reload_end.is_none()))));
    }

    fn schedule(&mut self, player: sim::PlayerId, weapon: &str, now: f32, duration: f32, events: &[(f32, String)], reload: bool) {
        for (cycle, entry) in events {
            #[cfg(test)]
            self.scheduled.push((player, weapon.to_string(), entry.clone(), now + cycle * duration));
            self.queue.push(Queued { at: now + cycle * duration, player, weapon: weapon.to_string(), entry: entry.clone(), deploy: false, reload, suppressor: None, grenade_pull: None, primary_attack: false });
        }
    }

    fn deploy(&mut self, player: sim::PlayerId, weapon: &str,
        now: f32, duration: f32, events: &[(f32, String)]) {
        let first = self.queue.len();
        self.schedule(player, weapon, now, duration, events, false);
        for q in &mut self.queue[first..] { q.deploy = true; }
    }

    fn interrupt_deploys(&mut self, events: &[sim::GameEvent]) {
        // These newly routed manual-action clips share pump/bolt cues with
        // Draw. A confirmed shot interrupts that playback, not matching sound
        // names: completed Draws, reloads and repeated authored events survive.
        // Leave the accepted AWP/knife/utility paths unchanged.
        for event in events {
            if let sim::GameEvent::Fire { player, weapon, .. } = event {
                if matches!(weapon.as_str(), "weapon_ssg08" | "weapon_nova" | "weapon_sawedoff" | "weapon_mag7") {
                    self.queue.retain(|q| !(q.player == *player && q.weapon == *weapon && q.deploy));
                }
            }
        }
    }

    /// Called only for a confirmed Fire event. The installed AWP attack clip
    /// owns its bolt cues; neither scope changes nor trigger attempts do.
    pub(crate) fn primary_attack(&mut self, player: sim::PlayerId, weapon: &str,
        now: f32, duration: f32, events: &[(f32, String)]) {
        self.queue.retain(|q| q.player != player || !q.primary_attack);
        let first = self.queue.len();
        self.schedule(player, weapon, now, duration, events, false);
        for q in &mut self.queue[first..] { q.primary_attack = true; }
    }
}

/// Runs after viewmodel dispatch, never on the tick clock. Accepted attacks,
/// cancellation and viewing boundaries have invalidated the receipt before
/// any due Inspect cue can play. This queue cannot remove deploy/reload/bolt
/// cues, and repeating a render timestamp cannot replay a consumed event.
#[allow(clippy::too_many_arguments)]
pub fn inspect(
    mut commands: Commands,
    world: Res<SimWorld>,
    local: Res<LocalPlayer>,
    view_state: Res<ViewModelState>,
    time: Res<Time>,
    level: Res<LoadedLevel>,
    mut sounds: ResMut<SoundLibrary>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut foley: ResMut<Foley>,
) {
    let receipt = view_state.knife_inspect.as_ref().filter(|receipt|
        !local.noclip && local.spectating.is_none() && local.id == receipt.key.owner
            && local.view_target() == receipt.key.owner
            && world.0.player(receipt.key.owner).is_some_and(|p|
                p.alive && p.life_epoch == receipt.key.life && p.active_weapon().is_some_and(|w|
                    w.def.class == weapons::WeaponClass::Knife
                        && w.def.class_name == receipt.key.weapon && !w.hides_viewmodel())));
    let due = foley.inspect.due(receipt, time.elapsed_secs());
    let Some(receipt) = receipt else { return; };
    let Some(player) = world.0.player(receipt.key.owner) else { return; };
    for (at, entry) in due {
        #[cfg(test)]
        foley.inspected.push((entry.clone(), at));
        let _ = at;
        sounds.play_with(&mut commands, &level.0.vfs, &mut sources, &entry,
            emit_for(player, &local, player.eye()));
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
    mut view_state: ResMut<ViewModelState>,
    props: Res<SurfaceProps>,
    level: Res<LoadedLevel>,
    mut sounds: ResMut<SoundLibrary>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut foley: ResMut<Foley>,
) {
    let now = world.0.time();
    let elapsed = foley.advance(world.0.tick, world.0.tick_rate);
    foley.retain_lives(&world.0.players);
    // Reliable remote events can arrive without an advancing snapshot tick.
    foley.interrupt_deploys(&world.0.events);
    let Some(elapsed) = elapsed else { return; };
    let fs = &level.0.vfs;
    let foley = &mut *foley;
    for p in &world.0.players {
        if !p.alive { continue; }
        let st = foley.players.entry(p.id).or_insert_with(|| PlayerFoley {
            life_epoch: Some(p.life_epoch),
            ..PlayerFoley::default()
        });
        let weapon = p.active_weapon().map(|w| w.def.class_name.clone()).unwrap_or_default();
        // Tick foley may precede the first render of this weapon. Share its
        // cosmetic choice instead of independently rolling or using draw[0].
        // Observing every viewed weapon also invalidates an intervening switch
        // even when rendering never presents that short-lived selection.
        let selected_draw = if p.id == local.view_target() {
            view_state.knife_draw_sounds(p, &views)
        } else { None };
        let viewed_knife = p.id == local.view_target() && p.active_weapon()
            .is_some_and(|w| w.def.class == weapons::WeaponClass::Knife);
        // Deploy: the draw sequence's sounds, at its own speed.
        if weapon != st.weapon {
            st.weapon = weapon.clone();
            st.reloading = false;
            st.shell_stage = None;
            st.suppressor_transition = None;
            foley.queue.retain(|q| q.player != p.id);
            if p.active_weapon().is_none_or(|w| w.suppressor_transition.is_none()) {
                let draw = if viewed_knife { selected_draw } else {
                    views.activity_sounds(&weapon, p.active_weapon().map_or("ACT_VM_DRAW", |w| w.draw_activity()))
                };
                if let Some((duration, events)) = draw {
                    foley.deploy(p.id, &weapon, now, duration, &events);
                }
            }
        }

        // Reload: the reload sequence stretched over the simulated reload,
        // as the viewmodel plays it; shotguns per stage.
        let st = foley.players.get_mut(&p.id).unwrap();
        let w = p.active_weapon();
        let transition = w.and_then(|w| w.suppressor_transition);
        let new_transition = transition.is_some() && transition != st.suppressor_transition;
        st.suppressor_transition = transition;
        foley.queue.retain(|q| q.player != p.id || q.suppressor.is_none() || q.suppressor == transition);
        if new_transition {
            let transition = transition.unwrap();
            if let Some((duration, events)) = views.activity_sounds(&weapon, transition.activity()) {
                let started = transition.started_tick as f32 * world.0.dt();
                for (cycle, entry) in events.iter() {
                    let at = started + cycle * duration;
                    // A newly received snapshot must not replay the preceding
                    // seconds of screw sounds as one burst.
                    if at >= now - elapsed.tick_seconds {
                        foley.queue.push(Queued { at, player: p.id, weapon: weapon.clone(),
                            entry: entry.clone(), deploy: false, reload: false, suppressor: Some(transition), grenade_pull: None, primary_attack: false });
                    }
                }
            }
        }
        let st = foley.players.get_mut(&p.id).unwrap();
        let grenade = w.and_then(|w| w.grenade_throw);
        let pull = grenade.filter(|s| s.phase != sim::utility::ThrowPhase::Throwing)
            .map(|s| s.pull_started_tick);
        let new_pull = pull.is_some() && pull != st.grenade_pull;
        st.grenade_pull = pull;
        let release = grenade.and_then(|s| s.throw_started_tick);
        let new_release = release.is_some() && release != st.grenade_release;
        st.grenade_release = release;
        if new_pull {
            if let Some((duration, events)) = views.activity_sounds(&weapon, "ACT_VM_PULLPIN") {
                for (cycle, entry) in events.iter() {
                    foley.queue.push(Queued { at: pull.unwrap() as f32 * world.0.dt() + cycle * duration,
                        player: p.id, weapon: weapon.clone(), entry: entry.clone(), deploy: false, reload: false,
                        suppressor: None, grenade_pull: pull, primary_attack: false });
                }
            }
        }
        if new_release {
            // Native logs place the throw cue on the release command edge,
            // independently of the later physical projectile release event.
            let entry = match sim::utility::Kind::from_name(&weapon) {
                Some(sim::utility::Kind::He) => Some("HEGrenade.Throw"),
                Some(sim::utility::Kind::Flash) => Some("Flashbang.Throw"),
                Some(sim::utility::Kind::Smoke) => Some("SmokeGrenade.Throw"),
                Some(sim::utility::Kind::Decoy) => Some("Decoy.Throw"),
                Some(sim::utility::Kind::Molotov) => Some("Molotov.Throw"),
                Some(sim::utility::Kind::Incendiary) => Some("IncGrenade.Throw"),
                None => None,
            };
            if let Some(entry) = entry {
                sounds.play_with(&mut commands, fs, &mut sources, entry, emit_for(p, &local, p.eye()));
            }
        }
        let st = foley.players.get_mut(&p.id).unwrap();
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
        let climbing = p.movement.ladder_normal.is_some();
        if st.climbing != climbing {
            st.climbing = climbing;
            st.moving = false;
        }
        let speed = if climbing { v.length() } else { (v.x * v.x + v.y * v.y).sqrt() };
        if let Some(left) = st.footstep(p.movement.on_ground || climbing, speed, elapsed) {
            // The installed surface table declares ladder.StepLeft/Right.
            // Resolve it through the same soundscript library as ground steps;
            // no fabricated waveform or alternate game's asset is introduced.
            let step = if climbing {
                props.get_declared("ladder").and_then(|s|
                    if left { s.step_left.clone() } else { s.step_right.clone() })
                    .unwrap_or_default()
            } else { surface_step(&level.0, &props, p.movement.origin, left) };
            if step.is_empty() { continue; }
            if climbing && !sounds.has(&step) && !sounds.has(&format!("{}{step}", team_prefix(p.team))) { continue; }
            let entry = step_entry(&sounds, p.team, &step, left);
            if std::env::var_os("CSR_SOUND_LOG").is_some() { info!("footstep sim {now:.3} player {} {entry} speed {speed:.1}", p.id); }
            let emit = emit_for(p, &local, p.movement.origin);
            sounds.play_with(&mut commands, fs, &mut sources, &entry, emit);
            sounds.play_with(&mut commands, fs, &mut sources, &format!("{}Default.Suit", team_prefix(p.team)), emit);
        }
    }

    // FixedUpdate precedes effects::process in Update. Cancel interrupted Draw
    // tails before catch-up ticks dispatch them under one audio clock. Use this
    // step's Fire events, not the accumulated pending batch: an earlier shot
    // must not interrupt a later re-deploy within the same render frame.
    // RemoteClient likewise publishes its newly received reliable events here.
    foley.interrupt_deploys(&world.0.events);

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
        if let Some(pull) = q.grenade_pull {
            // A quick release interrupts the pin-pull clip and its remaining
            // sounds. Never replay them over a throw or a fresh practice item.
            if p.active_weapon().and_then(|w| w.grenade_throw).is_none_or(|s|
                s.pull_started_tick != pull || s.phase == sim::utility::ThrowPhase::Throwing) {
                continue;
            }
        }
        let emit = emit_for(p, &local, p.movement.origin + glam::Vec3::Z * 48.0);
        #[cfg(test)]
        foley.dispatched.push((q.player, q.weapon.clone(), q.entry.clone(), q.at));
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

    // UNRUN: source-only handoff. Read only the installed models/schema and
    // sounds needed by this fixture, with an empty synthetic map and no GPU or
    // audio device. ViewModels::load performs the production clip/event decode.
    fn installed_handling_app(name: &str, rate: u32) -> (App, sim::PlayerId) {
        use std::{collections::BTreeMap, sync::Arc};
        let game = vfs::find_csgo_dir().expect("installed CS:GO Legacy");
        let mut installed = vfs::Vfs::new();
        installed.mount_game_dir(&game).unwrap();
        let schema = installed.read("scripts/items/items_game.txt").unwrap();
        let catalog = weapons::Catalog::from_items_game(&kv::decode_text(&schema)).unwrap();
        let def = catalog.get(name).unwrap().clone();
        let script = installed.read("scripts/game_sounds_weapons.txt").unwrap();
        let declarations = kv::parse(&kv::decode_text(&script)).unwrap();
        let model_bytes = installed.read(&def.view_model).unwrap();
        let model = mdl::Model::load_header(&model_bytes).unwrap();
        let mut files = BTreeMap::<String, Arc<[u8]>>::new();
        files.insert("scripts/items/items_game.txt".into(), schema.into());
        files.insert("scripts/game_sounds_weapons.txt".into(), script.into());
        files.insert(def.view_model.replace('\\', "/").to_ascii_lowercase(), model_bytes.into());
        let base = def.view_model.trim_end_matches(".mdl");
        for path in [format!("{base}.vvd"), format!("{base}.dx90.vtx")]
            .into_iter().chain((!model.anim_block_file.is_empty()).then(|| model.anim_block_file.clone())) {
            files.insert(path.replace('\\', "/").to_ascii_lowercase(), installed.read(&path).unwrap().into());
        }
        for event in model.sequences.iter().flat_map(|seq| &seq.events) {
            if let Some(wave) = event.client_sound().and_then(|entry|
                declarations.block(entry)).and_then(|entry| entry.str("wave")) {
                let path = format!("sound/{}", wave.trim_start_matches(['~', ')', '^', '*']));
                files.insert(path.replace('\\', "/").to_ascii_lowercase(), installed.read(&path).unwrap().into());
            }
        }
        let mut bsp = vec![0u8; 1036];
        bsp[..4].copy_from_slice(b"VBSP");
        bsp[4..8].copy_from_slice(&20i32.to_le_bytes());
        files.insert("maps/handling_fixture.bsp".into(), bsp.into());
        let mut fs = vfs::Vfs::new();
        fs.push_back("installed handling fixture", vfs::Mount::Memory(files));
        let mut level = level::Level::load_from_vfs(
            content::ContentId::csgo(content::ContentKind::Map, "handling_fixture"),
            "handling_fixture", game, fs).unwrap();
        Arc::get_mut(&mut level.catalog).unwrap().weapons.retain(|def| def.class_name == name);
        assert_eq!(level.catalog.weapons.len(), 1, "fixture must retain {name}");
        let full = mdl::Model::load(&level.vfs.read(&def.view_model).unwrap(),
            &level.vfs.read(&format!("{base}.vvd")).unwrap(),
            &level.vfs.read(&format!("{base}.dx90.vtx")).unwrap())
            .expect("fixture must mount a complete production weapon model");
        let draw = full.sequences.iter().find(|seq| seq.activity == "ACT_VM_DRAW").unwrap();
        let ani = (!full.anim_block_file.is_empty()).then(|| level.vfs.read(&full.anim_block_file)).flatten();
        full.decode_animation(draw.anims[0] as usize, ani.as_deref()).expect("fixture Draw must decode");
        assert_eq!(level.catalog.weapons[0].view_model, def.view_model);
        let views = crate::viewmodel::load(&level, &Default::default(),
            &mut Assets::<Mesh>::default(), &mut Assets::<crate::material::WorldMaterial>::default(),
            &mut Assets::<Image>::default(), &mut crate::textures::TextureCache::default(),
            &crate::world_mesh::SharedHandles { lightmap: Default::default() },
            &mut Assets::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>::default(), 1.);
        let sounds = SoundLibrary::load_with_era(&level.vfs, crate::audio::SoundEra::InstalledLegacy);
        let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
        world.set_tick_rate(rate);
        world.set_mode(sim::GameMode::Deathmatch, sim::RulesConfig::default());
        let id = world.add_player("handling dispatch", sim::Team::CounterTerrorist);
        let player = world.player_mut(id).unwrap();
        player.weapons = vec![sim::weapon::WeaponState::new(Arc::new(def))];
        player.active = 0;
        let mut app = App::new();
        app.insert_resource(SimWorld(world)).insert_resource(LocalPlayer::new(id, glam::Vec3::ZERO, 0.))
            .insert_resource(LoadedLevel(level)).insert_resource(views).insert_resource(sounds)
            .init_resource::<Assets<AudioSource>>().init_resource::<Foley>()
            .init_resource::<ViewModelState>().init_resource::<SurfaceProps>().init_resource::<Time>()
            .init_resource::<crate::effects::PendingEvents>().init_resource::<crate::effects::Effects>()
            .init_resource::<crate::tracers::Tracers>().init_resource::<crate::settings::Settings>()
            .init_resource::<crate::buymenu::BuyMenu>()
            .add_systems(FixedUpdate, update).add_systems(Update, crate::effects::process);
        (app, id)
    }

    fn handling_step(app: &mut App, id: sim::PlayerId, buttons: u32) {
        let events = {
            let mut world = app.world_mut().resource_mut::<SimWorld>();
            world.0.step(&[(id, sim::TickInput { buttons, ..Default::default() })]);
            world.0.events.clone()
        };
        app.world_mut().resource_mut::<crate::effects::PendingEvents>().0.extend(events);
        app.world_mut().run_schedule(FixedUpdate);
    }

    #[test]
    #[ignore = "requires installed CS:GO Legacy; source-only regression is UNRUN"]
    fn installed_manual_action_dispatch_interrupts_draw_before_fixed_catchup() {
        for rate in [64, 128] {
            for name in ["weapon_nova", "weapon_mag7"] {
                for catchup in [false, true] {
                    let (mut app, id) = installed_handling_app(name, rate);
                    let views = app.world().resource::<ViewModels>();
                    let (draw_duration, draw) = views.activity_sounds(name, "ACT_VM_DRAW").unwrap();
                    let (attack_duration, attack) = views.activity_sounds(name, "ACT_VM_PRIMARYATTACK").unwrap();
                    assert!(draw.iter().any(|(cycle, _)| *cycle > 0.));
                    assert!(!attack.is_empty());
                    // Exercise both a presented Draw onset and a first shot
                    // before foley has even observed the initial deployment.
                    if !catchup { app.world_mut().run_schedule(FixedUpdate); }
                    handling_step(&mut app, id, movement::buttons::ATTACK);
                    assert_eq!(app.world().resource::<SimWorld>().0.player(id).unwrap().shots, 1);
                    if catchup {
                        while app.world().resource::<SimWorld>().0.time() < draw_duration {
                            handling_step(&mut app, id, 0);
                        }
                    }
                    let shot_scheduled = app.world().resource::<SimWorld>().0.time();
                    app.update(); // actual effects::process drains the confirmed Fire
                    while app.world().resource::<SimWorld>().0.time() < shot_scheduled + attack_duration + 0.1 {
                        handling_step(&mut app, id, 0);
                        app.update();
                    }
                    let expected: Vec<_> = draw.iter().filter(|(cycle, _)| !catchup && *cycle == 0.)
                        .map(|(_, entry)| (id, name.to_string(), entry.clone(), 0.))
                        .chain(attack.iter().map(|(cycle, entry)|
                            (id, name.to_string(), entry.clone(), shot_scheduled + cycle * attack_duration))).collect();
                    assert_eq!(app.world().resource::<Foley>().dispatched, expected,
                        "{name} {rate}Hz catchup={catchup}: only the interrupted Draw tail may disappear");
                    // Repeated scheduling/dispatch timestamps cannot replay events.
                    app.world_mut().run_schedule(FixedUpdate);
                    app.update();
                    assert_eq!(app.world().resource::<Foley>().dispatched, expected);
                    assert!(!app.world().resource::<Assets<AudioSource>>().is_empty(),
                        "installed handling cues must reach real audio-source admission without a device");
                    assert!(app.world_mut().query::<&bevy::audio::AudioPlayer>()
                        .iter(app.world()).next().is_some(), "actual dispatch must admit an AudioPlayer");
                }
            }
        }
    }

    #[test]
    #[ignore = "requires installed CS:GO Legacy; source-only regression is UNRUN"]
    fn installed_uninterrupted_draw_dispatches_all_authored_events() {
        for name in ["weapon_nova", "weapon_mag7"] {
            let (mut app, id) = installed_handling_app(name, 128);
            let (duration, events) = app.world().resource::<ViewModels>()
                .activity_sounds(name, "ACT_VM_DRAW").unwrap();
            app.world_mut().run_schedule(FixedUpdate);
            while app.world().resource::<SimWorld>().0.time() < duration + 0.1 {
                handling_step(&mut app, id, 0);
                app.update();
            }
            assert_eq!(app.world().resource::<Foley>().dispatched, events.iter()
                .map(|(cycle, entry)| (id, name.to_string(), entry.clone(), cycle * duration)).collect::<Vec<_>>());
        }
    }

    #[test]
    fn manual_action_interruption_preserves_repeated_attack_reload_and_other_owner_cues() {
        let mut foley = Foley::default();
        // Same entry at distinct authored cycles is deliberate. Cancellation
        // follows the interrupted playback, never a waveform/name/time window.
        let repeated = [(0.3, "pump".into()), (0.6, "pump".into())];
        foley.deploy(1, "weapon_mag7", 0., 1., &repeated);
        foley.deploy(2, "weapon_mag7", 0., 1., &repeated);
        foley.deploy(1, "weapon_awp", 0., 1., &repeated);
        foley.schedule(1, "weapon_mag7", 0., 1., &repeated, true);
        foley.primary_attack(1, "weapon_mag7", 0., 1., &repeated);
        foley.interrupt_deploys(&[]);
        assert_eq!(foley.queue.len(), 10);
        foley.interrupt_deploys(&[sim::GameEvent::Fire { player: 1,
            weapon: "weapon_mag7".into(), origin: glam::Vec3::ZERO, suppressed: false }]);
        assert_eq!(foley.queue.len(), 8);
        assert!(!foley.queue.iter().any(|q| q.player == 1 && q.weapon == "weapon_mag7" && q.deploy));
        for cues in [
            foley.queue.iter().filter(|q| q.player == 2).collect::<Vec<_>>(),
            foley.queue.iter().filter(|q| q.weapon == "weapon_awp").collect(),
            foley.queue.iter().filter(|q| q.reload).collect(),
            foley.queue.iter().filter(|q| q.primary_attack).collect(),
        ] {
            assert_eq!(cues.iter().map(|q| (q.at, q.entry.as_str())).collect::<Vec<_>>(),
                vec![(0.3, "pump"), (0.6, "pump")]);
        }
    }

    // Reference to the previous per-tick loop: protects the measured offline
    // onset, threshold transitions, alternating feet and cadence together.
    fn offline_reference(state: &mut PlayerFoley, grounded: bool, speed: f32, dt: f32) -> Option<bool> {
        if grounded && speed > AUDIBLE_SPEED {
            let period = step_period(speed);
            if !state.moving { state.moving = true; state.step_timer = period; }
            state.step_timer -= dt;
            if state.step_timer <= 0.0 {
                state.step_timer = (state.step_timer + period).max(0.0);
                state.left = !state.left;
                return Some(state.left);
            }
        } else { state.moving = false; }
        None
    }

    #[test]
    fn offline_tick_sequence_preserves_previous_onset_and_cadence() {
        for rate in [64, 128] {
            let mut clock = Foley::default();
            let mut actual = PlayerFoley::default();
            let mut previous = PlayerFoley::default();
            for tick in 0..rate as u64 * 12 {
                let phase = tick / rate as u64;
                let speed = match phase { 0 => 135.0, 1..=3 => 140.0, 4..=6 => 250.0, 7 => 130.0, _ => 215.0 };
                let grounded = phase != 9;
                assert_eq!(actual.footstep(grounded, speed, clock.advance(tick, rate).unwrap()),
                    offline_reference(&mut previous, grounded, speed, 1.0 / rate as f32),
                    "rate={rate} tick={tick}");
                assert_eq!(actual.step_timer, previous.step_timer);
            }
        }
    }

    fn steps(rate: u32, interval: u64, speed: f32) -> Vec<(u64, bool)> {
        let mut clock = Foley::default();
        let mut player = PlayerFoley::default();
        (0..=rate as u64 * 12).step_by(interval as usize).filter_map(|tick| {
            player.footstep(true, speed, clock.advance(tick, rate).unwrap()).map(|left| (tick, left))
        }).collect()
    }

    #[test]
    fn steady_snapshot_cadence_matches_ticks_with_observation_quantization() {
        for rate in [64, 128] {
            for speed in [140.0, 215.0, 250.0] {
                let ticks = steps(rate, 1, speed);
                for interval in [4, 8] {
                    let snapshots = steps(rate, interval, speed);
                    assert_eq!(snapshots.len(), ticks.len(), "rate={rate} speed={speed} interval={interval}");
                    for ((tick, left), (observed, observed_left)) in ticks.iter().zip(snapshots) {
                        assert_eq!(*left, observed_left);
                        assert_eq!(observed, tick.div_ceil(interval) * interval);
                    }
                }
            }
        }
    }

    #[test]
    fn duplicate_ticks_and_large_gaps_do_not_backfill_steps() {
        let mut clock = Foley::default();
        let mut player = PlayerFoley::default();
        assert_eq!(player.footstep(true, 250.0, clock.advance(0, 128).unwrap()), None);
        assert!(clock.advance(0, 128).is_none());
        assert_eq!(player.footstep(true, 250.0, clock.advance(1280, 128).unwrap()), Some(true));
        assert_eq!(player.step_timer, FAST_STEP);
        assert!(clock.advance(1280, 128).is_none());
        assert_eq!(player.footstep(true, 250.0, clock.advance(1281, 128).unwrap()), None);
    }

    #[test]
    fn newly_observed_motion_does_not_charge_the_unknown_snapshot_gap() {
        let mut clock = Foley::default();
        let mut player = PlayerFoley::default();
        assert_eq!(player.footstep(true, 135.0, clock.advance(0, 128).unwrap()), None);
        assert_eq!(player.footstep(true, 250.0, clock.advance(1280, 128).unwrap()), None);
        assert_eq!(player.step_timer, FAST_STEP - 1.0 / 128.0);
        assert_eq!(player.footstep(false, 250.0, clock.advance(1281, 128).unwrap()), None);
        assert_eq!(player.footstep(true, 250.0, clock.advance(2560, 128).unwrap()), None);
        assert_eq!(player.step_timer, FAST_STEP - 1.0 / 128.0);
    }

    fn tracked_world() -> (sim::World, Foley, sim::PlayerId, sim::PlayerId) {
        let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
        let first = world.add_player("first", sim::Team::CounterTerrorist);
        let second = world.add_player("second", sim::Team::CounterTerrorist);
        let mut foley = Foley::default();
        for player in &mut world.players {
            player.alive = true;
            foley.players.insert(player.id, PlayerFoley {
                life_epoch: Some(player.life_epoch), moving: true, step_timer: 0.001, left: true,
                weapon: "weapon_ak47".into(), reloading: true, ..PlayerFoley::default()
            });
            foley.schedule(player.id, "weapon_ak47", 0.0, 1.0, &[(0.5, "reload".into())], true);
            foley.schedule(player.id, "weapon_ak47", 0.0, 1.0, &[(0.8, "deploy".into())], false);
        }
        (world, foley, first, second)
    }

    fn awp_world() -> (sim::World, Foley, sim::PlayerId, sim::PlayerId) {
        let (mut world, mut foley, first, second) = tracked_world();
        let catalog = weapons::Catalog::from_items_game(r#"items_game {
            items { 9 { name weapon_awp baseitem 1
                attributes { "primary clip size" 10 cycletime 1.455 } } }
        }"#).unwrap();
        for player in &mut world.players {
            player.weapons = vec![sim::weapon::WeaponState::new(std::sync::Arc::new(
                catalog.get("weapon_awp").unwrap().clone()))];
            player.active = 0;
            foley.players.get_mut(&player.id).unwrap().weapon = "weapon_awp".into();
        }
        (world, foley, first, second)
    }

    #[test]
    fn awp_primary_sound_queue_uses_authored_cycles_and_replaces_only_its_own_attack() {
        let (world, mut foley, first, second) = awp_world();
        // Independently inspected Legacy awp_fire: 51 frames at 30fps;
        // the two sound events are at cycles .36 and .56, with no fire cue.
        let duration = 50. / 30.;
        let events = [(0.36, "Weapon_AWP.BoltBack".into()), (0.56, "Weapon_AWP.BoltForward".into())];
        foley.primary_attack(first, "weapon_awp", 10., duration, &events);
        foley.primary_attack(second, "weapon_awp", 10., duration, &events);
        foley.retain_lives(&world.players);
        let own: Vec<_> = foley.queue.iter().filter(|q| q.player == first && q.primary_attack).collect();
        assert_eq!(own.len(), 2);
        assert_eq!(own[0].entry, "Weapon_AWP.BoltBack");
        assert!((own[0].at - 10.6).abs() < 0.00001);
        assert_eq!(own[1].entry, "Weapon_AWP.BoltForward");
        assert!((own[1].at - (10. + 28. / 30.)).abs() < 0.00001);
        foley.primary_attack(first, "weapon_awp", 12., duration, &events);
        assert_eq!(foley.queue.iter().filter(|q| q.player == first && q.primary_attack).count(), 2);
        assert!(foley.queue.iter().filter(|q| q.player == first && q.primary_attack).all(|q| q.at > 12.));
        assert_eq!(foley.queue.iter().filter(|q| q.player == second && q.primary_attack).count(), 2);
        assert_eq!(foley.queue.iter().filter(|q| !q.primary_attack).count(), 4,
            "replacing a primary clip must preserve unrelated deploy/reload cues");
    }

    #[test]
    fn awp_bolt_sounds_cancel_on_reload_holster_death_and_life_change() {
        for interruption in 0..4 {
            let (mut world, mut foley, first, second) = awp_world();
            for player in [first, second] {
                foley.primary_attack(player, "weapon_awp", 10., 50. / 30.,
                    &[(0.36, "Weapon_AWP.BoltBack".into()), (0.56, "Weapon_AWP.BoltForward".into())]);
            }
            let player = world.player_mut(first).unwrap();
            match interruption {
                0 => player.weapons[0].reload_end = Some(14.),
                1 => player.weapons.clear(),
                2 => player.alive = false,
                _ => player.life_epoch += 1,
            }
            foley.retain_lives(&world.players);
            assert!(!foley.queue.iter().any(|q| q.player == first && q.primary_attack));
            assert_eq!(foley.queue.iter().filter(|q| q.player == second && q.primary_attack).count(), 2);
        }
    }

    #[test]
    fn installed_awp_attack_declares_both_bolt_events_and_resolves_the_legacy_samples() {
        let Some(game) = vfs::find_csgo_dir() else { return; };
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&game).unwrap();
        let bytes = fs.read("models/weapons/v_snip_awp.mdl").unwrap();
        let model = mdl::Model::load_header(&bytes).unwrap();
        let attack = model.sequences.iter().find(|s| s.activity == "ACT_VM_PRIMARYATTACK").unwrap();
        let ani = (!model.anim_block_file.is_empty()).then(|| fs.read(&model.anim_block_file)).flatten();
        let clip = model.decode_animation(attack.anims[0] as usize, ani.as_deref()).unwrap();
        let events: Vec<_> = attack.events.iter().filter_map(|e|
            Some((e.cycle, e.client_sound()?.to_string()))).collect();
        assert_eq!(events, [(0.36, "Weapon_AWP.BoltBack".to_string()), (0.56, "Weapon_AWP.BoltForward".to_string())]);
        assert!((clip.duration() - 50. / 30.).abs() < 0.00001);
        let script = kv::parse(&fs.read_string("scripts/game_sounds_weapons.txt").unwrap()).unwrap();
        for (entry, sample) in [("Weapon_AWP.BoltBack", "weapons/awp/awp_boltback.wav"),
            ("Weapon_AWP.BoltForward", "weapons/awp/awp_boltforward.wav")] {
            assert_eq!(script.block(entry).unwrap().str("wave"), Some(format!("~{sample}").as_str()));
            assert!(!fs.read(&format!("sound/{sample}")).unwrap().is_empty());
        }
    }

    #[test]
    fn changed_life_even_at_duplicate_tick_discards_old_animation_and_cadence() {
        let (mut world, mut foley, first, second) = tracked_world();
        foley.advance(10, 128);
        world.player_mut(first).unwrap().life_epoch += 1;
        assert!(foley.advance(10, 128).is_none());
        foley.retain_lives(&world.players);
        assert!(!foley.players.contains_key(&first));
        assert!(foley.players.contains_key(&second));
        assert!(foley.queue.iter().all(|sound| sound.player == second));
        let mut new_life = PlayerFoley::default();
        assert_eq!(new_life.footstep(true, 250.0, foley.advance(100, 128).unwrap()), None);
        assert!(!new_life.left);
        assert_eq!(new_life.step_timer, FAST_STEP - 1.0 / 128.0);
    }

    #[test]
    fn death_and_removal_discard_only_their_owned_queued_sounds() {
        for removed in [false, true] {
            let (mut world, mut foley, first, second) = tracked_world();
            if removed { world.players.retain(|player| player.id != first); }
            else { world.player_mut(first).unwrap().alive = false; }
            foley.retain_lives(&world.players);
            assert!(!foley.players.contains_key(&first));
            assert!(foley.players.contains_key(&second));
            assert_eq!(foley.queue.len(), 2);
            assert!(foley.queue.iter().all(|sound| sound.player == second));
        }
    }

    #[test]
    fn clock_reversal_and_rate_change_reanchor_and_discard_old_queues() {
        for (tick, rate) in [(9, 128), (11, 64)] {
            let (_, mut foley, _, _) = tracked_world();
            foley.advance(10, 128);
            let elapsed = foley.advance(tick, rate).unwrap();
            assert_eq!(elapsed.seconds, 1.0 / rate as f32);
            assert!(foley.players.is_empty());
            assert!(foley.queue.is_empty());
        }
    }

    #[test]
    fn step_cadence_matches_measurements() {
        assert_eq!(step_period(250.0), FAST_STEP);
        assert_eq!(step_period(215.0), SLOW_STEP);
        assert!(130.0 <= AUDIBLE_SPEED && AUDIBLE_SPEED < 140.0);
    }
}

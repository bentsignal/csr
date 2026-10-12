//! Opt-in normal-input bot play/Pause proof. No screenshot-mode settings,
//! forced window focus, gameplay mutation, or OS-input claim.
use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::MouseButtonInput,
    },
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use std::{path::PathBuf, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Loading,
    Initial,
    Grab,
    Release,
    Move,
    Fire,
    Reload,
    Reloading,
    Played,
    Pause,
    Paused,
    Resume,
    Resumed,
    Done,
}
#[derive(Clone, Copy)]
enum Input {
    Key(KeyCode, bool),
    Mouse(bool),
}
// Bounded exact samples: overflow fails the proof rather than hiding later gaps.
const MAX_CADENCE_SAMPLES: usize = 32_768;
#[derive(Default)]
struct Cadence {
    samples: u64,
    total: f64,
    maximum: f64,
    intervals: Vec<f64>,
    overflow: bool,
}
impl Cadence {
    fn record(&mut self, elapsed: f64) {
        if elapsed.is_finite() && elapsed > 0.0 {
            self.samples += 1;
            self.total += elapsed;
            self.maximum = self.maximum.max(elapsed);
            if self.intervals.len() < MAX_CADENCE_SAMPLES {
                self.intervals.push(elapsed);
            } else {
                self.overflow = true;
            }
        }
    }
    fn mean(&self) -> f64 {
        self.total / self.samples.max(1) as f64
    }
    fn p95(&self) -> f64 {
        if self.intervals.is_empty() {
            return f64::INFINITY;
        }
        let mut sorted = self.intervals.clone();
        sorted.sort_unstable_by(f64::total_cmp);
        sorted[(sorted.len() * 95).div_ceil(100) - 1]
    }
    fn summary(&self) -> String {
        format!(
            "samples={} elapsed_s={:.3} mean_ms={:.3} p95_ms={:.3} max_ms={:.3} overflow={}",
            self.samples,
            self.total,
            self.mean() * 1000.0,
            self.p95() * 1000.0,
            self.maximum * 1000.0,
            self.overflow
        )
    }
    fn responsive(&self) -> bool {
        !self.overflow
            && self.total >= 3.0
            && self.samples >= 20
            && self.mean() <= 1.0 / 30.0
            && self.p95() <= 0.050
            && self.maximum <= 0.250
    }
}
#[derive(Resource)]
struct Proof {
    output: PathBuf,
    born: Instant,
    since: Instant,
    last_frame: Instant,
    stage: Stage,
    frames: u32,
    inputs: Vec<Input>,
    captures: u8,
    requested: u8,
    position: glam::Vec3,
    life: u64,
    shots: u64,
    clip: i32,
    tick: u64,
    serial: u64,
    pause_latency: f64,
    cadence: Cadence,
}
impl Proof {
    fn enter(&mut self, stage: Stage) {
        self.stage = stage;
        self.since = Instant::now();
        self.frames = 0;
    }
    fn elapsed(&self) -> f64 {
        self.since.elapsed().as_secs_f64()
    }
    fn key(&mut self, key: KeyCode, down: bool) {
        self.inputs.push(Input::Key(key, down));
    }
}

pub(crate) fn install(app: &mut App) {
    let Some(output) = std::env::var_os("CSR_TEST_BOT_PERF") else {
        return;
    };
    let now = Instant::now();
    app.insert_resource(Proof {
        output: output.into(),
        born: now,
        since: now,
        last_frame: now,
        stage: Stage::Loading,
        frames: 0,
        inputs: Vec::new(),
        captures: 0,
        requested: 0,
        position: glam::Vec3::ZERO,
        life: 0,
        shots: 0,
        clip: 0,
        tick: 0,
        serial: 0,
        pause_latency: 0.0,
        cadence: Cadence::default(),
    })
    .add_systems(First, emit)
    .add_systems(
        Update,
        observe
            .after(crate::frontend::input)
            .after(crate::player::update_camera),
    );
}

// First precedes InputPlugin's PreUpdate: these messages exercise its normal
// ButtonInput state, then the production look/grab/sample/fixed-tick chain.
fn emit(world: &mut World) {
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .iter(world)
        .next();
    let Some(window) = window else {
        return;
    };
    let inputs = std::mem::take(&mut world.resource_mut::<Proof>().inputs);
    for input in inputs {
        match input {
            Input::Key(key_code, down) => {
                world.write_message(KeyboardInput {
                    key_code,
                    logical_key: if key_code == KeyCode::Escape {
                        Key::Escape
                    } else {
                        Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified)
                    },
                    state: if down {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    },
                    text: None,
                    repeat: false,
                    window,
                });
            }
            Input::Mouse(down) => {
                world.write_message(MouseButtonInput {
                    button: MouseButton::Left,
                    state: if down {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    },
                    window,
                });
            }
        }
    }
}

fn capture(world: &mut World, proof: &mut Proof, flag: u8, label: &str) {
    if proof.requested & flag != 0 {
        return;
    }
    proof.requested |= flag;
    let path = proof.output.join(format!("{label}.png"));
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(move |_: On<ScreenshotCaptured>, mut proof: ResMut<Proof>| {
            proof.captures |= flag;
        });
}

fn observe(world: &mut World) {
    let Some(mut proof) = world.remove_resource::<Proof>() else {
        return;
    };
    if proof.stage == Stage::Done {
        world.insert_resource(proof);
        return;
    }
    let now = Instant::now();
    let frame = now.duration_since(proof.last_frame).as_secs_f64();
    proof.last_frame = now;
    proof.frames = proof.frames.saturating_add(1);
    let result = step(world, &mut proof, frame);
    if let Err(error) = result {
        error!("BOT_PERF failures: 1 stage={:?}: {error}", proof.stage);
        proof.stage = Stage::Done;
        world.write_message(AppExit::error());
    }
    world.insert_resource(proof);
}

fn step(world: &mut World, p: &mut Proof, frame: f64) -> Result<(), String> {
    if p.born.elapsed().as_secs_f64() > 120.0 {
        return Err("watchdog: missing state/input/render acknowledgment".into());
    }
    let opts = &world.resource::<crate::Options>().0;
    if opts.screenshot.is_some() || !opts.direct || !opts.deathmatch || opts.bots != 3 {
        return Err(
            "requires play de_dust2 --direct --deathmatch --bots 3 without --screenshot".into(),
        );
    }
    if !world.resource::<crate::player::InputEnabled>().0 {
        return Err("normal input disabled".into());
    }
    if *world.resource::<State<crate::session::AppState>>().get() != crate::session::AppState::Match
    {
        return Ok(());
    }
    let Some(local) = world.get_resource::<crate::player::LocalPlayer>() else {
        return Ok(());
    };
    let sim = &world.resource::<crate::player::SimWorld>().0;
    let me = sim.player(local.id).ok_or("local actor missing")?;
    if me.is_bot || sim.players.iter().filter(|a| a.is_bot).count() != 5 {
        return Err("expected one human and five bot actors".into());
    }
    let (alive, life, position, shots, tick) = (
        me.alive,
        me.life_epoch,
        me.movement.origin,
        me.shots,
        sim.tick,
    );
    let weapon = me.active_weapon();
    let (clip, reloading) = weapon.map_or((0, false), |w| (w.clip, w.reload_end.is_some()));
    let menu = world.resource::<crate::menu::Menu>();
    let (in_match, page) = (menu.in_match, menu.page);
    let paused = world.resource::<Time<Virtual>>().is_paused();
    let binds = world.resource::<crate::settings::Settings>().binds;
    let (focused, grabbed) = world
        .query_filtered::<(&Window, &CursorOptions), With<PrimaryWindow>>()
        .iter(world)
        .next()
        .map(|(w, c)| (w.focused, c.grab_mode != CursorGrabMode::None))
        .ok_or("window missing")?;
    let front = world
        .get_resource::<crate::frontend::Frontend>()
        .ok_or("classic frontend missing")?;
    if let Some(error) = &front.failed {
        return Err(format!("classic frontend failed: {error}"));
    }
    let (serial, surface) = (front.presented_serial(), front.pause_surface_visible());
    if p.stage != Stage::Loading && (!alive || life != p.life) {
        return Err("local actor died/changed life during input proof".into());
    }
    if matches!(
        p.stage,
        Stage::Move | Stage::Fire | Stage::Reload | Stage::Reloading | Stage::Resumed
    ) {
        p.cadence.record(frame);
    }
    match p.stage {
        Stage::Loading
            if in_match && alive && focused && !paused && page.is_none() && p.frames > 3 =>
        {
            std::fs::create_dir_all(&p.output).map_err(|e| e.to_string())?;
            p.position = position;
            p.life = life;
            p.shots = shots;
            p.clip = clip;
            let w = world
                .query_filtered::<&Window, With<PrimaryWindow>>()
                .iter(world)
                .next()
                .unwrap();
            info!(
                "BOT_PERF normal focused={} physical={}x{} scale={} bots=5 tick={tick}",
                w.focused,
                w.physical_width(),
                w.physical_height(),
                w.scale_factor()
            );
            capture(world, p, 1, "01-ready");
            p.enter(Stage::Initial);
        }
        Stage::Initial if p.captures & 1 != 0 => {
            p.inputs.push(Input::Mouse(true));
            p.enter(Stage::Grab);
        }
        Stage::Grab if p.frames >= 2 => {
            if !grabbed {
                return Err("capture click did not grab cursor".into());
            }
            if shots != p.shots {
                return Err("capture click fired a shot".into());
            }
            p.inputs.push(Input::Mouse(false));
            p.enter(Stage::Release);
        }
        Stage::Release if p.frames >= 2 && p.elapsed() >= 0.25 => {
            p.key(binds[0], true);
            p.enter(Stage::Move);
        }
        Stage::Move if p.elapsed() >= 1.0 => {
            p.key(binds[0], false);
            if position.distance(p.position) < 8.0 {
                return Err("normal movement input produced no displacement".into());
            }
            info!(
                "BOT_PERF movement={} tick={tick}",
                position.distance(p.position)
            );
            p.inputs.push(Input::Mouse(true));
            p.enter(Stage::Fire);
        }
        Stage::Fire if shots > p.shots && clip < p.clip => {
            p.inputs.push(Input::Mouse(false));
            p.key(binds[8], true);
            p.enter(Stage::Reload);
        }
        Stage::Fire if p.elapsed() > 5.0 => {
            return Err("fresh attack did not fire/consume ammo".into());
        }
        Stage::Reload if reloading => {
            p.key(binds[8], false);
            p.enter(Stage::Reloading);
        }
        Stage::Reload if p.elapsed() > 2.0 => {
            return Err("normal reload key did not start reload".into());
        }
        Stage::Reloading if !reloading && clip >= p.clip => {
            info!(
                "BOT_PERF fired={} reload_clip={clip} tick={tick}",
                shots - p.shots
            );
            capture(world, p, 2, "02-played");
            p.enter(Stage::Played);
        }
        Stage::Reloading if p.elapsed() > 8.0 => return Err("reload did not complete".into()),
        Stage::Played if p.captures & 2 != 0 => {
            p.serial = serial;
            p.key(KeyCode::Escape, true);
            p.enter(Stage::Pause);
        }
        Stage::Pause => {
            if p.frames == 1 {
                p.key(KeyCode::Escape, false);
            }
            if page == Some(crate::menu::Page::Pause) && paused && surface && serial >= p.serial + 2
            {
                p.pause_latency = p.elapsed();
                p.tick = tick;
                capture(world, p, 4, "03-pause");
                p.enter(Stage::Paused);
            } else if p.elapsed() > 3.0 {
                return Err(format!(
                    "Pause not presented promptly page={page:?} paused={paused} surface={surface} serial={serial}"
                ));
            }
        }
        Stage::Paused if p.captures & 4 != 0 && p.elapsed() >= 0.25 => {
            if !paused || tick != p.tick {
                return Err("offline simulation advanced while paused".into());
            }
            p.key(KeyCode::Escape, true);
            p.enter(Stage::Resume);
        }
        Stage::Resume => {
            if p.frames == 1 {
                p.key(KeyCode::Escape, false);
            }
            if page.is_none() && !paused && tick > p.tick {
                p.enter(Stage::Resumed);
            } else if p.elapsed() > 3.0 {
                return Err("Escape did not resume simulation promptly".into());
            }
        }
        Stage::Resumed if p.elapsed() >= 1.0 && p.cadence.total >= 3.0 => {
            capture(world, p, 8, "04-resumed");
            if p.captures & 8 != 0 {
                if !p.cadence.responsive() {
                    return Err(format!(
                        "game frame gaps exceeded bounds: {}",
                        p.cadence.summary()
                    ));
                }
                info!(
                    "BOT_PERF failures: 0 captures=4 bots=5 tick={tick} pause_ms={:.1} {}; engine input, no OS-input/GPU-present claim",
                    p.pause_latency * 1000.0,
                    p.cadence.summary()
                );
                p.stage = Stage::Done;
                world.write_message(AppExit::Success);
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cadence_rejects_a_stall_even_with_fast_average() {
        let mut c = Cadence::default();
        for _ in 0..4000 {
            c.record(0.001);
        }
        assert!(c.responsive());
        c.record(0.251);
        assert!(!c.responsive());
    }
    #[test]
    fn cadence_rejects_slow_mean_and_tail_independently() {
        let mut slow = Cadence::default();
        for _ in 0..100 {
            slow.record(0.04);
        }
        assert!(slow.p95() <= 0.05);
        assert!(!slow.responsive());
        let mut tail = Cadence::default();
        for _ in 0..180 {
            tail.record(0.02);
        }
        for _ in 0..20 {
            tail.record(0.10);
        }
        assert!(tail.mean() < 1.0 / 30.0);
        assert_eq!(tail.p95(), 0.10);
        assert!(!tail.responsive());
    }
    #[test]
    fn cadence_requires_three_seconds_and_fails_closed_on_sample_overflow() {
        let mut c = Cadence::default();
        for _ in 0..20 {
            c.record(0.02);
        }
        assert!(!c.responsive());
        for _ in 20..MAX_CADENCE_SAMPLES {
            c.record(0.001);
        }
        assert!(c.responsive());
        c.record(0.001);
        assert!(c.overflow);
        assert_eq!(c.intervals.len(), MAX_CADENCE_SAMPLES);
        assert!(!c.responsive());
    }
    #[test]
    fn cadence_needs_observations_and_ignores_invalid_samples() {
        let mut c = Cadence::default();
        for x in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
            c.record(x);
        }
        assert_eq!(c.samples, 0);
        assert!(!c.responsive());
        for _ in 0..20 {
            c.record(0.2);
        }
        assert!(!c.responsive());
    }
}

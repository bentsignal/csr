//! Bounded, opt-in rendered functional acceptance. Public input and read-only
//! observations only; this does not measure native/physical input or fidelity.
use crate::{frontend::Frontend, menu::{Action, ControlsScroll, Menu, Page},
    player::SimWorld, session::AppState, settings::{ACTIONS, Settings}};
use bevy::{input::{ButtonState, keyboard::{Key, KeyboardInput},
    mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel}}, prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured}, window::PrimaryWindow};
use std::{path::{Path, PathBuf}, time::{Duration, Instant}};

pub(crate) fn enabled() -> bool {
    crate::frontend_flow::enabled() && std::env::var_os("CSR_TEST_FLOW_CASE").is_some()
}
pub(crate) fn match_case() -> bool {
    std::env::var("CSR_TEST_FLOW_CASE").as_deref() == Ok("match-options")
}
pub(crate) fn install(app: &mut App) {
    app.init_resource::<Proof>()
        // Messages reach InputPlugin in PreUpdate, then Bevy UI focus, then the
        // unchanged menu controller in RunFixedMainLoop. No Interaction writes.
        .add_systems(First, deliver)
        .add_systems(PostUpdate, update.after(bevy::ui::UiSystems::PostLayout)
            .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate));
}

#[derive(Clone, Copy)]
enum PublicInput { Key(KeyCode), Click(Vec2), Wheel(Vec2, f32), Resize(u32, u32) }
#[derive(Clone, Copy)]
enum Release { Key(KeyCode), Mouse }

#[derive(Default)]
struct Stable { last: u64, fresh: u8, since: Option<Duration> }
impl Stable {
    fn observe(&mut self, valid: bool, serial: u64, now: Duration) -> bool {
        if !valid { *self = Self { last: serial, ..default() }; return false; }
        let since = *self.since.get_or_insert(now);
        if serial != self.last { self.last = serial; self.fresh = self.fresh.saturating_add(1); }
        self.fresh >= 3 && now.saturating_sub(since) >= Duration::from_millis(350)
    }
}

#[derive(Resource)]
struct Proof {
    case: String, phase: usize, cycle: usize, start: Instant, entered: Instant,
    output: PathBuf, validated: bool, finished: bool, pending: Option<PublicInput>,
    release: Option<Release>, gate: Stable, pending_capture: bool, settled_capture: bool,
    waiting_capture: bool, capture_error: Option<String>, baseline: Option<Settings>,
    probe_id: u64, awaiting_probe: bool, probe_serial: u64, sample: Vec<String>,
    next_probe: Instant, sample_count: u8, tick: u64,
}
impl Default for Proof {
    fn default() -> Self {
        Self { case: std::env::var("CSR_TEST_FLOW_CASE").unwrap_or_default(), phase: 0,
            cycle: 0, start: Instant::now(), entered: Instant::now(),
            output: std::env::var_os("CSR_TEST_FLOW_OUTPUT").map(PathBuf::from).unwrap_or_default(),
            validated: false, finished: false, pending: None, release: None, gate: default(),
            pending_capture: false, settled_capture: false, waiting_capture: false,
            capture_error: None, baseline: None, probe_id: 0, awaiting_probe: false,
            probe_serial: 0, sample: vec![], next_probe: Instant::now(), sample_count: 0, tick: 0 }
    }
}
impl Proof {
    fn next(&mut self) {
        self.enter(self.phase + 1);
    }
    fn enter(&mut self, phase: usize) {
        self.phase = phase; self.entered = Instant::now(); self.gate = default();
        self.settled_capture = false; self.waiting_capture = false; self.sample.clear();
        self.awaiting_probe = false; self.sample_count = 0; self.next_probe = Instant::now();
        self.probe_id += 1; self.probe_serial = 0;
        info!(case = self.case, cycle = self.cycle, phase = self.phase, "rendered acceptance: phase");
    }
    fn fail(&mut self, exit: &mut MessageWriter<AppExit>, reason: impl std::fmt::Display) {
        error!(case = self.case, phase = self.phase, cycle = self.cycle, %reason,
            "rendered acceptance: failures: 1");
        self.finished = true; exit.write(AppExit::error());
    }
    fn capture(&mut self, commands: &mut Commands, window: &Window, kind: &str) {
        let path = self.output.join(format!("{}-{}-{:02}-{kind}.png", self.case, self.cycle, self.phase));
        let expected = UVec2::new(window.physical_width(), window.physical_height());
        self.pending_capture = true;
        commands.spawn(Screenshot::primary_window()).observe(
            move |event: On<ScreenshotCaptured>, mut proof: ResMut<Proof>| {
                let size = event.image.size();
                let result = if size != expected {
                    Err(format!("capture {path:?}: expected {expected:?}, got {size:?}"))
                } else {
                    event.image.clone().try_into_dynamic().map_err(|e| e.to_string())
                        .and_then(|image| image.into_rgb8().save(&path).map_err(|e| e.to_string()))
                };
                if let Err(error) = result { proof.capture_error = Some(error); }
                proof.pending_capture = false;
            });
    }
    // Every ordinary phase gets a settled screenshot before its input is sent.
    // Capture completion blocks subsequent inputs and final success.
    fn admitted(&mut self, valid: bool, serial: u64, commands: &mut Commands, window: &Window) -> bool {
        if !self.gate.observe(valid, serial, self.start.elapsed()) {
            if !self.waiting_capture && self.entered.elapsed() >= Duration::from_secs(1) {
                self.capture(commands, window, "waiting"); self.waiting_capture = true;
            }
            return false;
        }
        if !self.settled_capture {
            self.capture(commands, window, "settled"); self.settled_capture = true; return false;
        }
        true
    }
    fn issue(&mut self, input: PublicInput) { self.pending = Some(input); self.next(); }
}

// Same isolation guard as the existing display proof; the caller prepares the
// private fixture. This harness never selects or backs up a real player config.
fn private_data(output: &Path) -> Result<(), String> {
    if !output.is_absolute() { return Err("CSR_TEST_FLOW_OUTPUT must be absolute".into()); }
    let expected = std::fs::canonicalize(output.join("userdata")).map_err(|e| e.to_string())?;
    let actual = std::env::current_dir().and_then(std::fs::canonicalize).map_err(|e| e.to_string())?;
    let declared = std::env::var_os("CSR_DATA_DIR").ok_or("CSR_DATA_DIR must select private userdata")?;
    if actual != expected || std::fs::canonicalize(declared).map_err(|e| e.to_string())? != expected {
        return Err("cwd and CSR_DATA_DIR must equal OUTPUT/userdata".into());
    }
    Ok(())
}

fn deliver(mut proof: ResMut<Proof>, mut window: Single<(Entity, &mut Window), With<PrimaryWindow>>,
    mut keys: MessageWriter<KeyboardInput>, mut mouse: MessageWriter<MouseButtonInput>,
    mut wheel: MessageWriter<MouseWheel>) {
    let (entity, window) = &mut *window;
    let key_event = |code, state| KeyboardInput { key_code: code, logical_key: match code {
        KeyCode::Escape => Key::Escape, KeyCode::Home => Key::Home, KeyCode::End => Key::End,
        KeyCode::F11 => Key::F11, _ => Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
    }, state, text: None, repeat: false, window: *entity };
    if let Some(release) = proof.release.take() {
        match release {
            Release::Key(code) => { keys.write(key_event(code, ButtonState::Released)); }
            Release::Mouse => { mouse.write(MouseButtonInput { button: MouseButton::Left,
                state: ButtonState::Released, window: *entity }); }
        }
        return;
    }
    if proof.finished { return; }
    match proof.pending.take() {
        Some(PublicInput::Key(code)) => {
            keys.write(key_event(code, ButtonState::Pressed)); proof.release = Some(Release::Key(code));
        }
        Some(PublicInput::Click(point)) => {
            window.set_physical_cursor_position(Some(point.as_dvec2()));
            mouse.write(MouseButtonInput { button: MouseButton::Left,
                state: ButtonState::Pressed, window: *entity }); proof.release = Some(Release::Mouse);
        }
        Some(PublicInput::Wheel(point, y)) => {
            window.set_physical_cursor_position(Some(point.as_dvec2()));
            wheel.write(MouseWheel { unit: MouseScrollUnit::Line, x: 0., y, window: *entity,
                phase: bevy::input::touch::TouchPhase::Moved });
        }
        Some(PublicInput::Resize(w, h)) => window.resolution.set_physical_resolution(w, h),
        None => (),
    }
}

fn property<'a>(sample: &'a [String], name: &str, kind: &str) -> Option<&'a str> {
    let prefix = format!("{name}:{kind}=");
    sample.iter().find_map(|entry| entry.strip_prefix(&prefix))
}
fn visible(sample: &[String], index: usize) -> bool {
    property(sample, &format!("visible{index}"), "boolean") == Some("true")
}
fn number(sample: &[String], name: &str) -> Option<f32> {
    property(sample, name, "number")?.parse::<f32>().ok().filter(|n| n.is_finite())
}
fn movie_point(sample: &[String], window: &Window) -> Option<Vec2> {
    if property(sample, "complete", "boolean") != Some("true") || number(sample, "matches") != Some(1.) { return None; }
    let point = Vec2::new(number(sample, "x")?, number(sample, "y")?);
    let size = Vec2::new(number(sample, "stageWidth")?, number(sample, "stageHeight")?);
    if size != Vec2::new(window.physical_width() as f32, window.physical_height() as f32)
        || point.min_element() < 0. || point.x >= size.x || point.y >= size.y { return None; }
    Some(point)
}

// Observe once per fresh raster, bounded to 32 samples per phase and throttled.
// Calls invoke only our read-only diagnostic; navigation uses pointer edges.
fn sample_movie(proof: &mut Proof, front: &Frontend, search: &str, label: &str) {
    if let Some((_, sample)) = front.flow_observation().filter(|(id, _)| *id == proof.probe_id) {
        proof.sample = sample.clone(); proof.awaiting_probe = false;
    }
    if proof.awaiting_probe || proof.sample_count >= 32 || Instant::now() < proof.next_probe { return; }
    let serial = front.presented_serial();
    if serial == proof.probe_serial { return; }
    proof.probe_serial = serial; proof.probe_id += 1; proof.sample_count += 1;
    let roots = front.flow_roots();
    let root = |name: &str| roots.get(name).cloned().unwrap_or_default();
    front.send(scaleform::Input::Call { path: "_global.csrDiagnostics".into(), function: "flowProbe".into(),
        args: vec![proof.probe_id.to_string().into(), scaleform::Value::List(
            ["MainMenu", "OptionsMenu", "StartSinglePlayer", "PauseMenu"]
                .map(|name| scaleform::Value::String(root(name))).to_vec()),
            root(search).into(), label.into()] });
    front.send(scaleform::Input::Inspect(format!("_global.csrFlowProbe.{}", proof.probe_id)));
    proof.awaiting_probe = true; proof.next_probe = Instant::now() + Duration::from_millis(200);
}

// Explicit expected controls projection: equality checks every unrelated field,
// including all video/audio preferences, after the real Restore + Apply path.
fn restored(baseline: &Settings) -> Settings {
    let mut expected = baseline.clone(); let d = Settings::default();
    expected.binds = d.binds; expected.console_key = d.console_key; expected.mouse_binds = d.mouse_binds;
    expected.wheel_up = d.wheel_up; expected.wheel_down = d.wheel_down;
    expected.sensitivity = d.sensitivity; expected.invert_y = d.invert_y; expected.controls_source = d.controls_source;
    expected
}

type Layout = (Entity, &'static ComputedNode, &'static UiGlobalTransform, Option<&'static Action>,
    Option<&'static ScrollPosition>, Has<ControlsScroll>);
fn rect(node: &ComputedNode, transform: &UiGlobalTransform) -> Rect {
    Rect::from_center_size(transform.translation, node.size())
}
fn control_point(nodes: &Query<Layout>, wanted: impl Fn(&Action) -> bool, window: &Window) -> Option<Vec2> {
    let viewport = Rect::from_corners(Vec2::ZERO, Vec2::new(window.physical_width() as f32, window.physical_height() as f32));
    let body = nodes.iter().find(|(_, _, _, _, _, body)| *body).map(|(_, n, t, _, _, _)| rect(n, t))?;
    nodes.iter().find_map(|(_, node, transform, action, _, _)| {
        let action = action?; if !wanted(action) || node.size().min_element() <= 0. { return None; }
        let bounds = rect(node, transform);
        let keyboard_or_adjust = matches!(action, Action::Bind(_) | Action::ClearBind(_) | Action::Adjust(_, _));
        (viewport.contains(bounds.min) && viewport.contains(bounds.max)
            && (!keyboard_or_adjust || (body.contains(bounds.min) && body.contains(bounds.max))))
            .then_some(bounds.center())
    })
}

#[allow(clippy::too_many_arguments)]
fn update(mut proof: ResMut<Proof>, front: Res<Frontend>, options: Res<crate::Options>, settings: Res<Settings>,
    menu: Res<Menu>, state: Res<State<AppState>>, clock: Res<Time<Virtual>>, frames: Res<bevy::diagnostic::FrameCount>,
    window: Single<&Window, With<PrimaryWindow>>, nodes: Query<Layout>, world: Option<Res<SimWorld>>,
    hud: Query<(&Visibility, &InheritedVisibility, &Node, &ComputedNode), With<crate::combat_hud::Surface>>, mut commands: Commands, mut exit: MessageWriter<AppExit>) {
    if proof.finished { return; }
    if let Some(error) = proof.capture_error.take() { proof.fail(&mut exit, error); return; }
    if proof.start.elapsed() > Duration::from_secs(240) || proof.entered.elapsed() > Duration::from_secs(40) {
        let reason = format!("bounded deadline; menu={:?} options={:?} raster={} sample={:?}",
            menu.page, front.options_receiver(), front.presented_serial(), proof.sample);
        proof.fail(&mut exit, reason); return;
    }
    if let Some(error) = &front.failed { proof.fail(&mut exit, error); return; }
    if !proof.validated {
        let validate = || -> Result<(), String> {
            if !matches!(proof.case.as_str(), "home-options" | "match-options" | "controls") { return Err("unknown CSR_TEST_FLOW_CASE".into()); }
            private_data(&proof.output)?;
            if options.0.screenshot.is_some() || options.0.direct || settings.fullscreen {
                return Err("normal windowed menu startup required, no --direct/--screenshot".into());
            }
            if match_case() && options.0.bots == 0 { return Err("existing authored difficulty-3 launch requires --bots 3 (zero is an invalid harness fixture)".into()); }
            if window.physical_width() != 1920 || window.physical_height() != 1080 {
                return Err("start at --size 1920 1080".into());
            }
            if !Path::new("config/user.cfg").is_file() || Settings::load() != *settings {
                return Err("root must seed private config/user.cfg before launch".into());
            }
            Ok(())
        };
        if let Err(error) = validate() { proof.fail(&mut exit, error); return; }
        proof.baseline = Some(settings.clone()); proof.validated = true;
        info!(case = proof.case, "rendered acceptance: public synthetic input; no native/physical latency or fidelity claim");
    }
    if proof.pending_capture || proof.pending.is_some() || proof.release.is_some() { return; }
    if proof.case == "match-options" && *state.get() != AppState::Match {
        // The old flow owns only the first authored match launch. Its own
        // bounded deadline remains active; acceptance phase 0 starts on Match.
        proof.entered = Instant::now(); return;
    }
    if proof.case == "controls" && ((2..=16).contains(&proof.phase) || proof.phase == 20) {
        controls(&mut proof, &settings, &menu, &nodes, &window, frames.0, &mut commands, &mut exit); return;
    }
    let phase = proof.phase;
    let (search, label) = match (proof.case.as_str(), phase) {
        ("home-options", 0 | 4) | ("controls", 0 | 18) => ("MainMenu", "OPTIONS"),
        ("home-options", 1 | 5) => ("MainMenu", "AUDIO SETTINGS"),
        ("home-options", 7) => ("MainMenu", "OFFLINE WITH BOTS"),
        ("home-options", 8) => ("StartSinglePlayer", "BACK"),
        ("controls", 1 | 19) => ("MainMenu", "KEYBOARD / MOUSE"),
        ("match-options", 1 | 8 | 13) => ("PauseMenu", "OPTIONS"),
        ("match-options", 2 | 9 | 14) => ("PauseMenu", "AUDIO SETTINGS"),
        ("match-options", 15) => ("OptionsMenu", "BACK"),
        _ => ("MainMenu", ""),
    };
    sample_movie(&mut proof, &front, search, label);
    let home = *state.get() == AppState::Menu && front.home_presented() && visible(&proof.sample, 0)
        && !visible(&proof.sample, 1) && !visible(&proof.sample, 2) && front.options_receiver().is_none();
    let audio = front.movie_ready("OptionsMenu") && front.options_receiver().is_some() && visible(&proof.sample, 1)
        && !visible(&proof.sample, 2) && front.flow_presented();
    let pause = menu.page == Some(Page::Pause) && clock.is_paused() && front.pause_presented()
        && front.options_receiver().is_none() && visible(&proof.sample, 3) && !visible(&proof.sample, 1);
    let tick = world.as_ref().map_or(0, |world| world.0.tick);
    let game = !menu.open() && !clock.is_paused() && tick > proof.tick && hud.iter().count() == 2
        && hud.iter().all(|(visibility, inherited, node, computed)| *visibility != Visibility::Hidden
            && inherited.get() && node.display != Display::None && computed.size().min_element() > 0.);
    let point = movie_point(&proof.sample, &window);
    let (valid, action) = match (proof.case.as_str(), phase) {
        ("home-options", 0 | 1 | 4 | 5 | 7 | 8) => {
            let valid = if phase == 8 { visible(&proof.sample, 2) && !visible(&proof.sample, 1) && front.options_receiver().is_none() }
                else if phase == 7 { front.movie_ready("MainMenu") } else { home || (phase == 1 || phase == 5) && visible(&proof.sample, 0) };
            (valid && point.is_some(), point)
        }
        ("home-options", 2 | 6) => (audio, Some(reference_point(&window, if phase == 2 { 333. } else { 486. }, 26.))),
        ("home-options", 3 | 9) => (home, None),
        ("controls", 0 | 1 | 18 | 19) => (home && point.is_some(), point),
        ("controls", 17) => (home, None),
        ("match-options", 0 | 5 | 7 | 12 | 17) => (game, None),
        ("match-options", 1 | 2 | 8 | 9 | 13 | 14) => (pause && point.is_some(), point),
        ("match-options", 3 | 10 | 15) => (audio && (phase != 15 || point.is_some()), point),
        ("match-options", 4 | 6 | 11 | 16) => (pause, None),
        _ => { proof.fail(&mut exit, "unexpected phase"); return; }
    };
    // Close/reopen pressure: resume and re-open on consecutive delivered key
    // edges after the owned cancel ack, without waiting for a Pause raster.
    if proof.case == "match-options" && matches!(phase, 11 | 12) {
        let edge_ready = if phase == 11 { menu.page == Some(Page::Pause) && front.options_receiver().is_none() }
            else { !menu.open() && !clock.is_paused() };
        if edge_ready { proof.issue(PublicInput::Key(KeyCode::Escape)); }
        return;
    }
    let valid = valid && !proof.sample.is_empty();
    let serial = if proof.case == "match-options" && matches!(phase, 0 | 5 | 7 | 17) {
        frames.0 as u64
    } else { front.presented_serial() };
    if !proof.admitted(valid, serial, &mut commands, &window) { return; }
    info!(case = proof.case, phase, cycle = proof.cycle, tick, page = ?menu.page,
        raster = front.presented_serial(), sample = ?proof.sample, "rendered acceptance: settled");
    if let Some(point) = action { movie_click(&front, point); proof.next(); return; }
    if proof.case == "home-options" {
        if phase == 9 {
            if proof.cycle == 1 { finish(&mut proof, &mut exit); }
            else { proof.cycle += 1; proof.enter(0); }
        } else { proof.next(); }
    } else if proof.case == "controls" {
        proof.next();
    } else if phase == 17 { finish(&mut proof, &mut exit); }
    else { proof.tick = tick; proof.issue(PublicInput::Key(KeyCode::Escape)); }
}

fn reference_point(window: &Window, x: f32, y: f32) -> Vec2 {
    Vec2::new(x / 1920. * window.physical_width() as f32, y / 1080. * window.physical_height() as f32)
}
fn movie_click(front: &Frontend, point: Vec2) {
    let (x, y) = (point.x as f64, point.y as f64);
    front.send(scaleform::Input::MouseMove { x, y });
    front.send(scaleform::Input::MouseDown { x, y, button: scaleform::MouseButton::Left });
    front.send(scaleform::Input::MouseUp { x, y, button: scaleform::MouseButton::Left });
}
fn finish(proof: &mut Proof, exit: &mut MessageWriter<AppExit>) {
    info!(case = proof.case, "rendered acceptance: synthetic functional assertions complete; failures: 0; screenshots require visual review");
    proof.finished = true; exit.write(AppExit::Success);
}

fn controls(proof: &mut Proof, settings: &Settings, menu: &Menu, nodes: &Query<Layout>, window: &Window,
    frame: u32, commands: &mut Commands, exit: &mut MessageWriter<AppExit>) {
    let phase = proof.phase;
    let baseline = proof.baseline.as_ref().unwrap();
    let expected = restored(baseline);
    let find = |action: fn(&Action) -> bool| control_point(nodes, action, window);
    let bind = find(|a| matches!(a, Action::Bind(0)));
    let clear = find(|a| matches!(a, Action::ClearBind(0)));
    let save = find(|a| matches!(a, Action::Save));
    let restore = find(|a| matches!(a, Action::ControlsDefaults));
    let back = find(|a| matches!(a, Action::ControlsBack));
    let mouse = find(|a| matches!(a, Action::Adjust(25, delta) if *delta > 0.));
    let wheel = find(|a| matches!(a, Action::Adjust(30, delta) if *delta > 0.));
    let body = nodes.iter().find(|(_, _, _, _, _, body)| *body);
    let scroll = body.and_then(|(_, _, _, _, scroll, _)| scroll).map_or(0., |s| s.y);
    let max = body.map_or(0., |(_, node, _, _, _, _)| ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.));
    let body_center = body.map(|(_, n, t, _, _, _)| rect(n, t).center());
    let bottom = find(|a| matches!(a, Action::Bind(i) if *i == ACTIONS.len()));
    let footer = save.is_some() && restore.is_some() && back.is_some();
    let (valid, input) = match phase {
        2 => (bind.is_some(), bind.map(PublicInput::Click)),
        3 => (menu.capture == Some(0), Some(PublicInput::Key(KeyCode::F11))),
        4 => (menu.capture.is_none() && settings.binds[0] == Some(KeyCode::F11), clear.map(PublicInput::Click)),
        5 => (settings.binds[0].is_none(), bind.map(PublicInput::Click)),
        6 => (menu.capture == Some(0), Some(PublicInput::Key(KeyCode::Escape))),
        7 => (menu.capture.is_none(), mouse.map(PublicInput::Click)),
        8 => (settings.mouse_binds[0] != baseline.mouse_binds[0], wheel.map(PublicInput::Click)),
        9 => (settings.wheel_up != baseline.wheel_up, body_center.map(|p| PublicInput::Wheel(p, -4.))),
        10 => (scroll > 0., Some(PublicInput::Key(KeyCode::End))),
        11 => (max > 0. && (scroll - max).abs() < 1. && bottom.is_some(), Some(PublicInput::Resize(1280, 720))),
        12 => (window.physical_width() == 1280 && window.physical_height() == 720, Some(PublicInput::Key(KeyCode::End))),
        13 => (max > 0. && (scroll - max).abs() < 1. && bottom.is_some(), Some(PublicInput::Key(KeyCode::Home))),
        14 => (scroll == 0. && bind.is_some(), restore.map(PublicInput::Click)),
        15 => (*settings == expected && Settings::load() == *baseline, save.map(PublicInput::Click)),
        16 => (*settings == expected && menu.status == "Settings saved." && Settings::load() == expected, back.map(PublicInput::Click)),
        20 => {
            if proof.admitted(menu.classic_controls && footer && *settings == expected && Settings::load() == expected,
                frame as u64, commands, window) { finish(proof, exit); }
            return;
        }
        _ => { proof.fail(exit, "unexpected controls phase"); return; }
    };
    if proof.admitted(menu.classic_controls && footer && valid && input.is_some(), frame as u64, commands, window) {
        info!(phase, scroll, max, dimensions = ?(window.physical_width(), window.physical_height()),
            capture = ?menu.capture, "rendered acceptance: controls measured layout and controller result");
        proof.issue(input.unwrap());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settlement_requires_distinct_rasters_and_restarts_after_a_gap() {
        let mut gate = Stable::default();
        let t = Duration::from_millis;
        assert!(!gate.observe(false, 20, t(0)));
        assert!(!gate.observe(true, 21, t(100)));
        assert!(!gate.observe(true, 21, t(900)), "elapsed time alone is insufficient");
        assert!(!gate.observe(true, 22, t(950)));
        assert!(gate.observe(true, 23, t(1000)));
        assert!(!gate.observe(false, 24, t(1100)));
        assert!(!gate.observe(true, 25, t(1200)));
        assert!(!gate.observe(true, 26, t(1300)));
        assert!(!gate.observe(true, 27, t(1400)), "fresh rasters alone are insufficient");
        assert!(gate.observe(true, 28, t(1600)));
    }

    #[test]
    fn pointer_observations_reject_ambiguity_invalid_coordinates_and_stale_resize() {
        let mut sample = vec!["complete:boolean=true".into(), "matches:number=1".into(),
            "x:number=350".into(), "y:number=26".into(), "stageWidth:number=1920".into(),
            "stageHeight:number=1080".into()];
        let mut window = Window { resolution: bevy::window::WindowResolution::new(1920, 1080), ..default() };
        assert_eq!(movie_point(&sample, &window), Some(Vec2::new(350., 26.)));
        sample[1] = "matches:number=2".into();
        assert!(movie_point(&sample, &window).is_none());
        sample[1] = "matches:number=1".into();
        sample[2] = "x:number=NaN".into();
        assert!(movie_point(&sample, &window).is_none());
        sample[2] = "x:number=350".into();
        window.resolution.set_physical_resolution(1280, 720);
        assert!(movie_point(&sample, &window).is_none());
    }
}

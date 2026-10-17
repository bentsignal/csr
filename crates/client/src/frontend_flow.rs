//! Opt-in, single-process rendered lifecycle proof. Opens pause through the
//! engine state; Resume, Exit and both match launches use authored pointer controls.
use crate::{
    LoadedLevel, Options,
    frontend::Frontend,
    menu::{Menu, Page},
    player::{InputEnabled, SimWorld},
    session::{AppState, MatchEntity},
};
use bevy::prelude::*;
use std::time::Instant;

pub(crate) fn enabled() -> bool {
    std::env::var("CSR_TEST_FLOW").as_deref() == Ok("1")
}

pub(crate) struct Flow {
    start: Instant,
    entered: Instant,
    phase: u8,
    step: usize,
    tick: u64,
    options_roundtrip: bool,
    video_options: bool,
    video_edit: bool,
    home_ready_ms: Option<f64>,
    pause_hover: bool,
    pause_hover_started: Option<Instant>,
    pause_ready_serial: Option<u64>,
    archive_loading: bool,
    installed_loading: bool,
    loading_ready_serial: Option<(crate::session::LoadingMap, u64)>,
    loading_captures: [bool; 2],
}
impl Default for Flow {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            entered: Instant::now(),
            phase: 0,
            step: 0,
            tick: 0,
            options_roundtrip: matches!(std::env::var("CSR_TEST_FLOW_OPTIONS").as_deref(), Ok("1" | "video")),
            video_options: std::env::var("CSR_TEST_FLOW_OPTIONS").as_deref() == Ok("video"),
            video_edit: std::env::var("CSR_TEST_FLOW_VIDEO_EDIT").as_deref() == Ok("1"),
            home_ready_ms: None,
            pause_hover: std::env::var("CSR_TEST_PAUSE_HOVER").as_deref() == Ok("1"),
            pause_hover_started: None,
            pause_ready_serial: None,
            archive_loading: std::env::var("CSR_TEST_FLOW_ARCHIVE_LOADING").as_deref() == Ok("1"),
            installed_loading: std::env::var("CSR_TEST_FLOW_INSTALLED_LOADING").as_deref() == Ok("1"),
            loading_ready_serial: None,
            loading_captures: [false; 2],
        }
    }
}
impl Flow {
    fn next(&mut self) {
        self.phase += 1;
        info!("frontend flow: entered phase {}", self.phase);
        self.step = 0;
        self.entered = Instant::now();
    }
    fn seconds(&self) -> f64 {
        self.entered.elapsed().as_secs_f64()
    }
}
fn click(front: &Frontend, window: &Window, x: f64, y: f64) {
    let x = x * window.physical_width() as f64 / 1920.;
    let y = y * window.physical_height() as f64 / 1080.;
    front.send(scaleform::Input::MouseMove { x, y });
    front.send(scaleform::Input::MouseDown {
        x,
        y,
        button: scaleform::MouseButton::Left,
    });
    front.send(scaleform::Input::MouseUp {
        x,
        y,
        button: scaleform::MouseButton::Left,
    });
}
fn capture(commands: &mut Commands, options: &Options, name: &str) {
    let path = std::env::var_os("CSR_TEST_FLOW_OUTPUT").map(std::path::PathBuf::from)
        .map(|folder| folder.join(name))
        .unwrap_or_else(|| options.0.screenshot.as_ref().unwrap().with_file_name(name));
    commands
        .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
        .observe(bevy::render::view::screenshot::save_to_disk(path));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    mut flow: Local<Flow>,
    front: Res<Frontend>,
    options: Res<Options>,
    settings: Res<crate::settings::Settings>,
    state: Res<State<AppState>>,
    app_frame: Res<bevy::diagnostic::FrameCount>,
    level: Option<Res<LoadedLevel>>,
    world: Option<Res<SimWorld>>,
    mut menu: ResMut<Menu>,
    mut clock: ResMut<Time<Virtual>>,
    mut input: ResMut<InputEnabled>,
    window: Single<&Window>,
    entities: Query<(), With<MatchEntity>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if (options.0.screenshot.is_none() && std::env::var_os("CSR_TEST_FLOW_OUTPUT").is_none())
        || front.failed.is_some()
        || (flow.archive_loading && flow.installed_loading)
        || flow.start.elapsed().as_secs() > 180
    {
        error!(
            "frontend flow failed at phase {}: missing capture, movie failure or timeout",
            flow.phase
        );
        exit.write(AppExit::error());
        return;
    }
    // Capture natural Loading only after an actual owned loader Init and two
    // subsequent raster assignments. Never hold the job or fake completion.
    if (flow.archive_loading || flow.installed_loading) && *state.get() == AppState::Loading {
        let index = usize::from(flow.phase == 7);
        let expected = if flow.installed_loading { ["de_nuke", "de_mirage"] }
            else { ["de_dust2_2017", "de_inferno_2016"] }[index];
        if matches!(flow.phase, 1 | 7) && !flow.loading_captures[index] {
            let observed = if flow.installed_loading { front.observed_installed_loading_image() }
                else { front.observed_loading_image() };
            if let Some((selected, url)) = observed.filter(|(map, _)| map.name == expected) {
                if flow.loading_ready_serial.as_ref().is_none_or(|(map, _)| *map != selected) {
                    flow.loading_ready_serial = Some((selected.clone(), front.presented_serial()));
                }
                let serial = flow.loading_ready_serial.as_ref().unwrap().1;
                let painted = if flow.installed_loading { front.radar_loading_painted(Some(selected.serial), app_frame.0, false) }
                    else { front.archive_loading_painted(Some(selected.serial), app_frame.0) };
                if front.presented_serial() >= serial.saturating_add(2) && painted {
                    let file = if flow.installed_loading { ["flow-loading-nuke.png", "flow-loading-mirage.png"] }
                        else { ["flow-loading-classic-dust2.png", "flow-loading-classic-inferno.png"] }[index];
                    capture(&mut commands, &options, file);
                    info!("frontend flow: captured actual {} Loading map={} generation={} url={} raster={}", if flow.installed_loading { "installed" } else { "archive" }, selected.name, selected.serial, url, front.presented_serial());
                    flow.loading_captures[index] = true;
                }
            }
        }
    }
    match flow.phase {
        0 | 6 => {
            if *state.get() != AppState::Menu || !front.ready {
                return;
            }
            let second = flow.phase == 6;
            let classic_inferno = flow.archive_loading || std::env::var("CSR_TEST_FLOW_CLASSIC_INFERNO").as_deref() == Ok("1");
            let points = [
                (486., 26.),
                (503., 264.),
                (1450., 206.),
                // The archived Inferno card occupies column two. Dust II
                // therefore moved to column two on the second row too.
                (if flow.installed_loading { if second { 1168. } else { 1345. } }
                    else if !second && flow.archive_loading { 1170. } else if second && !classic_inferno { 810. } else { 990. },
                    if second || flow.installed_loading { 500. } else { 680. }),
                (1450., 1017.),
                (1197., 580.),
                (1163., 754.),
            ];
            if flow.step < points.len() && flow.seconds() >= 5. + flow.step as f64 * 3. {
                if flow.step == 3 {
                    capture(&mut commands, &options, if second {
                        "flow-second-map-chooser.png"
                    } else {
                        "flow-first-map-chooser.png"
                    });
                }
                let (x, y) = points[flow.step];
                click(&front, &window, x, y);
                flow.step += 1;
            }
            if flow.step == points.len() {
                flow.next();
            }
        }
        1 | 7 => {
            if *state.get() != AppState::Match {
                return;
            }
            let expected = if flow.installed_loading {
                if flow.phase == 1 { "de_nuke" } else { "de_mirage" }
            } else if flow.phase == 1 {
                if flow.archive_loading { "de_dust2_2017" } else { "de_dust2" }
            } else {
                if flow.archive_loading || std::env::var("CSR_TEST_FLOW_CLASSIC_INFERNO").as_deref() == Ok("1") {
                    "de_inferno_2016"
                } else {
                    "de_inferno"
                }
            };
            if level.as_ref().is_none_or(|level| level.0.name != expected) {
                error!("frontend flow loaded an unexpected map");
                exit.write(AppExit::error());
                return;
            }
            if (flow.archive_loading || flow.installed_loading) && !flow.loading_captures[usize::from(flow.phase == 7)] {
                error!("frontend flow: natural Loading completed without an admitted current radar raster: {expected}");
                exit.write(AppExit::error());
                return;
            }
            if flow.step == 0 {
                flow.step = 1;
                flow.entered = Instant::now();
                return;
            }
            if flow.seconds() < 3. || world.as_ref().is_none_or(|world| world.0.tick == 0) {
                return;
            }
            if flow.step == 1 {
                capture(
                    &mut commands,
                    &options,
                    if flow.installed_loading {
                        if flow.phase == 1 { "flow-nuke.png" } else { "flow-mirage.png" }
                    } else if flow.phase == 1 {
                        "flow-dust2.png"
                    } else {
                        "flow-inferno.png"
                    },
                );
                info!(
                    "frontend flow: matched {expected}, tick {}",
                    world.as_ref().unwrap().0.tick
                );
                flow.step = 2;
                flow.entered = Instant::now();
                return;
            }
            if flow.seconds() < 3. {
                return;
            }
            if flow.phase == 1 {
                menu.page = Some(Page::Pause);
                clock.pause();
                input.0 = true;
            }
            flow.next();
        }
        2 => {
            if !menu.open() || flow.seconds() < 3. {
                return;
            }
            // A cold match worker can still be booting after the old fixed
            // delay. Do not turn a click on the world into a claimed UI test.
            if flow.step == 0 {
                if !front.pause_presented() { return; }
                let serial = *flow.pause_ready_serial.get_or_insert(front.presented_serial());
                // The image acknowledgment precedes the next root visibility
                // update. Require a subsequent raster before capturing/clicking.
                if front.presented_serial() <= serial { return; }
                if flow.pause_hover {
                    let started = *flow.pause_hover_started.get_or_insert_with(|| {
                        capture(&mut commands, &options, "flow-pause.png");
                        info!("frontend flow: normal-start Pause hover begin");
                        Instant::now()
                    });
                    let seconds = started.elapsed().as_secs_f64();
                    if seconds < 5. {
                        // Same menu pointer positions/cadence as the bounded
                        // Pause pressure case; never moves the system cursor.
                        front.send(scaleform::Input::MouseMove {
                            x: window.physical_width() as f64 * 0.32,
                            y: window.physical_height() as f64 * (0.48 + 0.19 * (seconds * 12.).sin().abs()),
                        });
                        return;
                    }
                    info!("frontend flow: normal-start Pause hover end");
                }
            }
            if flow.options_roundtrip {
                match flow.step {
                    0 => {
                        if !flow.pause_hover { capture(&mut commands, &options, "flow-pause.png"); }
                        click(&front, &window, 650., 665.);
                    }
                    1 => {
                        capture(&mut commands, &options, "flow-pause-options-list.png");
                        click(&front, &window, 650., if flow.video_options { 780. } else { 817. });
                    }
                    2 => {
                        if front.options_receiver().is_none() || !front.movie_ready("OptionsMenu") { return; }
                        capture(&mut commands, &options, if flow.video_options { "flow-pause-video.png" } else { "flow-pause-audio.png" });
                        if flow.video_options && flow.video_edit {
                            click(&front, &window, 1430., 230.);
                        } else {
                            click(&front, &window, 380., 970.);
                        }
                    }
                    3 if flow.video_options && flow.video_edit => {
                        capture(&mut commands, &options, "flow-video-vsync-open.png");
                        // Original popup row observed at 1920x1080. Exercise its
                        // authored pointer control rather than assume key focus.
                        click(&front, &window, 1250., 302.);
                    }
                    4 if flow.video_options && flow.video_edit => {
                        if !settings.vsync || window.present_mode != bevy::window::PresentMode::AutoVsync { return; }
                        info!("frontend flow: authored Video choice applied saved vsync=true and actual AutoVsync window policy");
                        capture(&mut commands, &options, "flow-video-vsync-enabled.png");
                        click(&front, &window, 1430., 230.);
                    }
                    5 if flow.video_options && flow.video_edit => {
                        click(&front, &window, 1250., 270.);
                    }
                    6 if flow.video_options && flow.video_edit => {
                        if settings.vsync || window.present_mode != bevy::window::PresentMode::AutoNoVsync { return; }
                        info!("frontend flow: authored Video choice restored saved vsync=false and actual AutoNoVsync window policy");
                        capture(&mut commands, &options, "flow-video-vsync-disabled.png");
                        click(&front, &window, 380., 970.);
                    }
                    _ => {
                        if front.options_receiver().is_some() || !front.pause_surface_visible() { return; }
                        info!("frontend flow: native {} Back restored Pause", if flow.video_options { "Video" } else { "Audio" });
                        capture(&mut commands, &options, "flow-pause-options-returned.png");
                        flow.tick = world.as_ref().unwrap().0.tick;
                        click(&front, &window, 650., 530.);
                        flow.next();
                        return;
                    }
                }
                flow.step += 1;
                flow.entered = Instant::now();
            } else {
                if !flow.pause_hover { capture(&mut commands, &options, "flow-pause.png"); }
                flow.tick = world.as_ref().unwrap().0.tick;
                click(&front, &window, 650., 530.);
                flow.next();
            }
        }
        3 => {
            if menu.open() || clock.is_paused() || world.as_ref().unwrap().0.tick <= flow.tick {
                return;
            }
            input.0 = false;
            if flow.step == 0 {
                info!("frontend flow: native Resume restored the simulation clock");
                capture(&mut commands, &options, "flow-resumed.png");
                flow.step = 1;
                flow.entered = Instant::now();
                return;
            }
            if flow.seconds() < 3. {
                return;
            }
            menu.page = Some(Page::Pause);
            clock.pause();
            input.0 = true;
            flow.next();
        }
        4 => {
            if flow.seconds() < 5. {
                return;
            }
            if flow.step == 0 {
                capture(&mut commands, &options, "flow-pause-exit.png");
                flow.step = 1;
                flow.entered = Instant::now();
                return;
            }
            click(&front, &window, 680., 710.);
            flow.next();
        }
        5 => {
            if *state.get() != AppState::Menu { return; }
            if front.home_presented() && flow.home_ready_ms.is_none() {
                let elapsed_ms = flow.seconds() * 1000.;
                flow.home_ready_ms = Some(elapsed_ms);
                info!(elapsed_ms, "frontend flow: native Exit reached fresh Ready Home");
            }
            if flow.seconds() < 3. { return; }
            if !front.home_presented() {
                if flow.step == 0 {
                    // Preserve the actual slow-return state rather than
                    // treating a fixed-delay capture as restored Home.
                    capture(&mut commands, &options, "flow-home-pending.png");
                    info!("frontend flow: Home still pending after {:.3}s", flow.seconds());
                    flow.step = 1;
                }
                return;
            }
            info!(elapsed_ms = flow.home_ready_ms.unwrap_or_default(), "frontend flow: capture restored Home");
            if !entities.is_empty() || world.is_some() || level.is_some() {
                error!("frontend flow retained match entities/resources after Exit");
                exit.write(AppExit::error());
                return;
            }
            input.0 = false;
            capture(&mut commands, &options, "flow-returned-home.png");
            info!("frontend flow: native Exit removed the match and retained the menu");
            flow.next();
        }
        8 => {
            if flow.seconds() < 3. {
                return;
            }
            if flow.installed_loading {
                info!("frontend flow: Nuke -> Pause -> Resume -> Menu -> Mirage; failures: 0");
                info!("frontend flow: both installed Loading rasters admitted; failures: 0");
            } else {
                info!("frontend flow: Dust II -> Pause -> Resume -> Menu -> Inferno; failures: 0");
            }
            if flow.archive_loading {
                info!("frontend flow: both original archive Loading rasters admitted; failures: 0");
            }
            if std::env::var("CSR_TEST_FLOW_CLASSIC_INFERNO").as_deref() == Ok("1") {
                info!("frontend flow: archived Inferno (2016) selected through authored map card");
            }
            exit.write(AppExit::Success);
            flow.next();
        }
        _ => {}
    }
}

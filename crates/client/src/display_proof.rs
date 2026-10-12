//! Opt-in normal-startup display proof. No synthetic focus or input events.
use crate::{frontend::Frontend, settings::Settings};
use bevy::{
    ecs::system::NonSendMarker,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{Monitor, OnMonitor, PrimaryWindow, RawHandleWrapper, WindowMode},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(crate) fn enabled() -> bool {
    std::env::var_os("CSR_TEST_DISPLAY_PROOF").is_some()
}
const NAMES: [&str; 6] = [
    "windowed-1600",
    "windowed-1920",
    "windowed-2560",
    "fullscreen",
    "fullscreen-volume",
    "returned-windowed-2560",
];

#[derive(Default)]
struct Settled {
    serial: u64,
    fresh: u8,
    since: Option<Duration>,
}
impl Settled {
    fn observe(&mut self, valid: bool, serial: u64, now: Duration) -> bool {
        if !valid {
            self.serial = serial;
            self.fresh = 0;
            self.since = None;
            return false;
        }
        self.since.get_or_insert(now);
        if serial != self.serial {
            self.serial = serial;
            self.fresh = self.fresh.saturating_add(1);
        }
        self.fresh >= 2 && now.saturating_sub(self.since.unwrap()) >= Duration::from_secs(1)
    }
}

#[derive(Resource)]
pub(crate) struct Proof {
    start: Instant,
    phase: usize,
    requested: bool,
    validated: bool,
    output: PathBuf,
    gate: Settled,
    capture_size: Option<(u32, u32)>,
    captured: Option<(u32, u32)>,
    fullscreen_size: Option<(u32, u32)>,
    finished: bool,
}
impl Default for Proof {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            phase: 0,
            requested: false,
            validated: false,
            output: std::env::var_os("CSR_TEST_DISPLAY_PROOF")
                .map(PathBuf::from)
                .unwrap_or_default(),
            gate: Default::default(),
            capture_size: None,
            captured: None,
            fullscreen_size: None,
            finished: false,
        }
    }
}
impl Proof {
    fn fail(&mut self, exit: &mut MessageWriter<AppExit>, error: impl std::fmt::Display) {
        error!("display proof: phase{} {error}; failures:1", self.phase);
        self.finished = true;
        exit.write(AppExit::error());
    }
}

fn owned_data_folder(output: &std::path::Path) -> Result<(), String> {
    if !output.is_absolute() {
        return Err("output must be an absolute private fixture directory".into());
    }
    let expected = std::fs::canonicalize(output.join("userdata"))
        .map_err(|e| format!("isolated userdata missing: {e}"))?;
    let actual = std::env::current_dir()
        .and_then(std::fs::canonicalize)
        .map_err(|e| e.to_string())?;
    let declared = std::env::var_os("CSR_DATA_DIR")
        .map(PathBuf::from)
        .ok_or("CSR_DATA_DIR must explicitly select the fixture userdata")?;
    let declared = std::fs::canonicalize(declared).map_err(|e| e.to_string())?;
    if actual != expected || declared != expected {
        return Err("player data is not the output directory's isolated userdata".into());
    }
    Ok(())
}

pub(crate) fn request(
    mut proof: ResMut<Proof>,
    mut settings: ResMut<Settings>,
    options: Res<crate::Options>,
    front: Option<Res<Frontend>>,
    mut exit: MessageWriter<AppExit>,
) {
    if proof.finished || proof.requested {
        return;
    }
    if !proof.validated {
        if options.0.screenshot.is_some() || options.0.direct {
            proof.fail(
                &mut exit,
                "normal menu startup required; no direct/screenshot/remote mode",
            );
            return;
        }
        if let Err(error) = owned_data_folder(&proof.output) {
            proof.fail(&mut exit, error);
            return;
        }
        proof.validated = true;
    }
    let phase = proof.phase;
    settings.fullscreen = matches!(phase, 3 | 4);
    settings.resolution = match phase {
        0 => 1,
        1 => 2,
        _ => 3,
    };
    if phase == 4 {
        settings.volume = 0.76;
    }
    proof.gate = Settled {
        serial: front.as_ref().map_or(0, |f| f.presented_serial()),
        ..Default::default()
    };
    proof.requested = true;
    info!(
        "display proof: requested {} through production Settings; volume{}",
        NAMES[phase], settings.volume
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn measure(
    mut proof: ResMut<Proof>,
    front: Res<Frontend>,
    images: Res<Assets<Image>>,
    window: Single<(&Window, Option<&RawHandleWrapper>, Option<&OnMonitor>), With<PrimaryWindow>>,
    monitors: Query<&Monitor>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    _ui_thread: NonSendMarker,
) {
    if proof.finished {
        return;
    }
    if proof.start.elapsed() > Duration::from_secs(120) {
        proof.fail(&mut exit, "deadline exceeded");
        return;
    }
    if !proof.requested || !front.movie_ready("MainMenu") {
        return;
    }
    if let Some(error) = &front.failed {
        proof.fail(&mut exit, error);
        return;
    }
    let (window, owner, on_monitor) = window.into_inner();
    let monitor_size = on_monitor
        .and_then(|m| monitors.get(m.0).ok())
        .map(|m| (m.physical_width, m.physical_height));
    let native = match owner
        .ok_or_else(|| "owned window handle unavailable".to_string())
        .and_then(native_dimensions)
    {
        Ok(size) => size,
        Err(error) => {
            proof.fail(&mut exit, error);
            return;
        }
    };
    let ecs = (window.physical_width(), window.physical_height());
    let Some((viewport, raster)) = front.raster_dimensions(&images) else {
        return;
    };
    let phase = proof.phase;
    let expected = match phase {
        0 => Some((1600, 900)),
        1 => Some((1920, 1080)),
        2 | 5 => Some((2560, 1440)),
        4 => proof.fullscreen_size,
        _ => monitor_size,
    };
    let mode_matches = if matches!(phase, 3 | 4) {
        matches!(window.mode, WindowMode::BorderlessFullscreen(_))
    } else {
        window.mode == WindowMode::Windowed
    };
    let valid = ecs.0 > 0
        && ecs.1 > 0
        && native == ecs
        && viewport == ecs
        && raster == ecs
        && expected.is_none_or(|e| e == ecs)
        && (!matches!(phase, 3 | 4) || monitor_size == Some(ecs))
        && mode_matches;
    if let Some(captured) = proof.captured.take() {
        if Some(captured) != proof.capture_size || !valid {
            proof.fail(
                &mut exit,
                "captured dimensions or settled native/ECS/raster dimensions changed",
            );
            return;
        }
        info!(
            "display proof: {} capture ACK{}x{}",
            NAMES[phase], captured.0, captured.1
        );
        proof.phase += 1;
        if proof.phase == NAMES.len() {
            proof.finished = true;
            info!("display proof: six native HWND/ECS/worker-raster captures ACKed; failures:0");
            exit.write(AppExit::Success);
            return;
        }
        proof.requested = false;
        proof.capture_size = None;
        return;
    }
    if proof.capture_size.is_some() {
        return;
    }
    let now = proof.start.elapsed();
    if !proof.gate.observe(valid, front.presented_serial(), now) {
        return;
    }
    if phase == 3 {
        proof.fullscreen_size = Some(ecs);
    }
    info!(
        "display proof: {} HWND{}x{}, physical{}x{}, logical{}x{}, scale{}, viewport{}x{}, raster{}x{}, serial{}, mode{:?}, monitor{:?}, natural focus{}",
        NAMES[phase],
        native.0,
        native.1,
        ecs.0,
        ecs.1,
        window.width(),
        window.height(),
        window.scale_factor(),
        viewport.0,
        viewport.1,
        raster.0,
        raster.1,
        front.presented_serial(),
        window.mode,
        monitor_size,
        window.focused
    );
    proof.capture_size = Some(ecs);
    let path = proof.output.join(format!("display-{}.png", NAMES[phase]));
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(
            move |shot: On<ScreenshotCaptured>, mut proof: ResMut<Proof>| {
                if proof.phase == phase && proof.capture_size.is_some() {
                    let size = shot.image.texture_descriptor.size;
                    proof.captured = Some((size.width, size.height));
                }
            },
        );
}

#[cfg(windows)]
fn native_dimensions(owner: &RawHandleWrapper) -> Result<(u32, u32), String> {
    use windows_sys::Win32::{
        Foundation::RECT,
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{GetClientRect, GetWindowThreadProcessId, IsWindow},
    };
    let raw_window_handle::RawWindowHandle::Win32(window) = owner.get_window_handle() else {
        return Err("Win32 window required".into());
    };
    let hwnd = window.hwnd.get() as *mut std::ffi::c_void;
    let mut process = 0;
    let mut rect = RECT::default();
    // Borrowed RawHandleWrapper keeps the owned HWND alive. This system is
    // UI-thread-only and also rejects stale/foreign handles before calling Win32.
    unsafe {
        let thread = GetWindowThreadProcessId(hwnd, &mut process);
        if IsWindow(hwnd) == 0 || process != std::process::id() || thread != GetCurrentThreadId() {
            return Err("stale, foreign or wrong-thread HWND".into());
        }
        if GetClientRect(hwnd, &mut rect) == 0 {
            return Err("GetClientRect failed".into());
        }
    }
    let width = u32::try_from(rect.right - rect.left).map_err(|_| "invalid native client width")?;
    let height =
        u32::try_from(rect.bottom - rect.top).map_err(|_| "invalid native client height")?;
    if width == 0 || height == 0 {
        return Err("native client minimized/empty".into());
    }
    Ok((width, height))
}
#[cfg(not(windows))]
fn native_dimensions(_: &RawHandleWrapper) -> Result<(u32, u32), String> {
    Err("native display proof currently requires Windows HWND evidence".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_needs_two_distinct_valid_rasters_after_request() {
        let mut gate = Settled {
            serial: 7,
            ..Default::default()
        };
        assert!(!gate.observe(true, 7, Duration::ZERO));
        assert!(!gate.observe(true, 7, Duration::from_secs(2)));
        assert!(!gate.observe(true, 8, Duration::from_secs(2)));
        assert!(gate.observe(true, 9, Duration::from_secs(3)));
    }
    #[test]
    fn mismatched_native_resize_discards_prior_raster_readiness() {
        let mut gate = Settled::default();
        assert!(!gate.observe(true, 1, Duration::ZERO));
        assert!(!gate.observe(false, 2, Duration::from_secs(2)));
        assert!(!gate.observe(true, 2, Duration::from_secs(3)));
        assert!(!gate.observe(true, 3, Duration::from_secs(5)));
        assert!(gate.observe(true, 4, Duration::from_secs(6)));
    }
}

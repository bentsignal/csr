//! Apply display preferences without overwriting the OS-owned fullscreen size.
use crate::settings::{RESOLUTIONS, Settings};
use bevy::window::{MonitorSelection, PresentMode, Window, WindowMode, WindowResolution};

/// Choose the initial physical size before the OS window is created. Screenshot
/// fixtures retain their controlled dimensions and windowed mode.
pub(crate) fn startup(
    options: &crate::ClientOptions,
    settings: &Settings,
) -> (WindowResolution, WindowMode) {
    let fixture = options.screenshot.is_some();
    let (width, height) = if fixture || options.size_explicit {
        (options.width, options.height)
    } else {
        RESOLUTIONS[settings.resolution.min(RESOLUTIONS.len() - 1)]
    };
    (
        WindowResolution::new(width, height),
        mode(!fixture && settings.fullscreen),
    )
}

fn mode(fullscreen: bool) -> WindowMode {
    if fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Preferences {
    fullscreen: bool,
    resolution: usize,
    present_mode: PresentMode,
}

#[derive(Default)]
pub(crate) struct DisplayState {
    applied: Option<Preferences>,
    windowed_override: Option<(u32, u32)>,
}
impl DisplayState {
    pub(crate) fn apply(
        &mut self,
        window: &mut Window,
        settings: &Settings,
        explicit_size: Option<(u32, u32)>,
    ) {
        let current = Preferences {
            fullscreen: settings.fullscreen,
            resolution: settings.resolution.min(RESOLUTIONS.len() - 1),
            present_mode: crate::input_latency::present_mode(settings.vsync),
        };
        let previous = self.applied;
        if previous.is_none() {
            self.windowed_override = explicit_size;
        } else if previous.is_some_and(|old| old.resolution != current.resolution) {
            // A deliberate resolution selection supersedes the launch flag.
            // Audio, presentation and fullscreen changes do not.
            self.windowed_override = None;
        }
        if previous.is_none_or(|old| old.fullscreen != current.fullscreen) {
            window.mode = mode(current.fullscreen);
        }
        // Borderless mode takes its drawable size from the selected monitor.
        // The saved size is a physical windowed preference, not a DPI-scaled
        // logical size or a request to shrink an existing fullscreen window.
        if !current.fullscreen
            && previous.is_none_or(|old| old.fullscreen || old.resolution != current.resolution)
        {
            let (width, height) = self
                .windowed_override
                .unwrap_or(RESOLUTIONS[current.resolution]);
            window.resolution.set_physical_resolution(width, height);
        }
        if previous.is_none_or(|old| old.present_mode != current.present_mode) {
            window.present_mode = current.present_mode;
            bevy::log::info!(
                "display presentation: requested {:?}, saved_vsync={}, surface_latency={:?}",
                current.present_mode,
                settings.vsync,
                window.desired_maximum_frame_latency
            );
        }
        self.applied = Some(current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::WindowResolution;

    #[test]
    fn startup_distinguishes_saved_cli_and_screenshot_dimensions() {
        let settings = Settings {
            resolution: 2,
            fullscreen: true,
            ..Default::default()
        };
        let ordinary = crate::ClientOptions::default();
        let (resolution, window_mode) = startup(&ordinary, &settings);
        assert_eq!(
            (resolution.physical_width(), resolution.physical_height()),
            (1920, 1080)
        );
        assert_eq!(
            window_mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        );

        // Explicit dimensions equal to the CLI defaults are still an override.
        let explicit = crate::ClientOptions {
            size_explicit: true,
            ..ordinary.clone()
        };
        let (resolution, window_mode) = startup(&explicit, &settings);
        assert_eq!(
            (resolution.physical_width(), resolution.physical_height()),
            (1600, 900)
        );
        assert_eq!(
            window_mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        );

        let mut fixture = crate::ClientOptions {
            screenshot: Some("shot.png".into()),
            ..ordinary
        };
        let (resolution, window_mode) = startup(&fixture, &settings);
        assert_eq!(
            (resolution.physical_width(), resolution.physical_height()),
            (1600, 900)
        );
        assert_eq!(window_mode, WindowMode::Windowed);
        fixture.width = 1280;
        fixture.height = 720;
        fixture.size_explicit = true;
        let (resolution, window_mode) = startup(&fixture, &settings);
        assert_eq!(
            (resolution.physical_width(), resolution.physical_height()),
            (1280, 720)
        );
        assert_eq!(window_mode, WindowMode::Windowed);
    }

    #[test]
    fn cli_windowed_size_survives_fullscreen_until_resolution_is_changed() {
        let mut state = DisplayState::default();
        let mut window = Window::default();
        let mut settings = Settings {
            resolution: 2,
            ..Default::default()
        };
        let explicit = Some((1536, 864));
        state.apply(&mut window, &settings, explicit);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (1536, 864)
        );
        settings.volume = 0.76;
        settings.vsync = true;
        state.apply(&mut window, &settings, explicit);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (1536, 864)
        );
        assert_eq!(window.present_mode, PresentMode::AutoVsync);

        settings.fullscreen = true;
        state.apply(&mut window, &settings, explicit);
        // OS monitor dimensions must remain independent of the CLI size.
        window.resolution.set_physical_resolution(2560, 1440);
        settings.volume = 0.5;
        state.apply(&mut window, &settings, explicit);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (2560, 1440)
        );
        settings.fullscreen = false;
        state.apply(&mut window, &settings, explicit);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (1536, 864)
        );

        settings.fullscreen = true;
        state.apply(&mut window, &settings, explicit);
        settings.resolution = 0;
        state.apply(&mut window, &settings, explicit);
        settings.fullscreen = false;
        state.apply(&mut window, &settings, explicit);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (1280, 720)
        );
        settings.volume = 0.76;
        state.apply(&mut window, &settings, explicit);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (1280, 720)
        );
    }

    #[test]
    fn saved_windowed_dimensions_are_physical_on_high_dpi() {
        let mut window = Window {
            resolution: WindowResolution::new(1280, 720),
            ..Default::default()
        };
        window.resolution.set_scale_factor(1.5);
        let settings = Settings {
            resolution: 2,
            ..Default::default()
        };
        DisplayState::default().apply(&mut window, &settings, None);
        assert_eq!(window.physical_width(), 1920);
        assert_eq!(window.physical_height(), 1080);
        assert_eq!(window.width(), 1280.);
        assert_eq!(window.height(), 720.);
        assert_eq!(window.scale_factor(), 1.5);
    }

    #[test]
    fn fullscreen_preserves_actual_monitor_size_and_audio_changes_do_not_resize() {
        let mut state = DisplayState::default();
        let mut window = Window {
            resolution: WindowResolution::new(1600, 900),
            ..Default::default()
        };
        let mut settings = Settings {
            fullscreen: true,
            ..Default::default()
        };
        state.apply(&mut window, &settings, None);
        assert_eq!(
            window.mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        );
        // Actual OS fullscreen resize arrives after entering the mode.
        window.resolution.set_physical_resolution(2560, 1440);
        window.resolution.set_scale_factor(1.25);
        settings.volume = 0.76;
        state.apply(&mut window, &settings, None); // also covers LocalPlayer admission reapplication
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (2560, 1440)
        );
        settings.resolution = 0; // retained only for the next windowed transition
        state.apply(&mut window, &settings, None);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (2560, 1440)
        );
        settings.fullscreen = false;
        state.apply(&mut window, &settings, None);
        assert_eq!(window.mode, WindowMode::Windowed);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (1280, 720)
        );
    }

    #[test]
    fn window_resize_and_dpi_events_survive_unrelated_preferences() {
        let mut state = DisplayState::default();
        let mut window = Window::default();
        let mut settings = Settings::default();
        state.apply(&mut window, &settings, None);
        // A player resize is OS state, not a change in the saved preference.
        window.resolution.set_physical_resolution(1913, 1227);
        window.resolution.set_scale_factor(1.25);
        settings.sensitivity = 3.;
        state.apply(&mut window, &settings, None);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (1913, 1227)
        );
        settings.vsync = true;
        state.apply(&mut window, &settings, None);
        assert_eq!(window.present_mode, PresentMode::AutoVsync);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (1913, 1227)
        );
        settings.resolution = 3;
        state.apply(&mut window, &settings, None);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (2560, 1440)
        );
    }
}

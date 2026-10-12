//! Apply display preferences without overwriting the OS-owned fullscreen size.
use crate::settings::{RESOLUTIONS, Settings};
use bevy::window::{MonitorSelection, PresentMode, Window, WindowMode};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Preferences {
    fullscreen: bool,
    resolution: usize,
    vsync: bool,
}

#[derive(Default)]
pub(crate) struct DisplayState {
    applied: Option<Preferences>,
}
impl DisplayState {
    pub(crate) fn apply(&mut self, window: &mut Window, settings: &Settings) {
        let current = Preferences {
            fullscreen: settings.fullscreen,
            resolution: settings.resolution.min(RESOLUTIONS.len() - 1),
            vsync: settings.vsync,
        };
        let previous = self.applied;
        if previous.is_none_or(|old| old.fullscreen != current.fullscreen) {
            window.mode = if current.fullscreen {
                WindowMode::BorderlessFullscreen(MonitorSelection::Current)
            } else {
                WindowMode::Windowed
            };
        }
        // Borderless mode takes its drawable size from the selected monitor.
        // The saved size is a physical windowed preference, not a DPI-scaled
        // logical size or a request to shrink an existing fullscreen window.
        if !current.fullscreen
            && previous.is_none_or(|old| old.fullscreen || old.resolution != current.resolution)
        {
            let (width, height) = RESOLUTIONS[current.resolution];
            window.resolution.set_physical_resolution(width, height);
        }
        if previous.is_none_or(|old| old.vsync != current.vsync) {
            window.present_mode = if current.vsync {
                PresentMode::AutoVsync
            } else {
                PresentMode::AutoNoVsync
            };
        }
        self.applied = Some(current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::WindowResolution;

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
        DisplayState::default().apply(&mut window, &settings);
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
        state.apply(&mut window, &settings);
        assert_eq!(
            window.mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        );
        // Actual OS fullscreen resize arrives after entering the mode.
        window.resolution.set_physical_resolution(2560, 1440);
        window.resolution.set_scale_factor(1.25);
        settings.volume = 0.76;
        state.apply(&mut window, &settings); // also covers LocalPlayer admission reapplication
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (2560, 1440)
        );
        settings.resolution = 0; // retained only for the next windowed transition
        state.apply(&mut window, &settings);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (2560, 1440)
        );
        settings.fullscreen = false;
        state.apply(&mut window, &settings);
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
        state.apply(&mut window, &settings);
        // A player resize is OS state, not a change in the saved preference.
        window.resolution.set_physical_resolution(1913, 1227);
        window.resolution.set_scale_factor(1.25);
        settings.sensitivity = 3.;
        state.apply(&mut window, &settings);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (1913, 1227)
        );
        settings.vsync = true;
        state.apply(&mut window, &settings);
        assert_eq!(window.present_mode, PresentMode::AutoVsync);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (1913, 1227)
        );
        settings.resolution = 3;
        state.apply(&mut window, &settings);
        assert_eq!(
            (window.physical_width(), window.physical_height()),
            (2560, 1440)
        );
    }
}

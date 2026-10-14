//! Desktop rendering policy: present the current input frame without CPU pipelining.
use bevy::{app::PluginGroupBuilder, prelude::*, render::pipelined_rendering::PipelinedRenderingPlugin};
use std::num::NonZeroU32;
use bevy::window::PresentMode;

/// A valid diagnostic override wins over saved preferences at startup and
/// during live preference application. It never changes the saved choice.
pub(crate) fn present_mode(vsync: bool) -> PresentMode {
    resolve_present_mode(vsync, std::env::var("CSR_PRESENT").ok().as_deref())
}

fn resolve_present_mode(vsync: bool, requested: Option<&str>) -> PresentMode {
    match requested {
        Some("immediate") => PresentMode::Immediate,
        Some("mailbox") => PresentMode::Mailbox,
        Some("fifo") => PresentMode::Fifo,
        _ if vsync => PresentMode::AutoVsync,
        _ => PresentMode::AutoNoVsync,
    }
}

/// Log app-frame intervals in ordinary interactive play as well as captures.
/// This is not a measurement of mouse-to-display or compositor latency.
pub(crate) fn install_frame_log(app: &mut App) {
    let limit = std::env::var("CSR_FRAME_LOG").ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v >= 0.);
    if let Some(limit) = limit {
        app.add_systems(Last, move |real: Res<Time<Real>>, mut frame: Local<u64>| {
            *frame += 1;
            let ms = real.delta_secs() * 1000.;
            if ms > limit { info!("slow frame {}: {ms:.1} ms", *frame); }
        });
    }
}

pub(crate) fn desktop_plugins() -> PluginGroupBuilder {
    // Bevy's pipeline renders frame N while the main app advances N + 1.
    // Local look already uses this frame's mouse input; keep that pose in the
    // frame we render instead of allowing an extra CPU frame to run ahead.
    DefaultPlugins.build().disable::<PipelinedRenderingPlugin>()
}

pub(crate) fn surface_frame_latency() -> Option<NonZeroU32> {
    // This is a backend hint, not a guarantee about the OS compositor/scanout.
    NonZeroU32::new(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_present_override_wins_and_invalid_override_keeps_saved_vsync() {
        for vsync in [false, true] {
            for (name, expected) in [("immediate", PresentMode::Immediate),
                ("mailbox", PresentMode::Mailbox), ("fifo", PresentMode::Fifo)]
            {
                assert_eq!(resolve_present_mode(vsync, Some(name)), expected);
            }
            let saved = if vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
            for requested in [None, Some(""), Some("invalid")] {
                assert_eq!(resolve_present_mode(vsync, requested), saved);
            }
        }
    }

    #[test]
    fn desktop_retains_renderer_without_running_a_cpu_frame_ahead() {
        let plugins = desktop_plugins();
        assert!(plugins.enabled::<bevy::render::RenderPlugin>());
        assert!(!plugins.enabled::<PipelinedRenderingPlugin>());
        assert_eq!(surface_frame_latency().unwrap().get(), 1);
    }
}

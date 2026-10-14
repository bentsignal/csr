//! Desktop rendering policy: present the current input frame without CPU pipelining.
use bevy::{app::PluginGroupBuilder, prelude::*, render::pipelined_rendering::PipelinedRenderingPlugin};
use std::num::NonZeroU32;

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
    fn desktop_retains_renderer_without_running_a_cpu_frame_ahead() {
        let plugins = desktop_plugins();
        assert!(plugins.enabled::<bevy::render::RenderPlugin>());
        assert!(!plugins.enabled::<PipelinedRenderingPlugin>());
        assert_eq!(surface_frame_latency().unwrap().get(), 1);
    }
}

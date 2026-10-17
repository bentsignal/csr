//! Shared classic HUD space. Geometry is in the movies' 720-high authored units,
//! scaled once by the recorded HUD setting, independently of desktop DPI.
use bevy::prelude::*;

pub(crate) const PLAYER_WIDTH: f32 = 36.0;
pub(crate) const PLAYER_HEIGHT: f32 = 40.0;
pub(crate) const PLAYER_GAP: f32 = 1.5;
pub(crate) const CENTER_WIDTH: f32 = 64.0;
pub(crate) const CENTER_GAP: f32 = 4.0;
pub(crate) const NOTICE_TOP: f32 = PLAYER_HEIGHT + 12.0;
pub(crate) const RADAR_LEFT: f32 = 12.0;
pub(crate) const RADAR_TOP: f32 = 48.0;
pub(crate) const RADAR_DIAMETER: f32 = 196.0;
pub(crate) const INVENTORY_PITCH: f32 = 64.0;
pub(crate) const INVENTORY_BOTTOM: f32 = 40.0;
pub(crate) const INVENTORY_ICON_HEIGHT: f32 = 32.0;
pub(crate) const INVENTORY_ICON_WIDTH: f32 = 128.0;

pub(crate) const fn unit(value: f32) -> Val {
    Val::Vh(value * scaleform::HUD_SCALING as f32 * 100.0 / 720.0)
}

pub(crate) const fn font(value: f32) -> FontSize {
    FontSize::Vh(value * scaleform::HUD_SCALING as f32 * 100.0 / 720.0)
}

/// Convert the previous 1080p fallback geometry to the same authored space.
pub(crate) const fn legacy(value: f32) -> Val { unit(value / 1.5) }
pub(crate) const fn legacy_font(value: f32) -> FontSize { font(value / 1.5) }

/// A region of an already-rasterized movie must remain one physical pixel per
/// screen pixel. Unlike `unit`, this must cancel Bevy's DPI AND UiScale.
pub(crate) fn raster_pixel(value: u32, dpi: f32, ui_scale: f32) -> Val {
    px(value as f32 / (dpi * ui_scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_space_scales_with_height_and_rasters_cancel_desktop_scaling() {
        // These exercise Bevy's real viewport-unit resolution, including a
        // non-16:9 viewport. Changing width or DPI must not resize HUD glyphs.
        for viewport in [Vec2::new(1920., 1080.), Vec2::new(1600., 900.), Vec2::new(1440., 1080.)] {
            for dpi in [1., 1.5, 2.] {
                let height = unit(40.).resolve(dpi, 0., viewport).unwrap();
                assert!((height - viewport.y * 40. * 0.85 / 720.).abs() < 0.001);
                let raster = raster_pixel(541, dpi, 1.25).resolve(dpi * 1.25, 0., viewport).unwrap();
                assert!((raster - 541.).abs() < 0.001);
            }
        }
        let viewport = Vec2::new(1920., 1080.);
        let strip = unit(PLAYER_HEIGHT).resolve(1., 0., viewport).unwrap();
        let notice = unit(NOTICE_TOP).resolve(1., 0., viewport).unwrap();
        assert!(notice > strip, "notices start below the player strip");
        assert!((notice - 66.3).abs() < 0.01);
    }
}

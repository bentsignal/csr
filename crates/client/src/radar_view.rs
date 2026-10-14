//! One cosmetic map transform shared by the overview texture and ally markers.
use bevy::{prelude::*, render::render_resource::AsBindGroup, shader::ShaderRef};

pub(crate) const DIAMETER: f32 = 250.0;
pub(crate) const OVERVIEW_PIXELS: f32 = 1024.0;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(crate) struct RadarMaterial {
    // Player overview UV, screen-to-overview scale, clockwise screen rotation.
    #[uniform(0)]
    pub view: Vec4,
    #[texture(1)]
    #[sampler(2)]
    pub overview: Handle<Image>,
}
impl UiMaterial for RadarMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://client/shaders/radar.wgsl".into()
    }
}

pub(crate) fn view(center: Vec2, zoom: f32, rotate: bool, yaw_degrees: f32) -> Vec4 {
    Vec4::new(
        center.x,
        center.y,
        DIAMETER / (OVERVIEW_PIXELS * zoom.clamp(0.4, 1.0)),
        if rotate {
            (yaw_degrees - 90.0).to_radians()
        } else {
            0.0
        },
    )
}

pub(crate) fn marker_offset(view: Vec4, overview_uv: Vec2) -> Vec2 {
    let delta = (overview_uv - Vec2::new(view.x, view.y)) * (DIAMETER / view.z);
    let (s, c) = view.w.sin_cos();
    Vec2::new(c * delta.x - s * delta.y, s * delta.x + c * delta.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn centered_zoom_and_rotation_keep_texture_and_markers_in_one_space() {
        let center = Vec2::new(0.2, 0.7);
        let uv = center + Vec2::new(20.0 / OVERVIEW_PIXELS, 0.0);
        assert_eq!(
            marker_offset(view(center, 0.5, false, 0.0), center),
            Vec2::ZERO
        );
        let north_up = marker_offset(view(center, 0.5, false, 0.0), uv);
        assert!((north_up - Vec2::new(10.0, 0.0)).length() < 1e-4);
        let east_up = marker_offset(view(center, 0.5, true, 0.0), uv);
        assert!((east_up - Vec2::new(0.0, -10.0)).length() < 1e-4);
        let zoomed = marker_offset(view(center, 1.0, true, 0.0), uv);
        assert!((zoomed - east_up * 2.0).length() < 1e-4);
    }
}

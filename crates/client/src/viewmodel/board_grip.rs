//! Held crossover equipment rides on the installed default knife's handle.
//! The knife's own arms, draw/idle/inspect clips, look lag, bob and settling
//! therefore move the board exactly like a CS weapon; only the blade is hidden.
use super::*;

/// Default board pose in the handle bone's frame. Board-local axes are the
/// imported deck's (X across, Y up through the grip tape, Z nose to tail),
/// centred on the deck and in inches. The fist closes over the deck's rail
/// midway between the trucks, wheels facing away from the palm.
const ATTACH: [f32; 6] = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

fn offset() -> glam::Mat4 {
    let mut v = ATTACH;
    if let Ok(text) = std::env::var("CSR_BOARD_ATTACH") {
        for (slot, value) in v.iter_mut().zip(text.split_whitespace()) {
            if let Ok(value) = value.parse::<f32>() { if value.is_finite() { *slot = value; } }
        }
    }
    let rotation = glam::Quat::from_euler(glam::EulerRot::ZYX,
        v[5].to_radians(), v[4].to_radians(), v[3].to_radians());
    glam::Mat4::from_rotation_translation(rotation, glam::Vec3::new(v[0], v[1], v[2]))
}

fn handle(model: &mdl::Model) -> Option<usize> {
    let name = |b: &mdl::skeleton::Bone| b.name.to_ascii_lowercase();
    model.bones.iter().position(|b| name(b) == "v_weapon.knife")
        .or_else(|| model.bones.iter().position(|b| { let n = name(b); n.contains("knife") && !n.contains("hand") }))
        .or_else(|| model.bones.iter().position(|b| name(b).ends_with("r_hand")))
}

/// Source-space transform from board-local inches to this frame's world pose.
pub(super) fn attachment(model: &mdl::Model, pose: &[glam::Mat4], source_from_model: glam::Mat4) -> Option<glam::Mat4> {
    let bone = handle(model)?;
    if std::env::var_os("CSR_BOARD_GRIP_TRACE").is_some() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| for (i, b) in model.bones.iter().enumerate() {
            let m = pose[i];
            info!("BOARD_GRIP bone={i} name={} parent={} at={:?} x={:?} y={:?} z={:?}", b.name, b.parent,
                m.w_axis.truncate(), m.x_axis.truncate(), m.y_axis.truncate(), m.z_axis.truncate());
        });
    }
    let m = source_from_model * pose.get(bone)? * offset();
    m.is_finite().then_some(m)
}

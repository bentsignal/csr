//! Held crossover equipment rides on the installed default knife's handle.
//! The knife's own arms, draw/idle/inspect clips, look lag, bob and settling
//! therefore move the board exactly like a CS weapon; only the blade is hidden.
//!
//! The board is carried by its front truck: the closed fist wraps the hanger
//! (the knife handle's axis becomes the axle), the deck hangs below the hand
//! with its nose up and graphic toward the viewer. That hang is derived once
//! from the knife's idle pose and then held rigidly, so every clip, including
//! inspection, turns the board with the hand.
use super::*;

/// Board-local axes are the imported deck's (X across, Y up through the grip
/// tape, Z tail to nose), in inches relative to the front truck's hanger.
/// Forward lean of the hanging deck away from vertical, degrees.
const LEAN: f32 = 18.0;

fn env<const N: usize>(name: &str, default: [f32; N]) -> [f32; N] {
    let mut v = default;
    if let Ok(text) = std::env::var(name) {
        for (slot, value) in v.iter_mut().zip(text.split_whitespace()) {
            if let Ok(value) = value.parse::<f32>() { if value.is_finite() { *slot = value; } }
        }
    }
    v
}

fn handle(bones: &[mdl::skeleton::Bone]) -> Option<usize> {
    let name = |b: &mdl::skeleton::Bone| b.name.to_ascii_lowercase();
    bones.iter().position(|b| name(b) == "v_weapon.knife")
        .or_else(|| bones.iter().position(|b| { let n = name(b); n.contains("knife") && !n.contains("hand") }))
        .or_else(|| bones.iter().position(|b| name(b).ends_with("r_hand")))
}

/// The hanging carry in the handle bone's frame, from the idle model pose.
pub(super) fn carry(bones: &[mdl::skeleton::Bone], idle: &[glam::Mat4]) -> Option<glam::Mat4> {
    use glam::Vec3;
    let held = *idle.get(handle(bones)?)?;
    let grip = held.w_axis.truncate();
    let axle = held.z_axis.truncate().normalize_or_zero();
    let [lean, turn, tx, ty, tz] = env("CSR_BOARD_CARRY", [LEAN, 0.0, 0.0, 0.0, 0.0]);
    // Model space is X forward, Z up; the eye sits at the model origin.
    let down = (-Vec3::Z + Vec3::X * lean.to_radians().tan()).normalize();
    let tail = (down - axle * axle.dot(down)).normalize_or_zero();
    if axle == Vec3::ZERO || tail == Vec3::ZERO { return None; }
    let nose = -tail;
    let mut across = axle;
    // The deck lies on the far side of the trucks from the viewer.
    if nose.cross(across).dot(grip) < 0.0 { across = -across; }
    let up = nose.cross(across);
    let hang = glam::Mat4::from_cols(across.extend(0.0), up.extend(0.0), nose.extend(0.0), grip.extend(1.0))
        * glam::Mat4::from_rotation_z(turn.to_radians())
        * glam::Mat4::from_translation(Vec3::new(tx, ty, tz));
    let carry = held.inverse() * hang;
    carry.is_finite().then_some(carry)
}

/// A carried board turns the forearm over (pronation), levelling the fist so
/// the axle runs across it. The roll is about the forearm's own axis and is
/// shared by the forearm twist bones, as the installed arm skin expects; a
/// further hand-local bend can relax the wrist. All on top of every clip.
const WRIST: [f32; 3] = [0.0, 0.0, 0.0];
pub(super) fn carry_wrist(bones: &[mdl::skeleton::Bone], pose: &mut [glam::Mat4]) {
    let [roll, bend, deviate] = env("CSR_BOARD_WRIST", WRIST);
    if [roll, bend, deviate] == [0.0; 3] { return; }
    let name = |i: usize| bones[i].name.to_ascii_lowercase();
    let Some(hand) = (0..bones.len()).find(|i| name(*i).ends_with("r_hand")) else { return };
    let Some(&wrist) = pose.get(hand) else { return };
    let elbow = (0..bones.len()).find(|i| name(*i).ends_with("r_forearm")).and_then(|i| pose.get(i).copied());
    let at = wrist.w_axis.truncate();
    let axis = elbow.map_or(glam::Vec3::X, |e| (at - e.w_axis.truncate()).normalize_or(glam::Vec3::X));
    let twist = glam::Mat4::from_translation(at) * glam::Mat4::from_axis_angle(axis, roll.to_radians())
        * glam::Mat4::from_translation(-at);
    let turn = twist * wrist * glam::Mat4::from_quat(glam::Quat::from_euler(glam::EulerRot::ZYX,
        deviate.to_radians(), bend.to_radians(), 0.0)) * wrist.inverse();
    let below = |mut i: usize| loop {
        if i == hand { return true; }
        let parent = bones[i].parent;
        if parent < 0 { return false; }
        i = parent as usize;
    };
    let held = handle(bones);
    for i in 0..bones.len().min(pose.len()) {
        if below(i) || (Some(i) == held && i != hand) { pose[i] = turn * pose[i]; }
        else if name(i).ends_with("r_foretwist") { pose[i] = twist * pose[i]; }
    }
}

/// Where the carried board's arm sits relative to a weapon (forward, right, up).
const VIEW: [f32; 3] = [0.0, 0.0, 0.0];
pub(super) fn view_offset(f: glam::Vec3, r: glam::Vec3, u: glam::Vec3) -> glam::Vec3 {
    let v = env("CSR_BOARD_VIEW", VIEW);
    f * v[0] + r * v[1] + u * v[2]
}

/// Collapse every left-arm bone far below the shoulders, out of view.
pub(super) fn hide_left_arm(bones: &[mdl::skeleton::Bone], pose: &mut [glam::Mat4]) {
    let left = |i: usize| { let n = bones[i].name.to_ascii_lowercase(); n.contains("_l_") || n.contains("left") };
    let Some(root) = (0..bones.len()).find(|i| left(*i)) else { return };
    let anchor = pose[root].w_axis.truncate() - glam::Vec3::Z * 40.;
    for i in 0..bones.len().min(pose.len()) {
        if left(i) { pose[i] = glam::Mat4::from_translation(anchor) * glam::Mat4::from_scale(glam::Vec3::splat(1e-4)); }
    }
}

/// Source-space transform from hanger-relative board inches to this frame's world pose.
pub(super) fn attachment(model: &mdl::Model, pose: &[glam::Mat4], source_from_model: glam::Mat4,
    carry: glam::Mat4) -> Option<glam::Mat4> {
    let bone = handle(&model.bones)?;
    if std::env::var_os("CSR_BOARD_GRIP_TRACE").is_some() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| for (i, b) in model.bones.iter().enumerate() {
            let m = pose[i];
            info!("BOARD_GRIP bone={i} name={} parent={} at={:?} x={:?} y={:?} z={:?}", b.name, b.parent,
                m.w_axis.truncate(), m.x_axis.truncate(), m.y_axis.truncate(), m.z_axis.truncate());
        });
    }
    let m = source_from_model * pose.get(bone)? * carry;
    m.is_finite().then_some(m)
}

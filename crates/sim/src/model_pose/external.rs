//! Retarget the provider's completed world pose onto the installed CS rig.
//! Bind axes come from anatomy: modern CS:GO's bind rig is Y-up although its
//! gameplay and evaluated animations are Source Z-up. Older ValveBiped works too.
use crate::Player;
use glam::{Mat3, Mat4, Quat};
fn mapped(name: &str) -> Option<&'static str> {
    let name = name.rsplit('.').next().unwrap_or(name).to_ascii_lowercase();
    Some(match name.as_str() {
        "pelvis" | "bip01_pelvis" => "HIPS",
        "spine_0" | "bip01_spine" => "SPINE",
        "spine_1" | "bip01_spine1" => "SPINE1",
        "spine_2" | "bip01_spine2" => "SPINE2",
        "spine_3" | "bip01_spine3" => "SPINE3",
        "neck_0" | "bip01_neck1" => "NECK1",
        "head_0" | "bip01_head1" => "HEAD",
        "clavicle_l" | "bip01_l_clavicle" => "LEFTSHOULDER",
        "arm_upper_l" | "bip01_l_upperarm" => "LEFTARM",
        "arm_lower_l" | "bip01_l_forearm" => "LEFTFOREARM",
        "hand_l" | "bip01_l_hand" => "LEFTHAND",
        "clavicle_r" | "bip01_r_clavicle" => "RIGHTSHOULDER",
        "arm_upper_r" | "bip01_r_upperarm" => "RIGHTARM",
        "arm_lower_r" | "bip01_r_forearm" => "RIGHTFOREARM",
        "hand_r" | "bip01_r_hand" => "RIGHTHAND",
        "leg_upper_l" | "bip01_l_thigh" => "LEFTUPLEG",
        "leg_lower_l" | "bip01_l_calf" => "LEFTLEG",
        "ankle_l" | "bip01_l_foot" => "LEFTFOOT",
        "ball_l" | "bip01_l_toe0" => "LEFTTOEBASE",
        "leg_upper_r" | "bip01_r_thigh" => "RIGHTUPLEG",
        "leg_lower_r" | "bip01_r_calf" => "RIGHTLEG",
        "ankle_r" | "bip01_r_foot" => "RIGHTFOOT",
        "ball_r" | "bip01_r_toe0" => "RIGHTTOEBASE",
        _ => return None,
    })
}
pub(super) fn apply(
    player: &Player,
    bones: &[mdl::skeleton::Bone],
    pose: &mut [Mat4],
    overstep: Option<f32>,
) -> bool {
    let Some(state) = player.mechanic.as_ref().and_then(|s| s.external.as_ref()) else {
        return false;
    };
    let rig = &state.rig;
    let native = if let Some(seconds) = overstep {
        mechanics::presentation::bones(&state.previous_pose, &state.pose, rig, seconds)
    } else {
        state
            .pose
            .bones
            .iter()
            .map(Mat4::from_cols_array_2d)
            .collect()
    };
    if native.is_empty() || native.len() != rig.rest.len() {
        return false;
    }
    let mapping: Vec<_> = bones
        .iter()
        .map(|b| mapped(&b.name).and_then(|name| rig.names.iter().position(|n| n == name)))
        .collect();
    let target = |name: &str| bones.iter().position(|b| mapped(&b.name) == Some(name));
    let (Some(hips), Some(head), Some(left), Some(right)) = (
        target("HIPS"),
        target("HEAD"),
        target("LEFTUPLEG"),
        target("RIGHTUPLEG"),
    ) else {
        return false;
    };
    let conversion = Mat4::from_cols_array_2d(&rig.to_source);
    let (_, basis, _) = conversion.to_scale_rotation_translation();
    let source_alignment = Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2);
    let frame = Mat4::from_rotation_translation(
        Quat::from_rotation_z(player.view_angles.y.to_radians()),
        player.movement.origin,
    );
    let inverse = frame.inverse();
    let rest = mdl::skeleton::model_space(bones, &mdl::skeleton::rest_pose(bones));
    let up = (rest[head].w_axis - rest[hips].w_axis)
        .truncate()
        .normalize_or_zero();
    let left_direction = (rest[left].w_axis - rest[right].w_axis).truncate();
    let left_direction = (left_direction - up * up.dot(left_direction)).normalize_or_zero();
    let forward = left_direction.cross(up).normalize_or_zero();
    if up.length_squared() < 0.9 || forward.length_squared() < 0.9 {
        return false;
    }
    let target_alignment =
        Quat::from_mat3(&Mat3::from_cols(forward, left_direction, up).transpose());
    let target_reference = Mat4::from_quat(target_alignment);
    let mut globals = vec![Mat4::IDENTITY; bones.len()];
    // Keep the CS finger grips and auxiliary bones beneath the driven hands.
    let original = pose.to_vec();
    for (i, bone) in bones.iter().enumerate() {
        let parent =
            (bone.parent >= 0 && (bone.parent as usize) < i).then_some(bone.parent as usize);
        let (position, mut rotation) = if let Some(parent) = parent {
            let local = original[parent].inverse() * original[i];
            let (_, q, t) = local.to_scale_rotation_translation();
            let (_, parent_q, _) = globals[parent].to_scale_rotation_translation();
            (globals[parent].transform_point3(t), parent_q * q)
        } else {
            let global = frame * target_reference * rest[i];
            let (_, q, p) = global.to_scale_rotation_translation();
            (p, q)
        };
        let mut position = position;
        if let Some(source) = mapping[i] {
            let actual = native[source];
            let reference = Mat4::from_cols_array_2d(&rig.rest[source]);
            let (_, a, _) = actual.to_scale_rotation_translation();
            let (_, r, _) = reference.to_scale_rotation_translation();
            let (_, target, _) = rest[i].to_scale_rotation_translation();
            rotation = (basis
                * a
                * r.inverse()
                * basis.inverse()
                * source_alignment
                * target_alignment
                * target)
                .normalize();
            if i == hips {
                position = conversion.transform_point3(actual.w_axis.truncate());
            }
        }
        globals[i] = Mat4::from_rotation_translation(rotation, position);
    }
    for (out, global) in pose.iter_mut().zip(globals) {
        *out = inverse * global;
    }
    true
}

//! Retarget the provider's completed world pose onto the installed CS rig.
//! Bind axes come from anatomy: modern CS:GO's bind rig is Y-up although its
//! gameplay and evaluated animations are Source Z-up. Older ValveBiped works too.
use crate::Player;
use glam::{Mat3, Mat4, Quat, Vec3};
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
    // The CS legs are longer than the original skater's. Rotation-only
    // retargeting puts their soles through the deck. Solve the existing CS
    // segments to the completed native ankles, retaining lengths and grips.
    for (thigh, knee, foot) in [
        ("LEFTUPLEG", "LEFTLEG", "LEFTFOOT"),
        ("RIGHTUPLEG", "RIGHTLEG", "RIGHTFOOT"),
    ] {
        let (Some(a), Some(b), Some(c)) = (target(thigh), target(knee), target(foot)) else {
            continue;
        };
        let Some(source) = mapping[c] else { continue };
        let goal = conversion.transform_point3(native[source].w_axis.truncate());
        let hip = globals[a].w_axis.truncate();
        let joint = globals[b].w_axis.truncate();
        let ankle = globals[c].w_axis.truncate();
        let l1 = hip.distance(joint);
        let l2 = joint.distance(ankle);
        let Some(bend) = knee_position(hip, joint, goal, l1, l2) else {
            continue;
        };
        let (_, foot_rotation, _) = globals[c].to_scale_rotation_translation();
        rotate_chain(
            bones,
            &mut globals,
            a,
            hip,
            Quat::from_rotation_arc((joint - hip).normalize(), (bend - hip).normalize()),
        );
        let knee = globals[b].w_axis.truncate();
        let ankle = globals[c].w_axis.truncate();
        rotate_chain(
            bones,
            &mut globals,
            b,
            knee,
            Quat::from_rotation_arc((ankle - knee).normalize(), (goal - knee).normalize()),
        );
        let (_, current, ankle) = globals[c].to_scale_rotation_translation();
        rotate_chain(
            bones,
            &mut globals,
            c,
            ankle,
            foot_rotation * current.inverse(),
        );
    }
    for (out, global) in pose.iter_mut().zip(globals) {
        *out = inverse * global;
    }
    true
}
fn knee_position(hip: Vec3, joint: Vec3, goal: Vec3, l1: f32, l2: f32) -> Option<Vec3> {
    if l1 < 0.01 || l2 < 0.01 {
        return None;
    }
    let delta = goal - hip;
    let distance = delta.length();
    if distance < 0.01 {
        return None;
    }
    let axis = delta / distance;
    let distance = distance.clamp((l1 - l2).abs() + 0.001, l1 + l2 - 0.001);
    let along = (l1 * l1 - l2 * l2 + distance * distance) / (2. * distance);
    let height = (l1 * l1 - along * along).max(0.).sqrt();
    let pole = joint - hip;
    let pole = pole - axis * pole.dot(axis);
    let direction = if pole.length_squared() > 1e-6 {
        pole.normalize()
    } else {
        axis.any_orthonormal_vector()
    };
    Some(hip + axis * along + direction * height)
}
fn rotate_chain(
    bones: &[mdl::skeleton::Bone],
    globals: &mut [Mat4],
    root: usize,
    pivot: Vec3,
    rotation: Quat,
) {
    let transform = Mat4::from_rotation_translation(rotation, pivot - rotation * pivot);
    let mut descendants = vec![false; bones.len()];
    for i in root..bones.len() {
        let parent = bones[i].parent;
        descendants[i] =
            i == root || (parent >= 0 && (parent as usize) < i && descendants[parent as usize]);
        if descendants[i] {
            globals[i] = transform * globals[i];
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ankle_target_preserves_segment_lengths_and_unreachable_targets_stay_finite() {
        let hip = Vec3::new(0., 0., 36.);
        let joint = Vec3::new(4., 0., 18.);
        let goal = Vec3::new(0., 5., 4.);
        let bend = knee_position(hip, joint, goal, 22., 20.).unwrap();
        assert!((bend.distance(hip) - 22.).abs() < 1e-4);
        assert!((bend.distance(goal) - 20.).abs() < 1e-4);
        for goal in [Vec3::new(0., 0., -1000.), hip + Vec3::X * 0.1] {
            assert!(
                knee_position(hip, joint, goal, 22., 20.)
                    .unwrap()
                    .is_finite()
            );
        }
    }
}

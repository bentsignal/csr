use super::*;
use glam::{Quat, Vec3};

fn fixture() -> mdl::Model {
    let pivot = Vec3::new(2., 3., 5.);
    let mut bones = vec![
        mdl::skeleton::Bone {
            name: "root".into(), parent: -1, pos: pivot,
            quat: Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            rot: Vec3::new(std::f32::consts::FRAC_PI_2, 0., 0.),
            pos_scale: Vec3::ONE, rot_scale: Vec3::ONE,
            pose_to_bone: Mat4::IDENTITY, flags: 0,
        },
    ];
    let mut child = bones[0].clone();
    child.name = "unanimated child".into(); child.parent = 0;
    child.pos = Vec3::Y * 7.; child.quat = Quat::IDENTITY; child.rot = Vec3::ZERO;
    bones.push(child);
    let bind = mdl::skeleton::model_space(&bones, &mdl::skeleton::rest_pose(&bones));
    for (bone, matrix) in bones.iter_mut().zip(bind) { bone.pose_to_bone = matrix.inverse(); }

    // Synthetic RAWROT48 + RAWPOS record. Quaternion (.5,.5,.5,.5) is
    // Rz(90) * Rx(90); half-floats (2,3,5) retain the authored pivot.
    let mut raw = vec![0, 0x03, 0, 0];
    for word in [49152u16, 49152, 24576, 0x4000, 0x4200, 0x4500] {
        raw.extend_from_slice(&word.to_le_bytes());
    }
    let desc = mdl::anim::AnimDesc {
        name: "authored base".into(), fps: 30., flags: 0, num_frames: 21,
        anim_block: 0, anim_offset: 0, section_frames: 0, sections: Vec::new(),
    };
    let mut decoy = desc.clone(); decoy.flags = mdl::anim::flags::ALLZEROS;
    mdl::Model {
        bones, raw: std::sync::Arc::new(raw), animations: vec![decoy, desc],
        sequences: vec![mdl::anim::Sequence {
            name: "first, not named idle".into(), activity: String::new(), activity_weight: 0, flags: 0,
            anims: vec![1], group_size: [1, 1], param_index: [-1, -1],
            param_start: [0.; 2], param_end: [0.; 2], fade_in: 0., fade_out: 0.,
            bone_weights: vec![1.; 2], events: Vec::new(),
        }], ..Default::default()
    }
}

fn vertex(position: Vec3) -> mdl::Vertex {
    mdl::Vertex { position, bones: [0, 1, 0], weights: [0.25, 0.75, 0.], bone_count: 2, ..Default::default() }
}

#[test]
fn sequence_zero_geometry_uses_authored_animation_hierarchy_and_inverse_bind() {
    let model = fixture();
    let rest = mdl::skeleton::model_space(&model.bones, &mdl::skeleton::rest_pose(&model.bones));
    let pose = sequence_zero(&model, None).unwrap();
    let pivot = model.bones[0].pos;
    for offset in [Vec3::new(-16., -1., 0.), Vec3::new(16., 1., 32.), Vec3::new(6., -1., 17.)] {
        let v = vertex(pivot + offset);
        assert!((mdl::skeleton::skin_vertex(&v, &rest, &model.bones) - v.position).length() < 1e-4);
        let expected = pivot + Vec3::new(-offset.y, offset.x, offset.z);
        let actual = mdl::skeleton::skin_vertex(&v, &pose, &model.bones);
        assert!((actual - expected).length() < 1e-4, "{actual:?} != {expected:?}");
        assert!((actual - v.position).length() > 1.);
    }
    // Frozen frame zero also resolves the existing external-block/section path.
    let mut external = model.clone();
    external.raw = Default::default(); external.anim_blocks = vec![(0, 0), (0, model.raw.len() as u32)];
    external.animations[1].anim_block = 1;
    external.animations[1].section_frames = 10;
    external.animations[1].sections = vec![(1, 0)];
    assert!(sequence_zero(&external, None).is_err());
    assert_eq!(sequence_zero(&external, Some(&model.raw)).unwrap(), pose);
}

#[test]
fn map_pose_keeps_bevy_world_source_pose_contract_with_scale_and_arbitrary_angles() {
    let model = fixture();
    let pose = sequence_zero(&model, None).unwrap();
    let rotation = mdl::angles_to_mat3(Vec3::new(23., 137., -19.));
    let origin = Vec3::new(127., -83., 41.);
    let world_source = Mat4::from_scale_rotation_translation(Vec3::splat(1.7), Quat::from_mat3(&rotation), origin);
    let world = bevy::math::Mat4::from_scale_rotation_translation(bevy::math::Vec3::splat(1.7),
        crate::coords::rotation_to_bevy(rotation), crate::coords::to_bevy(origin));
    let joints = crate::skinning::pose_transforms(world, &pose);
    let axis = crate::skinning::axis_change();
    let v = vertex(model.bones[0].pos + Vec3::new(16., 1., 32.));
    let expected = crate::coords::to_bevy(world_source.transform_point3(mdl::skeleton::skin_vertex(&v, &pose, &model.bones)));
    let mut actual = bevy::math::Vec3::ZERO;
    for k in 0..v.bone_count as usize {
        let bone = v.bones[k] as usize;
        let inverse = bevy::math::Mat4::from_cols_array(&model.bones[bone].pose_to_bone.to_cols_array());
        let skin = joints[bone].to_matrix() * axis * inverse * axis.inverse();
        actual += skin.transform_point3(crate::coords::to_bevy(v.position)) * v.weights[k];
    }
    assert!((actual - expected).length() < 1e-3, "{actual:?} != {expected:?}");
}

#[test]
fn initial_pose_retains_rest_without_sequences_and_respects_weights_and_rejects_unsupported_bases() {
    let mut model = fixture();
    let rest = mdl::skeleton::model_space(&model.bones, &mdl::skeleton::rest_pose(&model.bones));
    model.sequences[0].bone_weights[0] = 0.;
    assert_eq!(sequence_zero(&model, None).unwrap(), rest);
    model.sequences[0].group_size = [2, 1];
    assert!(sequence_zero(&model, None).is_err());
    model.sequences[0].group_size = [1, 1];
    model.animations[1].flags = mdl::anim::flags::DELTA;
    assert!(sequence_zero(&model, None).is_err());
    model.animations[1].flags = 0;
    model.sequences[0].anims[0] = -1;
    assert!(sequence_zero(&model, None).is_err());
    assert_eq!(initial(&model, &vfs::Vfs::new()), rest, "unsupported machinery retains prior presentation");
    model.sequences.clear();
    assert_eq!(sequence_zero(&model, None).unwrap(), rest);
}

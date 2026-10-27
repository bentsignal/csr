//! Render-only interpolation of completed provider poses. Bone-local rotation
//! interpolation preserves articulated lengths; nothing feeds the simulation.
use glam::Mat4;
use mechanics_link::{Pose, Rig};
fn blend(a: Mat4, b: Mat4, t: f32) -> Mat4 {
    let (sa, qa, pa) = a.to_scale_rotation_translation();
    let (sb, qb, pb) = b.to_scale_rotation_translation();
    Mat4::from_scale_rotation_translation(
        sa.lerp(sb, t),
        qa.normalize().slerp(qb.normalize(), t),
        pa.lerp(pb, t),
    )
}
pub fn fraction(current: &Pose, overstep_seconds: f32) -> f32 {
    if current.period > 0. {
        (current.phase + overstep_seconds / current.period).clamp(0., 1.)
    } else {
        1.
    }
}
pub fn bones(previous: &Pose, current: &Pose, rig: &Rig, overstep_seconds: f32) -> Vec<Mat4> {
    let t = fraction(current, overstep_seconds);
    if previous.bones.len() != current.bones.len()
        || rig.parents.len() != current.bones.len()
        || current.discontinuity
    {
        return current.bones.iter().map(Mat4::from_cols_array_2d).collect();
    }
    let mut out = Vec::with_capacity(current.bones.len());
    for i in 0..current.bones.len() {
        let a = Mat4::from_cols_array_2d(&previous.bones[i]);
        let b = Mat4::from_cols_array_2d(&current.bones[i]);
        let parent = rig.parents[i];
        if parent >= 0 && (parent as usize) < i {
            let parent = parent as usize;
            let pa = Mat4::from_cols_array_2d(&previous.bones[parent]);
            let pb = Mat4::from_cols_array_2d(&current.bones[parent]);
            out.push(out[parent] * blend(pa.inverse() * a, pb.inverse() * b, t));
        } else {
            out.push(blend(a, b, t));
        }
    }
    out
}
pub fn camera(previous: &Pose, current: &Pose, overstep_seconds: f32) -> Option<Mat4> {
    let b = Mat4::from_cols_array_2d(current.camera.as_ref()?);
    let a = previous
        .camera
        .as_ref()
        .map(Mat4::from_cols_array_2d)
        .unwrap_or(b);
    Some(if current.discontinuity {
        b
    } else {
        blend(a, b, fraction(current, overstep_seconds))
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec3};
    #[test]
    fn halfway_turn_keeps_a_child_at_its_authored_length() {
        let a = Mat4::IDENTITY;
        let b = Mat4::from_quat(Quat::from_rotation_z(2.));
        let offset = Mat4::from_translation(Vec3::X);
        let previous = Pose {
            bones: vec![a.to_cols_array_2d(), (a * offset).to_cols_array_2d()],
            ..Default::default()
        };
        let current = Pose {
            bones: vec![b.to_cols_array_2d(), (b * offset).to_cols_array_2d()],
            phase: 0.5,
            period: 1.,
            ..Default::default()
        };
        let rig = Rig {
            names: vec![],
            parents: vec![-1, 0],
            rest: vec![],
            to_source: Mat4::IDENTITY.to_cols_array_2d(),
            bone_basis: Mat4::IDENTITY.to_cols_array_2d(),
        };
        let sampled = bones(&previous, &current, &rig, 0.);
        let child = sampled[1].w_axis.truncate();
        assert!((child.length() - 1.).abs() < 1e-5);
        assert!((child - Vec3::new(1f32.cos(), 1f32.sin(), 0.)).length() < 1e-5);
    }
}

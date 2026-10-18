//! Frozen initial pose for map studio entities, before their rigid entity pose.
//! The skeleton rest pose is not necessarily the authored sequence-zero pose.
//! Keep this opt-in separate from character/viewmodel playback and bind matrices.

use glam::Mat4;
use std::io;

pub(crate) fn initial(model: &mdl::Model, fs: &vfs::Vfs) -> Vec<Mat4> {
    let ani = if model.anim_block_file.is_empty() { None } else { fs.read(&model.anim_block_file) };
    sequence_zero(model, ani.as_deref()).unwrap_or_else(|error| {
        // Preserve the prior presentation for unsupported animation machinery;
        // never turn an otherwise decoded solid map object into a new hole.
        bevy::log::warn!("Map studio initial pose unavailable: {}: {error}; retaining rest pose", model.name);
        mdl::skeleton::model_space(&model.bones, &mdl::skeleton::rest_pose(&model.bones))
    })
}

fn sequence_zero(model: &mdl::Model, ani: Option<&[u8]>) -> io::Result<Vec<Mat4>> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
    let mut local = mdl::skeleton::rest_pose(&model.bones);
    if let Some(sequence) = model.sequences.first() {
        // These rigid map consumers have no pose-parameter/animation controller.
        // Do not silently choose a corner of a blend grid or use a delta as a
        // base pose. Named/default animations remain outside cover admission.
        if sequence.group_size != [1, 1] || sequence.anims.len() != 1
            || sequence.flags & mdl::anim::flags::DELTA != 0
        {
            return Err(invalid("unsupported initial studio sequence"));
        }
        let index = usize::try_from(sequence.anims[0])
            .map_err(|_| invalid("negative initial studio animation"))?;
        let mut desc = model.animations.get(index)
            .ok_or_else(|| invalid("missing initial studio animation"))?.clone();
        // Reuse the existing section/block/compression reader, decoding only
        // frame zero. No clip or animation clock survives asset initialization.
        desc.num_frames = 1;
        let clip = mdl::anim::decode_clip(&model.raw, ani, &model.anim_blocks, &desc, &model.bones)?;
        if clip.delta || (0..model.bones.len()).any(|bone| {
            let weight = sequence.weight(bone);
            !weight.is_finite() || !(0.0..=1.0).contains(&weight)
        }) {
            return Err(invalid("invalid initial studio base pose"));
        }
        clip.sample_layered(0.0, &mut local, |bone| sequence.weight(bone));
    }
    let pose = mdl::skeleton::model_space(&model.bones, &local);
    if pose.iter().any(|matrix| !matrix.is_finite()) {
        return Err(invalid("non-finite initial studio pose"));
    }
    Ok(pose)
}

#[cfg(test)]
mod tests;

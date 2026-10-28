//! The installed CS arm posed around an imported board rail.
use super::*;
impl ViewModels {
    /// Use the installed CS hands and a closed weapon grip for the crossover
    /// equipment pose. Original arm geometry/animations never enter Git.
    pub(crate) fn board_grip(
        &self,
        team: sim::Team,
        meshes: &mut Assets<Mesh>,
    ) -> Option<(SkinnedAsset, crate::board_presentation::Grip)> {
        let arms = match team {
            sim::Team::Terrorist => self.arms_t.as_ref(),
            _ => self.arms_ct.as_ref(),
        }?;
        let view = self.weapons.get("weapon_knife")?;
        let mut pose = mdl::skeleton::rest_pose(&view.asset.model.bones);
        if let Some(seq) = view.clips.get("ACT_VM_IDLE").and_then(|c| c.first()) {
            sequence_pose::sample(&seq.clip, 0., &mut pose, |b| {
                seq.weights.get(b).copied().unwrap_or(1.)
            });
        }
        let world = mdl::skeleton::model_space(&view.asset.model.bones, &pose);
        let mut pose = skinning::bone_merge(&arms.model.bones, &view.asset.model.bones, &world);
        // Knife clips may omit the upper arm. Keep its authored relationship
        // to the animated forearm rather than mixing a rest-space shoulder
        // with a weapon-space wrist (which produces a stretched sleeve).
        let rest = mdl::skeleton::model_space(
            &arms.model.bones,
            &mdl::skeleton::rest_pose(&arms.model.bones),
        );
        if let Some(elbow) = arms
            .model
            .bones
            .iter()
            .position(|b| b.name.to_ascii_lowercase().contains("r_forearm"))
        {
            let shoulder = arms.model.bones[elbow].parent.max(0) as usize;
            pose[shoulder] = pose[elbow] * rest[elbow].inverse() * rest[shoulder];
        }
        if std::env::var_os("CSR_BOARD_GRIP_TRACE").is_some() {
            for (i, b) in arms.model.bones.iter().enumerate().filter(|(_, b)| {
                b.name.to_lowercase().contains("_r_") || b.name.to_lowercase().contains("right")
            }) {
                info!(
                    "BOARD_GRIP bone={i} name={} parent={} position={:?}",
                    b.name,
                    b.parent,
                    pose[i].w_axis.truncate()
                );
            }
        }
        let hand = arms.model.bones.iter().position(|b| {
            b.name.to_ascii_lowercase().ends_with("r_hand")
                || b.name.to_ascii_lowercase().contains("right_hand")
        })?;
        let mut asset = arms.clone();
        // Retain just the gripping arm, including its original weighted fingers
        // and sleeve. A knife's unused off-hand otherwise floats across view.
        let right: Vec<_> = arms
            .model
            .bones
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let mut at = i as i32;
                while at >= 0 {
                    let bone = &arms.model.bones[at as usize];
                    let name = bone.name.to_ascii_lowercase();
                    if name.contains("r_upperarm") {
                        return true;
                    }
                    at = bone.parent;
                }
                false
            })
            .collect();
        asset.parts = arms
            .model
            .meshes
            .iter()
            .zip(&arms.parts)
            .filter_map(|(source, (_, material))| {
                let on_right = |i: usize| {
                    let v = &source.vertices[i];
                    (0..v.bone_count.clamp(1, 3) as usize)
                        .filter(|j| right.get(v.bones[*j] as usize).copied().unwrap_or(false))
                        .map(|j| v.weights[j])
                        .sum::<f32>()
                        > 0.5
                };
                let mut indices: Vec<_> = source
                    .indices
                    .chunks_exact(3)
                    .filter(|t| t.iter().all(|i| on_right(*i as usize)))
                    .flatten()
                    .copied()
                    .collect();
                if indices.is_empty() {
                    return None;
                }
                if crate::props::needs_flip(source) {
                    for t in indices.chunks_exact_mut(3) {
                        t.swap(1, 2);
                    }
                }
                let mut mesh = Mesh::new(
                    bevy::mesh::PrimitiveTopology::TriangleList,
                    bevy::asset::RenderAssetUsages::default(),
                );
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_POSITION,
                    source
                        .vertices
                        .iter()
                        .map(|v| to_bevy(v.position).to_array())
                        .collect::<Vec<_>>(),
                );
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_NORMAL,
                    source
                        .vertices
                        .iter()
                        .map(|v| to_bevy(v.normal).to_array())
                        .collect::<Vec<_>>(),
                );
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_UV_0,
                    source
                        .vertices
                        .iter()
                        .map(|v| v.uv.to_array())
                        .collect::<Vec<_>>(),
                );
                let mut joints = vec![];
                let mut weights = vec![];
                for v in &source.vertices {
                    let mut js = [0u16; 4];
                    let mut ws = [0f32; 4];
                    for i in 0..v.bone_count.clamp(1, 3) as usize {
                        js[i] = v.bones[i] as u16;
                        ws[i] = v.weights[i].max(0.);
                    }
                    let sum = ws.iter().sum::<f32>();
                    if sum > 1e-6 {
                        ws.iter_mut().for_each(|w| *w /= sum);
                    } else {
                        ws[0] = 1.;
                    }
                    joints.push(js);
                    weights.push(ws);
                }
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_JOINT_INDEX,
                    bevy::mesh::VertexAttributeValues::Uint16x4(joints),
                );
                mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights);
                mesh.insert_indices(bevy::mesh::Indices::U32(indices));
                Some((meshes.add(mesh), material.clone()))
            })
            .collect();
        let elbow = arms
            .model
            .bones
            .iter()
            .position(|b| {
                let n = b.name.to_ascii_lowercase();
                n.contains("r_forearm") || n.contains("right_forearm")
            })
            .unwrap_or_else(|| arms.model.bones[hand].parent.max(0) as usize);
        let shoulder = arms.model.bones[elbow].parent.max(0) as usize;
        let finger = |suffix: &str| {
            arms.model
                .bones
                .iter()
                .position(|b| b.name.to_ascii_lowercase().ends_with(suffix))
        };
        let index = finger("r_finger1")?;
        let pinky = finger("r_finger4")?;
        let across = (pose[pinky].w_axis - pose[index].w_axis)
            .truncate()
            .normalize();
        let knuckles = (pose[index].w_axis + pose[pinky].w_axis).truncate() * 0.5;
        let forward = (knuckles - pose[hand].w_axis.truncate()).normalize();
        let forward = (forward - across * forward.dot(across)).normalize();
        let normal = across.cross(forward).normalize();
        let mut center = glam::Vec3::ZERO;
        for suffix in [
            "r_finger1",
            "r_finger12",
            "r_finger2",
            "r_finger22",
            "r_finger3",
            "r_finger32",
            "r_finger4",
            "r_finger42",
        ] {
            center += pose[finger(suffix)?].w_axis.truncate() / 8.;
        }
        let frame = glam::Mat4::from_cols(
            forward.extend(0.),
            normal.extend(0.),
            across.extend(0.),
            center.extend(1.),
        );
        let contact = pose[hand].inverse() * frame;
        Some((
            asset,
            crate::board_presentation::Grip {
                pose,
                parents: arms.model.bones.iter().map(|b| b.parent).collect(),
                twists: arms
                    .model
                    .bones
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.name.to_ascii_lowercase().contains("r_foretwist"))
                    .map(|(i, _)| i)
                    .collect(),
                hand,
                elbow,
                shoulder,
                contact,
            },
        ))
    }
}

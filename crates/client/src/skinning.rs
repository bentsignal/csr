//! GPU-skinned studio models.
//!
//! A [`SkinnedAsset`] is a model's meshes with joint indices/weights, its
//! skeleton and inverse bind poses. Instances spawn one joint entity per
//! bone; every frame [`apply_pose`] writes each joint's world transform:
//!
//! `joint = world_from_model · S · bone_to_model · S⁻¹`
//!
//! where `S` is the Source→Bevy axis change. Mesh vertices are stored as
//! `S · v` and inverse bind poses as `S · pose_to_bone · S⁻¹`, so skinning
//! reproduces `world_from_model · S · bone_to_model · pose_to_bone · v`.
//!
//! Bone-merging (viewmodel arms onto the gun, a rifle into a player's hand)
//! copies matching bones' model-space matrices by name from a parent pose.

use crate::coords::to_bevy;
use crate::material::WorldMaterial;
use crate::textures::TextureCache;
use crate::world_mesh::{material_from_vmt, SharedHandles};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::math::Mat4;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use mdl::skeleton::Bone;
use std::sync::Arc;

/// Source → Bevy axis change as a matrix: (x, y, z) → (x, z, -y).
pub fn axis_change() -> Mat4 {
    Mat4::from_cols(
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, -1.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    )
}

fn to_bevy_mat(m: glam::Mat4) -> Mat4 {
    Mat4::from_cols_array(&m.to_cols_array())
}

#[derive(Clone)]
pub struct SkinnedAsset {
    pub model: Arc<mdl::Model>,
    pub parts: Vec<(Handle<Mesh>, Handle<WorldMaterial>)>,
    pub inverse_bindposes: Handle<SkinnedMeshInverseBindposes>,
    pub(crate) culling: Vec<Option<Arc<crate::skinning_bounds::Certificate>>>,
}

/// The default imported bodygroup contains the authored suppressor geometry.
/// An absent suppressor selects its empty sibling by hiding only that bodypart.
pub fn suppressor_mesh_hidden(model: &mdl::Model, mesh: usize, visible: bool) -> bool {
    !visible && model.meshes.get(mesh).and_then(|m| m.hardware)
        .and_then(|h| model.body_parts.get(h.body_part as usize))
        .is_some_and(|(name, _, _)| name.eq_ignore_ascii_case("silencer"))
}

/// One spawned copy of a [`SkinnedAsset`].
#[derive(Clone, Debug)]
pub struct SkinnedInstance {
    pub joints: Vec<Entity>,
    pub meshes: Vec<Entity>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_asset(
    model: Arc<mdl::Model>,
    fs: &vfs::Vfs,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WorldMaterial>,
    images: &mut Assets<Image>,
    textures: &mut TextureCache,
    shared: &SharedHandles,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    exposure: f32,
    dynamic_lit: bool,
) -> SkinnedAsset {
    let read = |p: &str| fs.read_string(p);
    let s = axis_change();
    let s_inv = s.inverse();
    let inverse: Vec<Mat4> = model.bones.iter().map(|b| s * to_bevy_mat(b.pose_to_bone) * s_inv).collect();
    let mut culling = Vec::with_capacity(model.meshes.len());
    let joint_limit = model.bones.len().saturating_sub(1).min(255) as u16;
    // Body parts often share a skin texture. Resolve/convert it once in this
    // exact model/VFS/exposure scope, including the missing-VMT fallback.
    // Keep values rather than asset handles: every part still owns a separate
    // material for later lighting, finish and suppressor changes.
    let mut prepared_materials = std::collections::HashMap::new();
    let parts = model
        .meshes
        .iter()
        .map(|mesh| {
            let positions: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| to_bevy(v.position).to_array()).collect();
            let normals: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| to_bevy(v.normal).to_array()).collect();
            let uvs: Vec<[f32; 2]> = mesh.vertices.iter().map(|v| v.uv.to_array()).collect();
            let mut joints = Vec::with_capacity(mesh.vertices.len());
            let mut weights = Vec::with_capacity(mesh.vertices.len());
            for v in &mesh.vertices {
                let n = v.bone_count.clamp(1, 3) as usize;
                let mut j = [0u16; 4];
                let mut w = [0.0f32; 4];
                let mut sum = 0.0;
                for k in 0..n {
                    j[k] = (v.bones[k] as u16).min(joint_limit);
                    w[k] = v.weights[k].max(0.0);
                    sum += w[k];
                }
                if sum <= 1e-6 {
                    w = [1.0, 0.0, 0.0, 0.0];
                } else {
                    for x in w.iter_mut() {
                        *x /= sum;
                    }
                }
                joints.push(j);
                weights.push(w);
            }
            // Wind counter-clockwise around the vertex normals.
            let mut indices = mesh.indices.clone();
            if crate::props::needs_flip(mesh) {
                for t in indices.chunks_exact_mut(3) {
                    t.swap(1, 2);
                }
            }
            let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            m.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(joints));
            m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights);
            m.insert_indices(Indices::U32(indices));
            if let Some(tangents) = mesh.vertices.iter().map(|v| v.tangent).collect::<Option<Vec<_>>>() {
                m.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents.into_iter().map(|t| {
                    let xyz = to_bevy(glam::Vec3::new(t[0],t[1],t[2])); [xyz.x,xyz.y,xyz.z,t[3]]
                }).collect::<Vec<_>>());
            } else if let Err(e) = m.generate_tangents() { warn!("model tangents: {e}"); }
            let tex = model.skin_texture(0, mesh.material);
            let mat = prepared_materials.entry(tex).or_insert_with(|| {
                let vmt = model
                    .material_candidates(tex)
                    .into_iter()
                    .find_map(|c| vmt::Material::load(&c, &read));
                let mut mat = material_from_vmt(vmt.as_ref(), fs, textures, images, shared, exposure, false);
                if dynamic_lit {
                    mat.params.flags = (mat.params.flags & !crate::material::flags::VERTEX_LIT) | crate::material::flags::DYNAMIC_LIT;
                    // Dynamic Phong/rim/emission composition is unvalidated;
                    // preserve existing weapon/character presentation this pass.
                    mat.params.self_illum = Vec4::ZERO;
                }
                mat
            }).clone();
            culling.push(crate::skinning_bounds::certify(&mut m, &inverse));
            (meshes.add(m), materials.add(mat))
        })
        .collect();
    SkinnedAsset {
        model,
        parts,
        inverse_bindposes: bindposes.add(SkinnedMeshInverseBindposes::from(inverse)),
        culling,
    }
}

/// Spawns joints and skinned meshes. `own_materials` clones each material
/// so the instance can be lit independently.
pub fn spawn(
    commands: &mut Commands,
    asset: &SkinnedAsset,
    layer: Option<RenderLayers>,
    materials: Option<&mut Assets<WorldMaterial>>,
) -> SkinnedInstance {
    spawn_inner(commands, asset, layer, materials, None)
}

/// Explicit opt-in for world bodies, held weapons and corpses. Other callers
/// keep the original uncullable behavior regardless of their render layer.
pub(crate) fn spawn_world(
    commands: &mut Commands,
    asset: &SkinnedAsset,
    materials: Option<&mut Assets<WorldMaterial>>,
    kind: crate::skinning_bounds::WorldSkin,
) -> SkinnedInstance {
    spawn_inner(commands, asset, None, materials, Some(kind))
}

fn spawn_inner(
    commands: &mut Commands,
    asset: &SkinnedAsset,
    layer: Option<RenderLayers>,
    materials: Option<&mut Assets<WorldMaterial>>,
    world: Option<crate::skinning_bounds::WorldSkin>,
) -> SkinnedInstance {
    let joints: Vec<Entity> = asset
        .model
        .bones
        .iter()
        .map(|_| commands.spawn((Transform::default(), Visibility::default())).id())
        .collect();
    let mut materials = materials;
    let meshes = asset
        .parts
        .iter()
        .enumerate()
        .map(|(index, (mesh, mat))| {
            let original = mat.clone();
            let mat = match materials.as_deref_mut() {
                Some(store) => {
                    let copy = store.get(mat).cloned();
                    copy.map_or_else(|| mat.clone(), |m| store.add(m))
                }
                None => mat.clone(),
            };
            let mut e = commands.spawn((
                crate::skin_finish::SkinPart { material: asset.model.textures.get(asset.model.skin_texture(0,asset.model.meshes[index].material)).cloned().unwrap_or_default(), original, applied: None },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat),
                SkinnedMesh {
                    inverse_bindposes: asset.inverse_bindposes.clone(),
                    joints: joints.clone(),
                },
                Transform::default(),
                NoFrustumCulling,
            ));
            if let Some(l) = &layer {
                e.insert(l.clone());
            }
            if let Some(kind) = world
                && let Some(certificate) = asset.culling.get(index)
            {
                crate::skinning_bounds::enable(&mut e, mesh, certificate, kind);
            }
            e.id()
        })
        .collect();
    SkinnedInstance { joints, meshes }
}

/// Model-space bone matrices (Source space) for a child skeleton merged
/// onto a parent pose: bones with a parent-skeleton namesake copy its
/// matrix, the rest follow their own parent with their rest transform.
pub fn bone_merge(child: &[Bone], parent_bones: &[Bone], parent_pose: &[glam::Mat4]) -> Vec<glam::Mat4> {
    let mut out: Vec<glam::Mat4> = Vec::with_capacity(child.len());
    for b in child {
        let from_parent = parent_bones
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(&b.name))
            .and_then(|i| parent_pose.get(i).copied());
        let m = match from_parent {
            Some(m) => m,
            None => {
                let local = glam::Mat4::from_rotation_translation(b.quat, b.pos);
                if b.parent >= 0 { out[b.parent as usize] * local } else { local }
            }
        };
        out.push(m);
    }
    out
}

/// A bone merge for one immutable child/parent skeleton pair. Compile when an
/// instance is equipped, never from an animation pose: names, first-match
/// indices and child rest transforms stay fixed while parent matrices change.
pub(crate) struct PreparedBoneMerge {
    bones: Vec<MergeBone>,
}

struct MergeBone {
    source: Option<usize>,
    parent: i32,
    pos: glam::Vec3,
    quat: glam::Quat,
}

impl PreparedBoneMerge {
    pub(crate) fn new(child: &[Bone], parent: &[Bone]) -> Self {
        Self {
            bones: child.iter().map(|bone| MergeBone {
                // Preserve bone_merge's first ASCII-insensitive match, even
                // for duplicate names or a pose shorter than the skeleton.
                source: parent.iter().position(|p| p.name.eq_ignore_ascii_case(&bone.name)),
                parent: bone.parent,
                pos: bone.pos,
                quat: bone.quat,
            }).collect(),
        }
    }

    /// Same arithmetic and missing-source fallback as bone_merge. The caller
    /// owns the output so steady-state evaluation needs no allocation or name
    /// search. Skeletons retain the importer's parent-before-child invariant.
    pub(crate) fn evaluate_into(&self, parent_pose: &[glam::Mat4], out: &mut Vec<glam::Mat4>) {
        out.clear();
        for bone in &self.bones {
            let m = match bone.source.and_then(|i| parent_pose.get(i).copied()) {
                Some(m) => m,
                None => {
                    // Keep construction lazy, as in bone_merge: matched bones
                    // never evaluate their unused rest quaternion.
                    let local = glam::Mat4::from_rotation_translation(bone.quat, bone.pos);
                    if bone.parent >= 0 { out[bone.parent as usize] * local } else { local }
                }
            };
            out.push(m);
        }
    }
}

/// Writes joint transforms for a pose. `world_from_model` is in Bevy
/// space; `pose` holds Source-space bone-to-model matrices.
pub fn apply_pose(instance: &SkinnedInstance, world_from_model: Mat4, pose: &[glam::Mat4], transforms: &mut Query<&mut Transform>) {
    let s = axis_change();
    let s_inv = s.inverse();
    for (joint, m) in instance.joints.iter().zip(pose) {
        if let Ok(mut t) = transforms.get_mut(*joint) {
            let world = world_from_model * s * to_bevy_mat(*m) * s_inv;
            *t = Transform::from_matrix(world);
        }
    }
}

/// Joint transforms for a pose, for instances whose joints were just
/// spawned (and so cannot be queried yet).
pub fn pose_transforms(world_from_model: Mat4, pose: &[glam::Mat4]) -> Vec<Transform> {
    let s = axis_change();
    let s_inv = s.inverse();
    pose.iter()
        .map(|m| Transform::from_matrix(world_from_model * s * to_bevy_mat(*m) * s_inv))
        .collect()
}

/// Spawns an instance frozen in a pose (props on the ground).
pub fn spawn_posed(
    commands: &mut Commands,
    asset: &SkinnedAsset,
    materials: &mut Assets<WorldMaterial>,
    world_from_model: Mat4,
) -> SkinnedInstance {
    let instance = spawn(commands, asset, None, Some(materials));
    let pose = mdl::skeleton::model_space(&asset.model.bones, &mdl::skeleton::rest_pose(&asset.model.bones));
    for (joint, t) in instance.joints.iter().zip(pose_transforms(world_from_model, &pose)) {
        commands.entity(*joint).insert(t);
    }
    instance
}

#[cfg(test)]
mod material_preparation_tests {
    use super::*;

    #[test]
    fn repeated_skin_materials_match_serial_import_and_keep_part_and_vfs_scope() {
        let mesh = mdl::Mesh {
            vertices: [glam::Vec3::ZERO, glam::Vec3::X, glam::Vec3::Y].map(|position| mdl::Vertex {
                position, normal: glam::Vec3::Z, tangent: Some([1., 0., 0., 1.]),
                ..Default::default()
            }).to_vec(),
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        let model = Arc::new(mdl::Model {
            textures: vec!["paint".into(), "absent".into()],
            cd_textures: vec!["preferred/".into(), "fallback/".into()],
            // Different material slots alias one resolved texture. Missing
            // declarations must also retain their original fallback.
            skins: vec![vec![0, 0, 1, 1]],
            meshes: (0..4).map(|material| mdl::Mesh { material, ..mesh.clone() }).collect(),
            ..Default::default()
        });
        let mut images = Assets::<Image>::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<WorldMaterial>::default();
        let mut bindposes = Assets::<SkinnedMeshInverseBindposes>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        // Reuse all asset stores across successive imports, just as callers
        // do. The same material names must still use each selected VFS and
        // the current import's exposure/dynamic-lighting policy.
        for (color, exposure, dynamic_lit) in [("[.25 .5 .75]", 1., true), ("[.75 .5 .25]", 3., false)] {
            let mut fs = vfs::Vfs::new();
            fs.push_back("fixture", vfs::Mount::Memory(std::collections::BTreeMap::from([
                ("materials/preferred/paint.vmt".into(),
                    format!(r#"Patch {{ include "materials/base.vmt" insert {{ "$color" "{color}" }} }}"#).into_bytes().into()),
                ("materials/base.vmt".into(),
                    br#"VertexLitGeneric { "$basetexture" "missing/base" "$phong" "1" "$nocull" "1" }"#.to_vec().into()),
                ("materials/fallback/paint.vmt".into(),
                    br#"UnlitGeneric { "$color" "[1 0 0]" }"#.to_vec().into()),
            ])));
            let asset = build_asset(model.clone(), &fs, &mut meshes, &mut materials,
                &mut images, &mut textures, &shared, &mut bindposes, exposure, dynamic_lit);
            assert_eq!(asset.parts.len(), 4);
            let expected_color = if dynamic_lit { Vec4::new(0.25, 0.5, 0.75, 1.) }
                else { Vec4::new(0.75, 0.5, 0.25, 1.) };
            assert_eq!(materials.get(&asset.parts[0].1).unwrap().params.color, expected_color);
            assert_eq!(materials.get(&asset.parts[2].1).unwrap().base, textures.black);
            for (index, (_, handle)) in asset.parts.iter().enumerate() {
                assert!(asset.parts[..index].iter().all(|(_, earlier)| earlier != handle));
                // Original serial material path, independent of the reuse map.
                let tex = model.skin_texture(0, model.meshes[index].material);
                let source = model.material_candidates(tex).into_iter()
                    .find_map(|path| vmt::Material::load(&path, &|p| fs.read_string(p)));
                let mut expected = material_from_vmt(source.as_ref(), &fs, &mut textures,
                    &mut images, &shared, exposure, false);
                if dynamic_lit {
                    expected.params.flags = (expected.params.flags & !crate::material::flags::VERTEX_LIT)
                        | crate::material::flags::DYNAMIC_LIT;
                    expected.params.self_illum = Vec4::ZERO;
                }
                let actual = materials.get(handle).unwrap();
                let gpu_bytes = |material: &WorldMaterial| {
                    let mut buffer = bevy::render::render_resource::encase::UniformBuffer::new(Vec::new());
                    buffer.write(&material.params).unwrap();
                    buffer.into_inner()
                };
                assert_eq!(gpu_bytes(actual), gpu_bytes(&expected));
                // Compare every binding, alpha/cull policy and CPU field too.
                assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
            }
            let first = &asset.parts[0].1;
            let second = &asset.parts[1].1;
            let before = materials.get(second).unwrap().params.color;
            materials.get_mut(first).unwrap().params.color = Vec4::ZERO;
            assert_eq!(materials.get(second).unwrap().params.color, before);
        }
    }
}

#[cfg(test)]
mod prepared_merge_tests {
    use super::*;
    use glam::{Mat4 as SourceMat4, Quat, Vec3 as SourceVec3};

    fn bone(name: &str, parent: i32, pos: SourceVec3, quat: Quat) -> Bone {
        Bone {
            name: name.into(), parent, pos, quat,
            rot: SourceVec3::ZERO, pos_scale: SourceVec3::ONE, rot_scale: SourceVec3::ONE,
            pose_to_bone: SourceMat4::IDENTITY, flags: 0,
        }
    }

    fn assert_pose_bits(actual: &[SourceMat4], expected: &[SourceMat4]) {
        assert_eq!(actual.len(), expected.len());
        for (index, (a, b)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(a.to_cols_array().map(f32::to_bits), b.to_cols_array().map(f32::to_bits),
                "bone {index}");
        }
    }

    #[test]
    fn prepared_merge_preserves_first_match_and_authored_fallbacks() {
        let child = [
            bone("root", -1, SourceVec3::X, Quat::IDENTITY),
            bone("HAND", 0, SourceVec3::Y * 2., Quat::IDENTITY),
            bone("unbound_tip", 1, SourceVec3::Z * 3., Quat::IDENTITY),
            bone("unbound_root", -1, SourceVec3::X * 7., Quat::IDENTITY),
            bone("Ä", 3, SourceVec3::Y * 5., Quat::IDENTITY),
        ];
        let parent = [
            bone("hand", -1, SourceVec3::ZERO, Quat::IDENTITY),
            bone("Hand", 0, SourceVec3::ZERO, Quat::IDENTITY),
            bone("ROOT", 0, SourceVec3::ZERO, Quat::IDENTITY),
            bone("ä", 0, SourceVec3::ZERO, Quat::IDENTITY),
        ];
        let pose = [10., 99., 20., 88.].map(|x| SourceMat4::from_translation(SourceVec3::X * x));
        let merge = PreparedBoneMerge::new(&child, &parent);
        let mut out = Vec::new();
        merge.evaluate_into(&pose, &mut out);
        // Literal expectations distinguish first-match selection, direct
        // model-space copying, chained rest transforms and ASCII-only casing.
        let expected = [
            SourceVec3::new(20., 0., 0.), SourceVec3::new(10., 0., 0.),
            SourceVec3::new(10., 0., 3.), SourceVec3::new(7., 0., 0.),
            SourceVec3::new(7., 5., 0.),
        ].map(SourceMat4::from_translation);
        assert_pose_bits(&out, &expected);
        assert_pose_bits(&out, &bone_merge(&child, &parent, &pose));

        // A missing first-match matrix falls back to the child's own rest
        // hierarchy; it must never search for a different matching bone.
        for len in 0..=pose.len() {
            merge.evaluate_into(&pose[..len], &mut out);
            assert_pose_bits(&out, &bone_merge(&child, &parent, &pose[..len]));
        }
        let missing_first = [
            bone("other", -1, SourceVec3::ZERO, Quat::IDENTITY),
            bone("root", 0, SourceVec3::ZERO, Quat::IDENTITY),
            bone("ROOT", 0, SourceVec3::ZERO, Quat::IDENTITY),
        ];
        PreparedBoneMerge::new(&child, &missing_first).evaluate_into(&pose[..1], &mut out);
        assert_pose_bits(&out, &bone_merge(&child, &missing_first, &pose[..1]));
        assert_eq!(out[0], SourceMat4::from_translation(SourceVec3::X));
    }

    #[test]
    fn prepared_merge_tracks_animated_parent_without_reallocating() {
        let child = [
            bone("root", -1, SourceVec3::X, Quat::from_rotation_z(0.3)),
            bone("hand", 0, SourceVec3::Y, Quat::from_rotation_x(-0.2)),
            bone("accessory", 1, SourceVec3::Z, Quat::from_rotation_y(0.7)),
        ];
        // The parent order deliberately differs from the child's. The map is
        // independent of current matrices, including non-rigid transforms.
        let parent = [child[1].clone(), child[0].clone()];
        let merge = PreparedBoneMerge::new(&child, &parent);
        let mut out = Vec::with_capacity(child.len());
        let allocation = out.as_ptr();
        let capacity = out.capacity();
        for frame in 0..256 {
            let t = frame as f32 * 0.03125;
            let pose = [
                SourceMat4::from_scale_rotation_translation(SourceVec3::new(1., 0.75, 1.25),
                    Quat::from_rotation_z(t), SourceVec3::new(t, -t, t * 0.5)),
                SourceMat4::from_rotation_translation(Quat::from_rotation_x(-t), SourceVec3::X * t),
            ];
            merge.evaluate_into(&pose, &mut out);
            assert_pose_bits(&out, &bone_merge(&child, &parent, &pose));
            assert_eq!(out.as_ptr(), allocation);
            assert_eq!(out.capacity(), capacity);
        }
        // No new finite-value policy: preserve the original's propagation.
        let nonfinite = [SourceMat4::from_cols_array(&[f32::NAN; 16])];
        merge.evaluate_into(&nonfinite, &mut out);
        assert_pose_bits(&out, &bone_merge(&child, &parent, &nonfinite));
    }

    #[test]
    fn prepared_merge_replacement_uses_new_team_and_weapon_skeletons() {
        let weapon_a = [bone("hand", -1, SourceVec3::X, Quat::IDENTITY)];
        let weapon_b = [
            bone("root", -1, SourceVec3::Y, Quat::IDENTITY),
            bone("hand", 0, SourceVec3::Z, Quat::IDENTITY),
        ];
        let team_a = [weapon_a[0].clone(), weapon_b[0].clone()];
        let team_b = [weapon_b[0].clone(), weapon_a[0].clone()];
        let pose = [SourceVec3::X * 10., SourceVec3::Y * 20.].map(SourceMat4::from_translation);
        let mut out = Vec::new();
        for (child, parent) in [(&weapon_a[..], &team_a[..]), (&weapon_b[..], &team_a[..]),
            (&weapon_b[..], &team_b[..]), (&[][..], &team_b[..]), (&weapon_a[..], &[][..])] {
            // Same output buffer across equip/team replacements and empty
            // skeletons: no stale trailing matrices or cached source indices.
            let merge = PreparedBoneMerge::new(child, parent);
            merge.evaluate_into(&pose, &mut out);
            assert_pose_bits(&out, &bone_merge(child, parent, &pose));
        }
    }
}

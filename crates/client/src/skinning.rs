//! GPU-skinned studio models.
//!
//! A [`SkinnedAsset`] is a model's meshes with joint indices/weights, its
//! skeleton and inverse bind poses. Instances spawn one joint entity per
//! bone; every frame [`apply_pose`] computes each joint's world transform:
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
    let joint_limit = model.bones.len().saturating_sub(1).min(255) as u16;
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
            let vmt = model
                .material_candidates(tex)
                .into_iter()
                .find_map(|c| vmt::Material::load(&c, &read));
            let mut mat = material_from_vmt(vmt.as_ref(), fs, textures, images, shared, exposure, false);
            if dynamic_lit {
                mat.params.flags = (mat.params.flags & !crate::material::flags::VERTEX_LIT) | crate::material::flags::DYNAMIC_LIT;
            }
            (meshes.add(m), materials.add(mat))
        })
        .collect();
    let s = axis_change();
    let s_inv = s.inverse();
    let inverse: Vec<Mat4> = model.bones.iter().map(|b| s * to_bevy_mat(b.pose_to_bone) * s_inv).collect();
    SkinnedAsset {
        model,
        parts,
        inverse_bindposes: bindposes.add(SkinnedMeshInverseBindposes::from(inverse)),
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

/// Writes joint transforms for a pose. `world_from_model` is in Bevy
/// space; `pose` holds Source-space bone-to-model matrices.
pub fn apply_pose(instance: &SkinnedInstance, world_from_model: Mat4, pose: &[glam::Mat4], transforms: &mut Query<&mut Transform>) {
    let s = axis_change();
    let s_inv = s.inverse();
    for (joint, m) in instance.joints.iter().zip(pose) {
        if let Ok(mut t) = transforms.get_mut(*joint) {
            let world = world_from_model * s * to_bevy_mat(*m) * s_inv;
            let next = Transform::from_matrix(world);
            // Preserve every decomposed float bit, including signed zero.
            // Read without mutably dereferencing a settled joint.
            if !same_transform_bits(&t, &next) {
                *t = next;
            }
        }
    }
}

fn same_transform_bits(a: &Transform, b: &Transform) -> bool {
    a.translation.to_array().map(f32::to_bits) == b.translation.to_array().map(f32::to_bits)
        && a.rotation.to_array().map(f32::to_bits) == b.rotation.to_array().map(f32::to_bits)
        && a.scale.to_array().map(f32::to_bits) == b.scale.to_array().map(f32::to_bits)
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
mod tests {
    use super::*;

    #[derive(Component)]
    struct Joint;

    #[derive(Resource)]
    struct PoseFixture {
        instance: SkinnedInstance,
        world_from_model: Mat4,
        pose: Vec<glam::Mat4>,
    }

    #[derive(Resource, Default)]
    struct PoseChanges {
        local: Vec<Entity>,
        global: Vec<Entity>,
    }

    fn apply_fixture(fixture: Res<PoseFixture>, mut transforms: Query<&mut Transform>) {
        apply_pose(
            &fixture.instance,
            fixture.world_from_model,
            &fixture.pose,
            &mut transforms,
        );
    }

    fn record_pose_changes(
        local: Query<Entity, (With<Joint>, Changed<Transform>)>,
        global: Query<Entity, (With<Joint>, Changed<GlobalTransform>)>,
        mut seen: ResMut<PoseChanges>,
    ) {
        seen.local = local.iter().collect();
        seen.global = global.iter().collect();
    }

    fn fixture_app() -> (App, Entity, Entity) {
        bevy::tasks::ComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let mut app = App::new();
        app.add_plugins(bevy::transform::TransformPlugin)
            .init_resource::<PoseChanges>()
            .add_systems(Update, apply_fixture)
            .add_systems(Last, record_pose_changes);
        let a = app.world_mut().spawn((Joint, Transform::default())).id();
        let b = app.world_mut().spawn((Joint, Transform::default())).id();
        app.insert_resource(PoseFixture {
            instance: SkinnedInstance {
                joints: vec![a, b],
                meshes: Vec::new(),
            },
            world_from_model: Mat4::IDENTITY,
            pose: vec![
                glam::Mat4::from_rotation_translation(
                    glam::Quat::from_rotation_z(0.3),
                    glam::Vec3::new(1.0, 2.0, 3.0),
                ),
                glam::Mat4::from_translation(glam::Vec3::new(4.0, 5.0, 6.0)),
            ],
        });
        (app, a, b)
    }

    fn expect_changes(app: &App, expected: &[Entity]) {
        let seen = app.world().resource::<PoseChanges>();
        for changed in [&seen.local, &seen.global] {
            assert_eq!(changed.len(), expected.len(), "changed joints: {changed:?}");
            assert!(
                expected.iter().all(|entity| changed.contains(entity)),
                "changed joints: {changed:?}"
            );
        }
    }

    fn expect_pose(app: &App) {
        let fixture = app.world().resource::<PoseFixture>();
        // The existing spawn-time path remains our reference for the precise
        // matrix multiplication and decomposition, independent of the guard.
        let expected = pose_transforms(fixture.world_from_model, &fixture.pose);
        for (joint, expected) in fixture.instance.joints.iter().zip(expected) {
            let Some(actual) = app.world().get::<Transform>(*joint) else {
                continue;
            };
            assert_eq!(
                actual.translation.to_array().map(f32::to_bits),
                expected.translation.to_array().map(f32::to_bits)
            );
            assert_eq!(
                actual.rotation.to_array().map(f32::to_bits),
                expected.rotation.to_array().map(f32::to_bits)
            );
            assert_eq!(
                actual.scale.to_array().map(f32::to_bits),
                expected.scale.to_array().map(f32::to_bits)
            );
            assert_eq!(
                *app.world().get::<GlobalTransform>(*joint).unwrap(),
                GlobalTransform::from(expected)
            );
        }
    }

    #[test]
    fn real_bevy_pose_settles_and_exact_changes_reach_global_transforms() {
        let (mut app, a, b) = fixture_app();
        let child_transform = Transform::from_xyz(2.0, 3.0, 4.0);
        let child = app.world_mut().spawn((child_transform, ChildOf(a))).id();
        app.update();
        expect_changes(&app, &[a, b]);
        expect_pose(&app);
        for _ in 0..3 {
            app.update();
            expect_changes(&app, &[]);
            expect_pose(&app);
        }

        // One ULP in Source-space translation must not be suppressed by an
        // epsilon, animation throttling or a geometry-equivalence shortcut.
        app.world_mut().resource_mut::<PoseFixture>().pose[0]
            .w_axis
            .x = f32::from_bits(1.0f32.to_bits() + 1);
        app.update();
        expect_changes(&app, &[a]);
        expect_pose(&app);
        assert_eq!(
            app.world()
                .get::<Transform>(a)
                .unwrap()
                .translation
                .x
                .to_bits(),
            1.0f32.to_bits() + 1
        );
        assert_eq!(
            *app.world().get::<GlobalTransform>(child).unwrap(),
            app.world()
                .get::<GlobalTransform>(a)
                .unwrap()
                .mul_transform(child_transform),
        );
        app.update();
        expect_changes(&app, &[]);

        app.world_mut()
            .resource_mut::<PoseFixture>()
            .world_from_model = Mat4::from_scale_rotation_translation(
            Vec3::splat(1.25),
            Quat::from_rotation_y(0.2),
            Vec3::new(7.0, 8.0, 9.0),
        );
        app.update();
        expect_changes(&app, &[a, b]);
        expect_pose(&app);
        app.update();
        expect_changes(&app, &[]);
    }

    #[test]
    fn real_bevy_pose_rotation_only_change_propagates() {
        let (mut app, _, b) = fixture_app();
        app.update();
        app.update();
        expect_changes(&app, &[]);
        let before = *app.world().get::<Transform>(b).unwrap();
        app.world_mut().resource_mut::<PoseFixture>().pose[1] =
            glam::Mat4::from_rotation_translation(
                // An exact half-turn keeps decomposed scale bits unchanged.
                glam::Quat::from_xyzw(1.0, 0.0, 0.0, 0.0),
                glam::Vec3::new(4.0, 5.0, 6.0),
            );
        app.update();
        expect_changes(&app, &[b]);
        expect_pose(&app);
        let after = app.world().get::<Transform>(b).unwrap();
        assert_eq!(
            before.translation.to_array().map(f32::to_bits),
            after.translation.to_array().map(f32::to_bits)
        );
        assert_eq!(
            before.scale.to_array().map(f32::to_bits),
            after.scale.to_array().map(f32::to_bits)
        );
        assert_ne!(
            before.rotation.to_array().map(f32::to_bits),
            after.rotation.to_array().map(f32::to_bits)
        );
        app.update();
        expect_changes(&app, &[]);
    }

    #[test]
    fn real_bevy_pose_scale_only_change_propagates() {
        let (mut app, _, b) = fixture_app();
        app.update();
        app.update();
        expect_changes(&app, &[]);
        let before = *app.world().get::<Transform>(b).unwrap();
        app.world_mut().resource_mut::<PoseFixture>().pose[1] =
            glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::new(1.0, 1.5, 2.0),
                glam::Quat::IDENTITY,
                glam::Vec3::new(4.0, 5.0, 6.0),
            );
        app.update();
        expect_changes(&app, &[b]);
        expect_pose(&app);
        let after = app.world().get::<Transform>(b).unwrap();
        assert_eq!(
            before.translation.to_array().map(f32::to_bits),
            after.translation.to_array().map(f32::to_bits)
        );
        assert_eq!(
            before.rotation.to_array().map(f32::to_bits),
            after.rotation.to_array().map(f32::to_bits)
        );
        assert_ne!(
            before.scale.to_array().map(f32::to_bits),
            after.scale.to_array().map(f32::to_bits)
        );
        app.update();
        expect_changes(&app, &[]);
    }

    #[test]
    fn real_bevy_pose_preserves_signed_zero_new_joints_and_missing_entities() {
        let (mut app, a, b) = fixture_app();
        app.update();
        app.update();
        expect_changes(&app, &[]);

        // PartialEq considers +0 and -0 equal. Restore the decomposition's
        // exact bits even if another writer left the opposite zero sign.
        let expected_x = app.world().get::<Transform>(a).unwrap().rotation.x;
        assert_eq!(expected_x, 0.0);
        app.world_mut().get_mut::<Transform>(a).unwrap().rotation.x =
            f32::from_bits(expected_x.to_bits() ^ (1 << 31));
        app.update();
        expect_pose(&app);
        assert_eq!(
            app.world()
                .get::<Transform>(a)
                .unwrap()
                .rotation
                .x
                .to_bits(),
            expected_x.to_bits()
        );
        app.update();
        expect_changes(&app, &[]);

        let missing = app.world_mut().spawn(Transform::default()).id();
        assert!(app.world_mut().despawn(missing));
        let untouched = Transform::from_xyz(100.0, 200.0, 300.0);
        let unposed = app.world_mut().spawn((Joint, untouched)).id();
        {
            let mut fixture = app.world_mut().resource_mut::<PoseFixture>();
            fixture.instance.joints = vec![a, missing, b, unposed];
            fixture.pose.insert(1, glam::Mat4::IDENTITY);
        }
        app.update();
        expect_changes(&app, &[unposed]); // Initial spawn still propagates.
        expect_pose(&app);
        assert_eq!(*app.world().get::<Transform>(unposed).unwrap(), untouched);
        app.update();
        expect_changes(&app, &[]);

        // A new joint receives its pose immediately while the existing joints
        // and an extra joint beyond the supplied pose remain settled.
        let new = app.world_mut().spawn((Joint, Transform::default())).id();
        app.world_mut()
            .resource_mut::<PoseFixture>()
            .instance
            .joints[2] = new;
        app.update();
        expect_changes(&app, &[new]);
        expect_pose(&app);
        assert_eq!(*app.world().get::<Transform>(unposed).unwrap(), untouched);
        app.update();
        expect_changes(&app, &[]);
    }
}

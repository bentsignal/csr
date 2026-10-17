use super::*;
use bevy::{
    asset::{AssetApp, AssetPlugin, RenderAssetUsages},
    camera::{primitives::Frustum, visibility::{RenderLayers, VisibilityPlugin}},
    math::primitives::ViewFrustum,
    mesh::PrimitiveTopology,
    transform::TransformPlugin,
};

fn geometry() -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![
            [-3.0, 0.0, 0.0], [5.0, 2.0, 0.0], [0.0, 7.0, -1.0],
        ])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 3])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; 3])
        .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_INDEX,
            VertexAttributeValues::Uint16x4(vec![[0, 1, 2, 0]; 3]))
        .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![
            [1.0, 0.0, 0.0, 0.0], [0.1, 0.2, 0.7, 0.0],
            [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 0.0],
        ])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2]))
}

fn bone(name: &str, parent: i32, pos: glam::Vec3) -> mdl::skeleton::Bone {
    mdl::skeleton::Bone { name: name.into(), parent, pos, quat: glam::Quat::IDENTITY,
        rot: glam::Vec3::ZERO, pos_scale: glam::Vec3::ONE, rot_scale: glam::Vec3::ONE,
        pose_to_bone: glam::Mat4::IDENTITY, flags: 0 }
}

#[test]
fn certification_rejects_incomplete_malformed_or_nonfinite_emitted_data() {
    let bind = [Mat4::IDENTITY; 3];
    assert!(certify(&mut geometry(), &bind).is_some());
    let mutations: Vec<Box<dyn Fn(&mut Mesh)>> = vec![
        Box::new(|m| { m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[1.0, 0.0, 0.0, 0.0]; 2]); }),
        Box::new(|m| { m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[f32::NAN, 0.0, 0.0]; 3]); }),
        Box::new(|m| { m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[f32::INFINITY, 0.0, 0.0]; 3]); }),
        Box::new(|m| { m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; 2]); }),
        Box::new(|m| { m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[f32::INFINITY, 0.0, 0.0, 0.0]; 3]); }),
        Box::new(|m| { m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[1.1, -0.1, 0.0, 0.0]; 3]); }),
        Box::new(|m| { m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[0.0; 4]; 3]); }),
        Box::new(|m| { m.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[0.999, 0.0, 0.0, 0.0]; 3]); }),
        // Even a zero-weight lane is indexed by the GPU.
        Box::new(|m| { m.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(vec![[0, 1, 2, 99]; 3])); }),
        Box::new(|m| { m.insert_indices(Indices::U32(vec![0, 1, 3])); }),
        Box::new(|m| { m.insert_indices(Indices::U32(vec![0, 1])); }),
    ];
    for (index, mutate) in mutations.into_iter().enumerate() {
        let mut mesh = geometry();
        mutate(&mut mesh);
        assert!(certify(&mut mesh, &bind).is_none(), "mutation {index}");
    }
    assert!(certify(&mut geometry(), &[]).is_none());
    let mut bad = bind;
    bad[1].w_axis.x = f32::NAN;
    assert!(certify(&mut geometry(), &bad).is_none());
    bad = bind;
    bad[1].x_axis.w = 0.1;
    assert!(certify(&mut geometry(), &bad).is_none());
}

// The two observation systems live at the actual camera/light set boundaries.
// This proves the deferred flag change is flushed before both consumers; the
// second is a schedule sentinel, not an animated shadow-render acceptance test.
#[derive(Resource, Default)]
struct Consumers(Vec<Vec<(bool, Aabb)>>);

fn observe(mut seen: ResMut<Consumers>, skins: Query<(Has<NoFrustumCulling>, &Aabb), With<WorldSkinBounds>>) {
    seen.0.push(skins.iter().map(|(disabled, aabb)| (disabled, *aabb)).collect());
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin, VisibilityPlugin))
        .init_asset::<Mesh>()
        .init_asset::<SkinnedMeshInverseBindposes>()
        .init_resource::<Consumers>()
        .add_systems(PostUpdate, observe.in_set(VisibilitySystems::CheckVisibility))
        .add_systems(PostUpdate, observe.in_set(SimulationLightSystems::CheckLightVisibility)
            .after(VisibilitySystems::CheckVisibility)
            .before(VisibilitySystems::MarkNewlyHiddenEntitiesInvisible));
    install(&mut app);
    app.world_mut().spawn((Camera::default(), Frustum(ViewFrustum::from_clip_from_world(
        &Mat4::perspective_infinite_reverse_rh(1.0, 1.0, 0.1)))));
    app
}

struct Fixture {
    entity: Entity,
    joints: Vec<Entity>,
    mesh: Handle<Mesh>,
    bind: Handle<SkinnedMeshInverseBindposes>,
    // Synthetic oracle data retained only by the test, never the production path.
    vertices: Vec<([f32; 3], [u16; 4], [f32; 4])>,
}

fn fixture(app: &mut App, mut mesh: Mesh, inverse: Vec<Mat4>, poses: &[Transform]) -> Fixture {
    let cert = certify(&mut mesh, &inverse).expect("fixture certifies");
    let VertexAttributeValues::Float32x3(p) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() else { panic!() };
    let VertexAttributeValues::Uint16x4(j) = mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).unwrap() else { panic!() };
    let VertexAttributeValues::Float32x4(w) = mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).unwrap() else { panic!() };
    let vertices = p.iter().zip(j).zip(w).map(|((&p, &j), &w)| (p, j, w)).collect();
    let mesh = app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
    let bind = app.world_mut().resource_mut::<Assets<SkinnedMeshInverseBindposes>>().add(inverse);
    let joints: Vec<_> = poses.iter().map(|t| app.world_mut().spawn(*t).id()).collect();
    let entity = app.world_mut().spawn((
        Mesh3d(mesh.clone()), Transform::default(), NoFrustumCulling, NoAutoAabb, Aabb::default(),
        WorldSkinBounds { mesh: mesh.id(), certificate: cert, _kind: WorldSkin::Body },
        SkinnedMesh { inverse_bindposes: bind.clone(), joints: joints.clone() },
    )).id();
    Fixture { entity, joints, mesh, bind, vertices }
}

fn oracle(app: &App, f: &Fixture) {
    let world = app.world();
    assert!(!world.entity(f.entity).contains::<NoFrustumCulling>());
    let aabb = world.get::<Aabb>(f.entity).unwrap();
    let inv = world.resource::<Assets<SkinnedMeshInverseBindposes>>().get(&f.bind).unwrap();
    let entity_inverse = world.get::<GlobalTransform>(f.entity).unwrap().affine().inverse();
    for &(p, j, w) in &f.vertices {
        let matrices: Vec<_> = j.iter().map(|&i| {
            world.get::<GlobalTransform>(f.joints[i as usize]).unwrap().to_matrix() * inv[i as usize]
        }).collect();
        // Shader order: blend matrices first, then multiply vertex; the bounds
        // proof must cover this rounding path, not just blend transformed points.
        let blended = matrices[0] * w[0] + matrices[1] * w[1] + matrices[2] * w[2] + matrices[3] * w[3];
        let skinned = (blended * Vec3::from_array(p).extend(1.0)).truncate();
        let local = Vec3A::from(entity_inverse.transform_point3(skinned));
        assert!(local.cmpge(aabb.min()).all() && local.cmple(aabb.max()).all(), "{local:?} outside {aabb:?}");
        // Independent double-precision point blending also checks the enclosure
        // against exact arithmetic on the stored input floats, without sharing
        // Bevy's AABB or the shader's matrix-blend calculation.
        fn mul(m: Mat4, v: [f64; 4]) -> [f64; 4] {
            let a = m.to_cols_array();
            std::array::from_fn(|r| (0..4).map(|c| f64::from(a[c * 4 + r]) * v[c]).sum())
        }
        let mut exact = [0.0; 4];
        for lane in 0..4 {
            let i = j[lane] as usize;
            let joint = world.get::<GlobalTransform>(f.joints[i]).unwrap().to_matrix();
            let point = mul(joint, mul(inv[i], [f64::from(p[0]), f64::from(p[1]), f64::from(p[2]), 1.0]));
            for axis in 0..3 { exact[axis] += f64::from(w[lane]) * point[axis]; }
        }
        // GPU positions are world xyz; entity-local conversion is affine and
        // must use w=1 even when stored weights have a tiny unit-sum error.
        exact[3] = 1.0;
        let exact = mul(Mat4::from(entity_inverse), exact);
        for axis in 0..3 {
            assert!(exact[axis] >= f64::from(aabb.min()[axis]) && exact[axis] <= f64::from(aabb.max()[axis]));
        }
    }
}

#[test]
fn blended_axis_converted_weapon_and_ragdoll_poses_fit_with_nonidentity_parent() {
    use crate::skinning::{axis_change, bone_merge, pose_transforms};
    let s = axis_change();
    let bind: Vec<_> = [Vec3::ZERO, Vec3::new(-12.0, 0.0, -4.0), Vec3::new(0.0, -25.0, 0.0)]
        .map(|p| s * Mat4::from_translation(p) * s.transpose()).to_vec();
    let source = vec![
        glam::Mat4::from_rotation_z(0.7),
        glam::Mat4::from_rotation_translation(glam::Quat::from_rotation_y(-1.2), glam::Vec3::new(12.0, 0.0, 80.0)),
        glam::Mat4::from_translation(glam::Vec3::new(0.0, 25.0, 30.0)),
    ];
    let actor = Mat4::from_rotation_translation(Quat::from_rotation_y(1.1), Vec3::new(1500.0, -40.0, -700.0));
    let poses = pose_transforms(actor, &source);
    let mut mesh = geometry();
    if let VertexAttributeValues::Float32x3(p) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION).unwrap() {
        for p in p { *p = s.transform_point3(Vec3::from_array(*p)).to_array(); }
    }
    let mut app = app();
    let f = fixture(&mut app, mesh, bind, &poses);
    let parent = app.world_mut().spawn(Transform::from_xyz(200.0, 30.0, 15.0)
        .with_rotation(Quat::from_rotation_z(0.3)).with_scale(Vec3::new(0.75, 1.25, 1.5))).id();
    app.world_mut().entity_mut(f.entity).insert((ChildOf(parent), Transform::from_xyz(3.0, 2.0, 1.0)));
    app.update();
    oracle(&app, &f);
    // Check Source->Bevy algebra independently, including the inverse bind.
    let source_bind = [Mat4::IDENTITY, Mat4::from_translation(Vec3::new(-12.0, 0.0, -4.0)), Mat4::from_translation(Vec3::new(0.0, -25.0, 0.0))];
    let aabb = app.world().get::<Aabb>(f.entity).unwrap();
    let entity_inverse = app.world().get::<GlobalTransform>(f.entity).unwrap().affine().inverse();
    for &(p, indices, weights) in &f.vertices {
        let source_position = s.transpose().transform_point3(Vec3::from_array(p));
        let mut blended = Vec3::ZERO;
        for lane in 0..4 {
            let i = indices[lane] as usize;
            blended += (Mat4::from_cols_array(&source[i].to_cols_array()) * source_bind[i])
                .transform_point3(source_position) * weights[lane];
        }
        let local = Vec3A::from(entity_inverse.transform_point3(actor.transform_point3(s.transform_point3(blended))));
        assert!(local.cmpge(aabb.min()).all() && local.cmple(aabb.max()).all());
    }
    // Independent ragdoll world-space joints, no actor transform on the mesh.
    for (i, &joint) in f.joints.iter().enumerate() {
        app.world_mut().entity_mut(joint).insert(Transform::from_xyz(-120.0 + i as f32 * 40.0, 3.0, -500.0)
            .with_rotation(Quat::from_rotation_z(1.4 - i as f32)));
    }
    app.update();
    oracle(&app, &f);
    let parent_bones = vec![bone("hand", -1, glam::Vec3::ZERO)];
    let weapon = vec![bone("hand", -1, glam::Vec3::ZERO), bone("muzzle", 0, glam::Vec3::X * 55.0), bone("sight", 1, glam::Vec3::Z * 4.0)];
    let merged = bone_merge(&weapon, &parent_bones, &source[..1]);
    for (&joint, pose) in f.joints.iter().zip(pose_transforms(actor, &merged)) {
        app.world_mut().entity_mut(joint).insert(pose);
    }
    app.update();
    oracle(&app, &f);
}

#[test]
fn current_frame_guard_survives_visibility_teleport_asset_joint_and_nonfinite_transitions() {
    let mut app = app();
    let f = fixture(&mut app, geometry(), vec![Mat4::IDENTITY; 3], &[Transform::from_xyz(0.0, 0.0, -30.0); 3]);
    // Initial flag remains until the first propagated pose/calculation.
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
    app.update();
    oracle(&app, &f);
    assert!(app.world().get::<ViewVisibility>(f.entity).unwrap().get());
    for &joint in &f.joints { app.world_mut().entity_mut(joint).insert(Transform::from_xyz(50_000.0, 0.0, -30.0)); }
    app.update();
    oracle(&app, &f);
    assert!(!app.world().get::<ViewVisibility>(f.entity).unwrap().get());
    app.world_mut().entity_mut(f.entity).insert(Visibility::Hidden);
    for &joint in &f.joints { app.world_mut().entity_mut(joint).insert(Transform::from_xyz(0.0, 0.0, -30.0)); }
    app.update();
    oracle(&app, &f); // hidden skins still receive fresh boxes
    app.world_mut().entity_mut(f.entity).insert(Visibility::Visible);
    app.update();
    assert!(app.world().get::<ViewVisibility>(f.entity).unwrap().get());
    let removed = app.world_mut().resource_mut::<Assets<Mesh>>().remove(f.mesh.id()).unwrap();
    app.update();
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
    app.world_mut().resource_mut::<Assets<Mesh>>().insert(f.mesh.id(), removed).unwrap();
    app.update();
    oracle(&app, &f);
    let bind = app.world_mut().resource_mut::<Assets<SkinnedMeshInverseBindposes>>().remove(f.bind.id()).unwrap();
    app.update();
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
    app.world_mut().resource_mut::<Assets<SkinnedMeshInverseBindposes>>().insert(f.bind.id(), bind).unwrap();
    app.update();
    oracle(&app, &f);
    app.world_mut().entity_mut(f.joints[1]).remove::<GlobalTransform>();
    app.update();
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
    app.world_mut().entity_mut(f.joints[1]).insert((Transform::from_xyz(0.0, 0.0, -30.0), GlobalTransform::IDENTITY));
    app.update();
    oracle(&app, &f);
    app.world_mut().entity_mut(f.joints[2]).insert(Transform::from_xyz(f32::NAN, 0.0, -30.0));
    app.update();
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
    // Both consumers saw the fallback in this frame, never the old finite box
    // with culling enabled.
    let seen = &app.world().resource::<Consumers>().0;
    assert!(seen[seen.len() - 2..].iter().all(|s| s[0].0));
    app.world_mut().entity_mut(f.joints[2]).insert(Transform::from_xyz(0.0, 0.0, -30.0));
    app.update();
    oracle(&app, &f);
    let seen = &app.world().resource::<Consumers>().0;
    assert!(seen[seen.len() - 2..].iter().all(|s| !s[0].0));
}

#[test]
fn render_world_gpu_take_retains_certificate_and_tracked_edits_fail_closed() {
    let mut app = app();
    let f = fixture(&mut app, geometry(), vec![Mat4::IDENTITY; 3], &[Transform::from_xyz(0.0, 0.0, -30.0); 3]);
    app.update();
    // Bevy's real extraction uses untracked mutation. Follow that exact path.
    let gpu = app.world_mut().resource_mut::<Assets<Mesh>>().get_mut_untracked(f.mesh.id()).unwrap().take_gpu_data().unwrap();
    assert_eq!(gpu.asset_usage, RenderAssetUsages::RENDER_WORLD);
    assert!(app.world().resource::<Assets<Mesh>>().get(&f.mesh).unwrap().try_attribute(Mesh::ATTRIBUTE_POSITION).is_err());
    app.update();
    oracle(&app, &f);
    // A later tracked edit cannot hide behind an old certificate after upload.
    app.world_mut().resource_mut::<Assets<Mesh>>().get_mut(&f.mesh).unwrap().set_skinned_mesh_bounds(None);
    app.update();
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
    // Restore fresh CPU geometry through a tracked replacement and re-certify.
    let mut replacement = geometry();
    certify(&mut replacement, &[Mat4::IDENTITY; 3]).unwrap();
    app.world_mut().resource_mut::<Assets<Mesh>>().insert(f.mesh.id(), replacement).unwrap();
    app.update();
    oracle(&app, &f);
}

#[test]
fn real_asset_disposal_bounds_edit_cache_and_live_handle_restore_still_recertifies() {
    // Observe the production cache handler with real Bevy asset events, without
    // exposing the guard's Local state or fabricating disposal messages.
    #[derive(Resource, Default)]
    struct EditCache {
        edited: HashMap<AssetId<Mesh>, Option<bool>>,
        batch: Vec<AssetEvent<Mesh>>,
    }
    fn observe_cache(mut events: MessageReader<AssetEvent<Mesh>>, mut cache: ResMut<EditCache>) {
        let batch: Vec<_> = events.read().copied().collect();
        record_mesh_edits(&mut cache.edited, batch.iter().copied());
        cache.batch = batch;
    }
    fn assert_disposed(app: &App, id: AssetId<Mesh>) {
        let cache = app.world().resource::<EditCache>();
        let unused = cache.batch.iter().position(|e| e.is_unused(id)).expect("real Unused event");
        let removed = cache.batch.iter().position(|e| e.is_removed(id)).expect("real Removed event");
        assert!(unused < removed, "exercise Bevy's last-handle event order");
        assert!(!cache.edited.contains_key(&id), "disposed asset must leave no cache entry");
        assert!(!app.world().resource::<Assets<Mesh>>().contains(id));
    }

    let mut app = app();
    app.init_resource::<EditCache>()
        .add_systems(PostUpdate, observe_cache
            .after(bevy::asset::AssetEventSystems).before(GuardedBoundsReady));
    let f = fixture(&mut app, geometry(), vec![Mat4::IDENTITY; 3], &[Transform::from_xyz(0.0, 0.0, -30.0); 3]);
    app.update();
    oracle(&app, &f);
    assert!(app.world().resource::<EditCache>().edited.is_empty());

    // Explicit removal retains both fixture/entity strong handles. Its tombstone
    // must survive unrelated disposal and absence across multiple frames.
    let mut removed = app.world_mut().resource_mut::<Assets<Mesh>>().remove(f.mesh.id()).unwrap();
    app.update();
    assert_eq!(app.world().resource::<EditCache>().batch, vec![AssetEvent::Removed { id: f.mesh.id() }]);
    assert_eq!(app.world().resource::<EditCache>().edited.get(&f.mesh.id()), Some(&None));
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());

    // An unrelated edited mesh has an entry before normal last-handle disposal.
    let transient = app.world_mut().resource_mut::<Assets<Mesh>>().add(geometry());
    let transient_id = transient.id();
    app.world_mut().resource_mut::<Assets<Mesh>>().get_mut(&transient).unwrap().set_skinned_mesh_bounds(None);
    app.update();
    assert!(app.world().resource::<EditCache>().edited.contains_key(&transient_id));
    drop(transient);
    app.update();
    assert_disposed(&app, transient_id);
    assert_eq!(app.world().resource::<EditCache>().edited.len(), 1);

    // Particle-like meshes never opt into WorldSkinBounds. Repeated batches
    // include tracked edits and all last-handle drops in the same event batch.
    for _ in 0..32 {
        let mut ids = Vec::new();
        for _ in 0..16 {
            let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(geometry());
            ids.push(handle.id());
            app.world_mut().resource_mut::<Assets<Mesh>>().get_mut(&handle).unwrap().set_skinned_mesh_bounds(None);
            drop(handle);
        }
        app.update();
        for id in ids { assert_disposed(&app, id); }
        let cache = app.world().resource::<EditCache>();
        assert_eq!(cache.edited.len(), 1, "unrelated mesh churn must not accumulate entries");
        assert_eq!(cache.edited.get(&f.mesh.id()), Some(&None));
    }

    // Preserve the old finite generated bounds but corrupt GPU weights. This
    // restoration emits Added, so forgetting the live-handle tombstone would
    // bypass re-certification and incorrectly enable culling.
    removed.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[f32::NAN, 0.0, 0.0, 0.0]; 3]);
    app.world_mut().resource_mut::<Assets<Mesh>>().insert(f.mesh.id(), removed).unwrap();
    app.update();
    assert_eq!(app.world().resource::<EditCache>().batch, vec![AssetEvent::Added { id: f.mesh.id() }]);
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
    app.update();
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());

    let mut valid = geometry();
    certify(&mut valid, &[Mat4::IDENTITY; 3]).unwrap();
    app.world_mut().resource_mut::<Assets<Mesh>>().insert(f.mesh.id(), valid).unwrap();
    app.update();
    assert_eq!(app.world().resource::<EditCache>().batch, vec![AssetEvent::Modified { id: f.mesh.id() }]);
    oracle(&app, &f);

    // Scene teardown also releases the previously edited eligible mesh.
    let id = f.mesh.id();
    assert!(app.world_mut().despawn(f.entity));
    for &joint in &f.joints { assert!(app.world_mut().despawn(joint)); }
    drop(f);
    app.update();
    assert_disposed(&app, id);
    assert!(app.world().resource::<EditCache>().edited.is_empty());
}

#[test]
fn runtime_malformed_geometry_bindposes_and_missing_aabb_restore_conservatively() {
    let mut app = app();
    let f = fixture(&mut app, geometry(), vec![Mat4::IDENTITY; 3], &[Transform::from_xyz(0.0, 0.0, -30.0); 3]);
    app.update();
    let mut malformed = geometry();
    certify(&mut malformed, &[Mat4::IDENTITY; 3]).unwrap();
    // Keep the old finite retained bounds while invalidating the GPU weights.
    malformed.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, vec![[f32::NAN, 0.0, 0.0, 0.0]; 3]);
    app.world_mut().resource_mut::<Assets<Mesh>>().insert(f.mesh.id(), malformed).unwrap();
    app.update();
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
    let mut restored = geometry();
    certify(&mut restored, &[Mat4::IDENTITY; 3]).unwrap();
    app.world_mut().resource_mut::<Assets<Mesh>>().insert(f.mesh.id(), restored).unwrap();
    app.update();
    oracle(&app, &f);
    let mut invalid = vec![Mat4::IDENTITY; 3];
    invalid[0].x_axis.x = f32::INFINITY;
    app.world_mut().resource_mut::<Assets<SkinnedMeshInverseBindposes>>()
        .insert(f.bind.id(), invalid.into()).unwrap();
    app.update();
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
    app.world_mut().resource_mut::<Assets<SkinnedMeshInverseBindposes>>()
        .insert(f.bind.id(), vec![Mat4::IDENTITY; 3].into()).unwrap();
    app.world_mut().entity_mut(f.entity).remove::<Aabb>();
    app.update();
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
    assert!(app.world().entity(f.entity).contains::<Aabb>());
    app.update();
    oracle(&app, &f);
}

#[test]
fn finite_padding_covers_large_cancellation_and_falls_back_outside_envelope() {
    let mut app = app();
    let mut mesh = geometry();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[131_071.99, -131_071.99, 0.0]; 3]);
    let f = fixture(&mut app, mesh, vec![Mat4::from_translation(Vec3::new(-131_072.0, 131_072.0, 0.0)); 3],
        &[Transform::from_xyz(131_072.0, -131_072.0, -10.0); 3]);
    app.update();
    oracle(&app, &f);
    let aabb = app.world().get::<Aabb>(f.entity).unwrap();
    assert!(aabb.half_extents.min_element() >= 1.0 / 64.0);
    app.world_mut().entity_mut(f.entity).insert(Transform::from_scale(Vec3::new(0.000001, 1.0, 1.0)));
    app.update();
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
    app.world_mut().entity_mut(f.entity).insert(Transform::default());
    app.world_mut().entity_mut(f.joints[0]).insert(Transform::from_xyz(f32::MAX, 0.0, 0.0));
    app.update();
    assert!(app.world().entity(f.entity).contains::<NoFrustumCulling>());
}

#[test]
fn caller_exemptions_and_deferred_world_spawn_are_explicit() {
    use crate::skinning::{self, SkinnedAsset};
    #[derive(Resource)]
    struct Spawned([skinning::SkinnedInstance; 4], skinning::SkinnedInstance);
    let mut app = app();
    let f = fixture(&mut app, geometry(), vec![Mat4::IDENTITY; 3], &[Transform::default(); 3]);
    let certificate = app.world().get::<WorldSkinBounds>(f.entity).unwrap().certificate.clone();
    let model = mdl::Model { meshes: vec![mdl::Mesh::default()], bones: (0..3)
        .map(|i| bone(&format!("joint{i}"), -1, glam::Vec3::ZERO)).collect(), ..Default::default() };
    let asset = SkinnedAsset { model: Arc::new(model), parts: vec![(f.mesh.clone(), Default::default())],
        inverse_bindposes: f.bind.clone(), culling: vec![Some(certificate)] };
    app.add_systems(Update, move |mut commands: Commands, mut done: Local<bool>| {
        if *done { return; }
        *done = true;
        let exempt = [None, Some(RenderLayers::layer(0)), Some(RenderLayers::layer(2)), Some(RenderLayers::layer(3))]
            .map(|layer| skinning::spawn(&mut commands, &asset, layer, None));
        let admitted = skinning::spawn_world(&mut commands, &asset, None, WorldSkin::Corpse);
        for &joint in &admitted.joints { commands.entity(joint).insert(Transform::from_xyz(0.0, 0.0, -30.0)); }
        commands.insert_resource(Spawned(exempt, admitted));
    });
    app.update();
    let instances = app.world().resource::<Spawned>();
    for instance in &instances.0 {
        let e = app.world().entity(instance.meshes[0]);
        assert!(e.contains::<NoFrustumCulling>());
        assert!(!e.contains::<WorldSkinBounds>());
    }
    let e = app.world().entity(instances.1.meshes[0]);
    assert!(e.contains::<Aabb>() && !e.contains::<NoFrustumCulling>() && e.contains::<WorldSkinBounds>());
    assert!(app.world().get::<ViewVisibility>(instances.1.meshes[0]).unwrap().get());
    assert!(app.world().get::<Aabb>(instances.1.meshes[0]).unwrap().center.z < -20.0);
}

#[test]
fn experiment_requires_explicit_one_and_uncertified_world_spawns_stay_uncullable() {
    assert!(!experiment_requested(None));
    for value in ["", "0", "true", "false", "yes", " 1", "1 ", "01"] {
        assert!(!experiment_requested(Some(OsStr::new(value))), "{value:?}");
    }
    assert!(experiment_requested(Some(OsStr::new("1"))));

    // Empty metadata is the production disabled-policy asset path. A failed
    // certificate is the candidate's invalid-data path. Neither may opt in,
    // even with the candidate guard installed and a valid retained mesh box.
    use crate::skinning::{self, SkinnedAsset};
    #[derive(Resource)]
    struct Spawned(Vec<skinning::SkinnedInstance>);
    let mut app = app();
    let f = fixture(&mut app, geometry(), vec![Mat4::IDENTITY; 3], &[Transform::default(); 3]);
    let model = mdl::Model { meshes: vec![mdl::Mesh::default()], bones: (0..3)
        .map(|i| bone(&format!("joint{i}"), -1, glam::Vec3::ZERO)).collect(), ..Default::default() };
    let asset = SkinnedAsset { model: Arc::new(model), parts: vec![(f.mesh.clone(), Default::default())],
        inverse_bindposes: f.bind.clone(), culling: Vec::new() };
    app.add_systems(Update, move |mut commands: Commands, mut done: Local<bool>| {
        if *done { return; }
        *done = true;
        let mut invalid = asset.clone();
        invalid.culling = vec![None];
        let instances = [&asset, &invalid].into_iter().flat_map(|asset| {
            [WorldSkin::Body, WorldSkin::HeldWeapon, WorldSkin::Corpse].map(|kind|
                skinning::spawn_world(&mut commands, asset, None, kind))
        }).collect();
        commands.insert_resource(Spawned(instances));
    });
    app.update();
    app.update();
    for instance in &app.world().resource::<Spawned>().0 {
        let e = app.world().entity(instance.meshes[0]);
        assert!(e.contains::<NoFrustumCulling>());
        assert!(!e.contains::<WorldSkinBounds>() && !e.contains::<NoAutoAabb>());
    }
}

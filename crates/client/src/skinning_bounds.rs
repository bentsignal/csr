//! Guarded, explicit world-skin culling. Geometry is certified once, before GPU
//! extraction; poses are checked after propagation on every frame, even hidden.
//! No gameplay/animation writes belong here.

use bevy::{
    asset::{AssetEvent, AssetId},
    camera::{primitives::Aabb, visibility::{NoAutoAabb, NoFrustumCulling, VisibilitySystems}},
    ecs::schedule::ApplyDeferred,
    light::SimulationLightSystems,
    math::Vec3A,
    mesh::{Indices, VertexAttributeValues, skinning::{
        entity_aabb_from_skinned_mesh_bounds, SkinnedMesh, SkinnedMeshBounds,
        SkinnedMeshInverseBindposes,
    }},
    prelude::*,
    transform::TransformSystems,
};
use std::{collections::HashMap, sync::Arc};

/// Only these call sites may opt in. Render layers never grant eligibility.
#[derive(Clone, Copy, Debug)]
pub(crate) enum WorldSkin {
    Body,
    HeldWeapon,
    Corpse,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Certificate {
    bounds: SkinnedMeshBounds,
    joint_count: usize,
    position_magnitude: f64,
}

#[derive(Component)]
pub(crate) struct WorldSkinBounds {
    mesh: AssetId<Mesh>,
    certificate: Arc<Certificate>,
    _kind: WorldSkin,
}

/// Certify the final emitted data, not the imported VVD input. All four shader
/// indices must be legal, including zero-weight lanes (0 * NaN is still NaN).
/// Exact attribute lengths + a positive influence on every vertex establish
/// complete coverage by Bevy's per-positive-influence generator.
pub(crate) fn certify(mesh: &mut Mesh, inverse: &[Mat4]) -> Option<Arc<Certificate>> {
    if inverse.is_empty() || inverse.len() > 256 || !inverse.iter().all(|m| affine(*m)) {
        return None;
    }
    if mesh.try_has_morph_targets().ok()? { return None; }
    let VertexAttributeValues::Float32x3(positions) = mesh.try_attribute(Mesh::ATTRIBUTE_POSITION).ok()? else { return None };
    let VertexAttributeValues::Uint16x4(indices) = mesh.try_attribute(Mesh::ATTRIBUTE_JOINT_INDEX).ok()? else { return None };
    let VertexAttributeValues::Float32x4(weights) = mesh.try_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).ok()? else { return None };
    let count = positions.len();
    if count == 0 || indices.len() != count || weights.len() != count {
        return None;
    }
    // Other emitted attributes must not truncate the GPU vertex count either.
    if mesh.try_attributes().ok()?.any(|(_, a)| a.len() != count) {
        return None;
    }
    let VertexAttributeValues::Float32x3(normals) = mesh.try_attribute(Mesh::ATTRIBUTE_NORMAL).ok()? else { return None };
    let VertexAttributeValues::Float32x2(uvs) = mesh.try_attribute(Mesh::ATTRIBUTE_UV_0).ok()? else { return None };
    if normals.iter().flatten().chain(uvs.iter().flatten()).any(|x| !x.is_finite()) {
        return None;
    }
    if let Some(tangent) = mesh.try_attribute_option(Mesh::ATTRIBUTE_TANGENT).ok()? {
        let VertexAttributeValues::Float32x4(tangent) = tangent else { return None };
        if tangent.iter().flatten().any(|x| !x.is_finite()) { return None; }
    }
    let mut magnitude = 1.0f64;
    let mut influenced = vec![false; inverse.len()];
    for ((p, j), w) in positions.iter().zip(indices).zip(weights) {
        if !p.iter().all(|x| x.is_finite())
            || j.iter().any(|&x| x as usize >= inverse.len())
            || w.iter().any(|x| !x.is_finite() || *x < 0.0)
        {
            return None;
        }
        let sum: f64 = w.iter().map(|&x| f64::from(x)).sum();
        if (sum - 1.0).abs() > 4.0 * f64::from(f32::EPSILON) {
            return None;
        }
        for (&joint, &weight) in j.iter().zip(w) {
            if weight > 0.0 { influenced[joint as usize] = true; }
        }
        for &x in p { magnitude = magnitude.max(f64::from(x).abs()); }
    }
    let vertex_indices = mesh.try_indices().ok()?;
    let valid_indices = match vertex_indices {
        Indices::U16(v) => !v.is_empty() && v.len() % 3 == 0 && v.iter().all(|&i| (i as usize) < count),
        Indices::U32(v) => !v.is_empty() && v.len() % 3 == 0 && v.iter().all(|&i| (i as usize) < count),
    };
    if mesh.primitive_topology() != bevy::mesh::PrimitiveTopology::TriangleList || !valid_indices {
        return None;
    }
    mesh.generate_skinned_mesh_bounds().ok()?;
    let bounds = mesh.skinned_mesh_bounds()?;
    let covered: Vec<_> = bounds.aabb_index_to_joint_index.iter().map(|i| i.0 as usize).collect();
    if covered != influenced.iter().enumerate().filter_map(|(i, &used)| used.then_some(i)).collect::<Vec<_>>()
        || bounds.aabbs.len() != covered.len()
        || bounds.aabbs.iter().any(|a| !a.center.is_finite() || !a.half_size.is_finite() || a.half_size.min_element() < 0.0)
    {
        return None;
    }
    Some(Arc::new(Certificate { bounds: bounds.clone(), joint_count: inverse.len(), position_magnitude: magnitude }))
}

pub(crate) fn enable(
    entity: &mut EntityCommands,
    mesh: &Handle<Mesh>,
    certificate: &Option<Arc<Certificate>>,
    kind: WorldSkin,
) {
    if let Some(certificate) = certificate {
        // Intentionally no DynamicSkinnedMeshBounds: Bevy's updater silently
        // retains stale bounds on failure. Call its public calculator once in
        // our guard instead. NoAutoAabb prevents static bounds overwriting it.
        entity.insert((Aabb::default(), NoAutoAabb, WorldSkinBounds {
            mesh: mesh.id(), certificate: certificate.clone(), _kind: kind,
        }));
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct GuardedBoundsReady;

pub(crate) fn install(app: &mut App) {
    app.add_systems(PostUpdate,
        (update, ApplyDeferred).chain()
            .in_set(GuardedBoundsReady)
            .after(TransformSystems::Propagate)
            .after(VisibilitySystems::CalculateBounds)
            .before(VisibilitySystems::CheckVisibility)
            .before(SimulationLightSystems::CheckLightVisibility));
}

fn record_mesh_edits(
    edited: &mut HashMap<AssetId<Mesh>, Option<bool>>,
    events: impl IntoIterator<Item = AssetEvent<Mesh>>,
) {
    let mut unused = Vec::new();
    for event in events {
        match event {
            AssetEvent::Modified { id } | AssetEvent::Removed { id } => { edited.insert(id, None); }
            AssetEvent::Added { id } if edited.contains_key(&id) => { edited.insert(id, None); }
            AssetEvent::Unused { id } => { unused.push(id); }
            _ => {}
        }
    }
    // Bevy queues Unused before Removed on last-handle disposal. Clean up only
    // after the full batch so Removed cannot recreate a dead asset's entry.
    // Explicit remove with live handles emits no Unused: keep that tombstone
    // so a later Added replacement must re-certify even with retained bounds.
    for id in unused { edited.remove(&id); }
}

// Asset edits must use tracked Assets APIs, before AssetEventSystems (as all
// current geometry writers do). Render extraction alone uses get_mut_untracked
// to take GPU data; it retains exactly the certified bounds. Modified/replaced
// geometry is re-certified once per asset event; an extracted edited asset
// cannot be re-certified and stays uncullable. No CPU vertex copy is retained.
fn update(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    inverse_assets: Res<Assets<SkinnedMeshInverseBindposes>>,
    mut events: MessageReader<AssetEvent<Mesh>>,
    mut edited: Local<HashMap<AssetId<Mesh>, Option<bool>>>,
    joints: Query<&GlobalTransform>,
    mut skins: Query<(Entity, &WorldSkinBounds, Option<&Mesh3d>, Option<&SkinnedMesh>,
        Option<&GlobalTransform>, Option<&mut Aabb>, Has<NoFrustumCulling>)>,
) {
    record_mesh_edits(&mut edited, events.read().copied());
    for (entity, eligibility, mesh_handle, skin, global, aabb, disabled) in &mut skins {
        let fresh = (|| {
            let handle = mesh_handle?;
            let skin = skin?;
            let global = global?;
            if handle.id() != eligibility.mesh { return None; }
            let mesh = meshes.get(handle)?;
            let inverse = inverse_assets.get(&skin.inverse_bindposes)?;
            let cert = &eligibility.certificate;
            if let Some(valid) = edited.get_mut(&handle.id()) {
                let valid = valid.get_or_insert_with(|| {
                    // Only on a tracked edit, never a per-frame vertex walk.
                    let mut copy = mesh.clone();
                    certify(&mut copy, inverse).is_some_and(|c| *c == **cert)
                });
                if !*valid { return None; }
            }
            if mesh.skinned_mesh_bounds() != Some(&cert.bounds)
                || inverse.len() != cert.joint_count || skin.joints.len() != cert.joint_count
            {
                return None;
            }
            let padding = pose_padding(cert, skin, inverse, global, &joints)?;
            let computed = entity_aabb_from_skinned_mesh_bounds(&joints, mesh, skin, inverse, Some(global)).ok()?;
            padded(computed.min, computed.max, padding)
        })();
        if let Some(fresh) = fresh {
            // Missing Aabb also falls back this frame; repair it for next time.
            if let Some(mut aabb) = aabb {
                *aabb = fresh;
                if disabled { commands.entity(entity).remove::<NoFrustumCulling>(); }
            } else {
                commands.entity(entity).insert((fresh, NoFrustumCulling));
            }
        } else if !disabled {
            commands.entity(entity).insert(NoFrustumCulling);
        }
    }
}

fn affine(m: Mat4) -> bool {
    m.is_finite() && m.row(3) == Vec4::W
}

// Absolute row-sum bounds preserve cancellation terms, unlike the final AABB's
// magnitude alone. f64 keeps the padding budget independent of f32 overflow.
fn magnitude(m: Mat4) -> (f64, f64) {
    let mut linear = 0.0f64;
    let mut translation = 0.0f64;
    for row in 0..3 {
        let r = m.row(row);
        linear = linear.max(f64::from(r.x).abs() + f64::from(r.y).abs() + f64::from(r.z).abs());
        translation = translation.max(f64::from(r.w).abs());
    }
    (linear, translation)
}

fn pose_padding(
    cert: &Certificate, skin: &SkinnedMesh, inverse: &[Mat4],
    global: &GlobalTransform, joints: &Query<&GlobalTransform>,
) -> Option<f32> {
    let entity = global.to_matrix();
    if !affine(entity) { return None; }
    let determinant = global.affine().matrix3.determinant();
    if !determinant.is_finite() || determinant == 0.0 { return None; }
    // Match the calculator's affine inverse, including its rounding path.
    let entity_inverse = Mat4::from(global.affine().inverse());
    if !affine(entity_inverse) { return None; }
    let (el, et) = magnitude(entity);
    let (il, it) = magnitude(entity_inverse);
    // Restrict conditioning, not actor placement. Parent rotation/scale and
    // translation remain supported; singular/ill-conditioned meshes fall back.
    if el * il > 16.0 { return None; }
    let residual = entity * entity_inverse - Mat4::IDENTITY;
    let (rl, rt) = magnitude(residual);
    let tolerance = 32.0 * f64::from(f32::EPSILON);
    if !residual.is_finite() || rl > tolerance || rt > tolerance * (el * it + et + 1.0) {
        return None;
    }
    let mut world = 1.0f64;
    for (&joint, &bind) in skin.joints.iter().zip(inverse) {
        let joint = joints.get(joint).ok()?.to_matrix();
        if !affine(joint) || !affine(bind) || !(joint * bind).is_finite() { return None; }
        let (jl, jt) = magnitude(joint);
        let (bl, bt) = magnitude(bind);
        world = world.max(jl * (bl * cert.position_magnitude + bt) + jt);
    }
    let local = il * (world + et) + it;
    let work = (world + local + el * local + et).max(1.0) * il.max(1.0);
    // 512 epsilon exceeds the <100 dependent roundings in 4-joint GPU matrix
    // blend, Bevy joint-box conversion, inverse/local conversion, and final
    // center/half-extents reconstruction, including 4 epsilon weight-sum error.
    // The 1/64-inch floor covers tiny/subnormal arithmetic. Bound the envelope
    // so adding padding stays finite and meaningful; outside it never cull.
    if !work.is_finite() || work > 16_777_216.0 { return None; }
    Some((work * (512.0 * f64::from(f32::EPSILON)) + 1.0 / 64.0) as f32)
}

fn padded(min: Vec3A, max: Vec3A, padding: f32) -> Option<Aabb> {
    if !min.is_finite() || !max.is_finite() || min.cmpgt(max).any() || !padding.is_finite() || padding <= 0.0 {
        return None;
    }
    let mut aabb = Aabb::from_min_max(min.into(), max.into());
    aabb.half_extents += Vec3A::splat(padding);
    (aabb.center.is_finite() && aabb.half_extents.is_finite()
        && aabb.min().cmple(min).all() && aabb.max().cmpge(max).all()).then_some(aabb)
}

#[cfg(test)]
mod tests;

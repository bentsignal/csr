//! Cosmetic physics only: never supplies player collision, damage or TickInput.
//! Uses the installed model's named solids and joints, with a separate solver.
//! Rapier is not VPhysics; equivalent forces and joint frames are not established.
use glam::{Mat4, Quat, Vec3};
use mdl::{Model, phy::ragdoll::Ragdoll};
use rapier3d::prelude::*;
use std::{collections::HashMap, sync::Arc};

const SCALE: f32 = 0.0254;
const STEP: f32 = 1. / 128.;
const WORLD_RADIUS: f32 = 384.;

fn vector(v: Vec3) -> Vector { Vector::new(v.x, v.y, v.z) }
fn source(v: Vector) -> Vec3 { Vec3::new(v.x, v.y, v.z) }
fn rotation(q: Quat) -> Rotation { Rotation::from_xyzw(q.x, q.y, q.z, q.w) }
fn pose(m: Mat4) -> Pose {
    let (_, q, p) = m.to_scale_rotation_translation();
    Pose::from_parts(vector(p * SCALE), rotation(q))
}
fn matrix(p: &Pose) -> Mat4 {
    let q = p.rotation;
    Mat4::from_rotation_translation(Quat::from_xyzw(q.x, q.y, q.z, q.w), source(p.translation) / SCALE)
}

pub struct Rig {
    bones: Vec<mdl::skeleton::Bone>,
    data: Ragdoll,
    bone_ids: Vec<usize>,
    shapes: Vec<SharedShape>,
}

impl Rig {
    /// Checksums and named bindings must match; unsupported geometry fails closed.
    pub fn load(model: &Model, mdl_bytes: &[u8], phy_bytes: &[u8]) -> Option<Self> {
        let checksum = u32::from_le_bytes(mdl_bytes.get(8..12)?.try_into().ok()?);
        let data = Ragdoll::parse(phy_bytes).ok()?;
        if data.solids.len() > 30 { return None; } // one collision-group bit per solid
        let bone_ids = data.bind(checksum, &model.bones).ok()?.solid_bones;
        let shapes = data.solids.iter().map(|solid| {
            let parts: Option<Vec<_>> = solid.hulls.iter().map(|hull| {
                let points: Vec<_> = hull.points.iter().map(|p| vector(*p * SCALE)).collect();
                Some((Pose::IDENTITY, SharedShape::convex_hull(&points)?))
            }).collect();
            Some(SharedShape::compound(parts?))
        }).collect::<Option<Vec<_>>>()?;
        Some(Self { bones: model.bones.clone(), data, bone_ids, shapes })
    }
}

/// One bounded, independent corpse. Static geometry is limited to its origin's
/// neighborhood; it cannot push players, weapons, or other gameplay objects.
pub struct Corpse {
    rig: Arc<Rig>,
    world: PhysicsWorld,
    handles: Vec<RigidBodyHandle>,
    initial: Vec<Mat4>,
    local: Vec<Mat4>,
    accumulator: f32,
    elapsed: f32,
}

impl Corpse {
    pub fn new(rig: Arc<Rig>, collision: &collision::CollisionWorld,
        bone_pose: &[Mat4], origin: Vec3, yaw: f32, velocity: Vec3,
        hit: Option<(Vec3, Vec3)>) -> Option<Self> {
        if bone_pose.len() != rig.bones.len() || !origin.is_finite() || !velocity.is_finite() { return None; }
        let root = Mat4::from_rotation_translation(Quat::from_rotation_z(yaw.to_radians()), origin);
        let initial: Vec<_> = bone_pose.iter().map(|m| root * *m).collect();
        if initial.iter().any(|m| !m.is_finite()) { return None; }
        let local = rig.bones.iter().enumerate().map(|(i, b)| {
            if b.parent >= 0 { initial[b.parent as usize].inverse() * initial[i] } else { initial[i] }
        }).collect();
        let mut world = PhysicsWorld::new();
        world.gravity = vector(Vec3::new(0., 0., -800. * SCALE));
        world.integration_parameters.dt = STEP;
        add_static(&mut world, collision, origin);
        let mut handles = Vec::new();
        for (i, solid) in rig.data.solids.iter().enumerate() {
            let body = RigidBodyBuilder::dynamic().pose(pose(initial[rig.bone_ids[i]]))
                .linvel(vector(velocity * SCALE)).linear_damping(solid.damping)
                .angular_damping(solid.rotational_damping).ccd_enabled(true);
            let handle = world.bodies.insert(body);
            let mut filter = 1;
            for (a,b) in &rig.data.collision_pairs {
                if a.0 == i { filter |= 1 << (b.0 + 1); }
                if b.0 == i { filter |= 1 << (a.0 + 1); }
            }
            let collider = ColliderBuilder::new(rig.shapes[i].clone()).mass(solid.mass)
                .collision_groups(InteractionGroups::new(Group::from_bits_retain(1 << (i+1)),
                    Group::from_bits_retain(filter), InteractionTestMode::And))
                .friction(0.6).restitution(0.05);
            world.colliders.insert_with_parent(collider, handle, &mut world.bodies);
            handles.push(handle);
        }
        let bind = mdl::skeleton::model_space(&rig.bones, &mdl::skeleton::rest_pose(&rig.bones));
        for joint in &rig.data.constraints {
            let a = joint.parent.0;
            let b = joint.child.0;
            // Rest-space joint frames retain the model's anatomical reference;
            // current animation must not redefine zero bend for every death.
            let parent = bind[rig.bone_ids[a]];
            let child = bind[rig.bone_ids[b]];
            let mut builder = GenericJointBuilder::new(JointAxesMask::LOCKED_SPHERICAL_AXES)
                .local_frame1(pose(parent.inverse() * child)).local_frame2(Pose::IDENTITY)
                .contacts_enabled(false);
            for (axis, limit) in [JointAxis::AngX, JointAxis::AngY, JointAxis::AngZ].into_iter().zip(joint.axes) {
                builder = builder.limits(axis, [limit.min.to_radians(), limit.max.to_radians()]);
            }
            world.impulse_joints.insert(handles[a], handles[b], builder, true);
        }
        // Only actual ray direction/position are supplied. Impulse strength is a
        // conservative presentation policy, not a claimed original weapon force.
        if let Some((point, direction)) = hit.filter(|(p,d)| p.is_finite() && d.is_finite()) {
            if let Some(index) = handles.iter().enumerate().min_by(|(_,a),(_,b)| {
                let p = vector(point * SCALE);
                world.bodies[**a].translation().distance_squared(p)
                    .total_cmp(&world.bodies[**b].translation().distance_squared(p))
            }).map(|(i,_)| i) {
                let mass = world.bodies[handles[index]].mass();
                let center = world.bodies[handles[index]].center_of_mass();
                // Rewound hit points can belong to a historical player position.
                // Do not turn network delay into an enormous angular lever arm.
                let contact = center + vector((point*SCALE-source(center)).clamp_length_max(0.35));
                world.bodies[handles[index]].apply_impulse_at_point(
                    vector(direction.normalize_or_zero() * mass * 0.6), contact, true);
            }
        }
        Some(Self { rig, world, handles, initial, local, accumulator: 0., elapsed: 0. })
    }

    pub fn advance(&mut self, dt: f32) {
        // Bounded catch-up, then settled corpses incur no solver work. Pausing
        // supplies dt=0 from the caller instead of advancing by wall time.
        if !dt.is_finite() || dt <= 0. || self.elapsed >= 12. { return; }
        if self.elapsed > 0.1 && self.handles.iter().all(|h| self.world.bodies[*h].is_sleeping()) { return; }
        self.accumulator = (self.accumulator + dt).min(16. * STEP);
        while self.accumulator >= STEP {
            self.world.step(); self.accumulator -= STEP; self.elapsed += STEP;
        }
    }

    /// Source world-space matrices; unbound fingers/accessories follow their
    /// nearest physical ancestor using their captured local animation pose.
    pub fn bone_matrices(&self) -> Vec<Mat4> {
        let bound: HashMap<_,_> = self.rig.bone_ids.iter().copied().zip(self.handles.iter()).collect();
        let mut out = Vec::with_capacity(self.rig.bones.len());
        for (i, bone) in self.rig.bones.iter().enumerate() {
            let m = if let Some(handle) = bound.get(&i) { matrix(self.world.bodies[**handle].position()) }
                else if bone.parent >= 0 { out[bone.parent as usize] * self.local[i] }
                else { self.initial[i] };
            out.push(if m.is_finite() { m } else { self.initial[i] });
        }
        out
    }
}

fn overlaps(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> bool {
    a.cmple(d).all() && b.cmpge(c).all()
}

/// Clip a bounded polyhedron by the brush halfspaces. Some prop hulls have
/// thousands of bevel planes: enumerating plane triples is catastrophically
/// expensive. Incremental clipping retains slopes without using an AABB proxy.
fn vertices(convex: &collision::Convex) -> Vec<Vec3> {
    let corners: Vec<_> = (0..8).map(|i| Vec3::new(
        if i&1==0 {convex.mins.x} else {convex.maxs.x},
        if i&2==0 {convex.mins.y} else {convex.maxs.y},
        if i&4==0 {convex.mins.z} else {convex.maxs.z})).collect();
    let mut faces: Vec<Vec<Vec3>> = [[0,1,3,2],[4,6,7,5],[0,4,5,1],[2,3,7,6],[0,2,6,4],[1,5,7,3]]
        .iter().map(|face| face.iter().map(|i| corners[*i]).collect()).collect();
    for plane in &convex.planes {
        let mut cut: Vec<Vec3> = Vec::new();
        let mut next = Vec::new();
        for face in &faces {
            let mut clipped = Vec::new();
            let mut previous = *face.last().unwrap();
            let mut previous_distance = plane.normal.dot(previous)-plane.dist;
            for &current in face {
                let distance = plane.normal.dot(current)-plane.dist;
                if (distance <= 0.001) != (previous_distance <= 0.001) {
                    let p = previous.lerp(current, (previous_distance/(previous_distance-distance)).clamp(0.,1.));
                    clipped.push(p);
                    if cut.iter().all(|q| q.distance_squared(p)>0.00001) {cut.push(p);}
                }
                if distance <= 0.001 {clipped.push(current);}
                previous=current; previous_distance=distance;
            }
            if clipped.len()>=3 {next.push(clipped);}
        }
        if cut.len()>=3 {
            let center=cut.iter().copied().sum::<Vec3>()/cut.len() as f32;
            let axis=plane.normal.cross(if plane.normal.z.abs()<0.9 {Vec3::Z} else {Vec3::X}).normalize_or_zero();
            let up=plane.normal.cross(axis);
            cut.sort_by(|a,b| {
                let a=*a-center;let b=*b-center;
                a.dot(up).atan2(a.dot(axis)).total_cmp(&b.dot(up).atan2(b.dot(axis)))
            });
            next.push(cut);
        }
        faces=next;
        if faces.is_empty() {break;}
    }
    let mut out: Vec<Vec3> = Vec::new();
    for p in faces.into_iter().flatten() {
        if p.is_finite() && out.iter().all(|q|q.distance_squared(p)>0.00001) {out.push(p);}
    }
    out
}

fn add_static(world: &mut PhysicsWorld, collision: &collision::CollisionWorld, origin: Vec3) {
    let low = origin - Vec3::splat(WORLD_RADIUS);
    let high = origin + Vec3::splat(WORLD_RADIUS);
    let mut parts = Vec::new();
    for c in &collision.convexes {
        if c.contents & collision::contents::MASK_SOLID == 0 || !overlaps(c.mins,c.maxs,low,high) { continue; }
        let points: Vec<_> = vertices(c).iter().map(|p| vector(*p * SCALE)).collect();
        if let Some(shape) = SharedShape::convex_hull(&points) { parts.push((Pose::IDENTITY, shape)); }
    }
    let mut triangle_points = Vec::new();
    let mut triangle_indices = Vec::new();
    for t in &collision.triangles {
        if t.contents & collision::contents::MASK_SOLID == 0 || !overlaps(t.mins,t.maxs,low,high) { continue; }
        let i=triangle_points.len() as u32;
        triangle_points.extend(t.v.iter().map(|p|vector(*p*SCALE)));
        triangle_indices.push([i,i+1,i+2]);
    }
    if !triangle_indices.is_empty() {
        if let Ok(shape) = SharedShape::trimesh(triangle_points, triangle_indices) {
            world.colliders.insert(static_collider(ColliderBuilder::new(shape)));
        }
    }
    if !parts.is_empty() {world.colliders.insert(static_collider(ColliderBuilder::compound(parts)));}
}

fn static_collider(builder: ColliderBuilder) -> ColliderBuilder {
    builder.friction(0.6).collision_groups(InteractionGroups::new(Group::GROUP_1, Group::ALL, InteractionTestMode::And))
}

#[cfg(test)]
mod verification_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn floor() -> collision::CollisionWorld {
        let mut world = collision::CollisionWorld::new();
        world.add_convex(collision::Convex { plane_surfaces: Vec::new(),
            planes: vec![
                collision::Plane {normal: Vec3::X, dist: 512.}, collision::Plane {normal: -Vec3::X, dist: 512.},
                collision::Plane {normal: Vec3::Y, dist: 512.}, collision::Plane {normal: -Vec3::Y, dist: 512.},
                collision::Plane {normal: Vec3::Z, dist: 0.}, collision::Plane {normal: -Vec3::Z, dist: 32.},
            ], mins: Vec3::new(-512.,-512.,-32.), maxs: Vec3::new(512.,512.,0.),
            contents: collision::contents::SOLID, surface: 0,
        });
        world.finalize(); world
    }

    #[test]
    fn plane_intersections_preserve_floor_and_slope_geometry() {
        let world = floor();
        let mut convex = world.convexes[0].clone();
        let vertices = super::vertices(&convex);
        assert_eq!(vertices.len(), 8);
        assert!(vertices.iter().all(|p| p.z <= 0. && p.z >= -32.));
        convex.planes[4] = collision::Plane {normal: Vec3::new(1.,0.,1.).normalize(), dist: 0.};
        let vertices = super::vertices(&convex);
        assert!(vertices.iter().all(|p| p.x+p.z <= 0.05));
        assert!(vertices.iter().any(|p| (p.x+p.z).abs() < 0.05));
    }

    #[test]
    #[ignore = "requires read-only installed CS:GO Legacy models"]
    fn installed_bodies_fall_settle_and_preserve_bone_lengths() {
        let (_,fs) = level::mount_game(None).unwrap();
        for name in ["ctm_sas", "tm_phoenix"] {
            let base=format!("models/player/custom_player/legacy/{name}");
            let bytes=fs.read(&format!("{base}.mdl")).unwrap();
            let model=mdl::Model::load_header(&bytes).unwrap();
            let rig=Arc::new(Rig::load(&model,&bytes,&fs.read(&format!("{base}.phy")).unwrap()).unwrap());
            let pose=mdl::skeleton::model_space(&model.bones,&mdl::skeleton::rest_pose(&model.bones));
            let mut corpse=Corpse::new(rig.clone(),&floor(),&pose,Vec3::Z*40.,0.,Vec3::new(60.,0.,0.),None).unwrap();
            for _ in 0..(128*8) { corpse.advance(STEP); }
            let result=corpse.bone_matrices();
            for (index,bone) in rig.bone_ids.iter().enumerate() {
                let p=result[*bone].w_axis.truncate();
                assert!(p.is_finite() && p.z > -20. && p.z < 70., "{name} bone {index} = {p:?}");
            }
            let pelvis=result[rig.bone_ids[0]].w_axis.z;
            assert!(pelvis < 30., "{name} must fall, pelvis {pelvis}");
            for joint in &rig.data.constraints {
                let a=rig.bone_ids[joint.parent.0]; let b=rig.bone_ids[joint.child.0];
                let initial=pose[a].w_axis.truncate().distance(pose[b].w_axis.truncate());
                let actual=result[a].w_axis.truncate().distance(result[b].w_axis.truncate());
                assert!((actual-initial).abs() < 3., "{name} joint {a}/{b} initial{initial} actual{actual}");
            }
            println!("{name}: physical bones={} pelvis={pelvis:.2} failures: 0",rig.bone_ids.len());
        }
    }
}

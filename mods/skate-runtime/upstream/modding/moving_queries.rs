//! Solid dual-world boundary. Native local bones participate in BoardStep;
//! remote actor mirrors participate in Rapier. Never solve the local actor in
//! both worlds, and never feed reactions back to another peer's body.
use bevy::prelude::*;
use skate_core::{math::Vector3, physics::{
    board_step::CollisionBody,
    board_world::{ExternalLineHit, ExternalQueries, WorldLineHit},
    triangle_query::TriangleLineHit,
}};
use skate_dynamics::{SolidBody, solid::{collider_triangles, sweep_sphere}};
use std::{collections::BTreeSet, sync::Arc};

fn xyz(v: Vector3) -> [f32;3] { [v.x,v.y,v.z] }
fn native(v: [f32;3]) -> Vector3 { Vector3::new(v[0],v[1],v[2]) }

pub(crate) struct MovingQueries(pub Vec<SolidBody>);
impl ExternalQueries for MovingQueries {
    fn line(&self, start: Vector3, end: Vector3, radius: f32) -> Option<ExternalLineHit> {
        let (id, c) = sweep_sphere(&self.0, xyz(start), xyz(end), radius)?;
        let body = self.0.iter().find(|b| b.id == id)?;
        let q = body.pose.rotation;
        let transform = Mat4::from_rotation_translation(
            Quat::from_xyzw(q.x,q.y,q.z,q.w), Vec3::from_array(body.pose.translation.to_array()));
        Some(ExternalLineHit {
            hit: WorldLineHit { geometry: TriangleLineHit {
                position: native(c.point_b.to_array()), normal: native(c.normal.to_array()),
                fraction: c.time_of_impact, volume_parameter: [0.;3],
            }, tag: 0 }, surface: 0, geometry_id: 0x8000_0000 | (id as u32 & 0x7fff_ffff),
            frame: transform.to_cols_array_2d(),
        })
    }
    fn nearby(&self, center: Vector3, radius: f32) -> Vec<[Vector3;3]> {
        let center = Vec3::from_array(xyz(center));
        if !center.is_finite() || !radius.is_finite() || radius < 0. { return Vec::new(); }
        let mut triangles = Vec::new();
        for body in &self.0 {
            for collider in &body.colliders {
                let aabb = collider.shape.compute_aabb(&collider.pose);
                let min = Vec3::from_array(aabb.mins.to_array());
                let max = Vec3::from_array(aabb.maxs.to_array());
                if center.distance_squared(center.clamp(min,max)) > radius * radius { continue; }
                for t in collider_triangles(collider) {
                    let verts = t.map(Vec3::from_array);
                    let min = verts[0].min(verts[1]).min(verts[2]);
                    let max = verts[0].max(verts[1]).max(verts[2]);
                    if center.distance_squared(center.clamp(min,max)) <= radius*radius {
                        triangles.push(t.map(native));
                        if triangles.len() >= 64 { return triangles; }
                    }
                }
            }
        }
        triangles
    }
}


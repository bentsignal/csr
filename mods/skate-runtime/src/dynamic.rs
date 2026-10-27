// SPDX-License-Identifier: GPL-3.0-only
//! Feed CS-owned kinematic doors and intact breakables through the upstream
//! solid/native contact boundary. Their scene never recompiles the map mesh.
use crate::{
    modding::moving_queries::MovingQueries,
    physics::{GamePhysics, SkaterRuntime},
};
use mechanics_link::{Scene, SceneState};
use skate_dynamics::{
    SolidBody, SolidCollider,
    rapier3d::prelude::{Pose, Rotation, SharedShape, Vector},
};
pub(crate) struct Templates {
    doors: Vec<Object>,
    breakables: Vec<(Vec<usize>, Object)>,
}
struct Object {
    origin: Vector,
    parts: Vec<SharedShape>,
    radius: f32,
}
fn native(p: [f32; 3]) -> Vector {
    Vector::new(p[0] * 0.0254, p[2] * 0.0254, -p[1] * 0.0254)
}
fn object(parts: &[Vec<[f32; 3]>], origin: [f32; 3]) -> Result<Object, String> {
    let origin = native(origin);
    let mut radius = 0f32;
    let mut shapes = vec![];
    for part in parts {
        let points: Vec<_> = part.iter().map(|p| native(*p) - origin).collect();
        for p in &points {
            radius = radius.max(p.length());
        }
        if points.len() < 4 {
            continue;
        }
        shapes.push(SharedShape::convex_hull(&points).ok_or("invalid CS dynamic convex hull")?);
    }
    Ok(Object {
        origin,
        parts: shapes,
        radius,
    })
}
impl Templates {
    pub(crate) fn new(scene: &Scene) -> Result<Self, String> {
        let doors = scene
            .doors
            .iter()
            .map(|d| object(&d.parts, d.origin))
            .collect::<Result<_, _>>()?;
        let mut breakables = vec![];
        for b in &scene.breakables {
            let points: Vec<_> = b.parts.iter().flatten().copied().collect();
            let mut center = [0.; 3];
            for p in &points {
                for i in 0..3 {
                    center[i] += p[i] / points.len() as f32;
                }
            }
            breakables.push((b.killed_by.clone(), object(&b.parts, center)?));
        }
        Ok(Self { doors, breakables })
    }
    pub(crate) fn apply(
        &self,
        state: &SceneState,
        previous: &SceneState,
        dt: f32,
        physics: &mut GamePhysics,
        skater: &SkaterRuntime,
    ) {
        let friction = physics.csr_floor_material().static_friction;
        let mut solids = vec![];
        for (i, object) in self.doors.iter().enumerate() {
            let angle = state.doors.get(i).copied().unwrap_or(0.);
            let old = previous.doors.get(i).copied().unwrap_or(angle);
            let rotation = Rotation::from_scaled_axis(Vector::Y * angle.to_radians());
            solids.push(body(
                i as u64,
                object,
                rotation,
                Vector::Y * ((angle - old).to_radians() / dt),
                friction,
            ));
        }
        for (i, (killed, object)) in self.breakables.iter().enumerate() {
            if !killed
                .iter()
                .any(|i| state.broken.get(*i).copied().unwrap_or(false))
            {
                solids.push(body(
                    (self.doors.len() + i) as u64,
                    object,
                    Rotation::IDENTITY,
                    Vector::ZERO,
                    friction,
                ));
            }
        }
        // Camera, wheel lines, walking and edge queries use the same shapes.
        physics.set_external_queries(Some(std::sync::Arc::new(MovingQueries(solids.clone()))));
        let center = physics.board.bodies()[skate_core::physics::board::BodyId::Deck.index()]
            .rates
            .position;
        let center = Vector::new(center.x, center.y, center.z);
        let mut proxies = crate::physics::network::Proxies::default();
        for solid in solids {
            let radius = self
                .doors
                .iter()
                .chain(self.breakables.iter().map(|(_, o)| o))
                .nth(solid.id as usize)
                .map_or(0., |o| o.radius);
            if solid.center_of_mass.distance_squared(center) <= (radius + 8.).powi(2) {
                proxies.append_solid(solid, physics, skater, false);
            }
        }
        physics.network_proxies = proxies;
    }
}
fn body(id: u64, object: &Object, rotation: Rotation, angular: Vector, friction: f32) -> SolidBody {
    let pose = Pose::from_parts(object.origin, rotation);
    SolidBody {
        id,
        pose,
        center_of_mass: object.origin,
        inertia_rotation: rotation,
        inverse_mass: 0.,
        inverse_inertia: Vector::ZERO,
        linvel: Vector::ZERO,
        angvel: angular,
        contact_group: 0,
        colliders: object
            .parts
            .iter()
            .map(|shape| SolidCollider {
                shape: shape.clone(),
                pose,
                friction,
            })
            .collect(),
    }
}

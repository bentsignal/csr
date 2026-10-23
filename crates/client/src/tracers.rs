//! Cosmetic tracers follow the rendered muzzle; the simulation owns hit rays.
use crate::{
    coords::{to_bevy, to_source},
    player::{LocalPlayer, SimWorld},
};
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Default, PartialEq)]
pub enum InspectAction {
    #[default]
    Start,
    Cancel,
}

/// Local presentation requests; cancellation never changes weapon cooldowns.
#[derive(Resource, Default)]
pub struct InspectRequest {
    serial: u64,
    action: InspectAction,
}

impl InspectRequest {
    pub fn start(&mut self) { self.serial = self.serial.wrapping_add(1); self.action = InspectAction::Start; }
    pub fn cancel(&mut self) { self.serial = self.serial.wrapping_add(1); self.action = InspectAction::Cancel; }
    pub fn serial(&self) -> u64 { self.serial }
    pub fn after(&self, seen: &mut u64) -> Option<InspectAction> {
        let changed = *seen != self.serial;
        *seen = self.serial;
        changed.then_some(self.action)
    }
}
#[derive(Resource, Default)]
pub struct Muzzles {
    pub first: Option<(sim::PlayerId, Vec3)>,
    pub world: HashMap<sim::PlayerId, Vec3>,
    pub first_pair: Option<(sim::PlayerId, [Option<Vec3>; 2])>,
    pub world_pairs: HashMap<sim::PlayerId, [Option<Vec3>; 2]>,
}
impl Muzzles {
    /// Each batched shot retains its barrel instead of following the final
    /// shot's selected attachment for every event in that render frame.
    pub(crate) fn position(&self, owner: sim::PlayerId, first: bool, left: Option<bool>) -> Option<Vec3> {
        let pair = if first { self.first_pair.filter(|(id, _)| *id == owner).map(|(_, pair)| pair) }
            else { self.world_pairs.get(&owner).copied() };
        if let Some(left) = left {
            if let Some(pair) = pair { return pair[usize::from(left)]; }
        }
        if first { self.first.filter(|(id, _)| *id == owner).map(|(_, p)| p) }
            else { self.world.get(&owner).copied() }
    }
}
struct Trail {
    start: Vec3,
    end: Vec3,
    born: f32,
}
#[derive(Resource, Default)]
pub struct Tracers {
    pub pending: Vec<(sim::PlayerId, glam::Vec3, glam::Vec3, Option<bool>)>,
    live: Vec<Trail>,
}
/// Prefer the authored attachment, including its offset from the parent bone.
pub fn muzzle_attachment(model: &mdl::Model, left: bool) -> Option<(usize, glam::Vec3)> {
    let name = if left { "2" } else { "1" };
    model
        .attachments
        .iter()
        .find(|a| a.name == name)
        // The Elite viewmodel names its barrels 1/2; the held world model
        // instead exports muzzle_flash/muzzle_flash2 for right/left.
        .or_else(|| left.then(|| model.attachments.iter().find(|a| a.name == "muzzle_flash2")).flatten())
        .or_else(|| model.attachments.iter().find(|a| a.name == "muzzle_flash"))
        .map(|a| (a.bone, a.position))
        .or_else(|| {
            model
                .bone_index("weapon_muzzle")
                .or_else(|| {
                    model.bones.iter().position(|b| {
                        let name = b.name.to_ascii_lowercase();
                        name.contains("flash") || name.ends_with("muzzle")
                    })
                })
                .map(|i| (i, glam::Vec3::ZERO))
        })
}
pub fn muzzle_position(model: &mdl::Model, pose: &[glam::Mat4], left: bool) -> Option<glam::Vec3> {
    let (bone, offset) = muzzle_attachment(model, left)?;
    Some(pose.get(bone)?.transform_point3(offset))
}
/// Compensate for the separate viewmodel FOV so the world trail starts at the
/// same screen pixel as the visible barrel, including scoped-rifle ADS.
pub fn project_viewmodel_muzzle(
    muzzle: Vec3,
    eye: Vec3,
    forward: Vec3,
    world_fov: f32,
    view_fov: f32,
) -> Vec3 {
    let delta = muzzle - eye;
    let axial = forward * delta.dot(forward);
    eye + axial + (delta - axial) * ((world_fov * 0.5).tan() / (view_fov * 0.5).tan())
}
pub fn segment(start: Vec3, end: Vec3, age: f32) -> Option<(Vec3, Vec3, f32)> {
    let distance = start.distance(end);
    if distance < 1.0 || age < 0.0 || age > 0.12 {
        return None;
    }
    let tail = age * 7000.0;
    if tail >= distance {
        return None;
    }
    let direction = (end - start) / distance;
    Some((
        start + direction * tail,
        start + direction * (tail + 85.0).min(distance),
        0.25 * (1.0 - age / 0.12),
    ))
}
pub fn draw(
    mut trails: ResMut<Tracers>,
    muzzles: Res<Muzzles>,
    local: Res<LocalPlayer>,
    world: Res<SimWorld>,
    time: Res<Time>,
    mut gizmos: Gizmos,
) {
    let now = time.elapsed_secs();
    let pending = std::mem::take(&mut trails.pending);
    for (owner, eye, end, left) in pending {
        let muzzle = muzzles.position(owner, owner == local.view_target() && !local.noclip, left);
        let Some(mut start) = muzzle else { continue };
        // Do not let a cosmetic muzzle origin poke through nearby cover.
        let trace =
            world
                .0
                .collision
                .trace_ray(eye, to_source(start), collision::contents::MASK_SHOT);
        if trace.hit() {
            start = to_bevy(trace.end + trace.normal * 0.5);
        }
        let end = to_bevy(end);
        if (end - start).dot(end - to_bevy(eye)) <= 0.0 {
            continue;
        }
        if trails.live.len() < 256 {
            trails.live.push(Trail {
                start,
                end,
                born: now,
            });
        }
    }
    trails
        .live
        .retain(|t| segment(t.start, t.end, now - t.born).is_some());
    for trail in &trails.live {
        if let Some((a, b, alpha)) = segment(trail.start, trail.end, now - trail.born) {
            gizmos.line(a, b, Color::srgba(0.48, 0.44, 0.34, alpha));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_cover_no_longer_clamps_the_rendered_muzzle() {
        use bevy::{ecs::system::RunSystemOnce, gizmos::{AppGizmoBuilder, config::DefaultGizmoConfigGroup}};
        use collision::{CollisionWorld, Convex, Plane};
        use std::sync::Arc;

        let solid = |lo: glam::Vec3, hi: glam::Vec3| Convex { plane_surfaces: Vec::new(),
            planes: vec![
                Plane { normal: glam::Vec3::X, dist: hi.x },
                Plane { normal: -glam::Vec3::X, dist: -lo.x },
                Plane { normal: glam::Vec3::Y, dist: hi.y },
                Plane { normal: -glam::Vec3::Y, dist: -lo.y },
                Plane { normal: glam::Vec3::Z, dist: hi.z },
                Plane { normal: -glam::Vec3::Z, dist: -lo.z },
            ],
            mins: lo, maxs: hi, contents: collision::contents::SOLID, surface: 0,
        };
        let mut collision = CollisionWorld::new();
        collision.breakables = vec![collision::breakables::Breakable {
            entity: 1, model: 1, origin: glam::Vec3::ZERO, health: 1., material: 0,
            prop_data: String::new(), bullet_scale: 1., club_scale: 1., minimum_damage: 0.,
            trigger_only: false, studio: None,
            solids: vec![solid(glam::Vec3::new(8., -16., 48.), glam::Vec3::new(12., 16., 80.))],
        }].into();
        collision.finalize();
        let world = sim::World::new(Arc::new(collision));
        let frozen = world.collision.clone();
        let eye = glam::Vec3::new(0., 0., 64.);
        let muzzle = to_bevy(eye + glam::Vec3::X * 32.);
        let end = eye + glam::Vec3::X * 96.;
        let mut app = App::new();
        app.insert_resource(SimWorld(world))
            .insert_resource(LocalPlayer::new(1, eye, 0.))
            .insert_resource(Muzzles { first: Some((1, muzzle)), ..default() })
            .init_gizmo_group::<DefaultGizmoConfigGroup>()
            .init_resource::<Time>()
            .init_resource::<Tracers>();
        for removed in [false, true, false] {
            {
                let mut world = app.world_mut().resource_mut::<SimWorld>();
                let mut state = world.0.state();
                state.breakables[0] = sim::breakables::State {
                    health: if removed { 0. } else { 1. }, destroyed: removed,
                };
                world.0.restore(state);
            }
            let authority = app.world().resource::<SimWorld>().0.state();
            let mut trails = app.world_mut().resource_mut::<Tracers>();
            trails.live.clear();
            trails.pending.push((1, eye, end, None));
            drop(trails);
            app.world_mut().run_system_once(draw).unwrap();
            let trails = app.world().resource::<Tracers>();
            assert!(trails.pending.is_empty());
            assert_eq!(trails.live.len(), 1);
            let start = to_source(trails.live[0].start);
            assert!((start.x - if removed { 32. } else { 7.5 }).abs() < 0.1,
                "the actual draw must release the muzzle after cover breaks, and clip it after restore: {start:?}");
            assert_eq!(trails.live[0].end, to_bevy(end));
            assert!(frozen.trace_ray(eye, to_source(muzzle), collision::contents::MASK_SHOT).hit());
            assert_eq!(app.world().resource::<SimWorld>().0.state(), authority);
        }
        // Even though the current animation selects only the last barrel,
        // two queued shots must leave from different authored attachments.
        let right = to_bevy(eye + glam::Vec3::new(4., -4., 0.));
        let left = to_bevy(eye + glam::Vec3::new(4., 4., 0.));
        {
            let mut muzzles = app.world_mut().resource_mut::<Muzzles>();
            muzzles.first = Some((1, right));
            muzzles.first_pair = Some((1, [Some(right), Some(left)]));
        }
        {
            let mut trails = app.world_mut().resource_mut::<Tracers>();
            trails.live.clear();
            trails.pending.extend([(1, eye, end, Some(true)), (1, eye, end, Some(false))]);
        }
        let authority = app.world().resource::<SimWorld>().0.state();
        app.world_mut().run_system_once(draw).unwrap();
        let trails = app.world().resource::<Tracers>();
        assert_eq!(trails.live.len(), 2);
        assert_eq!(trails.live[0].start, left);
        assert_eq!(trails.live[1].start, right);
        assert_eq!(app.world().resource::<SimWorld>().0.state(), authority);
    }
    #[test]
    fn muzzle_uses_attachment_offset_in_animated_bone_space() {
        let model=mdl::Model {attachments:vec![mdl::Attachment{name:"1".into(),bone:0,position:glam::Vec3::new(0.0,3.0,9.0),rotation:glam::Mat3::IDENTITY}],..Default::default()};
        let pose=[glam::Mat4::from_translation(glam::Vec3::new(10.0,20.0,30.0))];
        assert_eq!(muzzle_position(&model,&pose,false),Some(glam::Vec3::new(10.0,23.0,39.0)));
        assert!(muzzle_position(&model,&[],false).is_none());
    }
    #[test]
    fn world_left_muzzle_keeps_single_barrel_fallback_and_viewmodel_numbering() {
        let attachment = |name: &str, x| mdl::Attachment { name: name.into(), bone: 0,
            position: glam::Vec3::X * x, rotation: glam::Mat3::IDENTITY };
        let mut model = mdl::Model { attachments: vec![attachment("muzzle_flash", 1.),
            attachment("muzzle_flash2", 2.)], ..Default::default() };
        let pose = [glam::Mat4::IDENTITY];
        assert_eq!(muzzle_position(&model, &pose, false), Some(glam::Vec3::X));
        assert_eq!(muzzle_position(&model, &pose, true), Some(glam::Vec3::X * 2.));
        model.attachments.push(attachment("2", 3.));
        assert_eq!(muzzle_position(&model, &pose, true), Some(glam::Vec3::X * 3.));
        model.attachments.truncate(1);
        assert_eq!(muzzle_position(&model, &pose, true), Some(glam::Vec3::X));
    }
    #[test]
    fn trail_starts_at_muzzle_and_never_overshoots_hit() {
        let start = Vec3::new(20.0, -3.0, 2.0);
        let end = start + Vec3::X * 40.0;
        let (a, b, alpha) = segment(start, end, 0.0).unwrap();
        assert_eq!(a, start);
        assert_eq!(b, end);
        assert!(alpha <= 0.25);
        assert!(segment(start, end, 0.01).is_none());
    }
    #[test]
    fn corrected_muzzle_has_same_screen_projection() {
        let p = Vec3::new(4.0, 2.0, -25.0);
        let wf = 1.28;
        let vf = 0.82;
        let q = project_viewmodel_muzzle(p, Vec3::ZERO, Vec3::NEG_Z, wf, vf);
        assert!((p.x / (-p.z * (vf / 2.0).tan()) - q.x / (-q.z * (wf / 2.0).tan())).abs() < 0.0001);
    }
}

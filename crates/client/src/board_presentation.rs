//! Held equipment pose and inspection, separate from the tick-owned board.
use crate::{
    player::{LocalPlayer, SimWorld},
    skinning::{self, SkinnedInstance},
    tracers::{InspectAction, InspectRequest},
};
use bevy::prelude::*;

pub(crate) struct Grip {
    pub pose: Vec<glam::Mat4>,
    pub parents: Vec<i32>,
    pub hand: usize,
    pub elbow: usize,
    pub shoulder: usize,
    pub twists: Vec<usize>,
    pub contact: glam::Mat4,
}
impl Grip {
    fn pose(&self, board: Mat4, rail: Vec3, f: Vec3, r: Vec3, u: Vec3) -> Vec<glam::Mat4> {
        // The rail passes through the closed grip, midway between the trucks.
        let contact = board * Mat4::from_translation(rail);
        let wrist = contact * self.contact.inverse();
        let end = wrist.w_axis.truncate();
        let start = end - f * 11. - r * 3. - u * 18.5;
        let upper = (self.pose[self.elbow].w_axis - self.pose[self.shoulder].w_axis).length();
        let lower = (self.pose[self.hand].w_axis - self.pose[self.elbow].w_axis).length();
        let direction = (end - start).normalize_or_zero();
        let distance = (end - start)
            .length()
            .clamp(0.01, (upper + lower - 0.01).max(0.01));
        let along = ((upper * upper - lower * lower + distance * distance) / (2. * distance))
            .clamp(0., upper);
        let bend = (r - u * 0.8) - direction * direction.dot(r - u * 0.8);
        let elbow = start
            + direction * along
            + bend.normalize_or_zero() * (upper * upper - along * along).max(0.).sqrt();
        let align = |from: usize, to: usize, a: Vec3, b: Vec3| {
            let old = (self.pose[to].w_axis - self.pose[from].w_axis)
                .truncate()
                .normalize_or_zero();
            let rotation = Quat::from_rotation_arc(old, (b - a).normalize_or_zero());
            Mat4::from_translation(a)
                * Mat4::from_quat(rotation)
                * Mat4::from_translation(-self.pose[from].w_axis.truncate())
        };
        let upper_transform = align(self.shoulder, self.elbow, start, elbow);
        let lower_transform = align(self.elbow, self.hand, elbow, end);
        let hand_transform = wrist * self.pose[self.hand].inverse();
        let delta = hand_transform.to_scale_rotation_translation().1
            * lower_transform.to_scale_rotation_translation().1.inverse();
        let axis = (end - elbow).normalize();
        let projection = axis * delta.xyz().dot(axis);
        let twist = Quat::from_xyzw(projection.x, projection.y, projection.z, delta.w);
        let twist = if twist.length_squared() > 1e-10 {
            twist.normalize()
        } else {
            Quat::IDENTITY
        };
        let twist_transform = Mat4::from_translation(elbow)
            * Mat4::from_quat(Quat::IDENTITY.slerp(twist, 0.65))
            * Mat4::from_translation(-elbow)
            * lower_transform;
        self.pose
            .iter()
            .enumerate()
            .map(|(i, pose)| {
                if self.twists.contains(&i) {
                    return twist_transform * *pose;
                }
                let mut at = i as i32;
                while at >= 0 {
                    let index = at as usize;
                    if index == self.hand {
                        return hand_transform * *pose;
                    }
                    if index == self.elbow {
                        return lower_transform * *pose;
                    }
                    if index == self.shoulder {
                        return upper_transform * *pose;
                    }
                    at = self.parents[index];
                }
                upper_transform * *pose
            })
            .collect()
    }
}

#[derive(Resource, Default)]
pub(crate) struct HeldBoard {
    pub transform: Option<Mat4>,
    seen: u64,
    controller_previous: u16,
    started: Option<f32>,
    equipped_at: f32,
    key: Option<(sim::PlayerId, u64, String)>,
    arms: Option<(sim::Team, SkinnedInstance, Grip)>,
}
pub(crate) fn update(
    mut commands: Commands,
    world: Res<SimWorld>,
    local: Res<LocalPlayer>,
    time: Res<Time>,
    models: Res<crate::viewmodel::ViewModels>,
    request: Res<InspectRequest>,
    mut held: ResMut<HeldBoard>,
    mut materials: ResMut<Assets<crate::material::WorldMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut transforms: Query<&mut Transform>,
    mut visible: Query<&mut Visibility>,
    material_query: Query<&MeshMaterial3d<crate::material::WorldMaterial>>,
    level: Res<crate::LoadedLevel>,
    characters: Res<crate::characters::CharacterModels>,
    visuals: Res<crate::movement_modes::Visuals>,
) {
    held.transform = None;
    if let Some((_, arms, _)) = &held.arms {
        for mesh in &arms.meshes {
            if let Ok(mut v) = visible.get_mut(*mesh) {
                *v = Visibility::Hidden;
            }
        }
    }
    let mut action = request.after(&mut held.seen);
    // Reuse the tick's XInput publication; inspection needs no extra hardware
    // polling and cannot alter the admitted controller report.
    let buttons = local.controller_buttons;
    if buttons & 0x40 != 0 && held.controller_previous & 0x40 == 0 {
        action = Some(InspectAction::Start);
    }
    held.controller_previous = buttons;
    let Some(p) = world.0.player(local.id).filter(|p| {
        p.alive
            && p.mechanic.is_none()
            && !local.render_third_person
            && !local.noclip
            && local.spectating.is_none()
    }) else {
        held.key = None;
        held.started = None;
        return;
    };
    let Some(item) = p.equipped_mechanic.as_ref().filter(|i| !i.used) else {
        held.key = None;
        held.started = None;
        return;
    };
    let now = time.elapsed_secs();
    let key = (p.id, p.life_epoch, item.id.clone());
    if held.key.as_ref() != Some(&key) {
        held.key = Some(key);
        held.equipped_at = now;
        held.started = None;
    }
    match action {
        Some(InspectAction::Start) => held.started = Some(now),
        Some(InspectAction::Cancel) => held.started = None,
        _ => {}
    }
    if std::env::var_os("CSR_TEST_BOARD_INSPECT").is_some() && held.started.is_none() {
        held.started = Some(now);
    }
    let inspection = held.started.map_or(0., |start| {
        let t = ((now - start) / 3.2).clamp(0., 1.);
        // Ease in, turn over to examine the underside/trucks, then return.
        let t = (t * std::f32::consts::PI).sin().powi(2);
        t * t * (3. - 2. * t)
    });
    let inspection = std::env::var("CSR_TEST_BOARD_INSPECT_PHASE")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .map_or(inspection, |v| v.clamp(0., 1.));
    let deploy = (1. - ((now - held.equipped_at) / 0.28).clamp(0., 1.)).powi(2);
    let (f, r, u) = movement::angle_vectors(local.render_angles);
    let basis = Mat3::from_cols(-r, -f, u)
        * Mat3::from_quat(Quat::from_euler(
            EulerRot::XYZ,
            -0.12,
            -0.48 + inspection * 0.22,
            0.35 + inspection * std::f32::consts::PI,
        ));
    let bob = if p.movement.on_ground {
        (world.0.time() * 6.).sin() * (p.movement.velocity.length() / 250.).min(1.) * 0.3
    } else {
        0.
    };
    let reach = (inspection * std::f32::consts::PI).sin();
    let center =
        local.render_eye + f * (36. + inspection * 5. - reach * 6.) + r * (10. - inspection * 5.)
            - u * (10. + deploy * 24. + reach * 3. - bob);
    let matrix = Mat4::from_mat3(basis);
    let matrix = Mat4::from_translation(center) * matrix;
    held.transform = Some(matrix);
    let respawn = held.arms.as_ref().is_none_or(|(team, arms, _)| {
        *team != p.team || arms.joints.first().is_none_or(|e| !transforms.contains(*e))
    });
    if respawn {
        if let Some((_, old, _)) = held.arms.take() {
            for e in old.joints.into_iter().chain(old.meshes) {
                commands.entity(e).despawn();
            }
        }
        if let Some((asset, grip)) = models.board_grip(p.team, &mut meshes) {
            let arms = skinning::spawn(&mut commands, &asset, None, Some(&mut materials));
            for e in &arms.meshes {
                commands.entity(*e).insert(Visibility::Hidden);
            }
            held.arms = Some((p.team, arms, grip));
        }
        return;
    }
    if let Some((_, arms, grip)) = &held.arms {
        let rail = visuals
            .1
            .get(&item.id)
            .map_or(Vec3::new(-3.9, 1.8, 0.), |m| m.grip);
        let pose = grip.pose(matrix, rail, f, r, u);
        skinning::apply_pose(arms, Mat4::IDENTITY, &pose, &mut transforms);
        let (cube, dir, color) = crate::characters::lighting_at_collision(
            &level.0,
            &world.0.collision,
            characters.sun,
            local.render_eye,
        );
        for e in &arms.meshes {
            if let Ok(mut v) = visible.get_mut(*e) {
                *v = Visibility::Inherited;
            }
            if let Ok(h) = material_query.get(*e) {
                if let Some(mut m) = materials.get_mut(&h.0) {
                    m.params.ambient_cube = cube;
                    m.params.sun_dir = dir;
                    m.params.sun_color = color;
                }
            }
        }
    }
}

//! Held equipment pose and inspection, separate from the tick-owned board.
use crate::{
    coords::to_bevy,
    player::{LocalPlayer, SimWorld},
    skinning::{self, SkinnedInstance},
    tracers::{InspectAction, InspectRequest},
};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct HeldBoard {
    pub transform: Option<Mat4>,
    seen: u64,
    controller_previous: u16,
    started: Option<f32>,
    equipped_at: f32,
    key: Option<(sim::PlayerId, u64, String)>,
    arms: Option<(
        sim::Team,
        SkinnedInstance,
        Vec<glam::Mat4>,
        glam::Mat4,
        Vec3,
    )>,
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
) {
    held.transform = None;
    if let Some((_, arms, _, _, _)) = &held.arms {
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
    let center = local.render_eye + f * (36. + inspection * 5.) + r * (10. - inspection * 5.)
        - u * (10. + deploy * 24. - inspection * 5. - bob);
    let matrix = Mat4::from_mat3(basis);
    let matrix = Mat4::from_translation(center) * matrix;
    held.transform = Some(matrix);
    let respawn = held.arms.as_ref().is_none_or(|(team, arms, _, _, _)| {
        *team != p.team || arms.joints.first().is_none_or(|e| !transforms.contains(*e))
    });
    if respawn {
        if let Some((_, old, _, _, _)) = held.arms.take() {
            for e in old.joints.into_iter().chain(old.meshes) {
                commands.entity(e).despawn();
            }
        }
        if let Some((asset, pose, hand, forearm)) = models.board_grip(p.team, &mut meshes) {
            let arms = skinning::spawn(&mut commands, &asset, None, Some(&mut materials));
            for e in &arms.meshes {
                commands.entity(*e).insert(Visibility::Hidden);
            }
            held.arms = Some((p.team, arms, pose, hand, forearm));
        }
        return;
    }
    if let Some((_, arms, pose, hand, forearm)) = &held.arms {
        // Wrist closes around the near rail; use the same rigid transform for
        // the board and hand so inspection never lets the grip drift apart.
        let grip = center + basis * Vec3::new(1.5, -0.7, -7.);
        // Turning the deck is wrist motion. Keep the forearm entering from
        // below the camera instead of spinning its cut end into the scene.
        let arm_basis = Mat3::from_cols(-r, -f, u)
            * Mat3::from_quat(Quat::from_euler(EulerRot::XYZ, -0.12, -0.48, 0.35));
        let rotation = arm_basis * Mat3::from_rotation_z(-1.2);
        let target = Mat4::from_rotation_translation(
            crate::coords::rotation_to_bevy(rotation),
            to_bevy(grip),
        );
        let hand =
            crate::skinning::axis_change() * *hand * crate::skinning::axis_change().inverse();
        let actual = target
            .transform_vector3(to_bevy(*forearm))
            .normalize_or_zero();
        let toward_body = to_bevy((-f * 0.7 + r * 0.6 - u).normalize());
        let adjust = if actual.length_squared() > 0.5 {
            Quat::from_rotation_arc(actual, toward_body)
        } else {
            Quat::IDENTITY
        };
        let pivot = to_bevy(grip);
        let arm = Mat4::from_translation(pivot)
            * Mat4::from_quat(adjust)
            * Mat4::from_translation(-pivot)
            * target
            * hand.inverse();
        skinning::apply_pose(arms, arm, pose, &mut transforms);
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

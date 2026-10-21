//! Shared grenade state drives bounded visuals; no separate client-side hit/visibility rules.
use crate::{
    coords::to_bevy,
    player::{LocalPlayer, SimWorld},
};
use bevy::prelude::*;
use sim::utility::Kind;
use std::collections::HashMap;

#[derive(Component)]
pub struct FlashOverlay;
struct Flying {
    instance: crate::skinning::SkinnedInstance,
    pose: Vec<glam::Mat4>,
    base: Mat4,
    rotation: Quat,
    last_time: f32,
}

/// A decoy keeps its shell after activation. Its field shares the projectile
/// id, so the existing posed body can survive that authoritative transition.
struct GrenadeBody {
    id: u32,
    kind: Kind,
    pos: glam::Vec3,
    velocity: glam::Vec3,
    settled: bool,
    born: f32,
}

fn grenade_bodies(world: &sim::World) -> impl Iterator<Item = GrenadeBody> + '_ {
    world.projectiles.iter().map(|p| GrenadeBody {
        id: p.id, kind: p.kind, pos: p.pos, velocity: p.velocity,
        settled: p.settled, born: p.born,
    }).chain(world.utility_fields.iter().filter(|f| f.kind == Kind::Decoy).map(|f| GrenadeBody {
        id: f.id, kind: f.kind, pos: f.pos, velocity: glam::Vec3::ZERO,
        settled: true, born: f.born,
    }))
}
#[derive(Resource)]
pub struct AssetsFx {
    smoke: Vec<crate::utility_particles::Layer>,
    fire: Vec<crate::utility_particles::Layer>,
    he: Vec<crate::utility_particles::Layer>,
    wick: Vec<crate::utility_particles::Layer>,
    held_wick: Option<((sim::PlayerId, u64, u64), crate::utility_particles::Emitter)>,
    fields: HashMap<u32, crate::utility_particles::Emitter>,
    explosions: Vec<crate::utility_particles::Emitter>,
    projectiles: HashMap<u32, Flying>,
    last_time: f32,
}
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<crate::particle_material::ParticleMaterial>>,
    mut images: ResMut<Assets<Image>>,
    level: Res<crate::LoadedLevel>,
) {
    let fs = &level.0.vfs;
    let smoke = crate::utility_particles::load(
        fs,
        "particles/explosions_fx.pcf",
        &[
            "explosion_child_smoke03d_ring",
            "explosion_child_smoke_bottom",
            "explosion_child_smoke07b",
            "explosion_child_smoke03e",
        ],
        &mut mats,
        &mut images,
        &["explosion_child_smoke07b"],
        false,
    );
    let fire = crate::utility_particles::load(
        fs,
        "particles/inferno_fx.pcf",
        &[
            "molotov_groundfire_child_base",
            "molotov_groundfire_main_center",
            "molotov_groundfire_main_fancy",
            "molotov_child_flame01a",
        ],
        &mut mats,
        &mut images,
        &[],
        true,
    );
    let he = crate::utility_particles::load(
        fs,
        "particles/explosions_fx.pcf",
        &[
            "explosion_child_core04b",
            "explosion_child_dust06a",
            "explosion_child_dust10a",
        ],
        &mut mats,
        &mut images,
        &["explosion_child_core04b", "explosion_child_dust06a", "explosion_child_dust10a"],
        false,
    );
    let wick = crate::utility_particles::load(
        fs,
        "particles/weapons/cs_weapon_fx.pcf",
        &["weapon_molotov_fp_wick"],
        &mut mats,
        &mut images,
        &[],
        false,
    );
    // Prewarm each material without a visible particle, so a first short HE
    // flash does not expire while its pipeline compiles.
    for layer in smoke.iter().chain(&fire).chain(&he).chain(&wick) {
        let mut mesh = Mesh::from(Rectangle::new(0.001, 0.001));
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0_f32; 4]; 4]);
        mesh.insert_attribute(
            crate::particle_material::ATTRIBUTE_NEXT_FRAME,
            vec![[0.0_f32; 3]; 4],
        );
        mesh.insert_attribute(crate::particle_material::ATTRIBUTE_CARD_UV, vec![[0.0_f32; 2]; 4]);
        let mesh = meshes.add(mesh);
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(layer.material.clone()),
            Transform::from_translation(Vec3::Y * -10000.0),
            bevy::camera::visibility::NoFrustumCulling,
        ));
    }
    commands.insert_resource(AssetsFx {
        smoke,
        fire,
        he,
        wick,
        held_wick: None,
        fields: HashMap::new(),
        explosions: Vec::new(),
        projectiles: HashMap::new(),
        last_time: 0.0,
    });
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::NONE),
        GlobalZIndex(90),
        FlashOverlay,
    ));
}

/// Ordered directly after viewmodel::update: use this frame's posed attachment,
/// on the same dedicated layer and FOV as the bottle and authored lighter.
pub fn update_wick(
    mut commands: Commands,
    world: Res<SimWorld>,
    local: Res<LocalPlayer>,
    sample: Res<crate::viewmodel::HeldWick>,
    mut fx: ResMut<AssetsFx>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut transforms: Query<&mut Transform>,
    level: Res<crate::LoadedLevel>,
) {
    let AssetsFx {
        wick, held_wick, ..
    } = &mut *fx;
    let current_key = sample.sample.map(|s| s.key);
    if held_wick.as_ref().map(|(key, _)| *key) != current_key {
        if let Some((_, emitter)) = held_wick.take() {
            emitter.despawn(&mut commands, &mut meshes);
        }
    }
    let Some(sample) = sample.sample else { return };
    let time = world.0.time();
    if held_wick.is_none() {
        let emitter = crate::utility_particles::Emitter::spawn(
            &mut commands,
            &mut meshes,
            wick,
            crate::coords::to_source(sample.position),
            time,
            time + 1.0,
            sample.key.2 as u32,
            false,
        );
        emitter.render_layers(&mut commands, crate::viewmodel::layer());
        *held_wick = Some((sample.key, emitter));
    }
    if let Some((_, emitter)) = held_wick {
        emitter.until = time + 1.0;
        emitter.follow(sample.position, &mut transforms);
        emitter.update(
            time,
            to_bevy(local.render_eye),
            wick,
            &mut meshes,
            &level.0.collision,
        );
    }
}
#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    world: Res<SimWorld>,
    local: Res<LocalPlayer>,
    mut fx: ResMut<AssetsFx>,
    characters: Res<crate::characters::CharacterModels>,
    mut mats: ResMut<Assets<crate::material::WorldMaterial>>,
    mut transforms: Query<&mut Transform>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut effects: ResMut<crate::effects::Effects>,
    level: Res<crate::LoadedLevel>,
    mut overlay: Query<&mut BackgroundColor, With<FlashOverlay>>,
    fixed: Res<Time<Fixed>>,
) {
    let _frame_scope = crate::input_latency::frame_scope("utility_fx");
    let w = &world.0;
    let time = w.time();
    let reset = time < fx.last_time;
    fx.last_time = time;
    let AssetsFx {
        smoke,
        fire,
        he,
        fields,
        explosions,
        ..
    } = &mut *fx;
    let gone: Vec<_> = fields
        .iter()
        .filter(|(id, emitter)| {
            reset || (!w.utility_fields.iter().any(|f| f.id == **id) && !emitter.alive(time))
        })
        .map(|(id, _)| *id)
        .collect();
    for id in gone {
        if let Some(emitter) = fields.remove(&id) {
            emitter.despawn(&mut commands, &mut meshes);
        }
    }
    if reset {
        for emitter in explosions.drain(..) {
            emitter.despawn(&mut commands, &mut meshes);
        }
    }
    for field in w
        .utility_fields
        .iter()
        .filter(|f| f.kind == Kind::Smoke || f.kind.fire())
    {
        if !fields.contains_key(&field.id) && fields.len() < 16 {
            let layers = if field.kind.fire() { &*fire } else { &*smoke };
            fields.insert(
                field.id,
                crate::utility_particles::Emitter::spawn(
                    &mut commands,
                    &mut meshes,
                    layers,
                    field.pos,
                    field.born,
                    field.until,
                    field.id,
                    field.kind.fire(),
                ),
            );
        }
    }
    let eye = to_bevy(local.render_eye);
    for (id, emitter) in fields.iter_mut() {
        if !w.utility_fields.iter().any(|f| f.id == *id) {
            emitter.until = emitter.until.min(time);
        }
        emitter.update(
            time,
            eye,
            if emitter.fire { fire } else { smoke },
            &mut meshes,
            &level.0.collision,
        );
    }
    for (kind, pos) in effects.utility_detonations.drain(..) {
        if kind == Kind::He && explosions.len() < 16 {
            if std::env::var_os("CSR_WEAPON_TRACE").is_some() {
                info!("HE emitter: birth={time:.5} seed={} pos={pos:?} layers={}", w.tick as u32, he.len());
            }
            explosions.push(crate::utility_particles::Emitter::spawn(
                &mut commands,
                &mut meshes,
                he,
                pos,
                time,
                time + crate::utility_particles::burst_duration(he),
                w.tick as u32,
                false,
            ));
        }
    }
    for emitter in &mut *explosions {
        emitter.update(time, eye, he, &mut meshes, &level.0.collision);
    }
    let mut i = 0;
    while i < explosions.len() {
        if !explosions[i].alive(time) {
            explosions.remove(i).despawn(&mut commands, &mut meshes);
        } else {
            i += 1;
        }
    }
    let gone: Vec<_> = fx
        .projectiles
        .keys()
        .filter(|id| reset || !grenade_bodies(w).any(|body| body.id == **id))
        .copied()
        .collect();
    for id in gone {
        if let Some(inst) = fx.projectiles.remove(&id) {
            for e in inst.instance.meshes.iter().chain(&inst.instance.joints) {
                commands.entity(*e).despawn();
            }
        }
    }
    for p in grenade_bodies(w) {
        let Some(asset) = characters.weapons.get(p.kind.name()) else {
            continue;
        };
        let pos = p.pos + p.velocity * (fixed.overstep_fraction() * world.0.dt());
        let position = to_bevy(pos);
        let rotation = if let Some(flying) = fx.projectiles.get_mut(&p.id) {
            let dt = (time - flying.last_time).clamp(0.0, 0.1);
            flying.last_time = time;
            if !p.settled {
                // Presentation only: the old yaw-only spin left every grenade
                // upright. Carry a stable two-axis tumble through each bounce;
                // stop after the authoritative soft-floor stop, including its
                // subsequent tiny gravity/contact cycles.
                let direction = to_bevy(p.velocity).normalize_or_zero();
                let axis = Vec3::Y.cross(direction).normalize_or(Vec3::X);
                flying.rotation = (Quat::from_axis_angle(axis, dt * 6.0)
                    * Quat::from_rotation_y(dt * 2.0)
                    * flying.rotation)
                    .normalize();
            }
            flying.rotation
        } else if p.settled {
            // A snapshot may contain an already activated decoy with no local
            // flight history. Its rotation is not part of simulation state;
            // show the installed body at rest instead of spinning it forever.
            Quat::IDENTITY
        } else {
            Quat::from_rotation_z((time - p.born) * 6.0)
        };
        let spin = Mat4::from_rotation_translation(rotation, position);
        if !fx.projectiles.contains_key(&p.id) {
            let mut base =
                crate::items::ground_transform(&asset.model, glam::Vec3::ZERO, 0.0, false);
            let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            for vertex in asset.model.meshes.iter().flat_map(|m| &m.vertices) {
                let p = base.transform_point3(to_bevy(vertex.position));
                lo = lo.min(p);
                hi = hi.max(p);
            }
            if lo.is_finite() {
                base = Mat4::from_translation(-(lo + hi) * 0.5) * base;
            }

            let pose = mdl::skeleton::model_space(
                &asset.model.bones,
                &mdl::skeleton::rest_pose(&asset.model.bones),
            );
            let instance =
                crate::skinning::spawn_posed(&mut commands, asset, &mut mats, spin * base);
            fx.projectiles.insert(
                p.id,
                Flying {
                    instance,
                    pose,
                    base,
                    rotation,
                    last_time: time,
                },
            );
        } else {
            let flying = &fx.projectiles[&p.id];
            crate::skinning::apply_pose(
                &flying.instance,
                spin * flying.base,
                &flying.pose,
                &mut transforms,
            );
        }
    }
    let flash = w.flash_amount(local.view_target());
    let smoke = w
        .utility_fields
        .iter()
        .filter(|f| f.kind == Kind::Smoke)
        .map(|f| {
            let distance = local.render_eye.distance(f.pos + glam::Vec3::Z * 55.0);
            let edge = ((140.0 - distance) / 30.0).clamp(0.0, 1.0);
            let life =
                ((time - f.born) / 0.6).clamp(0.0, 1.0) * ((f.until - time) / 3.0).clamp(0.0, 1.0);
            edge * edge * (3.0 - 2.0 * edge) * life
        })
        .fold(0.0, f32::max);
    for mut color in &mut overlay {
        *color = BackgroundColor(if flash > 0.0 {
            Color::srgba(1.0, 1.0, 1.0, flash)
        } else if smoke > 0.0 {
            Color::srgba(0.46, 0.44, 0.40, smoke)
        } else {
            Color::NONE
        });
    }
}

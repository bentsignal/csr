//! Presentation of authoritative map-door poses. Uses the existing studio
//! model/material path; there is no render-frame interaction or collision.

use crate::{LoadedLevel, coords::{rotation_to_bevy, to_bevy}, material::WorldMaterial,
    player::SimWorld, skinning::{self, SkinnedAsset, SkinnedInstance},
    textures::TextureCache, world_mesh::SharedHandles};
use bevy::{mesh::skinning::SkinnedMeshInverseBindposes, prelude::*};
use std::{collections::HashMap, sync::Arc};

struct Leaf {
    asset: SkinnedAsset,
    instance: SkinnedInstance,
    pose: Vec<glam::Mat4>,
    angle: f32,
}

#[derive(Resource, Default)]
pub(crate) struct DoorScene { leaves: Vec<Option<Leaf>> }

impl DoorScene {
    pub(crate) fn assets(&self) -> impl Iterator<Item = &SkinnedAsset> {
        self.leaves.iter().filter_map(|leaf| leaf.as_ref().map(|leaf| &leaf.asset))
    }
}

fn transform(def: &collision::doors::Door, angle: f32) -> Mat4 {
    Mat4::from_rotation_translation(rotation_to_bevy(def.rotation(angle)), to_bevy(def.origin))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn(
    commands: &mut Commands,
    level: &level::Level,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WorldMaterial>,
    images: &mut Assets<Image>,
    textures: &mut TextureCache,
    shared: &SharedHandles,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    exposure: f32,
) {
    let mut cache = HashMap::<(String, usize), Option<SkinnedAsset>>::new();
    let mut scene = DoorScene::default();
    let sun = crate::props::Sun::from_bsp(&level.bsp);
    for def in level.collision.doors.iter() {
        let ent = &level.bsp.entities[def.entity];
        let skin = ent.i64("skin").unwrap_or(0).max(0) as usize;
        let asset = cache.entry((def.model.clone(), skin)).or_insert_with(|| {
            let fs = &level.vfs;
            let base = def.model.trim_end_matches(".mdl");
            let mut model = mdl::Model::load(&fs.read(&def.model)?,
                &fs.read(&format!("{base}.vvd"))?, &fs.read(&format!("{base}.dx90.vtx"))?).ok()?;
            if skin < model.skins.len() { model.skins.swap(0, skin); }
            Some(skinning::build_asset(Arc::new(model), fs, meshes, materials, images,
                textures, shared, bindposes, exposure, true))
        }).clone();
        let Some(asset) = asset else {
            warn!("Map door model unavailable: {} (entity {})", def.model, def.entity);
            scene.leaves.push(None); continue;
        };
        let pose = mdl::skeleton::model_space(&asset.model.bones, &mdl::skeleton::rest_pose(&asset.model.bones));
        let instance = skinning::spawn(commands, &asset, None, Some(&mut *materials));
        for (joint, posed) in instance.joints.iter().zip(skinning::pose_transforms(transform(def, def.initial_angle), &pose)) {
            commands.entity(*joint).insert(posed);
        }
        let (cube, sun_dir, sun_color) = crate::characters::lighting_at(level, sun, def.center_at(def.initial_angle));
        let tint = ent.str("rendercolor").and_then(bsp::parse_origin).filter(|v| v.is_finite())
            .unwrap_or(glam::Vec3::splat(255.0)).clamp(glam::Vec3::ZERO, glam::Vec3::splat(255.0)) / 255.0;
        // The instance owns material copies. Resolve them after deferred spawn
        // below; assets in the cache retain their original per-material tint.
        for entity in &instance.meshes {
            commands.entity(*entity).insert(StudioLight { cube, sun_dir, sun_color, tint: Vec4::new(tint.x, tint.y, tint.z, 1.0) });
        }
        scene.leaves.push(Some(Leaf { asset, instance, pose, angle: def.initial_angle }));
    }
    commands.insert_resource(scene);
}

#[derive(Component)]
pub(crate) struct StudioLight {
    pub cube: [Vec4; 6], pub sun_dir: Vec4, pub sun_color: Vec4, pub tint: Vec4,
}

pub(crate) fn sync(
    mut commands: Commands,
    world: Res<SimWorld>,
    level: Res<LoadedLevel>,
    mut scene: ResMut<DoorScene>,
    mut transforms: Query<&mut Transform>,
    lights: Query<(Entity, &MeshMaterial3d<WorldMaterial>, &StudioLight)>,
    mut materials: ResMut<Assets<WorldMaterial>>,
) {
    for ((def, state), leaf) in level.0.collision.doors.iter().zip(&world.0.doors).zip(&mut scene.leaves) {
        let Some(leaf) = leaf else { continue; };
        if leaf.angle == state.angle { continue; }
        skinning::apply_pose(&leaf.instance, transform(def, state.angle), &leaf.pose, &mut transforms);
        leaf.angle = state.angle;
    }
    for (entity, handle, light) in &lights {
        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.params.ambient_cube = light.cube;
            material.params.sun_dir = light.sun_dir;
            material.params.sun_color = light.sun_color;
            material.params.color *= light.tint;
            commands.entity(entity).remove::<StudioLight>();
        }
    }
}

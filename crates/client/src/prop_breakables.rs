//! Actual mounted MDL presentation, owned by the same breakable ordinal as
//! PHY hit ownership and replicated health. Meshes survive removal for reset.
use crate::{breakables::BreakableMesh, coords::{rotation_to_bevy, to_bevy},
    doors::StudioLight, material::WorldMaterial, skinning::{self, SkinnedAsset},
    textures::TextureCache, world_mesh::SharedHandles};
use bevy::{mesh::skinning::SkinnedMeshInverseBindposes, prelude::*};
use std::{collections::HashMap, sync::Arc};

#[derive(Resource, Default)]
pub(crate) struct PropBreakableScene { pub(crate) assets: Vec<SkinnedAsset> }
impl PropBreakableScene {
    pub(crate) fn assets(&self) -> impl Iterator<Item = &SkinnedAsset> { self.assets.iter() }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn(commands: &mut Commands, level: &level::Level,
    meshes: &mut Assets<Mesh>, materials: &mut Assets<WorldMaterial>, images: &mut Assets<Image>,
    textures: &mut TextureCache, shared: &SharedHandles,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>, exposure: f32) {
    let mut cache = HashMap::<(String, usize, u32), Option<(SkinnedAsset, Arc<Vec<glam::Mat4>>)>>::new();
    let sun = crate::props::Sun::from_bsp(&level.bsp);
    for (index, def) in level.collision.breakables.iter().enumerate() {
        let Some(studio) = &def.studio else { continue; };
        let asset = cache.entry((studio.model.clone(), studio.skin, studio.body)).or_insert_with(|| {
            let fs = &level.vfs;
            let base = studio.model.trim_end_matches(".mdl");
            let mut model = mdl::Model::load_with_body_value(&fs.read(&studio.model)?,
                &fs.read(&format!("{base}.vvd"))?, &fs.read(&format!("{base}.dx90.vtx"))?, studio.body).ok()?;
            if studio.skin < model.skins.len() { model.skins.swap(0, studio.skin); }
            let pose = Arc::new(crate::studio_pose::initial(&model, fs));
            Some((skinning::build_asset(Arc::new(model), fs, meshes, materials, images,
                textures, shared, bindposes, exposure, true), pose))
        }).clone();
        let Some((asset, pose)) = asset else {
            warn!("Map breakable model unavailable: {} (entity {})", studio.model, def.entity);
            continue;
        };
        let transform = Mat4::from_scale_rotation_translation(Vec3::splat(studio.scale),
            rotation_to_bevy(mdl::angles_to_mat3(studio.angles)), to_bevy(def.origin));
        let instance = skinning::spawn(commands, &asset, None, Some(&mut *materials));
        for (joint, posed) in instance.joints.iter().zip(skinning::pose_transforms(transform, &pose)) {
            commands.entity(*joint).insert(posed);
        }
        let center = def.solids.iter().fold((glam::Vec3::splat(f32::INFINITY), glam::Vec3::splat(f32::NEG_INFINITY)),
            |(low, high), solid| (low.min(solid.mins), high.max(solid.maxs)));
        let (cube, sun_dir, sun_color) = crate::characters::lighting_at(level, sun, (center.0 + center.1) * 0.5);
        for entity in &instance.meshes {
            commands.entity(*entity).insert((BreakableMesh(vec![index]), StudioLight {
                cube, sun_dir, sun_color, tint: Vec4::new(studio.tint.x, studio.tint.y, studio.tint.z, 1.0),
            }));
        }
    }
    commands.insert_resource(PropBreakableScene { assets: cache.into_values().flatten().map(|(asset, _)| asset).collect() });
}

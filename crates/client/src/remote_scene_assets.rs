//! Gather current scene owners after spawn commands/resources are applied.
//! Asset-store differences include temporary orphans and are not ownership.
use crate::{
    characters::CharacterModels,
    items::GroundModels,
    material::WorldMaterial,
    remote_render_gate::MAX_SCENE_ASSETS,
    session::{MatchEntity, Persistent},
    skinning::SkinnedAsset,
    viewmodel::ViewModels,
};
use bevy::{asset::AssetId, prelude::*};
use std::collections::BTreeSet;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SceneAssets {
    pub meshes: Vec<AssetId<Mesh>>,
    pub images: Vec<AssetId<Image>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GatherError {
    MissingMaterials,
    MissingMaterial(AssetId<WorldMaterial>),
    TooManyAssets,
}

#[derive(Default)]
struct Owners {
    meshes: BTreeSet<AssetId<Mesh>>,
    images: BTreeSet<AssetId<Image>>,
    materials: BTreeSet<AssetId<WorldMaterial>>,
}
impl Owners {
    fn check(&self) -> Result<(), GatherError> {
        if self.meshes.len().saturating_add(self.images.len()) > MAX_SCENE_ASSETS
            || self.materials.len() > MAX_SCENE_ASSETS
        {
            return Err(GatherError::TooManyAssets);
        }
        Ok(())
    }
    fn model(&mut self, asset: &SkinnedAsset) -> Result<(), GatherError> {
        for (mesh, material) in &asset.parts {
            self.meshes.insert(mesh.id());
            self.materials.insert(material.id());
            self.check()?;
        }
        Ok(())
    }
}

/// Stable deduplicated IDs for live match entities and retained model resources.
/// Reads ownership only: no assets are promoted to strong handles or retained by
/// this result. The caller must keep the scene owners alive during preparation.
/// Missing model resources are allowed (maps may omit models); a missing owned
/// WorldMaterial or its store is an explicit error, never a skipped dependency.
pub fn gather(world: &mut World) -> Result<SceneAssets, GatherError> {
    let mut owners = Owners::default();
    let mut query = world.query_filtered::<(
        Option<&Mesh3d>,
        Option<&MeshMaterial3d<WorldMaterial>>,
        Option<&ImageNode>,
    ), (With<MatchEntity>, Without<Persistent>)>();
    for (mesh, material, image) in query.iter(world) {
        if let Some(mesh) = mesh {
            owners.meshes.insert(mesh.0.id());
        }
        if let Some(material) = material {
            owners.materials.insert(material.0.id());
        }
        if let Some(image) = image {
            owners.images.insert(image.image.id());
        }
        owners.check()?;
    }
    if let Some(models) = world.get_resource::<CharacterModels>() {
        for team in models
            .terrorist
            .iter()
            .chain(models.counter_terrorist.iter())
        {
            owners.model(&team.asset)?;
        }
        for asset in models.weapons.values() {
            owners.model(asset)?;
        }
    }
    if let Some(models) = world.get_resource::<ViewModels>() {
        for asset in models.assets() {
            owners.model(asset)?;
        }
    }
    if let Some(models) = world.get_resource::<GroundModels>() {
        for asset in models.planted.iter().chain(models.kit.iter()).chain(models.weapons.values()) {
            owners.model(asset)?;
        }
    }
    let materials = world
        .get_resource::<Assets<WorldMaterial>>()
        .ok_or(GatherError::MissingMaterials)?;
    for id in &owners.materials {
        let material = materials
            .get(*id)
            .ok_or(GatherError::MissingMaterial(*id))?;
        // Every bound image field of WorldMaterial, including shared/reused
        // fallback textures, lightmaps, prop lighting and cosmetic surfaces.
        for image in [
            &material.base,
            &material.base2,
            &material.lightmap,
            &material.modulate,
            &material.normal,
            &material.normal2,
            &material.prop_light,
            &material.phong_map,
            &material.skin_surface,
            &material.skin_position,
            &material.detail,
            &material.self_illum_mask,
        ] {
            owners.images.insert(image.id());
        }
        owners.check()?;
    }
    Ok(SceneAssets {
        meshes: owners.meshes.into_iter().collect(),
        images: owners.images.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology};
    use std::{collections::HashMap, sync::Arc};

    fn mesh(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        meshes.add(Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        ))
    }
    fn material(
        materials: &mut Assets<WorldMaterial>,
        handles: &[Handle<Image>],
    ) -> Handle<WorldMaterial> {
        materials.add(WorldMaterial {
            params: Default::default(),
            base: handles[0].clone(),
            base2: handles[1].clone(),
            lightmap: handles[2].clone(),
            modulate: handles[3].clone(),
            normal: handles[4].clone(),
            normal2: handles[5].clone(),
            prop_light: handles[6].clone(),
            phong_map: handles[7].clone(),
            skin_surface: handles[8].clone(),
            skin_position: handles[9].clone(),
            detail: handles[10].clone(),
            self_illum_mask: handles[11].clone(),
            alpha_mode: AlphaMode::Opaque,
            cull: false,
        })
    }
    fn model(mesh: Handle<Mesh>, material: Handle<WorldMaterial>) -> SkinnedAsset {
        SkinnedAsset {
            model: Arc::new(mdl::Model::default()),
            parts: vec![(mesh, material)],
            inverse_bindposes: Default::default(),
        }
    }

    #[test]
    fn reused_entity_and_retained_model_owners_include_all_twelve_images_once() {
        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<WorldMaterial>::default();
        let handles = (0..12)
            .map(|_| images.add(Image::default()))
            .collect::<Vec<_>>();
        let material = material(&mut materials, &handles);
        let entity_mesh = mesh(&mut meshes);
        let model_mesh = mesh(&mut meshes);
        let ground_mesh = mesh(&mut meshes);
        let dropped_mesh = mesh(&mut meshes);
        // These images/meshes already existed before the scene; ownership,
        // rather than a fresh-ID difference, must include them.
        world.spawn((
            MatchEntity,
            Mesh3d(entity_mesh.clone()),
            MeshMaterial3d(material.clone()),
        ));
        world.spawn((
            MatchEntity,
            Mesh3d(entity_mesh.clone()),
            MeshMaterial3d(material.clone()),
        ));
        world.spawn((MatchEntity, ImageNode::new(handles[0].clone())));
        world.insert_resource(CharacterModels {
            terrorist: None,
            counter_terrorist: None,
            weapons: HashMap::from([(
                "fixture".into(),
                model(model_mesh.clone(), material.clone()),
            )]),
            sun: None,
        });
        world.insert_resource(GroundModels {
            planted: Some(model(ground_mesh.clone(), material.clone())),
            kit: None,
            weapons: HashMap::from([(
                "mod_weapon_ground_fixture".parse().unwrap(),
                model(dropped_mesh.clone(), material),
            )]),
        });
        world.insert_resource(ViewModels::default());
        world.insert_resource(meshes);
        world.insert_resource(images);
        world.insert_resource(materials);
        let scene = gather(&mut world).unwrap();
        let mut expected_meshes = vec![entity_mesh.id(), model_mesh.id(), ground_mesh.id(), dropped_mesh.id()];
        expected_meshes.sort();
        let mut expected_images = handles.iter().map(Handle::id).collect::<Vec<_>>();
        expected_images.sort();
        assert_eq!(
            scene,
            SceneAssets {
                meshes: expected_meshes,
                images: expected_images
            }
        );
        assert_eq!(gather(&mut world).unwrap(), scene);
    }

    #[test]
    fn raw_fresh_orphans_and_old_persistent_owners_are_omitted() {
        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<WorldMaterial>::default();
        let scene_image = images.add(Image::default());
        let old_image = images.add(Image::default());
        let fresh_orphan = images.add(Image::default());
        let orphan_id = fresh_orphan.id();
        drop(fresh_orphan);
        let scene_mesh = mesh(&mut meshes);
        let old_mesh = mesh(&mut meshes);
        let unused_mesh = mesh(&mut meshes);
        let mat = material(&mut materials, &vec![scene_image.clone(); 12]);
        let old_mat = material(&mut materials, &vec![old_image.clone(); 12]);
        world.spawn((MatchEntity, Mesh3d(scene_mesh.clone()), MeshMaterial3d(mat)));
        world.spawn((
            crate::session::Persistent,
            MatchEntity,
            Mesh3d(old_mesh.clone()),
            MeshMaterial3d(old_mat),
            ImageNode::new(old_image),
        ));
        world.insert_resource(meshes);
        world.insert_resource(images);
        world.insert_resource(materials);
        assert!(world.resource::<Assets<Image>>().contains(orphan_id));
        assert!(world.resource::<Assets<Mesh>>().contains(unused_mesh.id()));
        assert_eq!(
            gather(&mut world).unwrap(),
            SceneAssets {
                meshes: vec![scene_mesh.id()],
                images: vec![scene_image.id()]
            }
        );
    }

    #[test]
    fn despawned_scene_cannot_leave_asset_store_ids_in_a_replacement() {
        let mut world = World::new();
        let mut images = Assets::<Image>::default();
        let old = images.add(Image::default());
        let current = images.add(Image::default());
        let entity = world.spawn((MatchEntity, ImageNode::new(old.clone()))).id();
        world.insert_resource(images);
        world.insert_resource(Assets::<WorldMaterial>::default());
        assert_eq!(gather(&mut world).unwrap().images, vec![old.id()]);
        world.despawn(entity);
        world.spawn((MatchEntity, ImageNode::new(current.clone())));
        assert!(world.resource::<Assets<Image>>().contains(old.id()));
        assert_eq!(gather(&mut world).unwrap().images, vec![current.id()]);
    }

    #[test]
    fn missing_owned_material_or_store_is_an_error_not_a_skipped_image_set() {
        let mut world = World::new();
        assert_eq!(gather(&mut world), Err(GatherError::MissingMaterials));
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<WorldMaterial>::default();
        let image = images.add(Image::default());
        let mat = material(&mut materials, &vec![image; 12]);
        let id = mat.id();
        materials.remove(id);
        world.spawn((MatchEntity, MeshMaterial3d(mat)));
        world.insert_resource(materials);
        assert_eq!(gather(&mut world), Err(GatherError::MissingMaterial(id)));
    }

    #[test]
    fn retained_model_assets_plus_images_enforce_the_shared_gate_bound() {
        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<WorldMaterial>::default();
        let image = images.add(Image::default());
        let mat = material(&mut materials, &vec![image; 12]);
        let parts = (0..MAX_SCENE_ASSETS)
            .map(|_| (mesh(&mut meshes), mat.clone()))
            .collect();
        world.insert_resource(CharacterModels {
            terrorist: None,
            counter_terrorist: None,
            weapons: HashMap::from([(
                "large".into(),
                SkinnedAsset {
                    model: Arc::new(mdl::Model::default()),
                    parts,
                    inverse_bindposes: Default::default(),
                },
            )]),
            sun: None,
        });
        world.insert_resource(meshes);
        world.insert_resource(images);
        world.insert_resource(materials);
        // MAX meshes plus their one unique image exceed the shared gate bound.
        assert_eq!(gather(&mut world), Err(GatherError::TooManyAssets));
    }
}

//! Things on the ground: dropped weapons, defuse kits and the planted bomb,
//! drawn with the game's world models and lit like characters.

use crate::LoadedLevel;
use crate::characters::{CharacterModels, lighting_at};
use crate::coords::{rotation_to_bevy, to_bevy};
use crate::material::WorldMaterial;
use crate::player::SimWorld;
use crate::skinning::{self, SkinnedAsset, SkinnedInstance};
use crate::textures::TextureCache;
use crate::world_mesh::SharedHandles;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

pub const PLANTED_C4_MODEL: &str = "models/weapons/w_c4_planted.mdl";
pub const DEFUSER_MODEL: &str = "models/weapons/w_defuser.mdl";

#[derive(Resource, Default)]
pub struct GroundModels {
    pub planted: Option<SkinnedAsset>,
    pub kit: Option<SkinnedAsset>,
    /// Separate authored drops; holding still uses CharacterModels.weapons.
    pub weapons: HashMap<content::ContentId, SkinnedAsset>,
    /// Includes failed imports, so an unavailable drop never retries each frame.
    prepared_paths: HashMap<String, Option<SkinnedAsset>>,
    /// Moved here after the last map-spawn consumer; removed with this map.
    context: Option<GroundLoadContext>,
}

struct GroundLoadContext {
    textures: TextureCache,
    shared: SharedHandles,
    exposure: f32,
}

impl GroundModels {
    pub(crate) fn with_load_context(
        mut self,
        textures: TextureCache,
        shared: SharedHandles,
        exposure: f32,
    ) -> Self {
        self.context = Some(GroundLoadContext { textures, shared, exposure });
        self
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_weapon(
        &mut self,
        def: &weapons::WeaponDefinition,
        fs: &vfs::Vfs,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<WorldMaterial>,
        images: &mut Assets<Image>,
        bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    ) {
        if def.dropped_model.is_empty() || self.weapons.contains_key(&def.id) {
            return;
        }
        let Some(context) = &mut self.context else { return };
        let asset = self.prepared_paths.entry(def.dropped_model.clone()).or_insert_with(|| {
            load_asset(fs, &def.dropped_model, meshes, materials, images,
                &mut context.textures, &context.shared, bindposes, context.exposure)
        });
        if let Some(asset) = asset {
            // Only immutable model/mesh templates are shared. spawn_posed still
            // creates each item's joints and mutable materials independently.
            self.weapons.insert(def.id.clone(), asset.clone());
        }
    }
}

struct Placed {
    instance: SkinnedInstance,
    materials: Vec<Handle<WorldMaterial>>,
    pos: glam::Vec3,
    yaw: f32,
    suppressor_attached: bool,
    lighting_dirty: bool,
}

#[derive(Resource, Default)]
pub struct GroundInstances {
    items: HashMap<u32, Placed>,
    bomb: Option<(glam::Vec3, Placed)>,
}

#[allow(clippy::too_many_arguments)]
pub fn load(
    level: &level::Level,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WorldMaterial>,
    images: &mut Assets<Image>,
    textures: &mut TextureCache,
    shared: &SharedHandles,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    exposure: f32,
) -> GroundModels {
    let mut asset = |path: &str| {
        load_asset(&level.vfs, path, meshes, materials, images, textures, shared, bindposes, exposure)
    };
    // Catalog presence is not a ground-item request. Deathmatch commonly
    // starts with none; prepare authored weapon drops only in sync below.
    GroundModels {
        planted: asset(PLANTED_C4_MODEL),
        kit: asset(DEFUSER_MODEL),
        ..Default::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn load_asset(
    fs: &vfs::Vfs,
    path: &str,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WorldMaterial>,
    images: &mut Assets<Image>,
    textures: &mut TextureCache,
    shared: &SharedHandles,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    exposure: f32,
) -> Option<SkinnedAsset> {
    let base = path.trim_end_matches(".mdl");
    let model = mdl::Model::load(
        &fs.read(path)?,
        &fs.read(&format!("{base}.vvd"))?,
        &fs.read(&format!("{base}.dx90.vtx"))?,
    ).ok()?;
    Some(skinning::build_asset(Arc::new(model), fs, meshes, materials,
        images, textures, shared, bindposes, exposure, true))
}

fn despawn(commands: &mut Commands, p: &Placed) {
    for e in p.instance.meshes.iter().chain(&p.instance.joints) {
        commands.entity(*e).despawn();
    }
}

/// Weapons lie on their side; the bomb and kits sit upright.
pub(crate) fn ground_transform(model: &mdl::Model, pos: glam::Vec3, yaw: f32, on_side: bool) -> Mat4 {
    ground_transform_with_suppressor(model, pos, yaw, on_side, true)
}

fn ground_transform_with_suppressor(
    model: &mdl::Model,
    pos: glam::Vec3,
    yaw: f32,
    on_side: bool,
    suppressor_attached: bool,
) -> Mat4 {
    let roll = if on_side { 90.0 } else { 0.0 };
    // World weapons include the character's reference skeleton. Undo its
    // authored hand rotation before laying the weapon's thin axis down.
    let hand = model
        .bones
        .iter()
        .find(|b| b.name == "weapon_hand_R")
        .map_or(glam::Mat3::IDENTITY, |b| {
            glam::Mat3::from_mat4(b.pose_to_bone)
        });
    let rot = mdl::angles_to_mat3(glam::Vec3::new(0.0, yaw, roll)) * hand;
    // World models are authored relative to the held skeleton, not a floor
    // pivot. Place the lowest rotated vertex on the surface and centre XY.
    let offset = resting_offset(
        model
            .meshes
            .iter()
            .enumerate()
            // Placement must use the same bodyparts that are actually drawn.
            // A detached suppressor must not shift the gun's floor contact or
            // centre around invisible geometry.
            .filter(|(index, _)| !skinning::suppressor_mesh_hidden(model, *index, suppressor_attached))
            .flat_map(|(_, m)| m.vertices.iter().map(|v| rot * v.position)),
    );
    Mat4::from_rotation_translation(rotation_to_bevy(rot), to_bevy(pos + offset))
}

fn resting_offset(points: impl Iterator<Item = glam::Vec3>) -> glam::Vec3 {
    let (mut lo, mut hi) = (
        glam::Vec3::splat(f32::INFINITY),
        glam::Vec3::splat(f32::NEG_INFINITY),
    );
    for p in points {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    if !lo.is_finite() {
        return glam::Vec3::ZERO;
    }
    glam::Vec3::new(-(lo.x + hi.x) * 0.5, -(lo.y + hi.y) * 0.5, 0.25 - lo.z)
}

#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    world: Res<SimWorld>,
    characters: Res<CharacterModels>,
    mut ground: ResMut<GroundModels>,
    mut placed: ResMut<GroundInstances>,
    mut materials: ResMut<Assets<WorldMaterial>>,
    level: Res<LoadedLevel>,
    mat_query: Query<&MeshMaterial3d<WorldMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let _frame_scope = crate::input_latency::frame_scope("ground_items");
    let world = &world.0;
    let sun = characters.sun;

    // Items that were picked up or cleared.
    let gone: Vec<u32> = placed
        .items
        .keys()
        .copied()
        .filter(|id| !world.items.iter().any(|i| i.id == *id))
        .collect();
    for id in gone {
        if let Some(p) = placed.items.remove(&id) {
            despawn(&mut commands, &p);
        }
    }
    for item in &world.items {
        if !placed.items.contains_key(&item.id)
            && let sim::GroundKind::Weapon(weapon) = &item.kind {
            ground.prepare_weapon(&weapon.def, &level.0.vfs, &mut meshes,
                &mut materials, &mut images, &mut bindposes);
        }
        let (asset, on_side) = match &item.kind {
            sim::GroundKind::Weapon(w) => (
                ground.weapons.get(&w.def.id).or_else(|| characters.weapons.get(&w.def.class_name)),
                w.def.class_name != sim::weapon::C4,
            ),
            sim::GroundKind::DefuseKit => (ground.kit.as_ref(), false),
        };
        let Some(asset) = asset else { continue };
        let suppressor_attached = match &item.kind {
            sim::GroundKind::Weapon(weapon) => weapon.suppressor_attached,
            sim::GroundKind::DefuseKit => true,
        };
        if let Some(p) = placed.items.get_mut(&item.id) {
            if p.pos != item.pos || p.yaw != item.yaw || p.suppressor_attached != suppressor_attached {
                let pose = mdl::skeleton::model_space(&asset.model.bones,
                    &mdl::skeleton::rest_pose(&asset.model.bones));
                let transform = ground_transform_with_suppressor(&asset.model,
                    item.pos, item.yaw, on_side, suppressor_attached);
                for (joint, transform) in p.instance.joints.iter()
                    .zip(skinning::pose_transforms(transform, &pose)) {
                    commands.entity(*joint).insert(transform);
                }
                if p.suppressor_attached != suppressor_attached {
                    for (index, entity) in p.instance.meshes.iter().enumerate() {
                        let hidden = skinning::suppressor_mesh_hidden(&asset.model, index, suppressor_attached);
                        commands.entity(*entity).insert(if hidden { Visibility::Hidden } else { Visibility::Inherited });
                    }
                }
                p.lighting_dirty |= p.pos != item.pos;
                p.pos = item.pos;
                p.yaw = item.yaw;
                p.suppressor_attached = suppressor_attached;
            }
            continue;
        }
        if std::env::var_os("CSR_WEAPON_TRACE").is_some() {
            info!("ground item model: item={} class={} model={}", item.id, item.class_name(), asset.model.name);
        }
        let instance = skinning::spawn_posed(
            &mut commands,
            asset,
            &mut materials,
            ground_transform_with_suppressor(&asset.model, item.pos, item.yaw, on_side, suppressor_attached),
        );
        if let sim::GroundKind::Weapon(weapon) = &item.kind {
            for (index, entity) in instance.meshes.iter().enumerate() {
                if skinning::suppressor_mesh_hidden(&asset.model, index, weapon.suppressor_attached) {
                    commands.entity(*entity).insert(Visibility::Hidden);
                }
            }
        }
        crate::skin_finish::tag(&mut commands,&instance,crate::skin_finish::Target::Ground(item.id));
        crate::decorations::tag(&mut commands,asset,&instance,crate::skin_finish::Target::Ground(item.id),0);
        placed.items.insert(
            item.id,
            Placed {
                instance,
                materials: Vec::new(),
                pos: item.pos,
                yaw: item.yaw,
                suppressor_attached,
                lighting_dirty: true,
            },
        );
    }

    // The planted bomb.
    let bomb_pos = world.bomb.map(|b| b.pos);
    if placed.bomb.as_ref().map(|b| b.0) != bomb_pos {
        if let Some((_, p)) = placed.bomb.take() {
            despawn(&mut commands, &p);
        }
        if let (Some(pos), Some(asset)) = (bomb_pos, ground.planted.as_ref()) {
            let instance = skinning::spawn_posed(
                &mut commands,
                asset,
                &mut materials,
                ground_transform(&asset.model, pos, 0.0, false),
            );
            placed.bomb = Some((
                pos,
                Placed {
                    instance,
                    materials: Vec::new(),
                    pos,
                    yaw: 0.0,
                    suppressor_attached: true,
                    lighting_dirty: true,
                },
            ));
        }
    }

    // Light new instances once their materials exist, and refresh moved drops.
    let GroundInstances { items, bomb } = &mut *placed;
    for p in items.values_mut().chain(bomb.iter_mut().map(|b| &mut b.1)) {
        if !p.lighting_dirty {
            continue;
        }
        if p.materials.is_empty() {
            p.materials = p.instance.meshes.iter()
                .filter_map(|e| mat_query.get(*e).ok().map(|m| m.0.clone()))
                .collect();
        }
        if p.materials.is_empty() {
            continue;
        }
        let (cube, sun_dir, sun_color) = lighting_at(&level.0, sun, p.pos + glam::Vec3::Z * 8.0);
        for h in &p.materials {
            if let Some(mut m) = materials.get_mut(h) {
                m.params.ambient_cube = cube;
                m.params.sun_dir = sun_dir;
                m.params.sun_color = sun_color;
            }
        }
        p.lighting_dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DROP: &str = "models/fixture/drop.mdl";
    const OTHER_DROP: &str = "models/fixture/other.mdl";
    const MISSING_DROP: &str = "models/fixture/missing.mdl";

    // Generated empty studio headers exercise the real MDL/VVD/VTX importer,
    // without installed assets, a renderer or any third-party geometry.
    fn model_files(path: &str, name: &str) -> std::collections::BTreeMap<String, Arc<[u8]>> {
        let mut mdl = vec![0u8; 408];
        mdl[..4].copy_from_slice(b"IDST");
        mdl[4..8].copy_from_slice(&48i32.to_le_bytes());
        mdl[12..12 + name.len()].copy_from_slice(name.as_bytes());
        let mut vvd = vec![0u8; 64];
        vvd[..4].copy_from_slice(b"IDSV");
        vvd[4..8].copy_from_slice(&4i32.to_le_bytes());
        let mut vtx = vec![0u8; 36];
        vtx[..4].copy_from_slice(&7i32.to_le_bytes());
        let base = path.trim_end_matches(".mdl");
        std::collections::BTreeMap::from([
            (path.into(), mdl.into()),
            (format!("{base}.vvd"), vvd.into()),
            (format!("{base}.dx90.vtx"), vtx.into()),
        ])
    }

    fn fixture_app(drop_name: &str, initial: &[&str]) -> App {
        let mut files = model_files(DROP, drop_name);
        files.extend(model_files(OTHER_DROP, "other authored drop"));
        files.extend(model_files(PLANTED_C4_MODEL, "planted"));
        files.extend(model_files(DEFUSER_MODEL, "kit"));
        let mut bsp = vec![0u8; 1036];
        bsp[..4].copy_from_slice(b"VBSP");
        bsp[4..8].copy_from_slice(&20i32.to_le_bytes());
        files.insert("maps/ground_fixture.bsp".into(), bsp.into());
        let mut fs = vfs::Vfs::new();
        fs.push_back("ground fixture", vfs::Mount::Memory(files));
        let mut level = level::Level::load_from_vfs(
            content::ContentId::csgo(content::ContentKind::Map, "ground_fixture"),
            "ground_fixture", Default::default(), fs,
        ).unwrap();
        let declarations = [
            ("weapon_ak47", DROP), ("weapon_glock", DROP),
            ("weapon_awp", OTHER_DROP), ("weapon_aug", MISSING_DROP),
            ("weapon_famas", ""),
        ].into_iter().enumerate().map(|(index, (class, dropped))| format!(
            r#""{index}" {{ "name" "{class}" "baseitem" "1" "model_dropped" "{dropped}" }}"#
        )).collect::<Vec<_>>().join("\n");
        level.catalog = Arc::new(weapons::Catalog::from_items_game(
            &format!("items_game {{ items {{ {declarations} }} }}")
        ).unwrap());
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<WorldMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mut bindposes = Assets::<SkinnedMeshInverseBindposes>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        // The held fallback has real, synthetic mesh/joint owners. Authored
        // empty drops above must not silently select this visibly different asset.
        let held = skinning::build_asset(Arc::new(mdl::Model {
            name: "held fallback".into(),
            bones: vec![mdl::skeleton::Bone {
                name: "fixture".into(), parent: -1, pos: glam::Vec3::ZERO,
                quat: glam::Quat::IDENTITY, rot: glam::Vec3::ZERO,
                pos_scale: glam::Vec3::ONE, rot_scale: glam::Vec3::ONE,
                pose_to_bone: glam::Mat4::IDENTITY, flags: 0,
            }],
            meshes: vec![mdl::Mesh {
                vertices: [glam::Vec3::ZERO, glam::Vec3::X, glam::Vec3::Y]
                    .map(|position| mdl::Vertex {
                        position, normal: glam::Vec3::Z, bone_count: 1,
                        weights: [1., 0., 0.], tangent: Some([1., 0., 0., 1.]),
                        ..Default::default()
                    }).to_vec(),
                indices: vec![0, 1, 2], ..Default::default()
            }],
            ..Default::default()
        }), &level.vfs, &mut meshes, &mut materials, &mut images,
            &mut textures, &shared, &mut bindposes, 1., true);
        let ground = load(&level, &mut meshes, &mut materials, &mut images,
            &mut textures, &shared, &mut bindposes, 1.)
            .with_load_context(textures, shared, 1.);
        let characters = CharacterModels {
            terrorist: None, counter_terrorist: None, sun: None,
            weapons: level.catalog.weapons.iter()
                .map(|def| (def.class_name.clone(), held.clone())).collect(),
        };
        let mut sim = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
        for (id, class) in initial.iter().enumerate() {
            let item = ground_item(&sim, id as u32, class);
            sim.items.push(item);
        }
        let mut app = App::new();
        app.insert_resource(meshes).insert_resource(materials).insert_resource(images)
            .insert_resource(bindposes).insert_resource(ground).insert_resource(characters)
            .insert_resource(LoadedLevel(level)).insert_resource(SimWorld(sim))
            .init_resource::<GroundInstances>().add_systems(Update, sync);
        app
    }

    fn ground_item(world: &sim::World, id: u32, class: &str) -> sim::GroundItem {
        sim::GroundItem {
            id, kind: sim::GroundKind::Weapon(sim::weapon::WeaponState::new(
                Arc::new(world.catalog.get(class).unwrap().clone()))),
            pos: glam::Vec3::ZERO, yaw: 0., dropped_by: 0, pickup_after: 0.,
            vertical_velocity: 0., settled: false,
        }
    }

    fn add_item(app: &mut App, id: u32, class: &str) {
        let item = ground_item(&app.world().resource::<SimWorld>().0, id, class);
        app.world_mut().resource_mut::<SimWorld>().0.items.push(item);
    }

    #[test]
    fn empty_initial_items_do_not_import_catalog_drops_but_keep_bomb_and_kit() {
        let mut app = fixture_app("authored drop", &[]);
        app.update();
        let ground = app.world().resource::<GroundModels>();
        assert_eq!(app.world().resource::<SimWorld>().0.catalog.weapons.len(), 5);
        assert!(ground.weapons.is_empty() && ground.prepared_paths.is_empty());
        assert_eq!(ground.planted.as_ref().unwrap().model.name, "planted");
        assert_eq!(ground.kit.as_ref().unwrap().model.name, "kit");
        // One held fixture plus bomb/kit, with zero catalog drop imports.
        assert_eq!(app.world().resource::<Assets<SkinnedMeshInverseBindposes>>().len(), 3);
    }

    #[test]
    fn sync_imports_initial_and_later_item_ids_once_per_authored_path() {
        // Also covers a remote world's already-populated initial items: no
        // offline-only catalog filter or drop-event dependency in sync.
        let mut app = fixture_app("authored drop", &["weapon_ak47"]);
        let before = app.world().resource::<SimWorld>().0.items.clone();
        app.update();
        assert_eq!(app.world().resource::<SimWorld>().0.items, before);
        let ground = app.world().resource::<GroundModels>();
        let def = &app.world().resource::<SimWorld>().0.catalog.weapons[0];
        let first = ground.weapons[&def.id].model.clone();
        assert_eq!(first.name, "authored drop");
        assert!(app.world().resource::<GroundInstances>().items[&0].instance.meshes.is_empty(),
            "authored drop must win over the distinct held fallback");
        assert_eq!(app.world().resource::<Assets<SkinnedMeshInverseBindposes>>().len(), 4);
        add_item(&mut app, 1, "weapon_glock"); // New ContentId, same path.
        add_item(&mut app, 2, "weapon_awp");   // New ContentId and path.
        app.update();
        app.update();
        let ground = app.world().resource::<GroundModels>();
        assert_eq!((ground.weapons.len(), ground.prepared_paths.len()), (3, 2));
        let alias = &app.world().resource::<SimWorld>().0.catalog.get("weapon_glock").unwrap().id;
        assert!(Arc::ptr_eq(&first, &ground.weapons[alias].model));
        assert_eq!(app.world().resource::<Assets<SkinnedMeshInverseBindposes>>().len(), 5);
        app.world_mut().resource_mut::<SimWorld>().0.items.clear();
        app.update();
        assert!(app.world().resource::<GroundInstances>().items.is_empty());
        add_item(&mut app, 3, "weapon_ak47");
        app.update();
        assert!(app.world().resource::<GroundInstances>().items.contains_key(&3));
        assert_eq!(app.world().resource::<Assets<SkinnedMeshInverseBindposes>>().len(), 5);
        let ground = app.world().resource::<GroundModels>();
        assert!(Arc::ptr_eq(&first, &ground.prepared_paths[DROP].as_ref().unwrap().model));
    }

    #[test]
    fn missing_and_empty_drop_fallbacks_keep_instance_owners_and_bound_failed_imports() {
        let mut app = fixture_app("authored drop", &["weapon_aug", "weapon_aug", "weapon_famas"]);
        app.update();
        let instances = app.world().resource::<GroundInstances>();
        assert_eq!(instances.items.len(), 3);
        let (a, b) = (&instances.items[&0].instance, &instances.items[&1].instance);
        assert_ne!(a.joints, b.joints);
        let (a, b) = (a.meshes[0], b.meshes[0]);
        assert_ne!(app.world().get::<MeshMaterial3d<WorldMaterial>>(a).unwrap().0,
            app.world().get::<MeshMaterial3d<WorldMaterial>>(b).unwrap().0);
        assert!(app.world().get::<crate::decorations::Parent>(a).is_some());
        assert!(app.world().get::<crate::skin_finish::SkinPart>(a).is_some());
        assert!(matches!(app.world().get::<crate::skin_finish::Target>(a),
            Some(crate::skin_finish::Target::Ground(0))));
        let ground = app.world().resource::<GroundModels>();
        assert!(ground.weapons.is_empty());
        assert_eq!(ground.prepared_paths.len(), 1);
        assert!(ground.prepared_paths[MISSING_DROP].is_none());
        // Even if data appears later, the failed path is not re-read for a new
        // item or a reappearance in this map. A map reload permits a retry.
        app.world_mut().resource_mut::<LoadedLevel>().0.vfs.push_front(
            "late fixture", vfs::Mount::Memory(model_files(MISSING_DROP, "late model")));
        app.world_mut().resource_mut::<SimWorld>().0.items.clear();
        app.update();
        assert!(app.world().get_entity(a).is_err() && app.world().get_entity(b).is_err());
        add_item(&mut app, 4, "weapon_aug");
        app.update();
        assert!(app.world().resource::<GroundModels>().prepared_paths[MISSING_DROP].is_none());
        assert_eq!(app.world().resource::<Assets<SkinnedMeshInverseBindposes>>().len(), 3);
        app.world_mut().resource_mut::<SimWorld>().0.items.clear();
        app.update();
        app.world_mut().resource_mut::<CharacterModels>().weapons.remove("weapon_aug");
        add_item(&mut app, 5, "weapon_aug");
        app.update();
        app.update();
        assert!(!app.world().resource::<GroundInstances>().items.contains_key(&5));
        assert!(app.world().resource::<GroundModels>().prepared_paths[MISSING_DROP].is_none());
        assert_eq!(app.world().resource::<Assets<SkinnedMeshInverseBindposes>>().len(), 3);
    }

    #[test]
    fn same_id_moved_drop_updates_pose_and_lighting_without_replacing_owners() {
        // Empty model_dropped selects the real synthetic held fallback mesh.
        let mut app = fixture_app("authored drop", &["weapon_famas"]);
        app.update(); app.update();
        let placed = &app.world().resource::<GroundInstances>().items[&0];
        let joints = placed.instance.joints.clone();
        let meshes = placed.instance.meshes.clone();
        let material = placed.materials[0].clone();
        let count = app.world().resource::<Assets<WorldMaterial>>().len();
        let tint = Vec4::new(0.1, 0.2, 0.3, 0.4);
        {
            let mut materials = app.world_mut().resource_mut::<Assets<WorldMaterial>>();
            let mut m = materials.get_mut(&material).unwrap();
            m.params.ambient_cube = [Vec4::splat(-1.); 6];
            m.params.color = tint;
        }
        app.world_mut().get_mut::<crate::skin_finish::SkinPart>(meshes[0]).unwrap().applied = Some((1, 2, 3));
        let item = {
            let mut world = app.world_mut().resource_mut::<SimWorld>();
            let item = &mut world.0.items[0];
            item.pos = glam::Vec3::new(23., 45., 67.);
            item.yaw = 123.;
            let sim::GroundKind::Weapon(weapon) = &mut item.kind else { panic!() };
            weapon.suppressor_attached = false;
            item.clone()
        };
        app.update();
        let placed = &app.world().resource::<GroundInstances>().items[&0];
        assert_eq!(placed.instance.joints, joints);
        assert_eq!(placed.instance.meshes, meshes);
        assert_eq!(placed.materials, vec![material.clone()]);
        assert_eq!(app.world().resource::<Assets<WorldMaterial>>().len(), count);
        assert_eq!(app.world().resource::<SimWorld>().0.items[0], item, "rendering cannot advance gameplay");
        let model = &app.world().resource::<CharacterModels>().weapons["weapon_famas"].model;
        let pose = mdl::skeleton::model_space(&model.bones, &mdl::skeleton::rest_pose(&model.bones));
        let transform = ground_transform_with_suppressor(model, item.pos, item.yaw, true, false);
        for (joint, expected) in joints.iter().zip(skinning::pose_transforms(transform, &pose)) {
            assert_eq!(*app.world().get::<Transform>(*joint).unwrap(), expected);
        }
        let expected = lighting_at(&app.world().resource::<LoadedLevel>().0, None, item.pos + glam::Vec3::Z * 8.);
        let materials = app.world().resource::<Assets<WorldMaterial>>();
        let m = materials.get(&material).unwrap();
        assert_eq!((m.params.ambient_cube, m.params.sun_dir, m.params.sun_color), expected);
        assert_eq!(m.params.color, tint);
        assert_eq!(app.world().get::<crate::skin_finish::SkinPart>(meshes[0]).unwrap().applied, Some((1, 2, 3)));
    }

    #[test]
    fn map_teardown_or_cancel_drops_preparation_and_next_map_uses_its_own_vfs() {
        let mut app = fixture_app("first map", &["weapon_ak47"]);
        app.update();
        let weak = Arc::downgrade(&app.world().resource::<GroundModels>()
            .prepared_paths[DROP].as_ref().unwrap().model);
        crate::session::clear_match(app.world_mut());
        assert!(app.world().get_resource::<GroundModels>().is_none());
        assert!(app.world().resource::<GroundInstances>().items.is_empty());
        assert!(weak.upgrade().is_none(), "no map-owned prepared model survives teardown");
        let mut next = fixture_app("next map override", &["weapon_ak47"]);
        next.update();
        assert_eq!(next.world().resource::<GroundModels>()
            .prepared_paths[DROP].as_ref().unwrap().model.name, "next map override");
    }

    #[test]
    fn detached_suppressor_does_not_move_visible_ground_weapon_off_its_pivot() {
        fn mesh(body_part: u32, points: [glam::Vec3; 2]) -> mdl::Mesh {
            mdl::Mesh {
                hardware: Some(mdl::HardwareMesh { body_part, sub_model: 0, mesh: 0, lod: 0 }),
                vertices: points.into_iter().map(|position| mdl::Vertex {
                    position, normal: glam::Vec3::Z, uv: glam::Vec2::ZERO, tangent: None,
                    bones: [0; 3], weights: [1., 0., 0.], bone_count: 1,
                }).collect(),
                ..default()
            }
        }
        let model = mdl::Model {
            body_parts: vec![("studio".into(), vec![], 0), ("silencer".into(), vec![], 0)],
            meshes: vec![
                mesh(0, [glam::Vec3::new(0., -2., 2.), glam::Vec3::new(20., 2., 6.)]),
                mesh(1, [glam::Vec3::new(-50., -10., -20.), glam::Vec3::new(100., 10., 20.)]),
            ],
            ..default()
        };
        let pos = glam::Vec3::new(100., 200., 30.);
        for on_side in [false, true] {
            for yaw in [0., 35., 180.] {
                for attached in [false, true] {
                    let transform = ground_transform_with_suppressor(&model, pos, yaw, on_side, attached);
                    let points: Vec<_> = model.meshes.iter().enumerate()
                        .filter(|(index, _)| !skinning::suppressor_mesh_hidden(&model, *index, attached))
                        .flat_map(|(_, mesh)| mesh.vertices.iter())
                        .map(|v| transform.transform_point3(to_bevy(v.position)))
                        .collect();
                    let lo = points.iter().copied().fold(Vec3::splat(f32::INFINITY), Vec3::min);
                    let hi = points.iter().copied().fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
                    assert!((lo.y - 30.25).abs() < 0.001, "visible gun misses floor");
                    assert!(((lo.x + hi.x) * 0.5 - 100.).abs() < 0.001);
                    assert!(((lo.z + hi.z) * 0.5 + 200.).abs() < 0.001);
                }
                let attached = ground_transform_with_suppressor(&model, pos, yaw, on_side, true);
                assert_eq!(ground_transform(&model, pos, yaw, on_side), attached);
                assert_ne!(ground_transform_with_suppressor(&model, pos, yaw, on_side, false), attached);
            }
        }
    }

    #[test]
    #[ignore = "requires the user's installed CS:GO Legacy assets"]
    fn installed_world_weapons_rest_flat_on_the_surface() {
        let (_, fs) = level::mount_game(None).unwrap();
        let catalog = weapons::Catalog::from_items_game(
            &fs.read_string("scripts/items/items_game.txt").unwrap(),
        )
        .unwrap();
        for name in [
            "weapon_ak47",
            "weapon_m4a1_silencer",
            "weapon_glock",
            "weapon_knife",
        ] {
            let def = catalog.get(name).unwrap();
            let base = def.world_model.trim_end_matches(".mdl");
            let model = mdl::Model::load(
                &fs.read(&def.world_model).unwrap(),
                &fs.read(&format!("{base}.vvd")).unwrap(),
                &fs.read(&format!("{base}.dx90.vtx")).unwrap(),
            )
            .unwrap();
            for yaw in [0.0, 35.0, 180.0] {
                let transform =
                    ground_transform(&model, glam::Vec3::new(100.0, 200.0, 30.0), yaw, true);
                let mut min = f32::INFINITY;
                let mut max = f32::NEG_INFINITY;
                for vertex in model.meshes.iter().flat_map(|m| &m.vertices) {
                    let z = transform.transform_point3(to_bevy(vertex.position)).y;
                    min = min.min(z);
                    max = max.max(z);
                }
                assert!((min - 30.25).abs() < 0.01, "{name} misses the floor: {min}");
                assert!(
                    max - min < 4.0,
                    "{name} is standing upright: height {}",
                    max - min
                );
            }
        }
    }
    #[test]
    fn held_model_pivot_is_removed_for_ground_placement() {
        for roll in [0.0, 90.0] {
            for yaw in [0.0, 35.0, 180.0] {
                let rot = mdl::angles_to_mat3(glam::Vec3::new(0.0, yaw, roll));
                let points = [
                    glam::Vec3::new(15.0, -8.0, 35.0),
                    glam::Vec3::new(50.0, 4.0, 60.0),
                    glam::Vec3::new(20.0, 6.0, 42.0),
                ]
                .map(|p| rot * p);
                let offset = resting_offset(points.into_iter());
                let lo = points
                    .into_iter()
                    .map(|p| p + offset)
                    .fold(glam::Vec3::splat(f32::INFINITY), glam::Vec3::min);
                let hi = points
                    .into_iter()
                    .map(|p| p + offset)
                    .fold(glam::Vec3::splat(f32::NEG_INFINITY), glam::Vec3::max);
                assert!((lo.z - 0.25).abs() < 0.001);
                assert!((lo.x + hi.x).abs() < 0.001 && (lo.y + hi.y).abs() < 0.001);
            }
        }
    }
}

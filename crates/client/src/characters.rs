//! Other players: skinned CS:GO character models animated from the game's
//! animation set, holding their current weapon's world model, lit from the
//! map's ambient light and the sun every few frames.

use crate::coords::{rotation_to_bevy, to_bevy};
use crate::material::WorldMaterial;
use crate::player::{LocalPlayer, SimWorld};
use crate::player_anim::{AnimState, PlayerAnimSet};
use crate::props::Sun;
use crate::skinning::{self, SkinnedAsset, SkinnedInstance};
use crate::textures::TextureCache;
use crate::world_mesh::SharedHandles;
use crate::LoadedLevel;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use collision::contents;
use std::collections::HashMap;
use std::sync::Arc;

#[path = "death_bodies.rs"]
mod death_bodies;

pub const T_MODEL: &str = "models/player/custom_player/legacy/tm_phoenix.mdl";
pub const CT_MODEL: &str = "models/player/custom_player/legacy/ctm_sas.mdl";

pub struct TeamModel {
    pub asset: SkinnedAsset,
    pub anims: Arc<PlayerAnimSet>,
    pub ragdoll: Option<Arc<ragdoll::Rig>>,
}

/// Everything needed to draw players.
#[derive(Resource)]
pub struct CharacterModels {
    pub terrorist: Option<TeamModel>,
    pub counter_terrorist: Option<TeamModel>,
    /// Third-person weapon models by weapon class name.
    pub weapons: HashMap<String, SkinnedAsset>,
    pub sun: Option<Sun>,
}

struct Character {
    team: sim::Team,
    body: SkinnedInstance,
    materials: Vec<Handle<WorldMaterial>>,
    anim: AnimState,
    weapon: Option<HeldWeapon>,
    light_timer: f32,
    light_target: Option<([Vec4;6],Vec4,Vec4)>,
    light_current: Option<([Vec4;6],Vec4,Vec4)>,
    last_pose: Vec<glam::Mat4>,
    pose_life: u64,
}

struct HeldWeapon {
    name: String,
    instance: SkinnedInstance,
    materials: Vec<Handle<WorldMaterial>>,
    merge: skinning::PreparedBoneMerge,
    pose: Vec<glam::Mat4>,
}

#[derive(Resource, Default)]
pub struct Characters(HashMap<sim::PlayerId, Character>, death_bodies::DeathBodies);

impl Characters {
    /// Read only by the opt-in interval report; no per-frame collection.
    pub(crate) fn performance_counts(&self) -> (usize, [usize; 3]) {
        (self.0.len(), self.1.performance_counts())
    }
}

fn load_model(fs: &vfs::Vfs, path: &str) -> Option<mdl::Model> {
    let base = path.trim_end_matches(".mdl");
    let m = fs.read(path)?;
    let v = fs.read(&format!("{base}.vvd"))?;
    let x = fs.read(&format!("{base}.dx90.vtx"))?;
    match mdl::Model::load(&m, &v, &x) {
        Ok(model) => Some(model),
        Err(e) => {
            warn!("{path}: {e}");
            None
        }
    }
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
) -> CharacterModels {
    let fs = &level.vfs;
    let mut team = |path: &str, admitted: Option<&Arc<PlayerAnimSet>>| -> Option<TeamModel> {
        let model = Arc::new(load_model(fs, path)?);
        // These exact team paths are admitted by this same level's VFS. Share
        // its prepared clips; only profile-free content uses the old fallback.
        let anims = match admitted {
            Some(animations) => animations.clone(),
            None => Arc::new(PlayerAnimSet::load(&model, fs)?),
        };
        let asset = skinning::build_asset(model, fs, meshes, materials, images, textures, shared, bindposes, exposure, true);
        let ragdoll = fs.read(path).and_then(|mdl_bytes| {
            let phy_bytes = fs.read(&path.replace(".mdl", ".phy"))?;
            ragdoll::Rig::load(&asset.model, &mdl_bytes, &phy_bytes).map(Arc::new)
        });
        Some(TeamModel { asset, anims, ragdoll })
    };
    let terrorist = team(T_MODEL, level.character_profiles.terrorist.as_ref().map(|p| &p.animations));
    let counter_terrorist = team(CT_MODEL, level.character_profiles.counter_terrorist.as_ref().map(|p| &p.animations));
    let mut weapons = HashMap::new();
    for def in &level.catalog.weapons {
        if def.world_model.is_empty() || weapons.contains_key(&def.class_name) {
            continue;
        }
        if let Some(model) = load_model(fs, &def.world_model) {
            let asset = skinning::build_asset(Arc::new(model), fs, meshes, materials, images, textures, shared, bindposes, exposure, true);
            weapons.insert(def.class_name.clone(), asset);
        }
    }
    for (name,path,_) in crate::cosmetic_catalog::model_items(fs) {
        let icon=path.replace(".mdl","_icon.mdl");
        if let Some(model)=load_model(fs,&icon).or_else(||load_model(fs,&path)) {
            let mut model=model;
            model.meshes.retain(|m|model.textures.get(model.skins.first().and_then(|s|s.get(m.material)).copied().unwrap_or(m.material)).is_some_and(|s|!s.contains("bare_arm")));
            let asset=skinning::build_asset(Arc::new(model),fs,meshes,materials,images,textures,shared,bindposes,exposure,true);
            weapons.insert(name,asset);
        }
    }
    CharacterModels {
        terrorist,
        counter_terrorist,
        weapons,
        sun: Sun::from_bsp(&level.bsp),
    }
}

fn own_materials(instance: &SkinnedInstance, query: &Query<&MeshMaterial3d<WorldMaterial>>) -> Vec<Handle<WorldMaterial>> {
    instance.meshes.iter().filter_map(|e| query.get(*e).ok().map(|m| m.0.clone())).collect()
}

fn despawn_instance(commands: &mut Commands, i: &SkinnedInstance) {
    for e in i.meshes.iter().chain(&i.joints) {
        commands.entity(*e).despawn();
    }
}

/// Ambient cube + sun visibility at a point, in material form.
pub fn lighting_at(level: &level::Level, sun: Option<Sun>, chest: glam::Vec3) -> ([Vec4; 6], Vec4, Vec4) {
    lighting_at_collision(level, &level.collision, sun, chest)
}

/// Runtime models use the simulation's current door/breakable overlay. The
/// wrapper above retains map-only collision for static setup.
pub(crate) fn lighting_at_collision(
    level: &level::Level,
    collision: &collision::CollisionWorld,
    sun: Option<Sun>,
    chest: glam::Vec3,
) -> ([Vec4; 6], Vec4, Vec4) {
    let samples=[chest,chest+glam::Vec3::Z*22.0,chest-glam::Vec3::Z*20.0];
    let mut cube=[[0.0;3];6];let mut count=0.0;
    for pos in samples {
        if !collision.trace_ray(chest,pos,contents::MASK_SOLID).hit() {
            if let Some(sample)=level.bsp.ambient_cube(pos) {
                if sample.iter().flatten().any(|v|*v>0.0001) {
                    for face in 0..6 {for channel in 0..3 {cube[face][channel]+=sample[face][channel];}}
                    count+=1.0;
                }
            }
        }
    }
    if count>0.0 {for face in &mut cube {for v in face {*v/=count;}}}
    else {cube=[[0.035;3];6];}
    // Like the engine's model ambient boost: keep characters readable in
    // dark spots by lifting a dim ambient up to a minimum brightness.
    const AMBIENT_MIN: f32 = 0.12;
    const AMBIENT_MAX_BOOST: f32 = 5.0;
    let lum = cube.iter().map(|c| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]).sum::<f32>() / 6.0;
    if lum < AMBIENT_MIN {
        let k = (AMBIENT_MIN / lum.max(1e-3)).min(AMBIENT_MAX_BOOST);
        for c in &mut cube {
            for v in c.iter_mut() {
                *v *= k;
            }
        }
    }
    let bevy_cube = [cube[0], cube[1], cube[4], cube[5], cube[3], cube[2]].map(|c| Vec4::new(c[0], c[1], c[2], 1.0));
    match sun {
        Some(s) => {
            let lit=samples.iter().filter(|pos| {
                if collision.trace_ray(chest,**pos,contents::MASK_SOLID).hit() {return false}
                let tr=collision.trace_ray(**pos,**pos+s.to_sun*16384.0,contents::MASK_SOLID);
                !tr.hit()||collision.is_sky(tr.surface)
            }).count() as f32/3.0;
            (
                bevy_cube,
                to_bevy(s.to_sun).extend(lit),
                Vec4::new(s.color.x, s.color.y, s.color.z, 1.0),
            )
        }
        None => (bevy_cube, Vec4::new(0.0, 1.0, 0.0, 0.0), Vec4::ZERO),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    world: Res<SimWorld>,
    (local,inventory):(Res<LocalPlayer>,Res<crate::cosmetics::Inventory>),
    mut models: ResMut<CharacterModels>,
    mut chars: ResMut<Characters>,
    mut muzzles: ResMut<crate::tracers::Muzzles>,
    mut materials: ResMut<Assets<WorldMaterial>>,
    level: Res<LoadedLevel>,
    mut effects: ResMut<crate::effects::Effects>,
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    poses: Res<crate::interpolation::RenderPoses>,
    mat_query: Query<&MeshMaterial3d<WorldMaterial>>,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
) {
    let _frame_scope = crate::input_latency::frame_scope("characters");
    muzzles.world.clear();
    muzzles.world_pairs.clear();
    let dt = time.delta_secs();
    let sim_now = world.0.time();
    let sun = models.sun;

    let deaths = std::mem::take(&mut effects.deaths).into_iter().map(|death| {
        let pose = chars.0.get(&death.key.victim)
            .filter(|c| c.pose_life == death.key.life_epoch && c.team == death.victim_team)
            .map(|c| c.last_pose.clone()).filter(|p| !p.is_empty());
        (death, pose)
    }).collect();
    death_bodies::update(&mut commands, &mut chars.1, deaths,
        world.0.round.round, sim_now, &mut models, &mut materials, &level.0, &world.0.collision,
        &mat_query, &mut transforms, &mut visibility);

    // Drop characters whose players left.
    let gone: Vec<sim::PlayerId> = chars.0.keys().copied().filter(|id| world.0.player(*id).is_none()).collect();
    for id in gone {
        if let Some(c) = chars.0.remove(&id) {
            despawn_instance(&mut commands, &c.body);
            if let Some(w) = &c.weapon {
                despawn_instance(&mut commands, &w.instance);
            }
        }
    }

    for p in &world.0.players {
        let render = poses.sample_at(p, fixed.overstep_fraction(), world.0.tick, world.0.tick_rate);
        let team_model = match p.team {
            sim::Team::Terrorist => models.terrorist.as_ref(),
            _ => models.counter_terrorist.as_ref(),
        };
        let Some(team_model) = team_model else { continue };
        if !chars.0.contains_key(&p.id) || chars.0[&p.id].team != p.team {
            if let Some(old) = chars.0.remove(&p.id) {
                despawn_instance(&mut commands, &old.body);
                if let Some(weapon) = old.weapon {
                    despawn_instance(&mut commands, &weapon.instance);
                }
            }
            let body = skinning::spawn_world(&mut commands, &team_model.asset, Some(&mut materials), crate::skinning_bounds::WorldSkin::Body);
            if p.id==local.id{crate::decorations::tag(&mut commands,&team_model.asset,&body,crate::skin_finish::Target::Gloves,0);}
            chars.0.insert(
                p.id,
                Character {
                    team: p.team,
                    body,
                    materials: Vec::new(),
                    anim: AnimState::default(),
                    weapon: None,
                    light_timer: (p.id % 10) as f32 * 0.015,
                    light_target: None,
                    light_current: None,
                    last_pose: Vec::new(),
                    pose_life: 0,
                },
            );
            // Materials become queryable next frame.
            continue;
        }
        let c = chars.0.get_mut(&p.id).unwrap();
        if c.materials.is_empty() {
            c.materials = own_materials(&c.body, &mat_query);
        }

        // Hidden in first person: the local player, or whoever we spectate.
        // CSR_NO_SELF hides our own body from a noclip camera too, so
        // fidelity captures match the real game's noclip view.
        let own_hidden = p.id == local.id && std::env::var_os("CSR_NO_SELF").is_some();
        let visible = p.alive && !own_hidden && (p.id != local.view_target() || local.noclip);
        for (index,e) in c.body.meshes.iter().chain(c.weapon.iter().flat_map(|w| w.instance.meshes.iter())).enumerate() {
            let default_glove=p.id==local.id && inventory.active("gloves").is_some() && team_model.asset.model.meshes.get(index).is_some_and(|m|team_model.asset.model.textures[team_model.asset.model.skin_texture(0,m.material)].contains("glove"));
            if let Ok(mut v) = visibility.get_mut(*e) {
                *v = if visible && !default_glove { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
        if !visible {
            continue;
        }

        // Weapon in hand follows the active weapon.
        let active = p.active_weapon().map(|w| w.def.class_name.clone()).unwrap_or_default();
        if c.weapon.as_ref().map(|w| w.name.as_str()) != Some(active.as_str()) {
            if let Some(w) = c.weapon.take() {
                despawn_instance(&mut commands, &w.instance);
            }
            if let Some(asset) = models.weapons.get(&active) {
                let inst = skinning::spawn_world(&mut commands, asset, Some(&mut materials), crate::skinning_bounds::WorldSkin::HeldWeapon);
                crate::skin_finish::tag(&mut commands,&inst,crate::skin_finish::Target::Held(p.id));
                crate::decorations::tag(&mut commands,asset,&inst,crate::skin_finish::Target::Held(p.id),0);
                // This instance and its merge share one child/parent pair.
                // Weapon changes drop both; team changes recreate Character;
                // clear_match drops Characters together with CharacterModels.
                c.weapon = Some(HeldWeapon {
                    name: active.clone(),
                    instance: inst,
                    materials: Vec::new(),
                    merge: skinning::PreparedBoneMerge::new(&asset.model.bones, team_model.anims.bones()),
                    pose: Vec::with_capacity(asset.model.bones.len()),
                });
            }
        }
        if let Some(w) = &mut c.weapon {
            if w.materials.is_empty() {
                w.materials = own_materials(&w.instance, &mat_query);
            }
        }

        if let Some(w) = &c.weapon {
            if let Some(asset) = models.weapons.get(&w.name) {
                for (index, entity) in w.instance.meshes.iter().enumerate() {
                    if let Ok(mut value) = visibility.get_mut(*entity) {
                        *value = if skinning::suppressor_mesh_hidden(&asset.model, index,
                            p.active_weapon().is_some_and(|w| w.suppressor_attached)) {
                            Visibility::Hidden
                        } else { Visibility::Inherited };
                    }
                }
            }
        }

        // Sample the replicated tick clock rather than advancing animation from
        // render dt, visibility or locally received sound/fire events.
        let input = render.animation.input;
        c.anim.move_cycle = render.animation.move_cycle;
        let CharacterModels { terrorist, counter_terrorist, weapons: weapon_assets, .. } = &mut *models;
        let anims = match p.team {
            sim::Team::Terrorist => terrorist.as_ref().map(|m| &m.anims),
            _ => counter_terrorist.as_ref().map(|m| &m.anims),
        };
        let Some(anims) = anims else { continue };
        let pose = anims.evaluate_prepared(&mut c.anim, &input, 0.);
        c.last_pose.clone_from(&pose);
        c.pose_life = p.life_epoch;
        let world_from_model = Mat4::from_rotation_translation(
            rotation_to_bevy(mdl::angles_to_mat3(glam::Vec3::new(0.0, render.angles.y, 0.0))),
            to_bevy(render.origin),
        );
        skinning::apply_pose(&c.body, world_from_model, &pose, &mut transforms);
        if let Some(w) = &mut c.weapon {
            if let Some(asset) = weapon_assets.get(&w.name) {
                w.merge.evaluate_into(&pose, &mut w.pose);
                skinning::apply_pose(&w.instance, world_from_model, &w.pose, &mut transforms);
                if let Some(point)=crate::tracers::muzzle_position(&asset.model,&w.pose,effects.left_muzzle(p.id, &w.name)) {
                    muzzles.world.insert(p.id,world_from_model.transform_point3(to_bevy(point)));
                }
                if w.name == "weapon_elite" {
                    let pair = [false, true].map(|left| crate::tracers::muzzle_position(&asset.model, &w.pose, left)
                        .map(|point| world_from_model.transform_point3(to_bevy(point))));
                    muzzles.world_pairs.insert(p.id, pair);
                }
            }
        }

        // Lighting, ~6 times a second.
        c.light_timer -= dt;
        if c.light_timer <= 0.0 {
            c.light_timer = 0.15;
            let chest = render.origin + glam::Vec3::Z * if p.movement.ducked {28.0}else{40.0};
            c.light_target=Some(lighting_at_collision(&level.0, &world.0.collision, sun, chest));
        }
        if let Some((target,dir,color))=c.light_target {
            let (mut cube,mut sun_dir,mut sun_color)=c.light_current.unwrap_or((target,dir,color));
            let blend=1.0-(-12.0*dt.min(0.1)).exp();
            for i in 0..6 {cube[i]=cube[i].lerp(target[i],blend);}
            sun_dir=sun_dir.lerp(dir,blend);sun_color=sun_color.lerp(color,blend);
            c.light_current=Some((cube,sun_dir,sun_color));
            let weapon_mats = c.weapon.iter().flat_map(|w| w.materials.iter());
            for h in c.materials.iter().chain(weapon_mats) {
                // Preserve stable bindings when interpolation produces the
                // same light. Check before obtaining mutable asset access.
                let changed = materials.get(h).is_some_and(|m|
                    m.params.ambient_cube != cube || m.params.sun_dir != sun_dir || m.params.sun_color != sun_color);
                if changed {
                    if let Some(mut m) = materials.get_mut(h) {
                        m.params.ambient_cube = cube;
                        m.params.sun_dir = sun_dir;
                        m.params.sun_color = sun_color;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod lighting_tests {
    use super::*;

    pub(crate) fn sun() -> Sun {
        Sun { to_sun: glam::Vec3::Z, color: glam::Vec3::new(0.7, 0.8, 0.9), ambient: glam::Vec3::ZERO }
    }

    // Synthetic horizontal breakable between the chest and upper sample.
    // The initial level retains the intact overlay while restore rebuilds the
    // simulation overlay, just as a remote correction or round reset does.
    pub(crate) fn install_cover(level: &mut level::Level) {
        use collision::{CollisionWorld, Convex, Plane};
        let mins = glam::Vec3::new(-64., -64., 16.);
        let maxs = glam::Vec3::new(64., 64., 18.);
        let solid = Convex { plane_surfaces: Vec::new(),
            mins, maxs, contents: contents::SOLID, surface: 0,
            planes: vec![
                Plane { normal: glam::Vec3::X, dist: maxs.x },
                Plane { normal: -glam::Vec3::X, dist: -mins.x },
                Plane { normal: glam::Vec3::Y, dist: maxs.y },
                Plane { normal: -glam::Vec3::Y, dist: -mins.y },
                Plane { normal: glam::Vec3::Z, dist: maxs.z },
                Plane { normal: -glam::Vec3::Z, dist: -mins.z },
            ],
        };
        let mut base = CollisionWorld::new();
        base.breakables = vec![collision::breakables::Breakable {
            entity: 0, model: 1, origin: glam::Vec3::ZERO, health: 10., material: 0,
            prop_data: String::new(), bullet_scale: 1., club_scale: 1., minimum_damage: 0.,
            trigger_only: false, solids: vec![solid.clone()], studio: None,
        }].into();
        base.finalize();
        level.collision = CollisionWorld::with_runtime_solids(Arc::new(base), vec![solid], vec![Some(0)]);
        level.bsp.leafs = vec![bsp::Leaf {
            contents: 0, cluster: -1, area: 0, mins: [-64; 3], maxs: [64; 3],
            first_leaf_face: 0, num_leaf_faces: 0, first_leaf_brush: 0, num_leaf_brushes: 0,
        }];
        level.bsp.leaf_ambient_index = vec![(2, 0)];
        level.bsp.leaf_ambient = vec![
            bsp::AmbientSample { cube: [[0.2; 3]; 6], pos: [128, 128, 0] },
            bsp::AmbientSample { cube: [[0.8; 3]; 6], pos: [128, 128, 255] },
        ];
    }

    pub(crate) fn restore_cover(world: &mut sim::World, destroyed: bool) {
        let mut state = world.state();
        state.breakables[0] = sim::breakables::State { health: if destroyed { 0. } else { 10. }, destroyed };
        world.restore(state);
    }

    #[test]
    fn live_cover_restore_changes_sample_reachability_and_sun_but_keeps_static_wrapper() {
        let mut bytes = vec![0u8; 1036];
        bytes[..4].copy_from_slice(b"VBSP");
        bytes[4..8].copy_from_slice(&20i32.to_le_bytes());
        let mut fs = vfs::Vfs::new();
        fs.push_back("lighting fixture", vfs::Mount::Memory(std::collections::BTreeMap::from([
            ("maps/lighting_fixture.bsp".into(), Arc::<[u8]>::from(bytes)),
        ])));
        let mut level = level::Level::load_from_vfs(
            content::ContentId::csgo(content::ContentKind::Map, "lighting_fixture"),
            "lighting_fixture", Default::default(), fs,
        ).unwrap();
        install_cover(&mut level);
        let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
        let chest = glam::Vec3::Z * 8.;
        let initial = lighting_at(&level, Some(sun()), chest);
        assert_eq!(initial, lighting_at_collision(&level, &world.collision, Some(sun()), chest));
        assert_eq!(initial.1.w, 0.);
        restore_cover(&mut world, true);
        let open = lighting_at_collision(&level, &world.collision, Some(sun()), chest);
        assert_eq!(open.1.w, 1.);
        assert_ne!(open.0, initial.0, "the upper ambient sample must become reachable too");
        assert_eq!(open.1.truncate(), initial.1.truncate());
        assert_eq!(open.2, initial.2);
        assert_eq!(lighting_at(&level, Some(sun()), chest), initial);
        restore_cover(&mut world, false);
        assert_eq!(lighting_at_collision(&level, &world.collision, Some(sun()), chest), initial);
    }
}

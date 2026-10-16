//! First-person weapon ("viewmodel").
//!
//! CS:GO draws the held weapon from a `v_*.mdl` with the team's arms model
//! bone-merged onto it, using the weapon's own animations (`ACT_VM_DRAW`,
//! `ACT_VM_IDLE`, `ACT_VM_PRIMARYATTACK`, `ACT_VM_RELOAD`). It is rendered by
//! a dedicated camera on its own render layer with a fresh depth buffer and
//! its own field of view, so the gun never clips into walls.

use crate::coords::{rotation_to_bevy, to_bevy};
use crate::material::WorldMaterial;
use crate::player::{LocalPlayer, SimWorld};
use crate::skinning::{self, SkinnedAsset, SkinnedInstance};
use crate::textures::TextureCache;
use crate::world_mesh::SharedHandles;
use crate::LoadedLevel;
use bevy::camera::visibility::RenderLayers;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use mdl::anim::Clip;
use std::collections::HashMap;
use std::sync::Arc;

mod grenade_hold;
mod inspection;
mod look_motion;
mod settling;

pub const VIEWMODEL_LAYER: usize = 2;
/// CS:GO `viewmodel_fov` default (horizontal degrees at 4:3).
pub const VIEWMODEL_FOV: f32 = 60.0;
/// CS:GO `viewmodel_offset_x/y/z` defaults: right, forward, up.
pub const VIEWMODEL_OFFSET: glam::Vec3 = glam::Vec3::new(1.0, 1.0, -1.0);

pub const T_ARMS: &str = "models/weapons/t_arms_phoenix.mdl";
pub const CT_ARMS: &str = "models/weapons/ct_arms_sas.mdl";

pub fn layer() -> RenderLayers {
    RenderLayers::layer(VIEWMODEL_LAYER)
}

/// Presentation-only attachment sampled from this frame's actual weapon pose.
/// No gameplay state or guessed bottle-space offset is stored here.
#[derive(Resource, Default)]
pub struct HeldWick {
    pub sample: Option<WickSample>,
}

#[derive(Clone, Copy)]
pub struct WickSample {
    pub key: (sim::PlayerId, u64, u64),
    pub position: Vec3,
}

fn held_wick(
    player: &sim::Player,
    show: bool,
    model: &mdl::Model,
    pose: &[glam::Mat4],
    world_from_model: Mat4,
) -> Option<WickSample> {
    let weapon = player.active_weapon()?;
    let state = weapon.grenade_throw?;
    if !player.alive || !show || weapon.def.class_name != "weapon_molotov"
        || state.phase != sim::utility::ThrowPhase::Holding || state.spawned {
        return None;
    }
    let attachment = model.attachments.iter().find(|a| a.name.eq_ignore_ascii_case("Wick"))?;
    let point = pose.get(attachment.bone)?.transform_point3(attachment.position);
    let position = world_from_model.transform_point3(to_bevy(point));
    position.is_finite().then_some(WickSample {
        key: (player.id, player.life_epoch, state.pull_started_tick),
        position,
    })
}

#[cfg(test)]
mod wick_tests {
    use super::*;

    #[test]
    fn wick_follows_current_pose_and_disappears_outside_a_visible_live_hold() {
        let catalog = weapons::Catalog::from_items_game(r#""items_game" { "items" { "46" { "name" "weapon_molotov" "baseitem" "0" "attributes" { "primary default clip size" "1" } } } }"#).unwrap();
        let mut collision = collision::CollisionWorld::new();
        collision.finalize();
        let mut world = sim::World::new(Arc::new(collision));
        let id = world.add_player("wick fixture", sim::Team::Terrorist);
        let p = world.player_mut(id).unwrap();
        p.weapons = vec![sim::weapon::WeaponState::new(Arc::new(catalog.get("weapon_molotov").unwrap().clone()))];
        p.active = 0;
        p.weapons[0].grenade_throw = Some(sim::utility::ThrowState {
            phase: sim::utility::ThrowPhase::Holding,
            mode: sim::utility::ThrowMode::Overhand,
            pull_started_tick: 7, throw_started_tick: None,
            release_requested: false, spawned: false,
        });
        let model = mdl::Model {
            attachments: vec![mdl::Attachment { name: "Wick".into(), bone: 0,
                position: glam::Vec3::new(1., 2., 3.), rotation: glam::Mat3::IDENTITY }],
            ..Default::default()
        };
        let pose = [glam::Mat4::from_translation(glam::Vec3::new(10., 20., 30.))];
        let transform = Mat4::from_translation(Vec3::new(100., 200., 300.));
        let sample = held_wick(p, true, &model, &pose, transform).unwrap();
        assert_eq!(sample.position, Vec3::new(111., 233., 278.));
        assert_eq!(sample.key, (id, p.life_epoch, 7));
        let moved = held_wick(p, true, &model, &pose, Mat4::from_translation(Vec3::Y * 5.) * transform).unwrap();
        assert_eq!(moved.position, sample.position + Vec3::Y * 5.);
        assert!(held_wick(p, false, &model, &pose, transform).is_none());
        p.alive = false;
        assert!(held_wick(p, true, &model, &pose, transform).is_none());
        p.alive = true;
        for phase in [sim::utility::ThrowPhase::PullingPin, sim::utility::ThrowPhase::Throwing] {
            p.weapons[0].grenade_throw.as_mut().unwrap().phase = phase;
            assert!(held_wick(p, true, &model, &pose, transform).is_none());
        }
        p.weapons[0].grenade_throw = None;
        assert!(held_wick(p, true, &model, &pose, transform).is_none());
    }
}

#[cfg(test)]
mod life_tests {
    use super::*;

    fn bones() -> Vec<mdl::skeleton::Bone> {
        ["weapon", "hand"].into_iter().map(|name| mdl::skeleton::Bone {
            name: name.into(), parent: -1, pos: glam::Vec3::new(1., 2., 3.),
            quat: glam::Quat::IDENTITY, rot: glam::Vec3::ZERO,
            pos_scale: glam::Vec3::ONE, rot_scale: glam::Vec3::ONE,
            pose_to_bone: glam::Mat4::IDENTITY, flags: 0,
        }).collect()
    }

    #[test]
    fn a_new_life_clears_a_reload_pose_that_draw_does_not_animate() {
        let bones = bones();
        let mut state = ViewModelState::default();
        assert!(state.begin_life(1, &bones, 1., 4, 5));
        let rest = state.pose.clone();
        let reload = Clip { name: "reload".into(), fps: 30., looping: false, delta: false,
            frames: vec![vec![(glam::Vec3::splat(90.), glam::Quat::from_rotation_x(1.)); 2]],
            animated: vec![true, true] };
        reload.sample_layered(0., &mut state.pose, |_| 1.);
        state.activity = "ACT_VM_RELOAD".into();
        state.reloading = true;
        state.shell_seen = Some(2);
        state.planting = true;
        state.clip_index = 2;
        // Render frames within this life retain the layered animation.
        assert!(!state.begin_life(1, &bones, 2., 8, 9));
        assert_ne!(state.pose, rest);
        assert!(state.begin_life(2, &bones, 3., 10, 11));
        let draw = Clip { name: "draw".into(), animated: vec![true, false], ..reload };
        draw.sample_layered(0., &mut state.pose, |_| 1.);
        assert_eq!(state.pose[1], rest[1], "unanimated hand cannot inherit the old reload");
        assert_eq!(state.activity, "ACT_VM_DRAW");
        assert_eq!((state.started, state.last_attack_seen, state.inspect_seen), (3., 10, 11));
        assert!(!state.reloading && !state.planting);
        assert_eq!((state.clip_index, state.shell_seen), (0, None));
    }

    fn installed_app(weapon_override: Option<&str>) -> (App, sim::PlayerId) {
        let level = level::Level::load("de_dust2", None).expect("installed Legacy Dust II");
        let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
        world.load_spawns(&level.bsp);
        let id = world.add_player("viewmodel regression", sim::Team::CounterTerrorist);
        if let Some(name) = weapon_override {
            let def = world.catalog.get(name).unwrap().clone();
            let player = world.player_mut(id).unwrap();
            player.weapons = vec![sim::weapon::WeaponState::new(Arc::new(def))];
            player.active = 0;
        }
        let player = world.player(id).unwrap();
        let local = LocalPlayer::new(id, player.eye(), 0.);
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<WorldMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mut textures = TextureCache::default();
        let mut bindposes = Assets::<SkinnedMeshInverseBindposes>::default();
        let models = load(&level, &Default::default(), &mut meshes, &mut materials,
            &mut images, &mut textures, &SharedHandles { lightmap: Default::default() },
            &mut bindposes, 1.);
        let mut app = App::new();
        app.insert_resource(SimWorld(world)).insert_resource(local).insert_resource(models)
            .insert_resource(materials).insert_resource(LoadedLevel(level))
            .insert_resource(ViewModelState::default())
            .init_resource::<HeldWick>()
            .insert_resource(crate::tracers::InspectRequest::default())
            .insert_resource(crate::tracers::Muzzles::default())
            .insert_resource(crate::cosmetics::Inventory::default())
            .insert_resource(crate::effects::Effects::default())
            .insert_resource(crate::characters::CharacterModels {
                terrorist: None, counter_terrorist: None, weapons: HashMap::new(), sun: None,
            })
            .insert_resource(Time::<()>::default()).insert_resource(Time::<Fixed>::from_hz(128.))
            .add_systems(Update, update);
        app.world_mut().spawn((Window::default(), bevy::window::PrimaryWindow));
        app.update(); // create the cached instances
        app.update(); // sample the actual authored Draw clip
        (app, id)
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_respawn_restarts_draw_through_the_production_system() {
        let (mut app, id) = installed_app(None);
        let weapon = app.world().resource::<SimWorld>().0.player(id).unwrap().active_weapon().unwrap().def.class_name.clone();
        assert_eq!(weapon, "weapon_m4a1_silencer");
        let initial_pose = app.world().resource::<ViewModelState>().pose.clone();
        let initial_joints = app.world().resource::<ViewModelState>().weapon.as_ref().unwrap().1.joints.clone();
        let reload = app.world().resource::<ViewModels>().weapons[&weapon].clips["ACT_VM_RELOAD"][0].clone();
        {
            let mut state = app.world_mut().resource_mut::<ViewModelState>();
            reload.clip.sample_layered(reload.clip.duration() * 0.5, &mut state.pose,
                |b| reload.weights.get(b).copied().unwrap_or(1.));
            assert_ne!(state.pose, initial_pose);
            state.activity = "ACT_VM_RELOAD".into();
            state.reloading = true;
        }
        app.world_mut().resource_mut::<SimWorld>().0.damage_environment(id, 1000, "fixture");
        app.update(); // death keeps the cached meshes but hides them
        app.world_mut().resource_mut::<SimWorld>().0.respawn(id);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(2));
        app.update(); // same weapon/owner/team, new actual replicated life epoch
        let state = app.world().resource::<ViewModelState>();
        assert_eq!(state.activity, "ACT_VM_DRAW");
        assert_eq!(state.pose, initial_pose, "actual Draw pose must equal the fresh-life baseline");
        assert_eq!(state.weapon.as_ref().unwrap().1.joints, initial_joints,
            "respawn resets playback without allocating replacement model instances");
        assert_eq!(state.life, Some(app.world().resource::<SimWorld>().0.player(id).unwrap().life_epoch));
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_knife_inspect_and_cancel_preserve_the_actual_stab_deadline() {
        let (mut app, id) = installed_app(Some("weapon_knife"));
        let input = sim::TickInput { buttons: movement::buttons::ATTACK2, ..Default::default() };
        let deadline = {
            let mut world = app.world_mut().resource_mut::<SimWorld>();
            world.0.step(&[(id, input.clone())]);
            assert!(world.0.events.iter().any(|e| matches!(e, sim::GameEvent::Melee { secondary: true, .. })));
            let deadline = world.0.player(id).unwrap().active_weapon().unwrap().next_attack;
            assert!(deadline > world.0.time());
            deadline
        };
        let stab = app.world().resource::<ViewModels>().weapons["weapon_knife"].clips["ACT_VM_MISSCENTER2"][0].clone();
        {
            let mut state = app.world_mut().resource_mut::<ViewModelState>();
            stab.clip.sample_layered(0.2, &mut state.pose, |b| stab.weights.get(b).copied().unwrap_or(1.));
            state.activity = "ACT_VM_MISSCENTER2".into();
        }
        let visible_stab = app.world().resource::<ViewModelState>().pose.clone();
        app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
        app.update();
        assert_eq!(app.world().resource::<ViewModelState>().activity, "INSPECT");
        assert_eq!(app.world().resource::<ViewModelState>().pose, visible_stab);
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(100));
        app.update();
        let visible_inspect = app.world().resource::<ViewModelState>().pose.clone();
        assert_ne!(visible_inspect, visible_stab);
        app.world_mut().resource_mut::<crate::tracers::InspectRequest>().cancel();
        app.update();
        assert_eq!(app.world().resource::<ViewModelState>().activity, "INSPECT_CANCEL");
        assert_eq!(app.world().resource::<ViewModelState>().pose, visible_inspect);
        let mut world = app.world_mut().resource_mut::<SimWorld>();
        assert_eq!(world.0.player(id).unwrap().active_weapon().unwrap().next_attack, deadline);
        world.0.step(&[(id, input)]);
        assert!(!world.0.events.iter().any(|e| matches!(e, sim::GameEvent::Melee { .. })));
        let knife = world.0.player(id).unwrap().active_weapon().unwrap();
        assert_eq!(knife.next_attack, deadline);
        assert!(knife.reload_end.is_none());
    }
}

/// Vertical FOV (radians) for a horizontal 4:3 FOV in degrees, as Source scales it.
pub fn vertical_fov(horizontal_4x3_deg: f32) -> f32 {
    2.0 * ((horizontal_4x3_deg.to_radians() * 0.5).tan() * 0.75).atan()
}

/// A decoded sequence: its clip and per-bone weights.
#[derive(Clone)]
struct SeqClip {
    clip: Arc<Clip>,
    weights: Arc<Vec<f32>>,
    /// Authored sequence transition length; not the clip's playback duration.
    fade_in: f32,
    left_muzzle: bool,
    /// Soundscript entries the sequence plays (`AE_CL_PLAYSOUND`), by cycle.
    sounds: Arc<Vec<(f32, String)>>,
}

struct WeaponView {
    scope_center: Option<glam::Vec3>,
    scope_rear: Option<(glam::Vec3, f32)>,
    asset: SkinnedAsset,
    /// Activity → sequences (decoded once).
    clips: HashMap<String, Vec<SeqClip>>,
    grenade_hold: Option<grenade_hold::HoldPoses>,
}

#[derive(Resource, Default)]
pub struct ViewModels {
    weapons: HashMap<String, WeaponView>,
    decorations:HashMap<String,SkinnedAsset>,
    gloves:HashMap<String,SkinnedAsset>,
    arms_t: Option<SkinnedAsset>,
    arms_ct: Option<SkinnedAsset>,
}

impl ViewModels {
    /// Model assets retained before instances exist, including arms and cosmetics.
    pub fn assets(&self) -> impl Iterator<Item = &SkinnedAsset> {
        self.weapons.values().map(|view| &view.asset)
            .chain(self.decorations.values())
            .chain(self.gloves.values())
            .chain(self.arms_t.iter())
            .chain(self.arms_ct.iter())
    }

    /// Animation-event sounds of a weapon's first sequence for an activity
    /// (`ACT_VM_DRAW`, `ACT_VM_RELOAD`, ...) and that sequence's duration.
    pub fn activity_sounds(&self, weapon: &str, activity: &str) -> Option<(f32, Arc<Vec<(f32, String)>>)> {
        let seq = self.weapons.get(weapon)?.clips.get(activity)?.first()?;
        Some((seq.clip.duration(), seq.sounds.clone()))
    }

    pub fn preview(&self,name:&str)->Option<&SkinnedAsset>{self.weapons.get(name).map(|w|&w.asset)}
    pub fn decoration(&self,path:&str)->Option<&SkinnedAsset>{self.decorations.get(path)}
}

#[cfg(test)]
mod asset_ownership_tests {
    use super::*;

    fn asset(name: &str) -> SkinnedAsset {
        SkinnedAsset {
            model: Arc::new(mdl::Model { name: name.into(), ..Default::default() }),
            parts: Vec::new(),
            inverse_bindposes: Default::default(),
        }
    }

    #[test]
    fn preparation_owners_include_weapon_cosmetics_gloves_and_both_arms() {
        let models = ViewModels {
            weapons: HashMap::from([("fixture".into(), WeaponView {
                scope_center: None, scope_rear: None, asset: asset("weapon"), clips: HashMap::new(), grenade_hold: None,
            })]),
            decorations: HashMap::from([("fixture".into(), asset("decoration"))]),
            gloves: HashMap::from([("fixture".into(), asset("glove"))]),
            arms_t: Some(asset("arms_t")),
            arms_ct: Some(asset("arms_ct")),
        };
        let mut names = models.assets().map(|asset| asset.model.name.as_str()).collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["arms_ct", "arms_t", "decoration", "glove", "weapon"]);
    }
}
#[derive(Resource, Default)]
pub struct ViewModelState {
    weapon: Option<(String, SkinnedInstance, Option<SkinnedInstance>, Vec<Handle<WorldMaterial>>)>,
    glove:String,
    team: Option<sim::Team>,
    owner: Option<sim::PlayerId>,
    /// A cached weapon instance can survive death, but its playback cannot.
    life: Option<u64>,
    activity: String,
    clip_index: usize,
    started: f32,
    last_attack_seen: u64,
    inspect_seen: u64,
    reloading: bool,
    /// Last shell insert seen (for the debug trace).
    shell_seen: Option<i32>,
    planting: bool,
    grenade_seen: Option<(u64, sim::utility::ThrowPhase)>,
    suppressor_seen: Option<sim::weapon::SuppressorTransition>,
    light_timer: f32,
    look_motion: look_motion::LookMotion,
    /// Persistent bone-local pose: sequences only overwrite the bones they
    /// animate (weighted), as the game layers masked viewmodel sequences.
    pose: Vec<(glam::Vec3, glam::Quat)>,
    inspection: inspection::Inspection,
}

impl ViewModelState {
    fn begin_life(&mut self, life: u64, bones: &[mdl::skeleton::Bone], now: f32,
        attack_serial: u64, inspect_serial: u64) -> bool {
        if self.life == Some(life) { return false; }
        self.life = Some(life);
        self.activity = "ACT_VM_DRAW".into();
        self.clip_index = 0;
        self.started = now;
        self.last_attack_seen = attack_serial;
        self.inspect_seen = inspect_serial;
        self.reloading = false;
        self.shell_seen = None;
        self.planting = false;
        self.grenade_seen = None;
        self.suppressor_seen = None;
        self.light_timer = 0.0;
        self.look_motion.clear();
        self.inspection = inspection::Inspection::default();
        // Masked clips deliberately preserve untouched bones within one life.
        // They must never inherit the reload/inspect pose of a previous life.
        self.pose = mdl::skeleton::rest_pose(bones);
        true
    }
}

fn load_model(fs: &vfs::Vfs, path: &str) -> Option<mdl::Model> {
    let base = path.trim_end_matches(".mdl");
    mdl::Model::load(&fs.read(path)?, &fs.read(&format!("{base}.vvd"))?, &fs.read(&format!("{base}.dx90.vtx"))?).ok()
}

#[allow(clippy::too_many_arguments)]
pub fn load(
    level: &level::Level,
    catalog:&crate::cosmetics::Catalog,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WorldMaterial>,
    images: &mut Assets<Image>,
    textures: &mut TextureCache,
    shared: &SharedHandles,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    exposure: f32,
) -> ViewModels {
    let fs = &level.vfs;
    let mut out = ViewModels::default();
    let mut build = |model: mdl::Model| {
        skinning::build_asset(Arc::new(model), fs, meshes, materials, images, textures, shared, bindposes, exposure, true)
    };
    for def in &level.catalog.weapons {
        if def.view_model.is_empty() || out.weapons.contains_key(&def.class_name) {
            continue;
        }
        let Some(model) = load_model(fs, &def.view_model) else { continue };
        let mut clips: HashMap<String, Vec<SeqClip>> = HashMap::new();
        let mut grenade_hold = None;
        // Sequences live in the model itself or in its include models
        // (knives), in which case they are retargeted by bone name.
        let mut sources: Vec<mdl::Model> = Vec::new();
        for inc in &model.include_models {
            if let Some(m) = fs.read(inc).and_then(|b| mdl::Model::load_header(&b).ok()) {
                sources.push(m);
            }
        }
        let own = std::iter::once(&model).chain(sources.iter());
        for src in own {
            let ani = (!src.anim_block_file.is_empty()).then(|| fs.read(&src.anim_block_file)).flatten();
            if def.class == weapons::WeaponClass::Grenade && std::ptr::eq(src, &model) {
                grenade_hold = grenade_hold::HoldPoses::load(src, ani.as_deref());
            }
            for seq in &src.sequences {
                if seq.activity.is_empty() && seq.name != "lookat01" && seq.name != "inspect" {
                    continue;
                }
                let Some(&first) = seq.anims.first() else { continue };
                match src.decode_animation(first as usize, ani.as_deref()) {
                    Ok(c) => {
                        let same = std::ptr::eq(src, &model);
                        let (mut clip, weights) = if same {
                            (c, (0..model.bones.len()).map(|b| seq.weight(b)).collect())
                        } else {
                            (
                                mdl::anim::retarget(&c, &src.bones, &model.bones),
                                mdl::anim::retarget_weights(seq, &src.bones, &model.bones),
                            )
                        };
                        if def.class==weapons::WeaponClass::Grenade
                            && matches!(seq.activity.to_ascii_uppercase().as_str(),"ACT_VM_PULLPIN"|"ACT_VM_THROW"|"ACT_VM_RELEASE") {
                            clip.looping=false;
                        }
                        if matches!(seq.activity.as_str(), "ACT_VM_ATTACH_SILENCER" | "ACT_VM_DETACH_SILENCER") {
                            clip.looping = false;
                        }
                        clips.entry(if seq.name == "lookat01" || seq.name == "inspect" {"INSPECT".into()}else{seq.activity.to_ascii_uppercase()}).or_default().push(SeqClip {
                            clip: Arc::new(clip),
                            weights: Arc::new(weights),
                            fade_in: seq.fade_in,
                            left_muzzle: seq.name.to_ascii_lowercase().contains("left"),
                            sounds: Arc::new(seq.events.iter().filter_map(|e| Some((e.cycle, e.client_sound()?.to_string()))).collect()),
                        });
                    }
                    Err(e) => warn!("{}: {}: {e}", def.view_model, seq.name),
                }
            }
        }
        // Knives attack with "miss/hit centre" activities.
        if !clips.contains_key("ACT_VM_PRIMARYATTACK") {
            if let Some(c) = clips.get("ACT_VM_MISSCENTER").cloned() {
                clips.insert("ACT_VM_PRIMARYATTACK".into(), c);
            }
        }
        // Align the optic using its original lens geometry in the idle pose.
        let mut scope_rear = None;
        let scope_center = if def.attr_f32("zoom levels").unwrap_or(0.0) > 0.0 && def.class == weapons::WeaponClass::Rifle {
            let mut idle = mdl::skeleton::rest_pose(&model.bones);
            if let Some(seq) = clips.get("ACT_VM_IDLE").and_then(|c| c.first()) {
                seq.clip.sample_layered(0.0, &mut idle, |b| seq.weights.get(b).copied().unwrap_or(1.0));
            }
            let pose = mdl::skeleton::model_space(&model.bones, &idle);
            let mut center = glam::Vec3::ZERO;
            let mut count = 0;
            for mesh in &model.meshes {
                let material = model.skins.first().and_then(|s| s.get(mesh.material)).copied().unwrap_or(mesh.material);
                let name = model.textures.get(material).map(String::as_str).unwrap_or("");
                let rear = name.contains("scope_glass");
                if !rear && !name.contains("scope_lens_dirt") { continue; }
                let mut min = glam::Vec3::splat(f32::INFINITY);
                let mut max = -min;
                for vertex in &mesh.vertices {
                    let mut position = glam::Vec3::ZERO;
                    for i in 0..vertex.bone_count as usize {
                        let bone = vertex.bones[i] as usize;
                        position += (pose[bone] * model.bones[bone].pose_to_bone).transform_point3(vertex.position) * vertex.weights[i];
                    }
                    min = min.min(position); max = max.max(position);
                    if !rear { center += position; count += 1; }
                }
                if rear && !mesh.vertices.is_empty() {
                    let size = max - min;
                    scope_rear = Some(((min + max) * 0.5, size.y.min(size.z) * 0.5));
                }
            }
            (count > 0).then(|| center / count as f32)
        } else { None };
        out.weapons.insert(def.class_name.clone(), WeaponView { asset: build(model), clips, scope_center, scope_rear, grenade_hold });
    }
    for (name,path,_) in crate::cosmetic_catalog::model_items(fs) {
        if let Some(model)=load_model(fs,&path){for (side,arms) in [("Terrorist",T_ARMS),("CounterTerrorist",CT_ARMS)]{
            if let Some(arms)=load_model(fs,arms){out.gloves.insert(format!("{name}:{side}"),build(crate::cosmetic_geometry::sleeves(model.clone(),&arms)));}
        }}
    }
    {
        for path in catalog.models.values().flat_map(|m|m.stickers.iter().cloned()).chain(["models/weapons/stattrack.mdl".into(),"models/weapons/stattrack_cut.mdl".into()]){
            if !out.decorations.contains_key(&path){if let Some(model)=load_model(fs,&path){out.decorations.insert(path,build(model));}}
        }
    }
    {
        for (name,info) in &catalog.models{
            if info.stickers.is_empty(){continue;}
            let Some(view)=out.weapons.get(name).map(|v|v.asset.model.clone())else{continue};
            let Some(world)=load_model(fs,&info.world)else{continue};
            for path in &info.stickers {
                if let Some(decal)=out.decorations.get(path).map(|a|a.model.clone()){
                    if let Some(model)=crate::cosmetic_geometry::world_decal(&decal,&view,&world){out.decorations.insert(format!("world:{path}"),build(model));}
                    else{warn!("Could not map world sticker {}",path);}
                }
            }
        }
    }
    for (name,_,path) in crate::cosmetic_catalog::model_items(fs){if let Some(model)=load_model(fs,&path){out.decorations.insert(format!("glove:{name}"),build(model));}}
    for (name,view) in &out.weapons {
        if !view.asset.model.bones.iter().any(|b|b.name=="v_weapon.stattrack"){continue;}
        let path=if crate::cosmetics::is_rare_knife(name){"models/weapons/stattrack_cut.mdl"}else{"models/weapons/stattrack.mdl"};
        let (Some(counter),Some(info))=(out.decorations.get(path),catalog.models.get(name))else{continue};
        let Some(world)=load_model(fs,&info.world)else{continue};
        let mut counter=(*counter.model).clone();
        let pose:Vec<_>=view.asset.model.bones.iter().map(|b|b.pose_to_bone.inverse()).collect();
        let merged=skinning::bone_merge(&counter.bones,&view.asset.model.bones,&pose);
        for mesh in &mut counter.meshes {for vertex in &mut mesh.vertices{
            let bone=vertex.bones[0] as usize;
            let transform=merged[bone]*counter.bones[bone].pose_to_bone;
            vertex.position=transform.transform_point3(vertex.position);
            vertex.normal=transform.transform_vector3(vertex.normal).normalize_or_zero();
        }}
        if let Some(model)=crate::cosmetic_geometry::world_decal(&counter,&view.asset.model,&world){
            out.decorations.insert(format!("world:{name}:{path}"),build(model));
        }
    }
    out.arms_t = load_model(fs, T_ARMS).map(&mut build);
    out.arms_ct = load_model(fs, CT_ARMS).map(&mut build);
    out
}

fn despawn(commands: &mut Commands, i: &SkinnedInstance) {
    for e in i.meshes.iter().chain(&i.joints) {
        commands.entity(*e).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    world: Res<SimWorld>,
    local: Res<LocalPlayer>,
    models: Res<ViewModels>,
    mut state: ResMut<ViewModelState>,
    (request,inventory): (Res<crate::tracers::InspectRequest>,Res<crate::cosmetics::Inventory>),
    (mut muzzles, mut wick): (ResMut<crate::tracers::Muzzles>, ResMut<HeldWick>),
    mut materials: ResMut<Assets<WorldMaterial>>,
    effects: Res<crate::effects::Effects>,
    level: Res<LoadedLevel>,
    characters: Res<crate::characters::CharacterModels>,
    (time, fixed): (Res<Time>, Res<Time<Fixed>>),
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    mat_query: Query<&MeshMaterial3d<WorldMaterial>>,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
) {
    let _frame_scope = crate::input_latency::frame_scope("viewmodel");
    muzzles.first=None;
    // Clear before every early return, including death, noclip and switching.
    wick.sample = None;
    let now = time.elapsed_secs();
    let dt = time.delta_secs();
    let Some(p) = world.0.player(local.view_target()) else { return };
    let show = p.alive && !local.noclip && !p.active_weapon().is_some_and(|w| w.hides_viewmodel());
    let active = p.active_weapon().map(|w| w.def.class_name.clone()).unwrap_or_default();

    let glove=if p.id==local.id{inventory.active("gloves").map(|i|i.weapon.as_str()).unwrap_or("")}else{""};
    // Respawn the viewmodel when the weapon or team changes.
    let current = state.weapon.as_ref().map(|w| w.0.clone());
    if current.as_deref() != Some(active.as_str()) || state.team != Some(p.team) || state.owner != Some(p.id) || state.glove!=glove {
        if let Some((_, gun, arms, _)) = state.weapon.take() {
            despawn(&mut commands, &gun);
            if let Some(a) = arms {
                despawn(&mut commands, &a);
            }
        }
        if let Some(view) = models.weapons.get(&active) {
            let gun = skinning::spawn(&mut commands, &view.asset, Some(layer()), Some(&mut *materials));
            crate::skin_finish::tag(&mut commands,&gun,crate::skin_finish::Target::View);
            crate::decorations::tag(&mut commands,&view.asset,&gun,crate::skin_finish::Target::View,VIEWMODEL_LAYER);
            let arms_asset = models.gloves.get(&format!("{}:{:?}",glove,p.team)).or_else(||match p.team {
                sim::Team::Terrorist => models.arms_t.as_ref(),
                _ => models.arms_ct.as_ref(),
            });
            // Knives and some weapons carry their own hands; others get arms.
            let arms = arms_asset.map(|a| skinning::spawn(&mut commands, a, Some(layer()), Some(&mut *materials)));
            if !glove.is_empty(){if let Some(a)=&arms{crate::skin_finish::tag(&mut commands,a,crate::skin_finish::Target::Gloves);}}
            state.glove=glove.into();
            state.weapon = Some((active.clone(), gun, arms, Vec::new()));
            state.team = Some(p.team);
            state.owner = Some(p.id);
            state.life = None;
            state.begin_life(p.life_epoch, &view.asset.model.bones, now,
                effects.attack_serial, request.serial());
            if std::env::var_os("CSR_WEAPON_TRACE").is_some() { info!("weapon activity: {} ACT_VM_DRAW", active); }
        }
        return;
    }
    if let Some(view) = models.weapons.get(&active) {
        if state.begin_life(p.life_epoch, &view.asset.model.bones, now,
            effects.attack_serial, request.serial())
            && std::env::var_os("CSR_WEAPON_TRACE").is_some() {
            info!("weapon activity: {} ACT_VM_DRAW life {}", active, p.life_epoch);
        }
    }
    let Some((name, gun, arms, mats)) = &mut state.weapon else { return };
    if mats.is_empty() {
        *mats = gun
            .meshes
            .iter()
            .chain(arms.iter().flat_map(|a| a.meshes.iter()))
            .filter_map(|e| mat_query.get(*e).ok().map(|m| m.0.clone()))
            .collect();
    }
    let (name, gun, arms) = (name.clone(), gun.clone(), arms.clone());
    let entities: Vec<Entity> = gun.meshes.iter().chain(arms.iter().flat_map(|a| a.meshes.iter())).copied().collect();
    for (index, e) in entities.iter().enumerate() {
        let clear_lens = p.active_weapon().is_some_and(|w| w.zoom_level > 0 && w.def.class == weapons::WeaponClass::Rifle)
            && models.weapons.get(&name).and_then(|view| view.asset.model.meshes.get(index).map(|mesh| {
                let model = &view.asset.model;
                let material = model.skins.first().and_then(|s| s.get(mesh.material)).copied().unwrap_or(mesh.material);
                model.textures.get(material).is_some_and(|m| m.starts_with("scope_") || m.contains("scope_glass"))
            })).unwrap_or(false);
        let hide_suppressor = index < gun.meshes.len() && models.weapons.get(&name).is_some_and(|view|
            skinning::suppressor_mesh_hidden(&view.asset.model, index, p.active_weapon().is_some_and(|w|
                w.suppressor_visible(world.0.time() + fixed.overstep_fraction() * world.0.dt(), world.0.dt()))));
        if let Ok(mut v) = visibility.get_mut(*e) {
            *v = if show && !clear_lens && !hide_suppressor { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
    if !p.alive || local.noclip { state.look_motion.clear(); return; }
    let Some(view) = models.weapons.get(&name) else { return };

    // Pick the activity from the weapon state.
    let w = p.active_weapon();
    let grenade = w.and_then(|w|w.grenade_throw);
    let grenade_key=grenade.map(|s|(s.pull_started_tick,s.phase));
    if grenade_key!=state.grenade_seen {
        // The smoke pull/throw clips animate different bone masks. A pin from
        // the old release must not survive into the next sandbox Draw/throw.
        state.pose=mdl::skeleton::rest_pose(&view.asset.model.bones);
        if grenade_key.is_none() && state.grenade_seen.is_some() {
            state.activity="ACT_VM_DRAW".into();state.started=now;state.clip_index=0;
        }
        state.grenade_seen=grenade_key;
    }
    let suppressor = w.and_then(|w| w.suppressor_transition);
    if suppressor != state.suppressor_seen {
        state.pose = mdl::skeleton::rest_pose(&view.asset.model.bones);
        state.activity = suppressor.map_or("ACT_VM_IDLE", |s| s.activity()).into();
        state.started = now;
        state.clip_index = 0;
        state.suppressor_seen = suppressor;
    }
    if let Some(transition) = suppressor { state.activity = transition.activity().into(); }
    let inspect_action = request.after(&mut state.inspect_seen);
    let knife = w.is_some_and(|w| w.def.class == weapons::WeaponClass::Knife);
    if knife && inspect_action == Some(crate::tracers::InspectAction::Cancel) && state.activity == "INSPECT" {
        state.activity = "INSPECT_CANCEL".into();
        state.started = now;
        state.clip_index = 0;
    }
    let reloading = w.is_some_and(|w| w.reload_end.is_some());
    // Consume reliable shot serials even while a newer suppressor transition
    // owns the pose. A delayed earlier shot must neither replace its clip nor
    // replay after the transition ends.
    let attack = effects.consume_attack(p.id, &name, &mut state.last_attack_seen)
        .filter(|_| suppressor.is_none());
    // Shotguns that load shell by shell follow the simulation's stage:
    // the start, then one insert per shell, then (below) the finish.
    let sim_now = world.0.time() + fixed.overstep_fraction() * world.0.dt();
    let shell = w.and_then(|w| w.shell_stage(sim_now));
    let shell_gun = w.is_some_and(|w| w.shell_reload().is_some());
    if reloading && !state.reloading {
        state.activity = if shell_gun { "ACT_SHOTGUN_RELOAD_START" } else { "ACT_VM_RELOAD" }.into();
        state.started = now;
    } else if !reloading && state.reloading && shell_gun && attack.is_none() && view.clips.contains_key("ACT_SHOTGUN_RELOAD_FINISH") {
        state.activity = "ACT_SHOTGUN_RELOAD_FINISH".into();
        state.started = now;
    } else if p.plant_start.is_some() && !state.planting {
        // C4: the keypad sequence while planting.
        state.activity = "ACT_VM_PRIMARYATTACK".into();
        state.started = now;
    } else if let Some(activity) = attack {
        state.activity = activity.into();
        state.clip_index = (state.clip_index + 1) % 3;
        state.started = now;
    }
    if suppressor.is_none() && attack.is_none() && !reloading && p.plant_start.is_none() && w.is_some_and(|w|w.zoom_level==0 && (knife || w.next_attack<=world.0.time())) && inspect_action == Some(crate::tracers::InspectAction::Start) && view.clips.contains_key("INSPECT") {
        state.activity="INSPECT".into();state.started=now;state.clip_index=0;
    }
    if state.activity=="INSPECT" && w.is_some_and(|w|w.zoom_level>0) {state.activity="ACT_VM_IDLE".into();state.started=now;}
    state.reloading = reloading;
    state.planting = p.plant_start.is_some();
    if let Some(grenade)=grenade {
        let activity=if grenade.phase==sim::utility::ThrowPhase::Throwing {grenade.mode.activity()}else{"ACT_VM_PULLPIN"};
        state.activity=activity.into();state.clip_index=0;
    }
    if let Some((stage, _)) = shell {
        if stage != state.shell_seen && stage.is_some() && std::env::var_os("CSR_WEAPON_TRACE").is_some() {
            info!("weapon activity: {} shell insert {}", name, stage.unwrap_or(0) + 1);
        }
        state.shell_seen = stage;
        if matches!(state.activity.as_str(), "ACT_SHOTGUN_RELOAD_START" | "ACT_VM_RELOAD") {
            let start = stage.is_none() && view.clips.contains_key("ACT_SHOTGUN_RELOAD_START");
            state.activity = if start { "ACT_SHOTGUN_RELOAD_START" } else { "ACT_VM_RELOAD" }.into();
        }
    }
    // The installed draw activities carry explicit first-frame SHOW/HIDE events.
    let activity = if state.activity == "INSPECT_CANCEL" {
        "ACT_VM_IDLE"
    } else if state.activity == "ACT_VM_DRAW" {
        w.map_or("ACT_VM_DRAW", |w| w.draw_activity())
    } else { state.activity.as_str() };
    let clips = view
        .clips
        .get(activity)
        .filter(|c| !c.is_empty())
        .or_else(|| {
            // Some knife variants name only the miss activities.
            match state.activity.as_str() {
                "ACT_VM_HITCENTER2" => view.clips.get("ACT_VM_MISSCENTER2"),
                "ACT_VM_HITCENTER" => view.clips.get("ACT_VM_MISSCENTER"),
                _ => None,
            }
        })
        .or_else(|| view.clips.get("ACT_VM_IDLE"));
    let Some(clips) = clips else { return };
    let mut seq = clips[state.clip_index % clips.len()].clone();
    let mut clip = seq.clip.clone();
    let mut t = now - state.started;
    if let Some(grenade)=grenade {
        let started=grenade.throw_started_tick.unwrap_or(grenade.pull_started_tick);
        // Grenade phase clips are loaded non-looping: holding keeps the final pose.
        t=(sim_now-started as f32*world.0.dt()).clamp(0.,clip.duration());
    }
    if let Some(transition) = suppressor {
        t = (sim_now - transition.started_tick as f32 * world.0.dt()).clamp(0., clip.duration());
    }
    if let (Some((_, frac)), "ACT_SHOTGUN_RELOAD_START" | "ACT_VM_RELOAD") = (shell, state.activity.as_str()) {
        // One pass of the start or insert clip per simulated stage.
        t = frac * clip.duration();
    } else if state.activity == "ACT_VM_RELOAD" {
        // Stretch the reload animation over the simulated reload time.
        if let Some(frac) = w.and_then(|w| w.reload_fraction(sim_now)) {
            t = frac * clip.duration();
        }
    }
    if grenade.is_none() && suppressor.is_none() && state.activity != "ACT_VM_IDLE" && t > clip.duration() && !reloading {
        state.activity = "ACT_VM_IDLE".into();
        state.started = now;
        if let Some(idle) = view.clips.get("ACT_VM_IDLE").and_then(|c| c.first()) {
            seq = idle.clone();
            clip = idle.clip.clone();
            t = 0.0;
        }
    }

    if state.started == now && std::env::var_os("CSR_WEAPON_TRACE").is_some() {
        info!("weapon activity: {} {}", name, state.activity);
    }
    let model = &view.asset.model;
    if state.pose.len() != model.bones.len() {
        state.pose = mdl::skeleton::rest_pose(&model.bones);
    }
    let weights = seq.weights.clone();
    // Restart inspection from what was actually displayed, including an
    // interrupted earlier inspect. Sampling against last frame repeatedly
    // would turn the authored duration into a frame-rate-dependent filter.
    let mut inspection = std::mem::take(&mut state.inspection);
    inspection.begin(
        matches!(state.activity.as_str(), "INSPECT" | "INSPECT_CANCEL").then_some(state.started),
        seq.fade_in,
        &state.pose,
    );
    inspection.sample_base(&mut state.pose);
    clip.sample_layered(t, &mut state.pose, |b| weights.get(b).copied().unwrap_or(1.0));
    inspection.apply(now, &mut state.pose);
    state.inspection = inspection;
    if let Some(hold) = &view.grenade_hold { hold.apply(grenade, &mut state.pose); }
    let pose = mdl::skeleton::model_space(&model.bones, &state.pose);

    // Preserve this frame's camera/aim. Local model presentation is separate
    // from authored clips and never changes the simulated shot direction.
    let view_angles = local.render_angles;
    let rot = mdl::angles_to_mat3(view_angles);
    let eye = local.render_eye;
    let (f, r, u) = movement::angle_vectors(view_angles);
    let mut offset = glam::Vec3::new(VIEWMODEL_OFFSET.y, -VIEWMODEL_OFFSET.x, VIEWMODEL_OFFSET.z);
    if let (Some(center), Some(w)) = (view.scope_center, w) {
        let scoped_fov = w.def.attr_f32("zoom fov 1").unwrap_or(45.0);
        let blend = ((90.0 - w.rendered_fov(world.0.time())) / (90.0 - scoped_fov)).clamp(0.0, 1.0);
        let rear = view.scope_rear.map_or(center, |(position, _)| position);
        offset = offset.lerp(glam::Vec3::new(8.0 - center.x, -rear.y, -rear.z), blend);
    }
    let look_translation = if show && p.id == local.id && local.spectating.is_none()
        && w.is_some_and(|w| w.zoom_level == 0
            && look_motion::observed_profile(level.0.item_schema_sha1(), &w.def)) {
        // The admitted input is flat local look, not simulated recoil punch or
        // a future tracked hand/head pose. Existing rendered punch stays current.
        let look = movement::angle_vectors(glam::Vec3::new(local.pitch, local.yaw, 0.)).0;
        state.look_motion.translation(now, look)
    } else { state.look_motion.clear(); glam::Vec3::ZERO };
    let settling = if show && p.id == local.id && local.spectating.is_none()
        && p.tracked.is_none() && p.aim.is_none()
        && w.is_some_and(|w| w.zoom_level == 0
            && look_motion::observed_profile(level.0.item_schema_sha1(), &w.def)) {
        settling::Settling::sample(p.movement.velocity,
            movement::angle_vectors(glam::Vec3::new(0., local.yaw, 0.)).0,
            p.movement.on_ground, p.movement.duck_amount)
    } else { settling::Settling::default() };
    let settled_offset = settling.offset(offset);
    let origin = eye + f * settled_offset.x - r * settled_offset.y + u * settled_offset.z + look_translation;
    if std::env::var_os("CSR_VIEWMODEL_MOTION_TRACE").is_some() {
        info!("viewmodel motion: time={now:.6} angles={view_angles:?} eye={eye:?} look_translation={look_translation:?} settling_weight={} velocity={:?} on_ground={} settling_translation={:?}",
            settling.weight, p.movement.velocity, p.movement.on_ground, settling.translation);
    }
    let world_from_model = Mat4::from_rotation_translation(rotation_to_bevy(rot * settling.rotation), to_bevy(origin));
    skinning::apply_pose(&gun, world_from_model, &pose, &mut transforms);
    wick.sample = held_wick(p, show, model, &pose, world_from_model);
    if show {if let Some(point)=crate::tracers::muzzle_position(model,&pose,seq.left_muzzle) {
        let muzzle=world_from_model.transform_point3(to_bevy(point));
        let world_fov=vertical_fov(w.map_or(90.0,|w|w.rendered_fov(world.0.time())));
        muzzles.first=Some((p.id,crate::tracers::project_viewmodel_muzzle(muzzle,to_bevy(eye),to_bevy(movement::angle_vectors(local.render_angles).0),world_fov,vertical_fov(VIEWMODEL_FOV))));
    }}
    if let Some(arms) = &arms {
        let arms_model = models.gloves.get(&format!("{}:{:?}",glove,p.team)).or_else(||match p.team {
            sim::Team::Terrorist => models.arms_t.as_ref(),
            _ => models.arms_ct.as_ref(),
        });
        if let Some(a) = arms_model {
            let arms_pose = skinning::bone_merge(&a.model.bones, &model.bones, &pose);
            skinning::apply_pose(arms, world_from_model, &arms_pose, &mut transforms);
        }
    }

    let aperture = if w.is_some_and(|w| w.zoom_level > 0 && w.def.class == weapons::WeaponClass::Rifle) {
        view.scope_rear.map_or(Vec4::ZERO, |(rear, radius)| {
            let lens = rear + offset;
            let focal = window.physical_height() as f32 * 0.5 / (vertical_fov(VIEWMODEL_FOV) * 0.5).tan();
            let distance = lens.x.max(1.0);
            Vec4::new(window.physical_width() as f32 * 0.5 - lens.y * focal / distance,
                window.physical_height() as f32 * 0.5 - lens.z * focal / distance,
                radius * focal / distance * 0.97, 0.0)
        })
    } else { Vec4::ZERO };
    if let Some((_, _, _, mats)) = &state.weapon {
        for handle in mats {
            // Unscoped weapons normally retain ZERO. Rewriting it every
            // frame otherwise recreates their texture bind groups too.
            if materials.get(handle).is_some_and(|m| m.params.scope_aperture != aperture) {
                if let Some(mut material) = materials.get_mut(handle) { material.params.scope_aperture = aperture; }
            }
        }
    }
    state.light_timer -= dt;
    if state.light_timer <= 0.0 {
        state.light_timer = 0.15;
        let (cube, sun_dir, sun_color) = crate::characters::lighting_at(&level.0, characters.sun, eye);
        if let Some((_, _, _, mats)) = &state.weapon {
            for h in mats {
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

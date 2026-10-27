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
mod air_lowering;
mod inspection;
mod knife_variants;
mod look_motion;
mod sequence_pose;
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
        state.completed_knife_index = Some(4);
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
        assert_eq!(state.completed_knife_index, None);
    }

    fn installed_app(weapon_override: Option<&str>) -> (App, sim::PlayerId) {
        let (mut app, id) = installed_unposed_app(weapon_override);
        app.update(); // create the cached instances
        assert_mesh_visibility(&app, Visibility::Hidden);
        app.update(); // sample the actual authored Draw clip
        (app, id)
    }

    fn installed_unposed_app(weapon_override: Option<&str>) -> (App, sim::PlayerId) {
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
        let models = load(&level, &Default::default(), &HashMap::new(), &mut meshes, &mut materials,
            &mut images, &mut textures, &SharedHandles { lightmap: Default::default() },
            &mut bindposes, 1.);
        let mut app = App::new();
        app.insert_resource(SimWorld(world)).insert_resource(local).insert_resource(models)
            .insert_resource(materials).insert_resource(LoadedLevel(level))
            .insert_resource(ViewModelState::default())
            .init_resource::<crate::interpolation::RenderPoses>()
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
        (app, id)
    }

    fn assert_mesh_visibility(app: &App, expected: Visibility) {
        let (_, gun, arms, _) = app.world().resource::<ViewModelState>().weapon.as_ref().unwrap();
        assert!(!gun.meshes.is_empty());
        for mesh in gun.meshes.iter().chain(arms.iter().flat_map(|a| &a.meshes)) {
            assert_eq!(app.world().get::<Visibility>(*mesh), Some(&expected));
        }
    }

    fn joint_transforms(app: &App) -> Vec<Transform> {
        let (_, gun, arms, _) = app.world().resource::<ViewModelState>().weapon.as_ref().unwrap();
        assert!(!gun.joints.is_empty());
        gun.joints.iter().chain(arms.iter().flat_map(|a| &a.joints))
            .map(|joint| *app.world().get::<Transform>(*joint).unwrap()).collect()
    }

    // UNRUN: source-only implementation; root owns Cargo and installed-data checks.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_view_target_visibility_preserves_playback_and_authoritative_state() {
        for weapon in ["weapon_m4a1", "weapon_awp", "weapon_smokegrenade"] {
            let (mut app, id) = installed_unposed_app(Some(weapon));
            let authoritative = app.world().resource::<SimWorld>().0.state();
            app.update();
            assert_mesh_visibility(&app, Visibility::Hidden);
            let unposed = joint_transforms(&app);
            assert!(unposed.iter().all(|t| *t == Transform::IDENTITY));
            app.update();
            assert_mesh_visibility(&app, Visibility::Inherited);
            let posed = joint_transforms(&app);
            assert_ne!(posed, unposed, "{weapon} must pose its joints before reveal");
            let state = app.world().resource::<ViewModelState>();
            let playback = (state.life, state.activity.clone(), state.started, state.pose.clone());
            let joints = state.weapon.as_ref().unwrap().1.joints.clone();
            app.world_mut().resource_mut::<LocalPlayer>().spectating = Some(u32::MAX);
            app.update();
            assert_mesh_visibility(&app, Visibility::Hidden);
            let state = app.world().resource::<ViewModelState>();
            assert_eq!((state.life, state.activity.clone(), state.started, state.pose.clone()), playback,
                "a temporarily missing target must not hard-reset playback or bones");
            assert_eq!(joint_transforms(&app), posed);
            app.world_mut().resource_mut::<LocalPlayer>().spectating = None;
            app.update();
            assert_mesh_visibility(&app, Visibility::Inherited);
            let state = app.world().resource::<ViewModelState>();
            assert_eq!((state.life, state.activity.clone(), state.started),
                (playback.0, playback.1, playback.2));
            assert_eq!(app.world().resource::<ViewModelState>().weapon.as_ref().unwrap().1.joints, joints);
            assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
            app.world_mut().resource_mut::<LocalPlayer>().noclip = true;
            app.update();
            assert_mesh_visibility(&app, Visibility::Hidden);
            app.world_mut().resource_mut::<LocalPlayer>().noclip = false;
            app.update();
            assert_mesh_visibility(&app, Visibility::Inherited);

            // Same weapon/team, different owner: old hands must not flash while
            // the replacement instance still has deferred identity joints.
            let other = {
                let mut world = app.world_mut().resource_mut::<SimWorld>();
                let weapons = world.0.player(id).unwrap().weapons.clone();
                let other = world.0.add_player("view target fixture", sim::Team::CounterTerrorist);
                let player = world.0.player_mut(other).unwrap();
                player.weapons = weapons;
                player.active = 0;
                other
            };
            app.world_mut().resource_mut::<LocalPlayer>().spectating = Some(other);
            app.update();
            assert_mesh_visibility(&app, Visibility::Hidden);
            assert!(joint_transforms(&app).iter().all(|t| *t == Transform::IDENTITY));
            app.update();
            assert_mesh_visibility(&app, Visibility::Inherited);
            assert_eq!(app.world().resource::<ViewModelState>().owner, Some(other));
            app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
            app.update();
            let state = app.world().resource::<ViewModelState>();
            assert_ne!(state.activity, "INSPECT", "local requests cannot inspect a spectator target");
            assert_eq!(state.inspect_seen, app.world().resource::<crate::tracers::InspectRequest>().serial());
            let joints = app.world().resource::<ViewModelState>().weapon.as_ref().unwrap().1.joints.clone();
            app.world_mut().resource_mut::<SimWorld>().0.damage_environment(other, 1000, "fixture");
            app.update();
            assert_mesh_visibility(&app, Visibility::Hidden);
            let authoritative = {
                let mut world = app.world_mut().resource_mut::<SimWorld>();
                world.0.respawn(other);
                // Keep this fixture's selected gun/grenade across respawn so
                // it exercises life changes with the same cached instance.
                let weapons = world.0.player(id).unwrap().weapons.clone();
                let player = world.0.player_mut(other).unwrap();
                player.weapons = weapons;
                player.active = 0;
                world.0.state()
            };
            app.update();
            assert_mesh_visibility(&app, Visibility::Inherited);
            let state = app.world().resource::<ViewModelState>();
            assert_eq!(state.activity, "ACT_VM_DRAW");
            assert_eq!(state.weapon.as_ref().unwrap().1.joints, joints);
            assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
        }
    }

    // UNRUN: source-only implementation; root owns Cargo and installed-data checks.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_unavailable_clips_cannot_reveal_unposed_or_cached_hands() {
        let (mut app, _) = installed_unposed_app(Some("weapon_m4a1"));
        let original = std::mem::take(&mut app.world_mut().resource_mut::<ViewModels>()
            .weapons.get_mut("weapon_m4a1").unwrap().clips);
        app.update();
        assert_mesh_visibility(&app, Visibility::Hidden);
        let unposed = joint_transforms(&app);
        for clips in [HashMap::new(), HashMap::from([("ACT_VM_IDLE".into(), Vec::new())])] {
            app.world_mut().resource_mut::<ViewModels>().weapons.get_mut("weapon_m4a1").unwrap().clips = clips;
            app.update();
            assert_mesh_visibility(&app, Visibility::Hidden);
            assert_eq!(joint_transforms(&app), unposed);
        }
        let mut empty = original["ACT_VM_IDLE"][0].clone();
        Arc::make_mut(&mut empty.clip).frames.clear();
        app.world_mut().resource_mut::<ViewModels>().weapons.get_mut("weapon_m4a1")
            .unwrap().clips.insert("ACT_VM_IDLE".into(), vec![empty]);
        app.update();
        assert_mesh_visibility(&app, Visibility::Hidden);
        assert_eq!(joint_transforms(&app), unposed);
        app.world_mut().resource_mut::<ViewModels>().weapons.get_mut("weapon_m4a1").unwrap().clips = original;
        app.update();
        assert_mesh_visibility(&app, Visibility::Inherited);
        let posed = joint_transforms(&app);
        assert_ne!(posed, unposed);
        app.world_mut().resource_mut::<ViewModels>().weapons.get_mut("weapon_m4a1").unwrap().clips.clear();
        app.update();
        assert_mesh_visibility(&app, Visibility::Hidden);
        assert_eq!(joint_transforms(&app), posed);
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_absolute_sequences_cover_guns_knives_and_all_six_grenades_without_pose_debt() {
        // Source-only candidate: compilation and installed-data checks UNRUN.
        let (app, _) = installed_app(None);
        let models = app.world().resource::<ViewModels>();
        let level = &app.world().resource::<LoadedLevel>().0;
        for name in [
            "ak47", "aug", "awp", "bizon", "cz75a", "deagle", "elite", "famas",
            "fiveseven", "g3sg1", "galilar", "glock", "hkp2000", "m249", "m4a1",
            "m4a1_silencer", "mac10", "mag7", "mp5sd", "mp7", "mp9", "negev",
            "nova", "p250", "p90", "revolver", "sawedoff", "scar20", "sg556",
            "ssg08", "tec9", "ump45", "usp_silencer", "xm1014", "knife", "taser",
            "hegrenade", "flashbang", "smokegrenade", "decoy", "molotov", "incgrenade",
        ] {
            let weapon = format!("weapon_{name}");
            let view = models.weapons.get(&weapon).unwrap_or_else(|| panic!("missing installed {weapon}"));
            let def = level.catalog.get(&weapon).unwrap();
            assert!(look_motion::shared_profile(level.item_schema_sha1(), def),
                "{weapon} must use the accepted shared motion profile");
            assert!(!look_motion::shared_profile(None, def));
            let mut pack = def.clone();
            pack.id = content::ContentId::new(content::GameOrigin::Mod,
                content::ContentKind::Weapon, name);
            assert!(!look_motion::shared_profile(level.item_schema_sha1(), &pack));
            assert!(view.clips.contains_key("ACT_VM_IDLE"), "{weapon} has no idle");
            let mut checked = 0;
            for (activity, clips) in &view.clips {
                for seq in clips.iter().filter(|seq| !seq.clip.delta) {
                    for time in [0., seq.clip.duration() / 3., seq.clip.duration()] {
                        let mut expected = mdl::skeleton::rest_pose(&view.asset.model.bones);
                        seq.clip.sample(time, &mut expected);
                        let mut previous = vec![(glam::Vec3::splat(90.),
                            glam::Quat::from_rotation_y(1.)); expected.len()];
                        sequence_pose::sample(&seq.clip, time, &mut previous,
                            |bone| seq.weights.get(bone).copied().unwrap_or(1.));
                        for (bone, (actual, expected)) in previous.iter().zip(&expected).enumerate() {
                            if seq.weights.get(bone).copied().unwrap_or(1.) >= 0.999 {
                                assert_eq!(actual, expected, "{weapon} {activity} bone {bone} time {time}");
                            }
                        }
                    }
                    checked += 1;
                }
            }
            assert!(checked > 0, "{weapon} has no admitted absolute clips");
        }
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_galil_idle_and_fire_clear_compressed_tracks_after_interrupted_reload() {
        let (mut app, id) = installed_app(Some("weapon_galilar"));
        {
            let view = &app.world().resource::<ViewModels>().weapons["weapon_galilar"];
            let hand = view.asset.model.bone_index("v_weapon.Bip01_R_Hand").unwrap();
            let idle = &view.clips["ACT_VM_IDLE"][0];
            assert!(!idle.clip.animated[hand], "fixture must exercise compressed-out tracks");
            assert_eq!(idle.weights[hand], 1.);
        }
        let reload = app.world().resource::<ViewModels>().weapons["weapon_galilar"]
            .clips["ACT_VM_RELOAD"][0].clone();
        let authoritative = app.world().resource::<SimWorld>().0.state();
        for (activity, index) in [("ACT_VM_IDLE", 0), ("ACT_VM_PRIMARYATTACK", 0),
            ("ACT_VM_PRIMARYATTACK", 1), ("ACT_VM_PRIMARYATTACK", 2)] {
            let expected = {
                let view = &app.world().resource::<ViewModels>().weapons["weapon_galilar"];
                let seq = &view.clips[activity][index];
                assert!(seq.weights.iter().all(|weight| *weight == 1.));
                let mut expected = mdl::skeleton::rest_pose(&view.asset.model.bones);
                seq.clip.sample(0., &mut expected);
                expected
            };
            {
                let mut state = app.world_mut().resource_mut::<ViewModelState>();
                state.activity = activity.into();
                state.clip_index = index;
                state.reloading = false;
                sequence_pose::sample(&reload.clip, reload.clip.duration() * 0.5, &mut state.pose,
                    |bone| reload.weights.get(bone).copied().unwrap_or(1.));
                assert_ne!(state.pose, expected, "fixture must interrupt an actual changed reload pose");
            }
            app.update();
            let state = app.world().resource::<ViewModelState>();
            assert_eq!(state.pose, expected, "{activity} variant {index} retains no previous pose");
            assert_eq!(state.owner, Some(id));
        }
        assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_weapon_switch_keeps_accepted_look_history_within_the_same_life() {
        // Source-only candidate: compilation and installed-data checks UNRUN.
        let (mut app, id) = installed_app(Some("weapon_m4a1"));
        app.world_mut().resource_mut::<LocalPlayer>().yaw = 10.;
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(10));
        app.update();
        let now = app.world().resource::<Time>().elapsed_secs();
        let forward = movement::angle_vectors(glam::Vec3::new(0., 10., 0.)).0;
        let before = app.world_mut().resource_mut::<ViewModelState>().look_motion.translation(now, forward);
        assert!(before.length() > 0.2, "fixture must retain a delayed look sample");
        for weapon in ["weapon_galilar", "weapon_smokegrenade", "weapon_m4a1"] {
            {
                let mut world = app.world_mut().resource_mut::<SimWorld>();
                let def = Arc::new(world.0.catalog.get(weapon).unwrap().clone());
                world.0.player_mut(id).unwrap().weapons = vec![sim::weapon::WeaponState::new(def)];
            }
            let authoritative = app.world().resource::<SimWorld>().0.state();
            app.update(); // replacement instances remain hidden until posed
            assert_mesh_visibility(&app, Visibility::Hidden);
            app.update();
            assert_mesh_visibility(&app, Visibility::Inherited);
            let after = app.world_mut().resource_mut::<ViewModelState>().look_motion.translation(now, forward);
            assert_eq!(after, before, "{weapon} must not restart the player's look history");
            assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
        }
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_airborne_weapon_switch_records_the_hidden_frame_before_the_delayed_reversal() {
        let (mut app, id) = installed_app(Some("weapon_m4a1"));
        // This synthetic frame schedule isolates presentation sampling, not a
        // native jump law. A look excursion exists ONLY on the replacement
        // frame. The old same-timestamp switch fixture cannot detect its loss.
        for step_ms in [5, 10, 30] {
            for weapon in ["weapon_galilar", "weapon_smokegrenade", "weapon_m4a1"] {
                let advance = |app: &mut App, millis| {
                    app.world_mut().resource_mut::<Time>()
                        .advance_by(std::time::Duration::from_millis(millis));
                    app.update();
                };
                advance(&mut app, 10); // current direction is zero yaw
                {
                    let mut world = app.world_mut().resource_mut::<SimWorld>();
                    let def = Arc::new(world.0.catalog.get(weapon).unwrap().clone());
                    let player = world.0.player_mut(id).unwrap();
                    player.weapons = vec![sim::weapon::WeaponState::new(def)];
                    player.movement.on_ground = false;
                    player.movement.velocity = glam::Vec3::new(225., 0., 100.);
                }
                let authoritative = app.world().resource::<SimWorld>().0.state();
                app.world_mut().resource_mut::<LocalPlayer>().yaw = 10.;
                advance(&mut app, 10); // hidden instance creation, excursion
                assert_mesh_visibility(&app, Visibility::Hidden);
                let life = app.world().resource::<ViewModelState>().life;
                app.world_mut().resource_mut::<LocalPlayer>().yaw = 0.;
                advance(&mut app, 10); // visible again, direction has returned
                assert_mesh_visibility(&app, Visibility::Inherited);
                // Ninety milliseconds later, the delayed time reaches the
                // hidden frame. Coarse/fine following frames must retain it.
                for _ in 0..90 / step_ms { advance(&mut app, step_ms); }
                let now = app.world().resource::<Time>().elapsed_secs();
                let delayed = app.world_mut().resource_mut::<ViewModelState>()
                    .look_motion.translation(now, glam::Vec3::X);
                assert!(delayed.length() > 0.2,
                    "{weapon}, {step_ms}ms: replacement must retain the look excursion");
                assert_eq!(app.world().resource::<ViewModelState>().life, life);
                assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative,
                    "render frames cannot change airborne simulation state");
                {
                    let mut world = app.world_mut().resource_mut::<SimWorld>();
                    let player = world.0.player_mut(id).unwrap();
                    player.movement.on_ground = true;
                    player.movement.velocity.z = 0.;
                }
                let landed = app.world().resource::<SimWorld>().0.state();
                advance(&mut app, 20);
                let now = app.world().resource::<Time>().elapsed_secs();
                let settled = app.world_mut().resource_mut::<ViewModelState>()
                    .look_motion.translation(now, glam::Vec3::X);
                assert!(settled.length() < 0.00001, "finite look history must settle");
                assert_eq!(app.world().resource::<ViewModelState>().life, life);
                assert_eq!(app.world().resource::<SimWorld>().0.state(), landed);
                assert_mesh_visibility(&app, Visibility::Inherited);
            }
        }
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_weapon_movement_uses_the_rendered_takeoff_timeline() {
        use std::time::Duration;
        for rate in [64, 128] {
            for weapon in ["weapon_m4a1", "weapon_awp", "weapon_smokegrenade"] {
                let (mut app, id) = installed_app(Some(weapon));
                let mut history = crate::interpolation::RenderPoses::default();
                {
                    let mut world = app.world_mut().resource_mut::<SimWorld>();
                    world.0.set_tick_rate(rate);
                    let actor = world.0.player_mut(id).unwrap();
                    actor.movement.on_ground = true;
                    actor.movement.velocity = glam::Vec3::ZERO;
                    history.capture(&world.0);
                    world.0.tick += 1;
                    let actor = world.0.player_mut(id).unwrap();
                    actor.movement.on_ground = false;
                    actor.movement.velocity = glam::Vec3::new(225., 0., 250.);
                    history.capture(&world.0);
                }
                let mut fixed = Time::<Fixed>::from_hz(rate as f64);
                fixed.accumulate_overstep(Duration::from_secs_f64(0.5 / rate as f64));
                app.insert_resource(fixed).insert_resource(history);
                let alpha = app.world().resource::<Time<Fixed>>().overstep_fraction();
                let sampled = {
                    let world = app.world().resource::<SimWorld>();
                    app.world().resource::<crate::interpolation::RenderPoses>()
                        .sample_at(world.0.player(id).unwrap(), alpha, world.0.tick, rate)
                };
                assert!(sampled.animation.input.on_ground, "takeoff is not yet on this timeline");
                assert!((sampled.velocity.x - 112.5).abs() < 0.0001);
                let authoritative = app.world().resource::<SimWorld>().0.state();
                app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(100));
                app.update();
                let rendered = joint_transforms(&app);
                assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);

                // A raw pose at the sampled speed/ground state must produce
                // exactly the same gun and hands at the same camera/time.
                {
                    let mut world = app.world_mut().resource_mut::<SimWorld>();
                    let actor = world.0.player_mut(id).unwrap();
                    actor.movement.velocity = sampled.velocity;
                    actor.movement.on_ground = sampled.animation.input.on_ground;
                }
                app.insert_resource(crate::interpolation::RenderPoses::default());
                app.world_mut().resource_mut::<ViewModelState>().air_lowering.clear();
                let reference = app.world().resource::<SimWorld>().0.state();
                app.update();
                for (actual, expected) in rendered.iter().zip(joint_transforms(&app)) {
                    assert!((actual.translation - expected.translation).length() < 0.001,
                        "{weapon} at {rate}: weapon motion led the rendered actor");
                    assert!(actual.rotation.abs_diff_eq(expected.rotation, 0.00001));
                }
                assert_eq!(app.world().resource::<SimWorld>().0.state(), reference);
            }
        }
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_air_lowering_moves_gun_and_hands_together_without_changing_simulation() {
        use std::time::Duration;
        for weapon in ["weapon_m4a1", "weapon_awp", "weapon_smokegrenade"] {
            let (mut app, id) = installed_app(Some(weapon));
            {
                let mut world = app.world_mut().resource_mut::<SimWorld>();
                let player = world.0.player_mut(id).unwrap();
                player.movement.on_ground = true;
                player.movement.velocity = glam::Vec3::ZERO;
            }
            app.world_mut().resource_mut::<LocalPlayer>().render_angles = glam::Vec3::new(20., 45., 0.);
            app.update();
            app.world_mut().resource_mut::<SimWorld>().0.player_mut(id).unwrap().movement.on_ground = false;
            let airborne = app.world().resource::<SimWorld>().0.state();
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(100));
            app.update();
            assert_mesh_visibility(&app, Visibility::Inherited);
            let lowered = joint_transforms(&app);
            // Same source pose, camera and timestamp, with only the presentation
            // history cleared. Every gun/arm joint must share the same offset.
            app.world_mut().resource_mut::<ViewModelState>().air_lowering.clear();
            app.update();
            let neutral = joint_transforms(&app);
            let expected = to_bevy(movement::angle_vectors(glam::Vec3::new(20., 45., 0.)).2 * -0.75);
            for (low, rest) in lowered.iter().zip(&neutral) {
                assert!((low.translation - rest.translation - expected).length() < 0.001,
                    "{weapon}: gun and hands must lower in their current view basis");
                assert!(low.rotation.abs_diff_eq(rest.rotation, 0.00001),
                    "{weapon}: lowering cannot rotate a joint: {:?} vs {:?}", low.rotation, rest.rotation);
            }
            assert_eq!(app.world().resource::<SimWorld>().0.state(), airborne);

            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(100));
            app.update();
            app.world_mut().resource_mut::<SimWorld>().0.player_mut(id).unwrap().movement.on_ground = true;
            let landed = app.world().resource::<SimWorld>().0.state();
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(20));
            app.update();
            let now = app.world().resource::<Time>().elapsed_secs();
            let returning = app.world_mut().resource_mut::<ViewModelState>().air_lowering.sample(now, false);
            assert!(returning > -0.75 && returning < 0., "landing must return continuously");
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(100));
            app.update();
            let now = app.world().resource::<Time>().elapsed_secs();
            assert_eq!(app.world_mut().resource_mut::<ViewModelState>().air_lowering.sample(now, false), 0.);
            assert_eq!(app.world().resource::<SimWorld>().0.state(), landed);

            // Hiding the viewing life discards its presentation history.
            app.world_mut().resource_mut::<SimWorld>().0.player_mut(id).unwrap().alive = false;
            app.update();
            assert_mesh_visibility(&app, Visibility::Hidden);
            assert_eq!(app.world_mut().resource_mut::<ViewModelState>().air_lowering.sample(now, true), 0.);
        }
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_magazine_reload_finishes_at_the_authoritative_deadline() {
        // Joining a reload late can give rendering a newer start time than sim.
        // Keep render time frozen while ordinary TickInput ticks finish it.
        let (mut app, id) = installed_app(Some("weapon_ak47"));
        {
            let mut world = app.world_mut().resource_mut::<SimWorld>();
            world.0.player_mut(id).unwrap().weapons[0].clip -= 5;
            world.0.step(&[(id, sim::TickInput {
                buttons: movement::buttons::RELOAD, ..Default::default()
            })]);
            assert!(world.0.player(id).unwrap().active_weapon().unwrap().reload_end.is_some());
        }
        app.update();
        assert_eq!(app.world().resource::<ViewModelState>().activity, "ACT_VM_RELOAD");
        {
            let mut world = app.world_mut().resource_mut::<SimWorld>();
            let end = world.0.player(id).unwrap().active_weapon().unwrap().reload_end.unwrap();
            // World::step processes the current tick, then advances its clock.
            // Include the first processed tick at/after a fractional deadline.
            while world.0.time() < end + world.0.dt() {
                world.0.step(&[(id, sim::TickInput::default())]);
            }
            assert!(world.0.player(id).unwrap().active_weapon().unwrap().reload_end.is_none());
        }
        let authoritative = app.world().resource::<SimWorld>().0.state();
        app.update();
        let state = app.world().resource::<ViewModelState>();
        assert_eq!(state.activity, "ACT_VM_IDLE", "render elapsed time cannot prolong a completed reload");
        assert!(!state.reload_from_inspect);
        assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
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
    fn installed_reload_interrupts_inspect_through_the_incoming_authored_fade() {
        let (mut app, id) = installed_app(Some("weapon_ak47"));
        app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
        app.update();
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(100));
        app.update();
        assert_eq!(app.world().resource::<ViewModelState>().activity, "INSPECT");
        let displayed = app.world().resource::<ViewModelState>().pose.clone();
        let fade = app.world().resource::<ViewModels>().weapons["weapon_ak47"].clips["ACT_VM_RELOAD"][0].fade_in;
        assert!(fade.is_finite() && fade > 0.);
        let authoritative = {
            let mut world = app.world_mut().resource_mut::<SimWorld>();
            world.0.player_mut(id).unwrap().weapons[0].clip -= 5;
            world.0.step(&[(id, sim::TickInput {
                buttons: movement::buttons::RELOAD, ..Default::default()
            })]);
            assert!(world.0.events.iter().any(|e| matches!(e, sim::GameEvent::Reload { player } if *player == id)));
            assert!(world.0.player(id).unwrap().active_weapon().unwrap().reload_end.is_some());
            world.0.state()
        };
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(1));
        app.update();
        let state = app.world().resource::<ViewModelState>();
        assert_eq!(state.activity, "ACT_VM_RELOAD");
        assert_eq!(state.pose, displayed, "the actual reload starts from the displayed inspect pose");
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(fade * 0.5));
        app.update();
        assert_ne!(app.world().resource::<ViewModelState>().pose, displayed,
            "the authored reload fade progresses instead of restarting every render frame");
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(fade));
        app.update();
        assert!(app.world().resource::<ViewModelState>().pose.iter().all(|(p, q)| p.is_finite() && q.is_finite()));
        assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative,
            "presentation cannot change the actual reload deadline, ammunition or readiness");
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_knife_inspect_and_reload_rearm_preserve_the_actual_stab_deadline() {
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
        assert_eq!(app.world().resource::<ViewModelState>().activity, "INSPECT");
        assert!(app.world().resource::<ViewModelState>().knife_reinspect.is_some());
        assert_eq!(app.world().resource::<ViewModelState>().pose, visible_inspect);
        let mut world = app.world_mut().resource_mut::<SimWorld>();
        assert_eq!(world.0.player(id).unwrap().active_weapon().unwrap().next_attack, deadline);
        world.0.step(&[(id, input)]);
        assert!(!world.0.events.iter().any(|e| matches!(e, sim::GameEvent::Melee { .. })));
        let knife = world.0.player(id).unwrap().active_weapon().unwrap();
        assert_eq!(knife.next_attack, deadline);
        assert!(knife.reload_end.is_none());
    }

    // UNRUN: source/data-only handoff; root owns Cargo and runtime verification.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_early_knife_models_retain_their_authored_activity_sets() {
        let (app, _) = installed_app(Some("weapon_knife"));
        let views = app.world().resource::<ViewModels>();
        for name in ["weapon_knife", "weapon_knife_t", "weapon_bayonet", "weapon_knife_flip",
            "weapon_knife_gut", "weapon_knife_karambit", "weapon_knife_m9_bayonet",
            "weapon_knife_tactical", "weapon_knife_butterfly", "weapon_knife_falchion", "weapon_knife_push"] {
            let view = views.weapons.get(name).expect(name);
            for activity in ["ACT_VM_DRAW", "ACT_VM_IDLE", "ACT_VM_MISSCENTER", "ACT_VM_MISSCENTER2", "INSPECT"] {
                let clips = view.clips.get(activity).expect(name);
                assert!(!clips.is_empty(), "{name}: {activity}");
                assert!(clips.iter().all(|seq| !seq.clip.frames.is_empty()
                    && seq.clip.frames[0].len() == view.asset.model.bones.len()));
            }
        }
        assert_eq!(views.weapons["weapon_knife_butterfly"].clips["INSPECT"].len(), 3);
        assert_eq!(views.weapons["weapon_knife_butterfly"].clips["ACT_VM_DRAW"].iter()
            .map(|seq| seq.activity_weight).collect::<Vec<_>>(), [1, 4]);
        assert_eq!(views.weapons["weapon_knife_butterfly"].clips["INSPECT"].iter()
            .map(|seq| seq.activity_weight).collect::<Vec<_>>(), [1, 1, 1]);
        assert_eq!(views.weapons["weapon_knife_falchion"].clips["INSPECT"].len(), 2);
        assert_eq!(views.weapons["weapon_knife_falchion"].clips["INSPECT"].iter()
            .map(|seq| seq.activity_weight).collect::<Vec<_>>(), [100, 1]);
        assert_eq!(views.weapons["weapon_knife_push"].clips["ACT_VM_MISSCENTER"].len(), 5);
    }

    // UNRUN: selection through production dispatch; no native RNG parity claim.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_butterfly_draw_and_reinspect_select_and_hold_authored_variants() {
        let (mut app, id) = installed_app(Some("weapon_knife_butterfly"));
        app.world_mut().resource_mut::<ViewModelState>().knife_variants = knife_variants::KnifeVariants::seeded(0);
        let mut expected = knife_variants::KnifeVariants::seeded(0);
        let draw = app.world().resource::<ViewModels>().weapons["weapon_knife_butterfly"].clips["ACT_VM_DRAW"].clone();
        let inspect = app.world().resource::<ViewModels>().weapons["weapon_knife_butterfly"].clips["INSPECT"].clone();
        let mut draws = std::collections::HashSet::new();
        let mut inspections = std::collections::HashSet::new();
        for _ in 0..32 {
            for name in ["weapon_knife", "weapon_knife_butterfly"] {
                let mut world = app.world_mut().resource_mut::<SimWorld>();
                let def = Arc::new(world.0.catalog.get(name).unwrap().clone());
                world.0.player_mut(id).unwrap().weapons[0] = sim::weapon::WeaponState::new(def);
                drop(world);
                app.update();
                app.update();
            }
            let authoritative = app.world().resource::<SimWorld>().0.state();
            let index = expected.choose(&draw);
            assert_eq!(app.world().resource::<ViewModelState>().clip_index, index);
            draws.insert(index);
            app.update();
            assert_eq!(app.world().resource::<ViewModelState>().clip_index, index,
                "a rendered Draw must not reroll its selected clip");

            for _ in 0..3 {
                app.world_mut().resource_mut::<crate::tracers::InspectRequest>().cancel();
                app.update();
                app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
                app.update();
                let index = expected.choose(&inspect);
                assert_eq!(app.world().resource::<ViewModelState>().clip_index, index);
                inspections.insert(index);
                app.update();
                assert_eq!(app.world().resource::<ViewModelState>().clip_index, index,
                    "an ongoing inspect must not reroll its selected clip");
            }
            assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
        }
        assert_eq!(draws.len(), 2, "both installed openings must be reachable");
        assert_eq!(inspections.len(), 3, "all three installed inspect variants must be reachable");
    }

    // UNRUN: real dispatch and tick foley share one installed Draw selection.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_selected_butterfly_draw_schedules_its_own_foley_once() {
        fn equip(app: &mut App, id: sim::PlayerId, name: &str) {
            let mut world = app.world_mut().resource_mut::<SimWorld>();
            let def = Arc::new(world.0.catalog.get(name).unwrap().clone());
            world.0.player_mut(id).unwrap().weapons[0] = sim::weapon::WeaponState::new(def);
            world.0.tick += 1;
        }

        let (mut app, id) = installed_unposed_app(Some("weapon_knife"));
        app.init_resource::<crate::audio::SoundLibrary>()
            .init_resource::<Assets<bevy::audio::AudioSource>>()
            .init_resource::<crate::surfaces::SurfaceProps>()
            .init_resource::<crate::foley::Foley>()
            .add_systems(FixedUpdate, crate::foley::update)
            .add_systems(Update, crate::foley::inspect.after(update));
        let clips = app.world().resource::<ViewModels>().weapons["weapon_knife_butterfly"].clips["ACT_VM_DRAW"].clone();
        assert_eq!(clips.len(), 2);
        let seed_for = |index| (0..128).find(|seed|
            knife_variants::KnifeVariants::seeded(*seed).choose(&clips) == index).unwrap();
        for render_first in [false, true] {
            for (index, entry) in [(0, "ButterflyKnife.draw01"), (1, "ButterflyKnife.draw02")] {
                equip(&mut app, id, "weapon_knife");
                app.world_mut().run_schedule(FixedUpdate);
                app.update();
                app.update();
                app.world_mut().resource_mut::<crate::foley::Foley>().scheduled.clear();
                app.world_mut().resource_mut::<ViewModelState>().knife_variants =
                    knife_variants::KnifeVariants::seeded(seed_for(index));
                equip(&mut app, id, "weapon_knife_butterfly");
                let authoritative = app.world().resource::<SimWorld>().0.state();
                let deployed = app.world().resource::<SimWorld>().0.time();
                if render_first { app.update(); }
                app.world_mut().run_schedule(FixedUpdate);
                app.update();
                app.update();
                let state = app.world().resource::<ViewModelState>();
                assert_eq!(state.activity, "ACT_VM_DRAW");
                assert_eq!(state.clip_index, index);
                let serial = state.knife_draw_serial;
                assert_eq!(app.world().resource::<crate::foley::Foley>().scheduled,
                    [(id, "weapon_knife_butterfly".to_string(), entry.to_string(), deployed)],
                    "selected Draw must schedule its declared cycle-zero cue once, at the original deploy time");
                assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);

                app.world_mut().run_schedule(FixedUpdate); // repeated tick
                app.update();
                app.world_mut().resource_mut::<SimWorld>().0.tick += 1;
                app.world_mut().run_schedule(FixedUpdate); // later tick, same weapon
                app.update();
                assert_eq!(app.world().resource::<ViewModelState>().knife_draw_serial, serial);
                assert_eq!(app.world().resource::<ViewModelState>().clip_index, index);
                assert_eq!(app.world().resource::<crate::foley::Foley>().scheduled.len(), 1);
            }
        }

        // Foley sees an intervening switch which rendering never presents.
        // Returning to the same weapon must still present that new deployment.
        equip(&mut app, id, "weapon_knife");
        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut().resource_mut::<ViewModelState>().knife_variants =
            knife_variants::KnifeVariants::seeded(seed_for(0));
        app.world_mut().resource_mut::<crate::foley::Foley>().scheduled.clear();
        equip(&mut app, id, "weapon_knife_butterfly");
        let authoritative = app.world().resource::<SimWorld>().0.state();
        let deployed = app.world().resource::<SimWorld>().0.time();
        app.world_mut().run_schedule(FixedUpdate);
        app.update();
        assert_eq!(app.world().resource::<ViewModelState>().clip_index, 0);
        assert_eq!(app.world().resource::<ViewModelState>().activity, "ACT_VM_DRAW");
        assert_eq!(app.world().resource::<crate::foley::Foley>().scheduled,
            [(id, "weapon_knife_butterfly".to_string(), "ButterflyKnife.draw01".to_string(), deployed)]);
        assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);

        // Firearms and utility retain the first-sequence declaration and the
        // existing simulation-time event schedule; they do not use this cache.
        for name in ["weapon_m4a1", "weapon_smokegrenade"] {
            app.world_mut().resource_mut::<crate::foley::Foley>().scheduled.clear();
            equip(&mut app, id, name);
            let deployed = app.world().resource::<SimWorld>().0.time();
            let (duration, events) = app.world().resource::<ViewModels>()
                .activity_sounds(name, "ACT_VM_DRAW").unwrap();
            let expected: Vec<_> = events.iter().map(|(cycle, entry)|
                (id, name.to_string(), entry.clone(), deployed + cycle * duration)).collect();
            let authoritative = app.world().resource::<SimWorld>().0.state();
            app.world_mut().run_schedule(FixedUpdate);
            app.update();
            app.update();
            assert_eq!(app.world().resource::<crate::foley::Foley>().scheduled, expected);
            assert!(app.world().resource::<ViewModelState>().knife_draw_choice.is_none());
            assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
        }
    }

    fn install_inspect_foley(app: &mut App) {
        app.init_resource::<crate::audio::SoundLibrary>()
            .init_resource::<Assets<bevy::audio::AudioSource>>()
            .init_resource::<crate::foley::Foley>()
            .add_systems(Update, crate::foley::inspect.after(update));
    }

    fn equip_inspect_fixture(app: &mut App, id: sim::PlayerId, name: &str) {
        let mut world = app.world_mut().resource_mut::<SimWorld>();
        let def = Arc::new(world.0.catalog.get(name).unwrap().clone());
        world.0.player_mut(id).unwrap().weapons = vec![sim::weapon::WeaponState::new(def)];
        world.0.player_mut(id).unwrap().active = 0;
        world.0.set_loadout(id, sim::Loadout { knife: Some(name.into()), ..Default::default() });
        drop(world);
        app.update();
        app.update();
    }

    fn start_selected_inspect(app: &mut App, name: &str, index: usize) -> (f32, SeqClip) {
        if app.world().resource::<ViewModelState>().activity == "INSPECT" {
            app.world_mut().resource_mut::<crate::tracers::InspectRequest>().cancel();
            app.update();
        }
        let clips = app.world().resource::<ViewModels>().weapons[name].clips["INSPECT"].clone();
        let seed = (0..4096).find(|seed|
            knife_variants::KnifeVariants::seeded(*seed).choose(&clips) == index).unwrap();
        app.world_mut().resource_mut::<ViewModelState>().knife_variants =
            knife_variants::KnifeVariants::seeded(seed);
        app.world_mut().resource_mut::<crate::foley::Foley>().inspected.clear();
        let authoritative = app.world().resource::<SimWorld>().0.state();
        app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
        app.update();
        let state = app.world().resource::<ViewModelState>();
        assert_eq!(state.activity, "INSPECT");
        assert_eq!(state.clip_index, index);
        assert_eq!(state.knife_inspect.as_ref().unwrap().key.sequence, index);
        assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
        (state.started, clips[index].clone())
    }

    // UNRUN: actual accepted render dispatch -> render-clock foley. Empty
    // SoundLibrary keeps the fixture silent; no device or waveform claim.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_selected_inspect_foley_uses_each_declared_variant_once() {
        use std::time::Duration;
        let (mut app, id) = installed_app(Some("weapon_knife_butterfly"));
        install_inspect_foley(&mut app);
        // Deliberately unrelated clocks: scheduling must use actual render
        // start, without borrowing a simulation tick or its interpolation.
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(7));
        for (name, declarations) in [
            ("weapon_knife_butterfly", vec![
                vec![(0., "ButterflyKnife.look01_a"), (0.6666667, "ButterflyKnife.look01_b")],
                vec![(0., "ButterflyKnife.look02_a"), (0.6962963, "ButterflyKnife.look02_b")],
                vec![(0., "ButterflyKnife.look03_a"), (0.73103446, "ButterflyKnife.look03_b")]]),
            ("weapon_knife_falchion", vec![vec![(0., "KnifeFalchion.inspect")],
                vec![(0.8806818, "KnifeFalchion.Catch")]]),
        ] {
            equip_inspect_fixture(&mut app, id, name);
            for (index, declared) in declarations.iter().enumerate() {
                let (started, seq) = start_selected_inspect(&mut app, name, index);
                assert_eq!(seq.sounds.len(), declared.len());
                for ((cycle, entry), (declared_cycle, declared_entry)) in seq.sounds.iter().zip(declared) {
                    assert!((cycle - declared_cycle).abs() < 0.000001);
                    assert_eq!(entry.as_str(), *declared_entry);
                }
                let expected: Vec<_> = seq.sounds.iter().map(|(cycle, entry)|
                    (entry.clone(), started + cycle * seq.clip.duration())).collect();
                let first: Vec<_> = expected.iter().filter(|(_, at)| *at <= started).cloned().collect();
                assert_eq!(app.world().resource::<crate::foley::Foley>().inspected, first);
                app.update(); // identical timestamp must neither reroll nor replay
                assert_eq!(app.world().resource::<crate::foley::Foley>().inspected, first);
                let authoritative = app.world().resource::<SimWorld>().0.state();
                app.world_mut().resource_mut::<Time>()
                    .advance_by(Duration::from_secs_f32(seq.clip.duration() + 0.01));
                app.update(); // also crosses natural completion before due emission
                app.update();
                assert_eq!(app.world().resource::<crate::foley::Foley>().inspected, expected);
                assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
            }
        }
    }

    // Viewing/life/weapon boundaries are processed by
    // production dispatch before this frame's already-due final cue.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_inspect_foley_suppresses_interrupted_cues_before_emission() {
        use std::time::Duration;
        let (mut app, id) = installed_app(Some("weapon_knife_butterfly"));
        install_inspect_foley(&mut app);
        app.init_resource::<crate::surfaces::SurfaceProps>()
            .add_systems(FixedUpdate, crate::foley::update);
        let other = app.world_mut().resource_mut::<SimWorld>().0
            .add_player("inspect observer", sim::Team::Terrorist);
        for boundary in 1..8 {
            {
                let mut local = app.world_mut().resource_mut::<LocalPlayer>();
                local.noclip = false;
                local.spectating = None;
            }
            app.world_mut().resource_mut::<SimWorld>().0.respawn(id);
            equip_inspect_fixture(&mut app, id, "weapon_knife_butterfly");
            app.world_mut().resource_mut::<SimWorld>().0.tick = 10;
            app.world_mut().run_schedule(FixedUpdate);
            let (_, seq) = start_selected_inspect(&mut app, "weapon_knife_butterfly", 0);
            let before = app.world().resource::<crate::foley::Foley>().inspected.clone();
            assert_eq!(before.len(), 1);
            app.world_mut().resource_mut::<crate::tracers::InspectRequest>().cancel();
            app.update();
            assert!(app.world().resource::<ViewModelState>().knife_reinspect.is_some());
            app.world_mut().resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(seq.clip.duration() * 0.8));
            match boundary {
                1 => app.world_mut().resource_mut::<SimWorld>().0.respawn(id),
                2 => app.world_mut().resource_mut::<SimWorld>().0.damage_environment(id, 1000, "fixture"),
                3 => app.world_mut().resource_mut::<LocalPlayer>().spectating = Some(other),
                4 => app.world_mut().resource_mut::<LocalPlayer>().noclip = true,
                5 => {
                    let mut world = app.world_mut().resource_mut::<SimWorld>();
                    let def = Arc::new(world.0.catalog.get("weapon_m4a1").unwrap().clone());
                    world.0.player_mut(id).unwrap().weapons[0] = sim::weapon::WeaponState::new(def);
                }
                6 => { *app.world_mut().resource_mut::<Time>() = Time::default(); }
                _ => {
                    app.world_mut().resource_mut::<SimWorld>().0.tick = 9;
                    app.world_mut().run_schedule(FixedUpdate);
                }
            }
            let authoritative = app.world().resource::<SimWorld>().0.state();
            app.update();
            assert_eq!(app.world().resource::<crate::foley::Foley>().inspected, before,
                "boundary {boundary} must suppress a cue already due on this render");
            assert!(app.world().resource::<ViewModelState>().knife_reinspect.is_none());
            app.world_mut().resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(seq.clip.duration() + 1.));
            app.update();
            assert_eq!(app.world().resource::<crate::foley::Foley>().inspected, before);
            assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
        }
    }

    // UNRUN: actual melee event priority rejects the simultaneous inspect,
    // while a cooldown-rejected melee input leaves an accepted Inspect alone.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_inspect_foley_follows_accepted_melee_priority_and_reinspect() {
        use std::time::Duration;
        let (mut app, id) = installed_app(Some("weapon_knife_butterfly"));
        install_inspect_foley(&mut app);
        equip_inspect_fixture(&mut app, id, "weapon_knife_butterfly");
        app.init_resource::<crate::effects::PendingEvents>()
            .init_resource::<crate::tracers::Tracers>()
            .init_resource::<crate::surfaces::SurfaceProps>()
            .init_resource::<crate::settings::Settings>()
            .init_resource::<crate::buymenu::BuyMenu>()
            .add_systems(Update, crate::effects::process.before(update));
        let (_, seq) = start_selected_inspect(&mut app, "weapon_knife_butterfly", 0);
        let before = app.world().resource::<crate::foley::Foley>().inspected.clone();
        app.world_mut().resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(seq.clip.duration() * 0.8));
        let melee = sim::TickInput { buttons: movement::buttons::ATTACK, ..Default::default() };
        let events = {
            let mut world = app.world_mut().resource_mut::<SimWorld>();
            world.0.step(&[(id, melee.clone())]);
            world.0.events.iter().filter(|event| matches!(event,
                sim::GameEvent::Melee { player, weapon, .. }
                    if *player == id && weapon == "weapon_knife_butterfly")).cloned().collect::<Vec<_>>()
        };
        assert_eq!(events.len(), 1, "actual held/loadout knife must produce the accepted attack");
        app.world_mut().resource_mut::<crate::effects::PendingEvents>().0.extend(events);
        // The request is consumed, but actual attack dispatch has priority.
        app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
        let authoritative = app.world().resource::<SimWorld>().0.state();
        app.update();
        assert!(knife_attack_activity(&app.world().resource::<ViewModelState>().activity));
        assert!(app.world().resource::<ViewModelState>().knife_inspect.is_none());
        assert_eq!(app.world().resource::<crate::foley::Foley>().inspected, before,
            "accepted attack suppresses the already-due old cue; rejected Inspect emits nothing");
        assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);

        // A fresh inspect after the consumed attack is accepted without
        // changing its deadline. A second accepted edge at the exact same
        // timestamp must replace the old queue and emit its own first cue.
        let (started, _) = start_selected_inspect(&mut app, "weapon_knife_butterfly", 0);
        let old_edge = app.world().resource::<ViewModelState>().inspect_transition;
        let (restarted, selected) = start_selected_inspect(&mut app, "weapon_knife_butterfly", 1);
        assert_eq!(restarted, started);
        assert_eq!(app.world().resource::<ViewModelState>().inspect_transition, old_edge + 1);
        let first = app.world().resource::<crate::foley::Foley>().inspected.clone();
        assert_eq!(first, [("ButterflyKnife.look02_a".into(), restarted)]);
        {
            let mut world = app.world_mut().resource_mut::<SimWorld>();
            world.0.step(&[(id, melee)]); // still inside the real attack cooldown
            assert!(!world.0.events.iter().any(|event| matches!(event, sim::GameEvent::Melee { .. })));
        }
        let authoritative = app.world().resource::<SimWorld>().0.state();
        app.update();
        assert_eq!(app.world().resource::<ViewModelState>().activity, "INSPECT");
        assert_eq!(app.world().resource::<crate::foley::Foley>().inspected, first);
        app.world_mut().resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(selected.clip.duration() + 0.01));
        app.update();
        let expected: Vec<_> = selected.sounds.iter().map(|(cycle, entry)|
            (entry.clone(), restarted + cycle * selected.clip.duration())).collect();
        assert_eq!(app.world().resource::<crate::foley::Foley>().inspected, expected,
            "only the final accepted variant's queue survives");
        assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
    }

    // UNRUN: readiness, life and view-owner boundaries of the shared receipt.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_knife_draw_choice_never_caches_a_missing_clip_fallback() {
        let (app, id) = installed_unposed_app(Some("weapon_knife_butterfly"));
        let mut player = app.world().resource::<SimWorld>().0.player(id).unwrap().clone();
        let views = app.world().resource::<ViewModels>();
        let view = &views.weapons["weapon_knife_butterfly"];
        let mut state = ViewModelState::default();
        state.knife_variants = knife_variants::KnifeVariants::seeded(0);
        assert_eq!(state.knife_draw_index(&player, None), None);
        let mut unavailable = WeaponView {
            scope_center: view.scope_center, scope_rear: view.scope_rear,
            asset: view.asset.clone(), clips: HashMap::new(), grenade_hold: None,
        };
        assert_eq!(state.knife_draw_index(&player, Some(&unavailable)), None);
        let mut undecoded = view.clips["ACT_VM_DRAW"].clone();
        Arc::make_mut(&mut undecoded[0].clip).frames.clear();
        unavailable.clips.insert("ACT_VM_DRAW".into(), undecoded);
        assert_eq!(state.knife_draw_index(&player, Some(&unavailable)), None);
        assert!(state.knife_draw_choice.is_none());
        assert_eq!(state.knife_draw_serial, 0);
        let index = state.knife_draw_index(&player, Some(view)).unwrap();
        assert_eq!(state.knife_draw_serial, 1);
        assert_eq!(state.knife_draw_index(&player, Some(view)), Some(index));
        assert_eq!(state.knife_draw_serial, 1, "the second observer must reuse the first choice");

        player.life_epoch += 1;
        state.knife_draw_index(&player, Some(view));
        assert_eq!(state.knife_draw_serial, 2);
        player.id += 1;
        state.knife_draw_index(&player, Some(view));
        assert_eq!(state.knife_draw_serial, 3);

        // An unavailable intervening model invalidates the prior choice, but
        // does not resolve index0 or advance the cosmetic RNG/deployment serial.
        assert_eq!(state.knife_draw_index(&player, None), None);
        assert!(state.knife_draw_choice.is_none());
        assert_eq!(state.knife_draw_serial, 3);
        state.knife_draw_index(&player, Some(view));
        assert_eq!(state.knife_draw_serial, 4);
    }

    // UNRUN: the first idle frame and later frames must select the same clip.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_completed_butterfly_variant_does_not_change_idle_on_the_next_frame() {
        use std::time::Duration;
        let (mut app, id) = installed_app(Some("weapon_knife_butterfly"));
        app.world_mut().resource_mut::<ViewModelState>().knife_variants = knife_variants::KnifeVariants::seeded(0);
        for activity in ["ACT_VM_DRAW", "INSPECT"] {
            if activity == "INSPECT" {
                // Advance real accepted requests until an alternate is selected.
                for _ in 0..16 {
                    app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
                    app.update();
                    if app.world().resource::<ViewModelState>().clip_index == 1 { break; }
                }
            } else {
                // A fresh viewing life picks the deterministic alternate Draw.
                app.world_mut().resource_mut::<SimWorld>().0.player_mut(id).unwrap().life_epoch += 1;
                app.update();
            }
            let state = app.world().resource::<ViewModelState>();
            assert_eq!(state.activity, activity);
            assert_eq!(state.clip_index, 1, "fixture must exercise the second activity variant");
            let duration = app.world().resource::<ViewModels>().weapons["weapon_knife_butterfly"].clips[activity][1].clip.duration();
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(duration + 0.01));
            app.update();
            let state = app.world().resource::<ViewModelState>();
            assert_eq!(state.activity, "ACT_VM_IDLE");
            assert_eq!(state.clip_index, 0, "completion explicitly samples the first idle sequence");
            let first_idle = state.pose.clone();
            app.update(); // identical timestamp; sequence ownership must be stable
            assert_eq!(app.world().resource::<ViewModelState>().pose, first_idle,
                "an old variant index must not select a different idle on the following frame");
        }
    }

    // UNRUN: actual TickInput -> Melee event -> Effects -> viewmodel dispatch.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_spaced_dagger_attacks_keep_cycling_after_completion_and_idle() {
        use std::time::Duration;
        let (mut app, id) = installed_app(Some("weapon_knife_push"));
        // Ordinary simulation ticks dress newly held knives from the owner's
        // loadout. Select the Daggers there too, so the first input does not
        // legitimately replace the fixture's model with the CT stock knife.
        app.world_mut().resource_mut::<SimWorld>().0.set_loadout(id, sim::Loadout {
            knife: Some("weapon_knife_push".into()), ..Default::default()
        });
        // Keep the installed model/catalog but use empty collision so every
        // actual melee input produces a miss, independent of the map spawn.
        let mut collision = collision::CollisionWorld::new();
        collision.finalize();
        app.world_mut().resource_mut::<SimWorld>().0.collision = Arc::new(collision);
        app.init_resource::<crate::effects::PendingEvents>()
            .init_resource::<crate::tracers::Tracers>()
            .init_resource::<crate::audio::SoundLibrary>()
            .init_resource::<Assets<bevy::audio::AudioSource>>()
            .init_resource::<crate::surfaces::SurfaceProps>()
            .init_resource::<crate::settings::Settings>()
            .init_resource::<crate::buymenu::BuyMenu>()
            .init_resource::<crate::foley::Foley>()
            .add_systems(Update, crate::effects::process.before(update));
        // Empty sound declarations keep this fixture silent; production melee
        // event routing and serial consumption still run without test hooks.
        let clips = app.world().resource::<ViewModels>().weapons["weapon_knife_push"].clips["ACT_VM_MISSCENTER"].clone();
        assert_eq!(clips.len(), 5);
        let mut selected = Vec::new();
        for expected in [1, 2, 3, 4, 0, 1, 2] {
            let events = {
                let mut world = app.world_mut().resource_mut::<SimWorld>();
                let deadline = world.0.player(id).unwrap().active_weapon().unwrap().next_attack;
                while world.0.time() <= deadline {
                    world.0.step(&[(id, sim::TickInput::default())]);
                }
                world.0.step(&[(id, sim::TickInput {
                    buttons: movement::buttons::ATTACK, ..Default::default()
                })]);
                world.0.events.iter().filter(|event| matches!(event,
                    sim::GameEvent::Melee { player, weapon, secondary: false, hit: sim::MeleeHit::Miss, .. }
                        if *player == id && weapon == "weapon_knife_push")).cloned().collect::<Vec<_>>()
            };
            assert_eq!(events.len(), 1, "each real input must produce one fresh light miss");
            let authoritative = app.world().resource::<SimWorld>().0.state();
            app.world_mut().resource_mut::<crate::effects::PendingEvents>().0.extend(events);
            app.update();
            let state = app.world().resource::<ViewModelState>();
            assert_eq!(state.activity, "ACT_VM_MISSCENTER");
            assert_eq!(state.last_attack_seen, app.world().resource::<crate::effects::Effects>().attack_serial);
            let index = state.clip_index % clips.len();
            assert_eq!(index, expected, "completion must preserve the existing five-clip cycle");
            selected.push(index);

            app.world_mut().resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(clips[index].clip.duration() + 0.01));
            app.update(); // completed attack selects the first idle
            let state = app.world().resource::<ViewModelState>();
            assert_eq!(state.activity, "ACT_VM_IDLE");
            assert_eq!(state.clip_index, 0);
            assert!(state.completed_knife_index.is_some());
            let first_idle = state.pose.clone();
            app.update(); // separate idle frame cannot erase the attack cursor
            assert_eq!(app.world().resource::<ViewModelState>().pose, first_idle);
            assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative,
                "presentation cannot change attack readiness or simulation state");
        }
        assert_eq!(selected, [1, 2, 3, 4, 0, 1, 2]);

        // Accepted non-attack actions keep their existing cursor boundary.
        app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
        app.update();
        assert_eq!(app.world().resource::<ViewModelState>().completed_knife_index, None);
        app.world_mut().resource_mut::<crate::tracers::InspectRequest>().cancel();
        app.update();
        let state = app.world().resource::<ViewModelState>();
        assert_eq!(state.activity, "INSPECT");
        assert!(state.knife_reinspect.is_some());
        assert_eq!((state.clip_index, state.completed_knife_index), (0, None));
    }

    // UNRUN by source-only child; root owns Cargo and installed-data execution.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_elite_alternating_fire_keeps_outgoing_hands_and_idle_serials() {
        use std::time::Duration;
        let (mut app, id) = installed_app(Some("weapon_elite"));
        app.init_resource::<crate::effects::PendingEvents>()
            .init_resource::<crate::tracers::Tracers>()
            .init_resource::<crate::audio::SoundLibrary>()
            .init_resource::<Assets<bevy::audio::AudioSource>>()
            .init_resource::<crate::surfaces::SurfaceProps>()
            .init_resource::<crate::settings::Settings>()
            .init_resource::<crate::buymenu::BuyMenu>()
            .init_resource::<crate::foley::Foley>()
            .add_systems(Update, crate::effects::process.before(update));
        let clips = {
            let views = app.world().resource::<ViewModels>();
            let view = &views.weapons["weapon_elite"];
            [view.clips["ACT_VM_PRIMARYATTACK"][0].clone(),
                view.clips["ACT_VM_SECONDARYATTACK"][0].clone()]
        };
        // Settle deployment before exercising real TickInput -> Fire ->
        // Effects -> viewmodel dispatch. No test-only attack injection.
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(2));
        app.update();
        let mut starts = [None, None];
        let mut previous_serial = app.world().resource::<crate::effects::Effects>().attack_serial;
        for shot in 0..8 {
            let events = {
                let mut world = app.world_mut().resource_mut::<SimWorld>();
                let deadline = world.0.player(id).unwrap().active_weapon().unwrap().next_attack;
                world.0.step(&[(id, sim::TickInput::default())]); // release the semi-auto button
                while world.0.time() <= deadline {
                    world.0.step(&[(id, sim::TickInput::default())]);
                }
                world.0.step(&[(id, sim::TickInput {
                    buttons: movement::buttons::ATTACK, ..Default::default()
                })]);
                world.0.events.iter().filter(|event| matches!(event,
                    sim::GameEvent::Fire { player, weapon, .. }
                        if *player == id && weapon == "weapon_elite")).cloned().collect::<Vec<_>>()
            };
            assert_eq!(events.len(), 1);
            let authoritative = app.world().resource::<SimWorld>().0.state();
            app.world_mut().resource_mut::<crate::effects::PendingEvents>().0.extend(events);
            app.update();
            let now = app.world().resource::<Time>().elapsed_secs();
            let side = shot % 2;
            starts[side] = Some(now);
            let state = app.world().resource::<ViewModelState>();
            let fire = state.elite_fire.as_ref().unwrap();
            assert_eq!(state.activity, if side == 0 { "ACT_VM_PRIMARYATTACK" } else { "ACT_VM_SECONDARYATTACK" });
            assert_eq!(fire.hands.each_ref().map(|hand| hand.as_ref().map(|hand| hand.started)), starts);
            assert_eq!(state.last_attack_seen, previous_serial + 1);
            previous_serial = state.last_attack_seen;
            if let Some(started) = starts[1 - side] {
                let mut expected = state.pose.clone();
                clips[1 - side].clip.sample(now - started, &mut expected);
                for (bone, mask) in fire.masks[1 - side].iter().enumerate() {
                    if *mask > 0. { assert_eq!(state.pose[bone], expected[bone], "outgoing bone {bone} reset on shot {shot}"); }
                }
            }
            assert_eq!(app.world().resource::<crate::effects::Effects>().fire_muzzles, [Some(side == 0)]);
            assert!(app.world().resource::<crate::tracers::Muzzles>().first_pair.is_some());
            // Empty render frames must advance existing clips without
            // consuming/restarting either hand or the global attack serial.
            for _ in 0..6 {
                app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(20));
                app.update();
                let state = app.world().resource::<ViewModelState>();
                let fire = state.elite_fire.as_ref().unwrap();
                assert_eq!(fire.hands.each_ref().map(|hand| hand.as_ref().map(|hand| hand.started)), starts);
                assert_eq!(state.last_attack_seen, previous_serial);
                assert_eq!(app.world().resource::<crate::effects::Effects>().attack_serial, previous_serial);
            }
            assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative,
                "rendering must preserve ammunition, readiness, aim and recoil");
        }
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
        app.update();
        let state = app.world().resource::<ViewModelState>();
        assert_eq!(state.activity, "ACT_VM_IDLE");
        assert!(state.elite_fire.is_none());
        let idle_started = state.started;
        for _ in 0..3 {
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(20));
            app.update();
            let state = app.world().resource::<ViewModelState>();
            assert!(state.elite_fire.is_none());
            assert_eq!(state.started, idle_started);
            assert_eq!(state.last_attack_seen, previous_serial);
        }
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_knife_inspect_requires_reload_for_one_active_restart() {
        use std::time::Duration;
        let (mut app, id) = installed_app(Some("weapon_knife"));
        for name in ["weapon_knife", "weapon_knife_t", "weapon_bayonet", "weapon_knife_flip",
            "weapon_knife_gut", "weapon_knife_karambit", "weapon_knife_m9_bayonet",
            "weapon_knife_tactical", "weapon_knife_butterfly", "weapon_knife_falchion", "weapon_knife_push"] {
            equip_inspect_fixture(&mut app, id, name);
            let authoritative = app.world().resource::<SimWorld>().0.state();
            app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
            app.update();
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(150));
            app.update();
            let displayed = app.world().resource::<ViewModelState>().pose.clone();
            let edge = app.world().resource::<ViewModelState>().inspect_transition;
            let started = app.world().resource::<ViewModelState>().started;
            let index = app.world().resource::<ViewModelState>().clip_index;
            // F alone is consumed, without rerolling, rebasing or replaying later.
            app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
            app.update();
            let state = app.world().resource::<ViewModelState>();
            assert_eq!((state.started, state.inspect_transition, state.clip_index), (started, edge, index), "{name}");
            assert_eq!(state.pose, displayed, "{name}");
            assert_eq!(state.inspect_seen, app.world().resource::<crate::tracers::InspectRequest>().serial());
            // R alone preserves both the displayed pose and sound receipt.
            for _ in 0..2 {
                app.world_mut().resource_mut::<crate::tracers::InspectRequest>().cancel();
                app.update();
                let state = app.world().resource::<ViewModelState>();
                assert!(state.knife_reinspect.is_some(), "{name}");
                assert_eq!((state.started, state.inspect_transition, state.clip_index), (started, edge, index));
                assert_eq!(state.knife_inspect.as_ref().unwrap().key.edge, edge);
                assert_eq!(state.pose, displayed, "{name}");
            }
            app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
            app.update();
            let state = app.world().resource::<ViewModelState>();
            assert_eq!(state.inspect_transition, edge.wrapping_add(1), "{name}");
            assert!(state.knife_reinspect.is_none());
            assert_eq!(state.pose, displayed, "{name}: accepted restart retains the displayed endpoint");
            let restarted = state.started;
            let index = state.clip_index;
            let duration = app.world().resource::<ViewModels>().weapons[name].clips["INSPECT"][index].clip.duration();
            app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
            app.update();
            assert_eq!(app.world().resource::<ViewModelState>().inspect_transition, edge.wrapping_add(1),
                "{name}: R permission is single-use");
            // No render has processed completion yet: this F must still work.
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(duration + 0.01));
            app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
            app.update();
            let state = app.world().resource::<ViewModelState>();
            assert_eq!(state.activity, "INSPECT", "{name}");
            assert_eq!(state.inspect_transition, edge.wrapping_add(2), "{name}");
            assert!(state.started > restarted);
            assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative);
        }
    }

    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy; no GPU required"]
    fn installed_secondary_slash_quick_inspect_continues_the_authored_attack() {
        use std::time::Duration;
        let (mut app, id) = installed_app(Some("weapon_knife"));
        {
            let mut world = app.world_mut().resource_mut::<SimWorld>();
            world.0.step(&[(id, sim::TickInput {
                buttons: movement::buttons::ATTACK2, ..Default::default()
            })]);
            assert!(world.0.events.iter().any(|event|
                matches!(event, sim::GameEvent::Melee { secondary: true, .. })));
        }
        // The existing no-renderer fixture does not run Effects::process.
        // Select its real secondary miss activity, then let production update
        // sample and retain the displayed attack phase (rather than injecting
        // an already blended pose as the previous regression did).
        {
            let mut state = app.world_mut().resource_mut::<ViewModelState>();
            state.activity = "ACT_VM_MISSCENTER2".into();
            state.started = 0.;
            state.clip_index = 0;
        }
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(200));
        app.update();
        let displayed = app.world().resource::<ViewModelState>().pose.clone();
        let attack = app.world().resource::<ViewModelState>().knife_attack.clone().unwrap();
        let inspect = app.world().resource::<ViewModels>().weapons["weapon_knife"].clips["INSPECT"][0].clone();
        let authoritative = app.world().resource::<SimWorld>().0.state();
        app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
        app.update();
        assert_eq!(app.world().resource::<ViewModelState>().pose, displayed);
        let inspect_started = app.world().resource::<ViewModelState>().started;

        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(inspect.fade_in * 0.5));
        app.update();
        let elapsed = app.world().resource::<Time>().elapsed_secs() - inspect_started;
        let mut incoming = displayed.clone();
        sequence_pose::sample(&inspect.clip, elapsed, &mut incoming,
            |b| inspect.weights.get(b).copied().unwrap_or(1.));
        let mut continuing = displayed.clone();
        sequence_pose::sample(&attack.clip, attack.time + elapsed, &mut continuing,
            |b| attack.weights.get(b).copied().unwrap_or(1.));
        assert_ne!(continuing, displayed, "the outgoing slash must still be moving during the fade");
        let weight = (elapsed / inspect.fade_in).clamp(0., 1.);
        let expected: Vec<_> = continuing.iter().zip(&incoming)
            .map(|(a, b)| (a.0.lerp(b.0, weight), a.1.slerp(b.1, weight))).collect();
        assert_eq!(app.world().resource::<ViewModelState>().pose, expected,
            "quick inspect must fade a continuing slash, not a frozen hand snapshot");

        app.world_mut().resource_mut::<crate::tracers::InspectRequest>().cancel();
        app.update();
        app.world_mut().resource_mut::<crate::tracers::InspectRequest>().start();
        app.update();
        assert_eq!(app.world().resource::<ViewModelState>().pose, expected,
            "repeated inspect must discard the old outgoing slash and capture what is visible");
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(inspect.fade_in * 2.));
        app.update();
        assert_eq!(app.world().resource::<SimWorld>().0.state(), authoritative,
            "the stab deadline and next-attack rejection remain authoritative");
    }
}

/// Vertical FOV (radians) for a horizontal 4:3 FOV in degrees, as Source scales it.
pub fn vertical_fov(horizontal_4x3_deg: f32) -> f32 {
    2.0 * ((horizontal_4x3_deg.to_radians() * 0.5).tan() * 0.75).atan()
}

// Installed AUG/SG556 have an `unzoom` attachment but no eye/zoom
// attachment. Use their authored rear-glass geometry; do not interpret the
// unzoom attachment as a scoped camera or infer an undocumented native offset.
fn scoped_rear_offset(rear: glam::Vec3) -> glam::Vec3 {
    glam::Vec3::new(8.0, 0.0, 0.0) - rear
}

fn posed_scope_rear(model: &mdl::Model, pose: &[glam::Mat4]) -> Option<(glam::Vec3, f32)> {
    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = -min;
    for mesh in &model.meshes {
        let material = model.skin_texture(0, mesh.material);
        if !model.textures.get(material).is_some_and(|name| name.contains("scope_glass")) { continue; }
        for vertex in &mesh.vertices {
            let point = mdl::skeleton::skin_vertex(vertex, pose, &model.bones);
            min = min.min(point);
            max = max.max(point);
        }
    }
    if !min.is_finite() || !max.is_finite() { return None; }
    let size = max - min;
    Some(((min + max) * 0.5, size.y.min(size.z) * 0.5))
}

fn scope_aperture(lens: Vec3, radius: f32, viewport: Vec2) -> Vec4 {
    // A lens behind the eye must not become a huge forward-facing cutout.
    if !lens.is_finite() || lens.x <= 0.0 || !radius.is_finite() || radius <= 0.0 { return Vec4::ZERO; }
    let focal = viewport.y * 0.5 / (vertical_fov(VIEWMODEL_FOV) * 0.5).tan();
    Vec4::new(viewport.x * 0.5 - lens.y * focal / lens.x,
        viewport.y * 0.5 - lens.z * focal / lens.x,
        radius * focal / lens.x * 0.97, 0.0)
}

#[cfg(test)]
mod optic_transform_tests {
    use super::*;

    #[test]
    fn rear_eye_relief_is_independent_of_scope_tube_length() {
        let rear = glam::Vec3::new(3.0, -2.0, 5.0);
        let front = rear + glam::Vec3::X * 10.0;
        let old_offset = glam::Vec3::new(8.0 - front.x, -rear.y, -rear.z);
        assert!((rear + old_offset).x < 0.0);
        assert_eq!(rear + scoped_rear_offset(rear), glam::Vec3::new(8.0, 0.0, 0.0));
        let aperture = scope_aperture(to_bevy(glam::Vec3::ZERO), 1.0, Vec2::new(1920., 1080.));
        assert_eq!(aperture, Vec4::ZERO);
    }

    #[test]
    fn aperture_tracks_actual_rendered_lens_in_camera_space() {
        let viewport = Vec2::new(1920., 1080.);
        let centered = scope_aperture(Vec3::new(8., 0., 0.), 1., viewport);
        assert_eq!(centered.truncate().truncate(), viewport * 0.5);
        let displaced = scope_aperture(Vec3::new(8., 1., -0.5), 1., viewport);
        assert!(displaced.x < centered.x && displaced.y > centered.y);
        assert_eq!(displaced.z, centered.z);
        assert_eq!(scope_aperture(Vec3::new(-1., 0., 0.), 1., viewport), Vec4::ZERO);
    }
}

/// A decoded sequence: its clip and per-bone weights.
#[derive(Clone)]
struct SeqClip {
    clip: Arc<Clip>,
    weights: Arc<Vec<f32>>,
    activity_weight: i32,
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

/// Elite's installed left/right attacks are full-weight absolute clips, with
/// no authored autolayers or pose controls. Replacing the entire clip resets
/// the other pistol. Keep each arm subtree on its own playback clock instead.
/// Decoding all 16 frames at 18fps confirms the opposite forearm, hand and
/// foretwist stay at idle; the shared weapon/spine roots are unchanged too.
struct EliteFire {
    masks: [Arc<Vec<f32>>; 2],
    hands: [Option<EliteHand>; 2],
}

struct EliteHand {
    clip: Arc<Clip>,
    weights: Arc<Vec<f32>>,
    started: f32,
    transition: inspection::Inspection,
}

impl EliteFire {
    fn new(bones: &[mdl::skeleton::Bone]) -> Option<Self> {
        // The installed skeleton has separate forearm/foretwist roots and
        // clavicle branches. Descendants include each gun, slide and muzzle;
        // naming only the pistol bones would still reset the outgoing wrist.
        for side in ["L", "R"] {
            for part in ["Forearm", "Hand"] {
                let name = format!("v_weapon.Bip01_{side}_{part}");
                if !bones.iter().any(|bone| bone.name == name) { return None; }
            }
        }
        let mut sides = vec![None; bones.len()];
        for (i, bone) in bones.iter().enumerate() {
            sides[i] = if bone.name.starts_with("v_weapon.Bip01_L_") { Some(0) }
                else if bone.name.starts_with("v_weapon.Bip01_R_") { Some(1) }
                else { usize::try_from(bone.parent).ok()
                    .and_then(|parent| sides.get(parent)).copied().flatten() };
        }
        Some(Self {
            masks: std::array::from_fn(|side| Arc::new(sides.iter()
                .map(|bone| if *bone == Some(side) { 1. } else { 0. }).collect())),
            hands: [None, None],
        })
    }

    fn begin(&mut self, seq: &SeqClip, now: f32, displayed: &[(glam::Vec3, glam::Quat)]) {
        let side = usize::from(!seq.left_muzzle);
        let weights = Arc::new(self.masks[side].iter().enumerate()
            .map(|(bone, mask)| mask * seq.weights.get(bone).copied().unwrap_or(1.)).collect());
        let outgoing = self.hands[side].take().map(|hand| inspection::Playback {
            clip: hand.clip, weights: hand.weights, time: (now - hand.started).max(0.),
        });
        let mut transition = inspection::Inspection::default();
        // Repeated shots in the same hand use the declared 0.08s fade-in,
        // continuing its outgoing recoil during the transition. The other
        // hand is neither restarted nor included in this blend.
        transition.begin(Some(now), seq.fade_in, displayed, outgoing, 0, "Elite fire");
        self.hands[side] = Some(EliteHand { clip: seq.clip.clone(), weights, started: now, transition });
    }

    fn sample(&mut self, now: f32, pose: &mut [(glam::Vec3, glam::Quat)]) {
        for (side, hand) in self.hands.iter_mut().enumerate() {
            let Some(hand) = hand else { continue };
            let mut sampled = pose.to_vec();
            hand.transition.sample_base(&mut sampled);
            sequence_pose::sample(&hand.clip, (now - hand.started).max(0.), &mut sampled,
                |bone| hand.weights.get(bone).copied().unwrap_or(0.));
            hand.transition.apply(now, &mut sampled);
            for (bone, (target, value)) in pose.iter_mut().zip(sampled).enumerate() {
                if self.masks[side][bone] > 0. { *target = value; }
            }
        }
    }
}

#[cfg(test)]
mod elite_fire_tests {
    use super::*;

    #[test]
    fn independent_hands_continue_and_same_hand_restarts_blend_moving_recoil() {
        let bones: Vec<_> = [("shared", -1), ("v_weapon.Bip01_L_Forearm", -1),
            ("v_weapon.Bip01_L_Hand", 1), ("left gun", 2),
            ("v_weapon.Bip01_R_Forearm", -1), ("v_weapon.Bip01_R_Hand", 4),
            ("right gun", 5), ("v_weapon.Bip01_L_ForeTwist", -1)]
            .into_iter().map(|(name, parent)| mdl::skeleton::Bone {
                name: name.into(), parent, pos: glam::Vec3::ZERO, quat: glam::Quat::IDENTITY,
                rot: glam::Vec3::ZERO, pos_scale: glam::Vec3::ONE, rot_scale: glam::Vec3::ONE,
                pose_to_bone: glam::Mat4::IDENTITY, flags: 0,
            }).collect();
        let clips = [true, false].map(|left| SeqClip {
            clip: Arc::new(Clip {
                name: "full-body attack with an idle opposite hand".into(), fps: 1.,
                looping: false, delta: false,
                frames: [0., 10.].map(|x| (0..bones.len()).map(|bone|
                    (glam::Vec3::X * if (left && [1, 2, 3, 7].contains(&bone))
                        || (!left && [4, 5, 6].contains(&bone)) { x } else { 0. }, glam::Quat::IDENTITY)
                ).collect()).to_vec(),
                animated: vec![true; bones.len()],
            }), weights: Arc::new(vec![1.; bones.len()]), activity_weight: 1,
            fade_in: 0.08, left_muzzle: left, sounds: Arc::new(Vec::new()),
        });
        let mut fire = EliteFire::new(&bones).unwrap();
        let mut pose = mdl::skeleton::rest_pose(&bones);
        fire.begin(&clips[0], 0., &pose);
        fire.sample(0.12, &mut pose);
        let outgoing = pose.clone();
        fire.begin(&clips[1], 0.12, &pose);
        clips[1].clip.sample(0., &mut pose); // ordinary full-body dispatch resets the left
        fire.sample(0.12, &mut pose);
        assert_eq!(pose, outgoing, "switching hands must preserve the outgoing motion");
        fire.sample(0.24, &mut pose);
        let displayed = pose.clone();
        fire.begin(&clips[0], 0.24, &pose);
        fire.sample(0.24, &mut pose);
        assert_eq!(pose, displayed, "same-hand restart begins at its displayed pose");
        fire.sample(0.28, &mut pose);
        // Half the authored fade: old left at 0.28s and new left at 0.04s.
        // Freezing the outgoing hand would instead give 1.4, not 1.6.
        assert!((pose[3].0.x - 1.6).abs() < 0.0001);
        assert!((pose[6].0.x - 1.6).abs() < 0.0001, "right continues its own original clock");
        assert_eq!(pose[7], pose[3], "independent foretwist root follows its hand");
        assert_eq!(pose[0], (glam::Vec3::ZERO, glam::Quat::IDENTITY));
        let once = pose.clone();
        fire.sample(0.28, &mut pose);
        assert_eq!(pose, once, "empty render frames cannot feed the blend back into itself");
        assert_eq!(fire.hands.each_ref().map(|hand| hand.as_ref().map(|hand| hand.started)), [Some(0.24), Some(0.12)]);
        let mut state = ViewModelState { elite_fire: Some(fire), ..Default::default() };
        state.begin_life(1, &bones, 1., 9, 0);
        assert!(state.elite_fire.is_none(), "a new deployment/life cannot inherit either hand");
        assert!(EliteFire::new(&bones[..4]).is_none(), "unknown skeletons retain ordinary playback");
    }
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
    /// Use the installed CS hands and a closed weapon grip for the crossover
    /// equipment pose. Original arm geometry/animations never enter Git.
    pub(crate) fn board_grip(&self, team: sim::Team, meshes:&mut Assets<Mesh>) -> Option<(SkinnedAsset, Vec<glam::Mat4>, glam::Mat4)> {
        let arms=match team {sim::Team::Terrorist=>self.arms_t.as_ref(),_=>self.arms_ct.as_ref()}?;
        let view=self.weapons.get("weapon_knife")?;
        let mut pose=mdl::skeleton::rest_pose(&view.asset.model.bones);
        if let Some(seq)=view.clips.get("ACT_VM_IDLE").and_then(|c|c.first()) {
            sequence_pose::sample(&seq.clip,0.,&mut pose,|b|seq.weights.get(b).copied().unwrap_or(1.));
        }
        let world=mdl::skeleton::model_space(&view.asset.model.bones,&pose);
        let pose=skinning::bone_merge(&arms.model.bones,&view.asset.model.bones,&world);
        let hand=arms.model.bones.iter().position(|b|b.name.to_ascii_lowercase().ends_with("r_hand") || b.name.to_ascii_lowercase().contains("right_hand"))?;
        let mut asset=arms.clone();
        // Retain just the gripping arm, including its original weighted fingers
        // and sleeve. A knife's unused off-hand otherwise floats across view.
        let right:Vec<_>=arms.model.bones.iter().enumerate().map(|(i,_)|{
            let mut at=i as i32;
            while at>=0 {let bone=&arms.model.bones[at as usize];let name=bone.name.to_ascii_lowercase();if name.contains("right")||name.contains("_r_"){return true;}at=bone.parent;}false
        }).collect();
        asset.parts=arms.model.meshes.iter().zip(&arms.parts).filter_map(|(source,(_,material))|{
            let on_right=|i:usize|{let v=&source.vertices[i];(0..v.bone_count.clamp(1,3) as usize).filter(|j|right.get(v.bones[*j] as usize).copied().unwrap_or(false)).map(|j|v.weights[j]).sum::<f32>()>0.5};
            let mut indices:Vec<_>=source.indices.chunks_exact(3).filter(|t|t.iter().all(|i|on_right(*i as usize))).flatten().copied().collect();
            if indices.is_empty(){return None;}
            if crate::props::needs_flip(source){for t in indices.chunks_exact_mut(3){t.swap(1,2);}}
            let mut mesh=Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList,bevy::asset::RenderAssetUsages::default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION,source.vertices.iter().map(|v|to_bevy(v.position).to_array()).collect::<Vec<_>>());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL,source.vertices.iter().map(|v|to_bevy(v.normal).to_array()).collect::<Vec<_>>());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0,source.vertices.iter().map(|v|v.uv.to_array()).collect::<Vec<_>>());
            let mut joints=vec![];let mut weights=vec![];
            for v in &source.vertices {let mut js=[0u16;4];let mut ws=[0f32;4];for i in 0..v.bone_count.clamp(1,3) as usize{js[i]=v.bones[i] as u16;ws[i]=v.weights[i].max(0.);}let sum=ws.iter().sum::<f32>();if sum>1e-6{ws.iter_mut().for_each(|w|*w/=sum);}else{ws[0]=1.;}joints.push(js);weights.push(ws);}
            mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX,bevy::mesh::VertexAttributeValues::Uint16x4(joints));
            mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT,weights);
            mesh.insert_indices(bevy::mesh::Indices::U32(indices));
            Some((meshes.add(mesh),material.clone()))
        }).collect();
        Some((asset,pose.clone(),pose[hand]))
    }
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
        self.sequence_sounds(weapon, activity, 0)
    }

    fn sequence_sounds(&self, weapon: &str, activity: &str, index: usize) -> Option<(f32, Arc<Vec<(f32, String)>>)> {
        let seq = self.weapons.get(weapon)?.clips.get(activity)?.get(index)?;
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
            culling: Vec::new(),
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
struct KnifeDrawChoice {
    owner: sim::PlayerId,
    life: u64,
    weapon: String,
    index: usize,
}

/// Accepted local presentation edge, on the same clock as sequence sampling.
/// Retain only identity and the selected clip's declared sound timing.
#[derive(Clone, PartialEq)]
pub(crate) struct KnifeInspectKey {
    pub owner: sim::PlayerId,
    pub life: u64,
    pub weapon: String,
    edge: u64,
    sequence: usize,
    pub started: f32,
}

pub(crate) struct KnifeInspect {
    pub key: KnifeInspectKey,
    pub duration: f32,
    pub sounds: Arc<Vec<(f32, String)>>,
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
    /// Completion displays idle[0] without erasing the previous action's
    /// cursor, which the existing knife attack cycle advances on its next edge.
    completed_knife_index: Option<usize>,
    started: f32,
    last_attack_seen: u64,
    inspect_seen: u64,
    /// Accepted presentation edges, distinct from consumed/rejected requests.
    inspect_transition: u64,
    pub(crate) knife_inspect: Option<KnifeInspect>,
    /// Reload permits one re-inspect without replacing the currently visible clip.
    /// Its simulation clock stamp prevents permission surviving a world reset.
    knife_reinspect: Option<(u64, u32)>,
    knife_variants: knife_variants::KnifeVariants,
    /// Shared with tick foley: whichever observes a deployment first chooses
    /// once; the other consumes the same installed sequence identity.
    knife_draw_choice: Option<KnifeDrawChoice>,
    knife_draw_serial: u64,
    knife_draw_seen: u64,
    reloading: bool,
    /// Keep the captured inspect pose through an incoming magazine reload fade.
    reload_from_inspect: bool,
    /// Last shell insert seen (for the debug trace).
    shell_seen: Option<i32>,
    planting: bool,
    grenade_seen: Option<(u64, sim::utility::ThrowPhase)>,
    suppressor_seen: Option<sim::weapon::SuppressorTransition>,
    light_timer: f32,
    look_motion: look_motion::LookMotion,
    air_lowering: air_lowering::AirLowering,
    /// Persistent bone-local pose for authored sequence masks. Absolute clips
    /// also restore decoded rest values for tracks omitted by compression.
    pose: Vec<(glam::Vec3, glam::Quat)>,
    inspection: inspection::Inspection,
    knife_attack: Option<inspection::Playback>,
    elite_fire: Option<EliteFire>,
}

impl ViewModelState {
    fn knife_draw_index(&mut self, player: &sim::Player, view: Option<&WeaponView>) -> Option<usize> {
        let weapon = player.active_weapon().filter(|w|
            player.alive && w.def.class == weapons::WeaponClass::Knife);
        let clips = view.and_then(|view| view.clips.get("ACT_VM_DRAW")).filter(|clips|
            !clips.is_empty() && clips.iter().all(|seq| !seq.clip.frames.is_empty()));
        let (Some(weapon), Some(clips)) = (weapon, clips) else {
            // Observe intervening weapons even when their models are not yet
            // available. Never retain or cache a guessed first-sequence choice.
            self.knife_draw_choice = None;
            return None;
        };
        if let Some(choice) = &self.knife_draw_choice {
            if choice.owner == player.id && choice.life == player.life_epoch
                && choice.weapon == weapon.def.class_name && choice.index < clips.len() {
                return Some(choice.index);
            }
        }
        let index = self.knife_variants.choose(clips);
        self.knife_draw_serial = self.knife_draw_serial.wrapping_add(1);
        self.knife_draw_choice = Some(KnifeDrawChoice {
            owner: player.id, life: player.life_epoch,
            weapon: weapon.def.class_name.clone(), index,
        });
        Some(index)
    }

    /// The viewed knife's actual Draw declaration, shared by render and tick
    /// foley. This neither schedules an event nor changes simulation state.
    pub(crate) fn knife_draw_sounds(&mut self, player: &sim::Player, views: &ViewModels)
        -> Option<(f32, Arc<Vec<(f32, String)>>)> {
        let name = player.active_weapon().map(|w| w.def.class_name.as_str()).unwrap_or_default();
        let index = self.knife_draw_index(player, views.weapons.get(name))?;
        views.sequence_sounds(name, "ACT_VM_DRAW", index)
    }

    fn select_knife_variant(&mut self, view: &WeaponView, activity: &str) {
        self.completed_knife_index = None;
        self.clip_index = view.clips.get(activity)
            .map_or(0, |clips| self.knife_variants.choose(clips));
    }

    fn begin_life(&mut self, life: u64, bones: &[mdl::skeleton::Bone], now: f32,
        attack_serial: u64, inspect_serial: u64) -> bool {
        if self.life == Some(life) { return false; }
        self.life = Some(life);
        self.activity = "ACT_VM_DRAW".into();
        self.clip_index = 0;
        self.completed_knife_index = None;
        self.started = now;
        self.last_attack_seen = attack_serial;
        self.inspect_seen = inspect_serial;
        self.inspect_transition = 0;
        self.knife_inspect = None;
        self.knife_reinspect = None;
        self.reloading = false;
        self.reload_from_inspect = false;
        self.shell_seen = None;
        self.planting = false;
        self.grenade_seen = None;
        self.suppressor_seen = None;
        self.light_timer = 0.0;
        self.inspection = inspection::Inspection::default();
        self.knife_attack = None;
        self.elite_fire = None;
        // Masked clips deliberately preserve zero-weight bones within one life.
        // They must never inherit the reload/inspect pose of a previous life.
        self.pose = mdl::skeleton::rest_pose(bones);
        true
    }
}

fn load_model(fs: &vfs::Vfs, path: &str) -> Option<mdl::Model> {
    let base = path.trim_end_matches(".mdl");
    mdl::Model::load(&fs.read(path)?, &fs.read(&format!("{base}.vvd"))?, &fs.read(&format!("{base}.dx90.vtx"))?).ok()
}

/// The caller supplies held assets built from this same level's VFS. Canonical
/// imports retain Model::load's geometry unchanged; hands_paintable entries can
/// replace them with filtered/icon geometry, so exclude those names entirely.
fn held_cosmetic_world_model<'a>(
    catalog: &weapons::Catalog,
    held: &'a HashMap<String, SkinnedAsset>,
    glove_models: &[(String, String, String)],
    name: &str,
    path: &str,
) -> Option<&'a mdl::Model> {
    let mut definitions = catalog.weapons.iter()
        .filter(|def| def.class_name == name && !def.world_model.is_empty());
    if definitions.next()?.world_model != path
        || definitions.any(|def| def.world_model != path)
        || glove_models.iter().any(|(glove, _, _)| glove == name)
    {
        return None;
    }
    held.get(name).map(|asset| asset.model.as_ref())
}

/// Borrow a successful decode by exact path for this load's cosmetic mappings.
/// Failed reads stay retryable; mapped meshes and GPU assets remain separate.
fn cosmetic_world_model<'a>(
    models: &'a mut HashMap<String, mdl::Model>,
    path: &str,
    decode: impl FnOnce(&str) -> Option<mdl::Model>,
) -> Option<&'a mdl::Model> {
    use std::collections::hash_map::Entry;
    match models.entry(path.to_owned()) {
        Entry::Occupied(entry) => Some(entry.into_mut()),
        Entry::Vacant(entry) => Some(entry.insert(decode(path)?)),
    }
}

#[cfg(test)]
mod cosmetic_loading_tests {
    use super::*;

    #[test]
    fn held_cosmetic_input_requires_exact_unmodified_source_and_maps_independently() {
        let mut catalog = weapons::Catalog::from_items_game(r#""items_game" { "items" {
            "1" { "name" "weapon_fixture" "baseitem" "1" "model_world" "world.mdl" }
        } }"#).unwrap();
        let source = mdl::Model {
            textures: vec!["body".into()],
            meshes: vec![mdl::Mesh {
                vertices: [glam::Vec3::ZERO, glam::Vec3::X, glam::Vec3::Y, glam::Vec3::Z]
                    .into_iter().enumerate().map(|(i, position)| mdl::Vertex {
                        position, normal: glam::Vec3::Z, uv: glam::Vec2::new(i as f32, 0.),
                        weights: [1., 0., 0.], bone_count: 1, ..Default::default()
                    }).collect(),
                indices: vec![0, 1, 2, 0, 2, 3], ..Default::default()
            }],
            ..Default::default()
        };
        let held = HashMap::from([("weapon_fixture".into(), SkinnedAsset {
            model: Arc::new(source.clone()), parts: Vec::new(),
            inverse_bindposes: Default::default(), culling: Vec::new(),
        })]);
        let mut local = HashMap::new();
        let exact = held_cosmetic_world_model(&catalog, &held, &[], "weapon_fixture", "world.mdl");
        let input = exact.or_else(|| cosmetic_world_model(&mut local, "world.mdl", |_| panic!("must borrow held input"))).unwrap();
        assert!(std::ptr::eq(input, held["weapon_fixture"].model.as_ref()));

        let serial = crate::cosmetic_geometry::world_decal(&source, &source, &source).unwrap();
        let mut sticker = crate::cosmetic_geometry::world_decal(&source, &source, input).unwrap();
        assert_eq!(sticker.textures, serial.textures);
        assert_eq!(sticker.meshes[0].indices, serial.meshes[0].indices);
        for (actual, expected) in sticker.meshes[0].vertices.iter().zip(&serial.meshes[0].vertices) {
            assert_eq!((actual.position, actual.normal, actual.uv, actual.bones, actual.weights),
                (expected.position, expected.normal, expected.uv, expected.bones, expected.weights));
        }
        sticker.textures[0] = "changed sticker".into();
        sticker.meshes[0].vertices[0].position = glam::Vec3::splat(100.);
        let counter = crate::cosmetic_geometry::world_decal(&source, &source, input).unwrap();
        assert_eq!(counter.textures, serial.textures);
        assert_eq!(counter.meshes[0].vertices[0].position, serial.meshes[0].vertices[0].position);
        assert_eq!(held["weapon_fixture"].model.meshes[0].vertices[0].position, glam::Vec3::ZERO);
        assert!(local.is_empty());

        // A different path, absent class/asset, filtered/icon override or
        // ambiguous canonical definition must use the existing local loader.
        let gloves = vec![("weapon_fixture".into(), "icon.mdl".into(), "glove.mdl".into())];
        let missing = HashMap::new();
        let unavailable = [
            held_cosmetic_world_model(&catalog, &held, &[], "weapon_fixture", "World.mdl"),
            held_cosmetic_world_model(&catalog, &held, &[], "other", "world.mdl"),
            held_cosmetic_world_model(&catalog, &missing, &[], "weapon_fixture", "world.mdl"),
        ];
        assert!(unavailable.iter().all(Option::is_none));
        let overridden = held_cosmetic_world_model(&catalog, &held, &gloves, "weapon_fixture", "world.mdl");
        assert!(overridden.is_none());
        let fallback = overridden.or_else(|| cosmetic_world_model(&mut local, "world.mdl", |path| {
            assert_eq!(path, "world.mdl"); Some(source.clone())
        })).unwrap();
        assert!(!std::ptr::eq(fallback, held["weapon_fixture"].model.as_ref()));
        let mapped = crate::cosmetic_geometry::world_decal(&source, &source, fallback).unwrap();
        assert_eq!(mapped.meshes[0].vertices[0].position, serial.meshes[0].vertices[0].position);
        let mut duplicate = catalog.weapons[0].clone();
        duplicate.world_model = "different.mdl".into();
        catalog.weapons.push(duplicate);
        assert!(held_cosmetic_world_model(&catalog, &held, &[], "weapon_fixture", "world.mdl").is_none());
    }

    #[test]
    fn cosmetic_mappings_reuse_successes_without_retaining_failures_or_mutations() {
        let mut models = HashMap::new();
        let mut attempts = Vec::new();
        let mut retry = false;
        let mut decode = |path: &str| {
            attempts.push(path.to_owned());
            if path == "retry.mdl" && !retry {
                retry = true;
                return None;
            }
            Some(mdl::Model { name: path.into(), textures: vec!["original".into()], ..Default::default() })
        };

        let sticker_input = cosmetic_world_model(&mut models, "world.mdl", &mut decode).unwrap();
        let raw = sticker_input.raw.clone();
        let mut mapped_sticker = sticker_input.clone();
        mapped_sticker.textures[0] = "sticker".into();
        let counter_input = cosmetic_world_model(&mut models, "world.mdl", &mut decode).unwrap();
        assert!(Arc::ptr_eq(&raw, &counter_input.raw));
        assert_eq!(counter_input.textures, ["original"]);

        // Keep exact-path identity even though the VFS normalizes lookups.
        assert_eq!(cosmetic_world_model(&mut models, "World.mdl", &mut decode).unwrap().name, "World.mdl");
        assert!(cosmetic_world_model(&mut models, "retry.mdl", &mut decode).is_none());
        assert!(!models.contains_key("retry.mdl"));
        assert!(cosmetic_world_model(&mut models, "retry.mdl", &mut decode).is_some());
        assert!(cosmetic_world_model(&mut models, "retry.mdl", &mut decode).is_some());

        // A later match must decode against its own mounted content.
        let mut next_load = HashMap::new();
        assert!(cosmetic_world_model(&mut next_load, "world.mdl", &mut decode).is_some());
        assert_eq!(attempts, ["world.mdl", "World.mdl", "retry.mdl", "retry.mdl", "world.mdl"]);
    }
}

fn inspect_sequence(seq: &mdl::anim::Sequence) -> bool {
    seq.name == "inspect" || seq.name.strip_prefix("lookat")
        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}

fn knife_attack_activity(activity: &str) -> bool {
    matches!(activity, "ACT_VM_PRIMARYATTACK" | "ACT_VM_MISSCENTER" | "ACT_VM_HITCENTER"
        | "ACT_VM_MISSCENTER2" | "ACT_VM_HITCENTER2")
}

#[allow(clippy::too_many_arguments)]
pub fn load(
    level: &level::Level,
    catalog:&crate::cosmetics::Catalog,
    held_weapons: &HashMap<String, SkinnedAsset>,
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
    // Glove team variants, mapped stickers and per-weapon StatTrak counters
    // retain their own geometry/material assets, but repeat the same VMT work.
    // Keep successful preparation values only for this level/load's inputs.
    let mut prepared = skinning::PreparedMaterials::new(fs, images, textures, shared, exposure, true);
    let mut build = |model: mdl::Model| {
        skinning::build_asset_with_materials(Arc::new(model), meshes, materials, bindposes, &mut prepared)
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
                let inspect = inspect_sequence(seq);
                if seq.activity.is_empty() && !inspect {
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
                        clips.entry(if inspect {"INSPECT".into()}else{seq.activity.to_ascii_uppercase()}).or_default().push(SeqClip {
                            clip: Arc::new(clip),
                            weights: Arc::new(weights),
                            activity_weight: seq.activity_weight,
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
                sequence_pose::sample(&seq.clip, 0.0, &mut idle, |b| seq.weights.get(b).copied().unwrap_or(1.0));
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
    // Sleeves only borrow these decoded models; keep the originals for the
    // separate default-arm assets built after the cosmetic assets below.
    let arms_t = load_model(fs, T_ARMS);
    let arms_ct = load_model(fs, CT_ARMS);
    let glove_models = crate::cosmetic_catalog::model_items(fs);
    for (name,path,_) in &glove_models {
        if let Some(model) = load_model(fs, path) {
            for (side, arms) in [("Terrorist", arms_t.as_ref()), ("CounterTerrorist", arms_ct.as_ref())] {
                if let Some(arms) = arms {
                    out.gloves.insert(format!("{name}:{side}"), build(crate::cosmetic_geometry::sleeves(model.clone(), arms)));
                }
            }
        }
    }
    {
        for path in catalog.models.values().flat_map(|m|m.stickers.iter().cloned()).chain(["models/weapons/stattrack.mdl".into(),"models/weapons/stattrack_cut.mdl".into()]){
            if !out.decorations.contains_key(&path){if let Some(model)=load_model(fs,&path){out.decorations.insert(path,build(model));}}
        }
    }
    // Borrow exact canonical held inputs from this level first. Other successful
    // decodes stay local to these two passes; all mapped assets are built anew.
    let mut world_models = HashMap::new();
    {
        for (name,info) in &catalog.models{
            if info.stickers.is_empty(){continue;}
            let Some(view)=out.weapons.get(name).map(|v|v.asset.model.clone())else{continue};
            let held=held_cosmetic_world_model(&level.catalog,held_weapons,&glove_models,name,&info.world);
            let Some(world)=held.or_else(||cosmetic_world_model(&mut world_models,&info.world,|path|load_model(fs,path)))else{continue};
            for path in &info.stickers {
                if let Some(decal)=out.decorations.get(path).map(|a|a.model.clone()){
                    if let Some(model)=crate::cosmetic_geometry::world_decal(&decal,&view,world){out.decorations.insert(format!("world:{path}"),build(model));}
                    else{warn!("Could not map world sticker {}",path);}
                }
            }
        }
    }
    for (name,_,path) in &glove_models{if let Some(model)=load_model(fs,path){out.decorations.insert(format!("glove:{name}"),build(model));}}
    for (name,view) in &out.weapons {
        if !view.asset.model.bones.iter().any(|b|b.name=="v_weapon.stattrack"){continue;}
        let path=if crate::cosmetics::is_rare_knife(name){"models/weapons/stattrack_cut.mdl"}else{"models/weapons/stattrack.mdl"};
        let (Some(counter),Some(info))=(out.decorations.get(path),catalog.models.get(name))else{continue};
        let held=held_cosmetic_world_model(&level.catalog,held_weapons,&glove_models,name,&info.world);
        let Some(world)=held.or_else(||cosmetic_world_model(&mut world_models,&info.world,|path|load_model(fs,path)))else{continue};
        let mut counter=(*counter.model).clone();
        let pose:Vec<_>=view.asset.model.bones.iter().map(|b|b.pose_to_bone.inverse()).collect();
        let merged=skinning::bone_merge(&counter.bones,&view.asset.model.bones,&pose);
        for mesh in &mut counter.meshes {for vertex in &mut mesh.vertices{
            let bone=vertex.bones[0] as usize;
            let transform=merged[bone]*counter.bones[bone].pose_to_bone;
            vertex.position=transform.transform_point3(vertex.position);
            vertex.normal=transform.transform_vector3(vertex.normal).normalize_or_zero();
        }}
        if let Some(model)=crate::cosmetic_geometry::world_decal(&counter,&view.asset.model,world){
            out.decorations.insert(format!("world:{name}:{path}"),build(model));
        }
    }
    out.arms_t = arms_t.map(&mut build);
    out.arms_ct = arms_ct.map(&mut build);
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
    (world, poses): (Res<SimWorld>, Res<crate::interpolation::RenderPoses>),
    local: Res<LocalPlayer>,
    models: Res<ViewModels>,
    mut state: ResMut<ViewModelState>,
    (request,inventory): (Res<crate::tracers::InspectRequest>,Res<crate::cosmetics::Inventory>),
    (mut muzzles, mut wick, mut optic): (ResMut<crate::tracers::Muzzles>, ResMut<HeldWick>, Option<ResMut<crate::scope::OpticAperture>>),
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
    muzzles.first_pair=None;
    // Clear before every early return, including death, noclip and switching.
    wick.sample = None;
    if let Some(optic) = optic.as_deref_mut() { optic.0 = Vec4::ZERO; }
    // Cached meshes belong to a successfully sampled view target. Every early
    // return must leave them hidden; reveal only after gun and arms are posed.
    if let Some((_, gun, arms, _)) = &state.weapon {
        for mesh in gun.meshes.iter().chain(arms.iter().flat_map(|a| &a.meshes)) {
            if let Ok(mut visible) = visibility.get_mut(*mesh) { *visible = Visibility::Hidden; }
        }
    }
    let now = time.elapsed_secs();
    let dt = time.delta_secs();
    let Some(p) = world.0.player(local.view_target()) else {
        state.knife_inspect = None;
        state.knife_reinspect = None;
        state.elite_fire = None;
        state.look_motion.clear();
        state.air_lowering.clear();
        return;
    };
    // Camera, body and weapon movement must describe the same rendered
    // moment. Raw command-state velocity/ground flags are one tick newer.
    let motion = poses.sample_at(p, fixed.overstep_fraction(), world.0.tick, world.0.tick_rate);
    let show = p.alive && !crate::movement_modes::third_person(p) && !local.render_third_person && !local.noclip && !p.active_weapon().is_some_and(|w| w.hides_viewmodel());
    if !show || p.id != local.id || local.spectating.is_some()
        || state.knife_inspect.as_ref().is_some_and(|inspect|
            inspect.key.owner != p.id || inspect.key.life != p.life_epoch
                || p.active_weapon().is_none_or(|w| w.def.class_name != inspect.key.weapon
                    || w.def.class != weapons::WeaponClass::Knife)) {
        state.knife_inspect = None;
        state.knife_reinspect = None;
    }
    // Look history belongs to the viewing life, not the selected weapon.
    // Switching within that life keeps the accepted response across the arsenal.
    if state.owner != Some(p.id) || state.life != Some(p.life_epoch) {
        state.look_motion.clear();
        state.air_lowering.clear();
    }
    let w = p.active_weapon();
    let shared_motion = show && p.id == local.id && local.spectating.is_none()
        && p.tracked.is_none() && p.aim.is_none()
        && w.is_some_and(|w| w.zoom_level == 0
            && look_motion::shared_profile(level.0.item_schema_sha1(), &w.def));
    // Record the viewing life on every frame, including the deferred-instance
    // frame below. Skipping a look reversal while switching weapons loses a
    // sample that is still needed when the measured delay reaches that frame.
    let look_translation = if shared_motion {
        // Flat local look, without recoil punch; camera/aim remain current.
        let look = movement::angle_vectors(glam::Vec3::new(local.pitch, local.yaw, 0.)).0;
        state.look_motion.translation(now, look)
    } else { state.look_motion.clear(); glam::Vec3::ZERO };
    // The same viewing life owns vertical presentation through mesh changes.
    // Climbing/tracked/scoped/hidden views have no admitted lowering profile.
    let air_offset = if shared_motion && p.movement.ladder_normal.is_none() {
        state.air_lowering.sample(now, !motion.animation.input.on_ground)
    } else { state.air_lowering.clear(); 0. };
    let active = p.active_weapon().map(|w| w.def.class_name.clone()).unwrap_or_default();
    let knife_draw_index = state.knife_draw_index(p, models.weapons.get(&active));
    if knife_draw_index.is_some() && state.knife_draw_seen != state.knife_draw_serial {
        // Tick foley can observe an intervening switch which rendering never
        // presents. Its new deployment still resets playback, even when the
        // final weapon/owner/life matches the cached instance.
        state.life = None;
    }

    let glove=if p.id==local.id{inventory.active("gloves").map(|i|i.weapon.as_str()).unwrap_or("")}else{""};
    // Respawn the viewmodel when the weapon or team changes.
    let current = state.weapon.as_ref().map(|w| w.0.clone());
    if current.as_deref() != Some(active.as_str()) || state.team != Some(p.team) || state.owner != Some(p.id) || state.glove!=glove {
        state.knife_inspect = None;
        state.knife_reinspect = None;
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
            // Deferred joints cannot be queried until the next update. Their
            // identity transforms must never be presented as a weapon pose.
            for mesh in gun.meshes.iter().chain(arms.iter().flat_map(|a| &a.meshes)) {
                commands.entity(*mesh).insert(Visibility::Hidden);
            }
            if !glove.is_empty(){if let Some(a)=&arms{crate::skin_finish::tag(&mut commands,a,crate::skin_finish::Target::Gloves);}}
            state.glove=glove.into();
            state.weapon = Some((active.clone(), gun, arms, Vec::new()));
            state.team = Some(p.team);
            state.owner = Some(p.id);
            state.life = None;
            state.begin_life(p.life_epoch, &view.asset.model.bones, now,
                effects.attack_serial, request.serial());
            if let Some(index) = knife_draw_index {
                state.clip_index = index;
                state.knife_draw_seen = state.knife_draw_serial;
            }
            if std::env::var_os("CSR_WEAPON_TRACE").is_some() { info!("weapon activity: {} ACT_VM_DRAW", active); }
        }
        return;
    }
    if let Some(view) = models.weapons.get(&active) {
        if state.begin_life(p.life_epoch, &view.asset.model.bones, now,
            effects.attack_serial, request.serial()) {
            if let Some(index) = knife_draw_index {
                state.clip_index = index;
                state.knife_draw_seen = state.knife_draw_serial;
            }
            if std::env::var_os("CSR_WEAPON_TRACE").is_some() {
                info!("weapon activity: {} ACT_VM_DRAW life {}", active, p.life_epoch);
            }
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
    if !p.alive || local.noclip {
        state.look_motion.clear();
        state.elite_fire = None;
        return;
    }
    let Some(view) = models.weapons.get(&name) else {
        state.knife_inspect = None;
        state.knife_reinspect = None;
        return;
    };

    // Pick the activity from the weapon state.
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
    // Consume the local request even while observing somebody else; it must
    // never inspect a spectator target's weapon or replay on returning home.
    let inspect_action = request.after(&mut state.inspect_seen)
        .filter(|_| show && p.id == local.id && local.spectating.is_none());
    let knife = w.is_some_and(|w| w.def.class == weapons::WeaponClass::Knife);
    // Requests are consumed above, but only accepted edges may restart a clip.
    // Original stock/Butterfly controls distinguish F from R -> F: R preserves
    // the playing inspection and permits one subsequent restart. Determine
    // completion here too, before the ordinary completion pass below, so a
    // render gap cannot discard the first post-completion Inspect request.
    let knife_inspect_active = knife && state.activity == "INSPECT"
        && now >= state.started && view.clips.get("INSPECT")
            .and_then(|clips| clips.get(state.clip_index))
            .is_some_and(|seq| !seq.clip.frames.is_empty()
                && now - state.started <= seq.clip.duration());
    if !knife_inspect_active || state.knife_reinspect.is_some_and(|(tick, rate)|
        world.0.tick < tick || world.0.tick_rate != rate) { state.knife_reinspect = None; }
    if knife_inspect_active && inspect_action == Some(crate::tracers::InspectAction::Cancel) {
        state.knife_reinspect = Some((world.0.tick, world.0.tick_rate));
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
        state.reload_from_inspect = state.activity == "INSPECT" && !shell_gun
            && grenade.is_none() && suppressor.is_none();
        state.activity = if shell_gun { "ACT_SHOTGUN_RELOAD_START" } else { "ACT_VM_RELOAD" }.into();
        state.started = now;
    } else if !reloading && state.reloading && shell_gun && attack.is_none() && view.clips.contains_key("ACT_SHOTGUN_RELOAD_FINISH") {
        state.activity = "ACT_SHOTGUN_RELOAD_FINISH".into();
        state.started = now;
    } else if !reloading && state.reloading && !shell_gun && attack.is_none() {
        // Magazine reload completion/cancellation belongs to the simulation.
        // A render-time restart (e.g. joining a reloading spectator) cannot
        // keep playing a reload after its authoritative deadline has ended.
        state.activity = "ACT_VM_IDLE".into();
        state.started = now;
        state.clip_index = 0;
    } else if p.plant_start.is_some() && !state.planting {
        // C4: the keypad sequence while planting.
        state.activity = "ACT_VM_PRIMARYATTACK".into();
        state.started = now;
    } else if let Some(activity) = attack {
        state.activity = activity.into();
        // Select against the actual activity's sequence count below. Shadow
        // Daggers author five light misses; a fixed three excluded two clips.
        state.clip_index = if knife {
            state.completed_knife_index.take().unwrap_or(state.clip_index).wrapping_add(1)
        }
            else { (state.clip_index + 1) % 3 };
        state.started = now;
    }
    if suppressor.is_none() && attack.is_none() && !reloading && p.plant_start.is_none() && w.is_some_and(|w|w.zoom_level==0 && (knife || w.next_attack<=world.0.time())) && inspect_action == Some(crate::tracers::InspectAction::Start) && (!knife || !knife_inspect_active || state.knife_reinspect.is_some()) && view.clips.contains_key("INSPECT") {
        state.activity="INSPECT".into();state.started=now;state.clip_index=0;
        if knife {
            state.knife_reinspect = None;
            state.select_knife_variant(view, "INSPECT");
        }
        state.inspect_transition = state.inspect_transition.wrapping_add(1);
        if knife {
            state.knife_inspect = view.clips.get("INSPECT")
                .and_then(|clips| clips.get(state.clip_index))
                .filter(|seq| !seq.clip.frames.is_empty())
                .map(|seq| KnifeInspect {
                    key: KnifeInspectKey { owner: p.id, life: p.life_epoch,
                        weapon: name.clone(), edge: state.inspect_transition,
                        sequence: state.clip_index, started: state.started },
                    duration: seq.clip.duration(), sounds: seq.sounds.clone(),
                });
        }
    }
    if state.activity=="INSPECT" && w.is_some_and(|w|w.zoom_level>0) {state.activity="ACT_VM_IDLE".into();state.started=now;}
    if !reloading || state.activity != "ACT_VM_RELOAD" || grenade.is_some() || suppressor.is_some() {
        state.reload_from_inspect = false;
    }
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
    // Dispatch has now applied accepted interruptions. Rejected requests do
    // not touch the receipt. Natural completion below may retain it so a
    // final declared cue crossed by this render frame can still be emitted.
    if state.activity != "INSPECT" {
        state.knife_inspect = None;
        state.knife_reinspect = None;
    }
    // The installed draw activities carry explicit first-frame SHOW/HIDE events.
    let activity = if state.activity == "ACT_VM_DRAW" {
        w.map_or("ACT_VM_DRAW", |w| w.draw_activity())
    } else { state.activity.as_str() };
    let clips = view
        .clips
        .get(activity)
        .filter(|c| !c.is_empty())
        .or_else(|| {
            // Some knife variants name only the miss activities.
            match state.activity.as_str() {
                "ACT_VM_HITCENTER2" => view.clips.get("ACT_VM_MISSCENTER2").filter(|c| !c.is_empty()),
                "ACT_VM_HITCENTER" => view.clips.get("ACT_VM_MISSCENTER").filter(|c| !c.is_empty()),
                _ => None,
            }
        })
        .or_else(|| view.clips.get("ACT_VM_IDLE").filter(|c| !c.is_empty()));
    let Some(clips) = clips else {
        state.knife_inspect = None;
        state.knife_reinspect = None;
        return;
    };
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
        state.knife_reinspect = None;
        state.activity = "ACT_VM_IDLE".into();
        state.started = now;
        // This frame explicitly samples idle[0]. Retaining an attack/inspect
        // variant index would silently select a different idle next frame.
        if knife {
            state.completed_knife_index = Some(state.clip_index);
            state.clip_index = 0;
        }
        if let Some(idle) = view.clips.get("ACT_VM_IDLE").and_then(|c| c.first()) {
            seq = idle.clone();
            clip = idle.clip.clone();
            t = 0.0;
        }
    }

    if clip.frames.is_empty() {
        state.knife_inspect = None;
        state.knife_reinspect = None;
        return;
    }
    if state.started == now && std::env::var_os("CSR_WEAPON_TRACE").is_some() {
        info!("weapon activity: {} {}", name, state.activity);
    }
    let model = &view.asset.model;
    if state.pose.len() != model.bones.len() {
        state.pose = mdl::skeleton::rest_pose(&model.bones);
    }
    let elite_attack = matches!(state.activity.as_str(), "ACT_VM_PRIMARYATTACK" | "ACT_VM_SECONDARYATTACK");
    if name == "weapon_elite" && elite_attack && !reloading {
        if attack.is_some() {
            let mut fire = state.elite_fire.take().or_else(|| EliteFire::new(&model.bones));
            if let Some(fire) = &mut fire { fire.begin(&seq, now, &state.pose); }
            state.elite_fire = fire;
        }
    } else {
        // Reload/inspect/deployment own both arms. Completion returns to the
        // ordinary idle; subsequent idle frames must not create new attacks.
        state.elite_fire = None;
    }
    let weights = seq.weights.clone();
    // Restart inspection, or its incoming magazine reload, from what was
    // actually displayed, including an interrupted earlier inspect. Sampling against last frame repeatedly
    // would turn the authored duration into a frame-rate-dependent filter.
    let mut inspection = std::mem::take(&mut state.inspection);
    inspection.begin(
        (state.activity == "INSPECT"
            || state.reload_from_inspect).then_some(state.started),
        seq.fade_in,
        &state.pose,
        if knife && state.activity == "INSPECT" { state.knife_attack.clone() } else { None },
        state.inspect_transition,
        &state.activity,
    );
    inspection.sample_base(&mut state.pose);
    sequence_pose::sample(&clip, t, &mut state.pose, |b| weights.get(b).copied().unwrap_or(1.0));
    // Close eye relief magnifies SG's authored arm travel. Restrain the local
    // scoped presentation while retaining the full unscoped clip and aim ray.
    if name == "weapon_sg556" && state.activity == "ACT_VM_PRIMARYATTACK" {
        if let (Some(w), Some(idle)) = (w, view.clips.get("ACT_VM_IDLE").and_then(|clips| clips.first())) {
            let zoom = ((90. - w.rendered_fov(sim_now))
                / (90. - w.def.attr_f32("zoom fov 1").unwrap_or(45.))).clamp(0., 1.);
            if zoom > 0. {
                let mut neutral = mdl::skeleton::rest_pose(&model.bones);
                sequence_pose::sample(&idle.clip, 0., &mut neutral,
                    |b| idle.weights.get(b).copied().unwrap_or(1.));
                let amount = 1. - 0.65 * zoom;
                for (pose, idle) in state.pose.iter_mut().zip(neutral) {
                    *pose = (idle.0.lerp(pose.0, amount), idle.1.slerp(pose.1, amount));
                }
            }
        }
    }
    inspection.apply(now, &mut state.pose);
    state.inspection = inspection;
    if let Some(mut fire) = state.elite_fire.take() {
        fire.sample(now, &mut state.pose);
        state.elite_fire = Some(fire);
    }
    state.knife_attack = (knife && knife_attack_activity(&state.activity)).then(|| inspection::Playback {
        clip: clip.clone(), weights: weights.clone(), time: t,
    });
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
        let blend = ((90.0 - w.rendered_fov(world.0.time() + fixed.overstep_fraction() * world.0.dt())) / (90.0 - scoped_fov)).clamp(0.0, 1.0);
        let rear = view.scope_rear.map_or(center, |(position, _)| position);
        // Eye relief is measured to the REAR lens, not the forward dirt
        // plane. Subtracting center.x can place the rear housing on/behind
        // the camera when the scope tube is longer than the nominal relief.
        offset = offset.lerp(scoped_rear_offset(rear), blend);
    }
    let settling = if shared_motion {
        settling::Settling::sample(motion.velocity,
            movement::angle_vectors(glam::Vec3::new(0., local.yaw, 0.)).0)
    } else { settling::Settling::default() };
    let settled_offset = settling.offset(offset);
    let origin = eye + f * settled_offset.x - r * settled_offset.y
        + u * (settled_offset.z + air_offset) + look_translation;
    if std::env::var_os("CSR_VIEWMODEL_MOTION_TRACE").is_some() {
        info!("viewmodel motion: time={now:.6} angles={view_angles:?} eye={eye:?} look_translation={look_translation:?} settling_weight={} velocity={:?} on_ground={} settling_translation={:?} air_offset={air_offset}",
            settling.weight, motion.velocity, motion.animation.input.on_ground, settling.translation);
    }
    let world_from_model = Mat4::from_rotation_translation(rotation_to_bevy(rot * settling.rotation), to_bevy(origin));
    skinning::apply_pose(&gun, world_from_model, &pose, &mut transforms);
    wick.sample = held_wick(p, show, model, &pose, world_from_model);
    if show {if let Some(point)=crate::tracers::muzzle_position(model,&pose,seq.left_muzzle) {
        let muzzle=world_from_model.transform_point3(to_bevy(point));
        let world_fov=vertical_fov(w.map_or(90.0,|w|w.rendered_fov(world.0.time())));
        muzzles.first=Some((p.id,crate::tracers::project_viewmodel_muzzle(muzzle,to_bevy(eye),to_bevy(movement::angle_vectors(local.render_angles).0),world_fov,vertical_fov(VIEWMODEL_FOV))));
    }}
    if show && w.is_some_and(|w| w.def.class_name == "weapon_elite") {
        let world_fov=vertical_fov(w.map_or(90.0,|w|w.rendered_fov(world.0.time())));
        let pair = [false, true].map(|left| crate::tracers::muzzle_position(model, &pose, left).map(|point| {
            let muzzle = world_from_model.transform_point3(to_bevy(point));
            crate::tracers::project_viewmodel_muzzle(muzzle, to_bevy(eye),
                to_bevy(movement::angle_vectors(local.render_angles).0), world_fov, vertical_fov(VIEWMODEL_FOV))
        }));
        muzzles.first_pair=Some((p.id, pair));
    }
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

    let aperture = if w.is_some_and(|w| w.zoom_level > 0 && w.def.class == weapons::WeaponClass::Rifle) {
        // Use the same posed lens and world transform that skinning rendered.
        // The old idle rear + unmodified offset ignored firing, settling,
        // look translation and air motion, cutting a hole beside the housing.
        posed_scope_rear(model, &pose).map_or(Vec4::ZERO, |(rear, radius)| {
            let delta = world_from_model.transform_point3(to_bevy(rear)) - to_bevy(eye);
            let lens = Vec3::new(delta.dot(to_bevy(f)), -delta.dot(to_bevy(r)), delta.dot(to_bevy(u)));
            scope_aperture(lens, radius, Vec2::new(window.physical_width() as f32, window.physical_height() as f32))
        })
    } else { Vec4::ZERO };
    if let Some(optic) = optic.as_deref_mut() { optic.0 = aperture; }
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
        let (cube, sun_dir, sun_color) = crate::characters::lighting_at_collision(&level.0, &world.0.collision, characters.sun, eye);
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

//! Bounded presentation of immutable authoritative deaths using installed models.
//! Physical bodies use runtime model data; missing rigs retain the authored fallback.
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use super::{CharacterModels, own_materials, despawn_instance, lighting_at};
use crate::{coords::{rotation_to_bevy,to_bevy}, material::WorldMaterial, skinning::{self,SkinnedInstance}};
use sim::death::{DeathReceipt,DeathKey};
use std::collections::VecDeque;

const MAX_BODIES: usize = 16;
const BODY_LIFETIME: f32 = 20.;

struct Body {
    death: DeathReceipt,
    born: f32,
    instance: SkinnedInstance,
    materials: Vec<Handle<WorldMaterial>>,
    corpse: Option<ragdoll::Corpse>,
    pending: Option<Task<Option<ragdoll::Corpse>>>,
    captured_pose: Vec<glam::Mat4>,
    last_time: f32,
    trace_pending: bool,
}

#[derive(Default)]
pub(super) struct DeathBodies {
    bodies: VecDeque<Body>,
    seen: VecDeque<DeathKey>,
    round: Option<u32>,
}

impl DeathBodies {
    pub(super) fn performance_counts(&self) -> [usize; 3] {
        [
            self.bodies.len(),
            self.bodies.iter().filter(|b| b.pending.is_some()).count(),
            self.bodies.iter().filter(|b| b.corpse.is_some()).count(),
        ]
    }
}

fn admitted(death: &DeathReceipt, round: u32) -> bool {
    death.round == round && death.pose.origin.is_finite() && death.pose.view_angles.is_finite()
        && matches!(death.victim_team,sim::Team::Terrorist|sim::Team::CounterTerrorist)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn update(
    commands: &mut Commands, state: &mut DeathBodies, deaths: Vec<(DeathReceipt, Option<Vec<glam::Mat4>>)>,
    round: u32, now: f32, models: &mut CharacterModels,
    materials: &mut Assets<WorldMaterial>, level: &level::Level,
    mat_query: &Query<&MeshMaterial3d<WorldMaterial>>,
    transforms: &mut Query<&mut Transform>, visibility: &mut Query<&mut Visibility>,
) {
    let _frame_scope = crate::input_latency::frame_scope("characters.bodies");
    let sun = models.sun;
    if state.round != Some(round) {
        for body in state.bodies.drain(..) { despawn_instance(commands,&body.instance); }
        state.seen.clear();state.round=Some(round);
    }
    while state.bodies.front().is_some_and(|b| now-b.born >= BODY_LIFETIME) {
        despawn_instance(commands,&state.bodies.pop_front().unwrap().instance);
    }
    for (death, captured_pose) in deaths {
        if !admitted(&death,round) || state.seen.contains(&death.key) { continue; }
        state.seen.push_back(death.key);
        if state.seen.len()>128 { state.seen.pop_front(); }
        let model=match death.victim_team {sim::Team::Terrorist=>models.terrorist.as_mut(),_=>models.counter_terrorist.as_mut()};
        let Some(model)=model else {continue};
        // Reuse the already loaded meshes/textures/skeleton. No weapon remains
        // in hand: the existing authoritative dropped item owns that visual.
        let instance=skinning::spawn(commands,&model.asset,None,Some(materials));
        // Explicit ownership supplements the Transform Add observer. Session
        // teardown despawns every joint/mesh and resets Characters together.
        for e in instance.joints.iter().chain(&instance.meshes) {
            commands.entity(*e).insert(crate::session::MatchEntity);
        }
        for e in &instance.meshes {commands.entity(*e).insert(Visibility::Hidden);}
        if state.bodies.len()==MAX_BODIES {despawn_instance(commands,&state.bodies.pop_front().unwrap().instance);}
        // A victim can die before its first rendered animation. Raw MDL rest
        // bones are authored Y-up, unlike evaluated Source character poses;
        // placing that pose at the feet embeds its hulls in the floor. Start
        // from the installed death clip's evaluated first frame in this case.
        let Some(pose) = captured_pose.or_else(|| model.anims.death_pose(0.)) else {
            despawn_instance(commands, &instance);
            continue;
        };
        let trace_pending = std::env::var_os("CSR_DEATH_TRACE").is_some();
        if trace_pending {
            info!("death body initial: origin={:?} yaw={} velocity={:?} pelvis={:?} rig={}",
                death.pose.origin, death.pose.view_angles.y, death.pose.velocity, pose.first().map(|m|m.w_axis), model.ragdoll.is_some());
        }
        // Cooking nearby prop/brush geometry can span several frames. Keep it
        // off the render thread; the bounded body owns and cancels its task.
        let pending = model.ragdoll.as_ref().map(|rig| {
            let rig=rig.clone(); let collision=level.collision.clone(); let pose=pose.clone(); let death=death.clone();
            AsyncComputeTaskPool::get().spawn(async move {
                ragdoll::Corpse::new(rig, &collision, &pose, death.pose.origin, death.pose.view_angles.y,
                    death.pose.velocity, death.hit.map(|hit| (hit.position, hit.direction)))
            })
        });
        state.bodies.push_back(Body {death,born:now,instance,materials:Vec::new(),corpse:None,pending,captured_pose:pose,last_time:now,trace_pending});
    }
    for body in &mut state.bodies {
        let model=match body.death.victim_team {sim::Team::Terrorist=>models.terrorist.as_mut(),_=>models.counter_terrorist.as_mut()};
        let Some(model)=model else {continue};
        if let Some(result) = body.pending.as_mut().and_then(|task| block_on(poll_once(task))) {
            body.corpse=result; body.pending=None;
        }
        let (pose, world) = if let Some(corpse) = &mut body.corpse {
            corpse.advance((now-body.last_time).max(0.));
            body.last_time=now;
            (corpse.bone_matrices(), Mat4::IDENTITY)
        } else if body.pending.is_some() {
            let rotation=rotation_to_bevy(mdl::angles_to_mat3(glam::Vec3::new(0.,body.death.pose.view_angles.y,0.)));
            (body.captured_pose.clone(),Mat4::from_rotation_translation(rotation,to_bevy(body.death.pose.origin)))
        } else {
            let Some(pose)=model.anims.death_pose(now-body.born) else {continue};
            let rotation=rotation_to_bevy(mdl::angles_to_mat3(glam::Vec3::new(0.,body.death.pose.view_angles.y,0.)));
            (pose,Mat4::from_rotation_translation(rotation,to_bevy(body.death.pose.origin)))
        };
        if body.trace_pending && body.corpse.is_some() && now-body.born > 0.5 {
            body.trace_pending=false;
            let floor=level.collision.trace_ray(body.death.pose.origin+glam::Vec3::Z*16.,body.death.pose.origin-glam::Vec3::Z*256.,collision::contents::MASK_SOLID);
            let (low,high)=pose.iter().fold((glam::Vec3::splat(f32::MAX),glam::Vec3::splat(f32::MIN)),|(low,high),m| (low.min(m.w_axis.truncate()),high.max(m.w_axis.truncate())));
            info!("death body physical: age={} pelvis={:?} bone_low={low:?} bone_high={high:?} floor={:?} floor_hit={}",now-body.born,pose.first().map(|m|m.w_axis),floor.end,floor.hit());
        }
        // Deferred entities stay hidden until their actual pose can be written.
        if body.instance.joints.iter().any(|e| transforms.get(*e).is_err()) {continue;}
        skinning::apply_pose(&body.instance,world,&pose,transforms);
        if body.materials.is_empty() {
            body.materials=own_materials(&body.instance,mat_query);
            let (cube,sun_dir,sun_color)=lighting_at(level,sun,body.death.pose.origin+glam::Vec3::Z*32.);
            for handle in &body.materials {
                if let Some(mut m)=materials.get_mut(handle) {m.params.ambient_cube=cube;m.params.sun_dir=sun_dir;m.params.sun_color=sun_color;}
            }
        }
        for e in &body.instance.meshes {
            if let Ok(mut v)=visibility.get_mut(*e) {v.set_if_neq(Visibility::Inherited);}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::{BTreeMap, HashMap}, sync::Arc};

    #[derive(Resource)]
    struct BodyFixture(DeathBodies);

    #[derive(Component)]
    struct BodyMesh;

    #[derive(Resource, Default)]
    struct VisibilityChanges(Vec<Entity>);

    fn record_visibility_changes(
        changed: Query<Entity, (With<BodyMesh>, Changed<Visibility>)>,
        mut seen: ResMut<VisibilityChanges>,
    ) {
        seen.0 = changed.iter().collect();
    }

    fn update_fixture(
        mut commands: Commands,
        mut state: ResMut<BodyFixture>,
        mut models: ResMut<CharacterModels>,
        mut materials: ResMut<Assets<WorldMaterial>>,
        level: Res<crate::LoadedLevel>,
        mat_query: Query<&MeshMaterial3d<WorldMaterial>>,
        mut transforms: Query<&mut Transform>,
        mut visibility: Query<&mut Visibility>,
    ) {
        update(&mut commands, &mut state.0, Vec::new(), 1, 1.0, &mut models,
            &mut materials, &level.0, &mat_query, &mut transforms, &mut visibility);
    }

    #[test]
    fn real_bevy_death_update_reveals_hidden_mesh_then_visibility_settles() {
        bevy::tasks::ComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let pool = AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        // Keep the existing pending-cook path deterministic without running a
        // physics cook. The fixture owns/cancels this task when it is dropped.
        let pending = pool.spawn(std::future::pending::<Option<ragdoll::Corpse>>());

        // Both content files are synthetic in-memory headers. Loading through
        // existing readers avoids test-only constructors in production code.
        let mut bsp = vec![0; 8 + 64 * 16 + 4];
        bsp[..4].copy_from_slice(b"VBSP");
        bsp[4..8].copy_from_slice(&21i32.to_le_bytes());
        let mut mdl = vec![0; 408];
        mdl[..4].copy_from_slice(b"IDST");
        mdl[4..8].copy_from_slice(&48i32.to_le_bytes());
        let mut fs = vfs::Vfs::new();
        fs.push_back("synthetic", vfs::Mount::Memory(BTreeMap::from([
            ("maps/body-fixture.bsp".into(), Arc::from(bsp)),
            ("models/body-fixture.mdl".into(), Arc::from(mdl)),
        ])));
        let model = Arc::new(mdl::Model {
            include_models: vec!["models/body-fixture.mdl".into()],
            ..Default::default()
        });
        let anims = Arc::new(crate::player_anim::PlayerAnimSet::load(&model, &fs).unwrap());
        let level = level::Level::load_from_vfs(
            content::ContentId::csgo(content::ContentKind::Map, "body-fixture"),
            "body-fixture", "no-game-install".into(), fs,
        ).unwrap();
        let mut app = App::new();
        app.add_plugins(bevy::camera::visibility::VisibilityPlugin)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>()
            .init_resource::<Assets<WorldMaterial>>()
            .init_resource::<VisibilityChanges>()
            .insert_resource(crate::LoadedLevel(level))
            .insert_resource(CharacterModels {
                terrorist: Some(super::super::TeamModel {
                    asset: skinning::SkinnedAsset {
                        model, parts: Vec::new(), inverse_bindposes: Handle::default(),
                    },
                    anims, ragdoll: None,
                }),
                counter_terrorist: None, weapons: HashMap::new(), sun: None,
            })
            .add_systems(Last, record_visibility_changes);
        let joint = app.world_mut().spawn(Transform::default()).id();
        let mesh = app.world_mut().spawn((
            BodyMesh, Mesh3d(Handle::default()), Visibility::Hidden,
            bevy::camera::visibility::NoFrustumCulling,
        )).id();
        // Consume Added<Visibility> before the real update reveals the mesh;
        // otherwise an insertion tick could mask a missing transition write.
        app.update();
        app.update();
        assert!(app.world().resource::<VisibilityChanges>().0.is_empty());
        assert!(!app.world().get::<InheritedVisibility>(mesh).unwrap().get());
        app.insert_resource(BodyFixture(DeathBodies {
            bodies: VecDeque::from([Body {
                death: DeathReceipt {
                    key: DeathKey { victim: 1, life_epoch: 1, tick: 1 }, round: 1,
                    pose: sim::death::DeathPose {
                        origin: glam::Vec3::ZERO, view_angles: glam::Vec3::ZERO,
                        velocity: glam::Vec3::ZERO, ducked: false, duck_amount: 0.0,
                        on_ground: true, tracked: None,
                    },
                    victim_name: "fixture".into(), victim_team: sim::Team::Terrorist,
                    attacker: None, hit: None,
                },
                born: 0.0, instance: SkinnedInstance { joints: vec![joint], meshes: vec![mesh] },
                // Skip unrelated material acquisition/lighting, just as after
                // a body's own handles have already been acquired.
                materials: vec![Handle::default()], corpse: None, pending: Some(pending),
                captured_pose: vec![glam::Mat4::IDENTITY], last_time: 0.0, trace_pending: false,
            }]),
            seen: VecDeque::new(), round: Some(1),
        })).add_systems(Update, update_fixture);
        app.update();
        assert_eq!(app.world().resource::<VisibilityChanges>().0, vec![mesh]);
        assert_eq!(*app.world().get::<Visibility>(mesh).unwrap(), Visibility::Inherited);
        assert!(app.world().get::<InheritedVisibility>(mesh).unwrap().get());
        for _ in 0..3 {
            app.update();
            assert!(app.world().resource::<VisibilityChanges>().0.is_empty());
            assert_eq!(*app.world().get::<Visibility>(mesh).unwrap(), Visibility::Inherited);
            assert!(app.world().get::<InheritedVisibility>(mesh).unwrap().get());
        }
    }
}

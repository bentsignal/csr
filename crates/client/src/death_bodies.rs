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
        let model=match death.victim_team {sim::Team::Terrorist=>models.terrorist.as_ref(),_=>models.counter_terrorist.as_ref()};
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
        let pose = captured_pose.unwrap_or_else(|| mdl::skeleton::model_space(
            &model.asset.model.bones, &mdl::skeleton::rest_pose(&model.asset.model.bones)));
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
            if let Ok(mut v)=visibility.get_mut(*e) {*v=Visibility::Inherited;}
        }
    }
}

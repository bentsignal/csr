//! Movement modules share CS combat and lifecycle. Only World::step selects
//! a module or publishes its state; rendering never advances mechanics.
use crate::{Player, TickInput};
use glam::Vec3;
use mechanics_api as api;
use movement::{MoveEvents, TraceWorld, buttons};
use std::sync::Arc;

pub struct Module {
    pub id: String,
    pub name: String,
    pub archive_hash: String,
    pub program: Option<mechanics::Program>,
    pub config: [f32; api::CONFIG_WORDS],
    pub external: Option<Arc<mechanics::external::Program>>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExternalState {
    pub session: u64,
    pub provider_hash: String,
    pub digest: [u8;32],
    pub start: mechanics::link::Start,
    pub history: mechanics::external::Journal,
    pub pose: mechanics::link::Pose,
    pub previous_pose: mechanics::link::Pose,
    pub rig: mechanics::link::Rig,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    pub id: String,
    pub archive_hash: String,
    pub data: [f32; api::STATE_WORDS],
    /// Pitch/yaw/roll of the board, independent of the weapon aim.
    pub angles: Vec3,
    pub height: f32,
    pub reset: bool,
    #[serde(default)]
    pub external: Option<Box<ExternalState>>,
}

pub(crate) fn toggle(player: &mut Player, modules: &[Arc<Module>]) {
    // Cycle through installed modes, then ordinary CS. A sole Skate pack is
    // therefore one key in either direction, with no map or inventory reset.
    let next = player
        .mechanic
        .as_ref()
        .and_then(|s| modules.iter().position(|m| m.id == s.id))
        .map_or(0, |i| i + 1);
    player.mechanic = modules.get(next).map(|module| State {
        id: module.id.clone(),
        archive_hash: module.archive_hash.clone(),
        data: [0.; api::STATE_WORDS],
        angles: Vec3::new(0., player.view_angles.y, 0.),
        height: 0.,
        reset: true,
        external: None,
    });
    player.movement.ladder_normal = None;
}

pub(crate) fn step(
    player: &mut Player,
    input: &TickInput,
    modules: &[Arc<Module>],
    world: &dyn TraceWorld,
    dt: f32,
    time: f32,
    collision: &Arc<collision::CollisionWorld>,
    scene: mechanics::link::SceneState,
) -> Result<MoveEvents, String> {
    let state = player.mechanic.as_ref().ok_or("no active mechanic")?;
    let module = modules
        .iter()
        .find(|m| m.id == state.id && m.archive_hash == state.archive_hash)
        .ok_or("active mechanics pack changed or is unavailable")?;
    if let Some(program) = &module.external {
        let start = mechanics::link::Start { position:player.movement.origin.to_array(),velocity:player.movement.velocity.to_array(),yaw:player.view_angles.y };
        let state=player.mechanic.as_mut().unwrap();
        if state.reset {state.external=None;}
        let external=state.external.get_or_insert_with(||Box::new(ExternalState{
            session:((time.to_bits() as u64)<<32)|player.id as u64,provider_hash:program.fingerprint().into(),digest:mechanics::external::digest([0;32],&start),start,history:Default::default(),pose:Default::default(),previous_pose:Default::default(),
            rig:mechanics::link::Rig{names:vec![],parents:vec![],rest:vec![],to_source:[[0.;4];4],bone_basis:[[0.;4];4]},
        }));
        let frame=mechanics::link::Frame{input:input.controller,dt,scene};
        if external.provider_hash!=program.fingerprint(){return Err("movement runtime changed since this state was recorded".into());}
        let (pose,rig)=program.advance_world(collision,external.session,&external.start,&external.history,external.digest,&frame)?;
        if pose.position.iter().chain(pose.velocity.iter()).any(|v|!v.is_finite()||v.abs()>131072.)
            ||pose.bones.len()>256||pose.bones.iter().flatten().flatten().any(|v|!v.is_finite()) {
            return Err("invalid provider physical pose".into());
        }
        let was_grounded=player.movement.on_ground;
        player.movement.origin=Vec3::from_array(pose.position);
        player.movement.velocity=Vec3::from_array(pose.velocity);
        player.movement.on_ground=pose.grounded;
        player.movement.ground_ticks=if pose.grounded{player.movement.ground_ticks.saturating_add(1)}else{0};
        player.movement.ducked=false;player.movement.duck_amount=0.;
        player.movement.ladder_normal=None;
        player.movement.old_buttons=input.buttons;
        external.digest=mechanics::external::digest(external.digest,&frame);
        if pose.generation!=external.pose.generation {external.previous_pose=external.pose.clone();}
        if pose.discontinuity||pose.period!=external.pose.period||external.previous_pose.generation==0 {external.previous_pose=pose.clone();}
        external.history.push(frame);external.pose=pose;external.rig=rig;state.reset=false;state.height=0.;
        return Ok(MoveEvents{jumped:was_grounded&&!player.movement.on_ground&&player.movement.velocity.z>0.,..Default::default()});
    }
    let mut packet = [0.; api::WORDS];
    packet[api::API] = api::VERSION;
    packet[api::DT] = dt;
    packet[api::POSITION..api::POSITION + 3].copy_from_slice(&player.movement.origin.to_array());
    packet[api::VELOCITY..api::VELOCITY + 3].copy_from_slice(&player.movement.velocity.to_array());
    packet[api::GROUNDED] = u8::from(player.movement.on_ground) as f32;
    packet[api::YAW] = input.view_angles.y;
    packet[api::PITCH] = input.view_angles.x;
    packet[api::FORWARD] = (input.forward_move / 450.).clamp(-1., 1.);
    packet[api::SIDE] = (input.side_move / 450.).clamp(-1., 1.);
    for (at, bit) in [
        (api::JUMP, buttons::JUMP),
        (api::DUCK, buttons::DUCK),
        (api::BRAKE, buttons::WALK),
    ] {
        packet[at] = u8::from(input.buttons & bit != 0) as f32;
    }
    if player.movement.ducked && packet[api::DUCK] == 0. {
        let mut maxs = movement::STAND_MAXS;
        maxs.z += state.height;
        let stand = world.trace_hull(
            player.movement.origin,
            player.movement.origin,
            movement::STAND_MINS,
            maxs,
            collision::contents::MASK_PLAYERSOLID,
        );
        if stand.start_solid || stand.all_solid {
            packet[api::DUCK] = 1.;
        }
    }
    packet[api::RESET] = u8::from(state.reset) as f32;
    packet[api::TIME] = time;
    packet[api::STATE..api::STATE + api::STATE_WORDS].copy_from_slice(&state.data);
    packet[api::CONFIG..].copy_from_slice(&module.config);
    let out = module.program.as_ref().ok_or("movement program unavailable")?.step(&packet, world)?;
    let grounded = out[api::OUT_GROUNDED] == 1.;
    let previous_grounded = player.movement.on_ground;
    let fall_speed = -player.movement.velocity.z;
    player.movement.origin = Vec3::from_slice(&out[api::OUT_POSITION..api::OUT_POSITION + 3]);
    player.movement.velocity = Vec3::from_slice(&out[api::OUT_VELOCITY..api::OUT_VELOCITY + 3]);
    player.movement.on_ground = grounded;
    player.movement.ducked = packet[api::DUCK] != 0.;
    player.movement.duck_amount = u8::from(player.movement.ducked) as f32;
    player.movement.old_buttons = input.buttons;
    player.movement.ground_ticks = if grounded {
        player.movement.ground_ticks.saturating_add(1)
    } else {
        0
    };
    player.movement.ladder_normal = None;
    let state = player.mechanic.as_mut().unwrap();
    state
        .data
        .copy_from_slice(&out[api::OUT_STATE..api::OUT_STATE + api::STATE_WORDS]);
    state.angles = Vec3::new(out[api::OUT_PITCH], out[api::OUT_YAW], out[api::OUT_ROLL]);
    state.height = out[api::OUT_HEIGHT];
    state.reset = false;
    Ok(MoveEvents {
        jumped: previous_grounded && !grounded && player.movement.velocity.z > 0.,
        landed: (!previous_grounded && grounded).then_some(fall_speed.max(0.)),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn world(trapping: bool) -> (crate::World, crate::PlayerId) {
        let body = if trapping {
            "unreachable"
        } else {
            "i32.const 256 i32.const 8 i32.const 24 memory.copy i32.const 280 f32.const 1 f32.store i32.const 320 i32.const 80 f32.load f32.const 1 f32.add f32.store"
        };
        let wasm = wat::parse_str(format!(
            r#"(module (memory (export "memory") 1)
            (func (export "csr_packet") (result i32) i32.const 0)
            (func (export "csr_step") (result i32) {body} i32.const 0))"#
        ))
        .unwrap();
        let module = Module {
            id: "mod_mode_test".into(),
            name: "Test".into(),
            archive_hash: "hash".into(),
            program: Some(mechanics::Program::compile(&wasm).unwrap()),
            config: [0.; api::CONFIG_WORDS],
            external: None,
        };
        let mut world = crate::World::new(Arc::new(collision::CollisionWorld::new()));
        world.mechanics.push(Arc::new(module));
        let id = world.add_player("Test", crate::Team::CounterTerrorist);
        (world, id)
    }
    #[test]
    fn toggle_edges_state_restore_and_respawn_share_tick_authority() {
        let (mut w, id) = world(false);
        let before = w.player(id).unwrap().weapons.len();
        let press = TickInput {
            toggle_mode: true,
            ..Default::default()
        };
        w.step(&[(id, press)]);
        assert!(w.player(id).unwrap().mechanic.is_some());
        w.step(&[(id, press)]);
        assert_eq!(w.player(id).unwrap().mechanic.as_ref().unwrap().data[0], 2.);
        let mut restored = w.clone();
        restored.restore(w.state());
        for _ in 0..3 {
            w.step(&[(id, TickInput::default())]);
            restored.step(&[(id, TickInput::default())]);
            assert_eq!(w.state_hash(), restored.state_hash());
        }
        w.respawn(id);
        assert!(w.player(id).unwrap().mechanic.as_ref().unwrap().reset);
        w.step(&[(id, TickInput::default())]);
        w.step(&[(id, press)]);
        assert!(w.player(id).unwrap().mechanic.is_none());
        assert_eq!(w.player(id).unwrap().weapons.len(), before);
    }
    #[test]
    fn a_module_trap_falls_back_to_cs_without_dropping_inventory() {
        let (mut w, id) = world(true);
        let before = w.player(id).unwrap().weapons.len();
        w.step(&[(
            id,
            TickInput {
                toggle_mode: true,
                ..Default::default()
            },
        )]);
        assert!(w.player(id).unwrap().mechanic.is_none());
        assert_eq!(w.player(id).unwrap().weapons.len(), before);
        assert!(w.player(id).unwrap().movement.origin.is_finite());
    }
}

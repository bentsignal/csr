//! Local prediction and authoritative correction, independent of UDP/Bevy.
//! The caller builds a World from the server's verified map/catalog and drives
//! one prediction per local fixed tick at the rate in the welcome message.
use crate::{protocol::*, session::validate_input};
use glam::Vec3;
use sim::{PlayerId, World, WorldState};
use std::collections::VecDeque;

pub const MAX_PENDING_INPUTS: usize = 256;

#[derive(Clone, Debug, PartialEq)]
pub enum Reconciliation {
    IgnoredStale,
    Corrected {
        authoritative_tick: u64,
        acknowledged_input: u64,
        replayed_inputs: usize,
        /// Previous predicted origin minus the corrected/replayed origin.
        /// Rendering can ease this offset without changing gameplay state.
        position_error: Vec3,
    },
}

pub struct Predictor {
    world: World,
    welcome: ServerControl,
    player: PlayerId,
    authoritative: WorldState,
    pending: VecDeque<InputCommand>,
    last_sent: u64,
    last_ack: u64,
    capacity: usize,
}

impl Predictor {
    pub fn new(world: World, welcome: ServerControl, initial: StateUpdate) -> Result<Self, String> {
        Self::with_capacity(world, welcome, initial, MAX_PENDING_INPUTS)
    }

    pub fn with_capacity(
        mut world: World,
        welcome: ServerControl,
        initial: StateUpdate,
        capacity: usize,
    ) -> Result<Self, String> {
        if world.collision.dynamic.is_some() {
            return Err(
                "dynamic world prediction requires replicated replay-safe mod state".into(),
            );
        }
        if capacity == 0 || capacity > MAX_PENDING_INPUTS {
            return Err("invalid prediction history capacity".into());
        }
        let ServerControl::Welcome {
            version,
            player,
            tick_rate,
            content,
            ..
        } = &welcome
        else {
            return Err("prediction requires server admission".into());
        };
        if *version != VERSION || *player == 0 || !sim::TICK_RATE_RANGE.contains(tick_rate) {
            return Err("invalid prediction welcome".into());
        }
        content.validate()?;
        let player = *player;
        let ack = initial.acknowledged_input;
        validate_state(&initial, Some(&welcome), ack, ack)?;
        world.validate_door_state(&initial.snapshot.state.doors)?;
        let authoritative = initial.snapshot.state;
        world.restore(authoritative.clone());
        world.events.clear();
        Ok(Self {
            world,
            welcome,
            player,
            authoritative,
            pending: VecDeque::new(),
            last_sent: ack,
            last_ack: ack,
            capacity,
        })
    }

    /// Predicted movement/weapon state. Its events are always empty. HUD
    /// economy/round results and remote truth come from authoritative_state.
    pub fn world(&self) -> &World {
        &self.world
    }
    pub fn authoritative_state(&self) -> &WorldState {
        &self.authoritative
    }
    pub fn player_id(&self) -> PlayerId {
        self.player
    }
    pub fn pending_inputs(&self) -> usize {
        self.pending.len()
    }
    pub fn acknowledged_input(&self) -> u64 {
        self.last_ack
    }
    pub fn authoritative_tick(&self) -> u64 {
        self.authoritative.tick
    }
    pub fn can_predict(&self) -> bool {
        self.pending.len() < self.capacity
            && self.world.tick < MAX_LOCAL_TICK
            && self.last_sent < MAX_PROTOCOL_COUNTER
    }

    /// Canonicalize and validate a command before transport advances a sequence.
    /// This does not mutate prediction or retain input.
    pub fn prepare(&self, input: sim::TickInput, view_tick: u64) -> Result<InputCommand, String> {
        if !self.can_predict() {
            return Err("prediction cannot advance".into());
        }
        if view_tick > self.authoritative.tick {
            return Err("prediction view tick is in the future".into());
        }
        let input = validate_input(input)?;
        Ok(InputCommand {
            sequence: self.last_sent + 1,
            view_tick,
            input,
        })
    }

    /// The sequence is the one returned by Client::send_input. Do not send
    /// another command while can_predict is false: await an authoritative
    /// update rather than losing the bounded replay history.
    pub fn predict(&mut self, mut command: InputCommand) -> Result<(), String> {
        if !self.can_predict() {
            return Err("prediction history is full; awaiting authority".into());
        }
        if command.sequence > MAX_PROTOCOL_COUNTER
            || self.last_sent.checked_add(1) != Some(command.sequence)
        {
            return Err("prediction input sequence is not consecutive".into());
        }
        if command.view_tick > self.authoritative.tick {
            return Err("prediction view tick is in the future".into());
        }
        command.input = validate_input(command.input)?;
        if self.world.tick >= MAX_LOCAL_TICK {
            return Err("local prediction timeline exhausted".into());
        }
        self.world.step_prediction(&[(self.player, command.input)]);
        self.pending.push_back(command);
        self.last_sent = command.sequence;
        Ok(())
    }

    /// Restore server truth, then silently replay only unacknowledged input.
    /// Reliable authoritative effects are consumed separately by the caller;
    /// reconciliation never returns or retains replay-generated GameEvents.
    pub fn reconcile(&mut self, state: StateUpdate) -> Result<Reconciliation, String> {
        if state.snapshot.state.tick <= self.authoritative.tick {
            return Ok(Reconciliation::IgnoredStale);
        }
        validate_state(&state, Some(&self.welcome), self.last_sent, self.last_ack)?;
        self.world.validate_door_state(&state.snapshot.state.doors)?;
        let before = self
            .world
            .player(self.player)
            .expect("validated assigned player")
            .movement
            .origin;
        self.last_ack = state.acknowledged_input;
        self.pending
            .retain(|command| command.sequence > self.last_ack);
        self.authoritative = state.snapshot.state;
        self.world.restore(self.authoritative.clone());
        self.world.events.clear();
        for command in &self.pending {
            self.world.step_prediction(&[(self.player, command.input)]);
        }
        let after = self
            .world
            .player(self.player)
            .expect("prediction retains assigned player")
            .movement
            .origin;
        Ok(Reconciliation::Corrected {
            authoritative_tick: self.authoritative.tick,
            acknowledged_input: self.last_ack,
            replayed_inputs: self.pending.len(),
            position_error: before - after,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{Team, TickInput};
    use std::sync::Arc;

    fn fixture(rate: u32) -> (World, PlayerId, ServerControl) {
        let mut collision = collision::CollisionWorld::new();
        let a = Vec3::new(-4096., -4096., 0.);
        let b = Vec3::new(4096., -4096., 0.);
        let c = Vec3::new(4096., 4096., 0.);
        let d = Vec3::new(-4096., 4096., 0.);
        collision.add_triangle([a, b, c], collision::contents::SOLID, 0);
        collision.add_triangle([a, c, d], collision::contents::SOLID, 0);
        collision.finalize();
        let mut world = World::new(Arc::new(collision));
        world.set_tick_rate(rate);
        let player = world.add_player("Local", Team::CounterTerrorist);
        let content = ContentManifest {
            map_id: "csgo_map_prediction".into(),
            map_name: "prediction".into(),
            map_hash: "0".repeat(40),
            catalog_hash: "1".repeat(40),
        };
        let welcome = ServerControl::Welcome {
            version: VERSION,
            player,
            tick_rate: rate,
            server_tick: 0,
            content,
        };
        (world, player, welcome)
    }
    fn update(world: &World, ack: u64) -> StateUpdate {
        StateUpdate {
            acknowledged_input: ack,
            snapshot: world.snapshot(),
        }
    }
    fn command(sequence: u64) -> InputCommand {
        InputCommand {
            sequence,
            view_tick: 0,
            input: TickInput {
                forward_move: 450.,
                ..Default::default()
            },
        }
    }

    #[test]
    fn matching_inputs_remain_deterministic_at_64_and_128_and_trim_replay() {
        for rate in [64, 128] {
            let (mut authority, player, welcome) = fixture(rate);
            let mut predictor =
                Predictor::new(authority.clone(), welcome, update(&authority, 0)).unwrap();
            let mut partial = None;
            for sequence in 1..=8 {
                let command = command(sequence);
                predictor.predict(command).unwrap();
                authority.step(&[(player, command.input)]);
                if sequence == 3 {
                    partial = Some(update(&authority, sequence));
                }
            }
            let corrected = predictor.reconcile(partial.unwrap()).unwrap();
            assert!(
                matches!(corrected, Reconciliation::Corrected { replayed_inputs: 5, position_error, .. } if position_error == Vec3::ZERO)
            );
            assert_eq!(predictor.world().state(), authority.state());
            assert_eq!(predictor.pending_inputs(), 5);
            predictor.reconcile(update(&authority, 8)).unwrap();
            assert_eq!(predictor.pending_inputs(), 0);
            assert_eq!(predictor.acknowledged_input(), 8);
            assert_eq!(predictor.world().state(), authority.state());
        }
    }

    #[test]
    fn authoritative_divergence_is_corrected_before_replaying_unacknowledged_inputs() {
        let (mut authority, player, welcome) = fixture(128);
        let mut predictor =
            Predictor::new(authority.clone(), welcome, update(&authority, 0)).unwrap();
        for sequence in 1..=8 {
            predictor.predict(command(sequence)).unwrap();
        }
        for sequence in 1..=3 {
            authority.step(&[(player, command(sequence).input)]);
        }
        authority.player_mut(player).unwrap().movement.origin.y += 128.;
        let state = update(&authority, 3);
        let authoritative = state.snapshot.state.clone();
        let mut expected = authority.clone();
        for sequence in 4..=8 {
            expected.step(&[(player, command(sequence).input)]);
        }
        let correction = predictor.reconcile(state).unwrap();
        assert!(
            matches!(correction, Reconciliation::Corrected { replayed_inputs: 5, position_error, .. } if position_error.y.abs() > 100.)
        );
        assert_eq!(predictor.world().state(), expected.state());
        assert_eq!(predictor.authoritative_state(), &authoritative);
        assert!(predictor.world().events.is_empty());
    }

    #[test]
    fn stale_snapshots_and_bad_acknowledgements_do_not_mutate_prediction() {
        let (mut authority, player, welcome) = fixture(128);
        let initial = update(&authority, 0);
        let mut predictor = Predictor::new(authority.clone(), welcome, initial.clone()).unwrap();
        predictor.predict(command(1)).unwrap();
        predictor.predict(command(2)).unwrap();
        authority.step(&[(player, command(1).input)]);
        predictor.reconcile(update(&authority, 1)).unwrap();
        let before = predictor.world().state();
        let mut stale = initial;
        stale.acknowledged_input = 1000;
        assert_eq!(
            predictor.reconcile(stale).unwrap(),
            Reconciliation::IgnoredStale
        );
        authority.step(&[(player, command(2).input)]);
        for ack in [0, 3] {
            assert!(predictor.reconcile(update(&authority, ack)).is_err());
        }
        let mut wrong_rate = update(&authority, 2);
        wrong_rate.snapshot.state.tick_rate = 64;
        assert!(predictor.reconcile(wrong_rate).is_err());
        let mut invalid_physics = update(&authority, 2);
        invalid_physics.snapshot.state.config.gravity = f32::NAN;
        assert!(predictor.reconcile(invalid_physics).is_err());
        let mut invalid_tick = update(&authority, 2);
        invalid_tick.snapshot.state.tick = u64::MAX;
        assert!(predictor.reconcile(invalid_tick).is_err());
        assert_eq!(predictor.world().state(), before);
        assert_eq!(predictor.pending_inputs(), 1);
        assert_eq!(predictor.acknowledged_input(), 1);
    }

    #[test]
    fn exhausted_local_timeline_is_rejected_before_advancing_or_retaining_input() {
        let (authority, _, welcome) = fixture(128);
        let mut predictor =
            Predictor::new(authority.clone(), welcome, update(&authority, 0)).unwrap();
        predictor.world.tick = MAX_LOCAL_TICK;
        let before = predictor.world().state();
        assert!(predictor.predict(command(1)).is_err());
        assert_eq!(predictor.world().state(), before);
        assert_eq!(predictor.pending_inputs(), 0);
    }

    #[test]
    fn bounded_history_waits_for_authority_without_losing_inputs() {
        let (mut authority, player, welcome) = fixture(128);
        let mut predictor =
            Predictor::with_capacity(authority.clone(), welcome, update(&authority, 0), 2).unwrap();
        predictor.predict(command(1)).unwrap();
        predictor.predict(command(2)).unwrap();
        let before = predictor.world().state();
        assert!(!predictor.can_predict());
        assert!(predictor.predict(command(3)).is_err());
        assert_eq!(predictor.world().state(), before);
        authority.step(&[(player, command(1).input)]);
        predictor.reconcile(update(&authority, 1)).unwrap();
        assert!(predictor.can_predict());
        predictor.predict(command(3)).unwrap();
        assert_eq!(predictor.pending_inputs(), 2);
    }

    #[test]
    fn tracked_head_hands_and_separate_aim_survive_silent_replay() {
        let (mut authority, player, welcome) = fixture(128);
        let mut predictor =
            Predictor::new(authority.clone(), welcome, update(&authority, 0)).unwrap();
        let pose = sim::Pose {
            rotation: glam::Quat::IDENTITY,
            ..Default::default()
        };
        let input = TickInput {
            tracked: Some(sim::TrackedPose {
                head: sim::Pose {
                    position: Vec3::Z * 64.,
                    ..pose
                },
                left_hand: sim::Pose {
                    position: Vec3::new(-20., -10., 40.),
                    ..pose
                },
                right_hand: sim::Pose {
                    position: Vec3::new(20., 10., 40.),
                    ..pose
                },
            }),
            aim: Some(sim::AimRay {
                origin: Vec3::new(20., 10., 40.),
                direction: Vec3::X,
            }),
            view_angles: Vec3::new(0., 90., 0.),
            ..Default::default()
        };
        for sequence in 1..=3 {
            predictor
                .predict(InputCommand {
                    sequence,
                    view_tick: 0,
                    input,
                })
                .unwrap();
        }
        authority.step(&[(player, input)]);
        let mut state = update(&authority, 1);
        state.snapshot.events.push(sim::GameEvent::Jump { player });
        predictor.reconcile(state).unwrap();
        let local = predictor.world().player(player).unwrap();
        assert_eq!(local.tracked, input.tracked);
        assert_eq!(local.aim.unwrap().direction, Vec3::X);
        assert_eq!(local.view_angles.y, 90.);
        assert!(predictor.world().events.is_empty());
    }

    #[test]
    fn remote_bot_metadata_stays_authoritative_without_generated_decisions() {
        let (mut world, _, welcome) = fixture(128);
        let bot = world.add_bot("Remote", Team::Terrorist);
        let initial = update(&world, 0);
        let mut predictor = Predictor::new(world, welcome, initial).unwrap();
        for sequence in 1..=8 {
            predictor.predict(command(sequence)).unwrap();
        }
        let actor = predictor.world().player(bot).unwrap();
        assert!(actor.is_bot);
        assert_eq!(actor.last_input, TickInput::default());
        assert!(predictor.world().events.is_empty());
    }
    #[test]
    fn external_dynamic_state_is_rejected_before_prediction_can_mutate_it() {
        struct External(std::sync::atomic::AtomicUsize);
        impl collision::DynamicWorld for External {
            fn trace_box(&self, _: Vec3, end: Vec3, _: Vec3, _: Vec3, _: i32) -> collision::Trace {
                collision::Trace::empty(end)
            }
            fn point_contents(&self, _: Vec3, _: i32) -> i32 {
                0
            }
            fn damage_target(&self, _: u64, _: f32, _: Vec3) {
                self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        let external = Arc::new(External(std::sync::atomic::AtomicUsize::new(0)));
        let mut collision = collision::CollisionWorld::new();
        collision.dynamic = Some(external.clone());
        let mut world = World::new(Arc::new(collision));
        let player = world.add_player("Local", Team::CounterTerrorist);
        let (_, _, mut welcome) = fixture(128);
        if let ServerControl::Welcome {
            player: assigned, ..
        } = &mut welcome
        {
            *assigned = player;
        }
        let initial = update(&world, 0);
        assert!(Predictor::new(world, welcome, initial).is_err());
        assert_eq!(external.0.load(std::sync::atomic::Ordering::Relaxed), 0);
    }
}

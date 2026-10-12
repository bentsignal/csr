//! Transport-independent ownership and tick scheduling.
use crate::lag::LagPolicy;
use crate::protocol::*;
use sim::{PlayerId, TickInput, World};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

const MAX_PENDING: usize = 32;
const INPUT_TIMEOUT: f32 = 0.25;

#[derive(Default)]
struct Peer {
    player: PlayerId,
    acknowledged: u64,
    pending: BTreeMap<u64, InputCommand>,
    last_input_tick: u64,
    rtt_seconds: f64,
}

pub struct Session {
    world: World,
    content: ContentManifest,
    peers: HashMap<u64, Peer>,
    departed_events: Vec<sim::GameEvent>,
    lag: LagPolicy,
}

impl Session {
    pub fn new(world: World, content: ContentManifest) -> Result<Self, String> {
        let policy = LagPolicy::prototype(world.tick_rate);
        Self::with_lag_policy(world, content, policy)
    }

    pub fn with_lag_policy(
        mut world: World,
        content: ContentManifest,
        policy: LagPolicy,
    ) -> Result<Self, String> {
        content.validate()?;
        if !sim::TICK_RATE_RANGE.contains(&world.tick_rate) {
            return Err("unsupported tick rate".into());
        }
        let lag = policy.validate(world.tick_rate)?;
        world.configure_hit_history(lag.max_history_ticks, lag.teleport_distance)?;
        Ok(Self {
            world,
            content,
            peers: HashMap::new(),
            departed_events: Vec::new(),
            lag,
        })
    }

    pub fn world(&self) -> &World {
        &self.world
    }
    pub fn content(&self) -> &ContentManifest {
        &self.content
    }
    pub fn lag_policy(&self) -> LagPolicy {
        self.lag
    }
    pub fn player_for(&self, client: u64) -> Option<PlayerId> {
        self.peers.get(&client).map(|p| p.player)
    }
    pub(crate) fn acknowledged_input_for(&self, client: u64) -> Option<u64> {
        self.peers.get(&client).map(|p| p.acknowledged)
    }

    /// Trusted local Begin boundary. The cutoff is read from the exact owned
    /// sender, never supplied by a gameplay packet. Ack/discard old sequences
    /// so retries and packets still in flight cannot preload controls.
    pub(crate) fn discard_owner_inputs(&mut self, client: u64, through: u64) -> Result<(), String> {
        let peer = self.peers.get_mut(&client).ok_or("owner has not joined")?;
        if through > MAX_PROTOCOL_COUNTER || through < peer.acknowledged {
            return Err("invalid local owner input cutoff".into());
        }
        peer.pending.clear();
        peer.acknowledged = through;
        peer.last_input_tick = self.world.tick;
        if let Some(player) = self.world.player_mut(peer.player) {
            player.last_input = TickInput {
                view_angles: player.view_angles,
                ..Default::default()
            };
        }
        Ok(())
    }

    /// Transport-observed latency only, never a client-supplied ping value.
    pub fn observe_rtt(&mut self, client: u64, seconds: f64) {
        if let Some(peer) = self.peers.get_mut(&client) {
            peer.rtt_seconds = if seconds.is_finite() && seconds >= 0. {
                seconds.min(2.)
            } else {
                0.
            };
        }
    }

    pub fn join(&mut self, client: u64, hello: ClientControl) -> ServerControl {
        let reject = |reason: &str| ServerControl::Rejected {
            reason: reason.into(),
        };
        let ClientControl::Hello {
            version,
            name,
            team,
            content,
        } = hello
        else {
            return reject("expected hello");
        };
        if version != VERSION {
            return reject("incompatible protocol version");
        }
        if content != self.content {
            return ServerControl::ContentRequired {
                version: VERSION,
                content: self.content.clone(),
            };
        }
        let name = name.trim();
        if name.is_empty() || name.len() > 64 || name.chars().any(char::is_control) {
            return reject("invalid player name");
        }
        let player = match self.peers.get(&client) {
            Some(p) => p.player,
            None => {
                if self.world.players.len() >= 64 {
                    return reject("server player slots are full");
                }
                let player = self.world.add_player(name, team);
                self.peers.insert(
                    client,
                    Peer {
                        player,
                        last_input_tick: self.world.tick,
                        ..Default::default()
                    },
                );
                player
            }
        };
        ServerControl::Welcome {
            version: VERSION,
            player,
            tick_rate: self.world.tick_rate,
            server_tick: self.world.tick,
            content: self.content.clone(),
        }
    }

    pub fn leave(&mut self, client: u64) {
        if let Some(peer) = self.peers.remove(&client) {
            let before = self.world.events.len();
            self.world.remove_player(peer.player);
            self.departed_events
                .extend(self.world.events[before..].iter().cloned());
        }
    }

    pub fn input(&mut self, client: u64, batch: InputBatch) -> Result<(), String> {
        if batch.0.len() > MAX_INPUT_BATCH {
            return Err("too many input commands".into());
        }
        let peer = self.peers.get_mut(&client).ok_or("input before hello")?;
        // Validate the entire datagram before admitting any command.
        let mut accepted = Vec::new();
        for mut command in batch.0 {
            if command.sequence == 0
                || command.sequence > MAX_PROTOCOL_COUNTER
                || command.view_tick > MAX_PROTOCOL_COUNTER
            {
                return Err("input counters exceed protocol bounds".into());
            }
            if command.sequence <= peer.acknowledged {
                continue;
            }
            if command.sequence - peer.acknowledged > 4096 {
                return Err("input sequence too far ahead".into());
            }
            command.input = validate_input(command.input)?;
            command.input.rewind_tick = self.lag.accepted_tick(
                self.world.tick,
                self.world.oldest_hit_tick(),
                command.view_tick,
                self.world.tick_rate,
                peer.rtt_seconds,
            );
            accepted.push(command);
        }
        for command in accepted {
            peer.pending.entry(command.sequence).or_insert(command);
        }
        // Preserve fresh intent after a server stall without an unbounded queue.
        while peer.pending.len() > MAX_PENDING {
            peer.pending.pop_first();
        }
        Ok(())
    }

    /// Exactly one World::step. A peer cannot accelerate gameplay by flooding.
    pub fn step(&mut self) -> EventBatch {
        let timeout = self.world.ticks(INPUT_TIMEOUT);
        let mut inputs = Vec::with_capacity(self.peers.len());
        for peer in self.peers.values_mut() {
            if let Some((sequence, mut command)) = peer.pending.pop_first() {
                peer.acknowledged = sequence;
                peer.last_input_tick = self.world.tick;
                command.input.rewind_tick = command.input.rewind_tick.and_then(|tick| {
                    self.lag.accepted_tick(
                        self.world.tick,
                        self.world.oldest_hit_tick(),
                        tick,
                        self.world.tick_rate,
                        peer.rtt_seconds,
                    )
                });
                inputs.push((peer.player, command.input));
            } else if self.world.tick.saturating_sub(peer.last_input_tick) >= timeout {
                let angles = self
                    .world
                    .player(peer.player)
                    .map(|p| p.view_angles)
                    .unwrap_or_default();
                inputs.push((
                    peer.player,
                    TickInput {
                        view_angles: angles,
                        ..Default::default()
                    },
                ));
            }
        }
        inputs.sort_unstable_by_key(|(id, _)| *id);
        self.world.step(&inputs);
        let mut events = std::mem::take(&mut self.departed_events);
        events.extend(self.world.events.iter().cloned());
        EventBatch {
            tick: self.world.tick,
            events,
        }
    }

    pub fn snapshot_for(&self, client: u64) -> Option<StateUpdate> {
        let peer = self.peers.get(&client)?;
        let mut snapshot = self.world.snapshot();
        // Events use their reliable channel; snapshots must not replay them.
        snapshot.events.clear();
        Some(StateUpdate {
            acknowledged_input: peer.acknowledged,
            snapshot,
        })
    }
}

pub(crate) fn validate_input(mut input: TickInput) -> Result<TickInput, String> {
    if !input.view_angles.is_finite()
        || !input.forward_move.is_finite()
        || !input.side_move.is_finite()
    {
        return Err("nonfinite input".into());
    }
    input.forward_move = input.forward_move.clamp(-450., 450.);
    input.side_move = input.side_move.clamp(-450., 450.);
    input.view_angles.x = input.view_angles.x.clamp(-89., 89.);
    input.view_angles.y = (input.view_angles.y + 180.).rem_euclid(360.) - 180.;
    input.view_angles.z = 0.;
    input.buttons &= 0x7f;
    if input.weapon_slot > 5 && input.weapon_slot != sim::SLOT_LAST {
        return Err("invalid weapon slot".into());
    }
    if let Some(tracked) = &mut input.tracked {
        for (pose, maximum) in [
            (&mut tracked.head, 96.),
            (&mut tracked.left_hand, 128.),
            (&mut tracked.right_hand, 128.),
        ] {
            if !pose.position.is_finite()
                || !pose.rotation.is_finite()
                || !pose.rotation.length_squared().is_finite()
                || pose.rotation.length_squared() < 0.0001
            {
                return Err("invalid tracked pose".into());
            }
            pose.position = pose.position.clamp_length_max(maximum);
            pose.rotation = pose.rotation.normalize();
        }
    }
    if let Some(aim) = &input.aim {
        if !aim.origin.is_finite() || !aim.direction.is_finite() {
            return Err("invalid aim ray".into());
        }
    }
    Ok(input)
}

/// Wall-clock accumulator. Work is bounded after a stall; sim time never jumps.
pub struct TickClock {
    interval: Duration,
    accumulated: Duration,
    pub discarded_wall_ticks: u64,
}

impl TickClock {
    pub fn new(rate: u32) -> Result<Self, String> {
        if !sim::TICK_RATE_RANGE.contains(&rate) {
            return Err("unsupported tick rate".into());
        }
        Ok(Self {
            interval: Duration::from_secs_f64(1. / rate as f64),
            accumulated: Duration::ZERO,
            discarded_wall_ticks: 0,
        })
    }
    pub fn advance(&mut self, elapsed: Duration) -> usize {
        self.accumulated = self.accumulated.saturating_add(elapsed);
        let due = self.accumulated.as_nanos() / self.interval.as_nanos();
        let remainder = self.accumulated.as_nanos() % self.interval.as_nanos();
        self.accumulated = Duration::from_nanos(remainder as u64);
        self.discarded_wall_ticks = self
            .discarded_wall_ticks
            .saturating_add(due.saturating_sub(8).min(u64::MAX as u128) as u64);
        due.min(8) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::Team;
    use std::sync::Arc;
    fn manifest() -> ContentManifest {
        ContentManifest {
            map_id: "csgo_map_test".into(),
            map_name: "test".into(),
            map_hash: "0".repeat(40),
            catalog_hash: "1".repeat(40),
        }
    }
    fn session() -> Session {
        Session::new(
            World::new(Arc::new(collision::CollisionWorld::new())),
            manifest(),
        )
        .unwrap()
    }
    fn hello() -> ClientControl {
        ClientControl::Hello {
            version: VERSION,
            name: "Player".into(),
            team: Team::CounterTerrorist,
            content: manifest(),
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
    fn redundant_reordered_inputs_are_applied_once_and_only_one_per_tick() {
        let mut s = session();
        assert!(matches!(
            s.join(100, hello()),
            ServerControl::Welcome { .. }
        ));
        s.input(
            100,
            InputBatch(vec![command(3), command(1), command(2), command(1)]),
        )
        .unwrap();
        for sequence in 1..=3 {
            s.step();
            assert_eq!(s.snapshot_for(100).unwrap().acknowledged_input, sequence);
        }
        s.input(100, InputBatch(vec![command(2)])).unwrap();
        s.step();
        assert_eq!(s.snapshot_for(100).unwrap().acknowledged_input, 3);
        assert_eq!(s.world.tick, 4);
    }
    #[test]
    fn content_mismatch_and_unjoined_inputs_never_create_players() {
        let mut s = session();
        let mut h = hello();
        if let ClientControl::Hello { content, .. } = &mut h {
            content.map_hash = "other".into();
        }
        assert!(matches!(
            s.join(10, h),
            ServerControl::ContentRequired { .. }
        ));
        assert!(s.input(10, InputBatch(vec![command(1)])).is_err());
        assert!(s.world.players.is_empty());
    }
    #[test]
    fn malformed_batch_is_atomic_and_each_connection_owns_its_player() {
        let mut s = session();
        s.join(10, hello());
        s.join(20, hello());
        let mut invalid = command(2);
        invalid.input.side_move = f32::NAN;
        assert!(s.input(10, InputBatch(vec![command(1), invalid])).is_err());
        s.input(20, InputBatch(vec![command(1)])).unwrap();
        s.step();
        assert_eq!(s.snapshot_for(10).unwrap().acknowledged_input, 0);
        assert_eq!(s.snapshot_for(20).unwrap().acknowledged_input, 1);
        assert!(s.input(20, InputBatch(vec![command(5000)])).is_err());
        for counter in 0..2 {
            let mut malformed = command(2);
            if counter == 0 {
                malformed.view_tick = u64::MAX;
            } else {
                malformed.sequence = u64::MAX;
            }
            assert!(
                s.input(10, InputBatch(vec![command(1), malformed]))
                    .is_err()
            );
            assert!(s.peers[&10].pending.is_empty());
        }
    }
    #[test]
    fn future_view_ticks_and_invalid_tracking_cannot_poison_the_world() {
        let mut s = session();
        s.join(10, hello());
        let mut future = command(1);
        future.view_tick = 1;
        future.input.rewind_tick = Some(0); // Inner metadata is not trusted.
        s.input(10, InputBatch(vec![future])).unwrap();
        let mut invalid = command(2);
        invalid.input.tracked = Some(sim::TrackedPose {
            head: sim::Pose {
                position: glam::Vec3::ZERO,
                rotation: glam::Quat::from_xyzw(1e30, 1e30, 1e30, 1e30),
            },
            ..Default::default()
        });
        assert!(s.input(10, InputBatch(vec![invalid])).is_err());
        s.step();
        assert_eq!(s.snapshot_for(10).unwrap().acknowledged_input, 1);
        assert_eq!(s.world.players[0].last_input.rewind_tick, None);
        assert!(s.world.players[0].movement.origin.is_finite());
    }
    #[test]
    fn input_backlog_is_bounded_and_a_stall_does_not_grant_extra_ticks() {
        let mut s = session();
        s.join(10, hello());
        for start in (1..=64).step_by(8) {
            s.input(10, InputBatch((start..start + 8).map(command).collect()))
                .unwrap();
        }
        assert_eq!(s.peers[&10].pending.len(), MAX_PENDING);
        s.step();
        assert_eq!(s.world.tick, 1);
        assert_eq!(s.snapshot_for(10).unwrap().acknowledged_input, 33);
    }
    #[test]
    fn stalled_input_stops_movement_and_disconnect_removes_ownership() {
        let mut s = session();
        s.join(10, hello());
        let player = s.player_for(10).unwrap();
        s.input(10, InputBatch(vec![command(1)])).unwrap();
        for _ in 0..40 {
            s.step();
        }
        assert_eq!(s.world.player(player).unwrap().last_input.forward_move, 0.);
        s.leave(10);
        assert!(s.world.player(player).is_none());
        assert!(s.snapshot_for(10).is_none());
    }
    #[test]
    fn clock_preserves_128_ticks_and_caps_stalls_without_jumping_sim_time() {
        let mut clock = TickClock::new(128).unwrap();
        let ticks: usize = (0..1000)
            .map(|_| clock.advance(Duration::from_millis(1)))
            .sum();
        assert_eq!(ticks, 128);
        assert_eq!(clock.advance(Duration::from_secs(1)), 8);
        assert_eq!(clock.discarded_wall_ticks, 120);
        assert_eq!(clock.advance(Duration::ZERO), 0);
    }

    #[test]
    fn old_protocol_cannot_join_or_spawn_a_player() {
        let mut s = session();
        let mut old = hello();
        if let ClientControl::Hello { version, .. } = &mut old {
            *version = 1;
        }
        assert!(matches!(s.join(10, old), ServerControl::Rejected { .. }));
        assert!(s.world.players.is_empty());
    }

    #[test]
    fn rewind_is_revalidated_on_consume_and_never_upgrades_an_invalid_request() {
        let mut s = session();
        s.join(10, hello());
        for _ in 0..16 {
            s.step();
        }
        let mut c = command(1);
        c.view_tick = s.world.tick - 2;
        c.input.rewind_tick = Some(u64::MAX);
        s.input(10, InputBatch(vec![c])).unwrap();
        assert_eq!(s.peers[&10].pending[&1].input.rewind_tick, Some(14));
        // Queued command ages beyond the server allowance before consumption.
        for _ in 0..16 {
            s.world.step(&[]);
        }
        s.step();
        assert_eq!(s.world.players[0].last_input.rewind_tick, None);
        let mut future = command(2);
        future.view_tick = s.world.tick + 1;
        s.input(10, InputBatch(vec![future])).unwrap();
        s.world.step(&[]); // It is now a valid time, but was invalid on receipt.
        s.step();
        assert_eq!(s.world.players[0].last_input.rewind_tick, None);
        assert_eq!(s.snapshot_for(10).unwrap().acknowledged_input, 2);
    }

    #[test]
    fn only_valid_outer_view_metadata_can_produce_an_authoritative_historical_hit() {
        let catalog = Arc::new(
            weapons::Catalog::from_items_game(
                r##"
            "items_game" { "items" { "1" {
                "name" "weapon_test" "baseitem" "1"
                "item_type_name" "#CSGO_Type_Rifle"
                "attributes" { "damage" "35" "range" "8192"
                    "primary clip size" "30" "is full auto" "1" "cycletime" "0.1" }
            } } }
        "##,
            )
            .unwrap(),
        );
        for valid in [true, false] {
            let world =
                World::with_catalog(Arc::new(collision::CollisionWorld::new()), catalog.clone());
            let mut s = Session::new(world, manifest()).unwrap();
            s.join(10, hello());
            let mut target_hello = hello();
            if let ClientControl::Hello { team, .. } = &mut target_hello {
                *team = Team::Terrorist;
            }
            s.join(20, target_hello);
            let shooter = s.player_for(10).unwrap();
            let victim = s.player_for(20).unwrap();
            s.world.players[0].movement = movement::PlayerMove::spawn_at(glam::Vec3::ZERO);
            s.world.players[0].weapons = vec![sim::weapon::WeaponState::new(Arc::new(
                catalog.get("weapon_test").unwrap().clone(),
            ))];
            s.world.players[0].active = 0;
            s.world.players[1].movement = movement::PlayerMove::spawn_at(glam::Vec3::X * 100.);
            s.world.players[1].armor = 0;
            s.step(); // Tick zero target pose is now in server history.
            s.world.players[1].movement.origin.y = 64.;
            let input = InputCommand {
                sequence: 1,
                view_tick: if valid { 0 } else { 100 },
                input: TickInput {
                    buttons: movement::buttons::ATTACK,
                    rewind_tick: Some(0),
                    ..Default::default()
                },
            };
            s.input(10, InputBatch(vec![input])).unwrap();
            let events = s.step();
            assert_eq!(
                events
                    .events
                    .iter()
                    .any(|e| matches!(e, sim::GameEvent::Damage {
                attacker, victim: hit, .. } if *attacker == shooter && *hit == victim)),
                valid
            );
            assert_eq!(s.world.player(victim).unwrap().health < 100, valid);
            assert_eq!(s.world.player(victim).unwrap().movement.origin.y, 64.);
            assert_eq!(
                s.world.player(shooter).unwrap().last_input.rewind_tick,
                valid.then_some(0)
            );
        }
    }
}

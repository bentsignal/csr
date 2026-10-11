//! Transport-independent ownership and tick scheduling.
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
}

pub struct Session {
    world: World,
    content: ContentManifest,
    peers: HashMap<u64, Peer>,
    departed_events: Vec<sim::GameEvent>,
}

impl Session {
    pub fn new(world: World, content: ContentManifest) -> Result<Self, String> {
        let id: content::ContentId = content
            .map_id
            .parse()
            .map_err(|e| format!("invalid map content id: {e}"))?;
        if id.kind != content::ContentKind::Map {
            return Err("content id is not a map".into());
        }
        if !sim::TICK_RATE_RANGE.contains(&world.tick_rate) {
            return Err("unsupported tick rate".into());
        }
        Ok(Self {
            world,
            content,
            peers: HashMap::new(),
            departed_events: Vec::new(),
        })
    }

    pub fn world(&self) -> &World {
        &self.world
    }
    pub fn content(&self) -> &ContentManifest {
        &self.content
    }
    pub fn player_for(&self, client: u64) -> Option<PlayerId> {
        self.peers.get(&client).map(|p| p.player)
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
            return reject("map or weapon content differs from the server");
        }
        let name = name.trim();
        if name.is_empty() || name.len() > 64 || name.chars().any(char::is_control) {
            return reject("invalid player name");
        }
        let player = match self.peers.get(&client) {
            Some(p) => p.player,
            None => {
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
            if command.sequence == 0 {
                return Err("input sequence starts at one".into());
            }
            if command.sequence <= peer.acknowledged {
                continue;
            }
            if command.sequence - peer.acknowledged > 4096 {
                return Err("input sequence too far ahead".into());
            }
            if command.view_tick > self.world.tick {
                return Err("view tick is in the future".into());
            }
            command.input = validate_input(command.input)?;
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
            if let Some((sequence, command)) = peer.pending.pop_first() {
                peer.acknowledged = sequence;
                peer.last_input_tick = self.world.tick;
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

fn validate_input(mut input: TickInput) -> Result<TickInput, String> {
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
            map_hash: "map".into(),
            catalog_hash: "catalog".into(),
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
        assert!(matches!(s.join(10, h), ServerControl::Rejected { .. }));
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
}

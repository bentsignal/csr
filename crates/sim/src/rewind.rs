//! Server-only historical player hulls. No live entity is moved for a query.
use crate::{Player, PlayerId, Team, World};
use glam::Vec3;
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy)]
pub(crate) struct Hull {
    pub mins: Vec3,
    pub maxs: Vec3,
    pub yaw: f32,
}
impl Hull {
    fn current(player: &Player) -> Self {
        let (mins, maxs) = player.bounds();
        Self {
            mins,
            maxs,
            yaw: player.view_angles.y,
        }
    }
}

#[derive(Clone)]
struct Record {
    hull: Hull,
    origin: Vec3,
    life: u64,
    team: Team,
    alive: bool,
    continuity: u64,
}
#[derive(Clone)]
struct Frame {
    tick: u64,
    players: BTreeMap<PlayerId, Record>,
}
#[derive(Clone)]
pub(crate) struct History {
    ticks: u32,
    teleport_squared: f32,
    round: Option<u32>,
    frames: VecDeque<Frame>,
}
impl History {
    pub fn new(rate: u32, ticks: u32, teleport_distance: f32) -> Result<Self, String> {
        if !crate::TICK_RATE_RANGE.contains(&rate)
            || ticks == 0
            || ticks > rate
            || !teleport_distance.is_finite()
            || teleport_distance <= 0.
            || teleport_distance > 1e6
        {
            return Err("invalid server hit history bounds".into());
        }
        Ok(Self {
            ticks,
            teleport_squared: teleport_distance * teleport_distance,
            round: None,
            frames: VecDeque::new(),
        })
    }
    pub fn clear(&mut self) {
        self.frames.clear();
        self.round = None;
    }
    pub fn oldest_tick(&self) -> Option<u64> {
        self.frames.front().map(|f| f.tick)
    }
    pub fn capture(&mut self, tick: u64, round: u32, players: &[Player]) {
        if self.round != Some(round) || self.frames.back().is_some_and(|f| tick < f.tick) {
            self.clear();
        }
        self.round = Some(round);
        // Refresh same-tick admission changes rather than duplicate a frame.
        if self.frames.back().is_some_and(|f| f.tick == tick) {
            self.frames.pop_back();
        }
        let previous = self.frames.back();
        let mut records = BTreeMap::new();
        for player in players.iter().take(64) {
            let hull = Hull::current(player);
            if !hull.mins.is_finite() || !hull.maxs.is_finite() || !hull.yaw.is_finite() {
                continue;
            }
            let prior = previous.and_then(|f| f.players.get(&player.id));
            let continuous = prior.is_some_and(|old| {
                old.life == player.life_epoch
                    && old.team == player.team
                    && old.alive == player.alive
                    && old.origin.distance_squared(player.movement.origin) <= self.teleport_squared
            });
            let continuity = prior
                .map_or(0, |old| old.continuity)
                .saturating_add(u64::from(!continuous));
            records.insert(
                player.id,
                Record {
                    hull,
                    origin: player.movement.origin,
                    life: player.life_epoch,
                    team: player.team,
                    alive: player.alive,
                    continuity,
                },
            );
        }
        self.frames.push_back(Frame {
            tick,
            players: records,
        });
        while self
            .frames
            .front()
            .is_some_and(|f| tick.saturating_sub(f.tick) > self.ticks as u64)
            || self.frames.len() > self.ticks as usize + 1
        {
            self.frames.pop_front();
        }
    }
    fn query(&self, player: &Player, round: u32, tick: u64) -> Option<Hull> {
        if self.round != Some(round) || !player.alive || player.life_epoch == u64::MAX {
            return None;
        }
        let latest = self.frames.back()?.players.get(&player.id)?;
        let offset = tick.checked_sub(self.frames.front()?.tick)?;
        let frame = usize::try_from(offset)
            .ok()
            .and_then(|offset| self.frames.get(offset))
            .filter(|frame| frame.tick == tick)
            .or_else(|| self.frames.iter().find(|frame| frame.tick == tick))?;
        let old = frame.players.get(&player.id)?;
        if !old.alive
            || !latest.alive
            || old.life != player.life_epoch
            || latest.life != player.life_epoch
            || old.team != player.team
            || latest.team != player.team
            || old.continuity != latest.continuity
            || latest.continuity == u64::MAX
            || latest.origin.distance_squared(player.movement.origin) > self.teleport_squared
        {
            return None;
        }
        Some(old.hull)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn server_history_is_bounded_and_restores_or_rounds_invalidate_it() {
        for rate in [64, 128] {
            let mut w = World::new(Arc::new(collision::CollisionWorld::new()));
            w.set_tick_rate(rate);
            for n in 0..70 {
                w.add_player(&n.to_string(), Team::CounterTerrorist);
            }
            w.configure_hit_history(rate, 128.).unwrap();
            for _ in 0..rate * 2 {
                w.step(&[]);
            }
            let history = w.rewind.as_ref().unwrap();
            assert_eq!(history.frames.len(), rate as usize + 1);
            assert!(history.frames.iter().all(|f| f.players.len() == 64));
            w.round.round += 1;
            w.step(&[]);
            assert_eq!(w.rewind.as_ref().unwrap().frames.len(), 1);
            let state = w.state();
            w.restore(state);
            assert!(w.oldest_hit_tick().is_none());
            w.set_tick_rate(rate);
            assert!(w.rewind.is_none());
        }
    }
}

impl World {
    /// Enable an explicit server-only history of at most one server second.
    /// Teleport distance is policy in Source units; no collision worlds clone.
    pub fn configure_hit_history(
        &mut self,
        ticks: u32,
        teleport_distance: f32,
    ) -> Result<(), String> {
        self.rewind = Some(History::new(self.tick_rate, ticks, teleport_distance)?);
        Ok(())
    }
    pub fn oldest_hit_tick(&self) -> Option<u64> {
        self.rewind.as_ref().and_then(History::oldest_tick)
    }
    pub(crate) fn target_hull(&self, player: &Player, tick: Option<u64>) -> Hull {
        tick.and_then(|tick| self.rewind.as_ref()?.query(player, self.round.round, tick))
            .unwrap_or_else(|| Hull::current(player))
    }
}

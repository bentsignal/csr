//! Live player hulls participate in the same movement sweeps as map geometry.
//! They are transient query inputs, never inserted into the map/bullet backend.
use crate::{Player, PlayerId, Team};
use collision::{contents, CollisionWorld, Trace};
use glam::Vec3;
use movement::{PlayerMove, TraceWorld};

#[derive(Clone, Copy)]
struct Body { id: PlayerId, team: Team, low: Vec3, high: Vec3, velocity: Vec3 }

pub(crate) struct PlayerWorld<'a> {
    map: &'a CollisionWorld,
    bodies: Vec<Body>,
    team: Team,
    solid_teammates: u8,
}

impl<'a> PlayerWorld<'a> {
    pub(crate) fn new(map: &'a CollisionWorld, players: &[Player], id: PlayerId, solid_teammates: u8) -> Self {
        let team = players.iter().find(|p| p.id == id).map(|p| p.team).unwrap_or(Team::Spectator);
        let bodies = players.iter().filter(|p| p.id != id && p.alive && p.team != Team::Spectator)
            .map(|p| { let (low, high) = p.bounds(); Body { id: p.id, team: p.team, low, high, velocity: p.movement.velocity } }).collect();
        Self { map, bodies, team, solid_teammates }
    }

    fn enabled(&self, body: &Body) -> bool { body.team != self.team || self.solid_teammates != 0 }

    fn support(&self, pm: &PlayerMove) -> Option<&Body> {
        if pm.velocity.z > movement::NON_JUMP_VELOCITY { return None; }
        let (mins, maxs) = pm.hull();
        self.bodies.iter().filter(|b| self.enabled(b)
            && pm.origin.z >= b.high.z - collision::DIST_EPSILON
            && pm.origin.z <= b.high.z + 2.1
            && pm.origin.x + maxs.x > b.low.x && pm.origin.x + mins.x < b.high.x
            && pm.origin.y + maxs.y > b.low.y && pm.origin.y + mins.y < b.high.y)
            .min_by(|a,b| (pm.origin.z-a.high.z).total_cmp(&(pm.origin.z-b.high.z)).then(a.id.cmp(&b.id)))
    }

    /// Translate supported feet with their moving base. Momentum is inherited
    /// only on departure, after the ordinary jump cap, not added every tick.
    pub(crate) fn move_player(&self, pm: &mut PlayerMove, input: &movement::MoveInput,
        config: &movement::MoveConfig, dt: f32) -> movement::MoveEvents {
        let support = self.support(pm).copied();
        if let Some(base) = support {
            let mut carry = self.map.trace_box(pm.origin, pm.origin + base.velocity * dt,
                pm.hull().0, pm.hull().1, contents::MASK_PLAYERSOLID);
            self.clip_players(pm.origin, pm.origin + base.velocity * dt, pm.hull().0, pm.hull().1,
                contents::MASK_PLAYERSOLID, Some(base.id), &mut carry);
            if !carry.start_solid && !carry.all_solid { pm.origin = carry.end; }
        }
        let events = movement::player_move(pm, input, config, self, dt);
        if let Some(base) = support {
            if !pm.on_ground { pm.velocity = (pm.velocity + base.velocity).clamp(Vec3::splat(-config.max_velocity), Vec3::splat(config.max_velocity)); }
        }
        events
    }

    fn clip_players(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3,
        mask: i32, ignore: Option<PlayerId>, result: &mut Trace) {
        if mask & contents::MONSTER == 0 { return; }
        for body in &self.bodies {
            if Some(body.id) == ignore || !self.enabled(body) { continue; }
            // Mode 2 permits side passage but retains a supporting top face.
            if body.team == self.team && self.solid_teammates == 2
                && (start.z + mins.z < body.high.z - collision::DIST_EPSILON || end.z > start.z) { continue; }
            let low = start + mins; let high = start + maxs;
            // Simultaneous spawns/restoration may overlap. Permit separation
            // from that initial overlap instead of permanently trapping both.
            if low.cmplt(body.high - Vec3::splat(0.001)).all()
                && high.cmpgt(body.low + Vec3::splat(0.001)).all() { continue; }
            let mut hit = Trace::empty(end);
            collision::clip_bounds(start, end, mins, maxs, body.low, body.high,
                contents::MONSTER, 0, &mut hit);
            if hit.fraction < result.fraction || hit.all_solid { *result = hit; }
        }
    }
}

impl TraceWorld for PlayerWorld<'_> {
    fn trace_hull(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32) -> Trace {
        let mut hit = self.map.trace_box(start, end, mins, maxs, mask);
        self.clip_players(start, end, mins, maxs, mask, None, &mut hit);
        hit
    }
    fn trace_player(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32, drop_scaffold: bool) -> Trace {
        let mut hit = self.map.trace_player(start, end, mins, maxs, mask, drop_scaffold);
        self.clip_players(start, end, mins, maxs, mask, None, &mut hit);
        hit
    }
    fn ladder_normal(&self, p: Vec3, mins: Vec3, maxs: Vec3) -> Option<Vec3> { self.map.ladder_normal(p, mins, maxs) }
    fn voxel_scale(&self) -> f32 { TraceWorld::voxel_scale(self.map) }
    fn climbable(&self, p: Vec3, mins: Vec3, maxs: Vec3) -> collision::Climbable { TraceWorld::climbable(self.map, p, mins, maxs) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{World, SpawnPoint, TickInput};
    use std::sync::Arc;
    fn pair(rate: u32, same_team: bool) -> (World, PlayerId, PlayerId) {
        let mut map = CollisionWorld::new();
        for triangle in [[Vec3::new(-4096.,-4096.,0.),Vec3::new(4096.,-4096.,0.),Vec3::new(4096.,4096.,0.)],
            [Vec3::new(-4096.,-4096.,0.),Vec3::new(4096.,4096.,0.),Vec3::new(-4096.,4096.,0.)]] {
            map.add_triangle(triangle, contents::SOLID, 0);
        }
        map.finalize();
        let mut world = World::new(Arc::new(map)); world.set_tick_rate(rate);
        world.rules.solid_teammates = 1;
        world.spawns = vec![SpawnPoint { origin: Vec3::new(-100.,0.,1.), yaw:0., team:Team::CounterTerrorist },
            SpawnPoint { origin:Vec3::new(0.,0.,1.), yaw:0., team:Team::Terrorist }];
        let a = world.add_player("runner", Team::CounterTerrorist);
        let b = world.add_player("base", if same_team {Team::CounterTerrorist} else {Team::Terrorist});
        world.player_mut(a).unwrap().movement = PlayerMove::spawn_at(Vec3::new(-100.,0.,1.));
        world.player_mut(b).unwrap().movement = PlayerMove::spawn_at(Vec3::new(0.,0.,1.));
        for p in &mut world.players { p.weapons.clear(); }
        (world,a,b)
    }
    #[test]
    fn live_player_contact_blocks_sides_but_dead_and_disabled_teammates_do_not() {
        for rate in [64,128] { for same in [false,true] {
            for (mode, dead, blocked) in [(1,false,true),(0,false,!same),(2,false,!same),(1,true,false)] {
                let (mut world,a,b) = pair(rate,same); world.rules.solid_teammates=mode;
                if dead { let p=world.player_mut(b).unwrap(); p.alive=false; p.respawn_at=10.; }
                for _ in 0..rate { world.step(&[(a,TickInput { forward_move:450., ..Default::default() }), (b,TickInput::default())]); }
                let x = world.player(a).unwrap().movement.origin.x;
                if blocked { assert!((x+32.).abs()<0.1,"rate={rate} same={same} mode={mode}: {x}"); }
                else { assert!(x>20.,"rate={rate} same={same} mode={mode}: {x}"); }
            }
        }}
    }
    #[test]
    fn crouched_player_support_carries_and_jump_inherits_base_motion_once_with_replay() {
        for rate in [64,128] {
            let (mut world,a,b)=pair(rate,true);
            let mut input=TickInput { buttons:movement::buttons::DUCK, ..Default::default() };
            for _ in 0..rate/2 { world.step(&[(b,input),(a,TickInput::default())]); }
            let top=world.player(b).unwrap().bounds().1.z;
            world.player_mut(a).unwrap().movement=PlayerMove::spawn_at(Vec3::new(0.,0.,top+0.05));
            for _ in 0..rate/4 { world.step(&[(b,input),(a,TickInput::default())]); }
            assert!(world.player(a).unwrap().movement.on_ground);
            assert!((world.player(a).unwrap().movement.origin.z-top).abs()<0.1);
            input.forward_move=450.;
            for _ in 0..rate/2 { world.step(&[(b,input),(a,TickInput::default())]); }
            let rider=world.player(a).unwrap().movement;
            let base=world.player(b).unwrap().movement;
            assert!(rider.origin.x>10. && (rider.origin.x-base.origin.x).abs()<4.,"rate={rate} rider={rider:?} base={base:?}");
            let snapshot=world.snapshot(); let mut replay=world.clone(); replay.restore(snapshot.state);
            let commands=[(b,input),(a,TickInput {buttons:movement::buttons::JUMP,..Default::default()})];
            world.step(&commands); replay.step_prediction(&commands);
            assert_eq!(world.state(),replay.state());
            let jumped=world.player(a).unwrap().movement;
            assert!(!jumped.on_ground && jumped.velocity.x>base.velocity.x*0.9);
            let initial=jumped.velocity.x;
            for _ in 0..8 { world.step(&[(b,input),(a,TickInput::default())]); }
            assert!((world.player(a).unwrap().movement.velocity.x-initial).abs()<0.1,"base velocity must not accumulate in air");
        }
    }
}

//! Tick-authoritative pin/hold/release playback using installed timing metadata.
use super::*;
use movement::buttons;

#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub enum ThrowMode { Overhand,Underhand,Combined }
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub enum ThrowPhase { PullingPin,Holding,Throwing }
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub struct ThrowState {
    pub phase: ThrowPhase,
    pub mode: ThrowMode,
    pub pull_started_tick: u64,
    pub throw_started_tick: Option<u64>,
    pub release_requested: bool,
    pub spawned: bool,
}
impl ThrowMode {
    fn buttons(bits:u32)->Option<Self> {
        match (bits&buttons::ATTACK!=0,bits&buttons::ATTACK2!=0) {
            (true,false)=>Some(Self::Overhand),(false,true)=>Some(Self::Underhand),
            (true,true)=>Some(Self::Combined),_=>None,
        }
    }
    pub fn activity(self)->&'static str {
        if self==Self::Underhand {"ACT_VM_RELEASE"}else{"ACT_VM_THROW"}
    }
    fn animation(self,profile:&weapons::GrenadeAnimations)->&weapons::GrenadeReleaseAnimation {
        if self==Self::Underhand {&profile.underhand}else{&profile.overhand}
    }
}
fn ticks(seconds:f32,dt:f32)->u64 {(seconds/dt).ceil() as u64}

impl World {
    pub(crate) fn step_grenade(&mut self,i:usize,bits:u32) {
        let tick=self.tick;let dt=self.dt();let time=self.time();
        let active=self.players[i].active;
        let Some(w)=self.players[i].weapons.get_mut(active) else {return};
        let Some(profile)=w.def.grenade_animations.as_ref() else {return};
        let mode=ThrowMode::buttons(bits);
        if w.grenade_throw.is_none() {
            if time<w.next_attack {return;}
            let Some(mode)=mode else {return};
            w.grenade_throw=Some(ThrowState {phase:ThrowPhase::PullingPin,mode,pull_started_tick:tick,
                throw_started_tick:None,release_requested:false,spawned:false});
        }
        let state=w.grenade_throw.as_mut().unwrap();
        if state.phase!=ThrowPhase::Throwing {
            if let Some(mode)=mode {state.mode=mode;}else{state.release_requested=true;}
            if tick>=state.pull_started_tick.saturating_add(ticks(profile.pull_pin_duration,dt)) {
                state.phase=ThrowPhase::Holding;
                if state.release_requested {
                    state.phase=ThrowPhase::Throwing;state.throw_started_tick=Some(tick);
                }
            }
        }
        if state.phase!=ThrowPhase::Throwing {return;}
        let animation=state.mode.animation(profile);let started=state.throw_started_tick.unwrap();
        let spawn=!state.spawned && tick>=started.saturating_add(ticks(animation.release_time,dt));
        let finish=state.spawned && tick>=started.saturating_add(ticks(animation.duration,dt));
        if spawn {
            if self.spawn_grenade(i) {
                self.players[i].active_weapon_mut().unwrap().grenade_throw.as_mut().unwrap().spawned=true;
            } else {
                // Existing start-solid launch qualification fails without
                // consuming inventory or manufacturing a projectile.
                self.players[i].active_weapon_mut().unwrap().grenade_throw=None;
            }
        } else if finish {
            let p=&mut self.players[i];
            if self.mode==crate::GameMode::Deathmatch && self.rules.grenade_practice && !p.is_bot {
                // Replenish only after the authored release clip, so another
                // throw begins from Draw instead of retaining the old hand pose.
                let w=p.active_weapon_mut().unwrap();
                w.grenade_throw=None;w.next_attack=time+0.5;
            }else{let index=p.active;p.take(index,time);}
        }
    }
}

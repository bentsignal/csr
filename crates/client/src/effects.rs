//! Turns simulation events into feedback: tracers, impact marks, sounds,
//! the kill feed, a hit marker, round announcements, the bomb's beeping
//! and its explosion.

use crate::audio::{Emit, SoundLibrary};
use crate::coords::to_bevy;
use crate::player::{LocalPlayer, SimWorld};
use crate::LoadedLevel;
use bevy::audio::AudioSource;
use bevy::prelude::*;
use sim::GameEvent;

/// Events produced by fixed ticks since the last frame.
#[derive(Resource, Default)]
pub struct PendingEvents(pub Vec<GameEvent>);

#[derive(Clone, Debug)]
pub struct KillFeedEntry {
    pub text: String,
    pub local_involved: bool,
    pub time: f32,
    /// Immutable authority, retained inside the existing bounded feed rather
    /// than a separate unconsumed corpse queue. No physical impulse is implied.
    pub death: sim::death::DeathReceipt,
}

impl KillFeedEntry {
    fn new(attacker: sim::PlayerId, weapon: &str, headshot: bool,
        death: sim::death::DeathReceipt, local: sim::PlayerId, time: f32) -> Self {
        let gun = if weapon == "planted_c4" { "bomb" } else { weapon.trim_start_matches("weapon_") };
        let text = match &death.attacker {
            Some(actor) => format!("{}  [{}{}]  {}", actor.name, gun,
                if headshot { " HS" } else { "" }, death.victim_name),
            None => format!("[{gun}]  {}", death.victim_name),
        };
        Self { text, local_involved: attacker == local || death.key.victim == local, time, death }
    }
}

#[derive(Resource, Default)]
pub struct Effects {
    /// Presentation consumes this every frame, before kill-feed trimming can
    /// discard simultaneous deaths. No receipt becomes simulation state.
    pub(crate) deaths: Vec<sim::death::DeathReceipt>,
    pub feedback: Vec<GameEvent>,
    pub(crate) utility_detonations: Vec<(sim::utility::Kind, glam::Vec3)>,
    pub kill_feed: Vec<KillFeedEntry>,
    /// Time of the local player's last hit on someone (hit marker).
    pub last_hit: f32,
    /// Last time each player fired (drives fire animations).
    pub last_fire: std::collections::HashMap<sim::PlayerId, f32>,
    pub attack_serial: u64,
    attacks: std::collections::HashMap<sim::PlayerId, (u64, String, &'static str)>,
    pub last_hit_headshot: bool,
    /// Announcer lines waiting to play (time, soundscript entry).
    queued: Vec<(f32, String)>,
    last_beep: f32,
    explosions: Vec<(Vec3, f32)>,
    /// Banner for the most recent round result (text, colour, time).
    pub banner: Option<(String, Color, f32)>,
}

impl Effects {
    fn attack(&mut self, player: sim::PlayerId, weapon: String, activity: &'static str, now: f32) {
        self.attack_serial += 1;
        self.last_fire.insert(player, now);
        self.attacks.insert(player, (self.attack_serial, weapon, activity));
    }

    /// Only consume attacks produced after deployment, for this held weapon.
    pub fn consume_attack(&self, player: sim::PlayerId, weapon: &str, seen: &mut u64) -> Option<&'static str> {
        let (serial, fired_weapon, activity) = self.attacks.get(&player)?;
        let fresh = *serial > *seen && fired_weapon == weapon;
        *seen = (*seen).max(*serial);
        fresh.then_some(*activity)
    }
}

const EXPLOSION_LIFE: f32 = 1.2;

#[allow(clippy::too_many_arguments)]
pub fn process(
    mut commands: Commands,
    mut pending: ResMut<PendingEvents>,
    mut effects: ResMut<Effects>,
    mut tracers: ResMut<crate::tracers::Tracers>,
    mut sounds: ResMut<SoundLibrary>,
    mut sources: ResMut<Assets<AudioSource>>,
    world: Res<SimWorld>,
    local: Res<LocalPlayer>,
    level: Res<LoadedLevel>,
    time: Res<Time>,
    props: Res<crate::surfaces::SurfaceProps>,
) {
    let now = time.elapsed_secs();
    // Own sounds play at the listener; everyone else's in the world.
    let emit = |player: sim::PlayerId, at: glam::Vec3| {
        let own = player == local.view_target() && !local.noclip;
        Emit { at: (!own).then_some(at), owner: Some(player), local: own }
    };
    let fs = &level.0.vfs;
    for event in pending.0.drain(..) {
        if matches!(event, GameEvent::Damage {..} | GameEvent::Impact {..} | GameEvent::RoundStart {..}) { effects.feedback.push(event.clone()); }
        match event {
            GameEvent::Fire { player, weapon, origin } => {
                effects.attack(player, weapon.clone(), "ACT_VM_PRIMARYATTACK", now);
                let sound = world.0.catalog.get(&weapon).map(|d| sounds.weapon_shot_entry(d).to_string()).unwrap_or_default();
                if !sound.is_empty() {
                    sounds.play_with(&mut commands, fs, &mut sources, &sound, emit(player, origin));
                }
            }
            GameEvent::DecoyShot {pos,weapon,..} => {
                if let Some(def) = weapon.parse().ok().and_then(|id| world.0.catalog.by_id(&id)) {
                    sounds.play_decoy(&mut commands,fs,&mut sources,def,pos);
                }
            }
            GameEvent::UtilityThrown { player,kind,pos } => {
                // The installed grenade view models expose ACT_VM_THROW.
                effects.attack(player, kind.name().to_string(), "ACT_VM_THROW", now);
                let sound=match kind {sim::utility::Kind::Molotov=>"Molotov.Throw",sim::utility::Kind::Incendiary=>"IncGrenade.Throw",_=>"HEGrenade.Throw"};
                sounds.play(&mut commands,fs,&mut sources,sound,if player==local.view_target(){None}else{Some(pos)});
            }
            GameEvent::DecoyExpired { pos, .. } => {
                // Audio only: no inferred HE damage or detonation particles.
                sounds.play_decoy_expiry(&mut commands, fs, &mut sources, pos);
            }
            GameEvent::UtilityBounced { kind, pos, .. } => {
                sounds.play_grenade_bounce(&mut commands, fs, &mut sources, kind, pos);
            }
            GameEvent::UtilityDetonated { kind,pos,.. } => {
                if effects.utility_detonations.len()<128 {effects.utility_detonations.push((kind,pos));}
                let sound=match kind {sim::utility::Kind::Flash=>Some("Flashbang.Explode"),sim::utility::Kind::He=>Some("BaseGrenade.Explode"),
                    sim::utility::Kind::Molotov|sim::utility::Kind::Incendiary=>Some("Inferno.Start_IncGrenade"),_=>None};
                // A smoke/decoy activation is not a shell collision. Contact
                // cues come only from the authoritative bounce event above.
                if let Some(sound)=sound {sounds.play(&mut commands,fs,&mut sources,sound,Some(pos));}
            }
            GameEvent::Melee { player, weapon, origin, secondary, hit } => {
                let activity = match (secondary, hit == sim::MeleeHit::Miss) {
                    (true, true) => "ACT_VM_MISSCENTER2", (true, false) => "ACT_VM_HITCENTER2",
                    (false, true) => "ACT_VM_MISSCENTER", (false, false) => "ACT_VM_HITCENTER",
                };
                effects.attack(player, weapon, activity, now);
                let sound = match hit {
                    sim::MeleeHit::Miss => "Weapon_Knife.Slash",
                    sim::MeleeHit::World => "Weapon_Knife.HitWall",
                    sim::MeleeHit::Player if secondary => "Weapon_Knife.Stab",
                    sim::MeleeHit::Player => "Weapon_Knife.Hit",
                };
                sounds.play_with(&mut commands, fs, &mut sources, sound, emit(player, origin));
            }
            GameEvent::Scope { player, weapon, origin, level } => {
                if let Some(def) = world.0.catalog.get(&weapon) {
                    let sound = if level == 0 { &def.zoom_out_sound } else { &def.zoom_in_sound };
                    if !sound.is_empty() { sounds.play_with(&mut commands, fs, &mut sources, sound, emit(player, origin)); }
                }
            }
            GameEvent::Tracer { player, start, end } => {
                tracers.pending.push((player,start,end));
            }
            GameEvent::Impact { pos, surface, .. } => {
                if !level.0.collision.is_sky(surface) {
                    // The surface property's bullet impact (via $surfaceprop).
                    let name = level.0.collision.surface_name(surface);
                    let sound = props.sounds(fs, name).and_then(|s| s.bullet_impact.clone()).filter(|s| sounds.has(s));
                    let sound = sound.unwrap_or_else(|| crate::feedback::impact_sound(name).to_string());
                    sounds.play(&mut commands, fs, &mut sources, &sound, Some(pos));
                }

            }
            GameEvent::Damage { attacker, victim, group, pos, .. } => {
                let target = world.0.player(victim);
                let sound = match group {
                    sim::weapon::HitGroup::Head if target.is_some_and(|p|p.helmet) => "Player.DamageHelmet",
                    sim::weapon::HitGroup::Head => "Player.DamageHeadShot",
                    _ if target.is_some_and(|p|p.armor>0) => "Player.DamageKevlar",
                    _ => "Flesh.BulletImpact",
                };
                sounds.play_with(&mut commands, fs, &mut sources, sound, emit(victim, pos));
                if attacker == local.view_target() {
                    effects.last_hit = now;
                    effects.last_hit_headshot = group == sim::weapon::HitGroup::Head;
                }
            }
            GameEvent::Kill { attacker, weapon, headshot, death, .. } => {
                if effects.deaths.len() < 128 { effects.deaths.push(death.clone()); }
                effects.kill_feed.push(KillFeedEntry::new(attacker, &weapon, headshot, death, local.id, now));
                if effects.kill_feed.len() > 6 {
                    effects.kill_feed.remove(0);
                }
            }
            GameEvent::Purchase { player, .. } if player == local.id => {
                sounds.play(&mut commands, fs, &mut sources, "uipanorama.buymenu_purchase", None);
            }
            GameEvent::Jump { player } => {
                if let Some(p) = world.0.player(player) {
                    crate::foley::jump(&mut commands, &mut sounds, &mut sources, &level.0, p, &local);
                }
            }
            GameEvent::Land { player, speed } => {
                if let Some(p) = world.0.player(player) {
                    crate::foley::land(&mut commands, &mut sounds, &mut sources, &level.0, &props, p, &local, speed);
                }
            }
            GameEvent::Pickup { player, item } => {
                let at = if player == local.id { None } else { world.0.player(player).map(|p| p.movement.origin) };
                let sound = if item == sim::weapon::C4 { "c4.draw" } else { "player.pickupweapon" };
                sounds.play(&mut commands, fs, &mut sources, sound, at);
            }
            GameEvent::PlantStart { player } => {
                let at = world.0.player(player).map(|p| p.movement.origin);
                sounds.play(&mut commands, fs, &mut sources, "c4.initiate", at);
            }
            GameEvent::BombPlanted { pos, .. } => {
                sounds.play(&mut commands, fs, &mut sources, "c4.plant", Some(pos));
                effects.queued.push((now + 0.6, "event.bombplanted".into()));
                effects.last_beep = now;
            }
            GameEvent::DefuseStart { player, .. } => {
                let at = world.0.player(player).map(|p| p.movement.origin);
                sounds.play(&mut commands, fs, &mut sources, "c4.disarmstart", at);
            }
            GameEvent::BombDefused { player } => {
                let at = if player == local.id { None } else { world.0.player(player).map(|p| p.movement.origin) };
                sounds.play(&mut commands, fs, &mut sources, "c4.disarmfinish", at);
                effects.queued.push((now + 0.3, "event.bombdefused".into()));
            }
            GameEvent::BombExploded { pos } => {
                sounds.play(&mut commands, fs, &mut sources, "c4.explode", Some(pos));
                effects.explosions.push((to_bevy(pos), now));
            }
            GameEvent::RoundEnd { reason } => {
                let (line, color) = match reason.winner() {
                    Some(sim::Team::Terrorist) => ("event.terwin", Color::srgb(0.95, 0.75, 0.3)),
                    Some(_) => ("event.ctwin", Color::srgb(0.45, 0.65, 1.0)),
                    None => ("event.rounddraw", Color::WHITE),
                };
                // After "bomb has been defused" when there is one.
                let delay = if reason == sim::RoundEndReason::BombDefused { 2.2 } else { 0.4 };
                effects.queued.push((now + delay, line.into()));
                let winner = match reason.winner() {
                    Some(sim::Team::Terrorist) => "Terrorists Win",
                    Some(_) => "Counter-Terrorists Win",
                    None => "Round Draw",
                };
                let text = match reason {
                    sim::RoundEndReason::TerroristsEliminated | sim::RoundEndReason::CounterTerroristsEliminated | sim::RoundEndReason::Draw => winner.to_string(),
                    other => format!("{winner}\n{}", other.message()),
                };
                effects.banner = Some((text, color, now));
            }
            GameEvent::MatchEnd { winner } => {
                let text = match winner {
                    Some(sim::Team::Terrorist) => "Terrorists win the match",
                    Some(_) => "Counter-Terrorists win the match",
                    None => "The match is a draw",
                };
                effects.banner = Some((text.into(), Color::WHITE, now));
            }
            GameEvent::Halftime => {
                effects.banner = Some(("Halftime - switching sides".into(), Color::WHITE, now));
            }
            GameEvent::RoundStart { .. } => {
                effects.deaths.clear();
                effects.utility_detonations.clear();
                effects.banner = None;
            }
            GameEvent::FreezeEnd => {
                // A "let's go" in the local team's voice.
                let team = world.0.player(local.id).map(|p| p.team);
                let voice = if team == Some(sim::Team::Terrorist) { "phoenix" } else { "sas" };
                let lines: Vec<String> = (1..=9).map(|n| format!("{voice}.radio.letsgo0{n}")).filter(|n| sounds.has(n)).collect();
                if !lines.is_empty() {
                    let pick = (world.0.tick as usize) % lines.len();
                    sounds.play(&mut commands, fs, &mut sources, &lines[pick], None);
                }
            }
            _ => {}
        }
    }
    // Announcer lines whose time has come.
    let due: Vec<String> = effects.queued.iter().filter(|q| q.0 <= now).map(|q| q.1.clone()).collect();
    effects.queued.retain(|q| q.0 > now);
    for line in due {
        sounds.play(&mut commands, fs, &mut sources, &line, None);
    }
    // The planted bomb beeps faster and faster.
    if let Some(b) = world.0.bomb {
        let left = (b.explode_at - world.0.time()).max(0.0);
        let total = (b.explode_at - b.planted_at).max(1.0);
        let interval = 0.1 + 0.9 * (left / total);
        if now - effects.last_beep >= interval {
            effects.last_beep = now;
            sounds.play(&mut commands, fs, &mut sources, "c4.plantsound", Some(b.pos));
        }
    }
    effects.explosions.retain(|e| now - e.1 < EXPLOSION_LIFE);
    effects.kill_feed.retain(|k| now - k.time < 6.0);
}

pub fn draw(effects: Res<Effects>, time: Res<Time>, mut gizmos: Gizmos) {
    let now = time.elapsed_secs();
    // Bomb blast: an expanding, fading fireball.
    for (pos, t) in &effects.explosions {
        let k = ((now - t) / EXPLOSION_LIFE).clamp(0.0, 1.0);
        for (i, scale) in [1.0f32, 0.75, 0.5].iter().enumerate() {
            let r = (60.0 + 700.0 * k.sqrt()) * scale;
            let a = (1.0 - k) * (0.9 - i as f32 * 0.2);
            gizmos.sphere(Isometry3d::from_translation(*pos + Vec3::Y * 40.0 * scale), r, Color::srgba(1.0, 0.55 + 0.15 * i as f32, 0.15, a));
        }
    }


}


#[cfg(test)]
mod attack_tests {
    use super::*;
    #[test]
    fn delayed_feed_keeps_authority_names_and_life_after_lookup_disappears() {
        let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
        let victim = world.add_player("victim <old> & name", sim::Team::CounterTerrorist);
        world.damage_environment(victim, 1000, "planted_c4");
        let (attacker, weapon, headshot, death) = world.events.iter().find_map(|event| match event {
            GameEvent::Kill { attacker, weapon, headshot, death, .. } => Some((*attacker, weapon.clone(), *headshot, death.clone())),
            _ => None,
        }).unwrap();
        world.player_mut(victim).unwrap().name = "new name".into();
        world.respawn(victim);
        assert!(world.player(victim).unwrap().life_epoch > death.key.life_epoch);
        world.remove_player(victim);
        let entry = KillFeedEntry::new(attacker, &weapon, headshot, death.clone(), victim, 10.);
        assert_eq!(entry.text, "[bomb]  victim <old> & name");
        assert!(entry.local_involved);
        assert_eq!(entry.death, death);
    }
    #[test]
    fn player_feed_retains_original_team_context_and_existing_text_format() {
        let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
        let victim = world.add_player("victim", sim::Team::CounterTerrorist);
        world.damage_environment(victim, 1000, "environment");
        let mut death = world.events.into_iter().find_map(|event| match event {
            GameEvent::Kill { death, .. } => Some(death), _ => None,
        }).unwrap();
        death.attacker = Some(sim::death::DeathActor { id: 2, name: "original attacker".into(), team: sim::Team::Terrorist });
        let entry = KillFeedEntry::new(2, "weapon_ak47", true, death, 3, 1.);
        assert_eq!(entry.text, "original attacker  [ak47 HS]  victim");
        assert!(!entry.local_involved);
        assert_eq!(entry.death.attacker.as_ref().unwrap().team, sim::Team::Terrorist);
        assert_eq!(entry.death.victim_team, sim::Team::CounterTerrorist);
    }
    #[test]
    fn drawing_a_weapon_never_replays_previous_attacks() {
        let mut effects = Effects::default();
        effects.attack(1, "weapon_ak47".into(), "ACT_VM_PRIMARYATTACK", 1.0);
        // Deployment snapshots the event cursor, including previous shots by this owner.
        let mut seen = effects.attack_serial;
        assert_eq!(effects.consume_attack(1, "weapon_knife", &mut seen), None);
        assert_eq!(effects.consume_attack(1, "weapon_ak47", &mut seen), None);
        // Distinct events in one render frame still have distinct serials.
        effects.attack(1, "weapon_knife".into(), "ACT_VM_MISSCENTER2", 1.0);
        assert_eq!(effects.consume_attack(1, "weapon_knife", &mut seen), Some("ACT_VM_MISSCENTER2"));
        assert_eq!(effects.consume_attack(1, "weapon_knife", &mut seen), None);
        effects.attack(1, "weapon_ak47".into(), "ACT_VM_PRIMARYATTACK", 2.0);
        assert_eq!(effects.consume_attack(1, "weapon_knife", &mut seen), None);
        let mut spectator_seen = effects.attack_serial;
        assert_eq!(effects.consume_attack(1, "weapon_ak47", &mut spectator_seen), None);
    }
}

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

fn damage_sound(group: sim::weapon::HitGroup, protection: sim::weapon::HitProtection) -> &'static str {
    use sim::weapon::{HitGroup, HitProtection};
    match protection {
        HitProtection::Helmet => "Player.DamageHelmet",
        HitProtection::Kevlar => "Player.DamageKevlar",
        HitProtection::Flesh if group == HitGroup::Head => "Player.DamageHeadShot",
        HitProtection::Flesh => "Flesh.BulletImpact",
    }
}

/// Attacker feedback is distinct from the victim's ordinary damage/death cue.
/// Keep the observed Feedback/fatal-headshot entry selection, but deliver the
/// shooter's confirmation at the listener. The owner explicitly requires head
/// feedback across the map; ordinary foley attenuation can bury or cut it off.
/// Body hits and other listeners retain their positional damage sounds. The
/// struck player still owns these voices for channel/instance accounting.
// Owner-requested balance for full-range attacker confirmation. This is a
// provisional mix adjustment, not a measured original-engine gain.
pub(crate) const ATTACKER_HEAD_GAIN: f32 = 0.35;

pub(crate) fn play_combat_cue(
    event: &GameEvent, local: &LocalPlayer, commands: &mut Commands,
    sounds: &mut SoundLibrary, sources: &mut Assets<AudioSource>, fs: &vfs::Vfs,
) {
    let shooter = |attacker, victim| !local.noclip && attacker == local.view_target() && attacker != victim;
    let (sound, victim, pos, feedback) = match event {
        GameEvent::Damage { attacker, victim, group, pos, protection, .. } => {
            let feedback = *group == sim::weapon::HitGroup::Head && shooter(*attacker, *victim);
            let sound = if feedback {
                if *protection == sim::weapon::HitProtection::Helmet {
                    "Player.DamageHelmetFeedback"
                } else {
                    "Player.DamageHeadShotFeedback"
                }
            } else { damage_sound(*group, *protection) };
            (sound, *victim, *pos, feedback)
        }
        GameEvent::Kill { attacker, victim, headshot: true, death, .. } if shooter(*attacker, *victim) => {
            ("Player.DamageHeadShot", *victim, death.hit.map_or(death.pose.origin, |hit| hit.position), true)
        }
        _ => return,
    };
    let own = feedback || (victim == local.view_target() && !local.noclip);
    sounds.play_with_gain(commands, fs, sources, sound,
        Emit { at: (!own).then_some(pos), owner: Some(victim), local: own },
        if feedback { ATTACKER_HEAD_GAIN } else { 1.0 });
}

/// Events produced by fixed ticks since the last frame.
#[derive(Resource, Default)]
pub struct PendingEvents(pub Vec<GameEvent>);

#[derive(Clone, Debug)]
pub struct KillFeedEntry {
    pub text: String,
    /// Keep the authoritative weapon and headshot independently of the
    /// diagnostic text so the authored HUD can choose its exported icons.
    pub weapon: String,
    pub headshot: bool,
    /// Mode of the causally preceding Fire event, when it matches this weapon.
    /// Never infer this from the attacker's potentially changed current gun.
    pub suppressed: Option<bool>,
    pub local_involved: bool,
    pub time: f32,
    /// Immutable authority, retained inside the existing bounded feed rather
    /// than a separate unconsumed corpse queue. No physical impulse is implied.
    pub death: sim::death::DeathReceipt,
}

impl KillFeedEntry {
    fn new(attacker: sim::PlayerId, weapon: &str, headshot: bool,
        death: sim::death::DeathReceipt, local: sim::PlayerId, time: f32,
        suppressed: Option<bool>) -> Self {
        let gun = if weapon == "planted_c4" { "bomb" } else { weapon.trim_start_matches("weapon_") };
        let text = match &death.attacker {
            Some(actor) => format!("{}  [{}{}]  {}", actor.name, gun,
                if headshot { " HS" } else { "" }, death.victim_name),
            None => format!("[{gun}]  {}", death.victim_name),
        };
        Self { text, weapon: weapon.into(), headshot, suppressed,
            local_involved: attacker == local || death.key.victim == local, time, death }
    }

    pub(crate) fn lifetime(&self) -> f32 { if self.local_involved { 7.5 } else { 5.0 } }
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

    fn firearm_attack(&mut self, player: sim::PlayerId, weapon: String, now: f32) {
        // The installed Elite model assigns the two pistols to distinct
        // activities, not variants of PRIMARYATTACK. Advance for every Fire
        // receipt even if several shots arrive before a render frame.
        let right = weapon == "weapon_elite" && self.attacks.get(&player)
            .is_some_and(|(_, previous, activity)| previous == &weapon
                && *activity == "ACT_VM_PRIMARYATTACK");
        self.attack(player, weapon,
            if right { "ACT_VM_SECONDARYATTACK" } else { "ACT_VM_PRIMARYATTACK" }, now);
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
    settings: Res<crate::settings::Settings>,
    mut buy_menu: ResMut<crate::buymenu::BuyMenu>,
    views: Res<crate::viewmodel::ViewModels>,
    mut foley: ResMut<crate::foley::Foley>,
) {
    let now = time.elapsed_secs();
    // Own sounds play at the listener; everyone else's in the world.
    let emit = |player: sim::PlayerId, at: glam::Vec3| {
        let own = player == local.view_target() && !local.noclip;
        Emit { at: (!own).then_some(at), owner: Some(player), local: own }
    };
    let fs = &level.0.vfs;
    // Hitscan Fire and Kill are ordered in the same event batch. Missing
    // remote Fire evidence stays unknown instead of reusing a prior mode.
    let mut shot_modes = std::collections::HashMap::<sim::PlayerId, (String, bool)>::new();
    for event in pending.0.drain(..) {
        play_combat_cue(&event, &local, &mut commands, &mut sounds, &mut sources, fs);
        if matches!(event, GameEvent::Damage {..} | GameEvent::Impact {..} | GameEvent::RoundStart {..}) { effects.feedback.push(event.clone()); }
        match event {
            GameEvent::Fire { player, weapon, origin, suppressed } => {
                shot_modes.insert(player, (weapon.clone(), suppressed));
                effects.firearm_attack(player, weapon.clone(), now);
                // These installed attack sequences declare separate bolt/pump
                // sounds, not another gunshot. Classic shot replacement does
                // not deliver those handling events. Use the same confirmed
                // shot, clip timing and interruption rules as the AWP.
                if matches!(weapon.as_str(), "weapon_awp" | "weapon_ssg08"
                    | "weapon_nova" | "weapon_sawedoff" | "weapon_mag7") {
                    if let Some((duration, events)) = views.activity_sounds(&weapon, "ACT_VM_PRIMARYATTACK") {
                        foley.primary_attack(player, &weapon, world.0.time(), duration, &events);
                    }
                }
                let sound = world.0.catalog.get(&weapon).map(|d| sounds.weapon_shot_mode_entry(d, suppressed).to_string()).unwrap_or_default();
                if !sound.is_empty() {
                    sounds.play_with(&mut commands, fs, &mut sources, &sound, emit(player, origin));
                }
            }
            GameEvent::DecoyShot {pos,weapon,..} => {
                if let Some(def) = weapon.parse().ok().and_then(|id| world.0.catalog.by_id(&id)) {
                    sounds.play_decoy(&mut commands,fs,&mut sources,def,pos);
                }
            }
            GameEvent::UtilityThrown { player,kind,.. } => {
                // The installed grenade view models expose ACT_VM_THROW.
                effects.attack(player, kind.name().to_string(), "ACT_VM_THROW", now);
                // The release-phase cue is played by tick-owned foley. This
                // physical spawn event must not repeat it a clip event later.
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
                    sim::utility::Kind::Molotov|sim::utility::Kind::Incendiary=>Some("Inferno.Start_IncGrenade"),
                    sim::utility::Kind::Smoke=>Some("BaseSmokeEffect.Sound"),_=>None};
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
            GameEvent::DoorSound { door, cue, pos } => {
                if let Some(def) = level.0.collision.doors.get(door as usize) {
                    let name = match cue {
                        sim::doors::Cue::Move => &def.move_sound,
                        sim::doors::Cue::Open => &def.open_sound,
                        sim::doors::Cue::Close => &def.close_sound,
                        sim::doors::Cue::Unlock => &def.unlock_sound,
                    };
                    if !name.is_empty() { sounds.play(&mut commands, fs, &mut sources, name, Some(pos)); }
                }
            }
            GameEvent::EmptyTrigger { player, weapon, origin } => {
                if let Some(profile) = world.0.catalog.get(&weapon).and_then(|def| def.empty_trigger.as_ref()) {
                    if std::env::var_os("CSR_WEAPON_TRACE").is_some() {
                        info!("empty trigger event: player={player} weapon={weapon} sound={} origin={origin:?}", profile.sound);
                    }
                    sounds.play_with(&mut commands, fs, &mut sources, &profile.sound, emit(player, origin));
                }
            }
            GameEvent::BurstModeChanged { player, origin, .. } => {
                // Both installed burst viewmodels have no separate switch
                // sequence; native mode changes emit this soundscript cue.
                sounds.play_with(&mut commands, fs, &mut sources,
                    "Weapon.AutoSemiAutoSwitch", emit(player, origin));
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
            GameEvent::Damage { attacker, group, .. } => {
                if attacker == local.view_target() {
                    effects.last_hit = now;
                    effects.last_hit_headshot = group == sim::weapon::HitGroup::Head;
                }
            }
            GameEvent::Kill { attacker, weapon, headshot, death, .. } => {
                if effects.deaths.len() < 128 { effects.deaths.push(death.clone()); }
                let suppressed = shot_modes.get(&attacker)
                    .filter(|(fired_weapon, _)| fired_weapon == &weapon).map(|(_, mode)| *mode);
                effects.kill_feed.push(KillFeedEntry::new(attacker, &weapon, headshot, death, local.id, now, suppressed));
                if effects.kill_feed.len() > 7 {
                    effects.kill_feed.remove(0);
                }
            }
            GameEvent::Purchase { player, .. } => {
                buy_menu.confirmed_purchase(player, local.id, settings.close_on_buy);
                if player == local.id {
                    sounds.play(&mut commands, fs, &mut sources, "uipanorama.buymenu_purchase", None);
                }
            }
            GameEvent::Jump { player } => {
                if let Some(p) = world.0.player(player) {
                    crate::foley::jump(&mut commands, &mut sounds, &mut sources, &level.0, p, &local);
                }
            }
            GameEvent::Land { player, speed } => {
                if let Some(p) = world.0.player(player) {
                    crate::foley::land(&mut commands, &mut sounds, &mut sources, &level.0, &world.0.collision, &props, p, &local, speed);
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
    effects.kill_feed.retain(|k| now - k.time < k.lifetime());
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
    fn dual_pistol_fire_receipts_alternate_independently_of_render_consumption() {
        let mut effects = Effects::default();
        let mut seen = 0;
        for n in 0..8 {
            effects.firearm_attack(1, "weapon_elite".into(), n as f32 * 0.12);
            let expected = if n % 2 == 0 { "ACT_VM_PRIMARYATTACK" } else { "ACT_VM_SECONDARYATTACK" };
            assert_eq!(effects.attacks[&1].2, expected);
            if n % 3 == 0 {
                assert_eq!(effects.consume_attack(1, "weapon_elite", &mut seen), Some(expected));
                assert_eq!(effects.consume_attack(1, "weapon_elite", &mut seen), None);
            }
        }
        effects.firearm_attack(2, "weapon_elite".into(), 1.);
        assert_eq!(effects.attacks[&2].2, "ACT_VM_PRIMARYATTACK");
        effects.firearm_attack(1, "weapon_ak47".into(), 2.);
        assert_eq!(effects.attacks[&1].2, "ACT_VM_PRIMARYATTACK");
    }

    #[test]
    fn installed_dual_pistols_bind_both_firing_hands() {
        let Some(game) = vfs::find_csgo_dir() else { return; };
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&game).unwrap();
        let bytes = fs.read("models/weapons/v_pist_elite.mdl").unwrap();
        let model = mdl::Model::load_header(&bytes).unwrap();
        for (activity, name) in [("ACT_VM_PRIMARYATTACK", "shoot_left1"),
            ("ACT_VM_SECONDARYATTACK", "shoot_right1")] {
            assert!(model.sequences.iter().any(|seq| seq.activity == activity && seq.name == name));
        }
    }

    // UNRUN by source-only child. Read installed app 4465480 declarations;
    // this checks event/sample routing, not historical PCM or audible parity.
    #[test]
    fn installed_manual_action_shots_declare_separate_handling_without_duplicate_gunfire() {
        let Some(game) = vfs::find_csgo_dir() else { return; };
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&game).unwrap();
        let script = kv::parse(&fs.read_string("scripts/game_sounds_weapons.txt").unwrap()).unwrap();
        for (model, expected) in [
            ("v_snip_ssg08", vec![
                (0.38, "Weapon_SSG08.BoltBack", "weapons/ssg08/ssg08_boltback.wav"),
                (0.58, "Weapon_SSG08.BoltForward", "weapons/ssg08/ssg08_boltforward.wav"),
            ]),
            ("v_shot_nova", vec![
                (0.4166667, "Weapon_Nova.PumpDuringShot", "weapons/nova/nova_pump.wav"),
            ]),
            ("v_shot_sawedoff", vec![
                (0.04166667, "Weapon_Sawedoff.PumpDuringShot", "weapons/sawedoff/sawedoff_pump.wav"),
            ]),
            ("v_shot_mag7", vec![
                (0.04166667, "Weapon_MAG7.PumpBack", "weapons/mag7/mag7_pump_back.wav"),
                (0.25, "Weapon_MAG7.PumpForward", "weapons/mag7/mag7_pump_forward.wav"),
            ]),
        ] {
            let bytes = fs.read(&format!("models/weapons/{model}.mdl")).unwrap();
            let model = mdl::Model::load_header(&bytes).unwrap();
            let attack = model.sequences.iter().find(|s| s.activity == "ACT_VM_PRIMARYATTACK").unwrap();
            let actual: Vec<_> = attack.events.iter().filter_map(|e|
                Some((e.cycle, e.client_sound()?))).collect();
            assert_eq!(actual.len(), expected.len());
            for ((cycle, entry), (expected_cycle, expected_entry, sample)) in actual.iter().zip(&expected) {
                assert!((cycle - expected_cycle).abs() < 0.00001);
                assert_eq!(entry, expected_entry);
                let wave = script.block(entry).unwrap().str("wave").unwrap();
                assert_eq!(wave.trim_start_matches('~'), *sample);
                assert!(!fs.read(&format!("sound/{sample}")).unwrap().is_empty());
            }
            // Production schedules these cycles using the decoded clip,
            // rather than a guessed bolt delay or the firing cooldown.
            let ani = (!model.anim_block_file.is_empty()).then(|| fs.read(&model.anim_block_file)).flatten();
            let clip = model.decode_animation(attack.anims[0] as usize, ani.as_deref()).unwrap();
            assert!(clip.duration().is_finite() && clip.duration() > 0.0);
        }
    }

    #[test]
    fn damage_audio_uses_immutable_contact_protection() {
        use sim::weapon::{HitGroup, HitProtection};
        for (group, protection, sound) in [
            (HitGroup::Head, HitProtection::Helmet, "Player.DamageHelmet"),
            (HitGroup::Head, HitProtection::Flesh, "Player.DamageHeadShot"),
            (HitGroup::Chest, HitProtection::Kevlar, "Player.DamageKevlar"),
            (HitGroup::LeftLeg, HitProtection::Flesh, "Flesh.BulletImpact"),
        ] { assert_eq!(damage_sound(group, protection), sound); }
    }

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
        let entry = KillFeedEntry::new(attacker, &weapon, headshot, death.clone(), victim, 10., None);
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
        let entry = KillFeedEntry::new(2, "weapon_ak47", true, death, 3, 1., None);
        assert_eq!(entry.text, "original attacker  [ak47 HS]  victim");
        assert_eq!(entry.weapon, "weapon_ak47");
        assert!(entry.headshot);
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

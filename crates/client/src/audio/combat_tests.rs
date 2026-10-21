//! Admission regressions, not listening/mix parity tests. No AudioPlugin/device,
//! installed assets or hand-built Damage/Kill events: fire through World::step
//! and drain the resulting batch through the actual effects::process system.
use super::*;
use crate::{effects, player::{LocalPlayer, SimWorld}, LoadedLevel};
use sim::{GameEvent, weapon::{HitGroup, HitProtection}};
use std::collections::BTreeMap;

fn level() -> level::Level {
    // Empty synthetic BSP; the VFS accepts its absent pakfile as an empty mount.
    let mut bsp = vec![0u8; 1036];
    bsp[..4].copy_from_slice(b"VBSP");
    bsp[4..8].copy_from_slice(&20i32.to_le_bytes());
    // Synthetic zero-spread content isolates event admission. These are not
    // fixtures for installed damage, recoil or classic waveform/mix parity.
    let schema = r#"items_game { items {
        7 { name weapon_ak47 baseitem 1
            attributes { damage 36 "armor ratio" 1.55 "primary clip size" 30 }
            visuals { sound_single_shot Weapon_AK47.Single } }
        60 { name weapon_m4a1_silencer baseitem 1
            attributes { damage 33 "armor ratio" 1.4 "primary clip size" 20 "has silencer" 1 }
            visuals { sound_single_shot Weapon_M4A1.Single sound_special1 Weapon_M4A1.Silenced } }
    } }"#;
    let mut fs = vfs::Vfs::new();
    fs.push_back("combat fixture", vfs::Mount::Memory(BTreeMap::from([
        ("maps/combat_fixture.bsp".into(), Arc::<[u8]>::from(bsp)),
        ("scripts/items/items_game.txt".into(), Arc::<[u8]>::from(schema.as_bytes())),
    ])));
    level::Level::load_from_vfs(
        content::ContentId::csgo(content::ContentKind::Map, "combat_fixture"),
        "combat_fixture", Default::default(), fs,
    ).unwrap()
}

fn sounds() -> SoundLibrary {
    let mut library = SoundLibrary { master_volume: 0.5, ..Default::default() };
    for (name, stack, limit, volume, bounds, channels) in [
        ("Weapon_AK47.Single", "CS_update_weapon", "CS_distance_weapon_start", 0.9, "", 2),
        ("Weapon_M4A1.Silenced", "CS_update_weapon", "CS_distance_weapon_start", 0.9, "", 2),
        ("Player.DamageHelmetFeedback", "CS_update_foley", "CS_limit_start", 0.7, "input_min 120 input_max 4000", 2),
        ("Player.DamageHeadShotFeedback", "CS_update_foley", "CS_limit_start", 0.7, "input_min 120 input_max 4000", 2),
        ("Player.DamageHeadShot", "CS_update_foley", "CS_limit_start", 1.0, "", 2),
        ("Player.DamageHelmet", "CS_update_foley", "CS_limit_start", 0.7, "input_min 120 input_max 1400", 2),
        ("Player.DamageKevlar", "CS_update_foley", "CS_limit_start", 1.0, "", 1),
        ("Flesh.BulletImpact", "CS_update_rics", "CS_limit_bullet_impact", 0.7, "", 1),
    ] {
        let channel = if name == "Flesh.BulletImpact" { "CHAN_AUTO" } else { "CHAN_STATIC" };
        let text = format!("Cue {{ wave synthetic.wav volume {volume} channel {channel}
            operator_stacks {{ start_stack {{ import_stack {limit} }}
            update_stack {{ import_stack {stack} volume_falloff {{ {bounds} }} }} }} }}");
        let doc = kv::parse(&text).unwrap();
        let mut entry = parse_entry(doc.block("Cue").unwrap()).unwrap();
        // Validity/decoding is intentionally outside this device-free test.
        entry.frozen_wave = Some(classic::FrozenWave { bytes: Arc::from([0u8; 4]), channels });
        library.entries.insert(name.to_ascii_lowercase(), entry);
    }
    library
}

#[test]
fn actual_ak_and_silenced_m4_shots_admit_head_cues_without_gun_voice_stealing() {
    for weapon in ["weapon_ak47", "weapon_m4a1_silencer"] {
        for distance in [200.0, 5000.0] {
            for helmet in [false, true] {
                for fatal in [false, true] {
                    for perspective in ["attacker", "victim", "bystander", "spectator", "noclip"] {
                        check_shot(weapon, distance, helmet, fatal, true, perspective);
                    }
                }
            }
        }
    }
}

#[test]
fn actual_body_shots_keep_nearby_positional_damage_without_head_feedback() {
    for weapon in ["weapon_ak47", "weapon_m4a1_silencer"] {
        for distance in [200.0, 5000.0] {
            for armored in [false, true] {
                check_shot(weapon, distance, armored, false, false, "attacker");
            }
        }
    }
}

fn check_shot(weapon: &str, distance: f32, armored: bool, fatal: bool, head: bool, perspective: &str) {
    let level = level();
    let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
    let attacker = world.add_player("attacker", sim::Team::Terrorist);
    let victim = world.add_player("victim", sim::Team::CounterTerrorist);
    let def = Arc::new(world.catalog.get(weapon).unwrap().clone());
    let shooter = world.player_mut(attacker).unwrap();
    shooter.movement = movement::PlayerMove::spawn_at(glam::Vec3::Z);
    shooter.weapons = vec![sim::weapon::WeaponState::new(def)];
    shooter.active = 0;
    let target = world.player_mut(victim).unwrap();
    target.movement = movement::PlayerMove::spawn_at(glam::Vec3::new(distance, 0.0, 1.0));
    target.health = if fatal { 1 } else { 1000 };
    target.armor = if armored { 100 } else { 0 };
    target.helmet = armored;
    world.step(&[(attacker, sim::TickInput { buttons: movement::buttons::ATTACK,
        aim: Some(sim::AimRay { origin: glam::Vec3::Z * if head { 64.0 } else { 50.0 },
            direction: glam::Vec3::X }), ..Default::default() })]);
    let expected_group = if head { HitGroup::Head } else { HitGroup::Chest };
    let protection = if !armored { HitProtection::Flesh }
        else if head { HitProtection::Helmet } else { HitProtection::Kevlar };
    assert_eq!(world.player(attacker).unwrap().shots, 1, "{weapon}");
    let (damage_index, contact) = world.events.iter().enumerate().find_map(|(i, event)| match event {
        GameEvent::Damage { attacker: a, victim: v, group, protection: p, pos, .. }
            if (*a, *v, *group, *p) == (attacker, victim, expected_group, protection) => Some((i, *pos)),
        _ => None,
    }).expect("actual firearm contact must carry the expected immutable protection");
    let kill_index = world.events.iter().position(|e| matches!(e, GameEvent::Kill { victim: v, .. } if *v == victim));
    assert_eq!(kill_index.is_some(), fatal);
    if let Some(kill) = kill_index { assert!(damage_index < kill); }

    let mut local = LocalPlayer::new(if perspective == "victim" { victim }
        else if matches!(perspective, "bystander" | "spectator") { 99 } else { attacker }, glam::Vec3::ZERO, 0.0);
    local.noclip = perspective == "noclip";
    if perspective == "spectator" { local.spectating = Some(attacker); }
    let feedback = head && matches!(perspective, "attacker" | "spectator");
    let cue = match (head, armored, feedback) {
        (true, true, true) => "player.damagehelmetfeedback",
        (true, false, true) => "player.damageheadshotfeedback",
        (true, true, false) => "player.damagehelmet",
        (true, false, false) => "player.damageheadshot",
        (false, true, _) => "player.damagekevlar",
        (false, false, _) => "flesh.bulletimpact",
    };
    let mut app = App::new();
    app.insert_resource(effects::PendingEvents(world.events.clone()))
        .insert_resource(SimWorld(world)).insert_resource(local).insert_resource(LoadedLevel(level))
        .insert_resource(sounds()).init_resource::<Assets<AudioSource>>()
        .init_resource::<effects::Effects>().init_resource::<crate::tracers::Tracers>()
        .init_resource::<Time>().init_resource::<crate::surfaces::SurfaceProps>()
        .init_resource::<crate::settings::Settings>().init_resource::<crate::buymenu::BuyMenu>()
        .init_resource::<crate::viewmodel::ViewModels>().init_resource::<crate::foley::Foley>()
        .add_systems(Update, effects::process).add_systems(Last, prune);
    app.update();
    assert!(app.world().resource::<effects::PendingEvents>().0.is_empty());
    let sounds = app.world().resource::<SoundLibrary>();
    let extra_death = usize::from(head && fatal && feedback);
    assert_eq!(sounds.playing.len(), 2 + extra_death, "one gun, one damage cue, optional fatal head cue: {weapon}/{perspective}");
    assert_eq!(sounds.playing.iter().filter(|p| p.entry == cue).count(), 1);
    assert_eq!(sounds.playing.iter().filter(|p| p.entry.contains("feedback")).count(), usize::from(feedback));
    let voice = sounds.playing.iter().find(|p| p.entry == cue).unwrap();
    assert_eq!(voice.owner, victim);
    assert!(app.world().get::<AudioPlayer>(voice.entity).is_some(), "gun requests must not cancel the damage voice before playback");
    if perspective == "victim" || feedback {
        assert!(app.world().get::<Emitter>(voice.entity).is_none());
        assert!(app.world().get::<SoundGain>(voice.entity).unwrap().0 > 0.0);
    } else {
        let emitter = app.world().get::<Emitter>(voice.entity).unwrap();
        assert_eq!(emitter.pos, to_bevy(contact));
        if !head {
            assert_eq!(emitter.gain(distance) > 0.0, distance == 200.0);
        }
    }
    if extra_death == 1 {
        assert_eq!(sounds.playing.iter().filter(|p| p.entry == "player.damageheadshot").count(), 1);
    }
    let gun = sounds.playing.iter().find(|p| p.owner == attacker).unwrap();
    assert_eq!(gun.entry, if weapon == "weapon_ak47" { "weapon_ak47.single" } else { "weapon_m4a1.silenced" });
    // Last sees the actual flushed AudioPlayers; a second drain adds no cues.
    app.update();
    assert_eq!(app.world().resource::<SoundLibrary>().playing.len(), 2 + extra_death);
}

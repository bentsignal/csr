//! Device-free lifecycle dispatch regression. Fire batches are synthetic;
//! Kill/death receipts and RoundStart come from the real simulation lifecycle.
use super::*;
use bevy::ecs::system::RunSystemOnce;
use std::{collections::BTreeMap, sync::Arc};

fn level() -> level::Level {
    let mut bsp = vec![0; 1036];
    bsp[..4].copy_from_slice(b"VBSP");
    bsp[4..8].copy_from_slice(&20i32.to_le_bytes());
    let mut fs = vfs::Vfs::new();
    fs.push_back("firing life fixture", vfs::Mount::Memory(BTreeMap::from([
        ("maps/firing_life.bsp".into(), Arc::<[u8]>::from(bsp)),
        ("scripts/items/items_game.txt".into(), Arc::<[u8]>::from(&b"items_game { items {} }"[..])),
    ])));
    level::Level::load_from_vfs(content::ContentId::csgo(content::ContentKind::Map, "firing_life"),
        "firing_life", Default::default(), fs).unwrap()
}

fn shot(player: sim::PlayerId) -> [GameEvent; 2] {
    [GameEvent::Fire { player, weapon: "weapon_elite".into(), origin: glam::Vec3::ZERO,
        suppressed: false, secondary: false },
        GameEvent::Tracer { player, start: glam::Vec3::ZERO, end: glam::Vec3::X * 100. }]
}

fn dispatch(app: &mut App, events: Vec<GameEvent>) {
    app.insert_resource(PendingEvents(events));
    // Match production order: capture all Fire indices before effects drains.
    app.world_mut().run_system_once(crate::muzzle_flash::capture).unwrap();
    app.world_mut().run_system_once(process).unwrap();
    assert!(app.world().resource::<PendingEvents>().0.is_empty());
}

#[test]
fn kill_and_round_start_reset_firing_life_without_rewinding_or_losing_batch_barrels() {
    let level = level();
    let mut sim = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
    let victim = sim.add_player("victim", sim::Team::Terrorist);
    let survivor = sim.add_player("survivor", sim::Team::CounterTerrorist);
    sim.player_mut(victim).unwrap().movement.origin = glam::Vec3::new(12., 34., 56.);
    sim.damage_environment(victim, 1000, "environment");
    let death = sim.events.iter().find_map(|event| match event {
        GameEvent::Kill { death, .. } => Some(death.clone()),
        _ => None,
    }).expect("real death must produce an immutable receipt");
    let mut batch = shot(victim).into_iter().chain(shot(survivor)).collect::<Vec<_>>();
    batch.append(&mut sim.events);
    let mut app = App::new();
    app.insert_resource(LoadedLevel(level)).insert_resource(SimWorld(sim))
        .insert_resource(LocalPlayer::new(survivor, glam::Vec3::ZERO, 0.))
        .init_resource::<Effects>().init_resource::<crate::tracers::Tracers>()
        .init_resource::<crate::muzzle_flash::Flashes>()
        .init_resource::<SoundLibrary>().init_resource::<Assets<AudioSource>>()
        .init_resource::<Time>().init_resource::<crate::surfaces::SurfaceProps>()
        .init_resource::<crate::settings::Settings>().init_resource::<crate::buymenu::BuyMenu>()
        .init_resource::<crate::viewmodel::ViewModels>().init_resource::<crate::foley::Foley>();
    dispatch(&mut app, batch);
    let effects = app.world().resource::<Effects>();
    assert_eq!(effects.attack_serial, 2);
    assert!(!effects.last_fire.contains_key(&victim));
    assert!(effects.last_fire.contains_key(&survivor));
    assert_eq!(effects.consume_attack(victim, "weapon_elite", &mut 0), None);
    assert_eq!(effects.consume_attack(survivor, "weapon_elite", &mut 0), Some("ACT_VM_PRIMARYATTACK"));
    assert_eq!(effects.fire_muzzles, vec![Some(true), Some(true)], "Kill must not shift captured Fire indices");
    assert_eq!(effects.deaths, vec![death.clone()]);
    assert_eq!(effects.kill_feed[0].death, death);
    assert_eq!(app.world().resource::<crate::tracers::Tracers>().pending.iter().map(|p| p.3).collect::<Vec<_>>(),
        vec![Some(true), Some(true)]);

    app.world_mut().resource_mut::<SimWorld>().0.respawn(victim);
    assert!(app.world().resource::<SimWorld>().0.player(victim).unwrap().life_epoch > death.key.life_epoch);
    let mut victim_seen = app.world().resource::<Effects>().attack_serial;
    let mut survivor_seen = victim_seen;
    dispatch(&mut app, shot(victim).into_iter().chain(shot(survivor)).collect());
    let effects = app.world().resource::<Effects>();
    assert_eq!(effects.attack_serial, 4, "death must not rewind the global cursor");
    assert_eq!(effects.consume_attack(victim, "weapon_elite", &mut victim_seen), Some("ACT_VM_PRIMARYATTACK"));
    assert_eq!(effects.consume_attack(survivor, "weapon_elite", &mut survivor_seen), Some("ACT_VM_SECONDARYATTACK"));
    assert_eq!(effects.fire_muzzles, vec![Some(true), Some(false)]);
    assert_eq!(app.world().resource::<crate::tracers::Tracers>().pending.iter().rev().take(2).map(|p| p.3).collect::<Vec<_>>(),
        vec![Some(false), Some(true)]);
    assert_eq!(effects.deaths, vec![death.clone()], "respawning must not mutate the captured death");
    assert_eq!(effects.kill_feed[0].death, death);

    let round_events = {
        let mut world = app.world_mut().resource_mut::<SimWorld>();
        world.0.set_mode(sim::GameMode::Competitive, Default::default());
        std::mem::take(&mut world.0.events)
    };
    assert!(round_events.iter().any(|e| matches!(e, GameEvent::RoundStart { .. })));
    let mut batch = shot(victim).into_iter().chain(shot(survivor)).collect::<Vec<_>>();
    batch.extend(round_events);
    dispatch(&mut app, batch);
    let effects = app.world().resource::<Effects>();
    assert_eq!(effects.attack_serial, 6, "RoundStart must not rewind the global cursor");
    assert!(effects.last_fire.is_empty());
    for player in [victim, survivor] {
        assert_eq!(effects.consume_attack(player, "weapon_elite", &mut 0), None);
    }
    assert_eq!(effects.fire_muzzles, vec![Some(false), Some(true)], "RoundStart must preserve this batch's barrel receipts");
    assert_eq!(app.world().resource::<crate::tracers::Tracers>().pending.iter().rev().take(2).map(|p| p.3).collect::<Vec<_>>(),
        vec![Some(true), Some(false)]);
    assert!(effects.deaths.is_empty());
    assert_eq!(effects.kill_feed[0].death, death);

    dispatch(&mut app, shot(victim).into_iter().chain(shot(survivor)).collect());
    let effects = app.world().resource::<Effects>();
    assert_eq!(effects.attack_serial, 8);
    assert_eq!(effects.last_fire.len(), 2);
    assert_eq!(effects.consume_attack(victim, "weapon_elite", &mut victim_seen), Some("ACT_VM_PRIMARYATTACK"));
    assert_eq!(effects.consume_attack(survivor, "weapon_elite", &mut survivor_seen), Some("ACT_VM_PRIMARYATTACK"));
    assert_eq!(effects.fire_muzzles, vec![Some(true), Some(true)]);

    dispatch(&mut app, Vec::new());
    assert_eq!(app.world().resource::<Effects>().attack_serial, 8);
    assert!(app.world().resource::<Effects>().fire_muzzles.is_empty());
}

use super::*;
use objectives::{BombSite, BuyZone, Zone};
use weapons::{Side, WeaponClass, WeaponDefinition};

#[test]
fn bsp_ladder_tick_input_snapshot_prediction_and_life_reset_64_128() {
    for rate in [64, 128] {
        let mut w = world();
        w.set_tick_rate(rate);
        let c = Arc::get_mut(&mut w.collision).unwrap();
        c.add_convex(collision::Convex {
            planes: vec![
                collision::Plane { normal: Vec3::X, dist: 1. },
                collision::Plane { normal: -Vec3::X, dist: 0. },
                collision::Plane { normal: Vec3::Y, dist: 24. },
                collision::Plane { normal: -Vec3::Y, dist: 24. },
                collision::Plane { normal: Vec3::Z, dist: 512. },
                collision::Plane { normal: -Vec3::Z, dist: 0. },
            ],
            mins: Vec3::new(0., -24., 0.), maxs: Vec3::new(1., 24., 512.),
            contents: contents::LADDER | contents::GRATE, surface: 0,
        });
        c.finalize();
        let id = w.add_player("climber", Team::CounterTerrorist);
        w.player_mut(id).unwrap().movement = PlayerMove::spawn_at(Vec3::new(-16. - collision::DIST_EPSILON, 0., 64.));
        let input = TickInput { forward_move: 450., ..Default::default() };
        for _ in 0..16 { w.step(&[(id, input)]); }
        assert!(w.player(id).unwrap().movement.origin.z > 64.);
        assert_eq!(w.player(id).unwrap().movement.ladder_normal, Some(-Vec3::X));
        let snapshot: Snapshot = w.decode(&state::encode(&w.snapshot())).unwrap();
        assert_eq!(snapshot, w.snapshot());
        let mut predicted = w.clone();
        predicted.player_mut(id).unwrap().movement.ladder_normal = None;
        predicted.restore(snapshot.state);
        for _ in 0..16 {
            w.step(&[(id, input)]);
            predicted.step_prediction(&[(id, input)]);
            assert_eq!(w.state(), predicted.state());
        }
        w.kill(0, None, "world", false, None);
        assert_eq!(w.player(id).unwrap().movement.ladder_normal, None);
        w.respawn(id);
        assert_eq!(w.player(id).unwrap().movement.ladder_normal, None);
    }
}

#[test]
fn disconnected_competitive_player_leaves_bomb_and_cancels_defusing() {
    let mut w = world();
    let t = w.add_player("departing carrier", Team::Terrorist);
    let ct = w.add_player("departing defuser", Team::CounterTerrorist);
    w.set_mode(GameMode::Competitive, rules());
    w.step(&[]);
    assert!(w.player(t).unwrap().has_bomb());
    assert!(w.remove_player(t));
    assert!(w.player(t).is_none());
    assert!(w.dropped_bomb().is_some());
    w.bomb = Some(PlantedBomb { pos: Vec3::ZERO, planter: t, site: 0, planted_at: 0., explode_at: 40., defuser: Some((ct, 0.)) });
    assert!(w.remove_player(ct));
    assert!(w.bomb.unwrap().defuser.is_none());
    assert!(!w.remove_player(ct));
}

#[test]
fn removed_competitive_bot_keeps_existing_gear_and_bomb_cleanup() {
    let mut w = world();
    let t = w.add_bot("departing carrier", Team::Terrorist);
    let ct = w.add_bot("departing defuser", Team::CounterTerrorist);
    w.set_mode(GameMode::Competitive, rules());
    w.step(&[]);
    assert!(w.player(t).unwrap().has_bomb());
    assert!(w.remove_player(t));
    assert!(w.dropped_bomb().is_some());
    assert_eq!(w.items.iter().filter(|item| item.class_name() == C4).count(), 1);
    assert!(w.items.iter().any(|item| item.class_name() == "weapon_glock"));
    let drops = w.events.iter().filter(|event| matches!(event, GameEvent::Dropped { player, .. } if *player == t)).count();
    assert_eq!(drops, 2);
    assert!(!w.remove_player(t));
    assert_eq!(w.events.iter().filter(|event| matches!(event, GameEvent::Dropped { player, .. } if *player == t)).count(), drops);
    w.player_mut(ct).unwrap().defuse_kit = true;
    w.bomb = Some(PlantedBomb { pos: Vec3::ZERO, planter: t, site: 0, planted_at: 0., explode_at: 40., defuser: Some((ct, 0.)) });
    assert!(w.remove_player(ct));
    assert!(w.bomb.unwrap().defuser.is_none());
    assert!(w.items.iter().any(|item| item.class_name() == "item_defuser"));
}

#[test]
fn prediction_preserves_bot_metadata_without_inventing_ai_or_replaying_events() {
    let mut w = world();
    let local = w.add_player("local", Team::CounterTerrorist);
    let bot = w.add_bot("remote bot", Team::Terrorist);
    w.player_mut(local).unwrap().movement.on_ground = true;
    let mut authority = w.clone();
    let input = TickInput { forward_move: 450., buttons: buttons::JUMP, ..Default::default() };
    w.step_prediction(&[(local, input)]);
    assert!(w.player(bot).unwrap().is_bot);
    assert_eq!(w.player(bot).unwrap().last_input, TickInput::default());
    assert!(w.events.is_empty());
    authority.step(&[(local, input)]);
    assert!(authority.events.iter().any(|event| matches!(event, GameEvent::Jump { player } if *player == local)));
    assert_ne!(authority.player(bot).unwrap().last_input, TickInput::default());
    assert_eq!(w.player(local).unwrap().movement, authority.player(local).unwrap().movement);
}

#[test]
fn special_mode_is_solo_without_round_timeouts_and_still_respawns() {
    let mut w=world();let id=w.add_player("player",Team::CounterTerrorist);w.add_bot("bot",Team::Terrorist);
    w.set_mode(GameMode::Special, RulesConfig{round_time:1.,max_rounds:1,..rules()});
    assert_eq!(w.players.len(),1);assert!(!w.players[0].is_bot);assert!(bots::Bots::default().think(&w).is_empty());
    let round=w.round.round;
    w.tick=64*60*120;w.step(&[]);
    assert_eq!(w.mode,GameMode::Special);assert_eq!(w.round.round,round);assert!(!w.competitive());assert!(w.player(id).unwrap().alive);
    assert!(!w.events.iter().any(|e|matches!(e,GameEvent::RoundEnd{..}|GameEvent::RoundStart{..})));
    w.damage_environment(id,1000,"environment");w.tick+=w.tick_rate as u64*10;w.step(&[]);assert!(w.player(id).unwrap().alive);
}

#[test]
fn environmental_damage_uses_normal_death_and_respawn_path() {
    let mut w=world();let id=w.add_player("player",Team::CounterTerrorist);
    w.damage_environment(id,40,"environment");
    assert!(w.player(id).unwrap().alive);assert_eq!(w.player(id).unwrap().health,60);
    w.damage_environment(id,70,"environment");
    let p=w.player(id).unwrap();assert!(!p.alive);assert_eq!(p.health,0);assert_eq!(p.deaths,1);assert!(p.respawn_at>w.time());
    w.damage_environment(id,100,"environment");assert_eq!(w.player(id).unwrap().deaths,1);
    assert!(w.events.iter().any(|e|matches!(e,GameEvent::Kill{attacker:WORLD,victim,..} if *victim==id)));
}

#[test]
fn knife_hits_dynamic_world_creature_without_bullet_events() {
    struct Creature(std::sync::Mutex<Vec<f32>>);
    impl collision::DynamicWorld for Creature {
        fn trace_box(&self,_:Vec3,end:Vec3,_:Vec3,_:Vec3,_:i32)->collision::Trace{collision::Trace::empty(end)}
        fn point_contents(&self,_:Vec3,_:i32)->i32{0}
        fn hit_target(&self,_:Vec3,_:Vec3)->Option<(u64,f32)>{Some((42,0.5))}
        fn damage_target(&self,_:u64,damage:f32,_:Vec3){self.0.lock().unwrap().push(damage);}
    }
    let creature=Arc::new(Creature(Default::default()));
    let mut c=CollisionWorld::new();c.dynamic=Some(creature.clone());
    let mut w=World::with_catalog(Arc::new(c),Arc::new(catalog()));
    w.add_player("player",Team::CounterTerrorist);
    let knife=w.players[0].weapons.iter().position(|w|w.def.class==WeaponClass::Knife).unwrap();w.players[0].active=knife;
    w.melee_attack(0,1.,false);
    assert_eq!(*creature.0.lock().unwrap(),vec![40.]);
    assert!(w.events.iter().any(|e|matches!(e,GameEvent::Melee{hit:MeleeHit::Player,..})));
    assert!(!w.events.iter().any(|e|matches!(e,GameEvent::Fire{..}|GameEvent::Tracer{..})));
}

#[test]
fn ray_box_hits_and_misses() {
    let t = ray_box(Vec3::ZERO, Vec3::X, Vec3::new(10.0, -1.0, -1.0), Vec3::new(12.0, 1.0, 1.0));
    assert_eq!(t, Some(10.0));
    assert!(ray_box(Vec3::ZERO, Vec3::Y, Vec3::new(10.0, -1.0, -1.0), Vec3::new(12.0, 1.0, 1.0)).is_none());
}

#[test]
fn authored_headshot_multiplier_affects_real_shots_but_not_body_damage() {
    // Resolved installed Legacy 1.38.8.1 stats, not a new native hurt trace.
    // Parse the metadata so the declaration survives the same catalog path.
    let parsed = weapons::Catalog::from_items_game(r#"items_game {
        prefabs { rifle { attributes {
            "armor ratio" "1.4" "primary clip size" "30" "is full auto" "1"
        } } }
        items {
            60 { name weapon_m4a1_silencer baseitem 1 prefab rifle attributes {
                damage 38 "headshot multiplier" 3.475 "range modifier" .94 "primary clip size" 20
            } }
            7 { name weapon_ak47 baseitem 1 prefab rifle attributes {
                damage 36 "armor ratio" 1.55 "range modifier" .98
            } }
        }
    }"#).unwrap();
    for (name, pitch, invalid, group, damage, health) in [
        ("weapon_m4a1_silencer", 0., false, HitGroup::Head, 91, 9),
        ("weapon_m4a1_silencer", 10., false, HitGroup::Chest, 26, 74),
        ("weapon_ak47", 0., false, HitGroup::Head, 111, 0),
        ("weapon_m4a1_silencer", 0., true, HitGroup::Head, 105, 0),
    ] {
        let mut world = world();
        world.catalog = Arc::new(weapons::Catalog { weapons: parsed.weapons.clone() });
        let attacker = world.add_player("shooter", Team::Terrorist);
        let victim = world.add_player("helmeted target", Team::CounterTerrorist);
        teleport(&mut world, attacker, Vec3::new(0., 0., 1.));
        teleport(&mut world, victim, Vec3::new(100., 0., 1.));
        let target = world.player_mut(victim).unwrap();
        target.armor = 100;
        target.helmet = true;
        let mut definition = parsed.get(name).unwrap().clone();
        if invalid { definition.attributes.insert("headshot multiplier".into(), "NaN".into()); }
        let shooter = world.player_mut(attacker).unwrap();
        shooter.weapons = vec![WeaponState::new(Arc::new(definition))];
        shooter.active = 0;
        world.step(&[(attacker, TickInput {
            buttons: buttons::ATTACK, view_angles: Vec3::new(pitch, 0., 0.), ..Default::default()
        })]);
        let hits = world.events.iter().filter_map(|event| match event {
            GameEvent::Damage { attacker: a, victim: v, health, group, .. }
                if *a == attacker && *v == victim => Some((*group, *health)),
            _ => None,
        }).collect::<Vec<_>>();
        assert_eq!(hits, vec![(group, damage)], "{name} pitch={pitch} invalid={invalid}");
        assert_eq!(world.player(victim).unwrap().health, health);
        assert_eq!(world.player(victim).unwrap().alive, health > 0);
        assert_eq!(world.events.iter().filter(|event|
            matches!(event, GameEvent::Fire { player, .. } if *player == attacker)).count(), 1);
    }
}

fn def(name: &str, index: u32, class: WeaponClass, side: Side, price: i32, clip: i32) -> WeaponDefinition {
    WeaponDefinition {
        recoil_trajectory: None,
        recoil_trajectory_unsuppressed: None,
        suppressor_animations: None,
        grenade_launch: None,
        grenade_animations: (class==WeaponClass::Grenade).then_some(weapons::GrenadeAnimations {
            pull_pin_duration:0.25,
            overhand:weapons::GrenadeReleaseAnimation { command_release_time: None,duration:0.4,release_time:0.1},
            underhand:weapons::GrenadeReleaseAnimation { command_release_time: None,duration:0.4,release_time:0.1},
        }),
        id: content::ContentId::csgo(content::ContentKind::Weapon, name.trim_start_matches("weapon_")),
        class_name: name.into(),
        def_index: index,
        name_token: String::new(),
        class,
        side,
        price,
        damage: 35.0,
        armor_ratio: 1.5,
        penetration: 1.0,
        range: 8192.0,
        range_modifier: 0.98,
        cycle_time: 0.1,
        full_auto: true,
        bullets: 1,
        clip_size: clip,
        reserve_ammo: 90,
        max_player_speed: 220.0,
        accuracy: Default::default(),
        recoil: Default::default(),
        view_model: String::new(),
        world_model: String::new(),
        dropped_model: String::new(),
        unsuppressed_fire_sound: String::new(),
        fire_sound: String::new(),
        empty_trigger: None,
        zoom_in_sound: String::new(),
        zoom_out_sound: String::new(),
        kill_award: 300,
        attributes: Default::default(),
    }
}

fn catalog() -> weapons::Catalog {
    weapons::Catalog {
        weapons: vec![
            def("weapon_glock", 4, WeaponClass::Pistol, Side::Terrorist, 200, 20),
            def("weapon_ak47", 7, WeaponClass::Rifle, Side::Terrorist, 2700, 30),
            def("weapon_galilar", 13, WeaponClass::Rifle, Side::Terrorist, 1800, 35),
            def("weapon_m4a1", 16, WeaponClass::Rifle, Side::CounterTerrorist, 3100, 30),
            def("weapon_hkp2000", 32, WeaponClass::Pistol, Side::CounterTerrorist, 200, 13),
            def("weapon_knife", 42, WeaponClass::Knife, Side::CounterTerrorist, 0, 0),
            def("weapon_c4", 49, WeaponClass::Equipment, Side::Terrorist, 0, 0),
            def("weapon_knife_t", 59, WeaponClass::Knife, Side::Terrorist, 0, 0),
        ],
    }
}

#[test]
fn stattrak_counts_only_enemy_kills_with_the_original_owners_item() {
    for (victim_team,owner_is_attacker,expected) in [(Team::CounterTerrorist,true,1),(Team::Terrorist,true,0),(Team::CounterTerrorist,false,0)] {
        let mut w=world();
        let attacker=w.add_player("owner",Team::Terrorist);
        let victim=w.add_player("target",victim_team);
        let def=Arc::new(w.catalog.get("weapon_ak47").unwrap().clone());
        let mut gun=WeaponState::new(def);
        gun.cosmetic_owner=Some(if owner_is_attacker{attacker}else{victim});
        gun.cosmetic=Some(weapon::Cosmetic{serial:42,paint:801,seed:1,wear:0.05,stattrak:Some(10),souvenir:None,stickers:[None;4]});
        w.players[0].weapons=vec![gun];w.players[0].active=0;
        w.events.clear();w.kill(1,Some(0),"weapon_ak47",false,None);
        assert_eq!(w.events.iter().filter(|e|matches!(e,GameEvent::StatTrakKill{owner,serial:42} if *owner==attacker)).count(),expected);
        assert_eq!(w.players[0].active_weapon().unwrap().cosmetic.unwrap().stattrak,Some(10+expected as u32));
    }
}

/// A flat 8192² floor at z = 0, T spawn at x = -1000, CT spawn at x = 1000,
/// a bomb site around x = 0, buy zones around each spawn.
fn world() -> World {
    let mut c = CollisionWorld::new();
    let s = 4096.0;
    let surface = c.surface_id("floor");
    c.add_triangle([Vec3::new(-s, -s, 0.0), Vec3::new(s, -s, 0.0), Vec3::new(s, s, 0.0)], contents::SOLID, surface);
    c.add_triangle([Vec3::new(-s, -s, 0.0), Vec3::new(s, s, 0.0), Vec3::new(-s, s, 0.0)], contents::SOLID, surface);
    c.finalize();
    let mut w = World::with_catalog(Arc::new(c), Arc::new(catalog()));
    for (x, team) in [(-1000.0, Team::Terrorist), (1000.0, Team::CounterTerrorist)] {
        for k in 0..5 {
            w.spawns.push(SpawnPoint { origin: Vec3::new(x, k as f32 * 64.0, 1.0), yaw: 0.0, team });
        }
        w.objectives.buy_zones.push(BuyZone {
            team,
            zone: Zone { mins: Vec3::new(x - 300.0, -300.0, -10.0), maxs: Vec3::new(x + 300.0, 600.0, 200.0) },
        });
    }
    w.objectives.bomb_sites.push(BombSite {
        name: 'A',
        zone: Zone { mins: Vec3::new(-200.0, -200.0, -10.0), maxs: Vec3::new(200.0, 200.0, 200.0) },
    });
    w
}

fn rules() -> RulesConfig {
    RulesConfig { freeze_time: 1.0, restart_delay: 1.0, ..Default::default() }
}

#[test]
fn disabled_team_spawns_stay_out_of_initial_placement_in_every_mode() {
    let mut map = bsp::Bsp::default();
    let mut expected = Vec::new();
    for (class, x, team) in [
        ("info_player_terrorist", -1000., Team::Terrorist),
        ("info_player_counterterrorist", 1000., Team::CounterTerrorist),
    ] {
        for k in 0..5 {
            // Missing enabled retains the ordinary default. Names and group /
            // priority metadata do not themselves disable a spawn in any mode.
            let enabled = if k == 0 { "" } else { "enabled 1" };
            let y = k as f32 * 64.;
            let doc = kv::parse(&format!(r#"spawn {{ classname {class} origin "{x} {y} 1"
                angles "0 90 0" {enabled} targetname spawnpoints.2v2
                spawngroup alternate spawnpriority 2 }}"#)).unwrap();
            map.entities.push(doc.block("spawn").unwrap().clone());
            expected.push(SpawnPoint { origin: Vec3::new(x, y, 1.), yaw: 90., team });
        }
        // This candidate has valid geometry but is outside the ordinary buy
        // zone. Disabled metadata, rather than zone size, must exclude it.
        let doc = kv::parse(&format!(r#"spawn {{ classname {class} origin "{x} 2000 1"
            EnAbLeD 0 targetname spawnpoints.guardian }}"#)).unwrap();
        map.entities.push(doc.block("spawn").unwrap().clone());
    }
    for mode in [GameMode::Deathmatch, GameMode::Special, GameMode::Competitive] {
        let mut w = world();
        w.load_spawns(&map);
        assert_eq!(w.spawns, expected);
        for i in 0..5 {
            w.add_player(&format!("T {i}"), Team::Terrorist);
            w.add_player(&format!("CT {i}"), Team::CounterTerrorist);
        }
        w.set_mode(mode, rules());
        w.step(&[]);
        assert_eq!(w.players.len(), 10);
        assert!(w.players.iter().all(|p| p.alive && p.in_buy_zone), "{mode:?}");
        for i in 0..w.players.len() {
            assert!(w.players[..i].iter().all(|p|
                p.movement.origin.distance(w.players[i].movement.origin) > 40.));
        }
        // Cover all offsets through the same placement path used by respawns.
        for tick in 0..w.spawns.len() as u64 {
            w.tick = tick;
            let id = w.players[0].id;
            w.respawn(id);
            let p = w.player(id).unwrap();
            let (mins, maxs) = p.bounds();
            assert!(w.objectives.in_buy_zone(p.team, mins, maxs), "{mode:?} tick {tick}");
        }
    }
}

#[test]
fn grenade_practice_survives_snapshot_and_respawn_without_changing_competitive() {
    let mut w = world();
    let mut c = catalog();
    for (name, index) in [("weapon_flashbang",43), ("weapon_hegrenade",44),
        ("weapon_smokegrenade",45), ("weapon_molotov",46),
        ("weapon_decoy",47), ("weapon_incgrenade",48)] {
        c.weapons.push(def(name,index,WeaponClass::Grenade,Side::Both,200,1));
    }
    w.catalog = Arc::new(c);
    w.rules.grenade_practice = true;
    let human = w.add_player("tester", Team::Terrorist);
    let bot = w.add_bot("bot", Team::CounterTerrorist);
    w.respawn(human);
    w.respawn(bot);
    assert_eq!(w.player(human).unwrap().weapons.iter().filter(|g| g.def.class == WeaponClass::Grenade).count(), 6);
    assert!(!w.player(bot).unwrap().has("weapon_hegrenade"));
    let state: state::WorldState = w.decode(&w.encode_state()).unwrap();
    w.restore(state);
    w.step(&[(human, TickInput { utility: 44, ..Default::default() })]);
    let mut throws = 0;
    for _ in 0..2 {
        // Let deployment/the prior throw cooldown expire with attack released.
        for _ in 0..w.tick_rate { w.step(&[(human, TickInput::default())]); }
        w.step(&[(human, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
        for _ in 0..(w.tick_rate*3/4) {
            w.step(&[(human,TickInput::default())]);
            throws += w.events.iter().filter(|e| matches!(e, GameEvent::UtilityThrown { player, .. } if *player == human)).count();
        }
        assert!(w.player(human).unwrap().has("weapon_hegrenade"));
    }
    assert_eq!(throws, 2);
    assert!(w.player(human).unwrap().active_weapon().unwrap().next_attack > w.time());
    w.respawn(human);
    assert!(w.player(human).unwrap().has("weapon_decoy"));
    w.set_mode(GameMode::Competitive, RulesConfig { grenade_practice: true, ..rules() });
    w.step(&[]);
    assert!(!w.player(human).unwrap().has("weapon_hegrenade"));
}

#[test]
fn grenade_pickups_share_buy_capacity_and_preserve_reachability() {
    for rate in [64, 128] {
        for (carried, incoming, blocked, own_delay, expected) in [
            (vec![43], 44, false, false, true),
            (vec![43], 43, false, false, true),
            (vec![43, 43], 43, false, false, false),
            (vec![43, 44], 44, false, false, false),
            (vec![43, 43, 44, 45], 47, false, false, false),
            (vec![46], 48, false, false, false),
            (vec![48], 46, false, false, false),
            (vec![43], 48, false, false, true),
            (vec![43], 44, true, false, false),
            (vec![43], 44, false, true, false),
        ] {
            let mut w = world();
            w.set_tick_rate(rate);
            let id = w.add_player("collector", Team::Terrorist);
            let mut catalog = catalog();
            for (name, index) in [("weapon_flashbang",43), ("weapon_hegrenade",44),
                ("weapon_smokegrenade",45), ("weapon_molotov",46),
                ("weapon_decoy",47), ("weapon_incgrenade",48)] {
                catalog.weapons.push(def(name, index, WeaponClass::Grenade, Side::Both, 200, 1));
            }
            w.catalog = Arc::new(catalog);
            for index in carried {
                let def = w.catalog.weapons.iter().find(|d| d.def_index == index).unwrap().clone();
                w.players[0].give(WeaponState::new(Arc::new(def)));
            }
            teleport(&mut w, id, Vec3::new(0., 0., 1.));
            let incoming_def = w.catalog.weapons.iter().find(|d| d.def_index == incoming).unwrap().clone();
            let class = incoming_def.class_name.clone();
            let before = w.players[0].weapons.len();
            let selected = w.players[0].active_weapon().unwrap().def.class_name.clone();
            w.new_item(GroundKind::Weapon(WeaponState::new(Arc::new(incoming_def))),
                Vec3::new(30., 0., 1.), 0., if own_delay { id } else { WORLD }, 1.);
            if blocked { wall_at(&mut w, 15.); }
            w.step(&[(id, TickInput::default())]);
            assert_eq!(w.players[0].weapons.len(), before + usize::from(expected),
                "rate={rate} incoming={incoming} blocked={blocked} own_delay={own_delay}");
            assert_eq!(w.items.is_empty(), expected);
            assert_eq!(w.events.iter().filter(|event|
                matches!(event, GameEvent::Pickup { player, item } if *player == id && item == &class)).count(), usize::from(expected));
            assert_eq!(w.players[0].active_weapon().unwrap().def.class_name, selected);
        }
    }
}

#[test]
fn grenade_buy_limits_and_team_economy() {
    let (mut w,t,_)=duel();
    w.step(&[]);
    let mut c=catalog();
    for (name,index) in [("weapon_flashbang",43),("weapon_hegrenade",44),("weapon_smokegrenade",45),("weapon_molotov",46)] {
        c.weapons.push(def(name,index,WeaponClass::Grenade,Side::Both,200,1));
    }
    w.catalog=Arc::new(c);
    w.player_mut(t).unwrap().money=16000;
    assert!(w.buy(t,Purchase::Weapon(43)).is_ok());
    assert!(w.buy(t,Purchase::Weapon(43)).is_ok());
    assert_eq!(w.buy(t,Purchase::Weapon(43)),Err(BuyError::AlreadyOwned));
    assert!(w.buy(t,Purchase::Weapon(45)).is_ok());
    assert_eq!(w.buy(t,Purchase::Weapon(45)),Err(BuyError::AlreadyOwned));
    assert!(w.buy(t,Purchase::Weapon(44)).is_ok());
    assert_eq!(w.buy(t,Purchase::Weapon(46)),Err(BuyError::AlreadyOwned));
    assert_eq!(w.player(t).unwrap().weapons.iter().filter(|g|g.def.class==WeaponClass::Grenade).count(),4);
    // A poor team saves together, even when one surviving player is wealthy.
    for n in 0..4 {w.add_bot(&format!("mate {n}"),Team::Terrorist);}
    w.round.round=3;
    for p in w.players.iter_mut().filter(|p|p.team==Team::Terrorist&&p.id!=t) {p.money=2000;}
    let mut coordinator=crate::tactics::Coordinator::default();coordinator.update(&w,None);
    assert_eq!(coordinator.plans[&Team::Terrorist].economy,crate::tactics::Economy::Eco);
    assert!(coordinator.reserve[&t]>2000);
    // Match point overrides an eco: there may not be another round to buy for.
    w.round.round+=1;w.round.score_ct=w.rules.rounds_to_win()-1;coordinator.update(&w,None);
    assert_eq!(coordinator.plans[&Team::Terrorist].economy,crate::tactics::Economy::Force);
}

/// One T and one CT in a fresh competitive match.
fn duel() -> (World, PlayerId, PlayerId) {
    let mut w = world();
    let t = w.add_player("t", Team::Terrorist);
    let ct = w.add_player("ct", Team::CounterTerrorist);
    w.set_mode(GameMode::Competitive, rules());
    (w, t, ct)
}

fn run(w: &mut World, seconds: f32, inputs: &[(PlayerId, TickInput)]) -> Vec<GameEvent> {
    let mut events = Vec::new();
    for _ in 0..(seconds * w.tick_rate as f32) as u32 {
        w.step(inputs);
        events.extend(w.events.iter().cloned());
    }
    events
}

fn teleport(w: &mut World, id: PlayerId, pos: Vec3) {
    w.player_mut(id).unwrap().movement = PlayerMove::spawn_at(pos);
}

#[test]
fn round_starts_frozen_with_defaults_and_bomb() {
    let (mut w, t, ct) = duel();
    assert_eq!(w.round.phase, Phase::Freeze);
    let tp = w.player(t).unwrap();
    assert!(tp.has("weapon_glock") && tp.has("weapon_knife_t") && tp.has_bomb());
    assert_eq!(tp.money, 800);
    let ctp = w.player(ct).unwrap();
    assert!(ctp.has("weapon_hkp2000") && ctp.has("weapon_knife") && !ctp.has_bomb());
    // Frozen: walking does nothing.
    let start = w.player(ct).unwrap().movement.origin;
    let walk = TickInput { forward_move: 450.0, ..Default::default() };
    run(&mut w, 0.5, &[(ct, walk)]);
    assert!(w.player(ct).unwrap().movement.origin.distance(start) < 1.0);
    let events = run(&mut w, 1.0, &[(ct, walk)]);
    assert!(events.contains(&GameEvent::FreezeEnd));
    assert!(w.player(ct).unwrap().movement.origin.distance(start) > 50.0);
}

#[test]
fn buying_rules() {
    let (mut w, t, ct) = duel();
    w.step(&[]);
    w.player_mut(t).unwrap().money = 5000;
    assert_eq!(w.buy(t, Purchase::Weapon(16)), Err(BuyError::WrongTeam));
    assert_eq!(w.buy(t, Purchase::DefuseKit), Err(BuyError::WrongTeam));
    assert_eq!(w.buy(t, Purchase::Weapon(7)), Ok(()));
    assert_eq!(w.buy(t, Purchase::Weapon(7)), Err(BuyError::AlreadyOwned));
    let p = w.player(t).unwrap();
    assert_eq!(p.money, 2300);
    assert_eq!(p.active_weapon().unwrap().def.class_name, "weapon_ak47");
    assert_eq!(p.weapons[0].def.class_name, "weapon_ak47", "inventory is in slot order");
    assert_eq!(w.buy(t, Purchase::Weapon(13)), Ok(()));
    assert!(!w.player(t).unwrap().has("weapon_ak47"), "a new primary replaces the old one");
    assert_eq!(w.items.len(), 1, "the old primary is dropped");
    assert_eq!(w.player(t).unwrap().money, 500);
    assert_eq!(w.buy(t, Purchase::Kevlar), Err(BuyError::NotEnoughMoney));
    w.player_mut(t).unwrap().money = 1000;
    assert_eq!(w.buy(t, Purchase::Kevlar), Ok(()));
    assert_eq!(w.buy(t, Purchase::Kevlar), Err(BuyError::AlreadyOwned));
    assert_eq!(w.buy(t, Purchase::KevlarHelmet), Ok(()), "helmet only, 350");
    assert_eq!(w.player(t).unwrap().money, 0);
    // Outside the buy zone.
    teleport(&mut w, ct, Vec3::new(0.0, 0.0, 1.0));
    w.step(&[]);
    assert_eq!(w.buy(ct, Purchase::Kevlar), Err(BuyError::NotInBuyZone));
}

#[test]
fn elimination_pays_winner_and_loss_bonus() {
    let (mut w, t, ct) = duel();
    run(&mut w, 1.1, &[]);
    let def = Arc::new(w.catalog.get("weapon_hkp2000").unwrap().clone());
    let (ti, cti) = (w.index_of(t).unwrap(), w.index_of(ct).unwrap());
    w.players[ti].health = 1;
    w.apply_damage(cti, ti, &def, 100.0, HitGroup::Chest, Vec3::ZERO, None);
    let events = run(&mut w, 0.1, &[]);
    assert!(events.contains(&GameEvent::RoundEnd { reason: RoundEndReason::TerroristsEliminated }));
    assert_eq!(w.round.score_ct, 1);
    assert_eq!(w.player(ct).unwrap().money, 800 + 300 + 3250);
    assert_eq!(w.player(t).unwrap().money, 800 + 1900);
    // The dead T dropped the pistol and the bomb.
    assert!(w.dropped_bomb().is_some());
    // Next round: the CT keeps their pistol; the T gets a fresh loadout.
    let events = run(&mut w, 1.1, &[]);
    assert!(events.contains(&GameEvent::RoundStart { round: 2 }));
    assert!(w.player(t).unwrap().alive && w.player(t).unwrap().has_bomb());
    assert!(w.items.is_empty());
}

fn plant(w: &mut World, t: PlayerId) -> Vec<GameEvent> {
    teleport(w, t, Vec3::new(0.0, 0.0, 1.0));
    let mut events = run(w, 0.1, &[(t, TickInput { weapon_slot: 5, ..Default::default() })]);
    let hold = TickInput { buttons: buttons::ATTACK, weapon_slot: 5, ..Default::default() };
    events.extend(run(w, w.rules.plant_time + 0.2, &[(t, hold)]));
    // Let go of the trigger (missing inputs repeat the last one).
    events.extend(run(w, 0.05, &[(t, TickInput::default())]));
    events
}

#[test]
fn plant_and_explode() {
    let (mut w, t, ct) = duel();
    run(&mut w, 1.1, &[]);
    let events = plant(&mut w, t);
    assert!(events.iter().any(|e| matches!(e, GameEvent::PlantStart { .. })));
    assert!(events.iter().any(|e| matches!(e, GameEvent::BombPlanted { site: 'A', .. })));
    assert!(w.bomb.is_some() && !w.player(t).unwrap().has_bomb());
    assert_eq!(w.player(t).unwrap().money, 800 + 300);
    // Far CT, near T: the blast kills the T.
    teleport(&mut w, ct, Vec3::new(3000.0, 0.0, 1.0));
    teleport(&mut w, t, Vec3::new(100.0, 0.0, 1.0));
    let secs = w.rules.c4_timer + 0.1;
    let events = run(&mut w, secs, &[]);
    assert!(events.iter().any(|e| matches!(e, GameEvent::BombExploded { .. })));
    assert!(events.contains(&GameEvent::RoundEnd { reason: RoundEndReason::TargetBombed }));
    assert!(!w.player(t).unwrap().alive, "standing next to the bomb is fatal");
    assert!(w.player(ct).unwrap().alive);
    assert_eq!(w.round.score_t, 1);
}

#[test]
fn defuse_with_and_without_kit() {
    for kit in [false, true] {
        let (mut w, t, ct) = duel();
        run(&mut w, 1.1, &[]);
        plant(&mut w, t);
        teleport(&mut w, t, Vec3::new(-2000.0, 0.0, 1.0));
        w.player_mut(ct).unwrap().defuse_kit = kit;
        teleport(&mut w, ct, Vec3::new(40.0, 0.0, 1.0));
        let look = TickInput { view_angles: Vec3::new(60.0, 180.0, 0.0), buttons: buttons::USE, ..Default::default() };
        let need = w.rules.defuse_duration(kit);
        let before = run(&mut w, need - 0.3, &[(ct, look)]);
        assert!(before.iter().any(|e| matches!(e, GameEvent::DefuseStart { kit: k, .. } if *k == kit)));
        assert!(w.bomb.is_some(), "not done yet");
        let events = run(&mut w, 0.5, &[(ct, look)]);
        assert!(events.contains(&GameEvent::RoundEnd { reason: RoundEndReason::BombDefused }), "kit {kit}");
        // Loss bonus plus the planted bonus.
        assert_eq!(w.player(t).unwrap().money, 800 + 300 + 1900 + 800);
    }
}

#[test]
fn releasing_use_cancels_defuse() {
    let (mut w, t, ct) = duel();
    run(&mut w, 1.1, &[]);
    plant(&mut w, t);
    teleport(&mut w, ct, Vec3::new(40.0, 0.0, 1.0));
    let look = TickInput { view_angles: Vec3::new(60.0, 180.0, 0.0), buttons: buttons::USE, ..Default::default() };
    run(&mut w, 5.0, &[(ct, look)]);
    run(&mut w, 0.1, &[(ct, TickInput::default())]);
    assert!(w.bomb.unwrap().defuser.is_none());
}

#[test]
fn time_runs_out_and_hiding_ts_get_nothing() {
    let (mut w, t, ct) = duel();
    let secs = 1.0 + w.rules.round_time + 0.1;
    let events = run(&mut w, secs, &[]);
    assert!(events.contains(&GameEvent::RoundEnd { reason: RoundEndReason::TargetSaved }));
    assert_eq!(w.player(t).unwrap().money, 800);
    assert_eq!(w.player(ct).unwrap().money, 800 + 3250);
}

#[test]
fn halftime_swaps_sides_and_resets_money() {
    let mut w = world();
    let t = w.add_player("t", Team::Terrorist);
    w.add_player("ct", Team::CounterTerrorist);
    w.set_mode(GameMode::Competitive, RulesConfig { max_rounds: 4, ..rules() });
    let total = 1.0 + w.rules.round_time + 0.1;
    run(&mut w, total, &[]);
    run(&mut w, 1.1, &[]);
    assert_eq!(w.round.round, 2);
    run(&mut w, total, &[]);
    let events = run(&mut w, 1.1, &[]);
    assert!(events.contains(&GameEvent::Halftime));
    assert_eq!(w.player(t).unwrap().team, Team::CounterTerrorist);
    assert_eq!(w.player(t).unwrap().money, 800);
    assert_eq!((w.round.score_t, w.round.score_ct), (2, 0), "scores follow the teams");
    // The second half also goes to the CTs on time: 2-2 after 4 rounds.
    run(&mut w, total, &[]);
    assert_eq!(w.round.phase, Phase::Over);
    run(&mut w, 1.1, &[]);
    let events = run(&mut w, total, &[]);
    assert!(events.contains(&GameEvent::MatchEnd { winner: None }));
    assert_eq!(w.round.phase, Phase::MatchOver);
}

#[test]
fn dropped_bomb_is_picked_up_by_terrorists_only() {
    let mut w = world();
    let t = w.add_player("t", Team::Terrorist);
    let t2 = w.add_player("t2", Team::Terrorist);
    let ct = w.add_player("ct", Team::CounterTerrorist);
    w.set_mode(GameMode::Competitive, rules());
    run(&mut w, 1.1, &[]);
    let carrier = if w.player(t).unwrap().has_bomb() { t } else { t2 };
    let other = if carrier == t { t2 } else { t };
    teleport(&mut w, carrier, Vec3::new(0.0, 1000.0, 1.0));
    run(&mut w, 0.1, &[(carrier, TickInput { weapon_slot: 5, ..Default::default() })]);
    w.step(&[(carrier, TickInput { drop: true, ..Default::default() })]);
    let pos = w.dropped_bomb().expect("bomb on the ground").pos;
    teleport(&mut w, ct, pos + Vec3::Z);
    run(&mut w, 0.2, &[]);
    assert!(!w.player(ct).unwrap().has_bomb());
    teleport(&mut w, other, pos + Vec3::Z);
    run(&mut w, 0.2, &[]);
    assert!(w.player(other).unwrap().has_bomb());
}

#[test]
fn slot_switching_and_last_weapon() {
    let (mut w, t, _) = duel();
    let tap = |slot| TickInput { weapon_slot: slot, ..Default::default() };
    w.step(&[(t, tap(3))]);
    assert_eq!(w.player(t).unwrap().active_weapon().unwrap().def.class_name, "weapon_knife_t");
    w.step(&[(t, tap(5))]);
    assert!(w.player(t).unwrap().active_weapon().unwrap().def.class_name == C4);
    w.step(&[(t, tap(SLOT_LAST))]);
    assert_eq!(w.player(t).unwrap().active_weapon().unwrap().def.class_name, "weapon_knife_t");
    // Holding a slot key (bots send it every tick) does not cycle.
    w.step(&[(t, tap(2))]);
    w.step(&[(t, tap(2))]);
    assert_eq!(w.player(t).unwrap().active_weapon().unwrap().def.class_name, "weapon_glock");
}

fn wall_at(w: &mut World, x: f32) {
    let collision = Arc::get_mut(&mut w.collision).unwrap();
    let surface = collision.surface_id("test_wall");
    let a = Vec3::new(x, -200.0, 0.0);
    let b = Vec3::new(x, 200.0, 0.0);
    let c = Vec3::new(x, 200.0, 160.0);
    let d = Vec3::new(x, -200.0, 160.0);
    collision.add_triangle([a, b, c], contents::SOLID, surface);
    collision.add_triangle([a, c, d], contents::SOLID, surface);
    collision.finalize();
}

#[test]
fn defusing_requires_an_unobstructed_bomb() {
    let (mut w, t, ct) = duel();
    run(&mut w, 1.1, &[]);
    plant(&mut w, t);
    teleport(&mut w, ct, Vec3::new(40.0, 0.0, 1.0));
    let use_bomb = TickInput { view_angles: Vec3::new(60.0, 180.0, 0.0), buttons: buttons::USE, ..Default::default() };
    run(&mut w, 0.1, &[(ct, use_bomb)]);
    assert!(w.bomb.unwrap().defuser.is_some());
    // A newly obstructed defuse cancels; holding use cannot restart it.
    wall_at(&mut w, 20.0);
    let events = run(&mut w, 11.0, &[(ct, use_bomb)]);
    assert!(w.bomb.unwrap().defuser.is_none());
    assert!(!events.iter().any(|e| matches!(e, GameEvent::BombDefused { .. } | GameEvent::DefuseStart { .. })));
}

#[test]
fn airborne_defuser_loses_progress() {
    let (mut w, t, ct) = duel();
    run(&mut w, 1.1, &[]);
    plant(&mut w, t);
    teleport(&mut w, ct, Vec3::new(40.0, 0.0, 1.0));
    let use_bomb = TickInput { view_angles: Vec3::new(60.0, 180.0, 0.0), buttons: buttons::USE, ..Default::default() };
    run(&mut w, 0.1, &[(ct, use_bomb)]);
    assert!(w.bomb.unwrap().defuser.is_some());
    teleport(&mut w, ct, Vec3::new(40.0, 0.0, 20.0));
    w.step(&[(ct, use_bomb)]);
    assert!(w.bomb.unwrap().defuser.is_none());
}

#[test]
fn interrupted_plant_requires_a_fresh_countdown() {
    let (mut w, t, _) = duel();
    run(&mut w, 1.1, &[]);
    teleport(&mut w, t, Vec3::new(0.0, 0.0, 1.0));
    let hold = TickInput { weapon_slot: 5, buttons: buttons::ATTACK, ..Default::default() };
    run(&mut w, 2.0, &[(t, hold)]);
    assert!(w.player(t).unwrap().plant_start.is_some());
    w.step(&[(t, TickInput::default())]);
    assert!(w.player(t).unwrap().plant_start.is_none());
    run(&mut w, 2.0, &[(t, hold)]);
    assert!(w.bomb.is_none());
    run(&mut w, 1.2, &[(t, hold)]);
    assert!(w.bomb.is_some());
}

#[test]
fn planted_bomb_keeps_round_live_after_last_terrorist_dies() {
    let (mut w, t, ct) = duel();
    run(&mut w, 1.1, &[]);
    plant(&mut w, t);
    let ti = w.index_of(t).unwrap();
    w.kill(ti, None, "test", false, None);
    // The normal round timer can expire while the bomb is ticking.
    w.rules.round_time = 1.0;
    let events = run(&mut w, 0.1, &[]);
    assert_eq!(w.round.phase, Phase::Live);
    assert!(!events.iter().any(|e| matches!(e, GameEvent::RoundEnd { .. })));
    teleport(&mut w, ct, Vec3::new(3000.0, 0.0, 1.0));
    let seconds = w.rules.c4_timer;
    let events = run(&mut w, seconds, &[]);
    assert!(events.contains(&GameEvent::RoundEnd { reason: RoundEndReason::TargetBombed }));
}

#[test]
fn dropped_gun_cancels_reload_and_cannot_be_picked_up_through_wall() {
    let (mut w, t, ct) = duel();
    run(&mut w, 1.1, &[]);
    teleport(&mut w, t, Vec3::new(-40.0, 0.0, 1.0));
    let ti = w.index_of(t).unwrap();
    let pistol = w.players[ti].weapon_in_slot(2).unwrap();
    w.players[ti].weapons[pistol].clip = 2;
    w.players[ti].weapons[pistol].reload_end = Some(w.time() + 0.5);
    w.drop_weapon(ti, pistol, 0.0);
    teleport(&mut w, t, Vec3::new(-1000.0, 0.0, 1.0));
    assert!(matches!(&w.items[0].kind, GroundKind::Weapon(gun) if gun.reload_end.is_none() && gun.clip == 2));
    let cti = w.index_of(ct).unwrap();
    w.players[cti].weapons.retain(|gun| weapon::slot(&gun.def) != 2);
    let pos = w.items[0].pos;
    teleport(&mut w, ct, pos + Vec3::new(40.0, 0.0, 0.0));
    wall_at(&mut w, pos.x + 20.0);
    run(&mut w, 1.0, &[]);
    assert!(!w.player(ct).unwrap().has("weapon_glock"));
    assert_eq!(w.items.len(), 1);
    teleport(&mut w, ct, pos);
    run(&mut w, 0.1, &[]);
    assert!(w.player(ct).unwrap().has("weapon_glock"));
    assert!(w.items.is_empty());
}



#[test]
fn shotgun_reload_loads_shells_and_firing_interrupts_it() {
    let mut world = world();
    let id = world.add_player("shotgunner", Team::CounterTerrorist);
    let mut nova = def("weapon_nova", 35, WeaponClass::Shotgun, Side::Both, 1050, 8);
    nova.full_auto = false;
    nova.cycle_time = 0.88;
    let player = world.player_mut(id).unwrap();
    player.weapons.insert(0, WeaponState::new(Arc::new(nova)));
    player.active = 0;
    player.weapons[0].clip = 0;
    // Start the explicit reload immediately; idling empty now auto-reloads.
    let reload = TickInput { buttons: buttons::RELOAD, ..Default::default() };
    world.step(&[(id, reload)]);
    let s = world.player(id).unwrap().active_weapon().unwrap().shell_reload().unwrap();
    // Partway: the start plus three inserts loads three shells.
    run(&mut world, s.start + 3.2 * s.insert, &[(id, TickInput::default())]);
    let w = world.player(id).unwrap().active_weapon().unwrap();
    assert_eq!(w.clip, 3);
    assert!(w.reload_end.is_some());
    // Pulling the trigger fires a loaded shell and abandons the reload.
    let fire = TickInput { buttons: buttons::ATTACK, ..Default::default() };
    let events = run(&mut world, 0.05, &[(id, fire)]);
    assert!(events.iter().any(|e| matches!(e, GameEvent::Fire { .. })));
    let w = world.player(id).unwrap().active_weapon().unwrap();
    assert_eq!((w.clip, w.reload_end), (2, None));
}

fn scoped_world() -> (World, PlayerId) {
    let mut world = world();
    let id = world.add_player("scope", Team::CounterTerrorist);
    let mut awp = def("weapon_awp", 9, WeaponClass::Sniper, Side::Both, 4750, 5);
    awp.cycle_time = 1.455;
    awp.full_auto = false;
    awp.accuracy.inaccuracy_stand = 80.8;
    for (key, value) in [
        ("zoom levels", "2"), ("zoom fov 1", "40"), ("zoom fov 2", "10"),
        ("zoom time 0", "0.05"), ("zoom time 1", "0.05"), ("zoom time 2", "0.05"),
        ("hide view model zoomed", "1"), ("unzoom after shot", "1"),
        // The installed shared prefab supplies this, even for the AWP.
        ("cycletime alt", "0.300000"),
        ("max player speed alt", "100"), ("inaccuracy stand alt", "2"),
    ] { awp.attributes.insert(key.into(), value.into()); }
    let player = world.player_mut(id).unwrap();
    player.weapons.insert(0, WeaponState::new(Arc::new(awp)));
    player.active = 0;
    (world, id)
}

#[test]
fn scope_taps_cycle_and_use_scoped_stats() {
    let (mut world, id) = scoped_world();
    let right = TickInput { buttons: buttons::ATTACK2, ..Default::default() };
    world.step(&[(id, right)]);
    assert_eq!(world.events.iter().filter(|e| matches!(e, GameEvent::Scope { .. })).count(), 1);
    assert!(!world.events.iter().any(|e| matches!(e, GameEvent::Fire { .. } | GameEvent::Tracer { .. })));
    let w = world.player(id).unwrap().active_weapon().unwrap();
    assert_eq!((w.zoom_level, w.fov(), w.max_speed(), w.clip), (1, 40.0, 100.0, 5));
    assert!(w.hides_viewmodel());
    assert_eq!(weapon::inaccuracy(w, AccuracyContext { speed: 0.0, on_ground: true, ducked: false }), 2.0);
    for (level, fov) in [(2, 10.0), (0, 90.0)] {
        run(&mut world, 0.4, &[(id, TickInput::default())]);
        world.step(&[(id, right)]);
        let w = world.player(id).unwrap().active_weapon().unwrap();
        assert_eq!((w.zoom_level, w.fov()), (level, fov));
    }
    assert_eq!(world.player(id).unwrap().active_weapon().unwrap().clip, 5);
}

#[test]
fn scope_input_uses_authored_alt_recoil_without_resetting_the_existing_spray() {
    let mut normal = world();
    let id = normal.add_player("scoped recoil", Team::CounterTerrorist);
    let mut aug = def("weapon_aug", 8, WeaponClass::Rifle, Side::CounterTerrorist, 3300, 5);
    aug.recoil.magnitude = 30.;
    for (key, value) in [("zoom levels", "2"), ("zoom fov 1", "45"), ("zoom fov 2", "20"),
        ("recoil angle alt", "0"), ("recoil angle variance alt", "0"),
        ("recoil magnitude alt", "0"), ("recoil magnitude variance alt", "0")] {
        aug.attributes.insert(key.into(), value.into());
    }
    // This synthetic normal-only observation must not leak into the scoped mode.
    aug.recoil_trajectory = Some(weapons::RecoilTrajectory {
        reference: "unscoped-only-fixture".into(),
        bullet_angles: vec![[0., 0.], [-2., 0.], [-4., 0.], [-6., 0.], [-8., 0.]],
        response: None,
    });
    let p = normal.player_mut(id).unwrap();
    p.weapons = vec![WeaponState::new(Arc::new(aug))];
    p.active = 0;
    p.weapons[0].recoil_index = 3.;
    assert_ne!(p.weapons[0].aim_punch(), [0.; 2]); // Warm the normal derived cache.
    let deadline = p.weapons[0].next_attack;
    let mut scoped = normal.clone();
    scoped.step(&[(id, TickInput { buttons: buttons::ATTACK2, ..Default::default() })]);
    normal.step(&[(id, TickInput::default())]);
    let w = scoped.player(id).unwrap().active_weapon().unwrap();
    assert_eq!(w.zoom_level, 1);
    assert_eq!(w.next_attack, deadline);
    assert_eq!(w.recoil_index, normal.player(id).unwrap().active_weapon().unwrap().recoil_index);
    assert!(w.recoil_index > 0., "scope preserves the ongoing spray rather than resetting it");
    for candidate in [&mut normal, &mut scoped] {
        candidate.step(&[(id, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
        assert_eq!(candidate.events.iter().filter(|e| matches!(e, GameEvent::Fire { .. })).count(), 1);
        assert_eq!(candidate.player(id).unwrap().active_weapon().unwrap().clip, 4);
    }
    let ray = |world: &World| world.events.iter().find_map(|e| match e {
        GameEvent::Tracer { start, end, .. } => Some((*end - *start).normalize()), _ => None,
    }).unwrap();
    assert!(ray(&scoped).abs_diff_eq(Vec3::X, 0.00001),
        "the real scoped shot uses the authored zero-alt recoil, not the warmed normal observation");
    assert!(ray(&normal).z > 0.01, "the ordinary unscoped control retains its recoil");
}

#[test]
fn bolt_action_unscopes_and_resumes_the_same_zoom() {
    let (mut world, id) = scoped_world();
    world.player_mut(id).unwrap().weapons[0].set_zoom(2, 0.0);
    world.step(&[(id, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
    let w = world.player(id).unwrap().active_weapon().unwrap();
    assert_eq!((w.zoom_level, w.resume_zoom, w.clip), (0, Some(2), 4));
    run(&mut world, 1.0, &[]);
    assert_eq!(world.player(id).unwrap().active_weapon().unwrap().zoom_level, 0);
    run(&mut world, 0.6, &[]);
    assert_eq!(world.player(id).unwrap().active_weapon().unwrap().zoom_level, 2);
}

#[test]
fn bolt_action_zoom_and_trigger_spam_cannot_shorten_primary_cadence() {
    for rate in [64, 128] {
        let (mut world, id) = scoped_world();
        world.set_tick_rate(rate);
        world.player_mut(id).unwrap().weapons[0].set_zoom(2, 0.0);
        let mut shots = Vec::new();
        let mut deadline = 0.;
        let mut scope_changes = 0;
        for tick in 0..rate * 8 {
            let now = world.time();
            // Repeated real press/release edges for both buttons reproduce
            // the owner case; holding a semiautomatic trigger is insufficient.
            let input = TickInput { buttons: if tick % 2 == 0 {
                buttons::ATTACK | buttons::ATTACK2
            } else { 0 }, ..Default::default() };
            // Keep the first shot at zoom 2 rather than cycling it to 0.
            let input = if tick == 0 { TickInput { buttons: buttons::ATTACK, ..input } } else { input };
            world.step(&[(id, input)]);
            scope_changes += world.events.iter().filter(|e| matches!(e, GameEvent::Scope { .. })).count();
            let w = world.player(id).unwrap().active_weapon().unwrap();
            if world.events.iter().any(|e| matches!(e, GameEvent::Fire { .. })) {
                assert!(now >= deadline, "rate={rate}: early shot at {now}, due {deadline}");
                assert!((w.next_attack - now - 1.455).abs() < 0.00001);
                deadline = w.next_attack;
                shots.push(now);
            } else if now < deadline {
                assert_eq!(w.next_attack, deadline, "zoom presses must preserve the shot deadline");
            }
        }
        assert_eq!(shots.len(), 5, "all rounds remain usable at rate={rate}");
        assert!(shots.windows(2).all(|pair| pair[1] - pair[0] >= 1.455));
        assert!(scope_changes > 0, "ready secondary presses still cycle zoom");
    }
}

#[test]
fn bolt_action_resume_precedes_a_ready_shot_and_never_the_primary_deadline() {
    for rate in [64, 128] {
        let (mut world, id) = scoped_world();
        world.set_tick_rate(rate);
        world.player_mut(id).unwrap().weapons[0].set_zoom(2, 0.0);
        world.step(&[(id, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
        let deadline = world.player(id).unwrap().active_weapon().unwrap().next_attack;
        assert_eq!(deadline, 1.455);
        while world.time() < deadline {
            world.step(&[(id, TickInput::default())]);
            let w = world.player(id).unwrap().active_weapon().unwrap();
            assert_eq!((w.zoom_level, w.resume_zoom), (0, Some(2)));
            assert!(!world.events.iter().any(|e| matches!(e, GameEvent::Fire { .. })));
        }
        // On the first ready tick, restore the original zoom before firing.
        // The new shot must unzoom and retain that level for its own resume.
        let now = world.time();
        world.step(&[(id, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
        let w = world.player(id).unwrap().active_weapon().unwrap();
        assert_eq!((w.zoom_level, w.resume_zoom, w.clip), (0, Some(2), 3));
        assert_eq!(w.next_attack, now + 1.455);
        assert_eq!(world.events.iter().filter(|e| matches!(e, GameEvent::Fire { .. })).count(), 1);
    }
}

#[test]
fn scoped_non_bolt_action_retains_alternate_cycle_policy() {
    let (mut world, id) = scoped_world();
    let w = &mut world.player_mut(id).unwrap().weapons[0];
    Arc::make_mut(&mut w.def).attributes.insert("unzoom after shot".into(), "0".into());
    w.set_zoom(1, 0.0);
    world.step(&[(id, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
    let w = world.player(id).unwrap().active_weapon().unwrap();
    assert_eq!((w.zoom_level, w.resume_zoom, w.clip), (1, None, 4));
    assert_eq!(w.next_attack, 0.3);
}

#[test]
fn switching_reloading_dropping_and_death_cancel_scope() {
    for action in 0..4 {
        let (mut world, id) = scoped_world();
        let w = &mut world.player_mut(id).unwrap().weapons[0];
        w.set_zoom(2, 0.0); w.resume_zoom = Some(2); w.clip = 4;
        match action {
            0 => world.step(&[(id, TickInput { weapon_slot: 3, ..Default::default() })]),
            1 => world.step(&[(id, TickInput { buttons: buttons::RELOAD, ..Default::default() })]),
            2 => world.step(&[(id, TickInput { drop: true, ..Default::default() })]),
            _ => world.kill(0, None, "test", false, None),
        }
        for weapon in &world.player(id).unwrap().weapons {
            assert_eq!((weapon.zoom_level, weapon.resume_zoom), (0, None), "action {action}");
        }
        for item in &world.items {
            if let GroundKind::Weapon(w) = &item.kind { assert_eq!((w.zoom_level, w.resume_zoom), (0, None)); }
        }
        run(&mut world, 0.5, &[]);
        assert!(!world.player(id).unwrap().weapons.iter().any(|w| w.zoom_level > 0));
    }
}

#[test]
fn scope_is_unavailable_during_deploy_and_reload() {
    let (mut world, id) = scoped_world();
    let w = &mut world.player_mut(id).unwrap().weapons[0];
    w.next_attack = 1.0;
    let right = TickInput { buttons: buttons::ATTACK2, ..Default::default() };
    run(&mut world, 0.5, &[(id, right)]);
    assert_eq!(world.player(id).unwrap().active_weapon().unwrap().zoom_level, 0);
    let w = &mut world.player_mut(id).unwrap().weapons[0];
    w.next_attack = 0.0; w.reload_end = Some(10.0);
    world.step(&[(id, TickInput::default())]); world.step(&[(id, right)]);
    assert_eq!(world.player(id).unwrap().active_weapon().unwrap().zoom_level, 0);
}

#[test]
fn knife_slash_and_stab_never_emit_gun_feedback() {
    for secondary in [false, true] {
        let mut world = world();
        let id = world.add_player("knife", Team::Terrorist);
        world.step(&[(id, TickInput { weapon_slot: 3, ..Default::default() })]);
        run(&mut world, 0.6, &[]);
        let input = TickInput { buttons: if secondary { buttons::ATTACK2 } else { buttons::ATTACK }, ..Default::default() };
        let events = run(&mut world, 2.0, &[(id, input)]);
        assert!(events.iter().any(|e| matches!(e, GameEvent::Melee { secondary: s, hit: MeleeHit::Miss, .. } if *s == secondary)));
        assert!(!events.iter().any(|e| matches!(e, GameEvent::Fire { .. } | GameEvent::Tracer { .. } | GameEvent::Impact { .. })));
        let w = world.player(id).unwrap().active_weapon().unwrap();
        assert_eq!((w.clip, w.recoil_index, w.accuracy_penalty), (0, 0.0, 0.0));
    }
}

#[test]
fn knife_damage_range_and_wall_occlusion() {
    for (secondary, distance, yaw, armor, expected) in [
        (false, 50.0, 180.0, 0, 40), (true, 40.0, 180.0, 0, 65),
        (true, 50.0, 180.0, 0, 0), (false, 80.0, 180.0, 0, 0),
        (false, 40.0, 180.0, 100, 34), (true, 40.0, 180.0, 100, 55),
        (true, 40.0, 0.0, 0, 180),
    ] {
        let mut world = world();
        let id = world.add_player("knife", Team::Terrorist);
        let victim = world.add_player("target", Team::CounterTerrorist);
        teleport(&mut world, id, Vec3::new(0.0, 0.0, 1.0));
        teleport(&mut world, victim, Vec3::new(distance, 0.0, 1.0));
        let v = world.player_mut(victim).unwrap(); v.armor = armor; v.view_angles.y = yaw;
        let player = world.player_mut(id).unwrap(); player.active = player.weapon_in_slot(3).unwrap();
        world.step(&[(id, TickInput { buttons: if secondary { buttons::ATTACK2 } else { buttons::ATTACK }, ..Default::default() })]);
        let actual: i32 = world.events.iter().filter_map(|e| if let GameEvent::Damage { health, .. } = e { Some(*health) } else { None }).sum();
        assert_eq!(actual, expected, "secondary={secondary}, distance={distance}, yaw={yaw}, armor={armor}");
        assert!(!world.events.iter().any(|e| matches!(e, GameEvent::Tracer { .. } | GameEvent::Fire { .. })));
    }
    let mut world = world();
    let surface = Arc::get_mut(&mut world.collision).unwrap().surface_id("wall");
    let c = Arc::get_mut(&mut world.collision).unwrap();
    c.add_triangle([Vec3::new(20.0, -100.0, 0.0), Vec3::new(20.0, 100.0, 0.0), Vec3::new(20.0, 0.0, 200.0)], contents::SOLID, surface);
    c.finalize();
    let id = world.add_player("knife", Team::Terrorist);
    let victim = world.add_player("target", Team::CounterTerrorist);
    teleport(&mut world, id, Vec3::new(0.0, 0.0, 1.0));
    teleport(&mut world, victim, Vec3::new(45.0, 0.0, 1.0));
    let p = world.player_mut(id).unwrap(); p.active = p.weapon_in_slot(3).unwrap();
    world.step(&[(id, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
    assert!(world.events.iter().any(|e| matches!(e, GameEvent::Melee { hit: MeleeHit::World, .. })));
    assert_eq!(world.player(victim).unwrap().health, 100);
}

#[test]
fn empty_bolt_action_does_not_resume_scope() {
    let (mut world, id) = scoped_world();
    let w = &mut world.player_mut(id).unwrap().weapons[0];
    w.set_zoom(1, 0.0); w.clip = 1;
    world.step(&[(id, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
    run(&mut world, 2.0, &[(id, TickInput::default())]);
    let w = world.player(id).unwrap().active_weapon().unwrap();
    assert_eq!((w.zoom_level, w.resume_zoom, w.clip), (0, None, 0));
}

#[test]
fn holding_scope_does_not_freeze_recoil_recovery() {
    let (mut world, id) = scoped_world();
    world.player_mut(id).unwrap().weapons[0].recoil_index = 3.0;
    run(&mut world, 0.5, &[(id, TickInput { buttons: buttons::ATTACK2, ..Default::default() })]);
    let w = world.player(id).unwrap().active_weapon().unwrap();
    assert_eq!(w.zoom_level, 2);
    assert_eq!(w.recoil_index, 0.0);
    assert_eq!(w.clip, 5);
}

#[test]
fn survival_starts_with_pistol_and_knife_and_zero_cash_and_can_buy_anywhere() {
    let mut w=world();
    w.set_mode(GameMode::Special,rules());
    let id=w.add_player("survivor",Team::CounterTerrorist);
    let p=w.player(id).unwrap();
    assert_eq!(p.money,0);assert_eq!(p.armor,0);assert!(!p.helmet);
    assert_eq!(p.weapons.len(),2);assert_eq!(p.weapons[0].def.class,WeaponClass::Pistol);assert_eq!(p.weapons[1].def.class,WeaponClass::Knife);
    w.tick=64*60*120;
    w.player_mut(id).unwrap().in_buy_zone=false;
    assert_eq!(w.buy_cost(id,Purchase::Kevlar),Err(BuyError::NotEnoughMoney));
    w.player_mut(id).unwrap().money=1000;
    assert_eq!(w.buy_cost(id,Purchase::Kevlar),Ok(rules::KEVLAR_PRICE));
    w.buy(id,Purchase::Kevlar).unwrap();
    assert_eq!(w.player(id).unwrap().money,1000-rules::KEVLAR_PRICE);
    assert_eq!(w.player(id).unwrap().armor,100);
    w.damage_environment(id,1000,"environment");
    assert_eq!(w.buy_cost(id,Purchase::Kevlar),Err(BuyError::NotAllowedNow));
    w.respawn(id);
    assert_eq!(w.player(id).unwrap().weapons.len(),2);
    assert_eq!(w.player(id).unwrap().money,1000-rules::KEVLAR_PRICE);
}

#[test]
fn dynamic_mob_hits_preserve_bullet_and_knife_contact_positions() {
    struct Creature(std::sync::Mutex<Vec<(Vec3,Vec3)>>);
    impl collision::DynamicWorld for Creature {
        fn trace_box(&self,_:Vec3,end:Vec3,_:Vec3,_:Vec3,_:i32)->collision::Trace{collision::Trace::empty(end)}
        fn point_contents(&self,_:Vec3,_:i32)->i32{0}
        fn hit_target(&self,_:Vec3,_:Vec3)->Option<(u64,f32)>{Some((42,0.5))}
        fn damage_target_at(&self,_:u64,_:f32,from:Vec3,point:Vec3){self.0.lock().unwrap().push((from,point));}
    }
    let creature=Arc::new(Creature(Default::default()));
    let mut c=CollisionWorld::new();c.dynamic=Some(creature.clone());
    let mut w=World::with_catalog(Arc::new(c),Arc::new(catalog()));
    w.add_player("player",Team::CounterTerrorist);
    w.players[0].view_angles=Vec3::ZERO;
    let knife=w.players[0].weapons.iter().position(|w|w.def.class==WeaponClass::Knife).unwrap();w.players[0].active=knife;
    w.melee_attack(0,1.,false);
    let (from,point)=creature.0.lock().unwrap()[0];
    assert!((point-from-Vec3::X*MELEE_RANGE*0.5).length()<0.001);
    w.give_weapon(w.players[0].id,"weapon_hkp2000");
    w.players[0].active=w.players[0].weapons.iter().position(|w|w.def.class==WeaponClass::Pistol).unwrap();
    w.fire(0,2.,None);
    let hits=creature.0.lock().unwrap();assert_eq!(hits.len(),2);
    let (from,point)=hits[1];
    assert!(point.is_finite() && (point-from).length()>100.);
    assert!(w.events.iter().any(|e|matches!(e,GameEvent::Tracer{end,..} if (*end-point).length()<0.001)));
}

mod authority;
mod recoil;
mod recoil_response;
mod fire_cadence;
mod empty_trigger;
mod aim;
mod rewind;
mod death;
mod movement_fidelity;

#[path = "tests/suppressors.rs"]
mod suppressors;
#[path = "tests/burst.rs"]
mod burst;

#[path = "tests/scope_hold.rs"]
mod scope_hold;

#[path = "tests/model_hitboxes.rs"]
mod model_hitboxes;

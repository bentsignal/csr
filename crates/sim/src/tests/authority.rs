//! Server authority: determinism, the replicated state, tick rates and the
//! VR seam.

use super::*;

/// A competitive 2v2 with two humans and two bots.
fn bot_match() -> (World, PlayerId, PlayerId) {
    let mut w = world();
    let t = w.add_player("t", Team::Terrorist);
    let ct = w.add_player("ct", Team::CounterTerrorist);
    w.add_bot("bot t", Team::Terrorist);
    w.add_bot("bot ct", Team::CounterTerrorist);
    w.bots = bots::Bots::with_skill(0.8);
    w.set_mode(GameMode::Competitive, rules());
    (w, t, ct)
}

/// Scripted human input for tick `n`: strafing, turning, jumping, firing,
/// switching and buying, so most of the step runs.
pub(super) fn scripted(n: u64, id: PlayerId) -> TickInput {
    let phase = (n / 40 + id as u64) % 6;
    TickInput {
        view_angles: Vec3::new(((n % 90) as f32 - 45.0) * 0.3, (n as f32 * 1.7 + id as f32 * 90.0) % 360.0, 0.0),
        forward_move: if phase < 3 { 450.0 } else { -200.0 },
        side_move: if phase % 2 == 0 { 450.0 } else { -450.0 },
        buttons: match phase {
            0 => buttons::ATTACK,
            1 if n % 50 == 0 => buttons::JUMP,
            2 => buttons::DUCK | buttons::ATTACK,
            4 if n % 30 == 0 => buttons::RELOAD,
            _ => 0,
        },
        weapon_slot: if n % 97 == 0 { 3 } else if n % 89 == 0 { 2 } else { 0 },
        buy: if n == 5 { Purchase::Weapon(7) } else if n == 6 { Purchase::Kevlar } else { Purchase::None },
        ..Default::default()
    }
}

#[test]
fn cloned_worlds_with_bots_step_identically() {
    let (mut a, t, ct) = bot_match();
    for n in 0..200 {
        a.step(&[(t, scripted(n, t)), (ct, scripted(n, ct))]);
    }
    let mut b = a.clone();
    let start = a.state_hash();
    for n in 200..1500 {
        let inputs = [(t, scripted(n, t)), (ct, scripted(n, ct))];
        a.step(&inputs);
        b.step(&inputs);
        assert_eq!(a.state_hash(), b.state_hash(), "diverged at tick {n}");
    }
    assert_ne!(a.state_hash(), start, "the match must actually progress");
    assert!(a.players.iter().all(|p| p.movement.origin.is_finite()));
}

#[test]
fn different_inputs_give_different_hashes() {
    let (mut a, t, _) = bot_match();
    let mut b = a.clone();
    a.step(&[(t, TickInput { view_angles: Vec3::new(0.0, 10.0, 0.0), ..Default::default() })]);
    b.step(&[(t, TickInput { view_angles: Vec3::new(0.0, 11.0, 0.0), ..Default::default() })]);
    assert_ne!(a.state_hash(), b.state_hash());
}

#[test]
fn restored_state_steps_identically() {
    // Everything that can be in flight: guns with finishes, a ground item,
    // a grenade, a smoke, a planted bomb.
    let (mut a, t, ct) = duel();
    run(&mut a, 1.1, &[]);
    let mut loadout = Loadout::default();
    loadout.skins.insert(
        "weapon_glock".into(),
        weapon::Cosmetic { serial: 7, paint: 38, seed: 3, wear: 0.2, stattrak: Some(4), souvenir: None, stickers: [Some((1, 0.0)), None, None, None] },
    );
    a.set_loadout(t, loadout);
    plant(&mut a, t);
    let ti = a.index_of(t).unwrap();
    let pistol = a.players[ti].weapon_in_slot(2).unwrap();
    a.drop_weapon(ti, pistol, 30.0);
    let now = a.time();
    a.projectiles.push(utility::Projectile { id: 900, owner: ct, kind: utility::Kind::He, pos: Vec3::new(0.0, 300.0, 64.0), velocity: Vec3::new(200.0, 0.0, 100.0), born: now, decoy_weapon: None });
    a.utility_fields.push(utility::Field { id: 901, owner: ct, kind: utility::Kind::Smoke, pos: Vec3::new(500.0, 0.0, 0.0), born: now, until: now + 18.0, decoy_weapon: None });
    assert!(a.bomb.is_some() && !a.items.is_empty());

    let bytes = a.encode_state();
    let mut b = world();
    let state: WorldState = b.decode(&bytes).expect("state decodes");
    b.restore(state);
    assert_eq!(b.state(), a.state());
    assert_eq!(b.state_hash(), a.state_hash());
    for n in 0..(a.tick_rate as u64 * 6) {
        let inputs = [(t, scripted(n, t)), (ct, scripted(n, ct))];
        a.step(&inputs);
        b.step(&inputs);
        assert_eq!(a.state_hash(), b.state_hash(), "diverged {n} ticks after restoring");
        assert_eq!(a.events, b.events);
    }
}

#[test]
fn snapshots_and_inputs_round_trip() {
    let (mut w, t, ct) = duel();
    run(&mut w, 1.1, &[(t, scripted(3, t))]);
    w.step(&[(ct, TickInput { buttons: buttons::ATTACK, view_angles: Vec3::new(0.0, 180.0, 0.0), ..Default::default() })]);
    let snap = w.snapshot();
    let back: Snapshot = w.decode(&state::encode(&snap)).unwrap();
    assert_eq!(back, snap);
    let input = TickInput {
        tracked: Some(TrackedPose { head: Pose { position: Vec3::Z * 60.0, rotation: glam::Quat::from_rotation_z(0.3) }, ..Default::default() }),
        aim: Some(AimRay { origin: Vec3::new(10.0, -8.0, 50.0), direction: Vec3::X }),
        ..scripted(9, t)
    };
    assert_eq!(w.decode::<TickInput>(&state::encode(&input)).unwrap(), input);
    // An unknown weapon is an error, not a silently different gun.
    let mut other = world();
    other.catalog = Arc::new(weapons::Catalog { weapons: Vec::new() });
    assert!(other.decode::<WorldState>(&w.encode_state()).is_err());
}

#[test]
fn firing_snapshot_round_trip_ignores_derived_recoil_cache() {
    let (mut a, shooter, _) = aim_setup();
    let mut definitions = a.catalog.weapons.clone();
    let rifle = definitions.iter_mut().find(|def| def.class_name == "weapon_ak47").unwrap();
    rifle.recoil.magnitude = 30.0;
    a.player_mut(shooter).unwrap().active_weapon_mut().unwrap().set_def(Arc::new(rifle.clone()));
    a.catalog = Arc::new(weapons::Catalog { weapons: definitions });
    a.step(&[(shooter, TickInput { buttons: buttons::ATTACK, ..Default::default() })]);
    let gun = a.player(shooter).unwrap().active_weapon().unwrap();
    assert!(gun.recoil_index > 0.0);
    let punch = gun.aim_punch(); // Populate the server's derived cache.
    assert!(punch[0] < 0.0, "the snapshot contains active recoil");
    let snap = a.snapshot();
    let decoded: Snapshot = a.decode(&state::encode(&snap)).unwrap();
    assert_eq!(decoded, snap, "cache population must not change state equality");

    let mut b = world();
    b.catalog = a.catalog.clone();
    b.restore(decoded.state);
    assert_eq!(b.player(shooter).unwrap().active_weapon().unwrap().aim_punch(), punch);
    for _ in 0..a.ticks(0.5) {
        let inputs = [(shooter, TickInput { buttons: buttons::ATTACK, ..Default::default() })];
        a.step(&inputs);
        b.step(&inputs);
        assert_eq!(a.state(), b.state());
        assert_eq!(a.events, b.events);
    }
}

#[test]
fn same_tick_drop_keeps_the_original_owners_finish() {
    for scenario in ["spawn", "respawn", "buy"] {
        let mut w = world();
        let owner = w.add_player("owner", Team::Terrorist);
        let mut loadout = Loadout::default();
        loadout.skins.insert("weapon_ak47".into(), weapon::Cosmetic {
            serial: 42, paint: 44, seed: 1, wear: 0.2,
            stattrak: Some(0), souvenir: None, stickers: [None; 4],
        });
        w.set_loadout(owner, loadout);
        if scenario == "respawn" { w.respawn(owner); }
        let buy = if scenario == "buy" {
            w.set_mode(GameMode::Competitive, rules());
            w.player_mut(owner).unwrap().money = 8000;
            Purchase::Weapon(7)
        } else { Purchase::None };
        w.step(&[(owner, TickInput { buy, drop: true, ..Default::default() })]);
        let dropped = w.items.iter().find_map(|item| match &item.kind {
            GroundKind::Weapon(gun) if gun.def.class_name == "weapon_ak47" => Some(gun),
            _ => None,
        }).expect("the rifle was dropped");
        assert_eq!(dropped.cosmetic_owner, Some(owner), "{scenario}");
        assert_eq!(dropped.cosmetic.map(|skin| skin.serial), Some(42), "{scenario}");
    }
}

/// Measurements that must not depend on the tick rate: run speed, jump
/// height, recoil after a short spray, firing-inaccuracy decay, recoil
/// recovery time and reload time.
fn feel(rate: u32) -> [f32; 6] {
    let mut w = world();
    w.set_tick_rate(rate);
    let id = w.add_player("p", Team::CounterTerrorist);
    teleport(&mut w, id, Vec3::new(0.0, 0.0, 1.0));
    run(&mut w, 0.5, &[]);
    let walk = TickInput { forward_move: 450.0, ..Default::default() };
    run(&mut w, 0.5, &[(id, walk)]);
    let speed = w.player(id).unwrap().movement.speed_2d();
    run(&mut w, 1.0, &[(id, TickInput::default())]);
    let z0 = w.player(id).unwrap().movement.origin.z;
    let mut apex = z0;
    w.step(&[(id, TickInput { buttons: buttons::JUMP, ..Default::default() })]);
    for _ in 0..w.ticks(1.0) {
        w.step(&[(id, TickInput::default())]);
        apex = apex.max(w.player(id).unwrap().movement.origin.z);
    }
    let mut rifle = w.catalog.get("weapon_m4a1").unwrap().clone();
    rifle.accuracy = weapons::Accuracy {
        spread: 0.6, inaccuracy_stand: 6.41, inaccuracy_crouch: 4.81, inaccuracy_move: 175.06, inaccuracy_jump: 140.76,
        inaccuracy_land: 0.242, inaccuracy_fire: 7.8, recovery_time_stand: 0.368, recovery_time_crouch: 0.305,
    };
    rifle.recoil = weapons::Recoil { angle: 0.0, angle_variance: 70.0, magnitude: 30.0, magnitude_variance: 0.0, seed: 223 };
    let p = w.player_mut(id).unwrap();
    p.weapons.insert(0, WeaponState::new(Arc::new(rifle)));
    p.active = 0;
    run(&mut w, 1.2, &[(id, TickInput::default())]);
    let fire = TickInput { buttons: buttons::ATTACK, ..Default::default() };
    while w.player(id).unwrap().shots < 5 {
        w.step(&[(id, fire)]);
    }
    let gun = w.player(id).unwrap().active_weapon().unwrap();
    let (punch, penalty) = (gun.aim_punch()[0], gun.accuracy_penalty);
    w.step(&[(id, TickInput::default())]);
    let released = w.time();
    run(&mut w, 0.2, &[(id, TickInput::default())]);
    let decayed = w.player(id).unwrap().active_weapon().unwrap().accuracy_penalty / penalty;
    while w.player(id).unwrap().active_weapon().unwrap().recoil_index > 0.0 {
        w.step(&[(id, TickInput::default())]);
    }
    let recovery = w.time() - released;
    w.step(&[(id, TickInput { buttons: buttons::RELOAD, ..Default::default() })]);
    let began = w.time();
    while w.player(id).unwrap().active_weapon().unwrap().reload_end.is_some() {
        w.step(&[(id, TickInput::default())]);
    }
    [speed, apex - z0, punch, decayed, recovery, w.time() - began]
}

#[test]
fn movement_and_weapon_timers_obey_their_tick_rate_contracts() {
    let (a, b) = (feel(64), feel(128));
    let names = ["run speed", "jump height", "recoil after 5 shots", "inaccuracy decay", "recoil recovery (s)", "reload (s)"];
    let tolerance = [2.0, 1.0, 0.05, 0.03, 2.0 / 64.0, 2.0 / 64.0];
    for i in [0, 2, 3, 4, 5] {
        assert!((a[i] - b[i]).abs() <= tolerance[i], "{}: {} at 64 tick, {} at 128 tick", names[i], a[i], b[i]);
    }
    // Native standing jumps differ by about1.17 units between these rates.
    // Compare the measured targets instead of requiring invented equal peaks.
    // The bound covers accumulated binary32 positions at native world heights.
    for (height, observed) in [(a[1], 54.653_687), (b[1], 55.825_32)] {
        assert!((height - observed).abs() < 0.0004, "height{height}, observed{observed}");
    }
}

#[test]
fn round_timers_match_at_64_and_128_tick() {
    let mut ends = Vec::new();
    for rate in [64, 128] {
        let mut w = world();
        w.set_tick_rate(rate);
        w.add_player("t", Team::Terrorist);
        w.add_player("ct", Team::CounterTerrorist);
        w.set_mode(GameMode::Competitive, rules());
        let (mut freeze_end, mut round_end) = (None, None);
        while round_end.is_none() {
            w.step(&[]);
            if w.events.contains(&GameEvent::FreezeEnd) {
                freeze_end = Some(w.time());
            }
            if w.events.iter().any(|e| matches!(e, GameEvent::RoundEnd { .. })) {
                round_end = Some(w.time());
            }
        }
        ends.push((freeze_end.unwrap(), round_end.unwrap()));
    }
    assert!((ends[0].0 - ends[1].0).abs() <= 1.0 / 64.0 + 1e-4, "{ends:?}");
    assert!((ends[0].1 - ends[1].1).abs() <= 1.0 / 64.0 + 1e-3, "{ends:?}");
}

#[test]
fn changing_tick_rate_keeps_the_clock() {
    let mut w = world();
    w.tick = 128 * 30;
    w.set_tick_rate(64);
    assert_eq!((w.tick, w.time()), (64 * 30, 30.0));
    w.set_tick_rate(1);
    assert_eq!(w.tick_rate, *TICK_RATE_RANGE.start());
}

/// A shooter at the origin facing +X and an unarmoured target 300 units
/// along +Y.
fn aim_setup() -> (World, PlayerId, PlayerId) {
    let mut w = world();
    let shooter = w.add_player("vr", Team::Terrorist);
    let target = w.add_player("target", Team::CounterTerrorist);
    teleport(&mut w, shooter, Vec3::new(0.0, 0.0, 1.0));
    teleport(&mut w, target, Vec3::new(0.0, 300.0, 1.0));
    w.player_mut(target).unwrap().armor = 0;
    run(&mut w, 1.2, &[]);
    (w, shooter, target)
}

#[test]
fn hitscan_follows_the_aim_ray_when_present() {
    let looking_away = Vec3::ZERO;
    // Flat: the view decides; looking away misses.
    let (mut w, shooter, target) = aim_setup();
    w.step(&[(shooter, TickInput { view_angles: looking_away, buttons: buttons::ATTACK, ..Default::default() })]);
    assert_eq!(w.player(target).unwrap().health, 100);
    // VR: the same view, with the gun held at the hip pointing at the
    // target's chest, hits.
    let (mut w, shooter, target) = aim_setup();
    let feet = w.player(shooter).unwrap().movement.origin;
    let hand = Vec3::new(8.0, 0.0, 40.0);
    let chest = w.player(target).unwrap().movement.origin + Vec3::Z * 48.0;
    let aim = AimRay { origin: hand, direction: chest - (feet + hand) };
    w.step(&[(shooter, TickInput { view_angles: looking_away, buttons: buttons::ATTACK, aim: Some(aim), ..Default::default() })]);
    assert!(w.player(target).unwrap().health < 100, "the aim ray hit");
    assert_eq!(w.player(shooter).unwrap().view_angles, looking_away, "the head did not turn");
    let muzzle = w.player(shooter).unwrap().movement.origin + hand;
    assert!(w.events.iter().any(|e| matches!(e, GameEvent::Tracer { start, .. } if start.distance(muzzle) < 0.5)));
}

#[test]
fn aim_origin_and_tracked_pose_are_clamped_and_replicated() {
    let (mut w, shooter, _) = aim_setup();
    let tracked = TrackedPose { head: Pose { position: Vec3::Z * 62.0, rotation: glam::Quat::IDENTITY }, ..Default::default() };
    let far = AimRay { origin: Vec3::new(500.0, 0.0, 64.0), direction: Vec3::new(0.0, 2.0, 0.0) };
    w.step(&[(shooter, TickInput { tracked: Some(tracked), aim: Some(far), ..Default::default() })]);
    let p = w.player(shooter).unwrap();
    assert_eq!(p.tracked, Some(tracked));
    let aim = p.aim.unwrap();
    assert!(aim.origin.distance(Vec3::Z * p.movement.view_offset()) <= MAX_AIM_OFFSET + 1e-3);
    assert!((aim.direction - Vec3::Y).length() < 1e-5, "the direction is normalised");
    // Garbage from a client is dropped, not simulated.
    w.step(&[(shooter, TickInput { aim: Some(AimRay { origin: Vec3::NAN, direction: Vec3::X }), ..Default::default() })]);
    assert!(w.player(shooter).unwrap().aim.is_none());
    // Flat players never carry one.
    w.step(&[(shooter, TickInput::default())]);
    let p = w.player(shooter).unwrap();
    assert!(p.aim.is_none() && p.tracked.is_none());
}

fn knife_catalog() -> weapons::Catalog {
    let mut c = catalog();
    c.weapons.push(def("weapon_knife_karambit", 507, WeaponClass::Knife, Side::Both, 0, 0));
    c.weapons.push(def("weapon_awp", 9, WeaponClass::Sniper, Side::Both, 4750, 10));
    c
}

#[test]
fn loadout_knife_and_finishes_are_applied_by_the_sim() {
    let mut w = world();
    w.catalog = Arc::new(knife_catalog());
    let id = w.add_player("me", Team::Terrorist);
    let bot = w.add_bot("bot", Team::Terrorist);
    let skin = weapon::Cosmetic { serial: 99, paint: 44, seed: 1, wear: 7.0, stattrak: Some(0), souvenir: None, stickers: [None; 4] };
    let mut loadout = Loadout { knife: Some("weapon_knife_karambit".into()), ..Default::default() };
    loadout.skins.insert("weapon_ak47".into(), skin);
    w.set_loadout(id, loadout.clone());
    w.step(&[]);
    let me = w.player(id).unwrap();
    assert!(me.has("weapon_knife_karambit") && !me.has("weapon_knife_t"));
    let ak = me.weapons.iter().find(|g| g.def.class_name == "weapon_ak47").unwrap();
    assert_eq!(ak.cosmetic.map(|c| (c.serial, c.wear)), Some((99, 1.0)), "wear is clamped");
    assert!(me.weapons.iter().all(|g| g.cosmetic_owner == Some(id)));
    let b = w.player(bot).unwrap();
    assert!(b.has("weapon_knife_t") && b.weapons.iter().all(|g| g.cosmetic.is_none() && g.cosmetic_owner == Some(bot)));
    // A rifle is not a knife.
    w.set_loadout(id, Loadout { knife: Some("weapon_awp".into()), ..loadout });
    let me = w.player(id).unwrap();
    assert!(me.has("weapon_knife_t") && !me.has("weapon_awp"));
}

#[test]
fn deathmatch_practice_loadout_respects_team_and_slot() {
    let mut w = world();
    w.catalog = Arc::new(knife_catalog());
    let id = w.add_player("me", Team::CounterTerrorist);
    // CT primary: the AWP (either side). CT pistol: the T-only Glock is refused.
    let practice = [None, None, Some("weapon_awp".to_string()), Some("weapon_glock".to_string())];
    w.set_loadout(id, Loadout { practice, ..Default::default() });
    w.respawn(id);
    let me = w.player(id).unwrap();
    assert_eq!(me.active_weapon().unwrap().def.class_name, "weapon_awp");
    assert!(!me.has("weapon_glock") && !me.has("weapon_m4a1_silencer"));
    // Competitive rounds hand out the default pistol only.
    w.set_mode(GameMode::Competitive, rules());
    assert!(!w.player(id).unwrap().has("weapon_awp"));
    // So does the solo mode.
    let mut w = world();
    w.catalog = Arc::new(knife_catalog());
    w.set_mode(GameMode::Special, rules());
    let id = w.add_player("me", Team::CounterTerrorist);
    w.set_loadout(id, Loadout { practice: [None, None, Some("weapon_awp".into()), None], ..Default::default() });
    w.respawn(id);
    assert!(!w.player(id).unwrap().has("weapon_awp"));
}

#[test]
fn a_dropped_skin_keeps_its_owner_and_finish() {
    let (mut w, t, _) = duel();
    let mut loadout = Loadout::default();
    loadout.skins.insert("weapon_glock".into(), weapon::Cosmetic { serial: 5, paint: 1, seed: 0, wear: 0.1, stattrak: None, souvenir: None, stickers: [None; 4] });
    w.set_loadout(t, loadout);
    w.step(&[]);
    let t2 = w.add_player("t2", Team::Terrorist);
    let ti = w.index_of(t).unwrap();
    let pistol = w.players[ti].weapon_in_slot(2).unwrap();
    w.drop_weapon(ti, pistol, 0.0);
    let t2i = w.index_of(t2).unwrap();
    w.players[t2i].weapons.retain(|g| weapon::slot(&g.def) != 2);
    let GroundKind::Weapon(gun) = w.items.remove(0).kind else { panic!("a gun") };
    w.players[t2i].give(gun);
    w.step(&[]);
    let gun = w.player(t2).unwrap().weapons.iter().find(|g| g.def.class_name == "weapon_glock").unwrap();
    assert_eq!((gun.cosmetic_owner, gun.cosmetic.map(|c| c.serial)), (Some(t), Some(5)));
}

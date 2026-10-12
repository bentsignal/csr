use super::*;
use crate::death::{DeathPose, DeathReceipt};

fn pair(rate: u32) -> (World, PlayerId, PlayerId) {
    let mut w = world();
    w.set_tick_rate(rate);
    let attacker = w.add_player("attacker at death", Team::Terrorist);
    let victim = w.add_player("victim at death", Team::CounterTerrorist);
    teleport(&mut w, attacker, Vec3::new(0., 0., 1.));
    teleport(&mut w, victim, Vec3::new(100., 0., 1.));
    w.players[1].armor = 0;
    w.players[1].helmet = false;
    w.players[1].health = 1;
    (w, attacker, victim)
}

fn receipt(w: &World, id: PlayerId) -> &DeathReceipt {
    w.events
        .iter()
        .find_map(|event| match event {
            GameEvent::Kill { victim, death, .. } if *victim == id => Some(death),
            _ => None,
        })
        .expect("authoritative death receipt")
}

fn tracked() -> TrackedPose {
    TrackedPose {
        head: Pose {
            position: Vec3::new(1., 2., 60.),
            rotation: glam::Quat::from_rotation_z(0.25),
        },
        left_hand: Pose {
            position: Vec3::new(20., -12., 48.),
            rotation: glam::Quat::IDENTITY,
        },
        right_hand: Pose {
            position: Vec3::new(24., 12., 50.),
            rotation: glam::Quat::from_rotation_x(-0.5),
        },
    }
}

#[test]
fn environment_freezes_pose_and_identity_before_drops_and_new_lives() {
    for rate in [64, 128] {
        let (mut w, _, victim) = pair(rate);
        w.set_mode(GameMode::Competitive, rules());
        w.tick = rate as u64 * 3 + 7;
        let p = &mut w.players[1];
        p.movement.velocity = Vec3::new(123., -45., 67.);
        p.movement.ducked = true;
        p.movement.duck_amount = 0.75;
        p.movement.on_ground = false;
        p.view_angles = Vec3::new(13., 47., 0.);
        p.tracked = Some(tracked());
        let expected = DeathPose {
            origin: p.movement.origin,
            view_angles: p.view_angles,
            velocity: p.movement.velocity,
            ducked: true,
            duck_amount: 0.75,
            on_ground: false,
            tracked: p.tracked,
        };
        let epoch = p.life_epoch;
        let tick = w.tick;
        w.damage_environment(victim, 1000, "environment");
        let death = receipt(&w, victim).clone();
        assert_eq!(
            death.key,
            crate::death::DeathKey {
                victim,
                life_epoch: epoch,
                tick
            }
        );
        assert_eq!(death.pose, expected);
        assert_eq!(death.victim_name, "victim at death");
        assert_eq!(death.victim_team, Team::CounterTerrorist);
        assert!(death.hit.is_none() && death.attacker.is_none());
        let kill = w
            .events
            .iter()
            .position(|e| matches!(e, GameEvent::Kill { victim: id, .. } if *id == victim))
            .unwrap();
        let drop = w
            .events
            .iter()
            .position(|e| matches!(e, GameEvent::Dropped { player, .. } if *player == victim))
            .unwrap();
        assert!(kill < drop);
        w.player_mut(victim).unwrap().name = "renamed".into();
        w.player_mut(victim).unwrap().team = Team::Terrorist;
        w.respawn(victim);
        assert!(w.player(victim).unwrap().life_epoch > epoch);
        assert_eq!(receipt(&w, victim), &death);
        w.remove_player(victim);
        assert_eq!(receipt(&w, victim), &death);
        w.damage_environment(victim, 1000, "environment");
        assert_eq!(
            w.events
                .iter()
                .filter(|e| matches!(e, GameEvent::Kill { victim: id, .. } if *id == victim))
                .count(),
            1
        );
    }
}

#[test]
fn firearm_preserves_actual_pellet_ray_once_and_prediction_never_publishes_it() {
    for rate in [64, 128] {
        let (mut w, attacker, victim) = pair(rate);
        let mut gun = def("weapon_test", 7, WeaponClass::Rifle, Side::Both, 0, 30);
        gun.bullets = 4;
        w.players[0].weapons = vec![WeaponState::new(Arc::new(gun))];
        w.players[0].active = 0;
        let mut predicted = w.clone();
        let input = TickInput {
            buttons: buttons::ATTACK,
            ..Default::default()
        };
        w.step(&[(attacker, input)]);
        predicted.step_prediction(&[(attacker, input)]);
        let death = receipt(&w, victim);
        assert_eq!(death.key.tick + 1, w.tick);
        assert_eq!(death.attacker.as_ref().unwrap().name, "attacker at death");
        let hit = death.hit.unwrap();
        let (start, end) = w
            .events
            .iter()
            .find_map(|e| match e {
                GameEvent::Tracer { player, start, end } if *player == attacker => {
                    Some((*start, *end))
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(hit.position, end);
        assert!((hit.direction - (end - start).normalize()).length() < 0.00001);
        assert_eq!(
            w.events
                .iter()
                .filter(|e| matches!(e, GameEvent::Kill { .. }))
                .count(),
            1
        );
        assert!(!predicted.player(victim).unwrap().alive);
        assert!(predicted.events.is_empty());
        assert_eq!(predicted.state(), w.state());
    }
}

#[test]
fn melee_keeps_its_real_ray_and_notice_survives_attacker_removal() {
    for rate in [64, 128] {
        let (mut w, attacker, victim) = pair(rate);
        w.players[1].movement.origin.x = 20.;
        let knife = Arc::new(def(
            "weapon_knife",
            42,
            WeaponClass::Knife,
            Side::Both,
            0,
            0,
        ));
        w.players[0].weapons = vec![WeaponState::new(knife)];
        w.players[0].active = 0;
        w.melee_attack(0, 1., true);
        let death = receipt(&w, victim).clone();
        let hit = death.hit.unwrap();
        assert_eq!(hit.direction, Vec3::X);
        assert_eq!(hit.group, HitGroup::Chest);
        assert_eq!(hit.position, w.players[0].eye() + Vec3::X * 4.);
        w.player_mut(attacker).unwrap().name = "new name".into();
        w.player_mut(attacker).unwrap().team = Team::CounterTerrorist;
        w.remove_player(attacker);
        assert_eq!(receipt(&w, victim), &death);
        let actor = death.attacker.unwrap();
        assert_eq!(
            (actor.id, actor.name.as_str(), actor.team),
            (attacker, "attacker at death", Team::Terrorist)
        );
    }
}

#[test]
fn bomb_has_no_invented_ray() {
    for rate in [64, 128] {
        let (mut w, _, victim) = pair(rate);
        w.explode(Vec3::new(100., 0., 33.));
        let death = receipt(&w, victim);
        assert!(death.hit.is_none() && death.attacker.is_none());
    }
}

#[test]
fn lagged_hit_keeps_current_death_pose_instead_of_historical_hull() {
    for rate in [64, 128] {
        let (mut w, _, victim) = pair(rate);
        let gun = Arc::new(def("weapon_test", 7, WeaponClass::Rifle, Side::Both, 0, 30));
        w.players[0].weapons = vec![WeaponState::new(gun)];
        w.players[0].active = 0;
        w.configure_hit_history(rate, 128.).unwrap();
        w.rewind
            .as_mut()
            .unwrap()
            .capture(0, w.round.round, &w.players);
        w.tick = 1;
        w.players[1].movement.origin.y = 80.;
        w.players[1].movement.velocity = Vec3::Y * 200.;
        let current = w.players[1].movement;
        w.rewind
            .as_mut()
            .unwrap()
            .capture(1, w.round.round, &w.players);
        w.fire(0, 1., Some(0));
        let death = receipt(&w, victim);
        assert_eq!(death.key.tick, 1);
        assert_eq!(death.pose.origin, current.origin);
        assert_eq!(death.pose.velocity, current.velocity);
        assert!(death.hit.unwrap().position.y.abs() < 16.);
        assert_eq!(w.player(victim).unwrap().movement, current);
    }
}

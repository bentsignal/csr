use super::*;
use glam::{Quat, Vec3};
use sim::death::{DeathActor, DeathHit};

fn batch() -> EventBatch {
    let mut event = test_death_event();
    if let GameEvent::Kill {
        attacker,
        headshot,
        death,
        ..
    } = &mut event
    {
        *attacker = 2;
        *headshot = true;
        death.attacker = Some(DeathActor {
            id: 2,
            name: "attacker : / name".into(),
            team: Team::Terrorist,
        });
        death.key.tick = 10;
        death.pose.origin = Vec3::new(-0., 1.25, -21.375);
        death.pose.view_angles = Vec3::new(12.5, -83.25, 0.);
        death.pose.velocity = Vec3::new(121.125, -17.75, 1e-20);
        death.pose.ducked = true;
        death.pose.duck_amount = 0.625;
        death.pose.tracked = Some(sim::TrackedPose {
            head: sim::Pose {
                position: Vec3::Z * 63.25,
                rotation: Quat::from_rotation_z(0.125),
            },
            left_hand: sim::Pose {
                position: Vec3::new(24., -18., 48.),
                rotation: Quat::from_rotation_x(-0.5),
            },
            right_hand: sim::Pose {
                position: Vec3::new(32., 12., 50.),
                rotation: Quat::from_rotation_y(0.75),
            },
        });
        death.hit = Some(DeathHit {
            direction: Vec3::new(1., 2., 3.).normalize(),
            position: Vec3::new(100., 64., 42.5),
            group: sim::weapon::HitGroup::Head,
        });
    }
    EventBatch {
        tick: 11,
        events: vec![GameEvent::Jump { player: 1 }, event],
    }
}

#[test]
fn exact_receipt_roundtrip_keeps_floats_tracking_and_independent_timestamps() {
    let input = batch();
    validate_events(&input).unwrap();
    let bytes = encode(&input).unwrap();
    let decoded: EventBatch = decode(&bytes).unwrap();
    assert_eq!(decoded, input);
    assert_eq!(encode(&decoded).unwrap(), bytes); // includes -0 and subnormal bits
    let GameEvent::Kill { death, .. } = &decoded.events[1] else {
        panic!()
    };
    assert_eq!(death.key.tick, 10);
    assert_eq!(decoded.tick, 11);
}

#[test]
fn bad_late_receipt_rejects_entire_batch_without_mutating_any_payload() {
    for case in 0..17 {
        let mut invalid = batch();
        let GameEvent::Kill {
            attacker,
            victim,
            headshot,
            death,
            ..
        } = &mut invalid.events[1]
        else {
            panic!()
        };
        match case {
            0 => death.key.tick = 12,
            1 => death.key.tick = MAX_PROTOCOL_COUNTER + 1,
            2 => death.key.life_epoch = 0,
            3 => death.key.victim = *victim + 1,
            4 => death.pose.origin.x = f32::NAN,
            5 => death.pose.velocity = Vec3::splat(f32::MAX),
            6 => death.pose.view_angles.z = f32::INFINITY,
            7 => death.pose.duck_amount = 1.1,
            8 => death.pose.duck_amount = f32::NAN,
            9 => death.victim_name = "v".repeat(65),
            10 => death.victim_name = "line\nbreak".into(),
            11 => death.attacker.as_mut().unwrap().name = "a".repeat(65),
            12 => *attacker = sim::WORLD,
            13 => death.attacker.as_mut().unwrap().id += 1,
            14 => death.hit.as_mut().unwrap().direction = Vec3::ZERO,
            15 => death.hit.as_mut().unwrap().position.z = f32::NAN,
            _ => *headshot = false,
        }
        let before = encode(&invalid).unwrap();
        assert!(validate_events(&invalid).is_err(), "case {case}");
        assert_eq!(encode(&invalid).unwrap(), before);
    }
}

#[test]
fn tracked_receipt_must_already_satisfy_existing_admission_limits() {
    for case in 0..6 {
        let mut invalid = batch();
        let GameEvent::Kill { death, .. } = &mut invalid.events[1] else {
            panic!()
        };
        let pose = death.pose.tracked.as_mut().unwrap();
        match case {
            0 => pose.head.position = Vec3::X * (crate::session::TRACKED_HEAD_LIMIT + 1.),
            1 => pose.left_hand.position = Vec3::Y * (crate::session::TRACKED_HAND_LIMIT + 1.),
            2 => pose.right_hand.position = Vec3::splat(f32::MAX),
            3 => pose.head.rotation = Quat::from_xyzw(0., 0., 0., 0.),
            4 => pose.left_hand.rotation = Quat::from_xyzw(0., 0., 0., 2.),
            _ => pose.right_hand.rotation = Quat::from_xyzw(f32::NAN, 0., 0., 1.),
        }
        assert!(validate_events(&invalid).is_err());
    }
}

#[test]
fn older_protocol_versions_cannot_admit_a_peer_to_movement_state_v34() {
    assert_eq!(VERSION, 34);
    assert_eq!(PROTOCOL_ID, 0x4353_5257_0000_0022);
    let content = ContentManifest {
        map_id: "csgo_map_test".into(),
        map_name: "test".into(),
        map_hash: "0".repeat(40),
        catalog_hash: "1".repeat(40),
    };
    let mut session = crate::session::Session::new(
        sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new())),
        content.clone(),
    )
    .unwrap();
    let before = session.world().state();
    for version in 0..VERSION {
        let reply = session.join(
            version as u64,
            ClientControl::Hello {
                version,
                name: "old".into(),
                team: Team::CounterTerrorist,
                content: content.clone(),
            },
        );
        assert!(matches!(reply, ServerControl::Rejected { .. }));
        assert!(session.world().players.is_empty());
        assert_eq!(session.world().state(), before);
    }
}

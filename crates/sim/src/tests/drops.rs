//! Synthetic ordinary-drop regressions. Tests UNRUN; root owns execution.
use super::*;

fn drop_fixture(rate: u32, height: f32) -> (World, PlayerId, WeaponState) {
    let mut w = world();
    w.set_tick_rate(rate);
    let id = w.add_player("dropper", Team::CounterTerrorist);
    let mut gun = WeaponState::new(Arc::new(w.catalog.get("weapon_m4a1").unwrap().clone()));
    gun.clip = 7;
    gun.reserve = 13;
    gun.cosmetic_owner = Some(id);
    gun.cosmetic = Some(weapon::Cosmetic { serial: 7, paint: 38, seed: 3,
        wear: 0.2, stattrak: Some(4), souvenir: None, stickers: [None; 4] });
    let p = w.player_mut(id).unwrap();
    p.movement = PlayerMove::spawn_at(Vec3::new(0., 0., height));
    p.view_angles = Vec3::ZERO;
    p.weapons = vec![gun.clone()];
    p.active = 0;
    (w, id, gun)
}

fn assert_payload(actual: &WeaponState, expected: &WeaponState) {
    assert_eq!((&actual.def.id, actual.clip, actual.reserve, actual.cosmetic, actual.cosmetic_owner),
        (&expected.def.id, expected.clip, expected.reserve, expected.cosmetic, expected.cosmetic_owner));
}

#[test]
fn unsupported_drop_starts_near_release_falls_lands_and_keeps_pickup_payload_64_128() {
    for rate in [64, 128] {
        let (mut w, id, gun) = drop_fixture(rate, 600.);
        let ahead = Vec3::new(48. * 0.8, 0., 636.);
        assert!(!w.collision.trace_ray(ahead, ahead - Vec3::Z * 512., contents::MASK_PLAYERSOLID).hit());
        w.step(&[(id, TickInput { drop: true, ..Default::default() })]);
        assert!(!w.player(id).unwrap().has("weapon_m4a1"));
        assert_eq!(w.items.len(), 1);
        assert!(w.events.iter().any(|event| matches!(event,
            GameEvent::Dropped { player, pos, .. } if *player == id && *pos == ahead)));
        let placed = w.items[0].clone();
        assert_eq!(placed.pos.truncate(), ahead.truncate());
        assert!(placed.pos.z < ahead.z && placed.pos.z > ahead.z - 1., "first tick must fall from release");
        assert!(!placed.settled && placed.vertical_velocity < 0.);
        assert_eq!((placed.yaw, placed.dropped_by, placed.pickup_after), (0., id, 1.));

        // Keep the dropper away until the existing vertical fall has landed.
        w.player_mut(id).unwrap().movement = PlayerMove::spawn_at(Vec3::new(300., 300., 1.));
        for _ in 0..rate * 2 { w.step(&[]); }
        let landed = &w.items[0];
        assert!(landed.settled && landed.vertical_velocity == 0.);
        assert!((landed.pos.z - 1. - collision::DIST_EPSILON).abs() < 0.001);
        assert_eq!(landed.pos.truncate(), ahead.truncate());
        assert_eq!((landed.id, landed.yaw, landed.dropped_by, landed.pickup_after),
            (placed.id, placed.yaw, placed.dropped_by, placed.pickup_after));
        let GroundKind::Weapon(landed_gun) = &landed.kind else { panic!("gun changed kind") };
        assert_payload(landed_gun, &gun);
        let pos = landed.pos;
        w.step(&[]);
        assert_eq!(w.items[0].pos, pos, "settled drop must not drift");
        w.player_mut(id).unwrap().movement = PlayerMove::spawn_at(pos);
        w.step(&[]);
        assert!(w.items.is_empty());
        assert_payload(w.player(id).unwrap().active_weapon().unwrap(), &gun);
        assert!(w.events.iter().any(|event| matches!(event,
            GameEvent::Pickup { player, item } if *player == id && item == "weapon_m4a1")));
    }
}

#[test]
fn supported_drop_keeps_existing_floor_placement_and_pickup_delay_64_128() {
    for rate in [64, 128] {
        let (mut w, id, _) = drop_fixture(rate, 1.);
        w.step(&[(id, TickInput { drop: true, ..Default::default() })]);
        assert_eq!(w.items.len(), 1);
        let placed = w.items[0].clone();
        assert!((placed.pos - Vec3::new(38.4, 0., 1. + collision::DIST_EPSILON)).length() < 0.001);
        assert!(placed.settled && placed.vertical_velocity == 0.);
        assert!(w.events.iter().any(|event| matches!(event,
            GameEvent::Dropped { pos, .. } if *pos == placed.pos)));
        // The gun is in range, but its original owner must still wait.
        for _ in 0..rate / 2 { w.step(&[]); }
        assert_eq!(w.items[0], placed);
        for _ in 0..rate { w.step(&[]); }
        assert!(w.items.is_empty());
        assert!(w.player(id).unwrap().has("weapon_m4a1"));
    }
}

#[test]
fn start_solid_drop_keeps_existing_anchor_without_depenetration_64_128() {
    for rate in [64, 128] {
        let (mut w, id, _) = drop_fixture(rate, 600.);
        let c = Arc::get_mut(&mut w.collision).unwrap();
        c.add_convex(collision::Convex { plane_surfaces: Vec::new(),
            planes: vec![
                collision::Plane { normal: Vec3::X, dist: 100. },
                collision::Plane { normal: -Vec3::X, dist: 100. },
                collision::Plane { normal: Vec3::Y, dist: 100. },
                collision::Plane { normal: -Vec3::Y, dist: 100. },
                collision::Plane { normal: Vec3::Z, dist: 700. },
                collision::Plane { normal: -Vec3::Z, dist: 0. },
            ],
            mins: Vec3::new(-100., -100., 0.), maxs: Vec3::new(100., 100., 700.),
            contents: contents::SOLID, surface: 0,
        });
        c.finalize();
        let start = Vec3::new(0., 0., 636.);
        let down = w.collision.trace_ray(start, start - Vec3::Z * 512., contents::MASK_PLAYERSOLID);
        assert!(down.start_solid && down.all_solid);
        let expected = down.end + Vec3::Z;
        w.step(&[(id, TickInput { drop: true, ..Default::default() })]);
        assert_eq!(w.items.len(), 1);
        assert_eq!(w.items[0].pos, expected);
        assert!(w.items[0].settled && w.items[0].vertical_velocity == 0.);
    }
}

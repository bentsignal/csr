//! Geometric/command regressions, not proof of native or historical parity.
use super::*;
use collision::{CollisionWorld, Convex, Plane};
use std::cell::RefCell;

struct ObservedTrace<'a> {
    world: &'a CollisionWorld,
    calls: RefCell<Vec<(Vec3, Vec3, Trace)>>,
}
impl TraceWorld for ObservedTrace<'_> {
    fn trace_hull(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32) -> Trace {
        let trace = self.world.trace_box(start, end, mins, maxs, mask);
        self.calls.borrow_mut().push((start, end, trace));
        trace
    }
}

fn bounds(mins: Vec3, maxs: Vec3) -> Convex {
    Convex {
        planes: vec![
            Plane {
                normal: Vec3::X,
                dist: maxs.x,
            },
            Plane {
                normal: -Vec3::X,
                dist: -mins.x,
            },
            Plane {
                normal: Vec3::Y,
                dist: maxs.y,
            },
            Plane {
                normal: -Vec3::Y,
                dist: -mins.y,
            },
            Plane {
                normal: Vec3::Z,
                dist: maxs.z,
            },
            Plane {
                normal: -Vec3::Z,
                dist: -mins.z,
            },
        ],
        mins,
        maxs,
        contents: contents::SOLID,
        surface: 0,
    }
}

fn floor() -> CollisionWorld {
    let mut w = CollisionWorld::new();
    w.add_convex(bounds(
        Vec3::new(-4096., -4096., -64.),
        Vec3::new(4096., 4096., 0.),
    ));
    w.finalize();
    w
}

fn ramp(normal: Vec3) -> Convex {
    let mut brush = bounds(Vec3::splat(-4096.), Vec3::splat(4096.));
    brush.planes.push(Plane { normal, dist: 0. });
    brush
}

fn input(buttons: u32) -> MoveInput {
    MoveInput {
        buttons,
        weapon_max_speed: 250.,
        ..Default::default()
    }
}

#[test]
fn first_supported_command_can_jump_without_an_idle_settling_tick() {
    for rate in [64, 128] {
        let mut pm = PlayerMove::spawn_at(Vec3::Z * 0.0625);
        let events = player_move(
            &mut pm,
            &input(buttons::JUMP),
            &MoveConfig::default(),
            &floor(),
            1. / rate as f32,
        );
        assert!(
            events.jumped,
            "first grounded press consumed at {rate}Hz: {pm:?}"
        );
        assert!(!pm.on_ground);
        assert!(pm.velocity.z > 200.);
        assert!(pm.origin.z > 1.);
        assert_eq!(pm.old_buttons, buttons::JUMP);
    }
}

#[test]
fn first_supported_duck_uses_ground_transition_not_airborne_feet_lift() {
    for rate in [64, 128] {
        let dt = 1. / rate as f32;
        let cfg = MoveConfig::default();
        let mut pm = PlayerMove::spawn_at(Vec3::Z * 0.0625);
        player_move(&mut pm, &input(buttons::DUCK), &cfg, &floor(), dt);
        assert!(pm.on_ground);
        assert!(
            !pm.ducked,
            "first grounded duck lifted the hull at {rate}Hz: {pm:?}"
        );
        assert!((pm.duck_amount - cfg.duck_speed * dt).abs() < 1e-6);
        assert!(pm.origin.z < 0.1);
    }
}

#[test]
fn floor_impact_keeps_incoming_downward_speed_after_slide_clipping() {
    for rate in [64, 128] {
        let dt = 1. / rate as f32;
        let cfg = MoveConfig::default();
        let mut pm = PlayerMove::spawn_at(Vec3::Z * 2.5);
        pm.velocity.z = -400.;
        let incoming = 400. + cfg.gravity * dt * 0.5;
        let events = player_move(&mut pm, &input(0), &cfg, &floor(), dt);
        assert!(pm.on_ground);
        assert!(
            (events.landed.unwrap() - incoming).abs() < 1e-3,
            "lost incoming impact at {rate}Hz: {events:?}"
        );
        assert!((pm.last_fall_speed - incoming).abs() < 1e-3);
        assert!(
            (pm.stamina - (cfg.stamina_land_cost * incoming).min(cfg.stamina_max)).abs() < 1e-3
        );
        assert_eq!(pm.velocity.z, 0.);
        assert!(player_move(&mut pm, &input(0), &cfg, &floor(), dt)
            .landed
            .is_none());
    }
}

#[test]
fn walkable_ramp_preserves_pre_contact_vertical_impact_not_projected_velocity() {
    let mut w = CollisionWorld::new();
    w.add_convex(ramp(Vec3::new(-0.6, 0., 0.8)));
    w.finalize();
    for rate in [64, 128] {
        let dt = 1. / rate as f32;
        let cfg = MoveConfig::default();
        let mut pm = PlayerMove::spawn_at(Vec3::Z * 15.);
        pm.velocity.z = -400.;
        let incoming = 400. + cfg.gravity * dt * 0.5;
        let events = player_move(&mut pm, &input(0), &cfg, &w, dt);
        assert!(pm.on_ground);
        assert!(
            (events.landed.unwrap() - incoming).abs() < 1e-3,
            "ramp projection erased impact at {rate}Hz: {events:?}"
        );
        assert_eq!(pm.velocity.z, 0.);
    }
}

#[test]
fn steep_ramp_contact_slides_without_ground_jump_land_or_stamina() {
    let mut w = CollisionWorld::new();
    w.add_convex(ramp(Vec3::new(-0.8, 0., 0.6)));
    w.finalize();
    for rate in [64, 128] {
        let mut pm = PlayerMove::spawn_at(Vec3::Z * 22.5);
        pm.velocity.z = -400.;
        for _ in 0..8 {
            let events = player_move(
                &mut pm,
                &input(buttons::JUMP),
                &MoveConfig::default(),
                &w,
                1. / rate as f32,
            );
            assert!(!pm.on_ground);
            assert!(!events.jumped);
            assert!(events.landed.is_none());
            assert_eq!(pm.stamina, 0.);
            assert!(pm.velocity.x < 0.);
        }
    }
}

#[test]
fn wall_floor_corner_preserves_crease_motion_and_actual_landing_speed() {
    let mut w = floor();
    w.add_convex(bounds(
        Vec3::new(0., -4096., 0.),
        Vec3::new(64., 4096., 1024.),
    ));
    w.finalize();
    for rate in [64, 128] {
        let dt = 1. / rate as f32;
        let cfg = MoveConfig::default();
        let mut pm = PlayerMove::spawn_at(Vec3::new(-17., 0., 2.5));
        pm.velocity = Vec3::new(400., 128., -400.);
        let incoming = 400. + cfg.gravity * dt * 0.5;
        let events = player_move(&mut pm, &input(0), &cfg, &w, dt);
        assert!((events.landed.unwrap() - incoming).abs() < 1e-3);
        assert!(pm.on_ground);
        assert!(pm.origin.x < -16.);
        assert!((pm.velocity.y - 128.).abs() < 1e-3);
        assert_eq!(pm.velocity.x, 0.);
        assert!(
            !w.trace_box(
                pm.origin,
                pm.origin,
                STAND_MINS,
                STAND_MAXS,
                contents::MASK_PLAYERSOLID
            )
            .start_solid
        );
    }
}

#[test]
fn two_nonorthogonal_steep_contacts_keep_motion_along_their_intersection() {
    let mut w = CollisionWorld::new();
    w.add_convex(ramp(Vec3::new(-0.8, 0., 0.6)));
    w.add_convex(ramp(Vec3::new(0.8, 0., 0.6)));
    w.finalize();
    let cfg = MoveConfig {
        gravity: 0.,
        ..Default::default()
    };
    for rate in [64, 128] {
        let dt = 1. / rate as f32;
        // Both hull support distances are positive but within the existing
        // collision epsilon, making both contacts occur at fraction zero.
        let mut pm = PlayerMove::spawn_at(Vec3::Z * ((0.8 * 16. + 0.02) / 0.6));
        pm.velocity = Vec3::new(100., 200., -400.);
        let events = player_move(&mut pm, &input(0), &cfg, &w, dt);
        assert!(
            (pm.velocity - Vec3::Y * 200.).length() < 1e-3,
            "lost crease at {rate}Hz: {pm:?}"
        );
        assert!((pm.origin.y - 200. * dt).abs() < 1e-3);
        assert!(!pm.on_ground);
        assert!(events.landed.is_none());
    }
}

#[test]
fn nearly_parallel_distinct_planes_are_not_deduplicated() {
    let a = Vec3::new(-0.8, 0.006, 0.6).normalize();
    let b = Vec3::new(-0.8, -0.006, 0.6).normalize();
    let mut w = CollisionWorld::new();
    w.add_convex(ramp(a));
    w.add_convex(ramp(b));
    w.finalize();
    let cfg = MoveConfig {
        gravity: 0.,
        ..Default::default()
    };
    for rate in [64, 128] {
        let mut pm =
            PlayerMove::spawn_at(Vec3::Z * ((a.x.abs() * 16. + a.y.abs() * 16. + 0.02) / a.z));
        let observed = ObservedTrace {
            world: &w,
            calls: RefCell::default(),
        };
        // Drive into both planes, rather than tangentially away from one.
        let incoming = Vec3::new(0., 0., -400.);
        pm.velocity = incoming;
        let events = player_move(&mut pm, &input(0), &cfg, &observed, 1. / rate as f32);
        // This is a feasible two-plane slide, not an opposed-plane crease:
        // the second projection separates slightly from the first plane.
        let expected = Mover::<CollisionWorld>::clip_velocity(
            Mover::<CollisionWorld>::clip_velocity(incoming, a, 1.),
            b,
            1.,
        );
        assert!(
            (pm.velocity - expected).length() < 1e-3,
            "distinct planes lost at {rate}Hz: {pm:?}, traces: {:?}",
            observed.calls.borrow()
        );
        assert!(observed
            .calls
            .borrow()
            .iter()
            .any(|(_, _, tr)| tr.normal == a && tr.fraction < 1.));
        assert!(observed
            .calls
            .borrow()
            .iter()
            .any(|(_, _, tr)| tr.normal == b && tr.fraction < 1.));
        assert!(pm.origin.x < -1.);
        assert!(!pm.on_ground);
        assert!(events.landed.is_none());
    }
}

#[test]
fn actual_all_solid_contact_never_uses_numerical_separation() {
    let mut w = CollisionWorld::new();
    w.add_convex(bounds(Vec3::splat(-100.), Vec3::splat(100.)));
    w.finalize();
    for rate in [64, 128] {
        let mut pm = PlayerMove::spawn_at(Vec3::ZERO);
        pm.velocity = Vec3::new(100., 200., -400.);
        let events = player_move(
            &mut pm,
            &input(0),
            &MoveConfig::default(),
            &w,
            1. / rate as f32,
        );
        assert_eq!(pm.origin, Vec3::ZERO);
        assert_eq!(pm.velocity.truncate(), glam::Vec2::ZERO);
        assert!(!pm.on_ground);
        assert!(events.landed.is_none());
    }
}

fn triangle_ramp() -> CollisionWorld {
    let mut w = CollisionWorld::new();
    let a = Vec3::new(-256., -512., -256. * (4. / 3.));
    let b = Vec3::new(256., -512., 256. * (4. / 3.));
    let c = Vec3::new(256., 512., 256. * (4. / 3.));
    let d = Vec3::new(-256., 512., -256. * (4. / 3.));
    w.add_triangle([a, b, c], contents::SOLID, 0);
    w.add_triangle([a, c, d], contents::SOLID, 0);
    w.finalize();
    w
}

#[test]
fn coplanar_triangle_seam_does_not_snag_surf_motion_and_replays_exactly() {
    let w = triangle_ramp();
    for rate in [64, 128] {
        let observed = ObservedTrace {
            world: &w,
            calls: RefCell::default(),
        };
        let mut pm = PlayerMove::spawn_at(Vec3::new(0., -64., 16. * (4. / 3.) + 0.0625));
        pm.velocity = Vec3::new(0., 500., -40.);
        let mut replay = pm;
        for _ in 0..rate / 4 {
            observed.calls.borrow_mut().clear();
            let events = player_move(
                &mut pm,
                &input(0),
                &MoveConfig::default(),
                &observed,
                1. / rate as f32,
            );
            let reproduced = player_move(
                &mut replay,
                &input(0),
                &MoveConfig::default(),
                &w,
                1. / rate as f32,
            );
            assert_eq!(pm, replay);
            assert_eq!(events, reproduced);
            assert!(!pm.on_ground);
            assert!(events.landed.is_none());
            assert!(
                (pm.velocity.y - 500.).abs() < 1e-3,
                "seam snag at {rate}Hz: {pm:?}, traces: {:?}",
                observed.calls.borrow()
            );
        }
        assert!(pm.origin.y > 0.);
    }
}

#[test]
fn repeated_ramp_contact_still_traces_and_stops_at_a_later_wall() {
    let mut w = triangle_ramp();
    w.add_convex(bounds(
        Vec3::new(-1024., 20., -1024.),
        Vec3::new(1024., 40., 1024.),
    ));
    w.finalize();
    for rate in [64, 128] {
        let mut pm = PlayerMove::spawn_at(Vec3::new(0., -64., 16. * (4. / 3.) + 0.0625));
        pm.velocity = Vec3::new(0., 500., -40.);
        for _ in 0..rate / 4 {
            player_move(
                &mut pm,
                &input(0),
                &MoveConfig::default(),
                &w,
                1. / rate as f32,
            );
            assert!(pm.origin.y < 4.);
            assert!(
                !w.trace_box(pm.origin, pm.origin, STAND_MINS, STAND_MAXS, MASK)
                    .start_solid
            );
        }
        assert!(
            pm.origin.y > 3.,
            "failed to reach actual blocking wall at {rate}Hz: {pm:?}"
        );
        assert_eq!(pm.velocity.y, 0.);
    }
}

#[test]
fn repeated_plane_repair_cannot_separate_through_an_adjacent_solid() {
    let mut w = triangle_ramp();
    let mut pm = PlayerMove::spawn_at(Vec3::new(-0.5504791, -52.28125, 20.651464));
    // This is the actual coplanar ramp's zero-fraction contact, not a fake
    // trace. A second solid leaves the starting hull clear but blocks any
    // outward correction under the collision system's existing margin.
    let tr = w.trace_box(
        pm.origin,
        pm.origin + Vec3::new(0., 4., -1.),
        STAND_MINS,
        STAND_MAXS,
        MASK,
    );
    assert_eq!(tr.fraction, 0.);
    assert!(!tr.start_solid);
    w.add_convex(bounds(
        Vec3::new(-100., -100., -100.),
        Vec3::new(pm.origin.x + STAND_MINS.x - 0.001, 100., 100.),
    ));
    w.finalize();
    assert!(
        !w.trace_box(pm.origin, pm.origin, STAND_MINS, STAND_MAXS, MASK)
            .start_solid
    );
    let before = pm.origin;
    let cfg = MoveConfig::default();
    let mut mover = Mover {
        pm: &mut pm,
        cfg: &cfg,
        world: &w,
        dt: 1. / 128.,
        max_speed: 250.,
        drop_scaffold: false,
        landing_speed: None,
        contact_repair_left: collision::DIST_EPSILON,
    };
    mover.repair_repeated_contact(&tr);
    assert_eq!(mover.pm.origin, before);
    assert_eq!(mover.contact_repair_left, collision::DIST_EPSILON);
}

#[test]
fn genuine_corner_followed_by_repeated_plane_keeps_all_constraints() {
    let a = Vec3::new(-0.8, 0., 0.6);
    let cfg = MoveConfig {
        gravity: 0.,
        ..Default::default()
    };
    let mut witnessed = false;
    for rate in [64, 128] {
        for tilt in [0.003, 0.006, 0.01, 0.1] {
            let b = Vec3::new(0.8, tilt, 0.6).normalize();
            let mut w = CollisionWorld::new();
            w.add_convex(ramp(a));
            w.add_convex(ramp(b));
            w.finalize();
            let observed = ObservedTrace {
                world: &w,
                calls: RefCell::default(),
            };
            for gap in [0.001, 0.01, 0.02, 0.0312] {
                let mut pm = PlayerMove::spawn_at(Vec3::new(0., 16., (16. * 0.8 + gap) / 0.6));
                pm.velocity = Vec3::new(100., 200., -400.);
                for _ in 0..8 {
                    observed.calls.borrow_mut().clear();
                    let previous = pm.velocity;
                    player_move(&mut pm, &input(0), &cfg, &observed, 1. / rate as f32);
                    let calls = observed.calls.borrow();
                    let contacts: Vec<_> = calls
                        .iter()
                        .filter_map(|(_, _, tr)| {
                            (tr.fraction < 1. && !tr.start_solid).then_some(tr.normal)
                        })
                        .collect();
                    witnessed |= contacts
                        .windows(3)
                        .any(|n| n[0] != n[1] && (n[2] == n[0] || n[2] == n[1]));
                    // Bound arithmetic roundoff only, not a permitted inward
                    // gameplay speed: each dot/projection uses a few f32 ops.
                    let rounding = pm.velocity.length() * f32::EPSILON * 8.;
                    assert!(
                        pm.velocity.dot(a) >= -rounding,
                        "inward a: {pm:?}, contacts:{contacts:?}"
                    );
                    assert!(
                        pm.velocity.dot(b) >= -rounding,
                        "inward b: {pm:?}, contacts:{contacts:?}"
                    );
                    assert!(pm.velocity.dot(previous) >= 0., "reversal: {pm:?}");
                    assert!(
                        !w.trace_box(pm.origin, pm.origin, STAND_MINS, STAND_MAXS, MASK)
                            .start_solid
                    );
                }
            }
        }
    }
    assert!(
        witnessed,
        "fixture must exercise a genuine corner then a repeated plane"
    );
}

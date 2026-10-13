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
        assert!(
            player_move(&mut pm, &input(0), &cfg, &floor(), dt)
                .landed
                .is_none()
        );
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
fn coplanar_triangle_seam_does_not_snag_surf_motion_and_replays_exactly() {
    let mut w = CollisionWorld::new();
    let a = Vec3::new(-256., -512., -256. * (4. / 3.));
    let b = Vec3::new(256., -512., 256. * (4. / 3.));
    let c = Vec3::new(256., 512., 256. * (4. / 3.));
    let d = Vec3::new(-256., 512., -256. * (4. / 3.));
    w.add_triangle([a, b, c], contents::SOLID, 0);
    w.add_triangle([a, c, d], contents::SOLID, 0);
    w.finalize();
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

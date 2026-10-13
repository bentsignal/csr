// Clean-room regression from our recorded f32 sweep operands, plus own synthetic obstacles.
use super::*;
const N: Vec3 = Vec3::new(-0.4072467088699341, -0.8021342158317566, 0.4367276132106781);
const D: f32 = -902.36083984375;
const S: Vec3 = Vec3::new(2877.599365234375, -1.6109105348587036, 614.2755126953125);
const E: Vec3 = Vec3::new(2871.753173828125, -6.047452449798584, 600.6754150390625);
fn signed(v: Vec3) -> f64 {
    N.x as f64 * v.x as f64 + N.y as f64 * v.y as f64 + N.z as f64 * v.z as f64 - D as f64
}
fn halfspace(s: Vec3, e: Vec3) -> Trace {
    let mut t = Trace::empty(e);
    clip_convex(
        &[Plane { normal: N, dist: D }],
        s,
        e,
        Vec3::ZERO,
        contents::SOLID,
        4,
        &mut t,
    );
    t
}
#[test]
fn measured_false_entry_is_a_separating_plane() {
    assert_eq!(N.dot(S) - D, 0.03125);
    assert_eq!(N.dot(E) - D, 0.0311279296875);
    assert!(signed(S) > 0. && signed(E) > signed(S));
    let tr = halfspace(S, E);
    assert!(!tr.hit(), "false tangent entry {tr:?}");
    assert_eq!(tr.fraction, 1.);
}
#[test]
fn actual_inward_one_two_seven_remain_contacts() {
    for (s, e) in [
        (
            S,
            Vec3::new(2877.599365234375, -1.6109105348587036, 612.2755126953125),
        ),
        (
            S,
            Vec3::new(2871.787841796875, -5.979032039642334, 600.6381225585938),
        ),
        (
            Vec3::new(2871.753173828125, -6.047501087188721, 600.6754150390625),
            Vec3::new(2871.753173828125, -6.047501087188721, 598.6754150390625),
        ),
    ] {
        assert!(signed(e) < signed(s));
        let tr = halfspace(s, e);
        assert_eq!(tr.fraction, 0.);
        assert_eq!(tr.normal, N);
        assert_eq!(tr.plane_dist, D);
        assert!(!tr.start_solid && !tr.all_solid);
        assert_eq!(tr.surface, 4);
    }
}
#[test]
fn actual_plane_all_solid_is_not_reconsidered() {
    let s = S - N;
    assert!(N.dot(s) - D < 0.);
    let tr = halfspace(s, s);
    assert!(tr.start_solid && tr.all_solid);
    assert_eq!(tr.fraction, 0.);
}
fn wall() -> Convex {
    Convex {
        planes: vec![
            Plane {
                normal: Vec3::X,
                dist: 2874.5,
            },
            Plane {
                normal: -Vec3::X,
                dist: -2874.,
            },
            Plane {
                normal: Vec3::Y,
                dist: 100.,
            },
            Plane {
                normal: -Vec3::Y,
                dist: 100.,
            },
            Plane {
                normal: Vec3::Z,
                dist: 700.,
            },
            Plane {
                normal: -Vec3::Z,
                dist: -500.,
            },
        ],
        mins: Vec3::new(2874., -100., 500.),
        maxs: Vec3::new(2874.5, 100., 700.),
        contents: contents::SOLID,
        surface: 9,
    }
}
fn ramp() -> Convex {
    Convex {
        planes: vec![Plane { normal: N, dist: D }],
        mins: S.min(E) - Vec3::splat(100.),
        maxs: S.max(E) + Vec3::splat(100.),
        contents: contents::SOLID,
        surface: 4,
    }
}
#[test]
fn intervening_wall_blocks_even_with_clear_endpoint_in_both_orders() {
    let mut just_wall = CollisionWorld::new();
    just_wall.add_convex(wall());
    just_wall.finalize();
    assert_eq!(just_wall.point_contents(E, contents::MASK_SOLID), 0);
    let expected = just_wall.trace_ray(S, E, contents::MASK_SOLID);
    assert!(expected.fraction > 0. && expected.fraction < 1.);
    assert_eq!(expected.normal, Vec3::X);
    for ramp_first in [false, true] {
        let mut w = CollisionWorld::new();
        if ramp_first {
            w.add_convex(ramp());
            w.add_convex(wall());
        } else {
            w.add_convex(wall());
            w.add_convex(ramp());
        }
        w.finalize();
        let t = w.trace_ray(S, E, contents::MASK_SOLID);
        assert_eq!(t.fraction, expected.fraction, "order {ramp_first}");
        assert_eq!(t.end, expected.end);
        assert_eq!(t.normal, expected.normal);
        assert_eq!(t.surface, expected.surface);
        assert!(!t.start_solid && !t.all_solid);
    }
}

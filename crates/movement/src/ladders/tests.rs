use super::*;
use collision::{CollisionWorld, Convex, Plane};

fn box_shape(low: Vec3, high: Vec3, contents: i32) -> Convex {
    Convex {
        planes: vec![
            Plane { normal: Vec3::X, dist: high.x }, Plane { normal: -Vec3::X, dist: -low.x },
            Plane { normal: Vec3::Y, dist: high.y }, Plane { normal: -Vec3::Y, dist: -low.y },
            Plane { normal: Vec3::Z, dist: high.z }, Plane { normal: -Vec3::Z, dist: -low.z },
        ], mins: low, maxs: high, contents, surface: 0,
    }
}

fn world() -> CollisionWorld {
    let mut world = CollisionWorld::new();
    world.add_convex(box_shape(Vec3::new(-128., -128., -16.), Vec3::new(128., 128., 0.), contents::SOLID));
    // Actual Legacy ladders carry GRATE, so ordinary solid sweeps also block
    // entry into the brush. Test this rather than an artificial empty trigger.
    world.add_convex(box_shape(Vec3::new(0., -24., 0.), Vec3::new(1., 24., 256.), contents::LADDER | contents::GRATE));
    world.finalize();
    world
}

fn at(z: f32) -> PlayerMove { PlayerMove::spawn_at(Vec3::new(-16. - collision::DIST_EPSILON, 0., z)) }

fn tick(pm: &mut PlayerMove, world: &CollisionWorld, input: MoveInput, rate: u32) -> MoveEvents {
    player_move(pm, &input, &MoveConfig::default(), world, 1. / rate as f32)
}

#[test]
fn brush_ladders_climb_hold_descend_and_leave_at_top_at_both_tick_rates() {
    let world = world();
    for rate in [64, 128] {
        let mut player = at(32.);
        let forward = MoveInput { forward_move: 450., ..Default::default() };
        let event = tick(&mut player, &world, forward, rate);
        assert_eq!(player.ladder_normal, Some(-Vec3::X));
        assert!(player.origin.z > 32. && player.velocity.z > 0.);
        assert!(!player.on_ground && event.landed.is_none());
        let height = player.origin.z;
        tick(&mut player, &world, MoveInput::default(), rate);
        assert_eq!(player.origin.z, height);
        assert_eq!(player.velocity, Vec3::ZERO);
        tick(&mut player, &world, MoveInput { forward_move: -450., ..forward }, rate);
        assert!(player.origin.z < height);
        for _ in 0..rate * 2 {
            tick(&mut player, &world, forward, rate);
            if player.ladder_normal.is_none() { break; }
        }
        assert!(player.origin.z > 256.);
        assert_eq!(player.ladder_normal, None);
        // Its upper cap is a normal solid surface, never a climbable face.
        assert_eq!(world.ladder_normal(Vec3::new(0., 0., 257.), STAND_MINS, STAND_MAXS), None);
    }
}

#[test]
fn ladder_jump_detaches_without_held_jump_recapture() {
    let world = world();
    for rate in [64, 128] {
        let mut player = at(64.);
        tick(&mut player, &world, MoveInput::default(), rate);
        let jump = MoveInput { buttons: buttons::JUMP, ..Default::default() };
        let event = tick(&mut player, &world, jump, rate);
        assert!(event.jumped);
        assert_eq!(player.ladder_normal, None);
        assert!(player.velocity.x < 0.);
        let event = tick(&mut player, &world, jump, rate);
        assert!(!event.jumped);
        assert_eq!(player.ladder_normal, None);
        assert!(player.velocity.z < 0.);
    }
}

#[test]
fn ladder_catch_arrests_a_fall_and_strafing_off_restores_gravity() {
    let world = world();
    let mut player = at(64.);
    player.velocity.z = -600.;
    let event = tick(&mut player, &world, MoveInput::default(), 128);
    assert!(event.landed.is_none());
    assert_eq!(player.velocity, Vec3::ZERO);
    assert_eq!(player.last_fall_speed, 0.);
    let sideways = MoveInput { side_move: 450., ..Default::default() };
    for _ in 0..64 { tick(&mut player, &world, sideways, 128); }
    assert!(player.origin.y < -40.);
    assert_eq!(player.ladder_normal, None);
    assert!(player.velocity.z < 0.);
}

#[test]
fn slanted_ladder_uses_its_plane_and_cannot_attach_through_a_wall() {
    let mut world = world();
    let slope = Vec3::new(-1., 0., 0.25).normalize();
    // Clip the ladder to a sloping front plane within its axial bounds.
    world.convexes[1].mins.x = -32.;
    world.convexes[1].planes[1].dist = 32.;
    world.convexes[1].planes.push(Plane { normal: slope, dist: 16. });
    world.finalize();
    // Hull support distance places its center at the sloping contact.
    let z = 64.;
    let x = (16. + slope.abs().dot(Vec3::new(16., 16., 36.))
        - slope.z * (z + 36.) + collision::DIST_EPSILON) / slope.x;
    let origin = Vec3::new(x, 0., z);
    assert_eq!(world.ladder_normal(origin, STAND_MINS, STAND_MAXS), Some(slope));

    let mut world = self::world();
    world.add_convex(box_shape(Vec3::new(-1., -24., 0.), Vec3::new(0., 24., 256.), contents::SOLID));
    world.finalize();
    let origin = Vec3::new(-17. - collision::DIST_EPSILON, 0., 32.);
    assert_eq!(world.ladder_normal(origin, STAND_MINS, STAND_MAXS), None);
}

#[test]
fn consecutive_ladder_brushes_do_not_occlude_each_other() {
    let mut world = world();
    world.convexes[1].maxs.z = 128.;
    world.convexes[1].planes[4].dist = 128.;
    world.add_convex(box_shape(Vec3::new(0., -24., 128.), Vec3::new(1., 24., 256.), contents::LADDER | contents::GRATE));
    world.finalize();
    let mut player = at(96.);
    let forward = MoveInput { forward_move: 450., ..Default::default() };
    for _ in 0..32 { tick(&mut player, &world, forward, 128); }
    assert!(player.origin.z > 128.);
    assert_eq!(player.ladder_normal, Some(-Vec3::X));
}

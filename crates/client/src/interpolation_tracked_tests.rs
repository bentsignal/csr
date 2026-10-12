use super::*;
use glam::{Quat, Vec3};

fn tracking(x: f32, rotation: Quat) -> sim::TrackedPose {
    let pose = sim::Pose {
        position: Vec3::new(x, 0., 64.),
        rotation,
    };
    sim::TrackedPose {
        head: pose,
        left_hand: sim::Pose {
            position: Vec3::new(x, -12., 48.),
            ..pose
        },
        right_hand: sim::Pose {
            position: Vec3::new(x, 12., 48.),
            ..pose
        },
    }
}
fn fixture(rate: u32) -> (sim::World, sim::PlayerId) {
    let mut world = sim::World::new(std::sync::Arc::new(collision::CollisionWorld::new()));
    world.set_tick_rate(rate);
    let id = world.add_player("Tracked", sim::Team::CounterTerrorist);
    world.events.clear();
    (world, id)
}

#[test]
fn remote_sample_retains_interpolated_tracking_without_raw_or_double_interpolation() {
    for rate in [64, 128] {
        let (mut world, id) = fixture(rate);
        let actor = world.player_mut(id).unwrap();
        actor.tracked = Some(tracking(0., Quat::IDENTITY));
        actor.aim = Some(sim::AimRay {
            origin: Vec3::new(12., 0., 62.),
            direction: Vec3::X,
        });
        let mut history = netcode::remote_pose::PoseHistory::new(rate, 8, 0, 128.).unwrap();
        history.push(&world.state()).unwrap();
        world.tick = u64::from(rate / 32);
        world.player_mut(id).unwrap().tracked = Some(tracking(
            8.,
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        ));
        history.push(&world.state()).unwrap();
        let sample = history.sample(id, world.tick as f64 / 2.).unwrap().unwrap();
        let before = world.state();
        let pose = Pose::remote(&sample.pose);
        let mut poses = RenderPoses::default();
        poses.set_overrides([(id, pose)]);
        for alpha in [0., 0.5, 1.] {
            let rendered = poses.sample(world.player(id).unwrap(), alpha);
            assert_eq!(rendered.tracked, sample.pose.tracked);
            assert_eq!(rendered.aim, sample.pose.aim);
            let head = rendered.tracked.unwrap().head;
            assert_eq!(head.position.x, 4.);
            assert_ne!(
                head.position,
                world.player(id).unwrap().tracked.unwrap().head.position
            );
            let gaze = head.rotation * Vec3::Y;
            assert!(gaze.x < -0.7 && gaze.y > 0.7);
            assert_eq!(rendered.aim.unwrap().direction, Vec3::X); // Independent of the head's gaze.
        }
        assert_eq!(world.state(), before);
        poses.clear_overrides();
        assert!(poses.2.is_empty());
        assert_eq!(poses.4, TrackingSource::Cleared);
    }
}

#[test]
fn local_tracking_lerps_safely_but_availability_and_new_lives_snap() {
    let (mut world, id) = fixture(128);
    let actor = world.player_mut(id).unwrap();
    actor.tracked = Some(tracking(0., Quat::IDENTITY));
    actor.aim = Some(sim::AimRay {
        origin: Vec3::ZERO,
        direction: Vec3::X,
    });
    let a = Pose::player(actor);
    actor.tracked = Some(tracking(8., -Quat::IDENTITY));
    actor.aim = Some(sim::AimRay {
        origin: Vec3::X * 8.,
        direction: Vec3::NEG_X,
    });
    let b = Pose::player(actor);
    let mid = a.lerp(b, 0.5);
    assert_eq!(mid.tracked.unwrap().head.position.x, 4.);
    assert!((mid.tracked.unwrap().head.rotation * Vec3::Y - Vec3::Y).length() < 1e-5);
    let aim = mid.aim.unwrap();
    assert_eq!(aim.origin, Vec3::X * 4.);
    assert!(aim.direction.is_finite());
    assert!((aim.direction.length() - 1.).abs() < 1e-5);
    let missing = Pose {
        tracked: None,
        aim: None,
        ..b
    };
    assert!(a.lerp(missing, 0.5).tracked.is_none());
    assert!(a.lerp(missing, 0.5).aim.is_none());
    assert_eq!(missing.lerp(b, 0.5).tracked.unwrap().head.position.x, 8.);
    assert_eq!(missing.lerp(b, 0.5).aim.unwrap().origin, Vec3::X * 8.);
    let next_life = Pose {
        life: Some(a.life.unwrap() + 1),
        ..b
    };
    assert_eq!(a.lerp(next_life, 0.5).tracked.unwrap().head.position.x, 8.);
    assert_eq!(a.lerp(next_life, 0.5).aim.unwrap().direction, Vec3::NEG_X);
    let mut bad = tracking(8., Quat::from_xyzw(0., 0., 0., 0.));
    assert!(
        a.lerp(
            Pose {
                tracked: Some(bad),
                ..b
            },
            0.5
        )
        .tracked
        .is_none()
    );
    bad.head.rotation = Quat::IDENTITY;
    bad.head.position.x = f32::NAN;
    assert!(
        a.lerp(
            Pose {
                tracked: Some(bad),
                ..b
            },
            0.5
        )
        .tracked
        .is_none()
    );
    assert!(
        a.lerp(
            Pose {
                aim: Some(sim::AimRay {
                    origin: Vec3::ZERO,
                    direction: Vec3::ZERO
                }),
                ..b
            },
            0.5
        )
        .aim
        .is_none()
    );
}

#[test]
fn missing_remote_actor_never_uses_raw_tracking_and_cancellation_discards_old_pairs() {
    let (mut world, id) = fixture(128);
    let actor = world.player_mut(id).unwrap();
    actor.tracked = Some(tracking(7., Quat::IDENTITY));
    actor.aim = Some(sim::AimRay {
        origin: Vec3::X,
        direction: Vec3::X,
    });
    let mut poses = RenderPoses::default();
    poses.capture(&world);
    let flat = poses.sample(world.player(id).unwrap(), 0.5);
    assert!(flat.tracked.is_some());
    assert!(flat.aim.is_some());
    poses.set_overrides([]);
    let missing = poses.sample(world.player(id).unwrap(), 0.5);
    assert!(missing.tracked.is_none());
    assert!(missing.aim.is_none());
    assert_eq!(missing.origin, flat.origin);
    assert_eq!(missing.eye, flat.eye);
    assert_eq!(missing.angles, flat.angles);
    poses.clear_overrides();
    let cancelled = poses.sample(world.player(id).unwrap(), 0.5);
    assert!(cancelled.tracked.is_none());
    assert!(cancelled.aim.is_none());
    poses.capture(&world); // Fresh offline tracking begins from availability None.
    assert_eq!(
        poses
            .sample(world.player(id).unwrap(), 0.5)
            .tracked
            .unwrap()
            .head
            .position
            .x,
        7.
    );
}

#[test]
fn cancellation_without_cached_pair_requires_fresh_capture_or_explicit_override() {
    let (mut world, id) = fixture(128);
    let actor = world.player_mut(id).unwrap();
    actor.tracked = Some(tracking(7., Quat::IDENTITY));
    actor.aim = Some(sim::AimRay {
        origin: Vec3::X,
        direction: Vec3::X,
    });
    let before = world.state();
    let actor = world.player(id).unwrap();
    let raw = Pose::player(actor);
    let mut poses = RenderPoses::default();
    poses.set_overrides([(id, raw)]);
    assert!(poses.0.is_empty()); // Remote scenes do not run offline capture.
    poses.clear_overrides();
    let cancelled = poses.sample(actor, 0.5);
    assert!(cancelled.tracked.is_none());
    assert!(cancelled.aim.is_none());
    assert_eq!(cancelled.origin, raw.origin);
    assert_eq!(cancelled.eye, raw.eye);
    assert_eq!(cancelled.angles, raw.angles);
    let predicted = poses.capture_prediction(actor, world.tick, world.dt());
    poses.set_overrides([(id, predicted)]);
    assert_eq!(poses.sample(actor, 0.5).tracked, raw.tracked);
    assert_eq!(poses.sample(actor, 0.5).aim, raw.aim);
    poses.clear_overrides();
    assert!(poses.sample(actor, 0.5).tracked.is_none());
    poses.capture(&world);
    assert_eq!(poses.sample(actor, 0.5).tracked, raw.tracked);
    assert_eq!(poses.sample(actor, 0.5).aim, raw.aim);
    assert_eq!(world.state(), before);
}

#[test]
fn prediction_stair_eye_keeps_feet_relative_tracking_unchanged() {
    let (mut world, id) = fixture(128);
    let dt = world.dt();
    let actor = world.player_mut(id).unwrap();
    actor.movement.on_ground = true;
    actor.tracked = Some(tracking(3., Quat::IDENTITY));
    actor.aim = Some(sim::AimRay {
        origin: Vec3::new(12., 0., 62.),
        direction: Vec3::X,
    });
    let tracked = actor.tracked;
    let aim = actor.aim;
    let mut poses = RenderPoses::default();
    poses.capture_prediction(actor, 0, dt);
    world.player_mut(id).unwrap().movement.origin.z += 18.;
    let before = world.state();
    let rendered = poses.capture_prediction(world.player(id).unwrap(), 1, world.dt());
    assert!(rendered.eye.z < world.player(id).unwrap().eye().z - 15.);
    assert_eq!(rendered.tracked, tracked);
    assert_eq!(rendered.aim, aim);
    assert_eq!(world.state(), before);
}

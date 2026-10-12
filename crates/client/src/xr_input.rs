//! Pure PC-XR input boundary: runtime meters into feet-relative Source units.
//!
//! The runtime driver owns pose validity flags, timestamps and action edges.
//! Supply `None` for an unavailable pose and drain one-shot commands once per
//! simulation tick. This module never advances gameplay or chooses a rewind tick.
use glam::{Quat, Vec2, Vec3};
use movement::buttons;
use sim::{AimRay, Pose, Purchase, TickInput, TrackedPose};

pub const INCHES_PER_METER: f32 = 1.0 / 0.0254;

/// OpenXR axes: +X right, +Y up, -Z forward. Position is in meters.
#[derive(Clone, Copy, Debug)]
pub struct RawPose {
    pub position_m: Vec3,
    pub orientation: Quat,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    pub focused: bool,
    pub head: Option<RawPose>,
    pub left_hand: Option<RawPose>,
    pub right_hand: Option<RawPose>,
    /// Weapon aim action space; separate from the right-hand grip space.
    pub aim: Option<RawPose>,
}

/// Complete semantic controls for one tick; no device-specific bindings.
#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    /// X is right, Y is forward, in [-1, 1]. Diagonal magnitude is capped at 1.
    pub movement: Vec2,
    pub buttons: u32,
    pub weapon_slot: u8,
    pub buy: Purchase,
    pub drop: bool,
    pub utility: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Uncalibrated,
    WrongLife,
    InvalidCalibration,
    InvalidPose(&'static str),
    NonfiniteControl,
    InvalidSlot,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "XR input: {self:?}")
    }
}
impl std::error::Error for Error {}

/// A floor origin and yaw alignment, independent of the player's moving origin.
/// Fields are private so every calibration and conversion is validated.
#[derive(Clone, Copy, Debug)]
pub struct Calibration {
    floor_origin_m: Vec3,
    source_yaw: Quat,
    initial_view_yaw: f32,
}

fn to_source(vector: Vec3) -> Vec3 {
    Vec3::new(vector.x, -vector.z, vector.y)
}
fn wrap_yaw(yaw: f32) -> f32 {
    // Wrap before adding: a finite but huge yaw must not overflow arithmetic.
    (yaw.rem_euclid(360.0) + 180.0).rem_euclid(360.0) - 180.0
}
fn validated(pose: RawPose, label: &'static str) -> Result<RawPose, Error> {
    let norm = pose.orientation.length_squared();
    if !pose.position_m.is_finite()
        || !pose.orientation.is_finite()
        || !norm.is_finite()
        || norm < 0.0001
    {
        return Err(Error::InvalidPose(label));
    }
    Ok(RawPose {
        orientation: pose.orientation.normalize(),
        ..pose
    })
}

impl Calibration {
    /// Recenter horizontally at the head, retaining height above the supplied
    /// runtime/calibrated floor. Never assume LOCAL's origin is a floor.
    pub fn from_head(
        head: RawPose,
        floor_height_m: f32,
        desired_source_yaw: f32,
    ) -> Result<Self, Error> {
        let head = validated(head, "head")?;
        if !floor_height_m.is_finite() || !desired_source_yaw.is_finite() {
            return Err(Error::InvalidCalibration);
        }
        let forward = to_source(head.orientation * Vec3::NEG_Z);
        if forward.truncate().length_squared() < 0.0001 {
            return Err(Error::InvalidCalibration);
        }
        let initial_view_yaw = wrap_yaw(desired_source_yaw);
        let yaw = forward.y.atan2(forward.x).to_degrees();
        let calibration = Self {
            floor_origin_m: Vec3::new(head.position_m.x, floor_height_m, head.position_m.z),
            source_yaw: Quat::from_rotation_z((initial_view_yaw - yaw).to_radians()),
            initial_view_yaw,
        };
        calibration.pose(head, "head")?;
        Ok(calibration)
    }

    /// Convert a fresh runtime pose for rendering without changing input state.
    /// Rotation changes basis by conjugation, then applies calibrated world yaw.
    /// Its local axes retain XR meaning after conversion: +Y is forward, +X
    /// right, +Z up. Gaze is `rotation * Vec3::Y`, unlike Source body yaw's
    /// +X-forward convention. Model/bone consumers must align their local basis.
    pub fn convert_pose(&self, pose: RawPose) -> Result<Pose, Error> {
        self.pose(pose, "pose")
    }

    fn pose(&self, pose: RawPose, label: &'static str) -> Result<Pose, Error> {
        let pose = validated(pose, label)?;
        let source_from_xr = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        let result = Pose {
            position: self.source_yaw
                * to_source((pose.position_m - self.floor_origin_m) * INCHES_PER_METER),
            rotation: (self.source_yaw
                * source_from_xr
                * pose.orientation
                * source_from_xr.inverse())
            .normalize(),
        };
        if !result.position.is_finite() || !result.rotation.is_finite() {
            return Err(Error::InvalidPose(label));
        }
        Ok(result)
    }

    fn direction(&self, pose: RawPose) -> Result<Vec3, Error> {
        let pose = validated(pose, "aim")?;
        Ok((self.source_yaw * to_source(pose.orientation * Vec3::NEG_Z)).normalize())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub input: TickInput,
    /// Remains available for local rendering even when a hand is unavailable.
    pub head: Option<Pose>,
}

#[derive(Debug, Default)]
pub struct Adapter {
    calibration: Option<Calibration>,
    life_epoch: Option<u64>,
    last_view_angles: Vec3,
}
impl Adapter {
    pub fn begin_life(&mut self, life_epoch: u64, calibration: Calibration) {
        self.last_view_angles = Vec3::new(0.0, calibration.initial_view_yaw, 0.0);
        self.calibration = Some(calibration);
        self.life_epoch = Some(life_epoch);
    }
    /// Call on cancellation, session loss or replacement. A new life requires
    /// explicit calibration, even when another session reuses the same epoch.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn calibration(&self) -> Option<Calibration> {
        self.calibration
    }
    /// Snap/smooth turning rotates the tracking origin, not gameplay directly.
    pub fn turn(&mut self, degrees: f32) -> Result<(), Error> {
        if !degrees.is_finite() {
            return Err(Error::NonfiniteControl);
        }
        let calibration = self.calibration.as_mut().ok_or(Error::Uncalibrated)?;
        let degrees = wrap_yaw(degrees);
        calibration.source_yaw =
            (Quat::from_rotation_z(degrees.to_radians()) * calibration.source_yaw).normalize();
        calibration.initial_view_yaw = wrap_yaw(calibration.initial_view_yaw + degrees);
        self.last_view_angles.y = wrap_yaw(self.last_view_angles.y + degrees);
        Ok(())
    }

    /// Invalid frames are rejected atomically: they never update cached look.
    /// Unfocused or head-unavailable frames send neutral controls. Tracking may
    /// still be published, but attacks always require a valid weapon aim pose.
    pub fn sample(
        &mut self,
        life_epoch: u64,
        frame: Frame,
        controls: Controls,
    ) -> Result<Sample, Error> {
        let calibration = self.calibration.ok_or(Error::Uncalibrated)?;
        if self.life_epoch != Some(life_epoch) {
            return Err(Error::WrongLife);
        }
        let head = frame
            .head
            .map(|p| calibration.pose(p, "head"))
            .transpose()?;
        let left = frame
            .left_hand
            .map(|p| calibration.pose(p, "left hand"))
            .transpose()?;
        let right = frame
            .right_hand
            .map(|p| calibration.pose(p, "right hand"))
            .transpose()?;
        let aim_pose = frame.aim.map(|p| calibration.pose(p, "aim")).transpose()?;
        let mut angles = self.last_view_angles;
        if let Some(head_pose) = frame.head {
            let forward = calibration.direction(head_pose)?;
            angles.x =
                (-forward.z.atan2(forward.truncate().length()).to_degrees()).clamp(-89.0, 89.0);
            if forward.truncate().length_squared() >= 0.0001 {
                angles.y = wrap_yaw(forward.y.atan2(forward.x).to_degrees());
            }
        }
        let tracked = match (head, left, right) {
            (Some(head), Some(left_hand), Some(right_hand)) => Some(TrackedPose {
                head,
                left_hand,
                right_hand,
            }),
            _ => None,
        };
        let mut input = TickInput {
            view_angles: angles,
            tracked,
            ..Default::default()
        };
        if frame.focused && head.is_some() {
            if !controls.movement.is_finite() {
                return Err(Error::NonfiniteControl);
            }
            if controls.weapon_slot > 5 && controls.weapon_slot != sim::SLOT_LAST {
                return Err(Error::InvalidSlot);
            }
            let stick = controls
                .movement
                .clamp(Vec2::splat(-1.0), Vec2::ONE)
                .clamp_length_max(1.0);
            input.forward_move = stick.y * 450.0;
            input.side_move = stick.x * 450.0;
            input.buttons = controls.buttons & 0x7f;
            input.weapon_slot = controls.weapon_slot;
            input.buy = controls.buy;
            input.drop = controls.drop;
            input.utility = controls.utility;
            if let (Some(pose), Some(raw)) = (aim_pose, frame.aim) {
                input.aim = Some(AimRay {
                    origin: pose.position,
                    direction: calibration.direction(raw)?,
                });
            } else {
                input.buttons &= !(buttons::ATTACK | buttons::ATTACK2);
            }
        }
        self.last_view_angles = angles;
        Ok(Sample { input, head })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pose(position_m: Vec3, orientation: Quat) -> RawPose {
        RawPose {
            position_m,
            orientation,
        }
    }
    fn head() -> RawPose {
        pose(Vec3::new(2.0, 1.7, -3.0), Quat::IDENTITY)
    }
    fn calibration(yaw: f32) -> Calibration {
        Calibration::from_head(head(), 0.0, yaw).unwrap()
    }
    fn frame() -> Frame {
        Frame {
            focused: true,
            head: Some(head()),
            left_hand: Some(pose(
                head().position_m + Vec3::new(-0.2, -0.3, -0.4),
                Quat::IDENTITY,
            )),
            right_hand: Some(pose(
                head().position_m + Vec3::new(0.2, -0.3, -0.4),
                Quat::IDENTITY,
            )),
            aim: Some(pose(
                head().position_m + Vec3::new(0.2, -0.3, -0.4),
                Quat::IDENTITY,
            )),
        }
    }
    fn adapter() -> Adapter {
        let mut a = Adapter::default();
        a.begin_life(7, calibration(0.0));
        a
    }
    fn close(actual: Vec3, expected: Vec3) {
        assert!(
            actual.distance(expected) < 0.0001,
            "{actual:?} != {expected:?}"
        );
    }
    #[test]
    fn meters_axes_floor_and_yaw_are_converted_once() {
        let c = calibration(0.0);
        close(
            c.convert_pose(head()).unwrap().position,
            Vec3::Z * 1.7 * INCHES_PER_METER,
        );
        let h = frame().right_hand.unwrap();
        close(
            c.convert_pose(h).unwrap().position,
            Vec3::new(0.4, -0.2, 1.4) * INCHES_PER_METER,
        );
        close(
            calibration(90.0).convert_pose(h).unwrap().position,
            Vec3::new(0.2, 0.4, 1.4) * INCHES_PER_METER,
        );
        let seated = Calibration::from_head(head(), 0.7, 0.0).unwrap();
        close(
            seated.convert_pose(head()).unwrap().position,
            Vec3::Z * INCHES_PER_METER,
        );
        let eye = c
            .convert_pose(pose(head().position_m + Vec3::X * 0.063, Quat::IDENTITY))
            .unwrap();
        assert!(
            (eye.position
                .distance(c.convert_pose(head()).unwrap().position)
                - 0.063 * INCHES_PER_METER)
                .abs()
                < 0.0001
        );
    }
    #[test]
    fn quaternion_basis_matches_rotated_vectors_and_normalizes() {
        let c = calibration(25.0);
        let q = Quat::from_rotation_y(0.4) * Quat::from_rotation_x(-0.3);
        let raw = pose(head().position_m, q * 3.0);
        let converted = c.convert_pose(raw).unwrap();
        assert!((converted.rotation.length() - 1.0).abs() < 0.00001);
        for v in [Vec3::X, Vec3::Y, Vec3::Z] {
            close(
                converted.rotation * to_source(v),
                c.source_yaw * to_source(q * v),
            );
        }
    }
    #[test]
    fn head_and_weapon_aim_stay_independent_with_complete_controls() {
        let mut a = adapter();
        let mut f = frame();
        f.aim.as_mut().unwrap().orientation = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        let controls = Controls {
            movement: Vec2::new(0.5, 0.25),
            buttons: buttons::ATTACK | buttons::USE | buttons::RELOAD,
            weapon_slot: sim::SLOT_LAST,
            buy: Purchase::Kevlar,
            drop: true,
            utility: 44,
        };
        let sample = a.sample(7, f, controls).unwrap();
        close(sample.input.view_angles, Vec3::ZERO);
        close(sample.input.aim.unwrap().direction, -Vec3::Y);
        assert_eq!(
            (sample.input.side_move, sample.input.forward_move),
            (225.0, 112.5)
        );
        assert_eq!(sample.input.buttons, controls.buttons);
        assert_eq!(sample.input.weapon_slot, sim::SLOT_LAST);
        assert_eq!(sample.input.buy, Purchase::Kevlar);
        assert!(sample.input.drop);
        assert_eq!(sample.input.utility, 44);
        assert_eq!(sample.input.rewind_tick, None);
        assert!(sample.input.tracked.is_some());
    }
    #[test]
    fn converted_pose_local_forward_matches_same_pose_weapon_ray() {
        let mut a = Adapter::default();
        a.begin_life(7, calibration(37.0));
        let raw = pose(
            head().position_m,
            Quat::from_rotation_y(0.6) * Quat::from_rotation_x(-0.4),
        );
        let f = Frame {
            head: Some(raw),
            aim: Some(raw),
            ..frame()
        };
        let sample = a.sample(7, f, Controls::default()).unwrap();
        close(
            sample.head.unwrap().rotation * Vec3::Y,
            sample.input.aim.unwrap().direction,
        );
    }
    #[test]
    fn missing_pose_never_fabricates_hands_or_falls_back_to_head_attack() {
        let mut a = adapter();
        let mut f = frame();
        f.left_hand = None;
        f.aim = None;
        let sample = a
            .sample(
                7,
                f,
                Controls {
                    buttons: buttons::ATTACK | buttons::ATTACK2 | buttons::USE,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(sample.head.is_some());
        assert!(sample.input.tracked.is_none());
        assert!(sample.input.aim.is_none());
        assert_eq!(sample.input.buttons, buttons::USE);
        f.head = None;
        let sample = a
            .sample(
                7,
                f,
                Controls {
                    movement: Vec2::ONE,
                    buttons: buttons::RELOAD,
                    drop: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(sample.input.forward_move, 0.0);
        assert_eq!(sample.input.buttons, 0);
        assert!(!sample.input.drop);
    }
    #[test]
    fn focus_loss_sends_neutral_controls_and_retains_valid_tracking() {
        let mut a = adapter();
        let mut f = frame();
        f.focused = false;
        let sample = a
            .sample(
                7,
                f,
                Controls {
                    movement: Vec2::NAN,
                    buttons: u32::MAX,
                    weapon_slot: 99,
                    buy: Purchase::Kevlar,
                    drop: true,
                    utility: 44,
                },
            )
            .unwrap();
        assert_eq!(sample.input.forward_move, 0.0);
        assert_eq!(sample.input.side_move, 0.0);
        assert_eq!(sample.input.buttons, 0);
        assert_eq!(sample.input.weapon_slot, 0);
        assert_eq!(sample.input.buy, Purchase::None);
        assert!(!sample.input.drop);
        assert_eq!(sample.input.utility, 0);
        assert!(sample.input.tracked.is_some());
        assert!(sample.input.aim.is_none());
    }
    #[test]
    fn bad_poses_controls_and_overflow_reject_atomically() {
        let mut a = adapter();
        let baseline = a
            .sample(7, frame(), Controls::default())
            .unwrap()
            .input
            .view_angles;
        for raw in [
            pose(Vec3::NAN, Quat::IDENTITY),
            pose(Vec3::ZERO, Quat::from_xyzw(0.0, 0.0, 0.0, 0.0)),
            pose(Vec3::ZERO, Quat::from_xyzw(f32::MAX, 0.0, 0.0, 0.0)),
            pose(Vec3::splat(f32::MAX), Quat::IDENTITY),
        ] {
            let mut f = frame();
            f.aim = Some(raw);
            assert!(matches!(
                a.sample(7, f, Controls::default()),
                Err(Error::InvalidPose("aim"))
            ));
            assert_eq!(a.last_view_angles, baseline);
        }
        let mut f = frame();
        f.head.as_mut().unwrap().orientation = Quat::from_rotation_y(0.5);
        assert_eq!(
            a.sample(
                7,
                f,
                Controls {
                    movement: Vec2::NAN,
                    ..Default::default()
                }
            )
            .unwrap_err(),
            Error::NonfiniteControl
        );
        assert_eq!(
            a.sample(
                7,
                f,
                Controls {
                    weapon_slot: 6,
                    ..Default::default()
                }
            )
            .unwrap_err(),
            Error::InvalidSlot
        );
        assert_eq!(a.last_view_angles, baseline);
        assert!(Calibration::from_head(head(), f32::NAN, 0.0).is_err());
        assert!(Calibration::from_head(head(), 0.0, f32::INFINITY).is_err());
    }
    #[test]
    fn turn_wrap_and_lifecycle_reset_cannot_reuse_old_calibration() {
        let mut a = adapter();
        a.turn(90.0).unwrap();
        let sample = a.sample(7, frame(), Controls::default()).unwrap();
        assert!((sample.input.view_angles.y - 90.0).abs() < 0.0001);
        close(sample.input.aim.unwrap().direction, Vec3::Y);
        let before = sample.input.view_angles;
        assert_eq!(a.turn(f32::NAN), Err(Error::NonfiniteControl));
        assert_eq!(a.last_view_angles, before);
        assert_eq!(
            a.sample(8, frame(), Controls::default()).unwrap_err(),
            Error::WrongLife
        );
        a.reset();
        a.reset();
        assert!(a.calibration().is_none());
        assert_eq!(
            a.sample(7, frame(), Controls::default()).unwrap_err(),
            Error::Uncalibrated
        );
        a.begin_life(7, calibration(-90.0));
        assert!(
            (a.sample(7, frame(), Controls::default())
                .unwrap()
                .input
                .view_angles
                .y
                + 90.0)
                .abs()
                < 0.0001
        );
        a.turn(f32::MAX).unwrap();
        assert!(
            a.sample(7, frame(), Controls::default())
                .unwrap()
                .input
                .view_angles
                .is_finite()
        );
    }
    #[test]
    fn analog_and_buttons_stay_within_sim_contract() {
        let sample = adapter()
            .sample(
                7,
                frame(),
                Controls {
                    movement: Vec2::splat(f32::MAX),
                    buttons: u32::MAX,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(
            (Vec2::new(sample.input.side_move, sample.input.forward_move).length() - 450.0).abs()
                < 0.0001
        );
        assert_eq!(sample.input.buttons, 0x7f);
    }
}

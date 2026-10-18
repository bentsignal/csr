//! Installed studio-model hitbox data and clean-room geometric queries.
//!
//! All coordinates remain in Source axes and units. Queries use rigid
//! bone-to-query-space matrices, never skinning/inverse-bind matrices.
use crate::{R, invalid};
use glam::{Mat4, Vec3};
use std::io;

pub const HITBOX_STRIDE: usize = 68;
const SET_STRIDE: usize = 12;
const MAX_SETS: usize = 128;
const MAX_HITBOXES: usize = 4096;
const MAX_TOTAL_HITBOXES: usize = 65_536;

#[derive(Clone, Debug)]
pub struct Hitbox {
    pub bone: i32,
    /// Authored raw group. In particular, group 8 is not silently remapped.
    pub group: i32,
    /// Capsule endpoints, or box-local minimum/maximum when radius is -1.
    pub min: Vec3,
    pub max: Vec3,
    /// Signed authored value: -1 is the supported box sentinel.
    pub radius: f32,
    /// MDL v49 box-local pitch/yaw/roll in degrees, relative to its bone.
    pub rotation: Vec3,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct HitboxSet {
    pub name: String,
    pub hitboxes: Vec<Hitbox>,
}

#[derive(Clone, Debug)]
pub enum Shape {
    Capsule {
        start: Vec3,
        end: Vec3,
        radius: f32,
    },
    Box {
        min: Vec3,
        max: Vec3,
        box_to_model: Mat4,
    },
}

#[derive(Clone, Debug)]
pub struct PosedHitbox {
    pub shape: Shape,
    pub group: i32,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub distance: f32,
    pub group: i32,
    pub index: usize,
}

fn table(r: &R<'_>, base: usize, count: usize, stride: usize, label: &str) -> io::Result<()> {
    let end = count.checked_mul(stride).and_then(|n| base.checked_add(n));
    if end.is_none_or(|end| end > r.0.len()) {
        return Err(invalid(format!("{label} table past end")));
    }
    Ok(())
}

fn count(value: i32, limit: usize, label: &str) -> io::Result<usize> {
    usize::try_from(value)
        .ok()
        .filter(|&n| n <= limit)
        .ok_or_else(|| invalid(format!("invalid {label} count {value}")))
}

fn relative(base: usize, offset: i32, label: &str) -> io::Result<usize> {
    let absolute = base as i128 + offset as i128;
    usize::try_from(absolute).map_err(|_| invalid(format!("invalid {label} offset {offset}")))
}

fn name(r: &R<'_>, base: usize, offset: i32, label: &str) -> io::Result<String> {
    if offset == 0 {
        return Ok(String::new());
    }
    let at = relative(base, offset, label)?;
    let bytes =
        r.0.get(at..)
            .ok_or_else(|| invalid(format!("{label} name past end")))?;
    let end = bytes
        .iter()
        .position(|&v| v == 0)
        .filter(|&n| n <= 4096)
        .ok_or_else(|| invalid(format!("unterminated {label} name")))?;
    Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

pub(crate) fn read_sets(r: &R<'_>, version: i32, bones: usize) -> io::Result<Vec<HitboxSet>> {
    let sets = count(r.i32(172)?, MAX_SETS, "hitbox set")?;
    if sets == 0 {
        return Ok(Vec::new());
    }
    if !matches!(version, 44 | 45 | 48 | 49) {
        return Err(invalid(format!("unsupported MDL v{version} hitbox layout")));
    }
    let base = usize::try_from(r.i32(176)?).map_err(|_| invalid("negative hitbox set offset"))?;
    table(r, base, sets, SET_STRIDE, "hitbox set")?;
    let mut output = Vec::with_capacity(sets);
    let mut total = 0usize;
    for set_index in 0..sets {
        let set = base + set_index * SET_STRIDE;
        let n = count(r.i32(set + 4)?, MAX_HITBOXES, "hitbox")?;
        total = total
            .checked_add(n)
            .filter(|&n| n <= MAX_TOTAL_HITBOXES)
            .ok_or_else(|| invalid("hitbox sets exceed total record budget"))?;
        let first = relative(set, r.i32(set + 8)?, "hitbox")?;
        table(r, first, n, HITBOX_STRIDE, "hitbox")?;
        let mut hitboxes = Vec::with_capacity(n);
        for index in 0..n {
            let at = first + index * HITBOX_STRIDE;
            // Legacy v44 props and the v45 props embedded in classic Cache
            // have 68-byte AABBs with zero reserved bytes. V45 names are
            // record-relative, unlike the observed file-relative v44 names.
            // Do not reinterpret reserved data as a newer capsule extension.
            if matches!(version, 44 | 45) && r.0[at + 36..at + HITBOX_STRIDE].iter().any(|&v| v != 0) {
                return Err(invalid(format!("unsupported MDL v{version} hitbox reserved data")));
            }
            let bone = r.i32(at)?;
            if bone < 0 || bone as usize >= bones {
                return Err(invalid(format!(
                    "hitbox set {set_index} box {index}: bone {bone} out of range {bones}"
                )));
            }
            let min = r.vec3(at + 8)?;
            let max = r.vec3(at + 20)?;
            let (rotation, radius) = if version == 49 {
                (r.vec3(at + 36)?, r.f32(at + 48)?)
            } else {
                (Vec3::ZERO, -1.0)
            };
            if !min.is_finite() || !max.is_finite() || !rotation.is_finite() || !radius.is_finite()
            {
                return Err(invalid(format!(
                    "hitbox set {set_index} box {index}: non-finite geometry"
                )));
            }
            if radius == -1.0 && min.cmpgt(max).any() {
                return Err(invalid(format!(
                    "hitbox set {set_index} box {index}: reversed box bounds"
                )));
            }
            hitboxes.push(Hitbox {
                bone,
                group: r.i32(at + 4)?,
                min,
                max,
                radius,
                rotation,
                name: name(
                    r,
                    if version == 44 { 0 } else { at },
                    r.i32(at + 32)?,
                    "hitbox",
                )?,
            });
        }
        output.push(HitboxSet {
            name: name(r, set, r.i32(set)?, "hitbox set")?,
            hitboxes,
        });
    }
    Ok(output)
}

fn rigid(matrix: Mat4) -> bool {
    if !matrix.is_finite() {
        return false;
    }
    let x = matrix.x_axis.truncate();
    let y = matrix.y_axis.truncate();
    let z = matrix.z_axis.truncate();
    let epsilon = 0.002;
    matrix.x_axis.w.abs() < epsilon
        && matrix.y_axis.w.abs() < epsilon
        && matrix.z_axis.w.abs() < epsilon
        && (matrix.w_axis.w - 1.0).abs() < epsilon
        && (x.length_squared() - 1.0).abs() < epsilon
        && (y.length_squared() - 1.0).abs() < epsilon
        && (z.length_squared() - 1.0).abs() < epsilon
        && x.dot(y).abs() < epsilon
        && x.dot(z).abs() < epsilon
        && y.dot(z).abs() < epsilon
        && (x.cross(y).dot(z) - 1.0).abs() < epsilon
}

impl HitboxSet {
    /// Pose each bone-local shape into model or world space. Matrices must
    /// be finite proper rigid transforms; scaled/sheared poses are rejected.
    pub fn pose(&self, bone_to_model: &[Mat4]) -> io::Result<Vec<PosedHitbox>> {
        self.hitboxes
            .iter()
            .enumerate()
            .map(|(index, hitbox)| {
                let matrix = usize::try_from(hitbox.bone)
                    .ok()
                    .and_then(|bone| bone_to_model.get(bone))
                    .copied()
                    .filter(|&matrix| rigid(matrix))
                    .ok_or_else(|| {
                        invalid(format!(
                            "hitbox {index}: missing or non-rigid bone transform"
                        ))
                    })?;
                if !hitbox.min.is_finite()
                    || !hitbox.max.is_finite()
                    || !hitbox.rotation.is_finite()
                    || !hitbox.radius.is_finite()
                {
                    return Err(invalid(format!("hitbox {index}: non-finite geometry")));
                }
                let shape = if hitbox.radius >= 0.0 {
                    // All installed supported character capsules have zero local
                    // angles. Do not invent their interaction for other assets.
                    if hitbox.rotation != Vec3::ZERO {
                        return Err(invalid(format!(
                            "hitbox {index}: rotated capsule is unsupported"
                        )));
                    }
                    Shape::Capsule {
                        start: matrix.transform_point3(hitbox.min),
                        end: matrix.transform_point3(hitbox.max),
                        radius: hitbox.radius,
                    }
                } else if hitbox.radius == -1.0 && !hitbox.min.cmpgt(hitbox.max).any() {
                    let rotation = Mat4::from_mat3(crate::angles_to_mat3(hitbox.rotation));
                    Shape::Box {
                        min: hitbox.min,
                        max: hitbox.max,
                        box_to_model: matrix * rotation,
                    }
                } else {
                    return Err(invalid(format!(
                        "hitbox {index}: unsupported radius {} or reversed box bounds",
                        hitbox.radius
                    )));
                };
                let posed = PosedHitbox {
                    shape,
                    group: hitbox.group,
                    index,
                };
                let (min, max) = posed.bounds();
                if !min.is_finite() || !max.is_finite() {
                    return Err(invalid(format!(
                        "hitbox {index}: transformed bounds overflow"
                    )));
                }
                Ok(posed)
            })
            .collect()
    }

    /// Closest intersection in Source/model units. Direction must be unit
    /// length; an invalid ray is an error rather than a gameplay hit.
    pub fn ray_intersection(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        bone_to_model: &[Mat4],
    ) -> io::Result<Option<RayHit>> {
        if !valid_ray(origin, direction, max_distance) {
            return Err(invalid("invalid hitbox ray"));
        }
        let mut closest: Option<RayHit> = None;
        for hitbox in self.pose(bone_to_model)? {
            if let Some(distance) = hitbox.ray_intersection(origin, direction, max_distance) {
                if closest.is_none_or(|hit| distance < hit.distance) {
                    closest = Some(RayHit {
                        distance,
                        group: hitbox.group,
                        index: hitbox.index,
                    });
                }
            }
        }
        Ok(closest)
    }
}

fn valid_ray(origin: Vec3, direction: Vec3, max_distance: f32) -> bool {
    origin.is_finite()
        && direction.is_finite()
        && (direction.length_squared() - 1.0).abs() < 0.0001
        && max_distance.is_finite()
        && max_distance >= 0.0
}

impl PosedHitbox {
    /// Invalid rays return None. Origin-inside returns distance zero; ties
    /// between shapes can be resolved by their authored index by the caller.
    pub fn ray_intersection(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<f32> {
        if !valid_ray(origin, direction, max_distance) {
            return None;
        }
        match self.shape {
            Shape::Capsule { start, end, radius } => {
                if !start.is_finite() || !end.is_finite() || !radius.is_finite() || radius < 0.0 {
                    return None;
                }
                capsule_ray(origin, direction, max_distance, start, end, radius)
            }
            Shape::Box {
                min,
                max,
                box_to_model,
            } => {
                if !min.is_finite()
                    || !max.is_finite()
                    || min.cmpgt(max).any()
                    || !rigid(box_to_model)
                {
                    return None;
                }
                let inverse = box_to_model.inverse();
                let local_origin = inverse.transform_point3(origin);
                let local_direction = inverse.transform_vector3(direction);
                if !local_origin.is_finite() || !local_direction.is_finite() {
                    return None;
                }
                box_ray(local_origin, local_direction, max_distance, min, max)
            }
        }
    }

    /// Shape-derived broad-phase bounds, including limbs outside the movement hull.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        match self.shape {
            Shape::Capsule { start, end, radius } => (
                start.min(end) - Vec3::splat(radius),
                start.max(end) + Vec3::splat(radius),
            ),
            Shape::Box {
                min,
                max,
                box_to_model,
            } => {
                let mut lo = Vec3::splat(f32::INFINITY);
                let mut hi = Vec3::splat(f32::NEG_INFINITY);
                for x in [min.x, max.x] {
                    for y in [min.y, max.y] {
                        for z in [min.z, max.z] {
                            let point = box_to_model.transform_point3(Vec3::new(x, y, z));
                            lo = lo.min(point);
                            hi = hi.max(point);
                        }
                    }
                }
                (lo, hi)
            }
        }
    }
}

fn box_ray(origin: Vec3, direction: Vec3, limit: f32, min: Vec3, max: Vec3) -> Option<f32> {
    let mut enter = 0.0f32;
    let mut exit = limit;
    for axis in 0..3 {
        if direction[axis] == 0.0 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
            if enter > exit {
                return None;
            }
        }
    }
    Some(enter)
}

fn capsule_ray(
    origin: Vec3,
    direction: Vec3,
    limit: f32,
    start: Vec3,
    end: Vec3,
    radius: f32,
) -> Option<f32> {
    // Solve the finite cylinder and its two spherical caps independently.
    // Double-precision scalar arithmetic keeps near-tangent discriminants stable.
    let origin = origin.as_dvec3();
    let direction = direction.as_dvec3();
    let start = start.as_dvec3();
    let end = end.as_dvec3();
    let axis = end - start;
    let offset = origin - start;
    let length_squared = axis.length_squared();
    let radius_squared = (radius as f64).powi(2);
    let along = if length_squared > 0.0 {
        (offset.dot(axis) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if (offset - axis * along).length_squared() <= radius_squared {
        return Some(0.0);
    }
    let mut nearest = f64::INFINITY;
    let unit = direction.length_squared();
    for center in [start, end] {
        let offset = origin - center;
        let b = offset.dot(direction);
        let c = offset.length_squared() - radius_squared;
        let discriminant = b * b - unit * c;
        if discriminant >= 0.0 {
            let t = (-b - discriminant.sqrt()) / unit;
            if t >= 0.0 {
                nearest = nearest.min(t);
            }
        }
    }
    if length_squared > 0.0 {
        let ad = axis.dot(direction);
        let ao = axis.dot(offset);
        let a = length_squared * unit - ad * ad;
        let b = length_squared * offset.dot(direction) - ao * ad;
        let c = length_squared * (offset.length_squared() - radius_squared) - ao * ao;
        let discriminant = b * b - a * c;
        if a > 0.0 && discriminant >= 0.0 {
            for t in [
                (-b - discriminant.sqrt()) / a,
                (-b + discriminant.sqrt()) / a,
            ] {
                let y = ao + t * ad;
                if t >= 0.0 && y >= 0.0 && y <= length_squared {
                    nearest = nearest.min(t);
                }
            }
        }
    }
    (nearest <= limit as f64).then_some(nearest as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_i32(data: &mut [u8], at: usize, value: i32) {
        data[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn put_f32(data: &mut [u8], at: usize, value: f32) {
        data[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn two_sets() -> Vec<u8> {
        let mut data = vec![0; 340];
        put_i32(&mut data, 172, 2);
        put_i32(&mut data, 176, 180);
        put_i32(&mut data, 184, 1);
        put_i32(&mut data, 188, 24);
        put_i32(&mut data, 196, 1);
        put_i32(&mut data, 200, 80);
        put_i32(&mut data, 208, 1);
        put_f32(&mut data, 252, 4.0);
        put_i32(&mut data, 276, 8);
        put_f32(&mut data, 312, 90.0);
        put_f32(&mut data, 320, -1.0);
        data
    }

    #[test]
    fn all_sets_preserve_capsules_box_angles_and_raw_group() {
        let data = two_sets();
        let sets = read_sets(&R(&data), 49, 1).unwrap();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].hitboxes[0].radius, 4.0);
        assert_eq!(sets[1].hitboxes[0].radius, -1.0);
        assert_eq!(sets[1].hitboxes[0].rotation.y, 90.0);
        assert_eq!(sets[1].hitboxes[0].group, 8);
        let older = read_sets(&R(&data), 48, 1).unwrap();
        assert_eq!(older[0].hitboxes[0].radius, -1.0);
        assert_eq!(older[1].hitboxes[0].rotation, Vec3::ZERO);
    }

    #[test]
    fn malformed_later_set_does_not_disappear() {
        for (at, value) in [
            (272, 1),
            (196, -1),
            (200, i32::MAX),
            (280, f32::NAN.to_bits() as i32),
        ] {
            let mut data = two_sets();
            put_i32(&mut data, at, value);
            assert!(
                read_sets(&R(&data), 49, 1).is_err(),
                "accepted mutation at {at}"
            );
        }
        let mut data = two_sets();
        data.truncate(339);
        assert!(read_sets(&R(&data), 49, 1).is_err());
        let data = vec![0; 180];
        assert!(read_sets(&R(&data), 49, 0).unwrap().is_empty());
    }

    #[test]
    fn v44_aabb_names_are_file_relative_and_geometry_is_retained() {
        let mut data = vec![0; 300];
        put_i32(&mut data, 172, 1);
        put_i32(&mut data, 176, 180);
        put_i32(&mut data, 180, 100);
        put_i32(&mut data, 184, 1);
        put_i32(&mut data, 188, 12);
        put_i32(&mut data, 196, 7);
        for (at, value) in [
            (200, -2.0),
            (204, -3.0),
            (208, -4.0),
            (212, 5.0),
            (216, 6.0),
            (220, 7.0),
        ] {
            put_f32(&mut data, at, value);
        }
        put_i32(&mut data, 224, 290);
        data[280..284].copy_from_slice(b"set\0");
        data[290..294].copy_from_slice(b"box\0");
        let sets = read_sets(&R(&data), 44, 1).unwrap();
        assert_eq!(sets[0].name, "set");
        let b = &sets[0].hitboxes[0];
        assert_eq!(
            (b.bone, b.group, b.radius, b.rotation, b.name.as_str()),
            (0, 7, -1.0, Vec3::ZERO, "box")
        );
        assert_eq!(
            (b.min, b.max),
            (Vec3::new(-2.0, -3.0, -4.0), Vec3::new(5.0, 6.0, 7.0))
        );
        // This same field must not silently become absolute for newer files.
        assert!(read_sets(&R(&data), 48, 1).is_err());
        assert!(read_sets(&R(&data), 49, 1).is_err());
        for (at, value) in [
            (192, 1),
            (224, -1),
            (224, 301),
            (228, 1),
            (200, f32::NAN.to_bits() as i32),
            (212, (-10.0f32).to_bits() as i32),
        ] {
            let mut malformed = data.clone();
            put_i32(&mut malformed, at, value);
            assert!(
                read_sets(&R(&malformed), 44, 1).is_err(),
                "accepted mutation at {at}"
            );
        }
        let mut unterminated = data.clone();
        unterminated[290..].fill(b'x');
        assert!(read_sets(&R(&unterminated), 44, 1).is_err());
        for version in [43, 46, 47, 50] {
            assert!(read_sets(&R(&data), version, 1).is_err());
        }
    }

    #[test]
    fn v45_aabbs_use_record_relative_names_and_reject_unknown_extensions() {
        // Synthetic layout matching the historical static-prop records, with
        // a nonempty name to distinguish relative from absolute addressing.
        let mut data = vec![0; 300];
        put_i32(&mut data, 172, 1);
        put_i32(&mut data, 176, 180);
        put_i32(&mut data, 180, 100);
        put_i32(&mut data, 184, 1);
        put_i32(&mut data, 188, 12);
        put_i32(&mut data, 196, 7);
        for (at, value) in [(200, -8.0), (204, -8.0), (212, 8.0), (216, 8.0), (220, 43.0)] {
            put_f32(&mut data, at, value);
        }
        put_i32(&mut data, 224, 98);
        data[280..284].copy_from_slice(b"set\0");
        data[290..294].copy_from_slice(b"box\0");
        let sets = read_sets(&R(&data), 45, 1).unwrap();
        let hitbox = &sets[0].hitboxes[0];
        assert_eq!(sets[0].name, "set");
        assert_eq!(hitbox.name, "box");
        assert_eq!((hitbox.bone, hitbox.group, hitbox.radius, hitbox.rotation), (0, 7, -1.0, Vec3::ZERO));
        assert_eq!((hitbox.min, hitbox.max), (Vec3::new(-8.0, -8.0, 0.0), Vec3::new(8.0, 8.0, 43.0)));
        for (at, value) in [(192, 1), (224, i32::MAX), (228, 1), (212, (-10.0f32).to_bits() as i32)] {
            let mut malformed = data.clone();
            put_i32(&mut malformed, at, value);
            assert!(read_sets(&R(&malformed), 45, 1).is_err(), "accepted mutation at {at}");
        }
    }

    #[test]
    fn capsules_cover_side_caps_parallel_tangents_and_inside() {
        let capsule = PosedHitbox {
            shape: Shape::Capsule {
                start: -Vec3::Z * 2.0,
                end: Vec3::Z * 2.0,
                radius: 1.0,
            },
            group: 1,
            index: 0,
        };
        for (origin, direction, expected) in [
            (Vec3::new(-3.0, 0.0, 0.0), Vec3::X, 2.0),
            (Vec3::new(-3.0, 1.0, 0.0), Vec3::X, 3.0),
            (-Vec3::Z * 5.0, Vec3::Z, 2.0),
            (Vec3::ZERO, Vec3::X, 0.0),
        ] {
            assert!(
                (capsule.ray_intersection(origin, direction, 10.0).unwrap() - expected).abs()
                    < 1e-5
            );
        }
        assert!(
            capsule
                .ray_intersection(Vec3::new(-3.0, 1.01, 0.0), Vec3::X, 10.0)
                .is_none()
        );
        assert!(
            capsule
                .ray_intersection(-Vec3::X * 3.0, Vec3::X, 1.99)
                .is_none()
        );
        let sphere = PosedHitbox {
            shape: Shape::Capsule {
                start: Vec3::ZERO,
                end: Vec3::ZERO,
                radius: 1.0,
            },
            group: 1,
            index: 0,
        };
        assert_eq!(
            sphere.ray_intersection(-Vec3::X * 3.0, Vec3::X, 10.0),
            Some(2.0)
        );
    }

    #[test]
    fn posed_box_rotation_changes_geometry_and_nearest_group_is_authored() {
        let make = |radius, min, max, rotation, group| Hitbox {
            bone: 0,
            group,
            min,
            max,
            radius,
            rotation,
            name: String::new(),
        };
        let set = HitboxSet {
            name: String::new(),
            hitboxes: vec![
                make(0.5, Vec3::X * 10.0, Vec3::X * 10.0, Vec3::ZERO, 1),
                make(
                    -1.0,
                    Vec3::new(-2.0, -0.5, -0.5),
                    Vec3::new(2.0, 0.5, 0.5),
                    Vec3::Y * 90.0,
                    8,
                ),
            ],
        };
        let hit = set
            .ray_intersection(-Vec3::X * 5.0, Vec3::X, 20.0, &[Mat4::IDENTITY])
            .unwrap()
            .unwrap();
        assert_eq!((hit.index, hit.group), (1, 8));
        assert!((hit.distance - 4.5).abs() < 1e-5);
        let posed = set.pose(&[Mat4::IDENTITY]).unwrap();
        let (min, max) = posed[1].bounds();
        assert!((min - Vec3::new(-0.5, -2.0, -0.5)).length() < 1e-5);
        assert!((max - Vec3::new(0.5, 2.0, 0.5)).length() < 1e-5);
        assert!(set.pose(&[Mat4::from_scale(Vec3::splat(2.0))]).is_err());
        assert!(
            set.ray_intersection(Vec3::ZERO, Vec3::X * 2.0, 10.0, &[Mat4::IDENTITY])
                .is_err()
        );
    }

    #[test]
    fn installed_character_sets_retain_all_nineteen_shapes() {
        let Some(game_dir) = vfs::find_csgo_dir() else {
            return;
        };
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&game_dir).unwrap();
        for path in [
            "models/player/custom_player/legacy/ctm_sas.mdl",
            "models/player/custom_player/legacy/tm_phoenix.mdl",
        ] {
            let bytes = fs.read(path).expect("installed Legacy character MDL");
            let model = crate::Model::load_header(&bytes).unwrap();
            assert_eq!(model.hitbox_sets.len(), 1, "{path}");
            let set = &model.hitbox_sets[0];
            assert_eq!(set.hitboxes.len(), 19, "{path}");
            assert_eq!(set.hitboxes.iter().filter(|h| h.radius == -1.0).count(), 4);
            assert_eq!(set.hitboxes.iter().filter(|h| h.radius > 0.0).count(), 15);
            assert!(set.hitboxes.iter().any(|h| h.group == 8));
            assert!(set.hitboxes.iter().any(|h| h.rotation != Vec3::ZERO));
            let pose = crate::skeleton::model_space(
                &model.bones,
                &crate::skeleton::rest_pose(&model.bones),
            );
            assert_eq!(set.pose(&pose).unwrap().len(), 19, "{path}");
        }
    }

    #[test]
    fn installed_v44_props_keep_aabbs_and_render_geometry() {
        let Some(game_dir) = vfs::find_csgo_dir() else {
            return;
        };
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&game_dir).unwrap();
        for stem in [
            "light_fixture01",
            "patio_table2",
            "railing04",
            "railing04long",
            "railing04med",
            "railing04small",
        ] {
            let base = format!("models/props_urban/{stem}");
            let mdl = fs.read(&format!("{base}.mdl")).unwrap();
            let vvd = fs.read(&format!("{base}.vvd")).unwrap();
            let vtx = fs.read(&format!("{base}.dx90.vtx")).unwrap();
            let model = crate::Model::load(&mdl, &vvd, &vtx).unwrap();
            assert_eq!(model.version, 44, "{stem}");
            assert_eq!(model.hitbox_sets.len(), 1, "{stem}");
            let set = &model.hitbox_sets[0];
            assert_eq!(set.name, "default", "{stem}");
            assert_eq!(set.hitboxes.len(), 1, "{stem}");
            let b = &set.hitboxes[0];
            assert_eq!(
                (b.bone, b.group, b.radius, b.rotation),
                (0, 0, -1.0, Vec3::ZERO),
                "{stem}"
            );
            assert!(b.name.is_empty(), "{stem}");
            assert!(
                model
                    .meshes
                    .iter()
                    .any(|m| !m.vertices.is_empty() && !m.indices.is_empty()),
                "{stem}"
            );
        }
    }
}

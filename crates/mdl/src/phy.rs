//! `.phy` collision models.
//!
//! A `.phy` holds one or more solids, each a "compact surface": a set of
//! convex "ledges" made of triangles over a shared point array. We only need
//! the convex pieces, so each ledge becomes a [`ConvexHull`] (its points and
//! triangle faces), converted from physics space (metres, Y down) to Source
//! units (inches, Z up).

use crate::{R, invalid};
use glam::Vec3;
use std::io;

/// Strict, bounded articulated-player metadata; separate from static-prop parsing.
pub mod ragdoll;

/// Inches per metre as used by the physics conversion.
const METERS_TO_INCHES: f32 = 1.0 / 0.0254;

#[derive(Clone, Debug, Default)]
pub struct ConvexHull {
    pub points: Vec<Vec3>,
    /// Triangles as indices into `points`.
    pub triangles: Vec<[u32; 3]>,
}

impl ConvexHull {
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut mn = Vec3::splat(f32::MAX);
        let mut mx = Vec3::splat(f32::MIN);
        for p in &self.points {
            mn = mn.min(*p);
            mx = mx.max(*p);
        }
        (mn, mx)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Phy {
    /// Convex hulls of all solids.
    pub hulls: Vec<ConvexHull>,
    /// Trailing key/value text (surface props, masses...).
    pub text: String,
}

fn physics_to_source(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, z, -y) * METERS_TO_INCHES
}

impl Phy {
    pub fn parse(bytes: &[u8]) -> io::Result<Phy> {
        let r = R(bytes);
        let header_size = r.i32(0)?.max(0) as usize;
        let solid_count = r.i32(8)?.max(0) as usize;
        let mut phy = Phy::default();
        let mut p = header_size;
        for _ in 0..solid_count {
            let size = r.i32(p)?.max(0) as usize;
            let start = p + 4;
            let solid = bytes
                .get(start..start + size)
                .ok_or_else(|| invalid("phy solid runs past end"))?;
            parse_solid(solid, &mut phy.hulls)?;
            p = start + size;
        }
        if let Some(rest) = bytes.get(p..) {
            phy.text = String::from_utf8_lossy(rest)
                .trim_end_matches('\0')
                .to_string();
        }
        Ok(phy)
    }
}

fn parse_solid(solid: &[u8], hulls: &mut Vec<ConvexHull>) -> io::Result<()> {
    // Newer files wrap the surface in a "VPHY" header.
    let surface_start = if solid.get(0..4) == Some(b"VPHY") {
        let model_type = R(solid).i16(6)?;
        if model_type != 0 {
            // Only polygon soups (type 0) carry compact ledges.
            return Ok(());
        }
        28
    } else {
        0
    };
    let s = solid
        .get(surface_start..)
        .ok_or_else(|| invalid("phy surface truncated"))?;
    let r = R(s);
    if s.len() < 48 {
        return Err(invalid("phy compact surface too small"));
    }
    let tree = usize::try_from(r.i32(32)?).map_err(|_| invalid("negative phy tree offset"))?;
    if tree < 48 || tree.checked_add(28).is_none_or(|end| end > s.len()) {
        return Err(invalid("phy ledge tree outside surface"));
    }
    // Installed Legacy compact surfaces share points between ledges. A
    // ledge's packed size counts its referenced points too; it is NOT the
    // distance to the next ledge. Walking by it skips roof/support pieces
    // and can decode triangles or points as additional collision geometry.
    // The 28-byte tree nodes locate the real pieces: the left child follows
    // a branch, the right child and leaf ledge use node-relative offsets.
    // Branches may also reference enclosing approximation hulls. Those must
    // never become solids: they would fill the gaps between concave pieces.
    let mut pending = vec![tree];
    let mut visited = std::collections::HashSet::new();
    let mut ledges = std::collections::BTreeSet::new();
    while let Some(node) = pending.pop() {
        if visited.len() >= 100_000 || !visited.insert(node) {
            return Err(invalid("phy ledge tree repeats or exceeds node limit"));
        }
        let end = node.checked_add(28).ok_or_else(|| invalid("phy tree node overflow"))?;
        let nr = R(s.get(node..end).ok_or_else(|| invalid("phy tree node outside surface"))?);
        let right = nr.i32(0)?;
        if right == 0 {
            let offset = node as i64 + nr.i32(4)? as i64;
            if let Ok(offset) = usize::try_from(offset) {
                if offset >= 48 && offset.checked_add(16).is_some_and(|end| end <= tree) {
                    ledges.insert(offset);
                }
            }
        } else {
            if right < 56 || right % 28 != 0 {
                return Err(invalid("invalid phy tree child offset"));
            }
            pending.push(node.checked_add(right as usize).ok_or_else(|| invalid("phy tree child overflow"))?);
            pending.push(end);
        }
    }
    // Stable storage order preserves tie behavior between adjacent pieces.
    // Restrict every point/triangle read to the geometry before the tree;
    // finite tree metadata must not be accepted as a vertex on bad input.
    let geometry = &s[..tree];
    for offset in ledges {
        let point_offset = r.i32(offset)?;
        let packed = r.i32(offset + 8)? as u32;
        let size = ((packed >> 8) as usize) * 16;
        let n_triangles = r.i16(offset + 12)?.max(0) as usize;
        let triangles_size = 16 + n_triangles * 16;
        if size < triangles_size || size > s.len()
            || offset.checked_add(triangles_size).is_none_or(|end| end > tree) {
            continue;
        }
        let Ok(points_at) = usize::try_from(offset as i64 + point_offset as i64) else {
            continue;
        };
        if points_at < offset + triangles_size {
            continue;
        }
        // A bad reference invalidates the entire piece. Substituting ZERO
        // fabricates collision geometry, including mixed valid/invalid hulls
        // which downstream zero-width rejection cannot detect.
        if n_triangles >= 4 {
            if let Ok(hull) = read_ledge(geometry, offset, n_triangles, points_at) {
                hulls.push(hull);
            }
        }
    }
    Ok(())
}

fn read_ledge(
    s: &[u8],
    offset: usize,
    n_triangles: usize,
    points_at: usize,
) -> io::Result<ConvexHull> {
    let r = R(s);
    let mut hull = ConvexHull::default();
    let mut remap = std::collections::HashMap::new();
    for t in 0..n_triangles {
        let to = offset + 16 + t * 16;
        let mut tri = [0u32; 3];
        for (e, slot) in tri.iter_mut().enumerate() {
            let edge = r.i32(to + 4 + e * 4)? as u32;
            let point_index = (edge & 0xFFFF) as usize;
            let idx = if let Some(&idx) = remap.get(&point_index) {
                idx
            } else {
                let po = points_at
                    .checked_add(point_index * 16)
                    .ok_or_else(|| invalid("phy point offset overflow"))?;
                let end = po
                    .checked_add(12)
                    .ok_or_else(|| invalid("phy point offset overflow"))?;
                let point = s
                    .get(po..end)
                    .ok_or_else(|| invalid("phy point outside surface"))?;
                let p = R(point).vec3(0)?;
                let pt = physics_to_source(p.x, p.y, p.z);
                // Retain the existing coordinate limit, checking after unit
                // conversion as well as rejecting nonfinite stored values.
                if !pt.is_finite() || pt.abs().max_element() >= 65_536.0 {
                    return Err(invalid("phy point is nonfinite or out of range"));
                }
                let idx = hull.points.len() as u32;
                hull.points.push(pt);
                remap.insert(point_index, idx);
                idx
            };
            *slot = idx;
        }
        hull.triangles.push(tri);
    }
    Ok(hull)
}

#[cfg(test)]
mod tests;

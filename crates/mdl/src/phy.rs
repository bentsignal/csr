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
    let ledgetree_root = r.i32(32)?.max(0) as usize;

    // Ledges are packed one after another from offset 48. The shared point
    // array follows the last ledge, so stop at the lowest point offset seen.
    let mut offset = 48usize;
    let mut points_start = ledgetree_root.min(s.len());
    let mut guard = 0;
    while offset + 16 <= points_start && guard < 100_000 {
        guard += 1;
        let point_offset = r.i32(offset)?;
        let packed = r.i32(offset + 8)? as u32;
        let size = ((packed >> 8) as usize) * 16;
        let n_triangles = r.i16(offset + 12)?.max(0) as usize;
        // The sequential scan can reach bytes that are not a ledge. Never
        // decode triangles outside the candidate's declared extent, or walk
        // past this surface. Keep any valid pieces already read.
        let Some(next_offset) = offset.checked_add(size) else {
            break;
        };
        if size < 16 || next_offset > s.len() || 16 + n_triangles * 16 > size {
            break;
        }
        let Ok(points_at) = usize::try_from(offset as i64 + point_offset as i64) else {
            offset = next_offset;
            continue;
        };
        if points_at > offset {
            points_start = points_start.min(points_at);
        }
        // A bad reference invalidates the entire piece. Substituting ZERO
        // fabricates collision geometry, including mixed valid/invalid hulls
        // which downstream zero-width rejection cannot detect.
        if n_triangles >= 4 {
            if let Ok(hull) = read_ledge(s, offset, n_triangles, points_at) {
                hulls.push(hull);
            }
        }
        offset = next_offset;
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

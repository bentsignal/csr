//! Authored articulated PHY data. No solver, impulses or animation policy.
//!
//! This additive reader follows the installed Legacy VPHY/type-0 layout and
//! the existing clean-room geometry reader. Limits are defensive parser policy,
//! not claims about the original engine. Static [`super::Phy`] stays unchanged.
use super::{ConvexHull, physics_to_source};
use crate::{R, invalid, skeleton::Bone};
use std::{
    collections::{HashMap, HashSet},
    io,
};

pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SOLIDS: usize = 64;
const MAX_HULLS: usize = 512;
const MAX_POINTS: usize = 262_144;
const MAX_TRIANGLES: usize = 524_288;
const MAX_TEXT: usize = 1024 * 1024;
const MAX_FIELDS: usize = 8192;
const MAX_NAME: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SolidId(pub usize);

#[derive(Clone, Debug)]
pub struct Solid {
    pub id: SolidId,
    pub name: String,
    pub parent: Option<String>,
    pub hulls: Vec<ConvexHull>,
    pub surface_prop: String,
    pub mass: f32,
    pub damping: f32,
    pub rotational_damping: f32,
    pub inertia: f32,
    pub volume: f32,
    pub mass_bias: Option<f32>,
}

/// Values are retained in the authored axis order/units. Applying them to a
/// solver requires a separately verified basis and unit convention.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisLimit {
    pub min: f32,
    pub max: f32,
    pub friction: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Constraint {
    pub parent: SolidId,
    pub child: SolidId,
    pub axes: [AxisLimit; 3],
}

/// Original block/field order, including repeated collisionpairs/jointmerges
/// and uninterpreted fields, is retained instead of collapsing to a map.
#[derive(Clone, Debug, PartialEq)]
pub struct MetadataBlock {
    pub kind: String,
    pub fields: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct Ragdoll {
    pub checksum: u32,
    /// Indexed by binary solid ID, independent of text declaration order.
    pub solids: Vec<Solid>,
    pub constraints: Vec<Constraint>,
    pub collision_pairs: Vec<(SolidId, SolidId)>,
    pub root: SolidId,
    pub total_mass: f32,
    pub joint_merges: Vec<(String, String)>,
    pub metadata: Vec<MetadataBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoneBinding {
    /// Bone index per solid ID, not the bone's parent or its hitbox index.
    pub solid_bones: Vec<usize>,
}

impl Ragdoll {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(invalid("ragdoll PHY exceeds byte limit"));
        }
        let r = R(bytes);
        let header = count(r.i32(0)?, bytes.len(), "header size")?;
        if header < 16 {
            return Err(invalid("ragdoll PHY header too small"));
        }
        let count = count(r.i32(8)?, MAX_SOLIDS, "solid count")?;
        if count == 0 {
            return Err(invalid("ragdoll PHY has no solids"));
        }
        let checksum = r.i32(12)? as u32;
        let mut at = header;
        let mut geometry = Vec::with_capacity(count);
        let mut budget = Budget::default();
        for _ in 0..count {
            let size = count_len(r.i32(at)?, bytes.len(), "solid size")?;
            let start = add(at, 4)?;
            at = add(start, size)?;
            let solid = bytes
                .get(start..at)
                .ok_or_else(|| invalid("ragdoll solid past end"))?;
            geometry.push(read_geometry(solid, &mut budget)?);
        }
        let rest = bytes
            .get(at..)
            .ok_or_else(|| invalid("ragdoll text past end"))?;
        if rest.len() > MAX_TEXT {
            return Err(invalid("ragdoll text exceeds limit"));
        }
        let text = std::str::from_utf8(rest)
            .map_err(|_| invalid("ragdoll text is not UTF-8"))?
            .trim_end_matches('\0');
        if text.contains('\0') {
            return Err(invalid("interior NUL in ragdoll text"));
        }
        let metadata = parse_metadata(text)?;
        let mut solids: Vec<Option<Solid>> = vec![None; count];
        let mut constraints = Vec::new();
        let mut collision_pairs = Vec::new();
        let mut edit = None;
        for block in &metadata {
            match block.kind.as_str() {
                "solid" => {
                    let id = integer(block, "index", count - 1)?;
                    if solids[id].is_some() {
                        return Err(invalid("duplicate ragdoll solid index"));
                    }
                    solids[id] = Some(Solid {
                        id: SolidId(id),
                        name: name(block, "name")?,
                        parent: optional(block, "parent")?.map(checked_name).transpose()?,
                        hulls: std::mem::take(&mut geometry[id]),
                        surface_prop: name(block, "surfaceprop")?,
                        mass: number(block, "mass", true)?,
                        damping: number(block, "damping", false)?,
                        rotational_damping: number(block, "rotdamping", false)?,
                        inertia: number(block, "inertia", true)?,
                        volume: number(block, "volume", true)?,
                        mass_bias: optional(block, "massbias")?
                            .map(|v| parse_number(v, true))
                            .transpose()?,
                    });
                }
                "ragdollconstraint" => {
                    if constraints.len() >= count - 1 {
                        return Err(invalid("too many ragdoll constraints"));
                    }
                    let axis = |prefix: &str| -> io::Result<AxisLimit> {
                        let min = signed_number(block, &format!("{prefix}min"))?;
                        let max = signed_number(block, &format!("{prefix}max"))?;
                        if min > max {
                            return Err(invalid("reversed ragdoll axis limits"));
                        }
                        Ok(AxisLimit {
                            min,
                            max,
                            friction: number(block, &format!("{prefix}friction"), false)?,
                        })
                    };
                    constraints.push(Constraint {
                        parent: SolidId(integer(block, "parent", count - 1)?),
                        child: SolidId(integer(block, "child", count - 1)?),
                        axes: [axis("x")?, axis("y")?, axis("z")?],
                    });
                }
                "collisionrules" => {
                    for (key, value) in &block.fields {
                        if key == "collisionpair" {
                            if collision_pairs.len() >= MAX_SOLIDS * MAX_SOLIDS {
                                return Err(invalid("too many collision pairs"));
                            }
                            let (a, b) = pair(value)?;
                            let a = parse_index(a, count - 1)?;
                            let b = parse_index(b, count - 1)?;
                            if a == b {
                                return Err(invalid("self collision pair"));
                            }
                            collision_pairs.push((SolidId(a), SolidId(b)));
                        }
                    }
                }
                "editparams" => {
                    if edit.replace(block).is_some() {
                        return Err(invalid("duplicate editparams"));
                    }
                }
                _ => {} // Retained verbatim as metadata, never executed.
            }
        }
        let solids: Vec<Solid> = solids
            .into_iter()
            .map(|s| s.ok_or_else(|| invalid("missing solid metadata")))
            .collect::<io::Result<_>>()?;
        let edit = edit.ok_or_else(|| invalid("missing editparams"))?;
        let root_name = name(edit, "rootname")?;
        let root = solids
            .iter()
            .position(|s| s.name == root_name)
            .ok_or_else(|| invalid("unknown ragdoll root"))?;
        let total_mass = number(edit, "totalmass", true)?;
        let mut joint_merges = Vec::new();
        for (key, value) in &edit.fields {
            if key == "jointmerge" {
                let (a, b) = pair(value)?;
                joint_merges.push((checked_name(a)?, checked_name(b)?));
            }
        }
        validate_tree(&solids, &constraints, root)?;
        Ok(Self {
            checksum,
            solids,
            constraints,
            collision_pairs,
            root: SolidId(root),
            total_mass,
            joint_merges,
            metadata,
        })
    }

    /// Explicitly bind only the exact matching MDL. Names use ASCII-insensitive
    /// matching (installed jointmerge names differ in case from MDL bones).
    /// Names must be unique; merged targets may be non-solid MDL bones.
    pub fn bind(&self, mdl_checksum: u32, bones: &[Bone]) -> io::Result<BoneBinding> {
        if mdl_checksum != self.checksum {
            return Err(invalid("MDL/PHY checksum mismatch"));
        }
        let mut names = HashMap::new();
        for (index, bone) in bones.iter().enumerate() {
            if names
                .insert(bone.name.to_ascii_lowercase(), index)
                .is_some()
            {
                return Err(invalid("ambiguous skeleton bone name"));
            }
        }
        let solid_bones = self
            .solids
            .iter()
            .map(|s| {
                names
                    .get(&s.name.to_ascii_lowercase())
                    .copied()
                    .ok_or_else(|| invalid("ragdoll bone missing from MDL"))
            })
            .collect::<io::Result<_>>()?;
        let mut unique_bones = HashSet::new();
        if solid_bones.iter().any(|bone| !unique_bones.insert(*bone)) {
            return Err(invalid("multiple solids bind to the same skeleton bone"));
        }
        for (a, b) in &self.joint_merges {
            if !names.contains_key(&a.to_ascii_lowercase())
                || !names.contains_key(&b.to_ascii_lowercase())
            {
                return Err(invalid("jointmerge bone missing from MDL"));
            }
        }
        Ok(BoneBinding { solid_bones })
    }
}

fn add(a: usize, b: usize) -> io::Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| invalid("ragdoll offset overflow"))
}
fn count(v: i32, max: usize, what: &str) -> io::Result<usize> {
    let n = usize::try_from(v).map_err(|_| invalid(format!("negative {what}")))?;
    if n > max {
        Err(invalid(format!("{what} exceeds limit")))
    } else {
        Ok(n)
    }
}
fn count_len(v: i32, max: usize, what: &str) -> io::Result<usize> {
    count(v, max, what)
}
fn parse_index(v: &str, max: usize) -> io::Result<usize> {
    let n = v
        .parse::<usize>()
        .map_err(|_| invalid("invalid solid index"))?;
    if n > max {
        Err(invalid("solid index out of range"))
    } else {
        Ok(n)
    }
}
fn integer(b: &MetadataBlock, key: &str, max: usize) -> io::Result<usize> {
    parse_index(required(b, key)?, max)
}
fn optional<'a>(b: &'a MetadataBlock, key: &str) -> io::Result<Option<&'a str>> {
    let mut values = b
        .fields
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.as_str());
    let value = values.next();
    if values.next().is_some() {
        return Err(invalid(format!("duplicate {key}")));
    }
    Ok(value)
}
fn required<'a>(b: &'a MetadataBlock, key: &str) -> io::Result<&'a str> {
    optional(b, key)?.ok_or_else(|| invalid(format!("missing {key}")))
}
fn checked_name(v: &str) -> io::Result<String> {
    if v.is_empty() || v.len() > MAX_NAME || v.chars().any(char::is_control) {
        Err(invalid("invalid ragdoll name"))
    } else {
        Ok(v.to_string())
    }
}
fn name(b: &MetadataBlock, key: &str) -> io::Result<String> {
    checked_name(required(b, key)?)
}
fn signed_number(b: &MetadataBlock, key: &str) -> io::Result<f32> {
    let v = required(b, key)?
        .parse::<f32>()
        .map_err(|_| invalid("invalid ragdoll number"))?;
    if !v.is_finite() {
        Err(invalid("nonfinite ragdoll number"))
    } else {
        Ok(v)
    }
}
fn parse_number(v: &str, positive: bool) -> io::Result<f32> {
    let v = v
        .parse::<f32>()
        .map_err(|_| invalid("invalid ragdoll number"))?;
    if !v.is_finite() || v < 0. || (positive && v == 0.) {
        Err(invalid("invalid physical value"))
    } else {
        Ok(v)
    }
}
fn number(b: &MetadataBlock, key: &str, positive: bool) -> io::Result<f32> {
    parse_number(required(b, key)?, positive)
}
fn pair(v: &str) -> io::Result<(&str, &str)> {
    let (a, b) = v
        .split_once(',')
        .ok_or_else(|| invalid("invalid ragdoll pair"))?;
    if b.contains(',') {
        return Err(invalid("invalid ragdoll pair"));
    }
    Ok((a.trim(), b.trim()))
}

fn validate_tree(solids: &[Solid], constraints: &[Constraint], root: usize) -> io::Result<()> {
    let mut names = HashSet::new();
    for s in solids {
        if !names.insert(s.name.to_ascii_lowercase()) {
            return Err(invalid("duplicate solid name"));
        }
    }
    let mut parent = vec![None; solids.len()];
    for c in constraints {
        if c.parent == c.child
            || c.child.0 == root
            || parent[c.child.0].replace(c.parent.0).is_some()
        {
            return Err(invalid("invalid ragdoll parent graph"));
        }
    }
    for (id, s) in solids.iter().enumerate() {
        let named_parent = parent[id].map(|p| solids[p].name.as_str());
        if s.parent.as_deref() != named_parent {
            return Err(invalid("solid parent disagrees with constraint"));
        }
        let mut at = id;
        for depth in 0..=solids.len() {
            if at == root {
                break;
            }
            if depth == solids.len() {
                return Err(invalid("ragdoll constraint cycle"));
            }
            at = parent[at].ok_or_else(|| invalid("disconnected ragdoll constraint graph"))?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct Budget {
    hulls: usize,
    points: usize,
    triangles: usize,
}
fn read_geometry(bytes: &[u8], budget: &mut Budget) -> io::Result<Vec<ConvexHull>> {
    if bytes.get(..4) != Some(b"VPHY") || R(bytes).i16(6)? != 0 {
        return Err(invalid("unsupported ragdoll solid format"));
    }
    let surface = bytes
        .get(28..)
        .ok_or_else(|| invalid("truncated VPHY header"))?;
    let r = R(surface);
    if surface.len() < 48 {
        return Err(invalid("truncated compact surface"));
    }
    let tree = count(r.i32(32)?, surface.len(), "ledge tree offset")?;
    let mut end = tree;
    let mut at = 48;
    let mut out = Vec::new();
    while add(at, 16)? <= end {
        let packed = r.i32(at + 8)? as u32;
        let size = ((packed >> 8) as usize) * 16;
        if size == 0 {
            break;
        }
        let triangles = count(r.i16(at + 12)? as i32, MAX_TRIANGLES, "triangle count")?;
        if triangles < 4 || size < 16 + triangles * 16 || add(at, size)? > surface.len() {
            return Err(invalid("invalid compact ledge extent"));
        }
        let points_at = at
            .checked_add_signed(r.i32(at)? as isize)
            .ok_or_else(|| invalid("invalid point offset"))?;
        if points_at <= at {
            return Err(invalid("invalid compact point location"));
        }
        end = end.min(points_at);
        if add(at, size)? > end {
            return Err(invalid("ledge overlaps point data"));
        }
        budget.hulls += 1;
        budget.triangles += triangles;
        if budget.hulls > MAX_HULLS || budget.triangles > MAX_TRIANGLES {
            return Err(invalid("ragdoll geometry budget exceeded"));
        }
        let mut hull = ConvexHull::default();
        let mut remap = HashMap::new();
        for t in 0..triangles {
            let mut tri = [0; 3];
            for (e, slot) in tri.iter_mut().enumerate() {
                let index = (r.i32(at + 16 + t * 16 + 4 + e * 4)? as u32 & 0xffff) as usize;
                let next = hull.points.len() as u32;
                if let Some(mapped) = remap.get(&index) {
                    *slot = *mapped;
                    continue;
                }
                let p = add(points_at, index * 16)?;
                // The observed shared point array ends at the ledge tree;
                // accepting finite tree/header bytes as vertices is corruption.
                if add(p, 16)? > tree {
                    return Err(invalid("compact point overlaps ledge tree"));
                }
                let value = physics_to_source(r.f32(p)?, r.f32(add(p, 4)?)?, r.f32(add(p, 8)?)?);
                if !value.is_finite() || value.abs().max_element() >= 65_536. {
                    return Err(invalid("invalid compact point"));
                }
                budget.points += 1;
                if budget.points > MAX_POINTS {
                    return Err(invalid("ragdoll point budget exceeded"));
                }
                hull.points.push(value);
                remap.insert(index, next);
                *slot = next;
            }
            hull.triangles.push(tri);
        }
        if hull.points.len() < 4 {
            return Err(invalid("ragdoll hull has too few points"));
        }
        out.push(hull);
        at = add(at, size)?;
    }
    if out.is_empty() {
        return Err(invalid("ragdoll solid has no supported hulls"));
    }
    Ok(out)
}

fn parse_metadata(text: &str) -> io::Result<Vec<MetadataBlock>> {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut tokens = Vec::new();
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes.get(at..at + 2) == Some(b"//") {
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        if tokens.len() >= MAX_FIELDS * 2 + 1024 {
            return Err(invalid("ragdoll token limit exceeded"));
        }
        match bytes[at] {
            b'{' | b'}' => {
                tokens.push(text[at..at + 1].to_string());
                at += 1;
            }
            b'"' => {
                at += 1;
                let mut value = String::new();
                let start = at;
                while at < bytes.len() && bytes[at] != b'"' {
                    if bytes[at] == b'\\' {
                        return Err(invalid("unsupported escaped ragdoll token"));
                    }
                    at += 1;
                }
                if at == bytes.len() {
                    return Err(invalid("unterminated ragdoll token"));
                }
                value.push_str(&text[start..at]);
                tokens.push(value);
                at += 1;
            }
            _ => {
                let start = at;
                while at < bytes.len()
                    && !bytes[at].is_ascii_whitespace()
                    && !b"{}\"".contains(&bytes[at])
                {
                    at += 1;
                }
                if start == at {
                    return Err(invalid("invalid ragdoll token"));
                }
                tokens.push(text[start..at].to_string());
            }
        }
    }
    let mut blocks = Vec::new();
    let mut at = 0;
    let mut fields = 0;
    while at < tokens.len() {
        if blocks.len() >= 256
            || at + 1 >= tokens.len()
            || tokens[at + 1] != "{"
            || matches!(tokens[at].as_str(), "{" | "}")
        {
            return Err(invalid("invalid ragdoll metadata block"));
        }
        let kind = tokens[at].clone();
        at += 2;
        let mut pairs = Vec::new();
        while at < tokens.len() && tokens[at] != "}" {
            if at + 1 >= tokens.len()
                || matches!(tokens[at].as_str(), "{" | "}")
                || matches!(tokens[at + 1].as_str(), "{" | "}")
            {
                return Err(invalid("nested or incomplete ragdoll field"));
            }
            fields += 1;
            if fields > MAX_FIELDS {
                return Err(invalid("ragdoll field limit exceeded"));
            }
            pairs.push((tokens[at].clone(), tokens[at + 1].clone()));
            at += 2;
        }
        if at >= tokens.len() {
            return Err(invalid("unterminated ragdoll block"));
        }
        at += 1;
        blocks.push(MetadataBlock {
            kind,
            fields: pairs,
        });
    }
    Ok(blocks)
}

#[cfg(test)]
mod tests;

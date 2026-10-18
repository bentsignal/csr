//! Mounted brush/studio breakables. Definitions/geometry are immutable and scoped by
//! the owning map ContentId and BSP entity ordinal, just like map doors.
use crate::Convex;
use glam::Vec3;

pub const MAX_BREAKABLES: usize = 1024;
pub const MAX_HEALTH: f32 = 1_000_000.0;

#[derive(Debug)]
pub struct Breakable {
    pub entity: usize,
    /// BSP brush model; studio definitions use `studio` instead.
    pub model: usize,
    pub origin: Vec3,
    pub health: f32,
    /// Hammer brush material; -1 for studio props, whose surfaces use MDL data.
    pub material: i64,
    pub prop_data: String,
    pub bullet_scale: f32,
    pub club_scale: f32,
    pub minimum_damage: f32,
    pub trigger_only: bool,
    pub solids: Vec<Convex>,
    pub studio: Option<Studio>,
}

impl Breakable {
    /// Surface contacts are kept just outside the brush by DIST_EPSILON.
    /// Used only to attach cosmetic decals to their immutable brush owner.
    pub fn contains_surface(&self, point: Vec3, normal: Vec3) -> bool {
        let inside = point - normal * (crate::DIST_EPSILON * 2.0);
        self.solids.iter().any(|solid| inside.cmpge(solid.mins).all() && inside.cmple(solid.maxs).all()
            && solid.planes.iter().all(|plane| plane.normal.dot(inside) <= plane.dist + 0.001))
    }

    pub(super) fn load(entity: usize, ent: &kv::Block, map: &bsp::Bsp) -> Option<Self> {
        if !ent.str("classname").is_some_and(|name| name.eq_ignore_ascii_case("func_breakable"))
            || ent.i64("startdisabled") == Some(1)
            || ent.str("parentname").is_some_and(|s| !s.is_empty()) { return None; }
        let angles = ent.str("angles").and_then(bsp::parse_origin).unwrap_or(Vec3::ZERO);
        // Rotating/parented machinery needs a shared transform/I/O path.
        if !angles.is_finite() || angles != Vec3::ZERO { return None; }
        let model = ent.str("model")?.strip_prefix('*')?.parse::<usize>().ok()?;
        if model == 0 || map.models.get(model).is_none() { return None; }
        let origin = ent.str("origin").and_then(bsp::parse_origin).unwrap_or(Vec3::ZERO);
        if !origin.is_finite() || origin.abs().max_element() >= 131_072.0 { return None; }
        let bounded = |key, default| ent.f32(key).filter(|v| v.is_finite() && *v >= 0.0 && *v <= MAX_HEALTH).unwrap_or(default);
        // Hammer's brush prop-data choices are format metadata, not asset
        // paths. A named choice also permits private packs to supply presets.
        let prop_data = match ent.str("propdata").unwrap_or("0") {
            "0" => "", "1" => "Wooden.Tiny", "2" => "Wooden.Small",
            "3" => "Wooden.Medium", "4" => "Wooden.Large", "5" => "Wooden.Huge",
            "6" => "Metal.Small", "7" => "Metal.Medium", "8" => "Metal.Large",
            "9" => "Cardboard.Small", "10" => "Cardboard.Medium", "11" => "Cardboard.Large",
            "12" => "Stone.Small", "13" => "Stone.Medium", "14" => "Stone.Large", "15" => "Stone.Huge",
            "16" => "Glass.Small", "17" => "Plastic.Small", "18" => "Plastic.Medium", "19" => "Plastic.Large",
            "20" => "Pottery.Small", "21" => "Pottery.Medium", "22" => "Pottery.Large", "23" => "Pottery.Huge",
            "24" => "Glass.Window", name => name,
        }.to_owned();
        Some(Self { entity, model, origin, health: bounded("health", 0.0),
            material: ent.i64("material").unwrap_or(0), prop_data,
            bullet_scale: bounded("dmg.bullets", 1.0), club_scale: bounded("dmg.club", 1.0),
            minimum_damage: bounded("minhealthdmg", 0.0),
            trigger_only: ent.i64("spawnflags").unwrap_or(0) & 1 != 0, solids: Vec::new(), studio: None })
    }

    /// Read only mounted propdata.txt, including bounded base inheritance.
    /// Explicit prop data overrides health/damage, as authored by the mapper.
    pub fn apply_prop_data(&mut self, root: &kv::Block) {
        if self.studio.is_some() { return; }
        fn value(root: &kv::Block, name: &str, key: &str, depth: usize) -> Option<f32> {
            if depth >= 16 || name.is_empty() { return None; }
            let block = root.block(name)?;
            block.f32(key).or_else(|| value(root, block.str("base")?, key, depth + 1))
        }
        for (key, target) in [("health", &mut self.health), ("dmg.bullets", &mut self.bullet_scale),
            ("dmg.club", &mut self.club_scale)] {
            if let Some(v) = value(root, &self.prop_data, key, 0)
                .filter(|v| v.is_finite() && *v >= 0.0 && *v <= MAX_HEALTH) { *target = v; }
        }
    }
}

/// The same authored transform is used for PHY collision and actual studio meshes.
#[derive(Debug)]
pub struct Studio {
    pub model: String,
    pub angles: Vec3,
    pub scale: f32,
    pub skin: usize,
    pub body: u32,
    pub tint: Vec3,
}

/// A bounded immediate OnBreak/Kill edge, derived from health rather than new
/// mutable entity-I/O state. Any destroyed source removes this shared brush.
#[derive(Debug)]
pub struct Dependency {
    pub entity: usize,
    pub model: usize,
    pub origin: Vec3,
    pub killed_by: Vec<usize>,
    pub solids: Vec<Convex>,
}

fn declared_value(block: &kv::Block, presets: &kv::Block, key: &str, depth: usize) -> Option<f32> {
    if depth >= 16 { return None; }
    if let Some(value) = block.get(key) { return value.as_str()?.parse().ok(); }
    declared_value(presets.block(block.str("base")?)?, presets, key, depth + 1)
}

pub type ModelDataLoader<'a> = dyn Fn(&str) -> Option<Vec<u8>> + 'a;

pub(super) fn load_props(map: &bsp::Bsp, hulls: &crate::PropHullLoader<'_>,
    models: &ModelDataLoader<'_>, presets: &kv::Block) -> Vec<Breakable> {
    let mut out = Vec::new();
    // Cache both successes and failures: map instances share mounted metadata.
    let mut cache = std::collections::HashMap::<String, Option<(kv::Block, Vec<mdl::ConvexHull>)>>::new();
    for (entity, ent) in map.entities.iter().enumerate() {
        if out.len() == MAX_BREAKABLES { break; }
        if !ent.str("classname").is_some_and(|c| c.eq_ignore_ascii_case("prop_dynamic"))
            || ent.i64("startdisabled") == Some(1) || ent.i64("solid") != Some(6)
            || ent.str("parentname").is_some_and(|s| !s.is_empty())
            || ent.str("damagefilter").is_some_and(|s| !s.is_empty())
            || ent.i64("rendermode").unwrap_or(0) != 0
            || ent.i64("renderfx").unwrap_or(0) != 0
            || ent.f32("renderamt").unwrap_or(255.0) != 255.0 { continue; }
        let Some(model) = ent.str("model").filter(|s| !s.starts_with('*')) else { continue; };
        let data = cache.entry(model.to_owned()).or_insert_with(|| {
            let bytes = models(model)?;
            let doc = kv::parse(mdl::Model::keyvalues(&bytes).ok()?).ok()?;
            let root = doc.block("mdlkeyvalue").unwrap_or(&doc);
            let data = root.block("prop_data")?.clone();
            // Avoid even loading PHY for an ordinary indestructible prop.
            declared_value(&data, presets, "health", 0)
                .filter(|v| v.is_finite() && *v > 0.0 && *v <= MAX_HEALTH)?;
            Some((data, hulls(model)?))
        });
        let Some((data, hulls)) = data else { continue; };
        let bounded = |key| declared_value(data, presets, key, 0)
            .filter(|v| v.is_finite() && (0.0..=MAX_HEALTH).contains(v));
        // No model health means no admission, even if the BSP names it a vent.
        let Some(health) = bounded("health").filter(|v| *v > 0.0) else { continue; };
        // Unlike legacy brush compatibility defaults, studio admission requires
        // an authored bullet AND club policy (local or inherited).
        let (Some(bullet_scale), Some(club_scale)) = (bounded("dmg.bullets"), bounded("dmg.club")) else { continue; };
        let Some(origin) = ent.str("origin").and_then(bsp::parse_origin) else { continue; };
        let angles = ent.str("angles").and_then(bsp::parse_origin).unwrap_or(Vec3::ZERO);
        let scale = ent.f32("modelscale").unwrap_or(1.0);
        let skin = ent.i64("skin").unwrap_or(0);
        let body = ent.i64("body").unwrap_or(0);
        // The supported slice is the authored static pose, not animated machinery.
        if !origin.is_finite() || origin.abs().max_element() >= 131_072.0 || !angles.is_finite()
            || !scale.is_finite() || scale <= 0.0 || scale > 16.0 || skin < 0 || skin > 65535
            || !(0..=u32::MAX as i64).contains(&body)
            || ent.str("defaultanim").is_some_and(|s| !s.is_empty())
            || ent.i64("randomanimation").unwrap_or(0) != 0 { continue; }
        let rotation = mdl::angles_to_mat3(angles);
        let solids: Vec<_> = hulls.iter().filter_map(|hull| {
            let points: Vec<_> = hull.points.iter().map(|p| origin + rotation * (*p * scale)).collect();
            let solid = crate::convex_from_hull(&points, &hull.triangles, crate::contents::SOLID, 0)?;
            // Runtime shapes bypass add_convex; retain its finite/world bounds.
            let sane = |v: Vec3| v.is_finite() && v.abs().max_element() < 131_072.0;
            (sane(solid.mins) && sane(solid.maxs)
                && solid.planes.iter().all(|p| p.normal.is_finite() && p.dist.is_finite())).then_some(solid)
        }).collect();
        if solids.is_empty() { continue; }
        let tint = ent.str("rendercolor").and_then(bsp::parse_origin).filter(|v| v.is_finite())
            .unwrap_or(Vec3::splat(255.0)).clamp(Vec3::ZERO, Vec3::splat(255.0)) / 255.0;
        out.push(Breakable { entity, model: 0, origin, health, material: -1,
            prop_data: data.str("base").unwrap_or("").to_owned(),
            bullet_scale, club_scale, minimum_damage: 0.0,
            trigger_only: false, solids, studio: Some(Studio { model: model.to_owned(),
                angles, scale, skin: skin as usize, body: body as u32, tint }) });
    }
    out
}

/// Delayed/scripted outputs retain the existing path; no general I/O executor.
fn immediate_outputs<'a>(ent: &'a kv::Block, input: &'a str) -> impl Iterator<Item = &'a str> {
    ent.get_all("OnBreak").take(64).filter_map(move |value| {
        let output = value.as_str()?;
        let parts: Vec<_> = output.split(if output.contains('\u{1b}') { '\u{1b}' } else { ',' }).collect();
        if parts.len() != 5 || !parts[1].eq_ignore_ascii_case(input) || !parts[2].is_empty()
            || parts[3].parse::<f32>().ok()? != 0.0
            || !parts[4].parse::<i32>().ok().is_some_and(|v| v == -1 || v > 0)
            || parts[0].is_empty() || parts[0].chars().any(|c| "!*?".contains(c)) { return None; }
        Some(parts[0])
    })
}

/// Resolve only admitted, destructible targets. The state vector owns their
/// removal, including reciprocal links, so prediction/restore need no I/O state.
pub(super) fn break_links(map: &bsp::Bsp, defs: &[Breakable]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (source, def) in defs.iter().enumerate() {
        let names: Vec<_> = immediate_outputs(&map.entities[def.entity], "Break").collect();
        for (target, other) in defs.iter().enumerate() {
            if other.health <= 0.0 || other.material == 7 || other.trigger_only { continue; }
            if map.entities[other.entity].str("targetname")
                .is_some_and(|name| names.iter().any(|s| s.eq_ignore_ascii_case(name))) {
                out.push((source, target));
            }
        }
    }
    out
}

pub(super) fn dependencies(map: &bsp::Bsp, defs: &[Breakable]) -> Vec<Dependency> {
    let mut out: Vec<Dependency> = Vec::new();
    for (index, def) in defs.iter().enumerate() {
        for name in immediate_outputs(&map.entities[def.entity], "Kill") {
            for (entity, ent) in map.entities.iter().enumerate() {
                if !ent.str("targetname").is_some_and(|s| s.eq_ignore_ascii_case(name)) { continue; }
                let class = ent.str("classname").unwrap_or("");
                if !(matches!(class, "func_brush" | "func_wall" | "func_detail_blocker")
                    || (class == "func_button" && ent.i64("spawnflags").unwrap_or(0) & 1 != 0))
                    || ent.i64("startdisabled") == Some(1)
                    || (class == "func_brush" && ent.i64("solidity") == Some(1))
                    || ent.str("parentname").is_some_and(|s| !s.is_empty()) { continue; }
                let angles = ent.str("angles").and_then(bsp::parse_origin).unwrap_or(Vec3::ZERO);
                let origin = ent.str("origin").and_then(bsp::parse_origin).unwrap_or(Vec3::ZERO);
                if angles != Vec3::ZERO || !origin.is_finite() || origin.abs().max_element() >= 131_072.0 { continue; }
                let Some(model) = ent.str("model").and_then(|s| s.strip_prefix('*')).and_then(|s| s.parse::<usize>().ok())
                    .filter(|m| *m > 0 && map.models.get(*m).is_some()) else { continue; };
                if let Some(dep) = out.iter_mut().find(|d| d.entity == entity) {
                    if !dep.killed_by.contains(&index) { dep.killed_by.push(index); }
                } else if out.len() < 128 {
                    out.push(Dependency { entity, model, origin, killed_by: vec![index], solids: Vec::new() });
                }
            }
        }
    }
    out
}


#[cfg(test)]
mod tests;

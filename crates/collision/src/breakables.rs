//! Mounted brush breakables. Definitions/geometry are immutable and scoped by
//! the owning map ContentId and BSP entity ordinal, just like map doors.
use crate::Convex;
use glam::Vec3;

pub const MAX_BREAKABLES: usize = 1024;
pub const MAX_HEALTH: f32 = 1_000_000.0;

#[derive(Debug)]
pub struct Breakable {
    pub entity: usize,
    pub model: usize,
    pub origin: Vec3,
    pub health: f32,
    pub material: i64,
    pub prop_data: String,
    pub bullet_scale: f32,
    pub club_scale: f32,
    pub minimum_damage: f32,
    pub trigger_only: bool,
    pub solids: Vec<Convex>,
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
            trigger_only: ent.i64("spawnflags").unwrap_or(0) & 1 != 0, solids: Vec::new() })
    }

    /// Read only mounted propdata.txt, including bounded base inheritance.
    /// Explicit prop data overrides health/damage, as authored by the mapper.
    pub fn apply_prop_data(&mut self, root: &kv::Block) {
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

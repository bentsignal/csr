//! Surface properties: which sounds a material makes when stepped on or
//! shot, from `scripts/surfaceproperties_manifest.txt` and the files it
//! lists. Brush and displacement materials name their property with
//! `$surfaceprop` in the VMT; static props with the MDL header's
//! surfaceprop. Properties inherit missing keys from their `base`.

use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceSounds {
    pub step_left: Option<String>,
    pub step_right: Option<String>,
    pub bullet_impact: Option<String>,
    /// Authored game-material code for impact decal translation.
    pub game_material: Option<String>,
}

#[derive(Resource, Default)]
pub struct SurfaceProps {
    props: HashMap<String, SurfaceSounds>,
    /// Collision surface name (texture or model path) → property name.
    by_surface: Mutex<HashMap<String, String>>,
}

impl SurfaceProps {
    pub fn load(fs: &vfs::Vfs) -> Self {
        let mut raw: HashMap<String, kv::Block> = HashMap::new();
        let files: Vec<String> = fs
            .read_string("scripts/surfaceproperties_manifest.txt")
            .and_then(|t| kv::parse(&t).ok())
            .map(|doc| {
                doc.iter()
                    .filter_map(|(_, v)| v.as_block())
                    .flat_map(|b| b.get_all("file").filter_map(|f| f.as_str().map(str::to_string)).collect::<Vec<_>>())
                    .collect()
            })
            .unwrap_or_default();
        for file in files {
            let Some(doc) = fs.read_string(&file).and_then(|t| kv::parse(&t).ok()) else { continue };
            for (name, v) in doc.iter() {
                if let Some(b) = v.as_block() {
                    // A later definition overrides an earlier one.
                    raw.insert(name.to_ascii_lowercase(), b.clone());
                }
            }
        }
        Self::from_blocks(&raw)
    }

    fn from_blocks(raw: &HashMap<String, kv::Block>) -> Self {
        fn lookup(raw: &HashMap<String, kv::Block>, name: &str, key: &str, depth: usize) -> Option<String> {
            let b = raw.get(name)?;
            if let Some(v) = b.str(key) {
                return Some(v.to_string());
            }
            let base = b.str("base")?.to_ascii_lowercase();
            (depth < 16 && base != name).then(|| lookup(raw, &base, key, depth + 1)).flatten()
        }
        let props = raw
            .keys()
            .map(|name| {
                (name.clone(), SurfaceSounds {
                    step_left: lookup(raw, name, "stepleft", 0),
                    step_right: lookup(raw, name, "stepright", 0),
                    bullet_impact: lookup(raw, name, "bulletimpact", 0),
                    game_material: lookup(raw, name, "gamematerial", 0),
                })
            })
            .collect();
        SurfaceProps { props, by_surface: Mutex::default() }
    }

    pub fn get(&self, prop: &str) -> Option<&SurfaceSounds> {
        self.props.get(&prop.to_ascii_lowercase()).or_else(|| self.props.get("default"))
    }

    /// Property name of a collision surface (material or model path).
    pub fn prop_of(&self, fs: &vfs::Vfs, surface: &str) -> String {
        let mut cache = self.by_surface.lock().unwrap();
        cache
            .entry(surface.to_string())
            .or_insert_with(|| {
                let found = if surface.ends_with(".mdl") {
                    fs.read(surface).and_then(|b| mdl::Model::load_header(&b).ok()).map(|m| m.surface_prop)
                } else {
                    vmt::Material::load(surface, &|p| fs.read_string(p)).and_then(|m| m.surface_prop().map(str::to_string))
                };
                found.filter(|s| !s.is_empty()).unwrap_or_else(|| "default".into()).to_ascii_lowercase()
            })
            .clone()
    }

    /// Sounds of a collision surface.
    pub fn sounds(&self, fs: &vfs::Vfs, surface: &str) -> Option<&SurfaceSounds> {
        let prop = self.prop_of(fs, surface);
        self.get(&prop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_inherit_from_base() {
        let doc = kv::parse(r#"
            "default" { "stepleft" "Default.StepLeft" "stepright" "Default.StepRight" "bulletimpact" "Default.BulletImpact" "gamematerial" "C" }
            "concrete" { "base" "default" "bulletimpact" "Concrete.BulletImpact" "stepleft" "Concrete.StepLeft" "gamematerial" "R" }
            "rock" { "base" "concrete" }
            "gravel" { "base" "rock" "stepleft" "Gravel.StepLeft" "stepright" "Gravel.StepRight" }
        "#).unwrap();
        let raw: HashMap<String, kv::Block> = doc.iter().map(|(k, v)| (k.to_string(), v.as_block().unwrap().clone())).collect();
        let props = SurfaceProps::from_blocks(&raw);
        let gravel = props.get("gravel").unwrap();
        assert_eq!(gravel.step_left.as_deref(), Some("Gravel.StepLeft"));
        assert_eq!(gravel.bullet_impact.as_deref(), Some("Concrete.BulletImpact"));
        assert_eq!(gravel.game_material.as_deref(), Some("R"));
        assert_eq!(props.get("rock").unwrap().step_right.as_deref(), Some("Default.StepRight"));
        assert_eq!(props.get("unknown").unwrap().step_left.as_deref(), Some("Default.StepLeft"));
    }
}

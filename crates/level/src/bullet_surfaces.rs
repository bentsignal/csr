//! Ballistic material definitions are mounted once, before simulation starts.
//! Input semantics come from Legacy app 4465480's surfaceproperties manifest,
//! base declarations and VMT/MDL surfaceprops. No native damage formula is used.
use collision::{bullet_exit::BulletSurface, CollisionWorld};
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;

type Definitions = BTreeMap<String, kv::Block>;

fn lookup<'a>(raw: &'a Definitions, name: &str, key: &str, seen: &mut Vec<String>) -> Option<&'a str> {
    if seen.len() >= 16 || seen.iter().any(|s| s == name) { return None; }
    seen.push(name.to_string());
    let block = raw.get(name)?;
    if let Some(value) = block.str(key) { return Some(value); }
    let base = block.str("base").map(str::to_ascii_lowercase)
        .or_else(|| (name != "default").then(|| "default".to_string()))?;
    lookup(raw, &base, key, seen)
}

fn resolve(raw: &Definitions, name: &str) -> Option<BulletSurface> {
    let name = name.to_ascii_lowercase();
    let get = |key| lookup(raw, &name, key, &mut Vec::new());
    let material = get("gamematerial")?.as_bytes();
    if material.len() != 1 { return None; }
    let penetration_modifier: f32 = get("penetrationmodifier")?.parse().ok()?;
    let damage_modifier: f32 = get("damagemodifier")?.parse().ok()?;
    if !penetration_modifier.is_finite() || penetration_modifier <= 0.
        || !damage_modifier.is_finite() || !(0. ..=1.).contains(&damage_modifier) { return None; }
    Some(BulletSurface { material: material[0].to_ascii_uppercase(), penetration_modifier, damage_modifier })
}

pub(super) fn load(world: &mut CollisionWorld, fs: &vfs::Vfs) {
    let mut raw = Definitions::new();
    if let Some(manifest) = fs.read_string("scripts/surfaceproperties_manifest.txt").and_then(|s| kv::parse(&s).ok()) {
        if let Some(files) = manifest.block("surfaceproperties_manifest") {
            for file in files.get_all("file").filter_map(kv::Value::as_str) {
                let Some(doc) = fs.read_string(file).and_then(|s| kv::parse(&s).ok()) else { continue; };
                for (name, value) in doc.iter() {
                    if let Some(block) = value.as_block() { raw.insert(name.to_ascii_lowercase(), block.clone()); }
                }
            }
        }
    }
    world.bullet_surfaces = world.surfaces.iter().map(|surface| {
        let prop = if surface.to_ascii_lowercase().ends_with(".mdl") {
            fs.read(surface).and_then(|bytes| mdl::Model::load_header(&bytes).ok()).map(|model| model.surface_prop)
        } else {
            vmt::Material::load(surface, &|path| fs.read_string(path))
                .and_then(|material| material.surface_prop().map(str::to_string))
        }?;
        // Unknown assets/properties are opaque; never infer wood from a path.
        resolve(&raw, &prop)
    }).collect();
}

pub(super) fn bind(world: &CollisionWorld, previous: Option<[u8; 20]>) -> Option<[u8; 20]> {
    previous.map(|previous| {
        let mut hash = Sha1::new();
        hash.update(previous);
        // Also changes identity for empty definitions: old peers stop at walls.
        hash.update(b"csr-practical-thin-slab-penetration-v1\0");
        hash.update((world.bullet_surfaces.len() as u64).to_le_bytes());
        for surface in &world.bullet_surfaces {
            if let Some(surface) = surface {
                hash.update([1, surface.material]);
                hash.update(surface.penetration_modifier.to_le_bytes());
                hash.update(surface.damage_modifier.to_le_bytes());
            } else { hash.update([0]); }
        }
        hash.finalize().into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_inheritance_default_missing_and_bad_values() {
        let doc = kv::parse(r#"
            "default" { "gamematerial" "C" "penetrationmodifier" "1" "damagemodifier" "0.5" }
            "wood" { "gamematerial" "W" "penetrationmodifier" "0.9" "damagemodifier" "0.6" }
            "box" { "base" "Wood" }
            "glass" { "gamematerial" "Y" "penetrationmodifier" "0.99" }
            "bad" { "base" "wood" "penetrationmodifier" "NaN" }
            "cycle" { "base" "cycle" }
        "#).unwrap();
        let raw = doc.iter().filter_map(|(name, value)| value.as_block().map(|b| (name.to_string(), b.clone()))).collect();
        assert_eq!(resolve(&raw, "BOX"), resolve(&raw, "wood"));
        assert_eq!(resolve(&raw, "glass").unwrap().damage_modifier, 0.5);
        for name in ["missing", "bad", "cycle"] { assert!(resolve(&raw, name).is_none()); }
    }

    #[test]
    fn resolved_values_and_policy_change_identity() {
        let mut world = CollisionWorld::new();
        let old = Some([7; 20]);
        let empty = bind(&world, old);
        assert_ne!(empty, old);
        world.bullet_surfaces.push(Some(BulletSurface { material: b'W', penetration_modifier: 0.9, damage_modifier: 0.6 }));
        let wood = bind(&world, old);
        assert_ne!(wood, empty);
        world.bullet_surfaces[0].as_mut().unwrap().damage_modifier = 0.4;
        assert_ne!(wood, bind(&world, old));
    }

    #[test]
    fn mounted_manifest_order_and_patch_surfaceprop_are_resolved_once() {
        use std::sync::Arc;
        let files = [
            ("scripts/surfaceproperties_manifest.txt", r#""surfaceproperties_manifest" { "file" "scripts/base.txt" "file" "scripts/override.txt" }"#),
            ("scripts/base.txt", r#""default" { "gamematerial" "C" "penetrationmodifier" "1" "damagemodifier" "0.5" } "wood" { "gamematerial" "W" "penetrationmodifier" "0.9" "damagemodifier" "0.6" }"#),
            ("scripts/override.txt", r#""wood" { "gamematerial" "W" "penetrationmodifier" "0.8" "damagemodifier" "0.4" }"#),
            ("materials/base.vmt", r#""LightmappedGeneric" { "$surfaceprop" "wood" }"#),
            ("materials/patch.vmt", r#""patch" { "include" "materials/base.vmt" }"#),
        ].into_iter().map(|(path, text)| (path.to_string(), Arc::<[u8]>::from(text.as_bytes()))).collect();
        let mut fs = vfs::Vfs::new();
        fs.push_front("pack:fixture", vfs::Mount::Memory(files));
        let mut world = CollisionWorld::new();
        let id = world.surface_id("patch");
        let unknown = world.surface_id("unmounted");
        load(&mut world, &fs);
        assert_eq!(world.bullet_surface(id), Some(BulletSurface { material: b'W', penetration_modifier: 0.8, damage_modifier: 0.4 }));
        assert!(world.bullet_surface(unknown).is_none());
        drop(fs);
        assert_eq!(world.bullet_surface(id).unwrap().damage_modifier, 0.4);
    }
}

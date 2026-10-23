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
    // With no resolved definitions both old and new worlds remain opaque.
    // Keep empty/synthetic catalog identities stable in that case.
    if world.bullet_surfaces.iter().all(Option::is_none) { return previous; }
    previous.map(|previous| {
        let mut hash = Sha1::new();
        hash.update(previous);
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
        assert_eq!(empty, old);
        world.bullet_surfaces.push(None);
        assert_eq!(bind(&world, old), old);
        world.bullet_surfaces.push(Some(BulletSurface { material: b'W', penetration_modifier: 0.9, damage_modifier: 0.6 }));
        let wood = bind(&world, old);
        assert_ne!(wood, empty);
        world.bullet_surfaces.last_mut().unwrap().as_mut().unwrap().damage_modifier = 0.4;
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

    #[test]
    #[ignore = "requires read-only installed Legacy Nuke geometry/materials; no native game"]
    fn installed_thin_wood_uses_mounted_definitions_and_actual_tick_fire() {
        use glam::Vec3;
        use collision::contents;
        use sim::{GameEvent, Team, TickInput, World, weapon::WeaponState};
        use std::sync::Arc;
        let level = crate::Level::load("de_nuke", None).expect("installed Legacy Nuke");
        let intact = World::with_catalog(level.collision.clone(), level.catalog.clone());
        let gun = Arc::new(level.catalog.get("weapon_ak47").unwrap().clone());
        // Find a real isolated wood slab with room for standing players on both
        // sides. Do not replace the mounted world, model, VMT or weapon values.
        let mut lane = None;
        'solids: for solid in &intact.collision.static_world().convexes {
            if !std::iter::once(&solid.surface).chain(&solid.plane_surfaces)
                .any(|&id| intact.collision.bullet_surface(id).is_some_and(|s| s.material == b'W')) { continue; }
            let center = (solid.mins + solid.maxs) * 0.5;
            for dir in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y] {
                let feet = intact.drop_to_floor(center - dir * 128. - Vec3::Z * 64., false);
                let from = feet + Vec3::Z * 64.;
                let to = from + dir * 256.;
                if [feet, to - Vec3::Z * 64.].into_iter().any(|pos|
                    intact.collision.trace_box(pos, pos, movement::STAND_MINS,
                        movement::STAND_MAXS, contents::MASK_PLAYERSOLID).start_solid) { continue; }
                let hit = intact.collision.trace_ray(from, to, contents::MASK_SHOT);
                if !intact.collision.bullet_surface(hit.surface).is_some_and(|s| s.material == b'W') { continue; }
                let Some(exit) = intact.collision.bullet_exit(from, to, &hit, 64.) else { continue; };
                let Some(leaving) = intact.collision.bullet_surface(exit.surface) else { continue; };
                let entry = intact.collision.bullet_surface(hit.surface).unwrap();
                if leaving.material != b'W' || exit.thickness >= 16. * gun.penetration
                    * entry.penetration_modifier.min(leaving.penetration_modifier) { continue; }
                if intact.collision.trace_ray(exit.next_start, to, contents::MASK_SHOT).hit() { continue; }
                lane = Some((from, to, hit.surface, exit.thickness));
                break 'solids;
            }
        }
        let (from, to, surface, thickness) = lane.expect("reachable actual Nuke wood slab");
        eprintln!("installed thin-cover: eye={from:?} target_eye={to:?} surface={} thickness={thickness}",
            intact.collision.surface_name(surface));
        for rate in [64, 128] {
            for opaque in [false, true] {
                let mut collision = (*level.collision).clone();
                if opaque { collision.bullet_surfaces.fill(None); }
                let mut world = World::with_catalog(Arc::new(collision), level.catalog.clone());
                world.set_tick_rate(rate);
                let shooter = world.add_player("wood control", Team::Terrorist);
                let victim = world.add_player("behind actual wood", Team::CounterTerrorist);
                for (id, eye) in [(shooter, from), (victim, to)] {
                    let player = world.player_mut(id).unwrap();
                    player.movement = movement::PlayerMove::spawn_at(eye - Vec3::Z * 64.);
                    player.armor = 0; player.helmet = false;
                }
                let player = world.player_mut(shooter).unwrap();
                player.weapons = vec![WeaponState::new(gun.clone())]; player.active = 0;
                let dir = (to - from).normalize();
                let angles = Vec3::new(0., dir.y.atan2(dir.x).to_degrees(), 0.);
                // Settle through the real solver before ordinary trigger input.
                for _ in 0..4 { world.step(&[(shooter, TickInput { view_angles: angles, ..Default::default() })]); }
                world.step(&[(shooter, TickInput { view_angles: angles,
                    buttons: movement::buttons::ATTACK, ..Default::default() })]);
                assert!(world.events.iter().any(|e| matches!(e, GameEvent::Fire { .. })));
                let health = world.player(victim).unwrap().health;
                assert_eq!(health < 100, !opaque, "rate={rate}, opaque={opaque}, health={health}");
                assert_eq!(world.player(shooter).unwrap().active_weapon().unwrap().clip, gun.clip_size - 1);
            }
        }
    }
}

//! Mounted definitions supplement BSP/items identity without new wire fields.
use collision::{CollisionWorld, Convex};
use sha1::{Digest, Sha1};

fn solids(hash: &mut Sha1, shapes: &[Convex]) {
    hash.update((shapes.len() as u64).to_le_bytes());
    for solid in shapes {
        hash.update(solid.contents.to_le_bytes());
        for value in solid.mins.to_array().into_iter().chain(solid.maxs.to_array()) { hash.update(value.to_le_bytes()); }
        hash.update((solid.planes.len() as u64).to_le_bytes());
        for plane in &solid.planes {
            for value in plane.normal.to_array().into_iter().chain([plane.dist]) { hash.update(value.to_le_bytes()); }
        }
    }
}
fn string(hash: &mut Sha1, value: &str) {
    hash.update((value.len() as u64).to_le_bytes()); hash.update(value.as_bytes());
}

pub(super) fn bind(collision: &CollisionWorld, previous: Option<[u8; 20]>) -> Option<[u8; 20]> {
    if collision.breakables.is_empty() { return previous; }
    previous.map(|previous| {
        let mut hash = Sha1::new();
        hash.update(b"csr-authored-breakable-definitions-v2\0");
        hash.update(previous);
        hash.update((collision.breakables.len() as u64).to_le_bytes());
        for def in collision.breakables.iter() {
            hash.update((def.entity as u64).to_le_bytes());
            for value in [def.health, def.bullet_scale, def.club_scale, def.minimum_damage] { hash.update(value.to_le_bytes()); }
            hash.update((def.model as u64).to_le_bytes());
            hash.update(def.material.to_le_bytes());
            hash.update([u8::from(def.trigger_only)]);
            for value in def.origin.to_array() { hash.update(value.to_le_bytes()); }
            string(&mut hash, &def.prop_data);
            if let Some(studio) = &def.studio {
                hash.update([1]);
                string(&mut hash, &studio.model);
                hash.update((studio.skin as u64).to_le_bytes());
                hash.update(studio.body.to_le_bytes());
                for value in studio.angles.to_array().into_iter().chain([studio.scale]).chain(studio.tint.to_array()) {
                    hash.update(value.to_le_bytes());
                }
            } else { hash.update([0]); }
            solids(&mut hash, &def.solids);
        }
        hash.update((collision.breakable_dependencies.len() as u64).to_le_bytes());
        for dep in collision.breakable_dependencies.iter() {
            hash.update((dep.entity as u64).to_le_bytes());
            hash.update((dep.model as u64).to_le_bytes());
            for value in dep.origin.to_array() { hash.update(value.to_le_bytes()); }
            hash.update((dep.killed_by.len() as u64).to_le_bytes());
            for &index in &dep.killed_by { hash.update((index as u64).to_le_bytes()); }
            solids(&mut hash, &dep.solids);
        }
        hash.finalize().into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use collision::{Plane, breakables::{Breakable, Studio, Dependency}};
    use glam::Vec3;
    use std::sync::Arc;

    fn fixture() -> CollisionWorld {
        let solid = Convex { planes: vec![Plane { normal: Vec3::X, dist: 1. }],
            mins: -Vec3::ONE, maxs: Vec3::ONE, contents: 1, surface: 0 };
        let mut world = CollisionWorld::new();
        world.breakables = vec![Breakable { entity: 4, model: 0, origin: Vec3::ZERO, health: 1., material: -1,
            prop_data: "Fixture.Base".into(), bullet_scale: 1., club_scale: 1., minimum_damage: 0., trigger_only: false,
            solids: vec![solid.clone()], studio: Some(Studio { model: "models/private_fixture.mdl".into(),
                angles: Vec3::ZERO, scale: 1., skin: 0, body: 0, tint: Vec3::ONE }) }].into();
        world.breakable_dependencies = vec![Dependency { entity: 5, model: 1, origin: Vec3::ZERO,
            killed_by: vec![0], solids: vec![solid] }].into();
        world
    }

    #[test]
    fn identity_binds_resolved_damage_pose_geometry_and_removal_edges_without_minting_a_missing_catalog() {
        let previous = Some([7; 20]);
        let baseline = bind(&fixture(), previous);
        assert_eq!(bind(&fixture(), previous), baseline);
        assert_eq!(bind(&fixture(), None), None);
        assert_eq!(bind(&CollisionWorld::new(), previous), previous);
        for change in 0..14 {
            let mut world = fixture();
            let def = &mut Arc::get_mut(&mut world.breakables).unwrap()[0];
            match change {
                0 => def.health = 2.,
                1 => def.bullet_scale = 0.5,
                2 => def.club_scale = 2.,
                3 => def.studio.as_mut().unwrap().model.push_str("_changed"),
                4 => def.studio.as_mut().unwrap().angles.x = 90.,
                5 => def.studio.as_mut().unwrap().skin = 1,
                6 => def.studio.as_mut().unwrap().body = 2,
                7 => def.studio.as_mut().unwrap().tint.x = 0.5,
                8 => def.solids[0].planes[0].dist = 2.,
                9 => def.studio.as_mut().unwrap().scale = 2.,
                10 => Arc::get_mut(&mut world.breakable_dependencies).unwrap()[0].killed_by.clear(),
                11 => Arc::get_mut(&mut world.breakable_dependencies).unwrap()[0].solids[0].planes[0].dist = 2.,
                12 => world.breakable_dependencies = Arc::from([]),
                _ => def.entity += 1,
            }
            assert_ne!(bind(&world, previous), baseline, "unbound definition change {change}");
        }
    }
}

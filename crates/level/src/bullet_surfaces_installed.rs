//! Read-only installed-level controls. No fetching, native process or rendering.
use super::*;
use collision::contents;
use glam::Vec3;
use std::sync::Arc;

fn local_level(name: &str) -> crate::Level {
    let (game_dir, fs) = crate::mount_game(None).expect("installed Legacy");
    crate::Level::load_local_id_from_vfs(
        &content::ContentId::csgo(content::ContentKind::Map, name), game_dir, fs,
        &workshop::Store::current(),
    ).unwrap_or_else(|error| panic!("already-local {name}: {error}"))
}

#[test]
#[ignore = "requires read-only Legacy and local classic Dust II archive; no native game"]
fn installed_dust2_mid_doors_pass_actual_rays_and_tick_fire() {
    for (map, material, model) in [
        ("de_dust2", 13, "dust_door_mid_doors_01.mdl"),
        ("de_dust2_2017", b'M', "dust_metal_door2.mdl"),
    ] {
        let mut level = local_level(map);
        let floor = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
        // Actual mid-door leaves, found from mounted BSP prop placement and PHY
        // ledges. Both lanes cross a leaf away from its edge and center gap.
        // These coordinates are controls, never runtime map-specific policy.
        let mut lanes = Vec::new();
        for x in [-470., -380.] {
            let eye_at = |y| floor.drop_to_floor(Vec3::new(x, y, -96.), false) + Vec3::Z * 64.;
            let front = eye_at(1450.);
            let back = eye_at(1800.);
            for (from, to) in [(front, back), (back, front)] {
                for point in [from, to] {
                    assert!(tests::installed_nonsolid_player_space(&level.bsp, point), "{map} {point:?}");
                    let feet = point - Vec3::Z * 64.;
                    assert!(!level.collision.trace_box(feet, feet, movement::STAND_MINS,
                        movement::STAND_MAXS, contents::MASK_PLAYERSOLID).start_solid, "{map} eye={point:?}");
                }
                let hit = level.collision.trace_ray(from, to, contents::MASK_SHOT);
                assert!(hit.hit() && !hit.start_solid && !hit.all_solid, "{map}: {hit:?}");
                assert!(level.collision.surface_name(hit.surface).ends_with(model), "{map}: {}", level.collision.surface_name(hit.surface));
                let entry = level.collision.bullet_surface(hit.surface).expect("mounted actual door declaration");
                assert_eq!(entry.material, material, "{map}");
                let exit = level.collision.bullet_exit(from, to, &hit, 64.).expect("full-world mid-door exit");
                assert!((8. ..12.).contains(&exit.thickness), "{map}: {exit:?}");
                assert!(exit.crossed_surfaces.iter().all(|&id|
                    level.collision.bullet_surface(id).is_some_and(|s| s.material == material)));
                assert!(!level.collision.trace_ray(exit.next_start, to, contents::MASK_SHOT).hit());
                eprintln!("mid-door: {map} from={from:?} to={to:?} material={material} thickness={}", exit.thickness);
                tests::installed_tick_fire_control(&level, from, to, true, false);
                lanes.push((from, to));
            }
        }
        drop(floor);
        // Exact same world and weapon with declarations removed must block.
        Arc::get_mut(&mut level.collision).unwrap().bullet_surfaces.fill(None);
        for (from, to) in lanes { tests::installed_tick_fire_control(&level, from, to, false, true); }
    }
}

#[test]
#[ignore = "requires read-only Legacy and available picker maps; no fetch or native game"]
fn installed_picker_maps_have_material_and_exact_exit_coverage() {
    let (game_dir, fs) = crate::mount_game(None).expect("installed Legacy");
    let entries: Vec<_> = crate::maps::list(&game_dir, &fs).into_iter()
        .filter(|entry| entry.status == crate::maps::MapStatus::Ready
            && entry.old.is_none_or(workshop::OldMap::menu_supported))
        .collect();
    assert!(!entries.is_empty());
    drop(fs);
    let mut totals = BTreeMap::<u8, usize>::new();
    for entry in entries {
        let level = local_level(&entry.name);
        let world = &level.collision;
        let resolved = world.bullet_surfaces.iter().flatten().count();
        assert!(resolved > 0, "{} has no mounted ballistic declarations", entry.name);
        let mut rays = BTreeMap::<u8, usize>::new();
        // All real collision backends remain present. Sample horizontal cover
        // from both axes; never replace a prop with an isolated synthetic slab.
        for solid in &world.convexes {
            let center = (solid.mins + solid.maxs) * 0.5;
            for dir in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y] {
                let from = center - dir * 96.;
                let to = center + dir * 96.;
                if [from, to].into_iter().any(|point| level.bsp.leafs[level.bsp.point_leaf(point)].contents & contents::SOLID != 0) { continue; }
                let hit = world.trace_ray(from, to, contents::MASK_SHOT);
                if !hit.hit() || hit.start_solid || hit.all_solid { continue; }
                let Some(surface) = world.bullet_surface(hit.surface) else { continue; };
                if rays.get(&surface.material).is_some_and(|&n| n >= 8) { continue; }
                let Some(exit) = world.bullet_exit(from, to, &hit, 64.) else { continue; };
                if exit.crossed_surfaces.iter().any(|&id| world.bullet_surface(id).is_none()) { continue; }
                *rays.entry(surface.material).or_default() += 1;
            }
        }
        assert!(!rays.is_empty(), "{} has no actual mounted exact-exit samples", entry.name);
        eprintln!("cover coverage: map={} resolved={}/{} exact_exit_rays={rays:?}", entry.name, resolved, world.surfaces.len());
        for (material, count) in rays { *totals.entry(material).or_default() += count; }
    }
    // Installed plaster/sheetrock/tile must be exercised by real map rays;
    // numeric declarations remain distinct from their ASCII representations.
    for material in [2, 5, 13, b'T', b'W', b'M'] {
        assert!(totals.get(&material).is_some_and(|&n| n > 0), "no actual level rays for material {material}; totals={totals:?}");
    }
}

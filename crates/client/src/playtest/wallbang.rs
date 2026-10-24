//! Offline scenario setup. Live shooting still uses ordinary authoritative input.
use glam::Vec3;
use collision::contents;

#[derive(Clone, Debug)]
pub(super) struct Lane {
    pub name: String,
    pub shooter: Vec3,
    pub target: Vec3,
}

fn standing(world: &sim::World, at: Vec3) -> Option<Vec3> {
    let feet = world.drop_to_floor(at, false);
    let hit = world.collision.trace_box(feet, feet, movement::STAND_MINS,
        movement::STAND_MAXS, contents::MASK_PLAYERSOLID);
    (!hit.start_solid && !hit.all_solid && (feet.z - at.z).abs() < 192.).then_some(feet)
}

fn cover_lane(world: &sim::World, name: String, a: Vec3, b: Vec3) -> Option<Lane> {
    let shooter = standing(world, a)?;
    let target = standing(world, b)?;
    let from = shooter + Vec3::Z * 64.;
    let to = target + Vec3::Z * 56.;
    let hit = world.collision.trace_ray(from, to, contents::MASK_SHOT);
    if !hit.hit() || hit.start_solid || hit.all_solid { return None; }
    let exit = world.collision.bullet_exit(from, to, &hit, 64.)?;
    if exit.crossed_surfaces.iter().any(|&s| world.collision.bullet_surface(s).is_none()) { return None; }
    if world.collision.trace_ray(exit.next_start, to, contents::MASK_SHOT).hit() { return None; }
    Some(Lane { name, shooter, target })
}

pub(super) fn lanes(level: &level::Level) -> Vec<Lane> {
    let world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
    let mut out = Vec::new();
    // These are the real mounted current/classic Dust II door controls already
    // exercised by the installed-level TickInput damage regression.
    if level.name == "de_dust2" || level.name == "de_dust2_2017" {
        for (leaf, x) in [("left", -470.), ("right", -380.)] {
            let a = Vec3::new(x, 1450., -96.);
            let b = Vec3::new(x, 1800., -96.);
            for (direction, a, b) in [("toward B", a, b), ("toward T", b, a)] {
                if let Some(lane) = cover_lane(&world, format!("Mid doors / {leaf} / {direction}"), a, b) { out.push(lane); }
            }
        }
    }
    // Discover additional real cover on the loaded map. Standable endpoints,
    // complete exit geometry and all material declarations are mandatory.
    // Labels come from the nearest authored NAV place, never a guessed callout.
    for solid in &level.collision.convexes {
        if out.len() >= 20 { break; }
        let center = (solid.mins + solid.maxs) * 0.5;
        for dir in [Vec3::X, Vec3::Y] {
            let a = center - dir * 128. + Vec3::Z * 64.;
            let b = center + dir * 128. + Vec3::Z * 64.;
            if out.iter().any(|lane| lane.shooter.distance_squared(a) < 256. * 256.) { continue; }
            let place = level.nav.as_ref().and_then(|nav| {
                let area = nav.areas.iter().filter(|a| a.place > 0).min_by(|a,b|
                    a.center().distance_squared(center).total_cmp(&b.center().distance_squared(center)))?;
                nav.places.get(area.place as usize - 1)
            }).map(String::as_str).unwrap_or("Cover");
            let name = format!("{place} / cover {} ({:.0}, {:.0})", out.len() + 1, center.x, center.y);
            if let Some(lane) = cover_lane(&world, name, a, b) { out.push(lane); }
        }
    }
    out
}

#[cfg(test)]
mod installed_tests {
    use super::*;
    #[test]
    #[ignore = "requires installed Legacy and already-local classic map archives"]
    fn installed_wallbang_picker_lanes_have_standing_endpoints_and_real_cover() {
        for name in ["de_dust2", "de_dust2_2017", "de_inferno_2016", "de_nuke_2016",
            "de_train_2015", "de_vertigo_2017", "de_mirage_2019", "de_cbble_2018",
            "de_cache_2015", "de_overpass_2015"] {
            let (game_dir, fs) = level::mount_game(None).unwrap();
            let level = level::Level::load_local_id_from_vfs(
                &content::ContentId::csgo(content::ContentKind::Map, name), game_dir, fs,
                &workshop::Store::current()).unwrap();
            let cases = lanes(&level);
            eprintln!("WALLBANG {name}: {} lanes: {:?}", cases.len(), cases.iter().map(|c| &c.name).collect::<Vec<_>>());
            assert!(!cases.is_empty(), "{name}: no usable player target lanes");
            if name.starts_with("de_dust2") {
                assert_eq!(cases.iter().filter(|c| c.name.starts_with("Mid doors")).count(), 4);
            }
            let world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
            for lane in cases {
                assert!(cover_lane(&world, lane.name.clone(), lane.shooter, lane.target).is_some(), "{name}: {}", lane.name);
            }
        }
    }
}

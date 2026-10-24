//! Offline scenario setup. Live shooting still uses ordinary authoritative input.
use glam::Vec3;
use collision::contents;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind { Wallbang, Contact, Boost, RunBoost }

#[derive(Clone, Debug)]
pub(super) struct Lane {
    pub name: String,
    pub kind: Kind,
    pub recommended_weapon: &'static str,
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
    let (recommended_weapon, exit) = ["weapon_ak47", "weapon_awp"].into_iter().find_map(|name| {
        let def = world.catalog.get(name)?;
        let (exit, retention) = sim::preview_bullet_exit(&world.collision,from,to,def.penetration)?;
        (def.damage * retention >= 1.).then_some((name,exit))
    })?;
    if exit.crossed_surfaces.iter().any(|&s| world.collision.bullet_surface(s).is_none()) { return None; }
    if world.collision.trace_ray(exit.next_start, to, contents::MASK_SHOT).hit() { return None; }
    Some(Lane { name, shooter, target, kind: Kind::Wallbang, recommended_weapon })
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
    let mut place_counts = std::collections::BTreeMap::<String, usize>::new();
    for solid in level.collision.convexes.iter().take(20_000) {
        if out.len() >= 20 { break; }
        let center = (solid.mins + solid.maxs) * 0.5;
        for dir in [Vec3::X, Vec3::Y] {
            let a = center - dir * 128. + Vec3::Z * 64.;
            let b = center + dir * 128. + Vec3::Z * 64.;
            if out.iter().any(|lane| lane.shooter.distance_squared(a) < 256. * 256.) { continue; }
            let Some(mut lane) = cover_lane(&world, String::new(), a, b) else {continue};
            let nav_area = |point: Vec3| level.nav.as_ref().and_then(|nav|
                nav.areas.iter().filter(|area| area.contains_xy(point, 8.)
                    && (area.z_at(point.x, point.y)-point.z).abs()<20.)
                    .min_by(|a,b| a.size().total_cmp(&b.size())));
            // Generic samples must be in the authored playable navigation,
            // not an isolated floor in scenery or an inaccessible map shell.
            if level.nav.is_some() && (nav_area(lane.shooter).is_none() || nav_area(lane.target).is_none()) {continue;}
            let place = nav_area(lane.target).and_then(|area| level.nav.as_ref()?.places.get(area.place.checked_sub(1)? as usize))
                .map(String::as_str).unwrap_or("Cover");
            if place_counts.get(place).copied().unwrap_or(0)>=2 {continue;}
            lane.name=format!("{place} / cover {} [{}]",out.len()+1,if lane.recommended_weapon=="weapon_awp" {"AWP"} else {"AK"});
            *place_counts.entry(place.into()).or_default()+=1;
            out.push(lane);
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

/// Pick an unobstructed, standable section of this map for partner tests.
pub(super) fn partner_lanes(level: &level::Level) -> Vec<Lane> {
    let world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
    let mut starts: Vec<_> = level.bsp.entities_of_class("info_player_counterterrorist")
        .filter_map(|e| e.str("origin").and_then(bsp::parse_origin)).collect();
    if let Some(nav)=&level.nav {
        let mut areas: Vec<_> = nav.areas.iter().collect();
        areas.sort_by(|a,b| b.size().total_cmp(&a.size()));
        starts.extend(areas.into_iter().take(256).map(|a| a.center()+Vec3::Z*8.));
    }
    for center in starts {
        for dir in [Vec3::X,Vec3::Y,-Vec3::X,-Vec3::Y] {
            let Some(a)=standing(&world,center) else {continue};
            let Some(b)=standing(&world,center+dir*96.) else {continue};
            let Some(end)=standing(&world,center+dir*384.) else {continue};
            if (a.z-b.z).abs()>2. || (a.z-end.z).abs()>8. {continue;}
            if world.collision.trace_box(a,end,movement::STAND_MINS,movement::STAND_MAXS,contents::MASK_PLAYERSOLID).hit() {continue;}
            // The ready-on-head spawn must fit a complete standing rider,
            // not just the partner's floor-level hull under a low ceiling.
            let high=b+Vec3::Z*96.;
            if world.collision.trace_box(high,high,movement::STAND_MINS,movement::STAND_MAXS,contents::MASK_PLAYERSOLID).start_solid {continue;}
            if world.collision.trace_box(b+Vec3::Z*72.1,end+Vec3::Z*72.1,
                movement::STAND_MINS,movement::STAND_MAXS,contents::MASK_PLAYERSOLID).hit() {continue;}
            return [(Kind::Contact,"Player contact / walk into partner"),
                (Kind::Boost,"Boost / jump onto crouched partner"),
                (Kind::RunBoost,"Run-boost / start on standing partner")].into_iter()
                .map(|(kind,name)| Lane {kind,name:name.into(),shooter:a,target:b,recommended_weapon:"weapon_ak47"}).collect();
        }
    }
    Vec::new()
}

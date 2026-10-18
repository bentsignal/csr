//! Opt-in installed Legacy regression. No assets are written or fetched.
//! Source-only handoff: compilation and execution are the parent's responsibility.
use crate::{Level, mount_game};
use collision::{HitOwner, contents};
use content::{ContentId, ContentKind};
use glam::Vec3;
use sim::{GameEvent, Team, TickInput, World, weapon::WeaponState};
use std::sync::Arc;

#[test]
#[ignore = "requires the selected local CS:GO Legacy Nuke/Cache data; parent executes under its Cargo slot"]
fn installed_nuke_cache_covers_decode_and_win_both_faces_before_tick_damage() {
    const SLATS: &str = "models/props/de_nuke/hr_nuke/nuke_vent_slats/nuke_vent_slats.mdl";
    const BOMB_VENT: &str = "models/props/de_nuke/hr_nuke/nuke_vent_bombsite/nuke_vent_bombsite_breakable_c.mdl";
    const CACHE_VENT: &str = "models/fmpone/de_cache/workingnewvents.mdl";
    for (name, cases) in [
        ("de_nuke", vec![(237, SLATS), (30, BOMB_VENT), (388, BOMB_VENT)]),
        ("de_cache", vec![(236, CACHE_VENT), (565, CACHE_VENT)]),
    ] {
        // The same discovered install, map ContentId selection, source-scoped
        // BSP and map-pak VFS path as ordinary play; never a fabricated BSP.
        let (game_dir, fs) = mount_game(None).expect("selected Legacy install");
        let id = ContentId::csgo(ContentKind::Map, name);
        let level = Level::load_local_id_from_vfs(&id, game_dir, fs, &workshop::Store::current())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        for (entity, model_path) in cases {
            let index = level.collision.breakables.iter().position(|def| def.entity == entity)
                .unwrap_or_else(|| panic!("{name} entity {entity}: actual cover was not admitted"));
            let def = &level.collision.breakables[index];
            let studio = def.studio.as_ref().expect("studio cover, not brush/header proxy");
            assert_eq!(studio.model, model_path);
            assert_eq!((def.health, def.bullet_scale, def.club_scale), (1., 1., 1.));
            assert_eq!((studio.body, studio.skin), (0, 0));
            let base = model_path.strip_suffix(".mdl").unwrap();
            let read = |path: &str| level.vfs.read(path).unwrap_or_else(|| panic!("{name}: missing mounted {path}"));
            let model = mdl::Model::load_with_body_value(&read(model_path),
                &read(&format!("{base}.vvd")), &read(&format!("{base}.dx90.vtx")), studio.body)
                .unwrap_or_else(|e| panic!("{name} entity {entity}: actual MDL/VVD/VTX decode: {e}"));
            assert!(!model.bones.is_empty());
            assert!(model.skins.get(studio.skin).is_some(), "authored skin must exist");
            assert!(!model.body_parts.is_empty());
            for (_, choices, selected) in &model.body_parts {
                assert!(*selected < choices.len());
                assert_eq!(*selected, 0, "these installed instances declare body zero");
            }
            assert!(model.meshes.iter().any(|mesh| !mesh.vertices.is_empty() && mesh.indices.len() >= 3));
            for mesh in &model.meshes {
                assert!(mesh.vertices.iter().all(|v| v.position.is_finite()));
                assert!(mesh.indices.iter().all(|&i| (i as usize) < mesh.vertices.len()));
                assert!(model.skins[studio.skin].get(mesh.material).is_some_and(|&t| t < model.textures.len()));
            }

            let phy = mdl::Phy::parse(&read(&format!("{base}.phy"))).expect("actual mounted PHY decode");
            assert!(!phy.hulls.is_empty());
            assert!(!def.solids.is_empty(), "production admission must retain PHY convexes");
            let (low, high) = phy.hulls.iter().fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(low, high), hull| { let (a, b) = hull.bounds(); (low.min(a), high.max(b)) });
            let size = high - low;
            let axis = (0..3).min_by(|&a, &b| size[a].total_cmp(&size[b])).unwrap();
            // A deterministic lane through the largest actual convex's interior,
            // across the whole PHY depth. No MDL header bounds or search for an
            // unoccluded ray: an overlapping button/wall must fail this fixture.
            let hull = phy.hulls.iter().max_by(|a, b| {
                let volume = |h: &mdl::ConvexHull| { let (a, b) = h.bounds(); (b - a).element_product() };
                volume(a).total_cmp(&volume(b))
            }).unwrap();
            let center = hull.points.iter().copied().sum::<Vec3>() / hull.points.len() as f32;
            let mut front = center; front[axis] = low[axis] - 24. / studio.scale;
            let mut back = center; back[axis] = high[axis] + 24. / studio.scale;
            let rotation = mdl::angles_to_mat3(studio.angles);
            let front = def.origin + rotation * (front * studio.scale);
            let back = def.origin + rotation * (back * studio.scale);
            assert!(front.is_finite() && back.is_finite());

            // Mounted weapon damage/type with zero spread for this ownership
            // regression only. Gravity is disabled for the single firing tick;
            // neither fixture setting asserts weapon or movement fidelity.
            let mut weapons = level.catalog.weapons.clone();
            weapons.iter_mut().find(|w| w.class_name == "weapon_ak47").expect("mounted AK-47").accuracy = Default::default();
            let catalog = Arc::new(weapons::Catalog { weapons });
            let intact = World::with_catalog(level.collision.clone(), catalog);
            let linked = intact.collision.breakable_dependencies.iter().filter(|dep| dep.killed_by.contains(&index))
                .collect::<Vec<_>>();
            if name == "de_nuke" && entity == 237 {
                assert_eq!(linked.len(), 1, "vent_a must own the literal immediate Kill dependency");
                assert_eq!(linked[0].entity, 238);
                assert!(!linked[0].solids.is_empty(), "linked button must be present before removal");
            }
            for (from, to) in [(front, back), (back, front)] {
                if name == "de_nuke" && entity == 237 {
                    assert!(collision::doors::ray_fraction(&linked[0].solids, from, to).is_some(),
                        "the regression lane must exercise the linked invisible button as well as the cover");
                }
                let (hit, owner) = intact.collision.trace_owned_ray(from, to, contents::MASK_SHOT);
                assert!(hit.hit() && !hit.start_solid && !hit.all_solid,
                    "{name} entity {entity}: invalid external PHY face lane {from:?} -> {to:?}: {hit:?}");
                assert_eq!(owner, Some(HitOwner::Breakable(index)),
                    "{name} entity {entity}: full-world first hit must be the cover from BOTH sides; \
                     unowned dependency occlusion is a real gap, never a reason to bypass strict first-hit: {hit:?}");
                let direction = (to - from).normalize();
                let angles = Vec3::new(-direction.z.atan2(direction.truncate().length()).to_degrees(),
                    direction.y.atan2(direction.x).to_degrees(), 0.);
                for knife in [false, true] {
                    let mut world = intact.clone();
                    world.config.gravity = 0.;
                    let player = world.add_player("installed-cover", Team::CounterTerrorist);
                    let weapon = world.catalog.get(if knife { "weapon_knife" } else { "weapon_ak47" }).unwrap().clone();
                    let p = world.player_mut(player).unwrap();
                    p.movement = movement::PlayerMove::spawn_at(from - Vec3::Z * p.movement.view_offset());
                    p.weapons = vec![WeaponState::new(Arc::new(weapon))]; p.active = 0;
                    let before = world.state();
                    let mut predicted = world.clone();
                    let inputs = [(player, TickInput { buttons: movement::buttons::ATTACK,
                        view_angles: angles, ..Default::default() })];
                    world.step(&inputs); predicted.step_prediction(&inputs);
                    assert_eq!(world.state(), predicted.state());
                    assert!(predicted.events.is_empty());
                    assert!(world.breakables[index].broken(),
                        "{name} entity {entity}, knife={knife}: ordinary TickInput did not remove the actual cover");
                    assert_eq!(world.breakables[index].health, 0.);
                    if knife {
                        assert!(world.events.iter().any(|e| matches!(e, GameEvent::Melee { hit: sim::MeleeHit::World, .. })));
                    } else {
                        assert!(world.events.iter().any(|e| matches!(e, GameEvent::Impact { door: None, .. })));
                    }
                    let after = world.collision.trace_owned_ray(from, to, contents::MASK_SHOT);
                    assert_ne!(after.1, Some(HitOwner::Breakable(index)));
                    // This is a local passage lane, not proof of full player-hull
                    // traversal. The Nuke button may not remain in that lane.
                    assert!(!after.0.hit(), "{name} entity {entity}: cover/dependency still blocks the lane: {after:?}");
                    assert_eq!(world.collision.breakable_dependencies.len(), intact.collision.breakable_dependencies.len(),
                        "overlay copying retains immutable removal edges");
                    assert_eq!(intact.collision.trace_owned_ray(from, to, contents::MASK_SHOT).1,
                        Some(HitOwner::Breakable(index)), "clone isolation");
                    let broken = world.decode::<sim::WorldState>(&world.encode_state()).unwrap();
                    let mut restored = intact.clone(); restored.restore(broken);
                    assert!(!restored.collision.trace_ray(from, to, contents::MASK_SHOT).hit());
                    world.restore(before);
                    assert!(!world.breakables[index].broken());
                    assert_eq!(world.collision.trace_owned_ray(from, to, contents::MASK_SHOT).1, Some(HitOwner::Breakable(index)));
                    // The public mode transition enters the normal competitive
                    // round-start reset path; no private damage/reset hook.
                    restored.set_mode(sim::GameMode::Competitive, restored.rules.clone());
                    assert_eq!(restored.breakables, intact.breakables);
                    assert!(restored.events.iter().any(|e| matches!(e, GameEvent::RoundStart { .. })));
                    assert_eq!(restored.collision.trace_owned_ray(from, to, contents::MASK_SHOT).1, Some(HitOwner::Breakable(index)));
                }
            }
        }
    }
}

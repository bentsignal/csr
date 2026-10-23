use super::*;

fn add_box(w: &mut CollisionWorld, low: Vec3, high: Vec3, contents: i32, name: &str) {
    let surface = w.surface_id(name);
    assert!(w.add_convex(Convex { plane_surfaces: Vec::new(),
        planes: vec![
            Plane {
                normal: Vec3::X,
                dist: high.x
            },
            Plane {
                normal: -Vec3::X,
                dist: -low.x
            },
            Plane {
                normal: Vec3::Y,
                dist: high.y
            },
            Plane {
                normal: -Vec3::Y,
                dist: -low.y
            },
            Plane {
                normal: Vec3::Z,
                dist: high.z
            },
            Plane {
                normal: -Vec3::Z,
                dist: -low.z
            },
        ],
        mins: low,
        maxs: high,
        contents,
        surface,
    }));
}

/// Compare every observable field, including tie metadata and float bits.
pub(super) fn exact(query: &mut PropLightingQuery<'_>, start: Vec3, end: Vec3, owner: usize) -> Trace {
    let expected = query.world.trace_prop_lighting_ray(start, end, owner);
    let actual = query.trace_prop_lighting_ray(start, end, owner);
    let context = || format!("{start:?} -> {end:?}, owner={owner}: {actual:?} != {expected:?}");
    assert_eq!(
        actual.fraction.to_bits(),
        expected.fraction.to_bits(),
        "{}",
        context()
    );
    assert_eq!(
        actual.end.to_array().map(f32::to_bits),
        expected.end.to_array().map(f32::to_bits),
        "{}",
        context()
    );
    assert_eq!(
        actual.normal.to_array().map(f32::to_bits),
        expected.normal.to_array().map(f32::to_bits),
        "{}",
        context()
    );
    assert_eq!(
        actual.plane_dist.to_bits(),
        expected.plane_dist.to_bits(),
        "{}",
        context()
    );
    assert_eq!(actual.start_solid, expected.start_solid, "{}", context());
    assert_eq!(actual.all_solid, expected.all_solid, "{}", context());
    assert_eq!(actual.contents, expected.contents, "{}", context());
    assert_eq!(actual.surface, expected.surface, "{}", context());
    actual
}

#[test]
fn covered_exposed_and_same_owner_only_exclusion_match_grid() {
    let mut w = CollisionWorld::new();
    add_box(
        &mut w,
        Vec3::new(-200., -8., -8.),
        Vec3::new(200., 8., 8.),
        contents::SOLID,
        "same_model",
    );
    add_box(
        &mut w,
        Vec3::new(150., -8., 24.),
        Vec3::new(180., 8., 32.),
        contents::SOLID,
        "same_model",
    );
    add_box(
        &mut w,
        Vec3::new(64., -8., 64.),
        Vec3::new(96., 8., 72.),
        contents::SOLID,
        "same_model",
    );
    w.static_prop_convexes = vec![0..2, 2..3, 3..3];
    add_box(
        &mut w,
        Vec3::new(-256., -256., 40.),
        Vec3::new(0., 256., 48.),
        contents::SOLID,
        "roof",
    );
    add_box(
        &mut w,
        Vec3::new(-256., -256., 128.),
        Vec3::new(256., 256., 136.),
        contents::PLAYERCLIP,
        "ignored_mask",
    );
    add_box(
        &mut w,
        Vec3::new(-256., -256., 1000.),
        Vec3::new(256., 256., 1008.),
        contents::SOLID,
        "tools/toolsskybox",
    );
    w.finalize();
    let mut query = w.prepare_prop_lighting();
    for (x, surface) in [
        (-80., "roof"),
        (80., "same_model"),
        (160., "tools/toolsskybox"),
        (300., ""),
    ] {
        let start = Vec3::new(x, 0., DIST_EPSILON);
        let tr = exact(&mut query, start, start + Vec3::Z * 16384., 0);
        assert!(!tr.start_solid && !tr.all_solid);
        assert_eq!(w.surface_name(tr.surface), surface);
    }
    // Empty, absent, or other-instance ownership must not exclude the first hull.
    for owner in [1, 2, usize::MAX] {
        assert!(exact(&mut query, Vec3::ZERO, Vec3::Z * 16384., owner).start_solid);
    }
}

#[test]
fn start_solid_exit_and_all_solid_metadata_match_grid() {
    let mut w = CollisionWorld::new();
    add_box(
        &mut w,
        Vec3::splat(-8.),
        Vec3::splat(8.),
        contents::SOLID,
        "start",
    );
    add_box(
        &mut w,
        Vec3::splat(-4.),
        Vec3::splat(4.),
        contents::WINDOW,
        "overlap",
    );
    add_box(
        &mut w,
        Vec3::new(-16., -16., 1000.),
        Vec3::new(16., 16., 1008.),
        contents::SOLID,
        "tools/toolsskybox",
    );
    w.finalize();
    let mut query = w.prepare_prop_lighting();
    let exit = exact(&mut query, Vec3::ZERO, Vec3::Z * 16384., usize::MAX);
    assert!(exit.start_solid && !exit.all_solid && query.is_sky(exit.surface));
    let exit_without_hit = exact(&mut query, Vec3::ZERO, Vec3::Z * 64., usize::MAX);
    assert!(exit_without_hit.start_solid && !exit_without_hit.all_solid);
    assert_eq!(
        exit_without_hit.contents,
        contents::SOLID | contents::WINDOW
    );
    for end in [Vec3::ZERO, Vec3::ONE, -Vec3::ONE] {
        let tr = exact(&mut query, Vec3::ZERO, end, usize::MAX);
        assert!(tr.start_solid && tr.all_solid);
        assert_eq!(tr.end, Vec3::ZERO);
        assert_eq!(w.surface_name(tr.surface), "start");
    }
}

#[test]
fn triangle_faces_edges_thickness_and_segment_boundaries_match_grid() {
    let mut w = CollisionWorld::new();
    for z in [-256., 0., 256., 512.] {
        w.add_triangle(
            [
                Vec3::new(-128., -128., z),
                Vec3::new(128., -128., z),
                Vec3::new(-128., 128., z),
            ],
            contents::SOLID,
            0,
        );
    }
    w.add_triangle(
        [
            Vec3::new(256., -128., 0.),
            Vec3::new(512., -128., 256.),
            Vec3::new(256., 128., 0.),
        ],
        contents::SOLID,
        0,
    );
    w.finalize();
    let mut query = w.prepare_prop_lighting();
    let face = exact(
        &mut query,
        Vec3::new(-64., -64., 600.),
        Vec3::new(-64., -64., -1000.),
        usize::MAX,
    );
    assert!(face.hit() && !face.start_solid);
    assert_eq!(face.normal, Vec3::Z);
    let thickness = exact(
        &mut query,
        Vec3::new(-64., -64., 511.5),
        Vec3::new(-64., -64., 511.5),
        usize::MAX,
    );
    assert!(thickness.start_solid && thickness.all_solid);
    for x in [
        -129., -128., -127.999, -0.03125, -0.0, 0., 0.03125, 128., 256., 300., 512.,
    ] {
        for y in [-128., -64., 0., 64., 128.] {
            for z in [-257., -256., -0.5, 0., DIST_EPSILON, 256., 512., 513.] {
                let start = Vec3::new(x, y, z);
                for delta in [
                    Vec3::Z * 16384.,
                    -Vec3::Z * 16384.,
                    Vec3::new(16000., 100., 100.),
                    Vec3::ZERO,
                ] {
                    exact(&mut query, start, start + delta, usize::MAX);
                }
            }
        }
    }
}

#[test]
fn nearest_sky_blocker_and_equal_fraction_order_match_grid() {
    for sky_first in [false, true] {
        let mut w = CollisionWorld::new();
        // Force a split/reordering of the BVH; insertion order still wins ties.
        for i in 0..24 {
            let p = Vec3::new(i as f32 * 100. - 1200., 512., 0.);
            add_box(&mut w, p, p + Vec3::splat(16.), contents::SOLID, "off_ray");
        }
        for name in if sky_first {
            ["tools/toolsskybox", "wall"]
        } else {
            ["wall", "tools/toolsskybox"]
        } {
            add_box(
                &mut w,
                Vec3::new(-32., -32., 512.),
                Vec3::new(32., 32., 520.),
                contents::SOLID,
                name,
            );
        }
        add_box(
            &mut w,
            Vec3::new(64., -32., 1024.),
            Vec3::new(128., 32., 1032.),
            contents::SOLID,
            "tools/toolsskybox",
        );
        add_box(
            &mut w,
            Vec3::new(64., -32., 8000.),
            Vec3::new(128., 32., 8016.),
            contents::SOLID,
            "far_wall",
        );
        // Convexes precede triangles in the old grid, even when they tie.
        let triangle_surface = w.surface_id("triangle_tie");
        w.add_triangle(
            [
                Vec3::new(-32., -32., 512.),
                Vec3::new(0., 32., 512.),
                Vec3::new(32., -32., 512.),
            ],
            contents::SOLID,
            triangle_surface,
        );
        w.finalize();
        let mut query = w.prepare_prop_lighting();
        let tie = exact(&mut query, Vec3::ZERO, Vec3::Z * 16384., usize::MAX);
        assert_eq!(query.is_sky(tie.surface), sky_first);
        let start = Vec3::new(96., 0., 0.);
        let sky = exact(&mut query, start, start + Vec3::Z * 16384., usize::MAX);
        assert!(
            query.is_sky(sky.surface),
            "sky before far solid must remain nearest hit"
        );
        let wall = exact(&mut query, start + Vec3::Z * 16000., start, usize::MAX);
        assert_eq!(w.surface_name(wall.surface), "far_wall");
    }
}

#[test]
fn deterministic_mixed_scene_and_reused_scratch_match_grid() {
    let mut w = CollisionWorld::new();
    for i in 0..160 {
        let p = Vec3::new(
            (i % 10) as f32 * 129. - 640.,
            (i / 10) as f32 * 127. - 1000.,
            (i % 7) as f32 * 256.,
        );
        add_box(
            &mut w,
            p,
            p + Vec3::new(100., 80., 24.),
            if i % 9 == 0 {
                contents::PLAYERCLIP
            } else {
                contents::SOLID
            },
            "synthetic",
        );
        w.add_triangle(
            [
                p + Vec3::Z * 80.,
                p + Vec3::new(100., 0., 120.),
                p + Vec3::new(0., 80., 80.),
            ],
            contents::SOLID,
            0,
        );
    }
    add_box(
        &mut w,
        Vec3::new(-2048., -2048., -32.),
        Vec3::new(2048., 2048., 0.),
        contents::SOLID,
        "large_floor",
    );
    w.static_prop_convexes = vec![0..2, 8..11, 0..0];
    w.finalize();
    let mut query = w.prepare_prop_lighting();
    assert!(query.nodes.len() > 1);
    assert_eq!(query.seen.len(), query.shapes.len());
    assert_eq!(query.nodes.len(), node_count(query.shapes.len()));
    let capacities = (
        query.candidates.capacity(),
        query.planes.capacity(),
        query.seen.capacity(),
        query.nodes.capacity(),
    );
    let mut state = 0x3210_9876u32;
    let mut random = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        (state >> 8) as f32 / 16777216.
    };
    for i in 0..1024 {
        let start = Vec3::new(
            random() * 3000. - 1500.,
            random() * 3000. - 1500.,
            random() * 2000. - 64.,
        );
        let direction = Vec3::new(random() * 2. - 1., random() * 2. - 1., random() * 2. - 1.)
            .normalize_or_zero();
        let length = [0., 32., 256., 257., 16384., 300000.][i % 6];
        exact(
            &mut query,
            start,
            start + direction * length,
            [0, 1, 2, usize::MAX][i % 4],
        );
    }
    assert_eq!(
        capacities,
        (
            query.candidates.capacity(),
            query.planes.capacity(),
            query.seen.capacity(),
            query.nodes.capacity()
        )
    );
    // Exercise stamp rollover without billions of queries, then repeat a hit.
    query.generation = u32::MAX;
    query.seen.fill(1);
    let tr = exact(&mut query, Vec3::Z * 8., -Vec3::Z * 128., usize::MAX);
    assert!(tr.hit());
    assert_eq!(query.generation, 1);
    exact(&mut query, Vec3::Z * 8., -Vec3::Z * 128., usize::MAX);
}

#[test]
fn empty_masked_and_dynamic_worlds_keep_existing_behavior() {
    let mut w = CollisionWorld::new();
    let mut empty = w.prepare_prop_lighting();
    assert!(!exact(&mut empty, Vec3::ZERO, Vec3::Z * 16384., 0).hit());
    drop(empty);
    add_box(
        &mut w,
        Vec3::splat(-1.),
        Vec3::ONE,
        contents::PLAYERCLIP,
        "not_solid_mask",
    );
    w.finalize();
    let mut masked = w.prepare_prop_lighting();
    assert!(masked.nodes.is_empty());
    assert!(!exact(&mut masked, Vec3::ZERO, Vec3::Z * 16384., 0).hit());
    drop(masked);

    struct Backend;
    impl DynamicWorld for Backend {
        fn trace_box(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32) -> Trace {
            let mut tr = Trace::empty(end);
            clip_bounds(
                start,
                end,
                mins,
                maxs,
                Vec3::splat(-4.),
                Vec3::splat(4.),
                mask,
                19,
                &mut tr,
            );
            tr
        }
        fn point_contents(&self, _: Vec3, _: i32) -> i32 {
            0
        }
    }
    w.dynamic = Some(std::sync::Arc::new(Backend));
    let mut dynamic = w.prepare_prop_lighting();
    assert!(dynamic.nodes.is_empty());
    assert!(exact(&mut dynamic, Vec3::ZERO, Vec3::Z * 128., 0).start_solid);
    exact(&mut dynamic, Vec3::Z * 128., -Vec3::Z * 128., usize::MAX);
}

// Frozen pre-reuse trace route from 553c93be; intentionally owns fresh query storage.
// Keep its traversal independent of the scratch wrapper under test.
use super::*;

fn reference(
    world: &CollisionWorld, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32,
    excluded_convexes: Option<&std::ops::Range<usize>>,
) -> (Trace, Option<HitOwner>) {
    if let Some(base) = &world.base {
        let mut tr = reference(base, start, end, mins, maxs, mask, excluded_convexes).0;
        let mut owner = None;
        let offset = (mins + maxs) * 0.5;
        let ext = (maxs - mins) * 0.5;
        let s = start + offset;
        let e = end + offset;
        for (index, solid) in world.convexes.iter().enumerate() {
            if solid.contents & mask != 0 && swept_bounds_overlap(s, e, ext, solid.mins, solid.maxs) {
                if clip_convex(&solid.planes, s, e, ext, solid.contents, solid.surface, &mut tr) {
                    owner = world.runtime_owners.get(index).copied().flatten();
                }
            }
        }
        tr.end = if tr.all_solid { start } else { start.lerp(end, tr.fraction) };
        return (tr, owner);
    }
    if let Some(world)=&world.dynamic { return (world.trace_box(start,end,mins,maxs,mask), None); }
    let offset = (mins + maxs) * 0.5;
    let ext = (maxs - mins) * 0.5;
    let s = start + offset;
    let e = end + offset;
    let mut tr = Trace::empty(end);

    // Long sweeps are split into short segments walked in order, so a
    // bullet or shadow ray only visits the grid cells it passes and can
    // stop at the first segment that contains a hit.
    let length = (e - s).length();
    let segments = ((length / (GRID_CELL * 2.0)).ceil() as usize).clamp(1, 1024);
    let mut cands = Vec::with_capacity(64);
    let mut scratch = Vec::with_capacity(32);
    let mut seen: std::collections::HashSet<(u8, u32)> = std::collections::HashSet::new();
    for seg in 0..segments {
        let f0 = seg as f32 / segments as f32;
        let f1 = (seg + 1) as f32 / segments as f32;
        let a = s.lerp(e, f0);
        let b = s.lerp(e, f1);
        let sweep_min = a.min(b) - ext - Vec3::ONE;
        let sweep_max = a.max(b) + ext + Vec3::ONE;
        world.candidates(sweep_min, sweep_max, &mut cands);
        for &shape in &cands {
            if segments > 1 {
                let key = match shape {
                    Shape::Convex(i) => (0u8, i),
                    Shape::Triangle(i) => (1u8, i),
                };
                if !seen.insert(key) {
                    continue;
                }
            }
            match shape {
                Shape::Convex(i) => {
                    if excluded_convexes.is_some_and(|range| range.contains(&(i as usize))) {
                        continue;
                    }
                    let c = &world.convexes[i as usize];
                    if c.contents & mask == 0 || !overlaps(c.mins, c.maxs, sweep_min, sweep_max) {
                        if segments > 1 {
                            seen.remove(&(0, i));
                        }
                        continue;
                    }
                    clip_convex(&c.planes, s, e, ext, c.contents, c.surface, &mut tr);
                }
                Shape::Triangle(i) => {
                    let t = &world.triangles[i as usize];
                    if t.contents & mask == 0 || !overlaps(t.mins, t.maxs, sweep_min, sweep_max) {
                        if segments > 1 {
                            seen.remove(&(1, i));
                        }
                        continue;
                    }
                    triangle_planes(t, &mut scratch);
                    clip_convex(&scratch, s, e, ext, t.contents, t.surface, &mut tr);
                }
            }
            if tr.all_solid {
                break;
            }
        }
        // Anything later in the sweep cannot beat a hit inside this segment.
        if tr.all_solid || tr.fraction <= f1 {
            break;
        }
    }
    if tr.fraction < 1.0 {
        tr.end = start + (end - start) * tr.fraction;
    } else {
        tr.end = end;
    }
    if tr.all_solid {
        tr.end = start;
    }
    (tr, None)
}


fn solid(low: Vec3, high: Vec3, contents: i32, surface: u32) -> Convex {
    Convex {
        planes: vec![
            Plane { normal: Vec3::X, dist: high.x },
            Plane { normal: -Vec3::X, dist: -low.x },
            Plane { normal: Vec3::Y, dist: high.y },
            Plane { normal: -Vec3::Y, dist: -low.y },
            Plane { normal: Vec3::Z, dist: high.z },
            Plane { normal: -Vec3::Z, dist: -low.z },
        ],
        mins: low, maxs: high, contents, surface,
    }
}

fn wall(x: f32, contents: i32, surface: u32) -> Convex {
    solid(Vec3::new(x, -80.0, -80.0), Vec3::new(x + 20.0, 80.0, 80.0), contents, surface)
}

fn mixed_world() -> CollisionWorld {
    let mut w = CollisionWorld::new();
    w.add_convex(solid(Vec3::new(-2000.0, -100.0, -20.0), Vec3::new(2000.0, 100.0, -10.0), contents::SOLID, 10));
    w.add_convex(wall(100.0, contents::SOLID, 11));
    w.add_convex(wall(100.0, contents::GRATE, 12));
    w.add_convex(wall(40.0, contents::PLAYERCLIP, 13));
    w.add_triangle([Vec3::new(300.0, -80.0, 0.0), Vec3::new(450.0, -80.0, 50.0), Vec3::new(300.0, 80.0, 0.0)], contents::SOLID, 14);
    w.static_prop_convexes = vec![1..3, 3..4];
    w.finalize();
    w
}

fn assert_trace(actual: Trace, expected: Trace) {
    assert_eq!(actual.fraction.to_bits(), expected.fraction.to_bits(), "{actual:?} != {expected:?}");
    assert_eq!(actual.end.to_array().map(f32::to_bits), expected.end.to_array().map(f32::to_bits));
    assert_eq!(actual.normal.to_array().map(f32::to_bits), expected.normal.to_array().map(f32::to_bits));
    assert_eq!(actual.plane_dist.to_bits(), expected.plane_dist.to_bits());
    assert_eq!(actual.start_solid, expected.start_solid);
    assert_eq!(actual.all_solid, expected.all_solid);
    assert_eq!(actual.contents, expected.contents);
    assert_eq!(actual.surface, expected.surface);
}

fn exact(w: &CollisionWorld, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32,
    excluded: Option<&std::ops::Range<usize>>) -> (Trace, Option<HitOwner>) {
    let expected = reference(w, start, end, mins, maxs, mask, excluded);
    let actual = w.trace_box_owned(start, end, mins, maxs, mask, excluded);
    assert_trace(actual.0, expected.0);
    assert_eq!(actual.1, expected.1);
    actual
}

fn assert_idle() {
    TRACE_SCRATCH.with(|slot| {
        let scratch = slot.borrow();
        assert!(scratch.cands.is_empty());
        assert!(scratch.planes.is_empty());
        assert!(scratch.seen.is_empty());
        assert!(scratch.cands.capacity() <= TRACE_CANDIDATE_CAPACITY_LIMIT);
        assert!(scratch.planes.capacity() <= TRACE_PLANE_CAPACITY_LIMIT);
        assert!(scratch.seen.capacity() <= TRACE_SEEN_CAPACITY_LIMIT);
    });
}

#[test]
fn scratch_matches_fresh_reference_for_masks_exclusions_grazing_and_segments() {
    let w = mixed_world();
    let rays = [
        (Vec3::ZERO, Vec3::X * 200.0),
        (Vec3::new(110.0, 0.0, 0.0), Vec3::X * 200.0),
        (Vec3::new(110.0, 0.0, 0.0), Vec3::new(112.0, 0.0, 0.0)),
        (Vec3::new(-2000.0, 80.0, 0.0), Vec3::new(2000.0, 80.0, 0.0)),
        (Vec3::new(2000.0, 80.03125, 0.0), Vec3::new(-2000.0, 80.03125, 0.0)),
        (Vec3::new(325.0, 0.0, 200.0), Vec3::new(325.0, 0.0, -200.0)),
        (Vec3::new(325.0, 0.0, 10.0), Vec3::new(325.0, 0.0, 10.0)),
        (Vec3::new(-5000.0, 500.0, 0.0), Vec3::new(-5000.0, 500.0, 0.0)),
    ];
    // Repeat across a warm TLS slot, including empty and fully excluded queries.
    for _ in 0..3 {
        for (start, end) in rays {
            for (mins, maxs) in [(Vec3::ZERO, Vec3::ZERO), (Vec3::new(-16.0, -16.0, 0.0), Vec3::new(16.0, 16.0, 72.0))] {
                for mask in [0, contents::SOLID, contents::GRATE, contents::MASK_PLAYERSOLID, -1] {
                    for excluded in [None, Some(0..1), Some(1..3), Some(0..w.convexes.len())] {
                        exact(&w, start, end, mins, maxs, mask, excluded.as_ref());
                        assert_idle();
                    }
                }
            }
        }
    }
    // Varied oblique rays and asymmetric hulls without external assets or RNG.
    let mut seed = 19u32;
    let mut next = || { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); (seed % 4096) as f32 - 2048.0 };
    for _ in 0..128 {
        let start = Vec3::new(next(), next() / 16.0, next() / 8.0);
        let end = Vec3::new(next(), next() / 16.0, next() / 8.0);
        exact(&w, start, end, Vec3::new(-3.0, -7.0, -2.0), Vec3::new(5.0, 11.0, 17.0), -1, None);
    }
    let tie = w.trace_ray(Vec3::ZERO, Vec3::X * 200.0, contents::SOLID | contents::GRATE);
    assert_eq!(tie.surface, 11);
    let exit = w.trace_ray(Vec3::X * 110.0, Vec3::X * 200.0, contents::SOLID | contents::GRATE);
    assert!(exit.start_solid && !exit.all_solid);
    assert_eq!(exit.contents, contents::SOLID | contents::GRATE);
    assert!(!w.trace_prop_lighting_ray(Vec3::ZERO, Vec3::X * 200.0, 0).hit());
}

#[test]
fn scratch_preserves_typed_winners_static_ties_and_start_solid_ownership() {
    let base = std::sync::Arc::new(mixed_world());
    let overlay = |solids, owners| CollisionWorld {
        base: Some(base.clone()), convexes: solids, runtime_owners: owners, ..Default::default()
    };
    let a = wall(20.0, contents::WINDOW, 21);
    let b = wall(20.0, contents::WINDOW, 22);
    for (solids, owners, winner) in [
        (vec![a.clone(), b.clone()], vec![Some(HitOwner::Door(7)), Some(HitOwner::Breakable(9))], Some(HitOwner::Door(7))),
        (vec![b, a], vec![Some(HitOwner::Breakable(9)), Some(HitOwner::Door(7))], Some(HitOwner::Breakable(9))),
        (vec![wall(100.0, contents::SOLID, 23)], vec![Some(HitOwner::Door(7))], None),
    ] {
        let w = overlay(solids, owners);
        let result = exact(&w, Vec3::ZERO, Vec3::X * 200.0, Vec3::ZERO, Vec3::ZERO, contents::SOLID | contents::WINDOW, None);
        assert_eq!(result.1, winner);
        assert_trace(w.trace_owned_ray(Vec3::ZERO, Vec3::X * 200.0, contents::SOLID | contents::WINDOW).0, result.0);
        assert_eq!(w.trace_breakable_ray(Vec3::ZERO, Vec3::X * 200.0, contents::SOLID | contents::WINDOW).1, winner.and_then(HitOwner::breakable));
        assert_eq!(exact(&w, Vec3::ZERO, Vec3::X * 200.0, Vec3::ZERO, Vec3::ZERO, contents::SOLID, None).1, None);
    }
    let w = overlay(vec![wall(105.0, contents::WINDOW, 24)], vec![Some(HitOwner::Breakable(9))]);
    let all = exact(&w, Vec3::X * 110.0, Vec3::X * 112.0, Vec3::ZERO, Vec3::ZERO, -1, None);
    assert!(all.0.all_solid);
    assert_eq!(all.1, Some(HitOwner::Breakable(9)));
    let exit = exact(&w, Vec3::X * 110.0, Vec3::X * 200.0, Vec3::ZERO, Vec3::ZERO, -1, None);
    assert!(exit.0.start_solid && !exit.0.all_solid);
    assert_eq!(exit.1, None);
    assert_idle();
}

struct DelegatingWorld {
    inner: CollisionWorld,
    scratch_should_be_free: bool,
}

impl DynamicWorld for DelegatingWorld {
    fn trace_box(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32) -> Trace {
        TRACE_SCRATCH.with(|slot| assert_eq!(slot.try_borrow_mut().is_ok(), self.scratch_should_be_free));
        self.inner.trace_box(start, end, mins, maxs, mask)
    }
    fn point_contents(&self, p: Vec3, mask: i32) -> i32 { self.inner.point_contents(p, mask) }
}

#[test]
fn scratch_reentrant_fallback_keeps_outer_storage_and_dynamic_base_calls_safe() {
    let expected = reference(&mixed_world(), Vec3::ZERO, Vec3::X * 2000.0, Vec3::ZERO, Vec3::ZERO, contents::SOLID, None).0;
    for scratch_should_be_free in [true, false] {
        let dynamic = std::sync::Arc::new(CollisionWorld {
            dynamic: Some(std::sync::Arc::new(DelegatingWorld { inner: mixed_world(), scratch_should_be_free })),
            ..Default::default()
        });
        let overlay = CollisionWorld { base: Some(dynamic), ..Default::default() };
        let query = || assert_trace(overlay.trace_ray(Vec3::ZERO, Vec3::X * 2000.0, contents::SOLID), expected);
        if scratch_should_be_free {
            query();
        } else {
            with_trace_scratch(|outer| {
                outer.cands.push(Shape::Convex(u32::MAX));
                outer.planes.push(Plane { normal: Vec3::Y, dist: 123.0 });
                outer.seen.insert((9, u32::MAX));
                let ptr = outer.cands.as_ptr();
                query();
                assert_eq!(outer.cands.as_ptr(), ptr);
                assert!(matches!(outer.cands.as_slice(), [Shape::Convex(u32::MAX)]));
                assert_eq!(outer.planes, vec![Plane { normal: Vec3::Y, dist: 123.0 }]);
                assert_eq!(outer.seen.len(), 1);
                assert!(outer.seen.contains(&(9, u32::MAX)));
            });
        }
        assert_idle();
    }
}

#[test]
fn scratch_threads_and_successive_worlds_are_independent() {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let threads: Vec<_> = (0..4).map(|index| {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            let mut w = CollisionWorld::new();
            w.add_convex(wall(300.0 + index as f32 * 500.0, contents::SOLID, index + 30));
            w.finalize();
            let query = || exact(&w, Vec3::ZERO, Vec3::X * 3000.0, Vec3::ZERO, Vec3::ZERO, contents::SOLID, None);
            for _ in 0..8 {
                assert_eq!(query().0.surface, index + 30);
                // Each thread holds its own scratch while all four query again.
                with_trace_scratch(|outer| {
                    outer.seen.insert((8, index));
                    barrier.wait();
                    assert_eq!(query().0.surface, index + 30);
                    assert_eq!(outer.seen.len(), 1);
                    assert!(outer.seen.contains(&(8, index)));
                });
                assert_idle();
            }
            let empty = CollisionWorld::new();
            assert!(!exact(&empty, Vec3::ZERO, Vec3::X * 3000.0, Vec3::ZERO, Vec3::ZERO, -1, None).0.hit());
            assert_idle();
        })
    }).collect();
    for thread in threads { thread.join().unwrap(); }
}

#[test]
fn scratch_discards_large_query_storage_and_clears_after_early_hit() {
    // More unique near-miss solids than the retained seen limit; a long ray
    // reaches all of them. Bounds overlap the guarded sweep but planes miss.
    let mut long = CollisionWorld::new();
    for i in 0..(TRACE_SEEN_CAPACITY_LIMIT + 64) {
        let x = i as f32 * 20.0;
        long.add_convex(solid(Vec3::new(x, 0.0, 0.0), Vec3::new(x + 1.0, 1.0, 1.0), contents::SOLID, 1));
    }
    long.finalize();
    let start = Vec3::new(-10.0, 1.5, 0.5);
    let end = Vec3::new(90_000.0, 1.5, 0.5);
    assert!(!exact(&long, start, end, Vec3::ZERO, Vec3::ZERO, -1, None).0.hit());
    TRACE_SCRATCH.with(|slot| assert_eq!(slot.borrow().seen.capacity(), 0));
    assert_idle();

    // Dense single cell forces a large candidate allocation, then all-solid
    // breaks out on the first shape. Cleanup must run on that early exit too.
    let mut dense = CollisionWorld::new();
    for _ in 0..(TRACE_CANDIDATE_CAPACITY_LIMIT + 64) {
        dense.add_convex(wall(0.0, contents::SOLID, 5));
    }
    dense.finalize();
    assert!(exact(&dense, Vec3::X, Vec3::X * 2.0, Vec3::ZERO, Vec3::ZERO, -1, None).0.all_solid);
    TRACE_SCRATCH.with(|slot| assert_eq!(slot.borrow().cands.capacity(), 0));
    assert_idle();
    let w = mixed_world();
    exact(&w, Vec3::new(325.0, 0.0, 200.0), Vec3::new(325.0, 0.0, -200.0), Vec3::ZERO, Vec3::ZERO, -1, None);
    assert_idle();
    exact(&w, Vec3::ZERO, Vec3::X * 2000.0, Vec3::ZERO, Vec3::ZERO, -1, None);
    assert_idle();
}

#[test]
fn scratch_warm_queries_retain_storage_and_unwind_clears_it() {
    let w = mixed_world();
    let query = || exact(&w, Vec3::new(325.0, 0.0, 200.0), Vec3::new(325.0, 0.0, -200.0), Vec3::ZERO, Vec3::ZERO, -1, None);
    query();
    let storage = || TRACE_SCRATCH.with(|slot| {
        let s = slot.borrow();
        (s.cands.as_ptr(), s.cands.capacity(), s.planes.as_ptr(), s.planes.capacity(), s.seen.capacity())
    });
    let warm = storage();
    assert!(warm.1 > 0 && warm.3 > 0 && warm.4 > 0);
    for _ in 0..8 {
        query();
        assert_eq!(storage(), warm);
        assert_idle();
    }
    let panic = std::panic::catch_unwind(|| with_trace_scratch(|s| {
        s.cands.push(Shape::Convex(0));
        s.planes.push(Plane { normal: Vec3::Z, dist: 1.0 });
        s.seen.insert((0, 0));
        panic!("synthetic query unwind");
    }));
    assert!(panic.is_err());
    assert_idle();
    query();
    assert_eq!(storage(), warm);
}

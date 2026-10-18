use super::*;

fn box_hull(low: Vec3, high: Vec3) -> mdl::ConvexHull {
    mdl::ConvexHull {
        points: vec![
            Vec3::new(low.x, low.y, low.z),
            Vec3::new(high.x, low.y, low.z),
            Vec3::new(high.x, high.y, low.z),
            Vec3::new(low.x, high.y, low.z),
            Vec3::new(low.x, low.y, high.z),
            Vec3::new(high.x, low.y, high.z),
            Vec3::new(high.x, high.y, high.z),
            Vec3::new(low.x, high.y, high.z),
        ],
        triangles: vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ],
    }
}

#[test]
fn collapsed_prop_piece_does_not_seal_a_doorway_or_remove_its_frame() {
    // Own geometry: a 64-wide opening with two jambs and a lintel. Model a
    // decoded bad ledge with eight point indices collapsed at the opening's
    // center, as observed in installed Legacy door-frame collision input.
    let pieces = vec![
        box_hull(Vec3::new(-40., -4., 0.), Vec3::new(-32., 4., 96.)),
        box_hull(Vec3::new(32., -4., 0.), Vec3::new(40., 4., 96.)),
        box_hull(Vec3::new(-32., -4., 88.), Vec3::new(32., 4., 96.)),
        box_hull(Vec3::new(-64., -128., -1.), Vec3::new(64., 128., 0.)),
        box_hull(Vec3::new(0., 0., 58.), Vec3::new(0., 0., 58.)),
    ];
    // Exercise the production placed-prop loader, including translated and
    // rotated doorways, rather than skipping its hull conversion in a mock.
    for yaw in [0., 90., 180., 270.] {
        let origin = Vec3::new(720., -432., 128.);
        let angles = Vec3::new(0., yaw, 0.);
        let rotation = mdl::angles_to_mat3(angles);
        let map = bsp::Bsp {
            static_props: bsp::StaticProps {
                models: vec!["synthetic-door.mdl".into()],
                props: vec![bsp::StaticProp {
                    origin,
                    angles,
                    model: 0,
                    solid: 6,
                    flags: 0,
                    skin: 0,
                    fade_min: 0.,
                    fade_max: 0.,
                    scale: 1.,
                    lighting_origin: None,
                    diffuse_modulation: None,
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let w = CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(), &|_| {
            Some(pieces.clone())
        });
        assert_eq!(w.convexes.len(), 4, "yaw {yaw}: invalid piece became solid");
        let placed = |p| origin + rotation * p;
        let sweep = |start, end| {
            w.trace_box(
                placed(start),
                placed(end),
                Vec3::new(-16., -16., 0.),
                Vec3::new(16., 16., 72.),
                contents::MASK_PLAYERSOLID,
            )
        };
        // Standing player crosses both ways and can occupy the threshold.
        for (start, end) in [(-80., 80.), (80., -80.), (0., 0.)] {
            let tr = sweep(Vec3::new(0., start, 0.0625), Vec3::new(0., end, 0.0625));
            assert!(!tr.hit(), "yaw {yaw}: open path blocked: {tr:?}");
        }
        // Both jambs, overhead lintel, and the thin floor still stop a hull.
        for (start, end) in [
            (Vec3::new(-36., -80., 0.0625), Vec3::new(-36., 80., 0.0625)),
            (Vec3::new(36., -80., 0.0625), Vec3::new(36., 80., 0.0625)),
            (Vec3::new(0., -80., 24.), Vec3::new(0., 80., 24.)),
            (Vec3::new(0., -40., 8.), Vec3::new(0., -40., -8.)),
        ] {
            let tr = sweep(start, end);
            assert!(
                tr.fraction < 1. && !tr.start_solid,
                "yaw {yaw}: real geometry lost: {tr:?}"
            );
        }
    }
}

#[test]
fn zero_width_hulls_are_rejected_without_a_minimum_thickness_cutoff() {
    for axis in 0..3 {
        let mut high = Vec3::ONE;
        high[axis] = 0.;
        let flat = box_hull(Vec3::ZERO, high);
        assert!(convex_from_hull(&flat.points, &flat.triangles, contents::SOLID, 0).is_none());
        high[axis] = 0.0001;
        let thin = box_hull(Vec3::ZERO, high);
        let convex = convex_from_hull(&thin.points, &thin.triangles, contents::SOLID, 0).unwrap();
        let mut world = CollisionWorld::new();
        assert!(world.add_convex(convex));
        world.finalize();
        // Acceptance alone is insufficient: the thin geometry must still
        // stop a sweep from either side, including a standing player hull.
        for (mins, maxs) in [
            (Vec3::ZERO, Vec3::ZERO),
            (Vec3::new(-16., -16., 0.), Vec3::new(16., 16., 72.)),
        ] {
            for (from, to) in [(-100., 100.), (100., -100.)] {
                let mut start = Vec3::splat(0.5);
                let mut end = start;
                start[axis] = from;
                end[axis] = to;
                let tr = world.trace_box(start, end, mins, maxs, contents::MASK_PLAYERSOLID);
                assert!(
                    tr.fraction < 1. && !tr.start_solid,
                    "axis {axis}: thin solid lost: {tr:?}"
                );
                assert!(tr.normal[axis].abs() > 0.99);
            }
        }
    }
    let mut invalid = box_hull(Vec3::ZERO, Vec3::ONE);
    invalid.points[0].x = f32::NAN;
    assert!(convex_from_hull(&invalid.points, &invalid.triangles, contents::SOLID, 0).is_none());
}

// Entirely authored boxes encoded as a compact surface with shared points.
// A branch's enclosing approximation spans the empty space between the
// tower and the other platform. Only leaf references describe real solids.
fn shared_point_phy(pieces: &[mdl::ConvexHull], approximation: &mdl::ConvexHull) -> Vec<u8> {
    let all: Vec<_> = pieces.iter().chain(std::iter::once(approximation)).collect();
    let ledge_bytes = 16 + 12 * 16;
    let points = 48 + all.len() * ledge_bytes;
    let tree = points + all.len() * 8 * 16;
    let mut surface = vec![0u8; tree + (2 * pieces.len() - 1) * 28];
    let write = |bytes: &mut [u8], at: usize, n: i32| {
        bytes[at..at + 4].copy_from_slice(&n.to_le_bytes());
    };
    write(&mut surface, 32, tree as i32);
    for (i, hull) in all.iter().enumerate() {
        let at = 48 + i * ledge_bytes;
        write(&mut surface, at, (points - at) as i32);
        write(&mut surface, at + 8, (((ledge_bytes + 8 * 16) / 16) << 8) as i32 | 4);
        surface[at + 12..at + 14].copy_from_slice(&12i16.to_le_bytes());
        for (t, tri) in hull.triangles.iter().enumerate() {
            for (e, index) in tri.iter().enumerate() {
                write(&mut surface, at + 16 + t * 16 + 4 + e * 4, (i * 8) as i32 + *index as i32);
            }
        }
        for (v, point) in hull.points.iter().enumerate() {
            let physics = Vec3::new(point.x, -point.z, point.y) * 0.0254;
            for axis in 0..3 {
                write(&mut surface, points + (i * 8 + v) * 16 + axis * 4, physics[axis].to_bits() as i32);
            }
        }
    }
    write(&mut surface, tree + 4, (48 + pieces.len() * ledge_bytes) as i32 - tree as i32);
    for i in 0..pieces.len() {
        let node = tree + i * 56;
        let leaf = if i + 1 < pieces.len() {
            write(&mut surface, node, 56);
            node + 28
        } else { node };
        write(&mut surface, leaf + 4, (48 + i * ledge_bytes) as i32 - leaf as i32);
    }
    let mut bytes = vec![0u8; 20];
    write(&mut bytes, 0, 16);
    write(&mut bytes, 8, 1);
    write(&mut bytes, 16, surface.len() as i32);
    bytes.extend(surface);
    bytes
}

#[test]
fn shared_point_phy_preserves_roof_support_and_open_space_after_placement() {
    let pieces = [
        box_hull(Vec3::new(-64., -64., 0.), Vec3::new(64., 64., 96.)),
        box_hull(Vec3::new(-48., -48., 96.), Vec3::new(48., 48., 160.)),
        box_hull(Vec3::new(192., -48., 0.), Vec3::new(256., 48., 128.)),
    ];
    let enclosing = box_hull(Vec3::new(-64., -64., 0.), Vec3::new(256., 64., 160.));
    let phy = mdl::Phy::parse(&shared_point_phy(&pieces, &enclosing)).unwrap();
    assert_eq!(phy.hulls.len(), pieces.len());
    for yaw in [0., 90., 270., 315.] {
        for scale in [0.5, 1., 1.5] {
            let origin = Vec3::new(720., -432., 128.);
            let angles = Vec3::new(0., yaw, 0.);
            let rotation = mdl::angles_to_mat3(angles);
            let placed = |point: Vec3| origin + rotation * (point * scale);
            let map = bsp::Bsp {
                static_props: bsp::StaticProps {
                    models: vec!["synthetic-support.mdl".into()],
                    props: vec![bsp::StaticProp {
                        origin, angles, model: 0, solid: 6, flags: 0, skin: 0,
                        fade_min: 0., fade_max: 0., scale,
                        lighting_origin: None, diffuse_modulation: None,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            };
            let world = CollisionWorld::from_bsp(&map, &MapCollisionOptions::default(),
                &|_| Some(phy.hulls.clone()));
            assert_eq!(world.convexes.len(), 3);
            let mins = Vec3::new(-16., -16., 0.);
            let maxs = Vec3::new(16., 16., 72.);
            for (x, top) in [(0., 160.), (224., 128.)] {
                let trace = world.trace_box(placed(Vec3::new(x, 0., 256.)),
                    placed(Vec3::new(x, 0., -32.)), mins, maxs, contents::MASK_PLAYERSOLID);
                assert!(trace.hit() && !trace.start_solid && !trace.all_solid, "{yaw}/{scale}: {trace:?}");
                assert!(trace.normal.z > 0.99, "roof must be walkable: {trace:?}");
                assert!((trace.end.z - origin.z - top * scale - DIST_EPSILON).abs() < 0.002,
                    "later roof piece lost: {yaw}/{scale}: {trace:?}");
                let standing = world.trace_box(trace.end, trace.end, mins, maxs, contents::MASK_PLAYERSOLID);
                assert!(!standing.start_solid && !standing.all_solid);
            }
            let across = world.trace_box(placed(Vec3::new(-24., 0., 160.)) + Vec3::Z * DIST_EPSILON,
                placed(Vec3::new(24., 0., 160.)) + Vec3::Z * DIST_EPSILON,
                mins, maxs, contents::MASK_PLAYERSOLID);
            assert!(!across.hit(), "standing traversal must remain above roof: {across:?}");
            let gap = world.trace_box(placed(Vec3::new(128., 0., 256.)),
                placed(Vec3::new(128., 0., -32.)), mins, maxs, contents::MASK_PLAYERSOLID);
            assert!(!gap.hit(), "branch approximation must not fill the open bay: {gap:?}");
        }
    }
}

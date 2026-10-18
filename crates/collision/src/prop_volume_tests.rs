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
        assert!(convex_from_hull(&thin.points, &thin.triangles, contents::SOLID, 0).is_some());
    }
    let mut invalid = box_hull(Vec3::ZERO, Vec3::ONE);
    invalid.points[0].x = f32::NAN;
    assert!(convex_from_hull(&invalid.points, &invalid.triangles, contents::SOLID, 0).is_none());
}

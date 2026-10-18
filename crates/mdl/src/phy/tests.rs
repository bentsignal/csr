use super::*;

const LEDGE: usize = 80; // 16-byte header plus four 16-byte triangles.

fn i32_at(bytes: &mut [u8], at: usize, value: i32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

// Authored tetrahedron bytes, not copied game data. All ledges reference the
// same four points, including a legitimate vertex at the origin.
fn surface(count: usize) -> Vec<u8> {
    let points = 48 + count * LEDGE;
    let mut s = vec![0; points + 4 * 16];
    let len = s.len() as i32;
    i32_at(&mut s, 32, len);
    for ledge in 0..count {
        let at = 48 + ledge * LEDGE;
        i32_at(&mut s, at, (points - at) as i32);
        i32_at(&mut s, at + 8, ((LEDGE / 16) << 8) as i32);
        s[at + 12..at + 14].copy_from_slice(&4i16.to_le_bytes());
        for (t, tri) in [[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]]
            .iter()
            .enumerate()
        {
            for (e, &index) in tri.iter().enumerate() {
                i32_at(&mut s, at + 16 + t * 16 + 4 + e * 4, index);
            }
        }
    }
    for (i, p) in [
        [0., 0., 0.],
        [0.0254, 0., 0.],
        [0., 0.0254, 0.],
        [0., 0., 0.0254],
    ]
    .iter()
    .enumerate()
    {
        for (axis, &value) in p.iter().enumerate() {
            i32_at(
                &mut s,
                points + i * 16 + axis * 4,
                f32::to_bits(value) as i32,
            );
        }
    }
    s
}

fn parse_surface(s: &[u8], wrapped: bool) -> Phy {
    let mut solid = Vec::new();
    if wrapped {
        solid.resize(28, 0);
        solid[..4].copy_from_slice(b"VPHY");
    }
    solid.extend_from_slice(s);
    let mut bytes = vec![0; 20];
    i32_at(&mut bytes, 0, 16);
    i32_at(&mut bytes, 8, 1);
    i32_at(&mut bytes, 16, solid.len() as i32);
    bytes.extend_from_slice(&solid);
    bytes.extend_from_slice(b"\"solid\" {}\0");
    let phy = Phy::parse(&bytes).unwrap();
    assert_eq!(phy.text, "\"solid\" {}");
    phy
}

fn assert_tetrahedron(hull: &ConvexHull) {
    assert_eq!(hull.triangles.len(), 4);
    assert_eq!(hull.points.len(), 4);
    for point in [Vec3::ZERO, Vec3::X, -Vec3::Z, Vec3::Y] {
        assert!(hull.points.iter().any(|p| p.distance(point) < 0.00001));
    }
    assert!(hull.triangles.iter().flatten().all(|&i| i < 4));
}

#[test]
fn valid_points_including_origin_survive_wrapped_and_unwrapped_surfaces() {
    for wrapped in [false, true] {
        let phy = parse_surface(&surface(2), wrapped);
        assert_eq!(phy.hulls.len(), 2);
        for hull in &phy.hulls {
            assert_tetrahedron(hull);
        }
    }
}

#[test]
fn bad_point_reference_discards_whole_piece_and_preserves_valid_neighbors() {
    for wrapped in [false, true] {
        for bad_index in [4, 65535] {
            let mut s = surface(3);
            // Only one edge is bad: the remaining points have positive extent
            // on every axis, so a downstream zero-width guard is insufficient.
            i32_at(&mut s, 48 + LEDGE + 16 + 4, bad_index);
            let phy = parse_surface(&s, wrapped);
            assert_eq!(phy.hulls.len(), 2);
            for hull in &phy.hulls {
                assert_tetrahedron(hull);
            }
        }
    }
}

#[test]
fn invalid_point_array_offsets_do_not_fabricate_origin_hulls() {
    for point_offset in [i32::MIN, i32::MAX] {
        let mut s = surface(3);
        i32_at(&mut s, 48 + LEDGE, point_offset);
        let phy = parse_surface(&s, true);
        assert_eq!(phy.hulls.len(), 2);
        for hull in &phy.hulls {
            assert_tetrahedron(hull);
        }
    }
}

#[test]
fn incomplete_point_and_nonfinite_coordinates_reject_the_entire_piece() {
    let s = surface(1);
    // Point 3 is referenced. Its Z must fit, but no interpretation of the
    // unused fourth float is required by this reader.
    assert_eq!(parse_surface(&s[..s.len() - 4], false).hulls.len(), 1);
    assert!(parse_surface(&s[..s.len() - 5], false).hulls.is_empty());
    for axis in 0..3 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, 2000.] {
            let mut s = surface(1);
            i32_at(&mut s, 48 + LEDGE + axis * 4, value.to_bits() as i32);
            assert!(
                parse_surface(&s, true).hulls.is_empty(),
                "axis {axis}, {value}"
            );
        }
    }
}

#[test]
fn invalid_ledge_extent_cannot_read_triangles_from_unrelated_bytes() {
    for size in [16, 64, 1 << 27] {
        let mut s = surface(2);
        i32_at(&mut s, 48 + LEDGE + 8, ((size / 16) << 8) as i32);
        let phy = parse_surface(&s, true);
        assert_eq!(phy.hulls.len(), 1);
        assert_tetrahedron(&phy.hulls[0]);
    }
}

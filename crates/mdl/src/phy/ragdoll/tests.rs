use super::*;
use glam::{Mat4, Quat, Vec3};

fn set_i32(bytes: &mut [u8], at: usize, value: i32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

// Own asset-free tetrahedron in the public compact-surface layout.
fn solid() -> Vec<u8> {
    let mut bytes = vec![0; 28 + 192 + 28];
    bytes[..4].copy_from_slice(b"VPHY");
    bytes[4..6].copy_from_slice(&256_u16.to_le_bytes());
    let surface = &mut bytes[28..];
    set_i32(surface, 32, 192);
    // Complete the compact surface with its single leaf tree node. The
    // static reader follows this reference; articulated parsing stays separate.
    set_i32(surface, 196, 48 - 192);
    set_i32(surface, 48, 80);
    set_i32(surface, 56, 5 << 8);
    surface[60..62].copy_from_slice(&4_i16.to_le_bytes());
    for (index, face) in [[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]]
        .iter()
        .enumerate()
    {
        for (edge, point) in face.iter().enumerate() {
            set_i32(surface, 64 + index * 16 + 4 + edge * 4, *point);
        }
    }
    for (index, point) in [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
        .iter()
        .enumerate()
    {
        for (axis, value) in point.iter().enumerate() {
            surface[128 + index * 16 + axis * 4..132 + index * 16 + axis * 4]
                .copy_from_slice(&f32::to_le_bytes(*value));
        }
    }
    bytes
}

fn file(text: &str, n: usize) -> Vec<u8> {
    let mut bytes = vec![0; 16];
    set_i32(&mut bytes, 0, 16);
    set_i32(&mut bytes, 8, n as i32);
    set_i32(&mut bytes, 12, 123);
    for index in 0..n {
        let mut solid = solid();
        // Distinct authored geometry per binary solid makes declaration-order
        // ownership failures observable, instead of comparing identical hulls.
        for point in 0..4 {
            let at = 28 + 128 + point * 16;
            let x = f32::from_le_bytes(solid[at..at + 4].try_into().unwrap());
            solid[at..at + 4].copy_from_slice(&(x + index as f32 * 2.).to_le_bytes());
        }
        bytes.extend_from_slice(&(solid.len() as i32).to_le_bytes());
        bytes.extend_from_slice(&solid);
    }
    bytes.extend_from_slice(text.as_bytes());
    bytes.push(0);
    bytes
}

fn metadata() -> String {
    let properties = "mass 2 surfaceprop flesh damping 0.4 rotdamping 8 inertia 10 volume 3";
    format!(
        r#"
        solid {{ index 1 name Arm parent Pelvis {properties} massbias 3 }}
        solid {{ index 0 name Pelvis {properties} }}
        ragdollconstraint {{ parent 0 child 1 xmin -15 xmax 20 xfriction 0.4 ymin 0 ymax 110 yfriction 0.4 zmin 0 zmax 0 zfriction 0.4 }}
        collisionrules {{ collisionpair "0,1" collisionpair "1,0" }}
        editparams {{ rootname Pelvis totalmass 4 jointmerge "arm,finger" }}
        extension {{ futurefield "retained" }}
    "#
    )
}

fn bones() -> Vec<Bone> {
    ["Pelvis", "Arm", "Finger"]
        .iter()
        .map(|name| Bone {
            name: name.to_string(),
            parent: -1,
            pos: Vec3::ZERO,
            quat: Quat::IDENTITY,
            rot: Vec3::ZERO,
            pos_scale: Vec3::ONE,
            rot_scale: Vec3::ONE,
            pose_to_bone: Mat4::IDENTITY,
            flags: 0,
        })
        .collect()
}

#[test]
fn preserves_authored_ownership_order_axes_and_repeated_fields() {
    let data = file(&metadata(), 2);
    let ragdoll = Ragdoll::parse(&data).unwrap();
    assert_eq!(ragdoll.solids[0].name, "Pelvis");
    assert!(ragdoll.solids[1].hulls[0].bounds().0.x > ragdoll.solids[0].hulls[0].bounds().1.x);
    assert_eq!(ragdoll.solids[1].parent.as_deref(), Some("Pelvis"));
    assert_eq!(ragdoll.solids[1].mass_bias, Some(3.));
    assert_eq!(
        ragdoll.constraints[0].axes[1],
        AxisLimit {
            min: 0.,
            max: 110.,
            friction: 0.4
        }
    );
    assert_eq!(
        ragdoll.collision_pairs,
        [(SolidId(0), SolidId(1)), (SolidId(1), SolidId(0))]
    );
    assert_eq!(ragdoll.joint_merges, [("arm".into(), "finger".into())]);
    assert_eq!(ragdoll.metadata.last().unwrap().fields[0].1, "retained");
    assert_eq!(ragdoll.bind(123, &bones()).unwrap().solid_bones, [0, 1]);
    // Additive decoder agrees with the unchanged static-prop reader on valid
    // geometry; grouping, not new shapes or conversion, is the addition.
    let static_phy = super::super::Phy::parse(&data).unwrap();
    assert_eq!(static_phy.hulls.len(), 2);
    for (a, b) in static_phy
        .hulls
        .iter()
        .zip(ragdoll.solids.iter().flat_map(|s| &s.hulls))
    {
        assert_eq!(a.points, b.points);
        assert_eq!(a.triangles, b.triangles);
    }
}

#[test]
fn encoded_ledge_extent_can_include_its_owned_point_array() {
    let mut data = file(&metadata(), 2);
    for index in 0..2 {
        let at = 16 + index * (4 + solid().len()) + 4 + 28;
        // Triangle records occupy80 bytes; this encoded144-byte extent also
        // owns the64-byte point array, as observed in installed player PHYs.
        set_i32(&mut data, at + 56, (9 << 8) | 4);
    }
    let ragdoll = Ragdoll::parse(&data).unwrap();
    let static_phy = super::super::Phy::parse(&data).unwrap();
    assert_eq!(static_phy.hulls.len(), 2);
    for (typed, flattened) in ragdoll.solids.iter().zip(&static_phy.hulls) {
        assert_eq!(typed.hulls[0].points, flattened.points);
        assert_eq!(typed.hulls[0].triangles, flattened.triangles);
    }
}

#[test]
fn unknown_version_cannot_borrow_the_observed_geometry_layout() {
    let supported = file(&metadata(), 2);
    assert!(Ragdoll::parse(&supported).is_ok());
    for version in [0_u16, 255, 257, u16::MAX] {
        let mut unknown = supported.clone();
        // All geometry and metadata remain unchanged; only the first solid's
        // version differs, so failure is caused by the format gate.
        unknown[24..26].copy_from_slice(&version.to_le_bytes());
        let error = Ragdoll::parse(&unknown).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "unsupported ragdoll solid format");
    }
}

#[test]
fn binding_rejects_wrong_model_missing_or_ambiguous_bones() {
    let ragdoll = Ragdoll::parse(&file(&metadata(), 2)).unwrap();
    assert!(ragdoll.bind(124, &bones()).is_err());
    assert!(ragdoll.bind(123, &bones()[..1]).is_err());
    assert!(ragdoll.bind(123, &bones()[..2]).is_err()); // missing merged Finger
    let mut duplicate = bones();
    duplicate.push(duplicate[0].clone());
    duplicate.last_mut().unwrap().name = "pelvis".into();
    assert!(ragdoll.bind(123, &duplicate).is_err());
}

#[test]
fn case_variant_solid_aliases_cannot_bind_independent_bodies_to_one_bone() {
    // Names remain a connected, case-sensitively distinct tree in the file,
    // but both would resolve to Pelvis under the binding's case normalization.
    let ambiguous = metadata().replace("name Arm", "name PELVIS");
    assert!(Ragdoll::parse(&file(&ambiguous, 2)).is_err());
    // Binding also defends its boundary if a caller modifies parsed metadata.
    let mut modified = Ragdoll::parse(&file(&metadata(), 2)).unwrap();
    modified.solids[1].name = "PELVIS".into();
    assert!(modified.bind(123, &bones()).is_err());
}

#[test]
fn invalid_metadata_never_produces_partial_body() {
    for (from, to) in [
        ("index 1", "index 0"),
        ("index 1", "index 2"),
        ("name Arm", "name Pelvis"),
        ("parent Pelvis", "parent Finger"),
        ("parent 0 child 1", "parent 1 child 1"),
        ("parent 0 child 1", "parent 1 child 0"),
        ("xmin -15 xmax 20", "xmin 30 xmax 20"),
        ("mass 2", "mass NaN"),
        ("mass 2", "mass -1"),
        ("damping 0.4", "damping inf"),
        ("xfriction 0.4", "xfriction -1"),
        ("collisionpair \"0,1\"", "collisionpair \"0,2\""),
        ("collisionpair \"0,1\"", "collisionpair \"0,0\""),
        ("totalmass 4", "totalmass 4 totalmass 5"),
        ("rootname Pelvis", "rootname Unknown"),
    ] {
        assert!(
            Ragdoll::parse(&file(&metadata().replace(from, to), 2)).is_err(),
            "{from} -> {to}"
        );
    }
    let disconnected = metadata().replace("ragdollconstraint { parent 0 child 1 xmin -15 xmax 20 xfriction 0.4 ymin 0 ymax 110 yfriction 0.4 zmin 0 zmax 0 zfriction 0.4 }", "");
    assert!(Ragdoll::parse(&file(&disconnected, 2)).is_err());
}

#[test]
fn malformed_geometry_and_header_return_errors_without_panic() {
    let data = file(&metadata(), 2);
    let binary_end = 16 + 2 * (4 + solid().len());
    for end in 0..binary_end {
        assert!(Ragdoll::parse(&data[..end]).is_err());
    }
    for (offset, value) in [
        (0, -1),
        (8, -1),
        (8, 65),
        (16, i32::MAX),
        (48 + 28 + 20, i32::MIN),
    ] {
        let mut broken = data.clone();
        set_i32(&mut broken, offset, value);
        assert!(Ragdoll::parse(&broken).is_err());
    }
    let mut broken = data.clone();
    set_i32(&mut broken, 20 + 28 + 128, f32::NAN.to_bits() as i32);
    assert!(Ragdoll::parse(&broken).is_err());
    let mut tree_as_point = data.clone();
    set_i32(&mut tree_as_point, 20 + 28 + 64 + 4, 4);
    assert!(Ragdoll::parse(&tree_as_point).is_err());
    let mut unsupported = data.clone();
    unsupported[26..28].copy_from_slice(&1_i16.to_le_bytes());
    assert!(Ragdoll::parse(&unsupported).is_err());
}

#[test]
fn cyclic_constraint_graph_is_rejected_even_with_consistent_parent_names() {
    let ragdoll = Ragdoll::parse(&file(&metadata(), 2)).unwrap();
    let mut solids = ragdoll.solids;
    solids.push(solids[1].clone());
    solids[2].id = SolidId(2);
    solids[2].name = "Other".into();
    solids[1].parent = Some("Other".into());
    solids[2].parent = Some("Arm".into());
    let axis = [AxisLimit {
        min: 0.,
        max: 1.,
        friction: 0.,
    }; 3];
    assert!(
        validate_tree(
            &solids,
            &[
                Constraint {
                    parent: SolidId(2),
                    child: SolidId(1),
                    axes: axis
                },
                Constraint {
                    parent: SolidId(1),
                    child: SolidId(2),
                    axes: axis
                },
            ],
            0
        )
        .is_err()
    );
}

#[test]
fn token_bounds_and_nested_unknown_data_fail_explicitly() {
    assert!(parse_metadata("solid { field \"unfinished").is_err());
    assert!(parse_metadata("extension { nested { data value } }").is_err());
    assert!(
        parse_metadata(&format!(
            "extension {{ {} }}",
            "a b ".repeat(MAX_FIELDS + 1)
        ))
        .is_err()
    );
    let mut huge = vec![0; MAX_BYTES + 1];
    set_i32(&mut huge, 0, 16);
    assert!(Ragdoll::parse(&huge).is_err());
}

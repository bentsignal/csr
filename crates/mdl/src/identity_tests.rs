use super::*;

/// Entirely synthetic MDL/VVD/VTX: one mesh with an intentionally shuffled
/// hardware vertex table and three distinct positions. No game asset bytes.
pub(crate) fn fixture() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    fn put(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    let checksum = 0xaabb_ccddu32;
    let mut mdl = vec![0u8; 688];
    mdl[..4].copy_from_slice(b"IDST");
    for (at, value) in [
        (4, 49),
        (8, checksum),
        (232, 1),
        (236, 408),
        (412, 1),
        (420, 16),
        (496, 1),
        (500, 148),
        (580, 3),
    ] {
        put(&mut mdl, at, value);
    }
    let mut vvd = vec![0u8; 208];
    vvd[..4].copy_from_slice(b"IDSV");
    for (at, value) in [(4, 4), (8, checksum), (12, 1), (16, 3), (56, 64)] {
        put(&mut vvd, at, value);
    }
    for i in 0..3 {
        put(&mut vvd, 64 + i * 48 + 16, (10.0f32 + i as f32).to_bits());
        put(&mut vvd, 64 + i * 48 + 36, 1.0f32.to_bits());
    }
    let mut vtx = vec![0u8; 139];
    for (at, value) in [
        (0, 7),
        (16, checksum),
        (20, 1),
        (28, 1),
        (32, 36),
        (36, 1),
        (40, 8),
        (44, 1),
        (48, 8),
        (52, 1),
        (56, 12),
        (64, 1),
        (68, 9),
        (73, 3),
        (77, 33),
        (81, 3),
        (85, 60),
    ] {
        put(&mut vtx, at, value);
    }
    for (i, source) in [2u16, 0, 1].into_iter().enumerate() {
        vtx[110 + i * 9..112 + i * 9].copy_from_slice(&source.to_le_bytes());
        vtx[133 + i * 2..135 + i * 2].copy_from_slice(&(i as u16).to_le_bytes());
    }
    (mdl, vvd, vtx)
}

#[test]
fn checksum_and_shuffled_hardware_identity_preserve_geometry() {
    let (mdl, vvd, vtx) = fixture();
    let model = Model::load(&mdl, &vvd, &vtx).unwrap();
    assert_eq!(model.checksum, 0xaabb_ccdd);
    assert_eq!(Model::load_header(&mdl).unwrap().checksum, model.checksum);
    assert_eq!(model.meshes.len(), 1);
    let mesh = &model.meshes[0];
    assert_eq!(
        mesh.hardware,
        Some(HardwareMesh {
            body_part: 0,
            sub_model: 0,
            mesh: 0,
            lod: 0
        })
    );
    assert_eq!(mesh.source_vertex_indices, [2, 0, 1]);
    assert_eq!(
        mesh.vertices
            .iter()
            .map(|v| v.position.x)
            .collect::<Vec<_>>(),
        [12., 10., 11.]
    );
    assert_eq!(mesh.indices, [0, 1, 2]);
}

#[test]
fn multiple_strip_groups_keep_concatenated_source_identity_and_index_bases() {
    let (mdl, vvd, mut vtx) = fixture();
    vtx.truncate(73);
    vtx.resize(178, 0);
    for (at, value) in [
        (64, 2u32),
        (73, 2),
        (77, 66),
        (81, 3),
        (85, 84),
        (106, 1),
        (110, 57),
        (114, 3),
        (118, 66),
    ] {
        vtx[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (at, value) in [(143, 2u16), (152, 0), (167, 1), (159, 1)] {
        vtx[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    let model = Model::load(&mdl, &vvd, &vtx).unwrap();
    let mesh = &model.meshes[0];
    assert_eq!(mesh.source_vertex_indices, [2, 0, 1]);
    assert_eq!(
        mesh.vertices
            .iter()
            .map(|v| v.position.x)
            .collect::<Vec<_>>(),
        [12., 10., 11.]
    );
    assert_eq!(mesh.indices, [0, 1, 0, 2, 2, 2]);
}

#[test]
fn source_identity_is_fixup_resolved_not_an_original_vvd_file_offset() {
    let (mdl, mut vvd, vtx) = fixture();
    vvd.resize(244, 0);
    for (at, value) in [(48, 3u32), (52, 208)] {
        vvd[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (i, source) in [1u32, 2, 0].into_iter().enumerate() {
        vvd[212 + i * 12..216 + i * 12].copy_from_slice(&source.to_le_bytes());
        vvd[216 + i * 12..220 + i * 12].copy_from_slice(&1u32.to_le_bytes());
    }
    let model = Model::load(&mdl, &vvd, &vtx).unwrap();
    let mesh = &model.meshes[0];
    assert_eq!(mesh.source_vertex_indices, [2, 0, 1]);
    // The resolved index2 now addresses original vertex0. Its position proves
    // the meaning of the retained index without mislabeling it as raw identity.
    assert_eq!(
        mesh.vertices
            .iter()
            .map(|v| v.position.x)
            .collect::<Vec<_>>(),
        [10., 11., 12.]
    );
}

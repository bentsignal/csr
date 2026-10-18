use super::*;
use crate::{Mesh, Vertex};

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn fixture(counts: &[u32]) -> Vec<u8> {
    let table_end = HEADER_BYTES + counts.len() * MESH_BYTES;
    let mut bytes = vec![0u8; table_end + counts.iter().sum::<u32>() as usize * RECORD_BYTES];
    for (at, value) in [
        (0, 2),
        (4, 0xaabb_ccdd),
        (8, 2),
        (12, RECORD_BYTES as u32),
        (16, counts.iter().sum()),
        (20, counts.len() as u32),
    ] {
        put(&mut bytes, at, value);
    }
    let mut offset = table_end;
    for (i, count) in counts.iter().enumerate() {
        let at = HEADER_BYTES + i * MESH_BYTES;
        put(&mut bytes, at + 4, *count);
        put(&mut bytes, at + 8, offset as u32);
        for ordinal in 0..*count as usize {
            bytes[offset + ordinal * RECORD_BYTES..offset + (ordinal + 1) * RECORD_BYTES]
                .fill((i * 13 + ordinal + 1) as u8);
        }
        offset += *count as usize * RECORD_BYTES;
    }
    bytes
}
fn lod_fixture(spans: &[(u32, u32)]) -> Vec<u8> {
    let mut bytes = fixture(&spans.iter().map(|&(_, count)| count).collect::<Vec<_>>());
    for (i, &(lod, _)) in spans.iter().enumerate() {
        put(&mut bytes, HEADER_BYTES + i * MESH_BYTES, lod);
    }
    bytes
}
fn key(map: &str, digest: u8, ordinal: u32) -> InstanceKey {
    InstanceKey::new(
        ContentId::csgo(ContentKind::Map, map),
        [digest; 20],
        ordinal,
    )
    .unwrap()
}
fn model(counts: &[usize]) -> Model {
    Model {
        checksum: 0xaabb_ccdd,
        meshes: counts
            .iter()
            .enumerate()
            .map(|(i, n)| Mesh {
                hardware: Some(HardwareMesh {
                    body_part: 0,
                    sub_model: 0,
                    mesh: i as u32,
                    lod: 0,
                }),
                vertices: vec![Vertex::default(); *n],
                source_vertex_indices: (0..*n as u32).rev().collect(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn borrowed_multi_mesh_records_preserve_unknown_words_and_bytes() {
    let mut bytes = fixture(&[2, 1]);
    put(&mut bytes, 8, 0xdead_beef);
    put(&mut bytes, 24, 7);
    put(&mut bytes, HEADER_BYTES + 12, 9);
    let file = File::parse(&bytes).unwrap();
    assert_eq!(file.raw_flags, 0xdead_beef);
    assert_eq!(file.reserved, [7, 0, 0, 0]);
    assert_eq!(file.meshes()[0].reserved, [9, 0, 0, 0]);
    assert_eq!(
        file.meshes()[0]
            .opaque_records()
            .copied()
            .collect::<Vec<_>>(),
        [[1; 12], [2; 12]]
    );
    assert_eq!(file.meshes()[1].opaque_records().next(), Some(&[14; 12]));
    assert_eq!(
        file.meshes()[0].opaque_bytes().as_ptr(),
        bytes[96..].as_ptr()
    );
}

#[test]
fn every_truncated_prefix_is_rejected_without_partial_spans() {
    let bytes = fixture(&[2, 1]);
    for end in 0..bytes.len() {
        assert!(File::parse(&bytes[..end]).is_err(), "prefix {end}");
    }
    assert!(File::parse(&bytes).is_ok());
}

#[test]
fn unsupported_sizes_versions_and_unbounded_tables_are_rejected() {
    for (at, value) in [(0, 1), (12, 4), (20, MAX_MESHES as u32 + 1)] {
        let mut bytes = fixture(&[1]);
        put(&mut bytes, at, value);
        assert!(File::parse(&bytes).is_err());
    }
    assert!(File::parse(&vec![0; MAX_FILE_BYTES + 1]).is_err());
}

#[test]
fn inconsistent_overflowing_overlapping_and_metadata_payloads_are_rejected() {
    for (at, value) in [(16, 5), (44, u32::MAX), (48, 4), (48, u32::MAX), (76, 96)] {
        let mut bytes = fixture(&[2, 1]);
        put(&mut bytes, at, value);
        assert!(File::parse(&bytes).is_err(), "word {at}={value}");
    }
}

#[test]
fn complete_candidate_retains_shuffled_parser_output_without_decoding_order() {
    let (mdl, vvd, vtx) = crate::identity_tests::fixture();
    let model = Model::load(&mdl, &vvd, &vtx).unwrap();
    let bytes = fixture(&[3]);
    let expected = key("de_fixture", 1, 2);
    let scoped = ScopedFile::parse(expected.clone(), "sp_hdr_2.vhv", &bytes).unwrap();
    let candidate = scoped.candidate(&expected, &model).unwrap();
    let (span, mesh) = candidate.matching_spans().next().unwrap();
    assert_eq!(span.opaque_records().len(), 3);
    assert_eq!(mesh.source_vertex_indices, [2, 0, 1]);
    assert_eq!(candidate.instance(), &expected);
}

#[test]
fn explicit_lod0_selection_keeps_hardware_order_and_the_complete_container() {
    let (mdl, vvd, vtx) = crate::identity_tests::fixture();
    let model = Model::load(&mdl, &vvd, &vtx).unwrap();
    let bytes = lod_fixture(&[(0, 3), (1, 2)]);
    let expected = key("de_fixture", 1, 2);
    let scoped = ScopedFile::parse(expected.clone(), "sp_hdr_2.vhv", &bytes).unwrap();
    assert!(
        scoped.candidate(&expected, &model).is_err(),
        "strict API must stay strict",
    );
    let candidate = scoped.candidate_lod0(&expected, &model).unwrap();
    assert_eq!(candidate.instance(), &expected);
    assert_eq!(candidate.matching_spans().len(), 1);
    let (span, mesh) = candidate.matching_spans().next().unwrap();
    assert_eq!(span.lod, 0);
    assert_eq!(
        span.opaque_records().copied().collect::<Vec<_>>(),
        [[1; 12], [2; 12], [3; 12]],
    );
    assert_eq!(span.opaque_bytes().as_ptr(), bytes[96..].as_ptr());
    assert_eq!(
        mesh.source_vertex_indices, [2, 0, 1],
        "never sort into VVD order",
    );
    assert_eq!(scoped.file().vertex_count, 5, "retain the full container total");
    assert_eq!(scoped.file().meshes().len(), 2);
    assert_eq!(
        scoped.file().meshes()[1].opaque_records().next(),
        Some(&[14; 12]),
    );
}

#[test]
fn lod0_selection_requires_complete_ordered_blocks_instead_of_filtering_spans() {
    let expected = key("de_fixture", 1, 0);
    let model = model(&[2, 1]);
    let bytes = lod_fixture(&[(0, 2), (0, 1), (1, 1), (1, 1)]);
    let scoped = ScopedFile::parse(expected.clone(), "sp_hdr_0.vhv", &bytes).unwrap();
    let candidate = scoped.candidate_lod0(&expected, &model).unwrap();
    assert_eq!(
        candidate.matching_spans().map(|(s, _)| s.vertex_count).collect::<Vec<_>>(),
        [2, 1],
    );
    for spans in [
        &[(0, 2), (0, 1), (1, 1)][..], // incomplete lower block
        &[(0, 2), (1, 1), (0, 1), (1, 1)][..], // interleaved LODs
        &[(1, 2), (1, 1), (0, 2), (0, 1)][..], // reversed blocks
        &[(0, 2), (0, 1), (1, 1), (2, 1)][..], // unsupported third LOD
        &[(0, 2), (0, 1), (1, 1), (0, 1)][..], // extra LOD0 span
        &[(0, 2), (0, 1), (1, 1), (1, 0)][..], // empty lower span
        &[(0, 1), (0, 2), (1, 1), (1, 1)][..], // count order differs
    ] {
        let bytes = lod_fixture(spans);
        let scoped = ScopedFile::parse(expected.clone(), "sp_hdr_0.vhv", &bytes).unwrap();
        assert!(scoped.candidate_lod0(&expected, &model).is_err(), "layout {spans:?}");
    }
}

#[test]
fn lod0_selection_retains_scope_checksum_and_every_hardware_gate() {
    let bytes = lod_fixture(&[(0, 2), (0, 1), (1, 1), (1, 1)]);
    let expected = key("de_a", 1, 0);
    let scoped = ScopedFile::parse(expected.clone(), "sp_hdr_0.vhv", &bytes).unwrap();
    let good = model(&[2, 1]);
    for counts in [&[2usize][..], &[2, 1, 1][..], &[2, 2][..], &[1, 2][..]] {
        assert!(scoped.candidate_lod0(&expected, &model(counts)).is_err());
    }
    for wrong in [key("de_b", 1, 0), key("de_a", 2, 0), key("de_a", 1, 1)] {
        assert!(scoped.candidate_lod0(&wrong, &good).is_err());
    }
    let mut variants = Vec::new();
    let mut wrong = good.clone();
    wrong.checksum ^= 1;
    variants.push(wrong);
    for identity in [
        None,
        Some(HardwareMesh { body_part: 0, sub_model: 1, mesh: 1, lod: 0 }),
        Some(HardwareMesh { body_part: 0, sub_model: 0, mesh: 1, lod: 1 }),
        Some(HardwareMesh { body_part: 0, sub_model: 0, mesh: 0, lod: 0 }),
    ] {
        let mut wrong = good.clone();
        wrong.meshes[1].hardware = identity;
        variants.push(wrong);
    }
    let mut wrong = good.clone();
    wrong.meshes[1].source_vertex_indices.clear();
    variants.push(wrong);
    let mut wrong = good.clone();
    wrong.meshes[1].vertices.clear();
    variants.push(wrong);
    for wrong in variants {
        assert!(scoped.candidate_lod0(&expected, &wrong).is_err());
    }
    // Single-LOD data uses the original validation and correspondence.
    let single = fixture(&[2, 1]);
    let scoped = ScopedFile::parse(expected.clone(), "sp_hdr_0.vhv", &single).unwrap();
    assert_eq!(
        scoped.candidate(&expected, &good).unwrap().matching_spans().len(),
        scoped.candidate_lod0(&expected, &good).unwrap().matching_spans().len(),
    );
}

#[test]
fn lower_lod_payload_must_pass_container_validation_before_selection() {
    let bytes = lod_fixture(&[(0, 3), (1, 2)]);
    let expected = key("de_fixture", 1, 0);
    for end in 0..bytes.len() {
        assert!(ScopedFile::parse(expected.clone(), "sp_hdr_0.vhv", &bytes[..end]).is_err());
    }
    // Corrupt only the lower-LOD descriptor/header total: no early LOD0 return.
    for (at, value) in [(76, u32::MAX), (76, 96), (72, u32::MAX), (16, 3)] {
        let mut corrupt = bytes.clone();
        put(&mut corrupt, at, value);
        assert!(ScopedFile::parse(expected.clone(), "sp_hdr_0.vhv", &corrupt).is_err());
    }
}

#[test]
fn same_names_checksums_and_counts_never_override_instance_scope() {
    let bytes = fixture(&[2, 1]);
    let model = model(&[2, 1]);
    let expected = key("de_a", 1, 0);
    let scoped = ScopedFile::parse(expected.clone(), "sp_hdr_0.vhv", &bytes).unwrap();
    for wrong in [key("de_b", 1, 0), key("de_a", 2, 0), key("de_a", 1, 1)] {
        assert!(scoped.candidate(&wrong, &model).is_err());
    }
    assert!(ScopedFile::parse(key("de_a", 1, 1), "sp_hdr_0.vhv", &bytes).is_err());
    assert!(ScopedFile::parse(expected.clone(), "other/sp_hdr_0.vhv", &bytes).is_err());
    assert!(InstanceKey::new(ContentId::csgo(ContentKind::Model, "prop"), [1; 20], 0).is_err());
    assert!(scoped.candidate(&expected, &model).is_ok());
}

#[test]
fn all_spans_identity_and_supported_lod_are_validated_before_candidate() {
    let bytes = fixture(&[2, 1]);
    let expected = key("de_a", 1, 0);
    let scoped = ScopedFile::parse(expected.clone(), "sp_hdr_0.vhv", &bytes).unwrap();
    let mut wrong = model(&[2, 1]);
    wrong.checksum ^= 1;
    assert!(scoped.candidate(&expected, &wrong).is_err());
    for counts in [&[2usize][..], &[2, 2][..], &[1, 2][..]] {
        assert!(scoped.candidate(&expected, &model(counts)).is_err());
    }
    let mut wrong = model(&[2, 1]);
    wrong.meshes[1].hardware = None;
    assert!(scoped.candidate(&expected, &wrong).is_err());
    let mut wrong = model(&[2, 1]);
    wrong.meshes[1].source_vertex_indices.clear();
    assert!(scoped.candidate(&expected, &wrong).is_err());
    let mut wrong = model(&[2, 1]);
    wrong.meshes[1].hardware.as_mut().unwrap().mesh = 0;
    assert!(scoped.candidate(&expected, &wrong).is_err());
    let mut unsupported = fixture(&[2, 1]);
    put(&mut unsupported, 68, 1);
    let unsupported = ScopedFile::parse(expected.clone(), "sp_hdr_0.vhv", &unsupported).unwrap();
    assert!(unsupported.candidate(&expected, &model(&[2, 1])).is_err());
    assert!(scoped.candidate(&expected, &model(&[2, 1])).is_ok());
}

#[test]
fn selected_map_mount_wins_even_when_generic_lookup_returns_another_instance() {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use vfs::{Mount, Vfs};
    let selected_bytes = fixture(&[3]);
    let mut other_bytes = selected_bytes.clone();
    other_bytes[68..].fill(0xaa);
    let mut fs = Vfs::new();
    fs.push_back(
        "selected-bsp",
        Mount::Memory(BTreeMap::from([(
            "sp_hdr_0.vhv".into(),
            Arc::from(selected_bytes.clone()),
        )])),
    );
    fs.push_front(
        "other-map",
        Mount::Memory(BTreeMap::from([(
            "sp_hdr_0.vhv".into(),
            Arc::from(other_bytes.clone()),
        )])),
    );
    assert_eq!(fs.read("sp_hdr_0.vhv"), Some(other_bytes.clone()));
    let expected = key("de_selected", 1, 0);
    let bytes = fs.read_from("selected-bsp", &expected.hdr_name()).unwrap();
    assert_eq!(bytes, selected_bytes);
    let scoped = ScopedFile::parse(expected.clone(), &expected.hdr_name(), &bytes).unwrap();
    let model = model(&[3]);
    let candidate = scoped.candidate(&expected, &model).unwrap();
    assert_eq!(
        candidate
            .matching_spans()
            .next()
            .unwrap()
            .0
            .opaque_records()
            .next(),
        Some(&[1; 12])
    );
    // Replacement with a missing selected file must not admit the other map's
    // identically named data, even though its checksum and counts are valid.
    fs.unmount("selected-bsp");
    fs.push_back("selected-bsp", Mount::Memory(BTreeMap::new()));
    assert!(fs.read_from("selected-bsp", &expected.hdr_name()).is_none());
    assert_eq!(fs.read(&expected.hdr_name()), Some(other_bytes));
}

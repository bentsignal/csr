//! Narrow authored static-prop lighting candidate.
//!
//! Only identical three-group records with zero control bytes qualify. The
//! blue-first normalized-byte interpretation remains subject to matched native
//! rendering before this candidate can be accepted as a format implementation.

use mdl::vhv::{InstanceKey, ScopedFile};

/// Keep the selected-map/checksum/hardware-count candidate gate ahead of
/// decoding. The proposed correspondence uses the existing mesh's hardware-
/// output order, not VVD order. Counts/checksums alone do not prove individual
/// record ordering; that interpretation still needs spatial native admission.
pub(super) fn load(
    level: &level::Level,
    ordinal: usize,
    model: &mdl::Model,
) -> Option<Vec<Option<Vec<glam::Vec3>>>> {
    // Preserve the accepted ordinary/no-cast-shadow receiver admission.
    // A file's existence does not establish that lighting-origin (0x02) props
    // use this receiver pipeline in native rendering. Keep that mode on its
    // authored-origin ambient/Sun fallback until separately observed.
    // Fade-only props retain their accepted spatial path until separately
    // observed; ignored normals and other unsupported modes remain closed.
    let flags = level.bsp.static_props.props.get(ordinal)?.flags;
    if !supports_bake(flags) {
        return None;
    }
    let key = InstanceKey::new(
        level.id.clone(),
        *level.bsp_sha1(),
        ordinal.try_into().ok()?,
    )
    .ok()?;
    let name = key.hdr_name();
    // Never substitute an installed map's VHV or a same-named generic mount.
    let bytes = level.vfs.read_from("map", &name)?;
    let scoped = ScopedFile::parse(key.clone(), &name, &bytes).ok()?;
    scoped_colors(flags, &key, model, &scoped)
}

fn supports_bake(flags: u8) -> bool {
    flags & !(0x01 | 0x10) == 0 && flags != 0x01
}

fn scoped_colors(
    flags: u8,
    key: &InstanceKey,
    model: &mdl::Model,
    scoped: &ScopedFile<'_>,
) -> Option<Vec<Option<Vec<glam::Vec3>>>> {
    if !supports_bake(flags) {
        return None;
    }
    if scoped.file().raw_flags != 2 {
        return None;
    }
    // Preserve accepted flags0 output/admission. No-cast-shadow receiver
    // modes may select LOD0 from the explicitly supported two-LOD container.
    // No lower-LOD records enter geometry emitted by the LOD0 model loader.
    let candidate = if flags == 0 {
        scoped.candidate(key, model)
    } else {
        scoped.candidate_lod0(key, model)
    }
    .ok()?;
    Some(
        candidate
            .matching_spans()
            .map(|(span, _)| decode(span.opaque_bytes()))
            .collect(),
    )
}

fn decode(bytes: &[u8]) -> Option<Vec<glam::Vec3>> {
    if bytes.is_empty() || bytes.len() % mdl::vhv::RECORD_BYTES != 0 {
        return None;
    }
    bytes
        .chunks_exact(mdl::vhv::RECORD_BYTES)
        .map(|record| {
            // Refuse directional or nonzero-control data, including a single bad
            // record. A rejected mesh retains its previous ambient-cube/Sun path.
            if record[..4] != record[4..8] || record[..4] != record[8..12] || record[3] != 0 {
                return None;
            }
            Some(glam::Vec3::new(record[2] as f32, record[1] as f32, record[0] as f32) / 255.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{decode, scoped_colors};
    use mdl::vhv::{InstanceKey, ScopedFile};

    fn bake_fixture(with_lod1: bool) -> (InstanceKey, mdl::Model, Vec<u8>) {
        let spans = if with_lod1 { 2 } else { 1 };
        let payload = 40 + spans * 28;
        let mut bytes = vec![0u8; payload + (2 + usize::from(with_lod1)) * 12];
        let mut put = |at: usize, value: u32| {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        for (at, value) in [
            (0, 2), (4, 123), (8, 2), (12, 12),
            (16, 2 + u32::from(with_lod1)), (20, spans as u32),
            (44, 2), (48, payload as u32),
        ] {
            put(at, value);
        }
        if with_lod1 {
            for (at, value) in [(68, 1), (72, 1), (76, (payload + 24) as u32)] {
                put(at, value);
            }
        }
        bytes[payload..payload + 24].copy_from_slice(&[
            17, 31, 47, 0, 17, 31, 47, 0, 17, 31, 47, 0,
            53, 71, 89, 0, 53, 71, 89, 0, 53, 71, 89, 0,
        ]);
        if with_lod1 {
            // Valid opaque container data, deliberately outside the decoder's
            // semantic subset. It belongs to LOD1 and must never color LOD0.
            bytes[payload + 24..].fill(255);
        }
        let key = InstanceKey::new(
            content::ContentId::csgo(content::ContentKind::Map, "de_fixture"),
            [7; 20],
            5,
        )
        .unwrap();
        let model = mdl::Model {
            checksum: 123,
            meshes: vec![mdl::Mesh {
                hardware: Some(mdl::HardwareMesh {
                    body_part: 0, sub_model: 0, mesh: 0, lod: 0,
                }),
                vertices: vec![mdl::Vertex::default(); 2],
                source_vertex_indices: vec![1, 0],
                ..Default::default()
            }],
            ..Default::default()
        };
        (key, model, bytes)
    }

    #[test]
    fn receiver_modes_select_only_lod0_without_changing_flags0_admission() {
        let (key, model, bytes) = bake_fixture(true);
        let scoped = ScopedFile::parse(key.clone(), &key.hdr_name(), &bytes).unwrap();
        let expected = vec![Some(vec![
            glam::Vec3::new(47.0, 31.0, 17.0) / 255.0,
            glam::Vec3::new(89.0, 71.0, 53.0) / 255.0,
        ])];
        for flags in 0..=u8::MAX {
            let colors = scoped_colors(flags, &key, &model, &scoped);
            if [16, 17].contains(&flags) {
                assert_eq!(colors.as_ref(), Some(&expected), "receiver flags {flags}");
            } else {
                assert!(colors.is_none(), "previously rejected flags {flags}");
            }
        }
        assert_eq!(model.meshes[0].source_vertex_indices, [1, 0]);
        let (key, model, bytes) = bake_fixture(false);
        let scoped = ScopedFile::parse(key.clone(), &key.hdr_name(), &bytes).unwrap();
        for flags in [0, 16, 17] {
            assert_eq!(
                scoped_colors(flags, &key, &model, &scoped),
                Some(expected.clone()),
            );
        }
        for flags in [2, 3, 18, 19] {
            assert!(scoped_colors(flags, &key, &model, &scoped).is_none(),
                "lighting-origin mode needs receiver evidence even for a single LOD: {flags}");
        }
    }

    #[test]
    fn lod0_receiver_still_requires_scope_raw_flags_equal_groups_and_zero_control() {
        let (key, model, bytes) = bake_fixture(true);
        let scoped = ScopedFile::parse(key.clone(), &key.hdr_name(), &bytes).unwrap();
        let wrong_bsp = InstanceKey::new(key.map().clone(), [8; 20], key.ordinal()).unwrap();
        assert!(scoped_colors(16, &wrong_bsp, &model, &scoped).is_none());
        for flags in [0u32, 1, 3, u32::MAX] {
            let mut corrupt = bytes.clone();
            corrupt[8..12].copy_from_slice(&flags.to_le_bytes());
            let scoped = ScopedFile::parse(key.clone(), &key.hdr_name(), &corrupt).unwrap();
            assert!(scoped_colors(16, &key, &model, &scoped).is_none());
        }
        for index in [96 + 12 + 4, 96 + 12 + 3] {
            let mut corrupt = bytes.clone();
            corrupt[index] += 1;
            let scoped = ScopedFile::parse(key.clone(), &key.hdr_name(), &corrupt).unwrap();
            assert_eq!(scoped_colors(16, &key, &model, &scoped), Some(vec![None]));
        }
        let mut corrupt = bytes;
        for index in [96 + 12 + 3, 96 + 12 + 7, 96 + 12 + 11] {
            corrupt[index] = 1;
        }
        let scoped = ScopedFile::parse(key.clone(), &key.hdr_name(), &corrupt).unwrap();
        assert_eq!(scoped_colors(16, &key, &model, &scoped), Some(vec![None]));
    }

    #[test]
    #[ignore = "requires read-only installed Legacy Inferno; no native process"]
    fn installed_lighting_origin_trim_retains_its_authored_point_without_admitting_bake() {
        let mut level = level::Level::load("de_inferno", None).unwrap();
        let (ordinal, prop) = level.bsp.static_props.props.iter().enumerate().find(|(_, prop)| {
            prop.flags == 3 && level.bsp.static_props.models[usize::from(prop.model)]
                .contains("/apartment_trims/")
        }).expect("installed lighting-origin apartment trim");
        let name = &level.bsp.static_props.models[usize::from(prop.model)];
        let base = name.trim_end_matches(".mdl");
        let model = mdl::Model::load(
            &level.vfs.read(name).unwrap(),
            &level.vfs.read(&format!("{base}.vvd")).unwrap(),
            &level.vfs.read(&format!("{base}.dx90.vtx")).unwrap(),
        ).unwrap();
        assert!(prop.lighting_origin.is_some_and(|point| point.is_finite()));
        for flags in [1, 2, 3, 4, 0x12, 0x13, 0x14, 0x20, 0x80, 0xff] {
            level.bsp.static_props.props[ordinal].flags = flags;
            assert!(super::load(&level, ordinal, &model).is_none(), "unsupported flags {flags}");
        }
    }

    #[test]
    #[ignore = "requires read-only installed Legacy Mirage; no native process"]
    fn installed_no_cast_shadow_cardboard_retains_authored_receiver_lighting() {
        let mut level = level::Level::load("de_mirage", None).unwrap();
        for (ordinal, suffix, checksum, lod0_count, lod1_count) in [
            (234, "/garbage_carboard002a.mdl", 1_195_020_340, 270, 130),
            (235, "/garbage_carboard001a.mdl", 887_594_147, 451, 162),
            (236, "/garbage_carboard001a.mdl", 887_594_147, 451, 162),
        ] {
            let prop = &level.bsp.static_props.props[ordinal];
            assert_eq!(prop.flags, 0x10);
            let name = &level.bsp.static_props.models[usize::from(prop.model)];
            assert!(
                name.ends_with(suffix),
                "installed ordinal {ordinal} identity changed",
            );
            let base = name.trim_end_matches(".mdl");
            let mdl = level.vfs.read(name).unwrap();
            let vvd = level.vfs.read(&format!("{base}.vvd")).unwrap();
            let vtx = level.vfs.read(&format!("{base}.dx90.vtx")).unwrap();
            for (data, offset) in [(&mdl, 8), (&vvd, 8), (&vtx, 16)] {
                assert_eq!(
                    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()),
                    checksum,
                );
            }
            let model = mdl::Model::load(&mdl, &vvd, &vtx).unwrap();
            assert_eq!(model.version, 49);
            assert_eq!(model.meshes.len(), 1);
            assert_eq!(model.meshes[0].vertices.len(), lod0_count as usize);
            let key = InstanceKey::new(
                level.id.clone(), *level.bsp_sha1(), ordinal as u32,
            ).unwrap();
            let bytes = level.vfs.read_from("map", &key.hdr_name()).unwrap();
            let scoped = ScopedFile::parse(key.clone(), &key.hdr_name(), &bytes).unwrap();
            assert_eq!(scoped.file().checksum, checksum);
            assert_eq!(scoped.file().raw_flags, 2);
            assert_eq!(scoped.file().vertex_count, lod0_count + lod1_count);
            assert_eq!(
                scoped.file().meshes().iter().map(|s| (s.lod, s.vertex_count)).collect::<Vec<_>>(),
                [(0, lod0_count), (1, lod1_count)],
            );
            assert!(
                scoped.candidate(&key, &model).is_err(),
                "strict full-file binding must reject both LODs",
            );
            assert_eq!(
                scoped.candidate_lod0(&key, &model).unwrap().matching_spans().len(),
                1,
            );
            let colors = super::load(&level, ordinal, &model)
                .expect("scoped equal-group cardboard LOD0 bake");
            assert_eq!(colors.len(), 1);
            assert_eq!(colors[0].as_ref().unwrap().len(), lod0_count as usize);
            assert_eq!(colors[0], decode(scoped.file().meshes()[0].opaque_bytes()));
            // No new multi-LOD admission for the already accepted flags0 mode.
            level.bsp.static_props.props[ordinal].flags = 0;
            assert!(super::load(&level, ordinal, &model).is_none());
            level.bsp.static_props.props[ordinal].flags = 0x10;
        }
    }

    #[test]
    fn separate_hardware_records_keep_their_channels_and_endpoints() {
        let bytes = [
            255, 128, 0, 0, 255, 128, 0, 0, 255, 128, 0, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255,
            0,
        ];
        let colors = decode(&bytes).unwrap();
        assert_eq!(colors.len(), 2);
        assert_eq!(colors[0], glam::Vec3::new(0.0, 128.0 / 255.0, 1.0));
        assert_eq!(colors[1], glam::Vec3::X);
    }

    #[test]
    fn unsupported_record_refuses_the_whole_mesh() {
        let valid = [17, 31, 47, 0, 17, 31, 47, 0, 17, 31, 47, 0];
        assert!(decode(&[]).is_none());
        assert!(decode(&valid[..11]).is_none());
        for index in [3, 4, 7, 8, 11] {
            let mut bytes = [valid, valid].concat();
            bytes[12 + index] += 1;
            assert!(
                decode(&bytes).is_none(),
                "accepted unsupported byte {index}"
            );
        }
        let control = [17, 31, 47, 1, 17, 31, 47, 1, 17, 31, 47, 1];
        assert!(decode(&control).is_none());
    }
}

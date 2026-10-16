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
    // Matched native observations cover only zero-flag instances. Other prop
    // lighting modes remain unsupported and must retain the existing fallback.
    if level.bsp.static_props.props.get(ordinal)?.flags != 0 {
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
    if scoped.file().raw_flags != 2 {
        return None;
    }
    let candidate = scoped.candidate(&key, model).ok()?;
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
    use super::decode;

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

//! Static props from the game lump (`sprp`).
//!
//! The game lump is a directory of sub-lumps whose offsets are absolute file
//! offsets. `sprp` holds a model-name dictionary, a leaf list, and one fixed
//! record per prop. The record size grows with the lump version, so the
//! stride is derived from the remaining byte count and only the leading
//! fields shared by every version are decoded (plus uniform scale when the
//! record is large enough to carry it). Diffuse modulation is exposed only
//! for the exact version/stride pairs observed in CS:GO Legacy maps, as is
//! the optional authored lighting origin.

use crate::lump::{self, Rec};
use glam::Vec3;
use std::io;

const SPRP: i32 = i32::from_be_bytes(*b"sprp");

#[derive(Clone, Debug)]
pub struct StaticProp {
    pub origin: Vec3,
    /// Pitch, yaw, roll in degrees (Source `QAngle`).
    pub angles: Vec3,
    pub model: u16,
    pub solid: u8,
    pub flags: u8,
    pub skin: i32,
    pub fade_min: f32,
    pub fade_max: f32,
    pub scale: f32,
    /// Explicit world-space non-vertex lighting sample point, only when flag
    /// 0x02 is set in a supported layout and all three coordinates are finite.
    /// This point is not transformed by the prop's origin, angles or scale.
    pub lighting_origin: Option<Vec3>,
    /// Authored RGBA bytes, without inferring a transfer function. Unsupported
    /// record layouts retain `None` rather than guessing from their length.
    pub diffuse_modulation: Option<[u8; 4]>,
}

#[derive(Clone, Debug, Default)]
pub struct StaticProps {
    pub version: u16,
    pub models: Vec<String>,
    pub props: Vec<StaticProp>,
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

pub fn parse(file: &[u8], game_lump: &[u8]) -> io::Result<StaticProps> {
    if game_lump.len() < 4 {
        return Ok(StaticProps::default());
    }
    let count = i32::from_le_bytes(game_lump[0..4].try_into().unwrap()).max(0) as usize;
    for i in 0..count {
        let o = 4 + i * 16;
        let Some(entry) = game_lump.get(o..o + 16) else {
            break;
        };
        let r = Rec(entry);
        if r.i32(0) != SPRP {
            continue;
        }
        let flags = r.u16(4);
        let version = r.u16(6);
        let ofs = r.i32(8).max(0) as usize;
        let len = r.i32(12).max(0) as usize;
        let raw = file
            .get(ofs..)
            .ok_or_else(|| invalid("sprp offset past end of file"))?;
        let data = if flags & 1 != 0 || raw.starts_with(b"LZMA") {
            lump::decompress_valve_lzma(raw)?
        } else {
            raw.get(..len)
                .ok_or_else(|| invalid("sprp runs past end of file"))?
                .to_vec()
        };
        return parse_sprp(&data, version);
    }
    Ok(StaticProps::default())
}

fn parse_sprp(d: &[u8], version: u16) -> io::Result<StaticProps> {
    let mut p = 0usize;
    let read_i32 = |p: &mut usize| -> io::Result<usize> {
        let b = d.get(*p..*p + 4).ok_or_else(|| invalid("truncated sprp"))?;
        *p += 4;
        Ok(i32::from_le_bytes(b.try_into().unwrap()).max(0) as usize)
    };
    let dict = read_i32(&mut p)?;
    let mut models = Vec::with_capacity(dict);
    for _ in 0..dict {
        let name = d
            .get(p..p + 128)
            .ok_or_else(|| invalid("truncated sprp dict"))?;
        let end = name.iter().position(|&b| b == 0).unwrap_or(128);
        models.push(String::from_utf8_lossy(&name[..end]).into_owned());
        p += 128;
    }
    let leaves = read_i32(&mut p)?;
    p += leaves * 2;
    let n = read_i32(&mut p)?;
    let mut props = Vec::with_capacity(n);
    if n > 0 {
        let rest = d.get(p..).ok_or_else(|| invalid("truncated sprp props"))?;
        let stride = rest.len() / n;
        if stride < 60 {
            return Err(invalid(format!("sprp record too small ({stride} bytes)")));
        }
        let has_instance_fields =
            rest.len() % n == 0 && matches!((version, stride), (10, 76) | (11, 80));
        for chunk in rest.chunks_exact(stride).take(n) {
            let r = Rec(chunk);
            props.push(StaticProp {
                origin: r.vec3(0),
                angles: r.vec3(12),
                model: r.u16(24),
                solid: r.u8(30),
                flags: r.u8(31),
                skin: r.i32(32),
                fade_min: r.f32(36),
                fade_max: r.f32(40),
                scale: if version >= 11 && stride >= 80 {
                    r.f32(76)
                } else {
                    1.0
                },
                lighting_origin: if has_instance_fields && r.u8(31) & 0x02 != 0 {
                    let point = r.vec3(44);
                    point.is_finite().then_some(point)
                } else {
                    None
                },
                diffuse_modulation: has_instance_fields.then(|| chunk[64..68].try_into().unwrap()),
            });
        }
    }
    Ok(StaticProps {
        version,
        models,
        props,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_sprp;

    // Synthetic records contain no game data. The dictionary/leaf arrays are
    // empty so that the records begin immediately after the three counts.
    fn fixture(records: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = vec![0; 8];
        bytes.extend_from_slice(&(records.len() as u32).to_le_bytes());
        for record in records {
            bytes.extend_from_slice(record);
        }
        bytes
    }

    #[test]
    fn observed_layouts_preserve_independent_instance_rgba() {
        for (version, stride) in [(10, 76), (11, 80)] {
            let mut blue = vec![0; stride];
            blue[64..68].copy_from_slice(&[0, 85, 255, 255]);
            let mut warm = vec![0; stride];
            warm[64..68].copy_from_slice(&[170, 1, 34, 129]);
            let props = parse_sprp(&fixture(&[blue, warm]), version).unwrap();
            assert_eq!(props.props.len(), 2);
            assert_eq!(props.props[0].diffuse_modulation, Some([0, 85, 255, 255]));
            assert_eq!(props.props[1].diffuse_modulation, Some([170, 1, 34, 129]));
        }
    }

    #[test]
    fn unobserved_or_ambiguous_layouts_do_not_invent_a_tint() {
        for (version, stride) in [(9, 72), (10, 80), (11, 76), (12, 80)] {
            let record = vec![255; stride];
            let props = parse_sprp(&fixture(&[record]), version).unwrap();
            assert_eq!(props.props[0].diffuse_modulation, None);
        }
        let mut ambiguous = fixture(&[vec![255; 76], vec![255; 76]]);
        ambiguous.push(0);
        let props = parse_sprp(&ambiguous, 10).unwrap();
        assert!(props
            .props
            .iter()
            .all(|prop| prop.diffuse_modulation.is_none()));
    }

    #[test]
    fn lighting_origin_is_flagged_finite_world_space_data_not_a_prop_transform() {
        for (version, stride) in [(10, 76), (11, 80)] {
            let mut record = vec![0; stride];
            // A translated, rotated and scaled prop samples the authored world
            // point directly, including a legitimate zero coordinate.
            for (offset, value) in [
                (0, 400.0_f32),
                (16, 90.0),
                (44, -32.5),
                (48, 0.0),
                (52, 228.25),
            ] {
                record[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
            if stride == 80 {
                record[76..80].copy_from_slice(&2.0_f32.to_le_bytes());
            }
            record[31] = 0x02 | 0x01;
            let parsed = parse_sprp(&fixture(&[record.clone()]), version).unwrap();
            assert_eq!(
                parsed.props[0].lighting_origin,
                Some(glam::Vec3::new(-32.5, 0.0, 228.25))
            );

            record[31] = 0x01;
            assert!(parse_sprp(&fixture(&[record.clone()]), version)
                .unwrap()
                .props[0]
                .lighting_origin
                .is_none());
            record[31] = 0x02;
            for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                record[48..52].copy_from_slice(&invalid.to_le_bytes());
                assert!(parse_sprp(&fixture(&[record.clone()]), version)
                    .unwrap()
                    .props[0]
                    .lighting_origin
                    .is_none());
            }
        }
        for (version, stride) in [(9, 72), (10, 80), (11, 76), (12, 80)] {
            let mut record = vec![0; stride];
            record[31] = 0x02;
            assert!(parse_sprp(&fixture(&[record]), version).unwrap().props[0]
                .lighting_origin
                .is_none());
        }
        let mut ambiguous = fixture(&[vec![0x02; 76], vec![0x02; 76]]);
        ambiguous.push(0);
        assert!(parse_sprp(&ambiguous, 10)
            .unwrap()
            .props
            .iter()
            .all(|p| p.lighting_origin.is_none()));
    }
}

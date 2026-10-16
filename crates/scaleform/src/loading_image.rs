//! The original loading movie's two observed image URL families. These are
//! explicit VFS lookups, never a general exception for parent paths.
use std::io::Cursor;

const MAX_SIDE: u32 = 4096;
const MAX_RGBA: usize = MAX_SIDE as usize * MAX_SIDE as usize * 4;
const MAX_INPUT: usize = 32 * 1024 * 1024;
const MAX_PNG: usize = MAX_RGBA + 1024 * 1024;

#[derive(Debug, PartialEq)]
pub(super) enum Request<'a> {
    Thumbnail(&'a str),
    Radar { map: &'a str, spectate: bool },
}

impl<'a> Request<'a> {
    pub fn parse(url: &'a str) -> Option<Self> {
        let request = if let Some(map) = url
            .strip_prefix("../../maps/")
            .and_then(|p| p.strip_suffix("_loading.jpg"))
        {
            Self::Thumbnail(map)
        } else {
            let path = url.strip_prefix("../overviews/")?;
            match path.strip_suffix("_radar_spectate.dds") {
                Some(map) => Self::Radar {
                    map,
                    spectate: true,
                },
                None => Self::Radar {
                    map: path.strip_suffix("_radar.dds")?,
                    spectate: false,
                },
            }
        };
        let map = match request {
            Self::Thumbnail(map) | Self::Radar { map, .. } => map,
        };
        (!map.is_empty()
            && map.len() <= 128
            && map.as_bytes().first()?.is_ascii_alphanumeric()
            && map.as_bytes().last()?.is_ascii_alphanumeric()
            && !map.contains("__")
            && map
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'))
        .then_some(request)
    }

    pub fn read(&self, fs: &vfs::Vfs) -> Option<Vec<u8>> {
        let (raw, dds) = match self {
            Self::Thumbnail(map) => {
                // A modern plain map JPG is not the optional authored loading
                // overview. Substituting it suppresses the movie's radar path.
                (fs.read(&format!("maps/{map}_loading.jpg"))?, false)
            }
            Self::Radar { map, spectate } => {
                let preferred = format!(
                    "resource/overviews/{map}_radar{}.dds",
                    if *spectate { "_spectate" } else { "" }
                );
                (
                    fs.read(&preferred).or_else(|| {
                        spectate
                            .then(|| fs.read(&format!("resource/overviews/{map}_radar.dds")))
                            .flatten()
                    })?,
                    true,
                )
            }
        };
        if raw.len() > MAX_INPUT {
            return None;
        }
        let image = if dds {
            decode_dds(&raw)?
        } else {
            let mut reader =
                image::ImageReader::with_format(Cursor::new(raw), image::ImageFormat::Jpeg);
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(MAX_SIDE);
            limits.max_image_height = Some(MAX_SIDE);
            limits.max_alloc = Some(MAX_RGBA as u64);
            reader.limits(limits);
            reader.decode().ok()?.into_rgba8()
        };
        if image.width() == 0
            || image.height() == 0
            || image.width() > MAX_SIDE
            || image.height() > MAX_SIDE
            || image.as_raw().len() > MAX_RGBA
        {
            return None;
        }
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).ok()?;
        let png = png.into_inner();
        (png.len() <= MAX_PNG).then_some(png)
    }
}

fn decode_dds(bytes: &[u8]) -> Option<image::RgbaImage> {
    if bytes.get(..4)? != b"DDS " {
        return None;
    }
    let number = |at| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    if number(4)? != 124 || number(76)? != 32 {
        return None;
    }
    let (w, h) = (number(16)?, number(12)?);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let format = match bytes.get(84..88)? {
        b"DXT1" => vtf::Format::Dxt1,
        b"DXT3" => vtf::Format::Dxt3,
        b"DXT5" => vtf::Format::Dxt5,
        _ => return None,
    };
    let raw = bytes.get(128..)?;
    let required = (w as usize).div_ceil(4) * (h as usize).div_ceil(4) * format.block_bytes()?;
    if raw.len() < required {
        return None;
    }
    image::RgbaImage::from_raw(
        w,
        h,
        vtf::decode_rgba8(format, raw, w as usize, h as usize)?,
    )
}

pub(super) fn radar_png(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() > MAX_INPUT { return None; }
    let image = decode_dds(bytes)?;
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).ok()?;
    let png = png.into_inner();
    (png.len() <= MAX_PNG).then_some(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, sync::Arc};

    #[test]
    fn only_observed_canonical_map_image_families_are_admitted() {
        assert_eq!(
            Request::parse("../../maps/de_dust2_loading.jpg"),
            Some(Request::Thumbnail("de_dust2"))
        );
        assert_eq!(
            Request::parse("../overviews/de_dust2_radar_spectate.dds"),
            Some(Request::Radar {
                map: "de_dust2",
                spectate: true
            })
        );
        for path in [
            "../../maps/../config_loading.jpg",
            "../../maps/C:_loading.jpg",
            "../../maps/de_dust2.jpg",
            "../icons/de_dust2_radar.dds",
            "../overviews/de_dust2%2f_radar.dds",
            "../overviews/_radar.dds",
            "../overviews/DE_DUST2_radar.dds",
            "../overviews/de__dust2_radar.dds",
            "../overviews/de_dust2__radar.dds",
        ] {
            assert!(Request::parse(path).is_none(), "{path}");
        }
        assert!(Request::parse(&format!("../../maps/{}_loading.jpg", "a".repeat(129))).is_none());
    }

    fn jpeg(pixel: [u8; 3]) -> Vec<u8> {
        let image = image::RgbImage::from_pixel(4, 4, image::Rgb(pixel));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    }

    #[test]
    fn thumbnail_requires_the_exact_loading_asset_and_rejects_corruption() {
        let mut fs = vfs::Vfs::new();
        fs.push_back(
            "fallback",
            vfs::Mount::Memory(BTreeMap::from([(
                "maps/de_dust2.jpg".into(),
                Arc::<[u8]>::from(jpeg([200, 40, 20])),
            )])),
        );
        let request = Request::Thumbnail("de_dust2");
        assert!(request.read(&fs).is_none());
        fs.push_front(
            "exact loading image",
            vfs::Mount::Memory(BTreeMap::from([(
                "maps/de_dust2_loading.jpg".into(),
                Arc::<[u8]>::from(jpeg([20, 40, 200])),
            )])),
        );
        let png = request.read(&fs).unwrap();
        assert_eq!(image::load_from_memory(&png).unwrap().width(), 4);
        fs.push_front(
            "bad preferred",
            vfs::Mount::Memory(BTreeMap::from([(
                "maps/de_dust2_loading.jpg".into(),
                Arc::<[u8]>::from(vec![0; 32]),
            )])),
        );
        assert!(request.read(&fs).is_none());
    }

    #[test]
    fn radar_fallback_decodes_real_dds_layout_and_rejects_truncation_and_bounds() {
        let mut dds = vec![0u8; 136];
        dds[..4].copy_from_slice(b"DDS ");
        for (at, value) in [(4, 124u32), (12, 4), (16, 4), (76, 32)] {
            dds[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        dds[84..88].copy_from_slice(b"DXT1");
        let mut fs = vfs::Vfs::new();
        fs.push_back(
            "radar",
            vfs::Mount::Memory(BTreeMap::from([(
                "resource/overviews/de_dust2_radar.dds".into(),
                Arc::<[u8]>::from(dds.clone()),
            )])),
        );
        assert!(
            Request::Radar {
                map: "de_dust2",
                spectate: true
            }
            .read(&fs)
            .is_some()
        );
        assert!(decode_dds(&dds[..135]).is_none());
        dds[16..20].copy_from_slice(&4097u32.to_le_bytes());
        assert!(decode_dds(&dds).is_none());
    }
}

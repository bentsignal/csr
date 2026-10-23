//! Add missing runtime bitmap exports without inspecting movie instructions.
use anyhow::{Result, ensure};
use ruffle_core::swf;
use crate::rewrite::{push_tag, split_tags};

pub(crate) fn complete(movie: &[u8], icons: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let movie = swf::decompress_swf(movie)?;
    let tags = split_tags(&movie.data)?;
    let mut used = std::collections::HashSet::from([0]);
    let mut names = std::collections::HashSet::new();
    crate::rewrite::symbol_metadata(&movie.data, &mut used, &mut names)?;
    let mut additions = Vec::new();
    for (name, png) in icons {
        ensure!(matches!(name.as_str(), "icon-revolver.png" | "icon-cz75a.png" | "icon-mp5sd.png"), "unsupported death icon");
        if !names.insert(name.clone()) { continue; }
        ensure!(png.len() <= 1024 * 1024, "death icon byte limit");
        let reader = image::ImageReader::with_format(std::io::Cursor::new(png), image::ImageFormat::Png);
        let (width, height) = reader.into_dimensions()?;
        ensure!(width > 0 && height > 0 && width <= 256 && height <= 256, "death icon size limit");
        let id = (1..=u16::MAX).find(|id| !used.contains(id)).ok_or_else(|| anyhow::anyhow!("no bitmap ID"))?;
        used.insert(id);
        let mut bitmap = id.to_le_bytes().to_vec();
        bitmap.extend_from_slice(png);
        // SWF DefineBitsJPEG2 also supports PNG, preserving straight alpha.
        push_tag(&mut additions, 21, &bitmap);
        let mut export = 1u16.to_le_bytes().to_vec();
        export.extend_from_slice(&id.to_le_bytes());
        export.extend_from_slice(name.as_bytes());
        export.push(0);
        push_tag(&mut additions, 56, &export);
    }
    // Export before frame-one actions can initialize the authored notice root.
    let mut result = additions;
    for tag in tags { result.extend_from_slice(tag.whole); }
    let mut header = movie.header.swf_header().clone();
    header.compression = swf::Compression::None;
    header.version = header.version.max(8);
    let mut output = Vec::new();
    swf::write::write_swf_raw_tags(&header, &result, &mut output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_export_is_added_once_without_replacing_authored_tags() {
        let mut original = Vec::new();
        push_tag(&mut original, 21, &[1, 0, 9, 8]);
        // Import ID 2 is reserved even though it has no local Define tag.
        push_tag(&mut original, 57, b"library.swf\0\x01\x00\x02\x00imported\0");
        push_tag(&mut original, 1, &[]);
        push_tag(&mut original, 0, &[]);
        let mut movie = Vec::new();
        swf::write::write_swf_raw_tags(&swf::Header::default_with_swf_version(10), &original, &mut movie).unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::write_buffer_with_format(&mut png, &[255, 255, 255, 128], 1, 1,
            image::ExtendedColorType::Rgba8, image::ImageFormat::Png).unwrap();
        let icons = vec![("icon-revolver.png".into(), png.into_inner())];
        let completed = complete(&movie, &icons).unwrap();
        let body = swf::decompress_swf(completed.as_slice()).unwrap();
        assert!(body.data.ends_with(&original));
        assert_eq!(&split_tags(&body.data).unwrap()[0].body[..2], &3u16.to_le_bytes());
        assert!(crate::library_symbols(&completed).unwrap().contains("icon-revolver.png"));
        assert_eq!(complete(&completed, &icons).unwrap(), completed);
    }
}

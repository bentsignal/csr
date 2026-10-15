//! Completes one observed Scaleform dynamic-font subset at runtime. All
//! authored instructions, existing glyph bytes/indices and layout stay intact.
use std::collections::HashSet;

use anyhow::{Context, Result, ensure};
use ruffle_core::swf;

use crate::rewrite::{push_tag, split_tags};

struct Glyph<'a> {
    code: u16,
    shape: &'a [u8],
    advance: &'a [u8],
    bounds: &'a [u8],
}

struct Font<'a> {
    header: &'a [u8], // id, flags, language, name
    glyphs: Vec<Glyph<'a>>,
    metrics: &'a [u8], // ascent, descent, leading
    kerning: Vec<&'a [u8]>,
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    let b = bytes.get(at..at + 2).context("truncated font integer")?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

impl<'a> Font<'a> {
    fn parse(body: &'a [u8]) -> Result<Self> {
        ensure!(body.len() >= 7, "truncated font");
        let flags = body[2];
        // Only Font3 with explicit layout and Unicode code table is admitted.
        ensure!(flags & 0x84 == 0x84, "font has no Unicode layout");
        let start = 5 + body[4] as usize;
        let count = u16_at(body, start)? as usize;
        let table = start + 2;
        let offset_size = if flags & 8 != 0 { 4 } else { 2 };
        let offset = |index: usize| -> Result<usize> {
            let at = table + index * offset_size;
            if offset_size == 4 {
                let b: [u8; 4] = body.get(at..at + 4).context("truncated font offset")?.try_into()?;
                Ok(u32::from_le_bytes(b) as usize)
            } else { Ok(u16_at(body, at)? as usize) }
        };
        let codes = table.checked_add(offset(count)?).context("font offset overflow")?;
        let layout = codes + count * 2;
        let metrics = body.get(layout..layout + 6).context("truncated font metrics")?;
        let advances = layout + 6;
        let mut bounds_at = advances + count * 2;
        let mut glyphs = Vec::with_capacity(count);
        for index in 0..count {
            let from = table + offset(index)?;
            let to = table + offset(index + 1)?;
            let shape = body.get(from..to).context("invalid glyph shape range")?;
            let bits = body.get(bounds_at).context("missing glyph bounds")? >> 3;
            let bounds_len = (5 + 4 * bits as usize).div_ceil(8);
            let bounds = body.get(bounds_at..bounds_at + bounds_len).context("truncated glyph bounds")?;
            bounds_at += bounds_len;
            glyphs.push(Glyph {
                code: u16_at(body, codes + index * 2)?, shape,
                advance: body.get(advances + index * 2..advances + index * 2 + 2).context("truncated glyph advance")?,
                bounds,
            });
        }
        let kern_count = u16_at(body, bounds_at)? as usize;
        let kern_at = bounds_at + 2;
        ensure!(body.len() == kern_at + kern_count * 6, "unexpected font layout tail");
        let kerning = body[kern_at..].chunks_exact(6).collect();
        Ok(Self { header: &body[..start], glyphs, metrics, kerning })
    }

    fn matches(&self, id: u16, flags: u8, codes: Option<&[u16]>) -> bool {
        self.header[0..2] == id.to_le_bytes() && self.header[2] == flags
            && self.header[5..].strip_suffix(&[0]).unwrap_or(&self.header[5..]) == b"Stratum2 Regular"
            && codes.is_none_or(|codes| self.glyphs.iter().map(|g| g.code).eq(codes.iter().copied()))
    }
}

fn zones(body: &[u8], count: usize) -> Result<Vec<&[u8]>> {
    ensure!(body.len() == 3 + count * 10, "font alignment count mismatch");
    let rows: Vec<_> = body[3..].chunks_exact(10).collect();
    ensure!(rows.iter().all(|row| row[0] == 2), "unsupported font alignment record");
    Ok(rows)
}

/// Called only for `message-box.swf`. This exact installed subset has eleven
/// placeholder glyphs; Ruffle otherwise drops the localized question's letters.
/// Append the installed same-family font's missing glyphs, without replacing
/// any existing outline, metric, code index, kerning pair or alignment record.
pub(crate) fn complete(movie: &[u8], library: &[u8]) -> Result<Vec<u8>> {
    let movie = swf::decompress_swf(movie)?;
    let tags = split_tags(&movie.data)?;
    let Some(target) = tags.iter().find(|tag| tag.code == 75 && tag.body.starts_with(&7u16.to_le_bytes())) else {
        return unchanged(&movie);
    };
    let old = Font::parse(target.body)?;
    let expected: Vec<_> = "#DTaegilotx".encode_utf16().collect();
    if !old.matches(7, 133, Some(&expected)) { return unchanged(&movie); }
    let library = swf::decompress_swf(library)?;
    let source_tags = split_tags(&library.data)?;
    let source_tag = source_tags.iter().find(|tag| tag.code == 75 && tag.body.starts_with(&12u16.to_le_bytes()))
        .context("installed Stratum2 Regular font missing")?;
    let source = Font::parse(source_tag.body)?;
    ensure!(source.matches(12, 140, None) && source.glyphs.len() == 1077, "unrecognized installed regular font");
    let old_codes: HashSet<_> = old.glyphs.iter().map(|g| g.code).collect();
    let added: Vec<_> = source.glyphs.iter().enumerate().filter(|(_, g)| !old_codes.contains(&g.code)).collect();
    let glyphs: Vec<_> = old.glyphs.iter().chain(added.iter().map(|(_, g)| *g)).collect();
    let old_zone_tag = tags.iter().find(|tag| tag.code == 73 && tag.body.starts_with(&7u16.to_le_bytes())).context("subset alignment missing")?;
    let source_zone_tag = source_tags.iter().find(|tag| tag.code == 73 && tag.body.starts_with(&12u16.to_le_bytes())).context("regular alignment missing")?;
    let old_zones = zones(old_zone_tag.body, old.glyphs.len())?;
    let source_zones = zones(source_zone_tag.body, source.glyphs.len())?;
    let mut merged_zones = old_zone_tag.body[..3].to_vec();
    for record in old_zones.into_iter().chain(added.iter().map(|(index, _)| source_zones[*index])) { merged_zones.extend_from_slice(record); }

    let mut merged = old.header.to_vec();
    merged[2] |= 8; // Our offset table is explicitly wide, independent of serializer heuristics.
    merged.extend_from_slice(&(glyphs.len() as u16).to_le_bytes());
    let mut offset = (glyphs.len() + 1) * 4;
    for glyph in &glyphs {
        merged.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += glyph.shape.len();
    }
    merged.extend_from_slice(&(offset as u32).to_le_bytes());
    for glyph in &glyphs { merged.extend_from_slice(glyph.shape); }
    for glyph in &glyphs { merged.extend_from_slice(&glyph.code.to_le_bytes()); }
    merged.extend_from_slice(old.metrics);
    for glyph in &glyphs { merged.extend_from_slice(glyph.advance); }
    for glyph in &glyphs { merged.extend_from_slice(glyph.bounds); }
    let mut kerning = old.kerning.clone();
    for pair in source.kerning {
        if !old_codes.contains(&u16_at(pair, 0)?) || !old_codes.contains(&u16_at(pair, 2)?) { kerning.push(pair); }
    }
    merged.extend_from_slice(&(kerning.len() as u16).to_le_bytes());
    for pair in kerning { merged.extend_from_slice(pair); }
    let mut result = Vec::new();
    for tag in &tags {
        if std::ptr::eq(tag.body, target.body) { push_tag(&mut result, 75, &merged); }
        else if std::ptr::eq(tag.body, old_zone_tag.body) { push_tag(&mut result, 73, &merged_zones); }
        else { result.extend_from_slice(tag.whole); }
    }
    let mut header = movie.header.swf_header().clone();
    header.compression = swf::Compression::None;
    let mut output = Vec::new();
    swf::write::write_swf_raw_tags(&header, &result, &mut output)?;
    tracing::info!("scaleform: completed observed MessageBox regular subset ({} original glyphs preserved, {} added)", old.glyphs.len(), added.len());
    Ok(output)
}

fn unchanged(movie: &swf::SwfBuf) -> Result<Vec<u8>> {
    let mut header = movie.header.swf_header().clone();
    header.compression = swf::Compression::None;
    let mut output = Vec::new();
    swf::write::write_swf_raw_tags(&header, &movie.data, &mut output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie(tags: &[u8]) -> Vec<u8> {
        let header = swf::Header {
            compression: swf::Compression::None, version: 9,
            stage_size: swf::Rectangle::default(), frame_rate: swf::Fixed8::from_f32(30.0), num_frames: 1,
        };
        let mut out = Vec::new();
        swf::write::write_swf_raw_tags(&header, tags, &mut out).unwrap();
        out
    }

    fn fixture(id: u16, flags: u8, codes: &[u16]) -> Vec<u8> {
        let mut font = id.to_le_bytes().to_vec();
        font.extend_from_slice(&[flags, 0, 16]);
        font.extend_from_slice(b"Stratum2 Regular");
        font.extend_from_slice(&(codes.len() as u16).to_le_bytes());
        let size = if flags & 8 != 0 { 4 } else { 2 };
        for index in 0..=codes.len() {
            let offset = (codes.len() + 1) * size + index * 2;
            if size == 4 { font.extend_from_slice(&(offset as u32).to_le_bytes()); }
            else { font.extend_from_slice(&(offset as u16).to_le_bytes()); }
        }
        for _ in codes { font.extend_from_slice(&[0x10, 0]); }
        for code in codes { font.extend_from_slice(&code.to_le_bytes()); }
        font.extend_from_slice(&[1, 0, 2, 0, 3, 0]);
        for code in codes { font.extend_from_slice(&code.to_le_bytes()); }
        font.extend(std::iter::repeat_n(0, codes.len())); // zero-bit bounds
        font.extend_from_slice(&0u16.to_le_bytes());
        let mut align = id.to_le_bytes().to_vec();
        align.push(0);
        for _ in codes { align.extend_from_slice(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 3]); }
        let mut tags = Vec::new();
        push_tag(&mut tags, 75, &font);
        push_tag(&mut tags, 73, &align);
        // Unrelated opaque instructions must remain byte-for-byte untouched.
        push_tag(&mut tags, 12, &[0x04, 0x07, 0x00]);
        push_tag(&mut tags, 0, &[]);
        movie(&tags)
    }

    fn check_preserved(original: &[u8], library: &[u8]) -> Vec<u8> {
        let output = complete(original, library).unwrap();
        let before = swf::decompress_swf(original).unwrap();
        let after = swf::decompress_swf(&output[..]).unwrap();
        let before_tags = split_tags(&before.data).unwrap();
        let after_tags = split_tags(&after.data).unwrap();
        assert_eq!(before_tags.len(), after_tags.len());
        for (before, after) in before_tags.iter().zip(&after_tags) {
            if before.code == 75 && before.body.starts_with(&7u16.to_le_bytes()) {
                let old = Font::parse(before.body).unwrap();
                let new = Font::parse(after.body).unwrap();
                assert!(new.glyphs.len() > old.glyphs.len());
                assert_eq!(old.metrics, new.metrics);
                assert_eq!(&old.header[..2], &new.header[..2]);
                assert_eq!(&old.header[3..], &new.header[3..]);
                for (a, b) in old.glyphs.iter().zip(&new.glyphs) {
                    assert_eq!((a.code, a.shape, a.advance, a.bounds), (b.code, b.shape, b.advance, b.bounds));
                }
                assert!(new.kerning.starts_with(&old.kerning));
                // Independent SWF reader validates offsets, codes, shapes and metrics.
                let decoded = swf::read::Reader::new(after.body, 9).read_define_font_2(3).unwrap();
                assert_eq!(decoded.glyphs.len(), new.glyphs.len());
            } else if before.code == 73 && before.body.starts_with(&7u16.to_le_bytes()) {
                assert!(after.body.starts_with(before.body));
            } else { assert_eq!(before.whole, after.whole); }
        }
        assert_eq!(complete(&output, library).unwrap(), output);
        output
    }

    #[test]
    fn completion_preserves_indices_raw_outlines_metrics_alignment_and_instructions() {
        let original = fixture(7, 133, &"#DTaegilotx".encode_utf16().collect::<Vec<_>>());
        let library = fixture(12, 140, &(0..1077).collect::<Vec<_>>());
        check_preserved(&original, &library);
        let unrelated = fixture(7, 133, &[65, 66]);
        assert_eq!(complete(&unrelated, &library).unwrap(), unrelated);
    }

    #[test]
    fn installed_messagebox_completion_preserves_existing_font_data() {
        let game = std::env::var_os("CSR_GAME_DIR").map(std::path::PathBuf::from)
            .unwrap_or_else(|| r"C:\Program Files (x86)\Steam\steamapps\common\csgo legacy\csgo".into());
        let flash = game.join("resource/flash");
        let Ok(original) = std::fs::read(flash.join("message-box.swf")) else { return };
        let library = std::fs::read(flash.join("fontlib.swf")).unwrap();
        check_preserved(&original, &library);
    }
}

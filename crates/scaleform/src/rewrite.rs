//! Load-time transforms that stand in for Scaleform features Ruffle lacks.
//! They run in memory on the player's own files; nothing is written to disk.
//!
//! - **Translator.** Scaleform replaced any text starting with `#TOKEN` with the
//!   localized string. We do the same when a movie is loaded: in text field
//!   initial text and in ActionScript constant pools (the menu assigns
//!   `htmlText = "#SFUI_..."` hundreds of times).
//! - **Font library.** Movies import `$TextFont`, `$TextFontBold` and
//!   `$BigButtonFont` from `gfxfontlib.swf`, a virtual file Scaleform resolved
//!   through `fontmapping.cfg` to fonts inside `fontlib.swf`. We build that
//!   movie in memory from the real fonts.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use ruffle_core::swf;

pub struct Localization {
    tokens: HashMap<String, String>,
}

impl Localization {
    /// `resource/csgo_<language>.txt` (UTF-16 LE KeyValues).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let body = bytes.strip_prefix(&[0xFF, 0xFE]).context("expected UTF-16 LE")?;
        let units: Vec<u16> = body.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let text = String::from_utf16_lossy(&units);
        let doc = kv::parse_escaped(&text).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let mut tokens = HashMap::new();
        if let Some(t) = doc.block("lang").and_then(|l| l.block("Tokens")) {
            for (k, v) in t.iter() {
                if let Some(s) = v.as_str() {
                    tokens.insert(k.to_ascii_lowercase(), s.to_string());
                }
            }
        }
        Ok(Self { tokens })
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn get(&self, token: &str) -> Option<&str> {
        let key = token.trim_start_matches('#').to_ascii_lowercase();
        self.tokens
            .get(&key)
            // The 2023 file renamed a few 2018 tokens with a "_1" suffix
            // (SFUI_Elevated_Status_Confirmed -> SFUI_Elevated_Status_Confirmed_1).
            .or_else(|| self.tokens.get(&format!("{key}_1")))
            .map(String::as_str)
    }

    /// Translates a whole-string token (`#SFUI_X`), as Scaleform's translator did.
    pub fn translate_whole(&self, s: &str) -> Option<&str> {
        let t = s.strip_prefix('#')?;
        if t.is_empty() || !t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        self.get(t)
    }

    /// Replaces every known `#TOKEN` inside a string (used for HTML text).
    pub fn translate_embedded(&self, s: &str, html: bool) -> Option<String> {
        if !s.contains('#') {
            return None;
        }
        let mut out = String::with_capacity(s.len());
        let mut changed = false;
        let mut rest = s;
        while let Some(pos) = rest.find('#') {
            out.push_str(&rest[..pos]);
            let after = &rest[pos + 1..];
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(after.len());
            let token = &after[..end];
            match (!token.is_empty()).then(|| self.get(token)).flatten() {
                Some(text) => {
                    if html {
                        out.push_str(&text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"));
                    } else {
                        out.push_str(text);
                    }
                    changed = true;
                    rest = &after[end..];
                }
                None => {
                    out.push('#');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        changed.then_some(out)
    }
}

#[derive(Default, Debug)]
pub struct RewriteStats {
    pub edit_texts: usize,
    pub pool_strings: usize,
    pub imports_merged: usize,
}

/// Returns an uncompressed copy of `data` with tokens translated.
pub fn translate_movie(data: &[u8], loc: &Localization, stats: &mut RewriteStats) -> Result<Vec<u8>> {
    let buf = swf::decompress_swf(data)?;
    let version = buf.header.version();
    let tags = rewrite_tags(&buf.data, version, loc, stats)?;
    let tags = merge_imports(&tags, stats)?;
    let mut header = buf.header.swf_header().clone();
    header.compression = swf::Compression::None;
    let mut out = Vec::new();
    swf::write::write_swf_raw_tags(&header, &tags, &mut out)?;
    Ok(out)
}

/// Merges every ImportAssets tag that names the same movie into the first one.
/// The movies import from `sharedlib.swf` (7 MB) up to seven times each, and
/// Ruffle loads the whole library once per tag. Imports only bind names to
/// character IDs, used when a symbol is placed, so merging keeps behaviour.
fn merge_imports(data: &[u8], stats: &mut RewriteStats) -> Result<Vec<u8>> {
    const IMPORT_ASSETS: u16 = 57;
    const IMPORT_ASSETS2: u16 = 71;
    let tags = split_tags(data)?;
    // url (lower-case) -> (original url bytes, entries)
    let mut groups: Vec<(String, Vec<u8>, Vec<u8>, u16)> = Vec::new();
    for tag in &tags {
        if tag.code != IMPORT_ASSETS && tag.code != IMPORT_ASSETS2 {
            continue;
        }
        let Some(nul) = tag.body.iter().position(|&b| b == 0) else { continue };
        let url = &tag.body[..nul];
        let mut rest = &tag.body[nul + 1..];
        if tag.code == IMPORT_ASSETS2 {
            rest = &rest[2.min(rest.len())..];
        }
        if rest.len() < 2 {
            continue;
        }
        let count = u16::from_le_bytes([rest[0], rest[1]]);
        let key = String::from_utf8_lossy(url).to_ascii_lowercase();
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => {
                g.2.extend_from_slice(&rest[2..]);
                g.3 += count;
                stats.imports_merged += 1;
            }
            None => groups.push((key, url.to_vec(), rest[2..].to_vec(), count)),
        }
    }
    let mut emitted = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(data.len());
    for tag in &tags {
        if tag.code != IMPORT_ASSETS && tag.code != IMPORT_ASSETS2 {
            out.extend_from_slice(tag.whole);
            continue;
        }
        let nul = tag.body.iter().position(|&b| b == 0).unwrap_or(0);
        let key = String::from_utf8_lossy(&tag.body[..nul]).to_ascii_lowercase();
        let Some(group) = groups.iter().find(|g| g.0 == key) else {
            out.extend_from_slice(tag.whole);
            continue;
        };
        if !emitted.insert(key) {
            continue;
        }
        let mut body = group.1.clone();
        body.push(0);
        body.extend_from_slice(&group.3.to_le_bytes());
        body.extend_from_slice(&group.2);
        push_tag(&mut out, IMPORT_ASSETS, &body);
    }
    Ok(out)
}

/// Scaleform made shared-library exports available to dynamic attachMovie
/// calls even when the authoring tool omitted a static import. Bind only
/// the requested missing names, with collision-free local character IDs.
/// This changes public symbol metadata; movie instructions remain untouched.
pub(crate) fn import_symbols(data: &[u8], library: &str, names: &[&str]) -> Result<Vec<u8>> {
    let buf = swf::decompress_swf(data)?;
    let mut used = std::collections::HashSet::new();
    let mut imported = std::collections::HashSet::new();
    symbol_metadata(&buf.data, &mut used, &mut imported)?;
    let missing: Vec<_> = names.iter().filter(|name| !imported.contains(**name)).collect();
    if missing.is_empty() { return Ok(data.to_vec()); }
    let mut next = used.iter().copied().max().unwrap_or(0);
    let mut body = library.as_bytes().to_vec();
    body.push(0);
    body.extend_from_slice(&u16::try_from(missing.len())?.to_le_bytes());
    for name in missing {
        next = next.checked_add(1).context("SWF character ID space exhausted")?;
        body.extend_from_slice(&next.to_le_bytes());
        body.extend_from_slice(name.as_bytes());
        body.push(0);
    }
    let mut tags = Vec::new();
    push_tag(&mut tags, 57, &body);
    tags.extend_from_slice(&buf.data);
    let tags = merge_imports(&tags, &mut RewriteStats::default())?;
    let mut header = buf.header.swf_header().clone();
    header.compression = swf::Compression::None;
    let mut out = Vec::new();
    swf::write::write_swf_raw_tags(&header, &tags, &mut out)?;
    Ok(out)
}

pub(crate) fn symbol_metadata(data: &[u8], used: &mut std::collections::HashSet<u16>, imported: &mut std::collections::HashSet<String>) -> Result<()> {
    for tag in split_tags(data)? {
        // Public SWF Define* records begin with their character ID.
        if matches!(tag.code, 2 | 6 | 7 | 10 | 11 | 14 | 20 | 21 | 22 | 32 | 33 | 34 | 35 | 36 | 37 | 39 | 46 | 48 | 60 | 75 | 83 | 84 | 87 | 90 | 91) {
            if tag.body.len() < 2 { bail!("truncated symbol definition"); }
            used.insert(u16::from_le_bytes([tag.body[0], tag.body[1]]));
            if tag.code == 39 {
                if tag.body.len() < 4 { bail!("truncated sprite definition"); }
                symbol_metadata(&tag.body[4..], used, imported)?;
            }
        }
        if matches!(tag.code, 56 | 57 | 71) {
            let mut body = tag.body;
            if tag.code != 56 {
                let nul = body.iter().position(|&b| b == 0).context("truncated import URL")?;
                body = &body[nul + 1..];
                if tag.code == 71 { body = body.get(2..).context("truncated import flags")?; }
            }
            if body.len() < 2 { bail!("truncated symbol count"); }
            let count = u16::from_le_bytes([body[0], body[1]]);
            body = &body[2..];
            for _ in 0..count {
                if body.len() < 2 { bail!("truncated symbol ID"); }
                used.insert(u16::from_le_bytes([body[0], body[1]]));
                body = &body[2..];
                let nul = body.iter().position(|&b| b == 0).context("truncated symbol name")?;
                imported.insert(String::from_utf8_lossy(&body[..nul]).into_owned());
                body = &body[nul + 1..];
            }
        }
    }
    Ok(())
}

/// Reads only the public export/import name tables of an installed library.
pub(crate) fn symbol_names(data: &[u8]) -> Result<std::collections::HashSet<String>> {
    let buf = swf::decompress_swf(data)?;
    let mut names = std::collections::HashSet::new();
    symbol_metadata(&buf.data, &mut std::collections::HashSet::new(), &mut names)?;
    Ok(names)
}

pub(crate) struct RawTag<'a> {
    pub(crate) code: u16,
    pub(crate) body: &'a [u8],
    /// Header plus body, as it appeared in the file.
    pub(crate) whole: &'a [u8],
}

pub(crate) fn split_tags(mut data: &[u8]) -> Result<Vec<RawTag<'_>>> {
    let mut tags = Vec::new();
    while data.len() >= 2 {
        let start = data;
        let v = u16::from_le_bytes([data[0], data[1]]);
        let code = v >> 6;
        let mut len = (v & 0x3f) as usize;
        let mut header = 2;
        if len == 0x3f {
            if data.len() < 6 {
                bail!("truncated long tag header");
            }
            len = u32::from_le_bytes([data[2], data[3], data[4], data[5]]) as usize;
            header = 6;
        }
        if data.len() < header + len {
            bail!("truncated tag {code}");
        }
        tags.push(RawTag { code, body: &data[header..header + len], whole: &start[..header + len] });
        data = &data[header + len..];
        if code == 0 {
            break;
        }
    }
    Ok(tags)
}

pub(crate) fn push_tag(out: &mut Vec<u8>, code: u16, body: &[u8]) {
    out.extend_from_slice(&((code << 6) | 0x3f).to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
}

const DO_ACTION: u16 = 12;
const DEFINE_SPRITE: u16 = 39;
const DEFINE_EDIT_TEXT: u16 = 37;
const DO_INIT_ACTION: u16 = 59;

fn rewrite_tags(data: &[u8], version: u8, loc: &Localization, stats: &mut RewriteStats) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    for tag in split_tags(data)? {
        match tag.code {
            DEFINE_SPRITE if tag.body.len() >= 4 => {
                let mut body = tag.body[..4].to_vec();
                body.extend(rewrite_tags(&tag.body[4..], version, loc, stats)?);
                push_tag(&mut out, tag.code, &body);
            }
            DO_ACTION => match rewrite_actions(tag.body, version, loc, stats)? {
                Some(body) => push_tag(&mut out, tag.code, &body),
                None => out.extend_from_slice(tag.whole),
            },
            DO_INIT_ACTION if tag.body.len() >= 2 => {
                match rewrite_actions(&tag.body[2..], version, loc, stats)? {
                    Some(actions) => {
                        let mut body = tag.body[..2].to_vec();
                        body.extend(actions);
                        push_tag(&mut out, tag.code, &body);
                    }
                    None => out.extend_from_slice(tag.whole),
                }
            }
            DEFINE_EDIT_TEXT => match rewrite_edit_text(tag.whole, version, loc)? {
                Some(bytes) => {
                    stats.edit_texts += 1;
                    out.extend(bytes);
                }
                None => out.extend_from_slice(tag.whole),
            },
            _ => out.extend_from_slice(tag.whole),
        }
    }
    Ok(out)
}

/// Rewrites the constant pool at the start of an action block. Compilers put
/// the pool first, so no jump crosses it and changing its length is safe.
fn rewrite_actions(
    actions: &[u8],
    version: u8,
    loc: &Localization,
    stats: &mut RewriteStats,
) -> Result<Option<Vec<u8>>> {
    use swf::avm1::types::Action;
    let mut reader = swf::avm1::read::Reader::new(actions, version);
    let Ok(Action::ConstantPool(pool)) = reader.read_action() else {
        return Ok(None);
    };
    let rest = *reader.get_mut();
    let mut changed = false;
    let translated: Vec<Vec<u8>> = pool
        .strings
        .iter()
        .map(|s| {
            let text = s.to_str_lossy(swf::UTF_8);
            match loc.translate_whole(&text) {
                Some(t) => {
                    changed = true;
                    stats.pool_strings += 1;
                    t.as_bytes().to_vec()
                }
                None => s.as_bytes().to_vec(),
            }
        })
        .collect();
    if !changed {
        return Ok(None);
    }
    let strings: Vec<&swf::SwfStr> = translated.iter().map(|b| swf::SwfStr::from_bytes(b)).collect();
    let mut out = Vec::new();
    swf::avm1::write::Writer::new(&mut out, version)
        .write_action(&Action::ConstantPool(swf::avm1::types::ConstantPool { strings }))?;
    out.extend_from_slice(rest);
    Ok(Some(out))
}

fn rewrite_edit_text(whole: &[u8], version: u8, loc: &Localization) -> Result<Option<Vec<u8>>> {
    let mut reader = swf::read::Reader::new(whole, version);
    let swf::Tag::DefineEditText(text) = reader.read_tag()? else {
        return Ok(None);
    };
    let Some(initial) = text.initial_text() else {
        return Ok(None);
    };
    let html = text.flags().contains(swf::EditTextFlag::HTML);
    let Some(new_text) = loc.translate_embedded(&initial.to_str_lossy(swf::UTF_8), html) else {
        return Ok(None);
    };
    let new_text = swf::SwfStr::from_utf8_str(&new_text);
    let tag = swf::Tag::DefineEditText(Box::new((*text).clone().with_initial_text(Some(new_text))));
    Ok(Some(encode_tag(&tag, version)?))
}

/// Serializes one tag (the swf crate only exposes whole-movie writing).
fn encode_tag(tag: &swf::Tag, version: u8) -> Result<Vec<u8>> {
    let header = swf::Header {
        compression: swf::Compression::None,
        version,
        stage_size: swf::Rectangle::default(),
        frame_rate: swf::Fixed8::from_f32(30.0),
        num_frames: 1,
    };
    let mut empty = Vec::new();
    swf::write_swf(&header, &[], &mut empty)?;
    let mut full = Vec::new();
    swf::write_swf(&header, std::slice::from_ref(tag), &mut full)?;
    Ok(full[empty.len() - 2..full.len() - 2].to_vec())
}

/// Builds the virtual `gfxfontlib.swf`: each alias from `fontmapping.cfg`
/// becomes a copy of the mapped font from `fontlib.swf`, renamed to the alias
/// and exported under it.
pub fn build_gfxfontlib(fontlib: &[u8], fontmapping: &str) -> Result<Vec<u8>> {
    const DEFINE_FONT3: u16 = 75;
    const DEFINE_FONT_ALIGN_ZONES: u16 = 73;
    const EXPORT_ASSETS: u16 = 56;

    let buf = swf::decompress_swf(fontlib)?;
    let tags = split_tags(&buf.data)?;
    // name -> (DefineFont3 body, align zones body)
    let mut fonts: HashMap<String, (&[u8], Option<&[u8]>)> = HashMap::new();
    let mut by_id: HashMap<u16, String> = HashMap::new();
    for tag in &tags {
        if tag.code == DEFINE_FONT3 && tag.body.len() > 5 {
            let id = u16::from_le_bytes([tag.body[0], tag.body[1]]);
            let name_len = tag.body[4] as usize;
            let name = String::from_utf8_lossy(&tag.body[5..5 + name_len])
                .trim_end_matches('\0')
                .to_string();
            // Keep the copy with glyphs (fontlib has empty bold/italic Arial stubs).
            let glyphs = u16::from_le_bytes([tag.body[5 + name_len], tag.body[6 + name_len]]);
            if glyphs > 0 {
                fonts.insert(name.clone(), (tag.body, None));
                by_id.insert(id, name);
            }
        } else if tag.code == DEFINE_FONT_ALIGN_ZONES && tag.body.len() > 2 {
            let id = u16::from_le_bytes([tag.body[0], tag.body[1]]);
            if let Some(name) = by_id.get(&id) {
                if let Some(entry) = fonts.get_mut(name) {
                    entry.1 = Some(tag.body);
                }
            }
        }
    }

    let doc = kv::parse(fontmapping).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let mapping = doc.block("english").context("fontmapping.cfg: no english block")?;
    let mut out = Vec::new();
    let mut exports: Vec<(u16, String)> = Vec::new();
    let mut next_id = 1u16;
    for (alias, value) in mapping.iter() {
        let Some(block) = value.as_block() else { continue };
        let Some(font) = block.str("font") else { continue };
        // Only the Stratum2 aliases are imported by the movies ($TextFont,
        // $TextFontBold, $BigButtonFont). "Arial Unicode MS" (54k glyphs) costs
        // ~0.4 s per import in Ruffle, and "PF DinText Pro" ($BodyText) is not
        // in fontlib.swf at all, so those aliases are left out.
        let Some((body, zones)) = fonts.get(font).filter(|_| font != "Arial Unicode MS") else {
            tracing::debug!("fontlib: skipping {alias} ({font})");
            continue;
        };
        let id = next_id;
        next_id += 1;
        let old_name_len = body[4] as usize;
        let mut new_body = Vec::with_capacity(body.len() + alias.len());
        new_body.extend_from_slice(&id.to_le_bytes());
        new_body.extend_from_slice(&body[2..4]); // flags, language
        new_body.push(alias.len() as u8);
        new_body.extend_from_slice(alias.as_bytes());
        new_body.extend_from_slice(&body[5 + old_name_len..]);
        push_tag(&mut out, DEFINE_FONT3, &new_body);
        if let Some(zones) = zones {
            let mut z = id.to_le_bytes().to_vec();
            z.extend_from_slice(&zones[2..]);
            push_tag(&mut out, DEFINE_FONT_ALIGN_ZONES, &z);
        }
        exports.push((id, alias.to_string()));
    }
    let mut export = (exports.len() as u16).to_le_bytes().to_vec();
    for (id, name) in &exports {
        export.extend_from_slice(&id.to_le_bytes());
        export.extend_from_slice(name.as_bytes());
        export.push(0);
    }
    push_tag(&mut out, EXPORT_ASSETS, &export);
    push_tag(&mut out, 1, &[]); // ShowFrame
    push_tag(&mut out, 0, &[]); // End
    let header = swf::Header {
        compression: swf::Compression::None,
        version: buf.header.version(),
        stage_size: buf.header.stage_size().clone(),
        frame_rate: swf::Fixed8::from_f32(30.0),
        num_frames: 1,
    };
    let mut swf_bytes = Vec::new();
    swf::write::write_swf_raw_tags(&header, &out, &mut swf_bytes)?;
    tracing::info!("gfxfontlib: {:?}", exports.iter().map(|e| &e.1).collect::<Vec<_>>());
    Ok(swf_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dynamic_imports_preserve_existing_ids_and_movie_instructions() {
        let mut tags = Vec::new();
        let mut sprite = 500u16.to_le_bytes().to_vec();
        sprite.extend_from_slice(&0u16.to_le_bytes());
        push_tag(&mut sprite, 0, &[]);
        push_tag(&mut tags, 39, &sprite);
        let actions = [0x04, 0x07, 0x00];
        push_tag(&mut tags, 12, &actions);
        let mut imported = b"sharedlib.swf\0".to_vec();
        imported.extend_from_slice(&1u16.to_le_bytes());
        imported.extend_from_slice(&800u16.to_le_bytes());
        imported.extend_from_slice(b"existing\0");
        push_tag(&mut tags, 57, &imported);
        push_tag(&mut tags, 0, &[]);
        let mut movie = Vec::new();
        swf::write::write_swf_raw_tags(&swf::Header::default_with_swf_version(10), &tags, &mut movie).unwrap();
        let rewritten = import_symbols(&movie, "sharedlib.swf", &["existing", "slider"]).unwrap();
        let body = swf::decompress_swf(rewritten.as_slice()).unwrap();
        let tags = split_tags(&body.data).unwrap();
        assert_eq!(tags.iter().filter(|tag| tag.code == 57).count(), 1);
        assert_eq!(tags.iter().find(|tag| tag.code == 12).unwrap().body, actions);
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        symbol_metadata(&body.data, &mut ids, &mut names).unwrap();
        assert_eq!(ids, std::collections::HashSet::from([500, 800, 801]));
        assert_eq!(names, std::collections::HashSet::from(["existing".into(), "slider".into()]));
        assert_eq!(import_symbols(&rewritten, "sharedlib.swf", &["slider"]).unwrap(), rewritten);
    }
}

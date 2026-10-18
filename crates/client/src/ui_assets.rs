//! Original interface art and fonts, read from the user's installation at runtime.
use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use std::collections::HashMap;

#[derive(Resource)]
pub struct UiAssets {
    pub regular: Handle<Font>,
    pub bold: Handle<Font>,
    pub images: HashMap<String, Handle<Image>>,
}
impl UiAssets {
    pub fn image(&self, key: &str) -> Option<Handle<Image>> { self.images.get(key).cloned() }
}

/// VFONT1 wraps an sfnt font with a salted byte stream. Decode in memory only.
fn decode_font(mut bytes: Vec<u8>) -> Option<Vec<u8>> {
    let trailer = bytes.len().checked_sub(7)?;
    if &bytes[trailer + 1..] != b"VFONT1" { return None; }
    let salt_len = bytes[trailer] as usize;
    if salt_len == 0 { return None; }
    let end = (trailer + 1).checked_sub(salt_len)?;
    let mut key = 0xa7u8;
    for &salt in &bytes[end..trailer] { key ^= salt.wrapping_add(0xa7); }
    for b in &mut bytes[..end] {
        let encoded = *b;
        *b ^= key;
        key = encoded.wrapping_add(0xa7);
    }
    bytes.truncate(end);
    if !bytes.starts_with(&[0, 1, 0, 0]) && !bytes.starts_with(b"OTTO") { return None; }
    Some(bytes)
}

const EQUIPMENT_SVG_BYTES: usize = 64 * 1024;
const EQUIPMENT_RASTER_EDGE: u32 = 256;

/// A deliberately limited filled-silhouette decoder, not a general SVG loader.
/// XML/SVG parsing belongs to usvg; the preflight prevents reference expansion,
/// external reads and expensive effects before usvg resolves the document.
fn equipment_svg(bytes: &[u8]) -> Option<Image> {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    use resvg::{tiny_skia, usvg};
    if bytes.len() > EQUIPMENT_SVG_BYTES { return None; }
    let text = std::str::from_utf8(bytes).ok()?;
    let doc = usvg::roxmltree::Document::parse_with_options(text, usvg::roxmltree::ParsingOptions {
        allow_dtd: false, nodes_limit: 1024, ..default()
    }).ok()?;
    if doc.root_element().tag_name().name() != "svg" { return None; }
    for node in doc.descendants().filter(|node| node.is_element()) {
        if node.ancestors().count() > 16
            || !matches!(node.tag_name().name(), "svg" | "g" | "path" | "rect" | "circle" | "ellipse" | "polygon" | "polyline" | "line" | "style" | "title" | "desc")
            || (node.tag_name().name() == "svg" && node != doc.root_element())
            || node.attributes().any(|attribute| attribute.name() == "href") {
            return None;
        }
    }
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..default()
    };
    let tree = usvg::Tree::from_xmltree(&doc, &options).ok()?;
    let size = tree.size();
    if [size.width(), size.height()].iter().any(|value| !value.is_finite() || *value < 1.0 / 64.0 || *value > 4096.0)
        || tree.has_defs_nodes() { return None; }
    fn bounded_rect(rect: usvg::Rect) -> bool {
        [rect.left(), rect.top(), rect.right(), rect.bottom()].iter()
            .all(|value| value.is_finite() && value.abs() <= 16384.0)
    }
    fn bounded_paths(group: &usvg::Group, segments: &mut usize) -> bool {
        group.children().iter().all(|node| match node {
            usvg::Node::Group(group) => bounded_paths(group, segments),
            usvg::Node::Path(path) => {
                *segments += path.data().segments().count();
                *segments <= 8192 && path.stroke().is_none()
                    && bounded_rect(path.data().bounds())
                    && bounded_rect(path.abs_stroke_bounding_box())
            }
            _ => false,
        })
    }
    if !bounded_paths(tree.root(), &mut 0) { return None; }
    let scale = EQUIPMENT_RASTER_EDGE as f32 / size.width().max(size.height());
    let width = (size.width() * scale).ceil().clamp(1.0, EQUIPMENT_RASTER_EDGE as f32) as u32;
    let height = (size.height() * scale).ceil().clamp(1.0, EQUIPMENT_RASTER_EDGE as f32) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let rgba = pixmap.take_demultiplied();
    if !rgba.chunks_exact(4).any(|pixel| pixel[3] != 0) { return None; }
    // tiny-skia demultiplies its sRGB output for Bevy's straight-alpha UI.
    let mut image = Image::new(Extent3d { width, height, depth_or_array_layers: 1 }, TextureDimension::D2,
        rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    image.sampler = ImageSampler::linear();
    Some(image)
}

fn equipment_image(fs: &vfs::Vfs, name: &str) -> Option<Image> {
    if name.is_empty() || name.len() > 64 || !name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_') { return None; }
    let bytes = fs.read_limited(&format!("materials/panorama/images/icons/equipment/{name}.svg"), EQUIPMENT_SVG_BYTES)?;
    equipment_svg(&bytes)
}

fn add_equipment_fallback(fs: &vfs::Vfs, name: &str, art: &mut HashMap<String, Handle<Image>>, images: &mut Assets<Image>) {
    if !art.contains_key(name) {
        if let Some(image) = equipment_image(fs, name) { art.insert(name.to_string(), images.add(image)); }
    }
}

pub fn load(mut commands: Commands, level: Res<crate::GameData>, mut fonts: ResMut<Assets<Font>>, mut images: ResMut<Assets<Image>>) {
    let fs = &level.vfs;
    let mut font = |name: &str| {
        fs.read(&format!("panorama/fonts/{name}.vfont")).and_then(decode_font).map(|bytes| fonts.add(Font::from_bytes(bytes))).unwrap_or_else(|| {
            warn!("Missing or invalid original font: {name}"); Handle::default()
        })
    };
    let regular = font("stratum2regular");
    let bold = font("stratum2bold");
    let mut art = HashMap::new();
    let mut png = |key: &str, path: &str| {
        if let Some(bytes) = fs.read(path) {
            if let Ok(img) = Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::linear(), RenderAssetUsages::RENDER_WORLD) {
                art.insert(key.to_string(), images.add(img));
            }
        }
    };
    for (key, path) in [
        ("health", "healtharmor/icon-cross1"), ("armor", "healtharmor/icon-shield"),
        ("helmet", "healtharmor/icon-armor-helmet"), ("cart", "money/icon-cart"),
        ("c4", "weaponpanel/icon-c4"), ("defuser", "weaponpanel/icon-defuser"),
        ("avatar", "teamcounter/teamcounter_botavatar"),
        ("ct", "winpanel/ct_patch"), ("t", "winpanel/t_patch"),
        ("logo", "freezepanel/logo"), ("radar-player", "radar/icon-player-indicator"),
        ("radar-ct", "radar/icon-ct-on-map"), ("radar-t", "radar/icon-t-on-map"),
    ] { png(key, &format!("materials/panorama/images/hud/{path}.png")); }
    for (side, model) in [("ct", "ctm_sas"), ("t", "tm_phoenix")] {
        png(&format!("character-{side}"), &format!("resource/flash/econ/characters/customplayer_{model}.png"));
    }
    png("knife_default_t", "materials/panorama/images/hud/deathnotice/icon-knife_default_t.png");
    for weapon in &level.catalog.weapons {
        let name = weapon.class_name.trim_start_matches("weapon_");
        png(name, &format!("materials/panorama/images/hud/deathnotice/icon-{name}.png"));
        png(&format!("item-{name}"), &format!("resource/flash/econ/weapons/base_weapons/weapon_{name}.png"));
    }
    for path in fs.archive_paths_with_prefix("materials/panorama/images/map_icons/screenshots/360p/") {
        if let Some(map)=path.rsplit('/').next().and_then(|p|p.strip_suffix(".png")) { png(&format!("map-{map}"), &path); }
    }
    png("menu-background", "materials/panorama/images/map_icons/screenshots/1080p/de_dust2.png");
    // Preserve every successfully decoded PNG. Missing class art is rasterized
    // once from the selected VFS, kept only in memory and reused by the HUD.
    for weapon in &level.catalog.weapons {
        let name = weapon.class_name.trim_start_matches("weapon_");
        add_equipment_fallback(fs, name, &mut art, &mut images);
    }
    info!("Original UI: Stratum regular/bold, {} images", art.len());
    commands.insert_resource(UiAssets { regular, bold, images: art });
}

/// Covers text spawned later by menus, too. Runs before Bevy's text layout.
pub fn fonts(assets: Res<UiAssets>, mut text: Query<&mut TextFont, Added<TextFont>>) {
    for mut font in &mut text {
        let bold = match font.font_size {
            FontSize::Px(size) => size >= 24.0,
            // HUD typography uses physical viewport-height units, keeping its
            // original font-weight choice through resolution/DPI changes.
            FontSize::Vh(size) => size >= 24.0 * scaleform::HUD_SCALING as f32 / 1080.0 * 100.0,
            _ => false,
        };
        font.font = bevy::text::FontSource::Handle(if bold { assets.bold.clone() } else { assets.regular.clone() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Original synthetic geometry only; no installed/public game asset fixture.
    const TOY_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="32" viewBox="0 0 16 32"><path fill="white" fill-opacity="0.5" d="M4 4H12V28H4Z"/></svg>"#;

    #[test]
    fn equipment_silhouette_preserves_aspect_alpha_and_selected_vfs() {
        let mut fs = vfs::Vfs::new();
        let path = "materials/panorama/images/icons/equipment/fixture_tool.svg";
        fs.push_back("fixture", vfs::Mount::Memory([(path.into(), std::sync::Arc::from(TOY_SVG))].into()));
        let image = equipment_image(&fs, "fixture_tool").unwrap();
        assert_eq!((image.width(), image.height()), (128, 256));
        assert!(matches!(image.sampler, ImageSampler::Descriptor(ref sampler) if sampler.mag_filter == bevy::image::ImageFilterMode::Linear && sampler.min_filter == bevy::image::ImageFilterMode::Linear));
        let data = image.data.unwrap();
        assert_eq!(&data[..4], &[0, 0, 0, 0]);
        let center = (128 * 128 + 64) * 4;
        assert_eq!(&data[center..center+3], &[255, 255, 255], "unpremultiply the authored color");
        assert!((127..=128).contains(&data[center+3]));
        let mut images = Assets::<Image>::default();
        let mut art = HashMap::new();
        add_equipment_fallback(&fs, "fixture_tool", &mut art, &mut images);
        let first = art["fixture_tool"].clone();
        add_equipment_fallback(&fs, "fixture_tool", &mut art, &mut images);
        assert_eq!(art["fixture_tool"], first, "cache once; also retain a valid PNG handle");
        let png = images.add(Image::default());
        art.insert("fixture_tool".into(), png.clone());
        add_equipment_fallback(&fs, "fixture_tool", &mut art, &mut images);
        assert_eq!(art["fixture_tool"], png, "a preloaded PNG has priority over the SVG");
        fs.push_front("selected override", vfs::Mount::Memory([(path.into(), std::sync::Arc::from(b"invalid".as_slice()))].into()));
        assert!(equipment_image(&fs, "fixture_tool").is_none(), "do not bypass the selected VFS override");
        for name in ["../fixture_tool", "fixture_tool.png", "", "a/b"] { assert!(equipment_image(&fs, name).is_none()); }
    }

    #[test]
    fn equipment_svg_rejects_unbounded_or_unsupported_content() {
        assert!(equipment_svg(&vec![b' '; EQUIPMENT_SVG_BYTES + 1]).is_none());
        for svg in [
            "", "<svg>", "<svg xmlns='http://www.w3.org/2000/svg'/>",
            "<!DOCTYPE svg [<!ENTITY shape 'x'>]><svg>&shape;</svg>",
            "<svg xmlns='http://www.w3.org/2000/svg'><use href='#a'/></svg>",
            "<svg xmlns='http://www.w3.org/2000/svg'><image href='file:///fixture.png'/></svg>",
            "<svg xmlns='http://www.w3.org/2000/svg'><text>fixture</text></svg>",
            "<svg xmlns='http://www.w3.org/2000/svg'><filter id='a'/></svg>",
            "<svg xmlns='http://www.w3.org/2000/svg' width='999999' height='32'><rect width='16' height='32'/></svg>",
            "<svg xmlns='http://www.w3.org/2000/svg'><path d='M0 0H999999V32H0Z'/></svg>",
            "<svg xmlns='http://www.w3.org/2000/svg'><path stroke='white' d='M0 0H16V32H0Z'/></svg>",
        ] { assert!(equipment_svg(svg.as_bytes()).is_none(), "{svg}"); }
        let nested = format!("<svg xmlns='http://www.w3.org/2000/svg'>{}<rect width='16' height='32'/>{}</svg>", "<g>".repeat(17), "</g>".repeat(17));
        assert!(equipment_svg(nested.as_bytes()).is_none());
        let many_nodes = format!("<svg xmlns='http://www.w3.org/2000/svg'>{}</svg>", "<rect width='1' height='1'/>".repeat(1100));
        assert!(equipment_svg(many_nodes.as_bytes()).is_none());
        let many_segments = format!("<svg xmlns='http://www.w3.org/2000/svg'><path d='M0 0{}Z'/></svg>", "h1v1".repeat(4200));
        assert!(equipment_svg(many_segments.as_bytes()).is_none());
    }

    #[test]
    fn decodes_salted_font_and_rejects_bad_trailers() {
        let data = b"\0\x01\0\0testfont";
        let salts = [5u8, 27];
        let mut key = salts.iter().fold(0xa7u8, |key, s| key ^ s.wrapping_add(0xa7));
        let mut encoded = Vec::new();
        for &b in data { let value = b ^ key; encoded.push(value); key = value.wrapping_add(0xa7); }
        encoded.extend(salts); encoded.push(3); encoded.extend(b"VFONT1");
        assert_eq!(decode_font(encoded.clone()).as_deref(), Some(data.as_slice()));
        for n in 0..encoded.len() { assert!(decode_font(encoded[..n].to_vec()).is_none()); }
        let at = encoded.len() - 7; encoded[at] = 255;
        assert!(decode_font(encoded).is_none());
    }
}

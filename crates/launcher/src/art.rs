//! CSGO Rewrite's own artwork, drawn in code: the app icon and the Steam
//! library images for the "Add to Steam" shortcut.
//!
//! Everything here is original (a crosshair mark and a stroke-drawn
//! wordmark); no Valve images or fonts are used, and no image files are
//! committed (`tools/check-no-assets.sh`). `build.rs` includes this file to
//! embed the icon in the exe, so it depends on `std` and `png` only.

#![allow(dead_code)]

type Rgba = [f32; 4];

const INK: Rgba = [0.95, 0.95, 0.94, 1.0];
const AMBER: Rgba = [0.93, 0.64, 0.22, 1.0];
const BG_TOP: Rgba = [0.15, 0.19, 0.25, 1.0];
const BG_BOTTOM: Rgba = [0.05, 0.07, 0.10, 1.0];

/// A straight-alpha RGBA image with channels in 0..=1.
pub struct Image {
    pub width: u32,
    pub height: u32,
    px: Vec<Rgba>,
}

impl Image {
    fn new(width: u32, height: u32) -> Self {
        Image { width, height, px: vec![[0.0; 4]; (width * height) as usize] }
    }

    /// Paints `color` over pixel (x, y) with the given coverage.
    fn blend(&mut self, x: u32, y: u32, color: Rgba, coverage: f32) {
        let a = color[3] * coverage.clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let p = &mut self.px[(y * self.width + x) as usize];
        let out_a = a + p[3] * (1.0 - a);
        for c in 0..3 {
            p[c] = if out_a > 0.0 { (color[c] * a + p[c] * p[3] * (1.0 - a)) / out_a } else { 0.0 };
        }
        p[3] = out_a;
    }

    /// Paints every pixel whose centre gets coverage from `shape`
    /// (a signed distance in pixels: negative inside).
    fn fill(&mut self, color: impl Fn(f32, f32) -> Rgba, bounds: [f32; 4], shape: impl Fn(f32, f32) -> f32) {
        let x0 = bounds[0].floor().max(0.0) as u32;
        let y0 = bounds[1].floor().max(0.0) as u32;
        let x1 = (bounds[2].ceil().max(0.0) as u32).min(self.width);
        let y1 = (bounds[3].ceil().max(0.0) as u32).min(self.height);
        for y in y0..y1 {
            for x in x0..x1 {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let coverage = 0.5 - shape(fx, fy);
                if coverage > 0.0 {
                    self.blend(x, y, color(fx, fy), coverage);
                }
            }
        }
    }

    fn all(&self) -> [f32; 4] {
        [0.0, 0.0, self.width as f32, self.height as f32]
    }

    /// RGBA8 PNG bytes.
    pub fn png(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.px.len() * 4);
        for p in &self.px {
            for c in p {
                data.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("PNG header");
        writer.write_image_data(&data).expect("PNG data");
        writer.finish().expect("PNG end");
        out
    }
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

fn with_alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a]
}

/// Distance from (px, py) to the segment a-b.
fn segment_distance(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (cx, cy) = (a.0 + dx * t - px, a.1 + dy * t - py);
    (cx * cx + cy * cy).sqrt()
}

/// The dark backdrop shared by every image: a vertical gradient with faint
/// diagonal bands.
fn backdrop(img: &mut Image) {
    let h = img.height as f32;
    let band = (img.width.max(img.height) as f32 / 9.0).max(8.0);
    img.fill(
        |x, y| {
            let base = mix(BG_TOP, BG_BOTTOM, y / h);
            let stripe = (((x + y) / band).floor() as i32).rem_euclid(2) as f32;
            mix(base, [1.0, 1.0, 1.0, 1.0], stripe * 0.018)
        },
        img.all(),
        |_, _| -1.0,
    );
}

/// The crosshair mark: a ring, four gapped arms and a centre dot.
fn crosshair(img: &mut Image, cx: f32, cy: f32, radius: f32, color: Rgba) {
    let stroke = (radius * 0.16).max(1.4);
    let ring = radius * 0.72;
    let bounds = [cx - radius - stroke, cy - radius - stroke, cx + radius + stroke, cy + radius + stroke];
    let gap = radius * 0.30;
    let arms = [
        ((cx, cy - radius), (cx, cy - gap)),
        ((cx, cy + gap), (cx, cy + radius)),
        ((cx - radius, cy), (cx - gap, cy)),
        ((cx + gap, cy), (cx + radius, cy)),
    ];
    let dot = (radius * 0.11).max(1.0);
    img.fill(
        |_, _| color,
        bounds,
        |x, y| {
            let r = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            let mut d = (r - ring).abs() - stroke / 2.0;
            for (a, b) in arms {
                d = d.min(segment_distance(x, y, a, b) - stroke / 2.0);
            }
            d.min(r - dot)
        },
    );
}

/// Stroke glyphs on a grid 6 units tall (y down): polylines and the
/// glyph's width in units.
fn glyph(c: char) -> (&'static [&'static [(f32, f32)]], f32) {
    match c {
        'C' => (&[&[(4.0, 0.0), (1.0, 0.0), (0.0, 1.0), (0.0, 5.0), (1.0, 6.0), (4.0, 6.0)]], 4.0),
        'S' => (
            &[&[(4.0, 0.0), (1.0, 0.0), (0.0, 1.0), (0.0, 2.0), (1.0, 3.0), (3.0, 3.0), (4.0, 4.0), (4.0, 5.0), (3.0, 6.0), (0.0, 6.0)]],
            4.0,
        ),
        'G' => (
            &[&[(4.0, 1.0), (3.0, 0.0), (1.0, 0.0), (0.0, 1.0), (0.0, 5.0), (1.0, 6.0), (3.0, 6.0), (4.0, 5.0), (4.0, 3.2), (2.4, 3.2)]],
            4.0,
        ),
        'O' => (
            &[&[(1.0, 0.0), (3.0, 0.0), (4.0, 1.0), (4.0, 5.0), (3.0, 6.0), (1.0, 6.0), (0.0, 5.0), (0.0, 1.0), (1.0, 0.0)]],
            4.0,
        ),
        'R' => (
            &[&[(0.0, 6.0), (0.0, 0.0), (3.0, 0.0), (4.0, 1.0), (4.0, 2.0), (3.0, 3.0), (0.0, 3.0)], &[(2.2, 3.0), (4.0, 6.0)]],
            4.0,
        ),
        'E' => (&[&[(4.0, 0.0), (0.0, 0.0), (0.0, 6.0), (4.0, 6.0)], &[(0.0, 3.0), (3.2, 3.0)]], 4.0),
        'W' => (&[&[(0.0, 0.0), (1.2, 6.0), (2.5, 2.2), (3.8, 6.0), (5.0, 0.0)]], 5.0),
        'I' => (&[&[(0.0, 0.0), (0.0, 6.0)]], 0.0),
        'T' => (&[&[(0.0, 0.0), (4.0, 0.0)], &[(2.0, 0.0), (2.0, 6.0)]], 4.0),
        _ => (&[], 2.0),
    }
}

const GLYPH_GAP: f32 = 1.7;

/// Width in pixels of `text` drawn `height` pixels tall (stroke centre lines).
fn text_width(text: &str, height: f32, tracking: f32) -> f32 {
    let unit = height / 6.0;
    let n = text.chars().count();
    let units: f32 = text.chars().map(|c| glyph(c).1).sum();
    (units + (GLYPH_GAP + tracking) * n.saturating_sub(1) as f32) * unit
}

/// Draws `text` with its stroke centre lines spanning (x, y)..(x + width,
/// y + height).
fn text(img: &mut Image, s: &str, x: f32, y: f32, height: f32, stroke: f32, tracking: f32, color: Rgba) {
    let unit = height / 6.0;
    let mut segments = Vec::new();
    let mut pen = x;
    for c in s.chars() {
        let (lines, width) = glyph(c);
        for line in lines {
            for pair in line.windows(2) {
                segments.push(((pen + pair[0].0 * unit, y + pair[0].1 * unit), (pen + pair[1].0 * unit, y + pair[1].1 * unit)));
            }
        }
        pen += (width + GLYPH_GAP + tracking) * unit;
    }
    let pad = stroke;
    let bounds = [x - pad, y - pad, pen + pad, y + height + pad];
    img.fill(
        |_, _| color,
        bounds,
        |px, py| segments.iter().map(|&(a, b)| segment_distance(px, py, a, b)).fold(f32::MAX, f32::min) - stroke / 2.0,
    );
}

/// "CSGO" over an amber rule and "REWRITE", centred on `cx`, top at `top`.
fn wordmark(img: &mut Image, cx: f32, top: f32, height: f32) {
    let stroke = height * 0.15;
    let w = text_width("CSGO", height, 0.4);
    text(img, "CSGO", cx - w / 2.0, top, height, stroke, 0.4, INK);
    let rule_y = top + height * 1.32;
    let rule_w = w + stroke;
    let rule_h = (height * 0.05).max(2.0);
    img.fill(|_, _| AMBER, [cx - rule_w / 2.0, rule_y, cx + rule_w / 2.0, rule_y + rule_h], |_, _| -1.0);
    let small = height * 0.36;
    let tracking = 1.6;
    let sw = text_width("REWRITE", small, tracking);
    text(img, "REWRITE", cx - sw / 2.0, rule_y + rule_h + height * 0.3, small, small * 0.16, tracking, AMBER);
}

/// The app icon: an amber crosshair on a rounded dark tile.
pub fn icon(size: u32) -> Image {
    let mut img = Image::new(size, size);
    let s = size as f32;
    let radius = s * 0.2;
    let half = s / 2.0 - s * 0.02;
    let c = s / 2.0;
    img.fill(
        |_, y| mix(BG_TOP, BG_BOTTOM, y / s),
        img.all(),
        |x, y| {
            // Rounded square.
            let qx = (x - c).abs() - (half - radius);
            let qy = (y - c).abs() - (half - radius);
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
            outside + qx.max(qy).min(0.0) - radius
        },
    );
    crosshair(&mut img, c, c, s * 0.33, AMBER);
    img
}

/// A Windows .ico holding PNG-compressed images (Windows Vista and later).
pub fn ico() -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = ICON_SIZES.iter().map(|&s| (s, icon(s).png())).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in &images {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &images {
        out.extend_from_slice(data);
    }
    out
}

pub const ICON_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

/// Library capsule (portrait), 600x900.
pub fn portrait() -> Image {
    let mut img = Image::new(600, 900);
    backdrop(&mut img);
    crosshair(&mut img, 300.0, 250.0, 120.0, with_alpha(AMBER, 0.9));
    wordmark(&mut img, 300.0, 470.0, 120.0);
    img
}

/// Wide capsule (recent games, Big Picture), 920x430.
pub fn landscape() -> Image {
    let mut img = Image::new(920, 430);
    backdrop(&mut img);
    crosshair(&mut img, 220.0, 215.0, 120.0, with_alpha(AMBER, 0.9));
    wordmark(&mut img, 590.0, 110.0, 110.0);
    img
}

/// Library hero banner (Steam draws the logo over it), 1920x620.
pub fn hero() -> Image {
    let mut img = Image::new(1920, 620);
    backdrop(&mut img);
    crosshair(&mut img, 1500.0, 310.0, 250.0, with_alpha(AMBER, 0.22));
    img
}

/// Transparent logo for the hero banner, 1000x420.
pub fn logo() -> Image {
    let mut img = Image::new(1000, 420);
    wordmark(&mut img, 500.0, 40.0, 150.0);
    img
}

/// Installer splash, 600x400.
pub fn splash() -> Image {
    let mut img = Image::new(600, 400);
    backdrop(&mut img);
    crosshair(&mut img, 300.0, 130.0, 80.0, with_alpha(AMBER, 0.9));
    wordmark(&mut img, 300.0, 250.0, 56.0);
    img
}

/// Steam grid file suffixes (`<appid><suffix>.png`) and their images.
pub fn steam_library() -> Vec<(&'static str, Image)> {
    vec![("p", portrait()), ("", landscape()), ("_hero", hero()), ("_logo", logo()), ("_icon", icon(256))]
}

/// A 32-bit Windows `.res` file with the icon as icon group 1, which the MSVC
/// linker accepts as an input to embed it in the exe.
pub fn icon_res() -> Vec<u8> {
    const RT_ICON: u16 = 3;
    const RT_GROUP_ICON: u16 = 14;
    fn entry(out: &mut Vec<u8>, kind: u16, id: u16, flags: u16, data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&32u32.to_le_bytes());
        for v in [0xFFFF, kind, 0xFFFF, id] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes()); // data version
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&0x0409u16.to_le_bytes()); // en-US
        out.extend_from_slice(&0u32.to_le_bytes()); // version
        out.extend_from_slice(&0u32.to_le_bytes()); // characteristics
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    let mut out = Vec::new();
    entry(&mut out, 0, 0, 0, &[]);
    let mut group = vec![0, 0, 1, 0];
    group.extend_from_slice(&(ICON_SIZES.len() as u16).to_le_bytes());
    for (i, &size) in ICON_SIZES.iter().enumerate() {
        let data = icon(size).png();
        let id = i as u16 + 1;
        entry(&mut out, RT_ICON, id, 0x1010, &data);
        let dim = if size >= 256 { 0 } else { size as u8 };
        group.extend_from_slice(&[dim, dim, 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&32u16.to_le_bytes());
        group.extend_from_slice(&(data.len() as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    entry(&mut out, RT_GROUP_ICON, 1, 0x1030, &group);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_size(bytes: &[u8]) -> (u32, u32) {
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        (u32::from_be_bytes(bytes[16..20].try_into().unwrap()), u32::from_be_bytes(bytes[20..24].try_into().unwrap()))
    }

    #[test]
    fn steam_images_have_steams_sizes() {
        let sizes: Vec<(&str, (u32, u32))> = steam_library().iter().map(|(s, img)| (*s, png_size(&img.png()))).collect();
        assert_eq!(
            sizes,
            vec![("p", (600, 900)), ("", (920, 430)), ("_hero", (1920, 620)), ("_logo", (1000, 420)), ("_icon", (256, 256))]
        );
    }

    #[test]
    fn wordmark_is_drawn_and_logo_is_transparent_around_it() {
        let logo = logo();
        let ink = logo.px.iter().filter(|p| p[3] > 0.5).count();
        assert!(ink > 10_000, "wordmark drew {ink} pixels");
        assert_eq!(logo.px[0][3], 0.0);
        // Nothing is clipped at the edges.
        let w = logo.width as usize;
        for x in 0..w {
            assert_eq!(logo.px[x][3], 0.0);
            assert_eq!(logo.px[logo.px.len() - w + x][3], 0.0);
        }
    }

    #[test]
    fn ico_directory_points_at_each_png() {
        let ico = ico();
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
        let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
        assert_eq!(count, ICON_SIZES.len());
        for (i, &size) in ICON_SIZES.iter().enumerate() {
            let e = &ico[6 + 16 * i..6 + 16 * (i + 1)];
            let len = u32::from_le_bytes(e[8..12].try_into().unwrap()) as usize;
            let at = u32::from_le_bytes(e[12..16].try_into().unwrap()) as usize;
            assert_eq!(png_size(&ico[at..at + len]), (size, size));
        }
    }
}

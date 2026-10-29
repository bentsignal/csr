//! Counter-Strike Rust's own artwork, drawn in code: the emblem (a soldier
//! bust holding a rifle, in oxidized metal), the wordmark, the app icon, the
//! launcher backdrop, the Steam library images and the installer splash.
//!
//! Everything here is original geometry and procedural noise; no Valve
//! images or fonts are used, and no image files are committed
//! (`tools/check-no-assets.sh`). `build.rs` includes this file to embed the
//! icon in the exe, so it depends on `std` and `png` only.

#![allow(dead_code)]

pub type Rgba = [f32; 4];

/// Warm off-white text.
pub const INK: Rgba = [0.925, 0.914, 0.894, 1.0];
/// The single accent: oxidized rust orange (#C4541F).
pub const RUST: Rgba = [0.769, 0.329, 0.122, 1.0];
const BG_TOP: Rgba = [0.082, 0.086, 0.094, 1.0];
const BG_BOTTOM: Rgba = [0.031, 0.033, 0.037, 1.0];

/// A straight-alpha RGBA image with channels in 0..=1.
pub struct Image {
    pub width: u32,
    pub height: u32,
    px: Vec<Rgba>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Self {
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

    /// Applies `f` to every pixel's colour (alpha untouched).
    fn map(&mut self, f: impl Fn(f32, f32, [f32; 3]) -> [f32; 3]) {
        let w = self.width;
        for (i, p) in self.px.iter_mut().enumerate() {
            let (x, y) = ((i as u32 % w) as f32 + 0.5, (i as u32 / w) as f32 + 0.5);
            let c = f(x, y, [p[0], p[1], p[2]]);
            p[0] = c[0];
            p[1] = c[1];
            p[2] = c[2];
        }
    }

    fn all(&self) -> [f32; 4] {
        [0.0, 0.0, self.width as f32, self.height as f32]
    }

    /// Straight-alpha sRGB RGBA8 pixels, row by row.
    pub fn rgba8(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.px.len() * 4);
        for p in &self.px {
            for c in p {
                data.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
        data
    }

    /// RGBA8 PNG bytes.
    pub fn png(&self) -> Vec<u8> {
        let data = self.rgba8();
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

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn rgb(c: Rgba) -> [f32; 3] {
    [c[0], c[1], c[2]]
}

fn with_alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// --- Noise -------------------------------------------------------------------

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = hash(ix, iy, seed);
    let b = hash(ix + 1, iy, seed);
    let c = hash(ix, iy + 1, seed);
    let d = hash(ix + 1, iy + 1, seed);
    a + (b - a) * sx + (c - a) * sy + (a - b - c + d) * sx * sy
}

/// Fractal noise in 0..1.
fn fbm(x: f32, y: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..octaves {
        sum += amp * value_noise(x * freq, y * freq, seed.wrapping_add(o * 101));
        norm += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    sum / norm
}

// --- Signed distances ----------------------------------------------------------

/// Distance from (px, py) to the segment a-b.
fn segment_distance(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (cx, cy) = (a.0 + dx * t - px, a.1 + dy * t - py);
    (cx * cx + cy * cy).sqrt()
}

/// Signed distance to a stroke from a to b with square caps.
fn stroke_box(px: f32, py: f32, a: (f32, f32), b: (f32, f32), width: f32) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-4);
    let (ux, uy) = (dx / len, dy / len);
    let (mx, my) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let (qx, qy) = (px - mx, py - my);
    let u = (qx * ux + qy * uy).abs() - (len + width) / 2.0;
    let v = (-qx * uy + qy * ux).abs() - width / 2.0;
    let outside = (u.max(0.0).powi(2) + v.max(0.0).powi(2)).sqrt();
    outside + u.max(v).min(0.0)
}

/// Signed distance to a simple polygon (negative inside).
fn polygon(px: f32, py: f32, v: &[(f32, f32)]) -> f32 {
    let mut d = f32::MAX;
    let mut inside = false;
    let mut j = v.len() - 1;
    for i in 0..v.len() {
        let (a, b) = (v[i], v[j]);
        d = d.min(segment_distance(px, py, a, b));
        if (a.1 > py) != (b.1 > py) && px < (b.0 - a.0) * (py - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    if inside { -d } else { d }
}

fn ellipse(px: f32, py: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> f32 {
    let (x, y) = ((px - cx) / rx, (py - cy) / ry);
    ((x * x + y * y).sqrt() - 1.0) * rx.min(ry)
}

fn rounded_box(px: f32, py: f32, b: [f32; 4], radius: f32) -> f32 {
    let (cx, cy) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
    let (hw, hh) = ((b[2] - b[0]) / 2.0 - radius, (b[3] - b[1]) / 2.0 - radius);
    let qx = (px - cx).abs() - hw;
    let qy = (py - cy).abs() - hh;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}

// --- The soldier ---------------------------------------------------------------

/// The soldier bust in a 100x100 unit box (y down), facing left with a rifle
/// held across the chest. Returns (whole shape, rifle only) signed distances
/// in units. Parts are separated by thin gaps so the silhouette reads as an
/// emblem at every size.
fn soldier(x: f32, y: f32) -> (f32, f32) {
    const GAP: f32 = 1.6;
    // Helmet in profile: dome, front lip and a flared back.
    let dome = ellipse(x, y, 54.0, 25.0, 19.0, 15.5).max(y - 28.5);
    let lip = polygon(x, y, &[(33.5, 26.0), (42.0, 25.0), (42.0, 29.5), (33.0, 29.0)]);
    let flare = polygon(x, y, &[(62.0, 24.0), (72.5, 27.0), (76.5, 34.0), (66.0, 32.5)]);
    let helmet = dome.min(lip).min(flare);
    // Face profile: brow, nose, lips, chin, jaw, nape.
    let face = polygon(
        x,
        y,
        &[
            (39.5, 28.0),
            (38.8, 32.0),
            (37.6, 34.2),
            (34.6, 38.6),
            (37.4, 39.8),
            (36.9, 42.0),
            (38.0, 43.0),
            (37.6, 45.0),
            (40.0, 48.6),
            (46.0, 50.0),
            (54.0, 47.5),
            (62.0, 42.0),
            (65.0, 31.0),
        ],
    );
    let neck = polygon(x, y, &[(43.0, 47.0), (58.0, 44.0), (64.0, 57.0), (42.0, 59.0)]);
    let torso = polygon(
        x,
        y,
        &[
            (40.0, 55.0),
            (29.0, 57.5),
            (17.0, 63.0),
            (9.0, 72.0),
            (5.0, 100.0),
            (97.0, 100.0),
            (93.0, 74.0),
            (86.0, 63.0),
            (73.0, 56.0),
            (62.0, 53.5),
        ],
    );
    let head = face.min(neck).max(-(helmet - GAP));
    let body = helmet.min(head).min(torso);

    // Rifle along an axis from the stock (lower right) up-left to the muzzle.
    let (sx, sy) = (93.0, 86.0);
    let (dx, dy) = (-0.927_f32, -0.375_f32);
    let (qx, qy) = (x - sx, y - sy);
    let u = qx * dx + qy * dy;
    let v = qx * dy - qy * dx; // positive towards the magazine side (down)
    let parts: [&[(f32, f32)]; 10] = [
        &[(0.0, -3.4), (18.0, -2.6), (18.0, 3.0), (0.0, 5.0)],
        &[(17.0, -2.8), (50.0, -2.8), (50.0, 3.0), (17.0, 3.0)],
        &[(23.0, -3.8), (46.0, -3.8), (46.0, -2.0), (23.0, -2.0)],
        &[(49.0, -2.3), (70.0, -2.3), (70.0, 2.6), (49.0, 2.6)],
        &[(51.0, -3.6), (68.0, -3.6), (68.0, -2.0), (51.0, -2.0)],
        &[(69.0, -1.0), (92.0, -1.0), (92.0, 1.0), (69.0, 1.0)],
        &[(84.0, -4.0), (86.0, -4.0), (86.0, -0.5), (84.0, -0.5)],
        &[(89.0, -1.6), (94.0, -1.6), (94.0, 1.6), (89.0, 1.6)],
        &[(36.5, 2.6), (44.0, 2.6), (49.0, 14.5), (41.5, 16.0)],
        &[(25.5, 2.6), (31.0, 2.6), (30.0, 10.5), (25.0, 9.8)],
    ];
    let rifle = parts.iter().map(|p| polygon(u, v / 1.2, p) * 1.2).fold(f32::MAX, f32::min);
    // The support arm: a fist around the handguard, forearm down to the elbow.
    let (hx, hy) = (sx + dx * 60.0, sy + dy * 60.0);
    let fist = ellipse(x, y, hx + 0.5, hy + 0.8, 5.4, 4.8);
    let forearm = polygon(x, y, &[(hx - 5.0, hy + 1.5), (hx + 5.5, hy - 0.5), (hx + 16.0, 100.0), (hx + 3.0, 100.0)]);
    let arm = fist.min(forearm);
    let whole = body.max(-(rifle - GAP)).min(rifle);
    let whole = whole.max(-(arm - GAP)).min(arm);
    (whole, rifle)
}

/// The soldier's bounding box in units, including the muzzle.
const SOLDIER_BOUNDS: [f32; 4] = [0.0, 8.0, 99.0, 100.0];

/// Oxidized-metal colour at a point (in units) of a shape drawn at
/// `px_per_unit`, embossed from its signed distance `shape`: lit edges face
/// the top left, shaded edges the bottom right.
fn rust_metal(x: f32, y: f32, shape: &dyn Fn(f32, f32) -> f32, px_per_unit: f32) -> Rgba {
    // Texture detail scales with the drawn size so small icons stay clean.
    let detail = (px_per_unit / 2.5).clamp(0.15, 1.0);
    let patch = fbm(x * 0.05 + 3.0, y * 0.05, 4, 31);
    let grain = fbm(x * 0.32, y * 0.32, 2 + (3.0 * detail) as u32, 7);
    let fleck = fbm(x * 0.9 + 17.0, y * 0.9, 2, 11);
    let runs = fbm(x * 0.45, y * 0.035, 3, 23);
    let deep = [0.30, 0.10, 0.04];
    let mid = [0.62, 0.24, 0.085];
    let bright = [0.88, 0.45, 0.18];
    let t = smoothstep(0.25, 0.75, patch) * 0.8 + (grain - 0.5) * 0.5 * detail + 0.1;
    let mut c = if t < 0.5 { mix3(deep, mid, t * 2.0) } else { mix3(mid, bright, (t - 0.5) * 2.0) };
    c = mix3(c, [0.95, 0.56, 0.26], smoothstep(0.68, 0.8, fleck) * 0.5 * detail);
    c = mix3(c, [0.22, 0.07, 0.03], smoothstep(0.6, 0.8, runs) * 0.3 * detail);
    // Emboss: sample the shape a little up-left and down-right.
    let r = (1.6 / px_per_unit).max(0.35);
    let lit = smoothstep(0.0, r, shape(x - r * 0.7, y - r * 0.7));
    let shade = smoothstep(0.0, r, shape(x + r * 0.7, y + r * 0.7));
    let k = (1.05 - y * 0.0035) * (1.0 + lit * 0.28) * (1.0 - shade * 0.5);
    [c[0] * k, c[1] * k, c[2] * k, 1.0]
}

/// Draws the rust-metal emblem with the unit box mapped to (ox, oy) at
/// `scale` px per unit, clipped by `clip` (a signed distance in px).
fn emblem_at(img: &mut Image, ox: f32, oy: f32, scale: f32, clip: &dyn Fn(f32, f32) -> f32) {
    let b = SOLDIER_BOUNDS;
    let bounds = [ox + b[0] * scale - 2.0, oy + b[1] * scale - 2.0, ox + b[2] * scale + 2.0, oy + b[3] * scale + 2.0];
    let unit = |x: f32, y: f32| ((x - ox) / scale, (y - oy) / scale);
    let shape = |u: f32, v: f32| soldier(u, v).0;
    img.fill(
        |x, y| {
            let (u, v) = unit(x, y);
            rust_metal(u, v, &shape, scale)
        },
        bounds,
        |x, y| {
            let (u, v) = unit(x, y);
            (soldier(u, v).0 * scale).max(clip(x, y))
        },
    );
}

/// The emblem alone on transparency, `size` px square.
pub fn emblem(size: u32) -> Image {
    let mut img = Image::new(size, size);
    let s = size as f32;
    let scale = s / 100.0;
    emblem_at(&mut img, 0.0, -4.0 * scale, scale, &|_, y| y - s);
    img
}

// --- Wordmark ------------------------------------------------------------------

/// Stroke glyphs on a grid 6 units tall (y down): polylines and the
/// glyph's width in units.
fn glyph(c: char) -> (&'static [&'static [(f32, f32)]], f32) {
    match c {
        'C' => (&[&[(4.0, 0.0), (1.0, 0.0), (0.0, 1.0), (0.0, 5.0), (1.0, 6.0), (4.0, 6.0)]], 4.0),
        'S' => (
            &[&[(4.0, 0.0), (1.0, 0.0), (0.0, 1.0), (0.0, 2.0), (1.0, 3.0), (3.0, 3.0), (4.0, 4.0), (4.0, 5.0), (3.0, 6.0), (0.0, 6.0)]],
            4.0,
        ),
        'O' => (
            &[&[(1.0, 0.0), (3.0, 0.0), (4.0, 1.0), (4.0, 5.0), (3.0, 6.0), (1.0, 6.0), (0.0, 5.0), (0.0, 1.0), (1.0, 0.0)]],
            4.0,
        ),
        'U' => (&[&[(0.0, 0.0), (0.0, 5.0), (1.0, 6.0), (3.0, 6.0), (4.0, 5.0), (4.0, 0.0)]], 4.0),
        'N' => (&[&[(0.0, 6.0), (0.0, 0.0), (4.0, 6.0), (4.0, 0.0)]], 4.0),
        'R' => (
            &[&[(0.0, 6.0), (0.0, 0.0), (3.0, 0.0), (4.0, 1.0), (4.0, 2.0), (3.0, 3.0), (0.0, 3.0)], &[(2.2, 3.0), (4.0, 6.0)]],
            4.0,
        ),
        'E' => (&[&[(4.0, 0.0), (0.0, 0.0), (0.0, 6.0), (4.0, 6.0)], &[(0.0, 3.0), (3.2, 3.0)]], 4.0),
        'K' => (&[&[(0.0, 0.0), (0.0, 6.0)], &[(3.9, 0.0), (0.0, 3.5)], &[(1.5, 2.3), (4.0, 6.0)]], 4.0),
        'I' => (&[&[(0.0, 0.0), (0.0, 6.0)]], 0.0),
        'T' => (&[&[(0.0, 0.0), (4.0, 0.0)], &[(2.0, 0.0), (2.0, 6.0)]], 4.0),
        '-' => (&[&[(0.0, 3.0), (2.0, 3.0)]], 2.0),
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
        |px, py| segments.iter().map(|&(a, b)| stroke_box(px, py, a, b, stroke)).fold(f32::MAX, f32::min),
    );
}

/// "COUNTER-STRIKE" over a spaced "RUST" between two rules, centred on `cx`,
/// top at `top`. Returns the lockup's (width, height).
fn wordmark(img: &mut Image, cx: f32, top: f32, height: f32, main: Rgba) -> (f32, f32) {
    let stroke = height * 0.17;
    let tracking = 0.35;
    let w = text_width("COUNTER-STRIKE", height, tracking);
    text(img, "COUNTER-STRIKE", cx - w / 2.0, top, height, stroke, tracking, main);
    let small = height * 0.42;
    let small_tracking = 3.2;
    let sw = text_width("RUST", small, small_tracking);
    let sy = top + height + height * 0.62;
    text(img, "RUST", cx - sw / 2.0, sy, small, small * 0.2, small_tracking, RUST);
    let rule_h = (height * 0.045).max(1.0);
    let rule_y = sy + small / 2.0 - rule_h / 2.0;
    let gap = small * 1.1;
    let left = cx - w / 2.0 - stroke / 2.0;
    let right = cx + w / 2.0 + stroke / 2.0;
    img.fill(|_, _| with_alpha(RUST, 0.85), [left, rule_y, cx - sw / 2.0 - gap, rule_y + rule_h], |_, _| -1.0);
    img.fill(|_, _| with_alpha(RUST, 0.85), [cx + sw / 2.0 + gap, rule_y, right, rule_y + rule_h], |_, _| -1.0);
    (w + stroke, sy + small + stroke - top)
}

// --- Backdrop ------------------------------------------------------------------

/// The shared scene: charcoal gradient, a warm horizon glow behind a low
/// skyline, the soldier as a dark rim-lit silhouette, grain and vignette.
/// `figure` places the soldier (centre x, top y, px per unit), `glow` the
/// light (x, y) and `s` is the px-per-logical-px scale.
pub fn scene(img: &mut Image, figure: Option<(f32, f32, f32)>, glow: (f32, f32), s: f32) {
    let (w, h) = (img.width as f32, img.height as f32);
    let glow_color = [0.62, 0.24, 0.08];
    let horizon = glow.1;
    img.fill(
        |x, y| {
            let base = rgb(mix(BG_TOP, BG_BOTTOM, y / h));
            let dx = (x - glow.0) / (w * 0.62);
            let dy = (y - horizon) / (h * 0.42);
            let g = (1.0 - (dx * dx + dy * dy).sqrt()).max(0.0);
            let c = mix3(base, glow_color, g * g * 0.6);
            [c[0], c[1], c[2], 1.0]
        },
        img.all(),
        |_, _| -1.0,
    );
    // A distant, hazy town skyline: flat roofs, a dome and a tower.
    let ground = horizon + 26.0 * s;
    let mut x = -20.0 * s;
    let mut i = 0;
    let mut blocks = Vec::new();
    while x < w {
        let bw = (34.0 + hash(i, 1, 5) * 90.0) * s;
        let bh = (14.0 + hash(i, 2, 5) * 52.0) * s;
        blocks.push([x, ground - bh, x + bw, ground]);
        x += bw - 4.0 * s;
        i += 1;
    }
    let haze = [0.15, 0.085, 0.06];
    let sky = |x: f32, y: f32| {
        let t = ((ground - y) / (70.0 * s)).clamp(0.0, 1.0);
        let base = mix3(haze, [0.07, 0.065, 0.065], t);
        let dx = (x - glow.0) / (w * 0.5);
        let lit = (1.0 - dx.abs()).max(0.0) * 0.5;
        let c = mix3(base, [0.24, 0.11, 0.05], lit * (1.0 - t) * 0.6);
        [c[0], c[1], c[2], 1.0]
    };
    let dome = (glow.0 - 250.0 * s, ground - 52.0 * s, 34.0 * s);
    let tower = [glow.0 + 190.0 * s, ground - 108.0 * s, glow.0 + 212.0 * s, ground];
    img.fill(sky, [0.0, ground - 120.0 * s, w, ground + 2.0], |x, y| {
        let mut d = f32::MAX;
        for b in &blocks {
            d = d.min(rounded_box(x, y, *b, 0.5));
        }
        d = d.min(ellipse(x, y, dome.0, dome.1, dome.2, dome.2 * 0.95));
        d = d.min(rounded_box(x, y, [dome.0 - dome.2 * 1.25, dome.1, dome.0 + dome.2 * 1.25, ground], 0.5));
        d = d.min(rounded_box(x, y, tower, 0.5));
        d = d.min(polygon(x, y, &[(tower[0] - 4.0 * s, tower[1]), (tower[2] + 4.0 * s, tower[1]), ((tower[0] + tower[2]) / 2.0, tower[1] - 22.0 * s)]));
        // Arched doorways cut into the nearest walls.
        d.max(-(ellipse(x, y, glow.0 - 420.0 * s, ground - 10.0 * s, 9.0 * s, 15.0 * s)))
    });
    img.fill(
        |_, y| {
            let t = ((y - ground) / (h - ground)).clamp(0.0, 1.0);
            let c = mix3([0.075, 0.055, 0.048], [0.028, 0.029, 0.032], t.sqrt());
            [c[0], c[1], c[2], 1.0]
        },
        [0.0, ground, w, h],
        |_, _| -1.0,
    );
    if let Some((cx, top, scale)) = figure {
        let ox = cx - 50.0 * scale;
        let b = SOLDIER_BOUNDS;
        let bounds = [ox + b[0] * scale - 2.0, top + b[1] * scale - 2.0, ox + b[2] * scale + 2.0, h];
        let unit = |x: f32, y: f32| ((x - ox) / scale, (y - top) / scale);
        // Backlit: a warm rim on the edges facing the upper right, and a
        // faint one from the glow below.
        let rim = 3.2 * s / scale;
        let shape = |u: f32, v: f32| soldier(u, v).0;
        img.fill(
            |x, y| {
                let (u, v) = unit(x, y);
                let key = smoothstep(0.0, rim, soldier(u + 0.8 * rim, v - 0.6 * rim).0);
                let fill = smoothstep(0.0, rim, soldier(u - 0.5 * rim, v + 0.85 * rim).0);
                let metal = rust_metal(u, v, &shape, scale);
                let fade = 1.0 - smoothstep(50.0, 100.0, v) * 0.75;
                let base = mix3([0.028, 0.027, 0.029], rgb(metal), 0.07 * fade + 0.02);
                let lit = mix3(base, [0.93, 0.50, 0.22], key * 0.9 * fade);
                let lit = mix3(lit, [0.55, 0.20, 0.07], fill * 0.35);
                [lit[0], lit[1], lit[2], 1.0]
            },
            bounds,
            |x, y| {
                let (u, v) = unit(x, y);
                soldier(u, v).0 * scale
            },
        );
    }
    // Grain and vignette.
    img.map(|x, y, c| {
        let nx = (x / w - 0.5) * 1.6;
        let ny = (y / h - 0.45) * 1.9;
        let v = 1.0 - 0.5 * smoothstep(0.35, 1.25, (nx * nx + ny * ny).sqrt());
        let g = (hash(x as i32, y as i32, 99) - 0.5) * 0.018;
        [c[0] * v + g, c[1] * v + g, c[2] * v + g]
    });
}

/// The launcher window's backdrop at `w`x`h` physical pixels for `s`
/// physical px per logical px; the layout is a 1120x680 logical window.
pub fn launcher_backdrop(w: u32, h: u32, s: f32) -> Image {
    let mut img = Image::new(w, h);
    let lh = h as f32 / s;
    scene(&mut img, Some((622.0 * s, (lh - 392.0) * s, 3.9 * s)), (600.0 * s, (lh - 150.0) * s), s);
    // Keep the left column legible.
    img.map(|x, _, c| {
        let t = 1.0 - smoothstep(0.0, 520.0 * s, x);
        let k = 1.0 - 0.45 * t;
        [c[0] * k, c[1] * k, c[2] * k]
    });
    img
}

// --- Icon and library images -------------------------------------------------------

/// The app icon: the rust emblem on a dark rounded tile.
pub fn icon(size: u32) -> Image {
    let mut img = Image::new(size, size);
    let s = size as f32;
    let radius = s * 0.16;
    let inset = (s * 0.02).max(0.0);
    let tile = [inset, inset, s - inset, s - inset];
    img.fill(
        |x, y| {
            let base = mix(BG_TOP, BG_BOTTOM, y / s);
            let dx = (x - s * 0.5) / s;
            let dy = (y - s * 0.95) / s;
            let g = (1.0 - (dx * dx + dy * dy).sqrt() * 1.6).max(0.0);
            mix(base, [0.45, 0.17, 0.06, 1.0], g * g * 0.5)
        },
        img.all(),
        |x, y| rounded_box(x, y, tile, radius),
    );
    let scale = s * 0.0098;
    let ox = s / 2.0 - 50.0 * scale;
    let oy = s - inset - 100.0 * scale;
    // Clip the bust to the tile's rounded outline.
    emblem_at(&mut img, ox, oy, scale, &|x, y| rounded_box(x, y, tile, radius));
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
    scene(&mut img, Some((300.0, 150.0, 5.4)), (330.0, 640.0), 1.4);
    img.fill(|_, y| with_alpha(BG_BOTTOM, smoothstep(560.0, 760.0, y) * 0.9), [0.0, 560.0, 600.0, 900.0], |_, _| -1.0);
    wordmark(&mut img, 300.0, 735.0, 34.0, INK);
    img
}

/// Wide capsule (recent games, Big Picture), 920x430.
pub fn landscape() -> Image {
    let mut img = Image::new(920, 430);
    scene(&mut img, Some((690.0, 60.0, 3.9)), (660.0, 330.0), 1.0);
    img.map(|x, _, c| {
        let k = 1.0 - 0.55 * (1.0 - smoothstep(0.0, 560.0, x));
        [c[0] * k, c[1] * k, c[2] * k]
    });
    wordmark(&mut img, 285.0, 175.0, 36.0, INK);
    img
}

/// Library hero banner (Steam draws the logo over it), 1920x620.
pub fn hero() -> Image {
    let mut img = Image::new(1920, 620);
    scene(&mut img, Some((1380.0, 40.0, 6.0)), (1300.0, 470.0), 1.5);
    img
}

/// Transparent logo for the hero banner, 1000x420.
pub fn logo() -> Image {
    let mut img = Image::new(1000, 420);
    wordmark(&mut img, 500.0, 150.0, 60.0, INK);
    img
}

/// Installer splash, 600x400.
pub fn splash() -> Image {
    let mut img = Image::new(600, 400);
    scene(&mut img, Some((300.0, 36.0, 2.7)), (320.0, 300.0), 0.8);
    img.fill(|_, y| with_alpha(BG_BOTTOM, smoothstep(250.0, 320.0, y) * 0.92), [0.0, 250.0, 600.0, 400.0], |_, _| -1.0);
    wordmark(&mut img, 300.0, 318.0, 22.0, INK);
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
        for row in logo.px.chunks(w) {
            assert_eq!(row[0][3], 0.0);
            assert_eq!(row[w - 1][3], 0.0);
        }
    }

    #[test]
    fn emblem_is_rust_coloured_and_fits_its_box() {
        let e = emblem(128);
        let inked: Vec<_> = e.px.iter().filter(|p| p[3] > 0.5).collect();
        assert!(inked.len() > 128 * 128 / 4, "emblem drew {} pixels", inked.len());
        // Oxidized orange, never teal: red dominates blue on average.
        let (r, b) = inked.iter().fold((0.0, 0.0), |(r, b), p| (r + p[0], b + p[2]));
        assert!(r > b * 2.0);
        // The top row and the sides stay clear.
        assert!(e.px[..128].iter().all(|p| p[3] == 0.0));
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

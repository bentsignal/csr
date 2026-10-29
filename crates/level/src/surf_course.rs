//! Original, generated practice content. The same validated pack can be
//! exported/installed; it contains no game assets or executable definitions.
//!
//! Route: a start deck drops onto a long surf ramp, which launches onto a
//! large skate plaza (funbox, rail gap, ledge, manual pad, bank and quarter
//! pipe), whose end kicker throws the rider onto a second surf ramp.
use glam::Vec3;

pub const NAME: &str = "surf_skate_clip";
pub const ID: &str = "mod_map_surf_skate_clip";

/// First surf ramp, along +X. Riders surf its -Y face, pushing toward +Y.
pub const RAMP1_X: [f32; 2] = [-8000., -4800.];
/// The skate plaza: one flat brush, so no internal seams under the wheels.
pub const DECK_X: [f32; 2] = [-4600., 3000.];
pub const DECK_Z: f32 = 600.;
/// The clear riding lane from the landing to the end kicker.
pub const LANE_Y: f32 = -150.;
/// The plaza's end kicker, spanning the lane.
pub const KICKER_X: [f32; 2] = [2760., 3000.];
pub const RAMP2_X: [f32; 2] = [3050., 8050.];
pub const FINISH_X: [f32; 2] = [8150., 9350.];

const RAMP_SLOPE: f32 = 1.6;
const RAMP_DROP: f32 = 0.2;
const RAMP_HALF: f32 = 320.;
const RAMP1_PEAK: f32 = 1500.;
const RAMP2_PEAK: f32 = 350.;
/// The second ridge sits beside the lane, so a rider leaving the end kicker
/// comes down onto its surfable (-Y) face rather than over the ridge.
pub const RAMP2_Y: f32 = 60.;
/// Ridge line (Y) of a surf ramp.
pub fn ramp_ridge(second: bool) -> f32 { if second { RAMP2_Y } else { 0. } }

/// Ridge height of a surf ramp at `x`.
pub fn ramp_peak(second: bool, x: f32) -> f32 {
    if second { RAMP2_PEAK - RAMP_DROP * (x - RAMP2_X[0]) } else { RAMP1_PEAK - RAMP_DROP * (x - RAMP1_X[0]) }
}

const MATERIALS: [(&str, &str); 7] = [
    ("surf_blue", "concrete"),
    ("concrete", "concrete"),
    ("wood", "wood"),
    ("metal", "metal"),
    ("surf_orange", "concrete"),
    ("paint", "concrete"),
    ("dark", "concrete"),
];
const SURF: usize = 0;
const CONCRETE: usize = 1;
const WOOD: usize = 2;
const METAL: usize = 3;
const SURF2: usize = 4;
const PAINT: usize = 5;
const DARK: usize = 6;

pub fn pack() -> Result<packs::Pack, String> {
    let mut files = vec![(format!("maps/{NAME}.bsp"), geometry())];
    for (name, surface) in MATERIALS {
        files.push((format!("materials/csr_course/{name}.vmt"), format!(
            "LightmappedGeneric\n{{\n\"$basetexture\" \"csr_course/{name}\"\n\"$surfaceprop\" \"{surface}\"\n}}\n"
        ).into_bytes()));
        files.push((format!("materials/csr_course/{name}.vtf"), texture(name)));
    }
    let manifest = packs::Manifest {
        format_version: 1,
        id: "csr_surf_skate_course".into(),
        name: "Surf / Skate Clip Course".into(),
        version: "2.0.0".into(),
        origin: "mod".into(),
        kind: packs::PackKind::Assets,
        provides: vec![ID.into()],
        content_hash: String::new(),
        min_game_version: "0.5.0".into(),
        description: "An original surf ramp, a long skate plaza with a funbox, rail gap, ledge, bank and quarter pipe, and a second surf run.".into(),
        maps: vec![packs::MapDef {
            id: ID.into(),
            bsp: NAME.into(),
            name: "Surf / Skate Clip Course".into(),
            source: packs::MapSource::Pack,
        }],
        resources: vec![],
        mechanics: vec![],
    };
    let inputs: Vec<_> = files
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    packs::Pack::from_bytes(packs::build(&manifest, &inputs)?)
}

/// Tileable hash value noise in [0, 1] with `cells` cells across 256 texels.
fn noise(x: usize, y: usize, cells: usize, seed: u32) -> f32 {
    let hash = |x: usize, y: usize| {
        let mut h = (x as u32 % cells as u32).wrapping_mul(374_761_393)
            ^ (y as u32 % cells as u32).wrapping_mul(668_265_263)
            ^ seed.wrapping_mul(2_246_822_519);
        h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
        (h ^ (h >> 16)) as f32 / u32::MAX as f32
    };
    let size = 256. / cells as f32;
    let (fx, fy) = (x as f32 / size, y as f32 / size);
    let (ix, iy) = (fx as usize, fy as usize);
    let (tx, ty) = (fx.fract(), fy.fract());
    let (tx, ty) = (tx * tx * (3. - 2. * tx), ty * ty * (3. - 2. * ty));
    let a = hash(ix, iy) + (hash(ix + 1, iy) - hash(ix, iy)) * tx;
    let b = hash(ix, iy + 1) + (hash(ix + 1, iy + 1) - hash(ix, iy + 1)) * tx;
    a + (b - a) * ty
}

/// 256x256 RGBA8888 with a full mip chain. At 4 units per texel one tile
/// covers 1024 units; patterns are drawn to read at riding distance.
fn texture(kind: &str) -> Vec<u8> {
    const N: usize = 256;
    let mut image = vec![[0f32; 3]; N * N];
    for y in 0..N {
        for x in 0..N {
            let grain = noise(x, y, 64, 1) * 0.6 + noise(x, y, 16, 2) * 0.4;
            let fine = noise(x, y, 128, 3);
            let c = match kind {
                "surf_blue" | "surf_orange" => {
                    let base = if kind == "surf_blue" { [52., 128., 196.] } else { [222., 122., 44.] };
                    // Large panels with a dark seam and a soft lighter bevel.
                    let edge = x.min(y).min(N - 1 - x).min(N - 1 - y) as f32;
                    let half = (x % 128).min(y % 128).min(127 - x % 128).min(127 - y % 128) as f32;
                    let shade = if edge < 3. { 0.62 } else if edge < 7. { 1.12 } else if half < 1.5 { 0.86 } else { 1. }
                        * (0.95 + grain * 0.1);
                    base.map(|c| c * shade)
                }
                "concrete" | "paint" | "dark" => {
                    let base = match kind { "concrete" => [168., 166., 160.], "paint" => [184., 58., 44.], _ => [74., 78., 84.] };
                    // Broom-finished slab with expansion joints every 512 units.
                    let joint = (x % 128).min(y % 128).min(127 - x % 128).min(127 - y % 128) < 1;
                    let speck = if fine > 0.93 { 0.8 } else if fine < 0.05 { 1.12 } else { 1. };
                    let shade = (0.86 + grain * 0.22) * speck * if joint { 0.62 } else { 1. };
                    base.map(|c| c * shade)
                }
                "wood" => {
                    // Plywood sheets: 32-texel planks along X, grain streaks.
                    let plank = y / 32;
                    let tone = 0.88 + noise(plank, 0, 8, 4) * 0.2;
                    let streak = (((x as f32) * 0.07 + noise(x, y, 32, 5) * 6.).sin() * 0.5 + 0.5) * 0.12;
                    let gap = y % 32 == 0 || (x + plank * 96) % 256 == 0;
                    let shade = (tone - streak) * (0.95 + fine * 0.08) * if gap { 0.55 } else { 1. };
                    [186. * shade, 140. * shade, 92. * shade]
                }
                _ => {
                    // Brushed steel.
                    let brush = noise(x, y / 8, 128, 6) * 0.14;
                    let shade = 0.86 + brush + grain * 0.06;
                    [150. * shade, 158. * shade, 166. * shade]
                }
            };
            image[y * N + x] = c;
        }
    }
    let mut levels = vec![image];
    while levels.last().unwrap().len() > 1 {
        let prev = levels.last().unwrap();
        let size = (prev.len() as f32).sqrt() as usize;
        let half = size / 2;
        let mut next = vec![[0f32; 3]; half * half];
        for y in 0..half {
            for x in 0..half {
                let mut sum = [0f32; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = prev[(y * 2 + dy) * size + x * 2 + dx];
                    for i in 0..3 { sum[i] += p[i] * 0.25; }
                }
                next[y * half + x] = sum;
            }
        }
        levels.push(next);
    }
    let mut b = vec![0; 80];
    b[..4].copy_from_slice(b"VTF\0");
    put32(&mut b, 4, 7);
    put32(&mut b, 8, 2);
    put32(&mut b, 12, 80);
    put16(&mut b, 16, N as u16);
    put16(&mut b, 18, N as u16);
    put16(&mut b, 24, 1);
    b[56] = levels.len() as u8;
    put32(&mut b, 57, u32::MAX);
    put16(&mut b, 63, 1);
    // Mips are stored smallest first.
    for level in levels.iter().rev() {
        for c in level {
            b.extend(c.map(|v| v.clamp(0., 255.) as u8));
            b.push(255);
        }
    }
    b
}
fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn floats(b: &mut Vec<u8>, v: impl IntoIterator<Item = f32>) {
    for f in v {
        b.extend(f.to_le_bytes());
    }
}

#[derive(Default)]
struct Builder {
    lumps: Vec<Vec<u8>>,
    faces: usize,
    brushes: usize,
}
impl Builder {
    fn new() -> Self {
        Self {
            lumps: vec![vec![]; 64],
            ..Default::default()
        }
    }
    /// A flat-bottomed block whose top may slope.
    fn block(&mut self, x: [f32; 2], y: [f32; 2], bottom: f32, top: impl Fn(f32, f32) -> f32, material: usize) {
        self.solid(x, y, |_, _| bottom, top, material);
    }
    /// A convex solid over an axis-aligned footprint with planar top and
    /// bottom. Edges where they meet (wedges) drop that side face entirely.
    fn solid(
        &mut self,
        x: [f32; 2],
        y: [f32; 2],
        bottom: impl Fn(f32, f32) -> f32,
        top: impl Fn(f32, f32) -> f32,
        material: usize,
    ) {
        let p = [
            Vec3::new(x[0], y[0], bottom(x[0], y[0])),
            Vec3::new(x[1], y[0], bottom(x[1], y[0])),
            Vec3::new(x[1], y[1], bottom(x[1], y[1])),
            Vec3::new(x[0], y[1], bottom(x[0], y[1])),
            Vec3::new(x[0], y[0], top(x[0], y[0])),
            Vec3::new(x[1], y[0], top(x[1], y[0])),
            Vec3::new(x[1], y[1], top(x[1], y[1])),
            Vec3::new(x[0], y[1], top(x[0], y[1])),
        ];
        let center = p.iter().copied().sum::<Vec3>() / 8.;
        let first_side = self.lumps[19].len() / 8;
        let mut sides = 0;
        for indices in [
            [0, 3, 2, 1],
            [4, 5, 6, 7],
            [0, 1, 5, 4],
            [1, 2, 6, 5],
            [2, 3, 7, 6],
            [3, 0, 4, 7],
        ] {
            // Drop coincident corners; a wedge's zero-height side vanishes.
            let mut corners: Vec<usize> = vec![];
            for i in indices {
                if corners.last().is_none_or(|&last| p[last].distance(p[i]) > 0.01)
                    && (corners.is_empty() || p[corners[0]].distance(p[i]) > 0.01)
                {
                    corners.push(i);
                }
            }
            if corners.len() < 3 {
                continue;
            }
            // Newell normal: robust for quads with a collapsed edge.
            let mut n = Vec3::ZERO;
            for (k, &i) in corners.iter().enumerate() {
                let (a, b) = (p[i], p[corners[(k + 1) % corners.len()]]);
                n += Vec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
            }
            if n.length() < 1e-3 {
                continue;
            }
            let mut n = n.normalize();
            if n.dot(p[corners[0]] - center) < 0. {
                corners.reverse();
                n = -n;
            }
            let plane = self.lumps[1].len() / 20;
            floats(&mut self.lumps[1], n.to_array());
            floats(&mut self.lumps[1], [n.dot(p[corners[0]])]);
            self.lumps[1].extend(3i32.to_le_bytes());
            let texinfo = self.lumps[6].len() / 72;
            let tangent = if n.z.abs() > 0.9 {
                Vec3::X
            } else {
                Vec3::Z.cross(n).normalize()
            };
            let bitangent = n.cross(tangent);
            // One authored luxel per face: every vertex samples its centre.
            for axis in [tangent * 0.25, bitangent * 0.25, Vec3::ZERO, Vec3::ZERO] {
                floats(&mut self.lumps[6], [axis.x, axis.y, axis.z, 0.]);
            }
            self.lumps[6].extend(0i32.to_le_bytes());
            self.lumps[6].extend((material as i32).to_le_bytes());
            let edge = self.lumps[13].len() / 4;
            let base = self.lumps[3].len() / 12;
            for &i in &corners {
                floats(&mut self.lumps[3], p[i].to_array());
            }
            let count = corners.len();
            for i in 0..count {
                let e = self.lumps[12].len() / 4;
                self.lumps[12].extend(((base + i) as u16).to_le_bytes());
                self.lumps[12].extend(((base + (i + 1) % count) as u16).to_le_bytes());
                self.lumps[13].extend((e as i32).to_le_bytes());
            }
            let mut face = [0u8; 56];
            put16(&mut face, 0, plane as u16);
            put32(&mut face, 4, edge as u32);
            put16(&mut face, 8, count as u16);
            put16(&mut face, 10, texinfo as u16);
            put16(&mut face, 12, u16::MAX);
            face[16..20].copy_from_slice(&[0, 255, 255, 255]);
            put32(&mut face, 20, self.lumps[8].len() as u32);
            let light = (100. + n.dot(Vec3::new(-0.4, -0.3, 0.86)).max(0.) * 125.) as u8;
            self.lumps[8].extend([light, light, light, 0]);
            self.lumps[7].extend(face);
            self.lumps[16].extend((self.faces as u16).to_le_bytes());
            self.faces += 1;
            self.lumps[19].extend((plane as u16).to_le_bytes());
            self.lumps[19].extend((texinfo as i16).to_le_bytes());
            self.lumps[19].extend((-1i16).to_le_bytes());
            self.lumps[19].extend([0, 0]);
            sides += 1;
        }
        // Source collision uses the six axial planes for brush broadphase.
        // Sloping faces and dropped wedge sides need non-rendered bevels.
        for (normal, dist) in [
            (Vec3::Z, p[4..].iter().map(|v| v.z).fold(f32::NEG_INFINITY, f32::max)),
            (Vec3::NEG_Z, -p[..4].iter().map(|v| v.z).fold(f32::INFINITY, f32::min)),
            (Vec3::X, x[1]),
            (Vec3::NEG_X, -x[0]),
            (Vec3::Y, y[1]),
            (Vec3::NEG_Y, -y[0]),
        ] {
            let plane = self.lumps[1].len() / 20;
            floats(&mut self.lumps[1], normal.to_array());
            floats(&mut self.lumps[1], [dist]);
            self.lumps[1].extend(2i32.to_le_bytes());
            self.lumps[19].extend((plane as u16).to_le_bytes());
            self.lumps[19].extend((-1i16).to_le_bytes());
            self.lumps[19].extend((-1i16).to_le_bytes());
            self.lumps[19].extend([1, 0]);
            sides += 1;
        }
        self.lumps[18].extend((first_side as i32).to_le_bytes());
        self.lumps[18].extend((sides as i32).to_le_bytes());
        self.lumps[18].extend(1i32.to_le_bytes());
        self.lumps[17].extend((self.brushes as u16).to_le_bytes());
        self.brushes += 1;
    }
    /// A surf ramp: a chevron slab whose ridge runs along X and descends.
    fn surf_ramp(&mut self, x: [f32; 2], second: bool, material: usize) {
        const THICK: f32 = 24.;
        let ridge = ramp_ridge(second);
        let face = move |x: f32, y: f32| ramp_peak(second, x) - RAMP_SLOPE * (y - ridge).abs();
        self.solid(x, [ridge - RAMP_HALF, ridge], move |x, y| face(x, y) - THICK, face, material);
        self.solid(x, [ridge, ridge + RAMP_HALF], move |x, y| face(x, y) - THICK, face, material);
    }
    /// A wedge on the deck rising along +X (or falling when `down`).
    fn kicker(&mut self, x: [f32; 2], y: [f32; 2], base: f32, height: f32, down: bool, material: usize) {
        let rise = move |px: f32, _: f32| {
            let t = ((px - x[0]) / (x[1] - x[0])).clamp(0., 1.);
            base + height * if down { 1. - t } else { t }
        };
        self.block(x, y, base, rise, material);
    }
    fn finish(mut self) -> Vec<u8> {
        let mut entities = String::from(
            "{\n\"classname\" \"worldspawn\"\n\"skyname\" \"sky_dust\"\n}\n{\n\"classname\" \"light_environment\"\n\"angles\" \"0 35 0\"\n\"pitch\" \"-55\"\n\"_light\" \"255 245 225 230\"\n\"_ambient\" \"170 190 210 100\"\n}\n",
        );
        for team in ["info_player_counterterrorist", "info_player_terrorist"] {
            for i in 0..5 {
                entities += &format!(
                    "{{\n\"classname\" \"{team}\"\n\"origin\" \"{} -128 1602\"\n\"angles\" \"0 0 0\"\n}}\n",
                    RAMP1_X[0] as i32 - 300 - i * 40
                );
            }
        }
        self.lumps[0] = entities.into_bytes();
        self.lumps[0].push(0);
        for (name, _) in MATERIALS {
            let start = self.lumps[43].len() as u32;
            self.lumps[44].extend(start.to_le_bytes());
            self.lumps[43].extend(format!("csr_course/{name}\0").as_bytes());
            floats(&mut self.lumps[2], [0.5; 3]);
            let index = (self.lumps[44].len() / 4 - 1) as i32;
            self.lumps[2].extend(index.to_le_bytes());
            for _ in 0..4 {
                self.lumps[2].extend(256i32.to_le_bytes());
            }
        }
        let mut node = [0u8; 32];
        put32(&mut node, 4, u32::MAX);
        put32(&mut node, 8, u32::MAX);
        bounds(&mut node, 12, 18);
        put16(&mut node, 26, self.faces as u16);
        self.lumps[5].extend(node);
        let mut leaf = [0u8; 32];
        bounds(&mut leaf, 8, 14);
        put16(&mut leaf, 22, self.faces as u16);
        put16(&mut leaf, 26, self.brushes as u16);
        self.lumps[10].extend(leaf);
        let mut model = vec![];
        let (lo, hi) = (BOUNDS.map(|b| b.0 as f32), BOUNDS.map(|b| b.1 as f32));
        floats(&mut model, [lo[0], lo[1], lo[2], hi[0], hi[1], hi[2], 0., 0., 0.]);
        model.extend(0i32.to_le_bytes());
        model.extend(0i32.to_le_bytes());
        model.extend((self.faces as i32).to_le_bytes());
        self.lumps[14] = model;
        self.lumps[52].extend([1, 0, 0, 0]);
        // Ambient samples use Source's 128x model-light scale (unlike face
        // luxels), so exponent -7 keeps this cube near .5 linear irradiance.
        for _ in 0..6 {
            self.lumps[56].extend([130, 140, 155, 249]);
        }
        self.lumps[56].extend([128, 128, 128, 0]);
        let mut bytes = vec![0; 1036];
        bytes[..4].copy_from_slice(b"VBSP");
        put32(&mut bytes, 4, 21);
        for (i, lump) in self.lumps.iter().enumerate() {
            if lump.is_empty() {
                continue;
            }
            while bytes.len() % 4 != 0 {
                bytes.push(0);
            }
            let offset = bytes.len();
            put32(&mut bytes, 8 + i * 16, offset as u32);
            put32(&mut bytes, 12 + i * 16, lump.len() as u32);
            bytes.extend(lump);
        }
        bytes
    }
}
const BOUNDS: [(i16, i16); 3] = [(-9300, 9800), (-2400, 2400), (-2200, 2400)];
fn bounds(b: &mut [u8], min: usize, max: usize) {
    for (i, (lo, hi)) in BOUNDS.into_iter().enumerate() {
        put16(b, min + i * 2, lo as u16);
        put16(b, max + i * 2, hi as u16);
    }
}
fn geometry() -> Vec<u8> {
    let mut b = Builder::new();
    let z = DECK_Z;
    // Start deck and the first surf ramp.
    b.block([RAMP1_X[0] - 700., RAMP1_X[0]], [-384., 384.], 1450., |_, _| 1600., DARK);
    b.surf_ramp(RAMP1_X, false, SURF);

    // The plaza sits ~260 below the first ridge exit: a landing Skate rides
    // away from (a 550-unit drop bails). Lines low on the face fall short.
    b.block([DECK_X[0], DECK_X[1]], [-1200., 1200.], z - 150., |_, _| z, CONCRETE);
    // North lane: funbox, rail gap, launch box.
    b.kicker([-3000., -2840.], [200., 520.], z, 24., false, WOOD);
    b.block([-2840., -2260.], [200., 520.], z, |_, _| z + 24., CONCRETE);
    b.kicker([-2260., -2100.], [200., 520.], z, 24., true, WOOD);
    b.kicker([-1600., -1440.], [220., 500.], z, 20., false, WOOD);
    b.block([-1420., -820.], [358., 362.], z + 17., |_, _| z + 20., METAL);
    for post in [-1380., -860.] {
        b.block([post - 2., post + 2.], [358., 362.], z, |_, _| z + 17., METAL);
    }
    b.kicker([-800., -640.], [220., 500.], z, 20., true, WOOD);
    b.kicker([200., 360.], [200., 520.], z, 40., false, WOOD);
    b.block([360., 780.], [200., 520.], z, |_, _| z + 40., CONCRETE);
    b.kicker([780., 940.], [200., 520.], z, 40., true, WOOD);
    // Gap jump: launch, a clear gap, and a landing ramp.
    b.kicker([1300., 1460.], [220., 500.], z, 36., false, WOOD);
    b.kicker([1760., 1960.], [220., 500.], z, 36., true, WOOD);
    // Light posts along both edges give the plaza scale and a sense of speed.
    for i in 0..10 {
        let x = DECK_X[0] + 400. + i as f32 * 760.;
        for y in [-1185., 1185.] {
            b.block([x - 5., x + 5.], [y - 5., y + 5.], z, |_, _| z + 260., DARK);
        }
    }
    // South lane: long ledge with steel coping, then a manual pad.
    b.block([-3400., -1400.], [-760., -700.], z, |_, _| z + 13., PAINT);
    b.block([-3400., -1400.], [-760., -700.], z + 13., |_, _| z + 16., METAL);
    b.block([-900., -100.], [-1000., -640.], z, |_, _| z + 8., CONCRETE);
    // A raised pad, then a four-stair set with a sloped handrail.
    b.kicker([-60., 100.], [-1150., -800.], z, 40., false, WOOD);
    b.block([100., 500.], [-1150., -800.], z, |_, _| z + 40., CONCRETE);
    for step in 0..3 {
        let x0 = 500. + step as f32 * 26.;
        b.block([x0, x0 + 26.], [-1150., -800.], z, move |_, _| z + 30. - step as f32 * 10., CONCRETE);
    }
    let rail = move |x: f32| z + 72. - (x - 500.) * (30. / 78.);
    b.solid([470., 620.], [-977., -973.], move |x, _| rail(x) - 3., move |x, _| rail(x), METAL);
    for (post, floor) in [(480., z + 40.), (606., z)] {
        b.block([post - 2., post + 2.], [-977., -973.], floor, move |_, _| rail(post + 2.) - 3., METAL);
    }
    b.block([1400., 2300.], [-1000., -700.], z, |_, _| z + 16., PAINT);
    b.block([1400., 2300.], [-1000., -700.], z + 16., |_, _| z + 18., METAL);
    // Far side: a long bank, then a quarter pipe with a deck behind it.
    b.block([-2400., -400.], [900., 1200.], z, |_, y| z + (y - 900.) * 0.6, CONCRETE);
    let radius = 110.;
    let arc = |deg: f32| (900. + radius * deg.to_radians().sin(), z + radius * (1. - deg.to_radians().cos()));
    for i in 0..6 {
        let (y0, z0) = arc(i as f32 * 14.);
        let (y1, z1) = arc((i + 1) as f32 * 14.);
        let slope = (z1 - z0) / (y1 - y0);
        // Each facet starts under its predecessor, hiding its seam face.
        let start = if i == 0 { y0 } else { y0 - 6. };
        b.block([400., 2000.], [start, y1], z, move |_, y| z0 + (y - y0) * slope, WOOD);
    }
    let (lip, top) = arc(84.);
    b.block([400., 2000.], [lip, lip + 90.], z, |_, _| top, CONCRETE);
    b.block([400., 2000.], [lip - 3., lip + 3.], top, |_, _| top + 2., METAL);
    // End kicker over the lane, launching onto the second surf ramp.
    b.kicker(KICKER_X, [-500., 100.], z, 80., false, WOOD);
    b.surf_ramp(RAMP2_X, true, SURF2);
    // Below the ridge exit so face lines reach it; the drop costs little or no CS fall damage.
    let finish = ramp_peak(true, RAMP2_X[1]) - 250.;
    b.block(FINISH_X, [-800., 800.], finish - 150., move |_, _| finish, DARK);
    // A broad catch floor makes a missed line recoverable with the marker.
    b.block([-9200., 9700.], [-2300., 2300.], -2150., |_, _| -2000., DARK);
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_pack_keeps_surf_brushes_and_clear_landings() {
        let pack = pack().unwrap();
        let map = bsp::Bsp::parse(&pack.file(&format!("maps/{NAME}.bsp")).unwrap()).unwrap();
        assert!(
            map.leaf_ambient[0]
                .cube
                .iter()
                .flatten()
                .all(|v| *v > 0.3 && *v < 0.8)
        );
        assert!(
            map.texinfo
                .iter()
                .all(|t| t.lightmap_vecs.iter().all(|v| *v == glam::Vec4::ZERO))
        );
        let world = collision::CollisionWorld::from_bsp(&map, &Default::default(), &|_| None);
        let down = |x: f32, y: f32, from: f32| {
            world.trace_ray(Vec3::new(x, y, from), Vec3::new(x, y, -3000.), collision::contents::MASK_PLAYERSOLID)
        };
        for second in [false, true] {
            let x = if second { RAMP2_X[0] + 1000. } else { RAMP1_X[0] + 1000. };
            let ramp = down(x, LANE_Y, 2400.);
            let expected = ramp_peak(second, x) - RAMP_SLOPE * (LANE_Y - ramp_ridge(second)).abs();
            assert!(ramp.fraction < 1. && (ramp.end.z - expected).abs() < 0.2, "{ramp:?}");
            assert!(ramp.normal.z < 0.7 && ramp.normal.z > 0.4);
            assert!(ramp.normal.y < 0., "riders push into the face toward +Y");
        }
        let floor = down(0., LANE_Y, 900.);
        assert!((floor.end.z - DECK_Z).abs() < 0.2);
        // Skate survives a ridge exit onto the plaza; a 550-unit drop bails.
        assert!((150. ..320.).contains(&(ramp_peak(false, RAMP1_X[1]) - DECK_Z)));
        let path = world.trace_box(
            Vec3::new(DECK_X[0] + 100., LANE_Y, DECK_Z + 4.),
            Vec3::new(KICKER_X[0] - 10., LANE_Y, DECK_Z + 4.),
            movement::STAND_MINS,
            movement::STAND_MAXS,
            collision::contents::MASK_PLAYERSOLID,
        );
        assert!(
            !path.start_solid && path.fraction == 1.,
            "the riding lane must have no walls or internal seams"
        );
        let kicker = down((KICKER_X[0] + KICKER_X[1]) * 0.5, LANE_Y, 900.);
        assert!((kicker.end.z - (DECK_Z + 40.)).abs() < 0.5, "{kicker:?}");
        let rail = down(-1100., 360., 900.);
        assert!((rail.end.z - (DECK_Z + 20.)).abs() < 0.2, "{rail:?}");
    }
}

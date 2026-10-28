//! Original, generated practice content. The same validated pack can be
//! exported/installed; it contains no game assets or executable definitions.
use glam::Vec3;

pub const NAME: &str = "surf_skate_clip";
pub const ID: &str = "mod_map_surf_skate_clip";

pub fn pack() -> Result<packs::Pack, String> {
    let mut files = vec![(format!("maps/{NAME}.bsp"), geometry())];
    for (name, color, surface) in [
        ("ramp", [55, 115, 155], "concrete"),
        ("concrete", [145, 151, 153], "concrete"),
        ("wood", [164, 119, 72], "wood"),
        ("metal", [107, 133, 143], "metal"),
    ] {
        files.push((format!("materials/csr_course/{name}.vmt"), format!(
            "LightmappedGeneric\n{{\n\"$basetexture\" \"csr_course/{name}\"\n\"$surfaceprop\" \"{surface}\"\n}}\n"
        ).into_bytes()));
        files.push((format!("materials/csr_course/{name}.vtf"), texture(color)));
    }
    let manifest = packs::Manifest {
        format_version: 1,
        id: "csr_surf_skate_course".into(),
        name: "Surf / Skate Clip Course".into(),
        version: "1.0.0".into(),
        origin: "mod".into(),
        kind: packs::PackKind::Assets,
        provides: vec![ID.into()],
        content_hash: String::new(),
        min_game_version: "0.5.0".into(),
        description: "An original surf ramp, trick deck and second surf run.".into(),
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

fn texture(color: [u8; 3]) -> Vec<u8> {
    let mut b = vec![0; 80];
    b[..4].copy_from_slice(b"VTF\0");
    put32(&mut b, 4, 7);
    put32(&mut b, 8, 2);
    put32(&mut b, 12, 80);
    put16(&mut b, 16, 32);
    put16(&mut b, 18, 32);
    put16(&mut b, 24, 1);
    b[56] = 1;
    put32(&mut b, 57, u32::MAX);
    put16(&mut b, 63, 1);
    for y in 0..32 {
        for x in 0..32 {
            let shade = if x == 0 || y == 0 {
                0.65
            } else if x == 16 || y == 16 {
                0.92
            } else {
                1.
            };
            b.extend(color.map(|c| (c as f32 * shade) as u8));
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
    /// A convex six-sided solid, with a planar sloping top.
    fn block(
        &mut self,
        x: [f32; 2],
        y: [f32; 2],
        bottom: f32,
        top: impl Fn(f32, f32) -> f32,
        material: usize,
    ) {
        let p = [
            Vec3::new(x[0], y[0], bottom),
            Vec3::new(x[1], y[0], bottom),
            Vec3::new(x[1], y[1], bottom),
            Vec3::new(x[0], y[1], bottom),
            Vec3::new(x[0], y[0], top(x[0], y[0])),
            Vec3::new(x[1], y[0], top(x[1], y[0])),
            Vec3::new(x[1], y[1], top(x[1], y[1])),
            Vec3::new(x[0], y[1], top(x[0], y[1])),
        ];
        let center = p.iter().copied().sum::<Vec3>() / 8.;
        let first_side = self.lumps[19].len() / 8;
        for mut indices in [
            [0, 3, 2, 1],
            [4, 5, 6, 7],
            [0, 1, 5, 4],
            [1, 2, 6, 5],
            [2, 3, 7, 6],
            [3, 0, 4, 7],
        ] {
            let mut n = (p[indices[1]] - p[indices[0]])
                .cross(p[indices[2]] - p[indices[0]])
                .normalize();
            if n.dot(p[indices[0]] - center) < 0. {
                indices.reverse();
                n = -n;
            }
            let plane = self.lumps[1].len() / 20;
            floats(&mut self.lumps[1], n.to_array());
            floats(&mut self.lumps[1], [n.dot(p[indices[0]])]);
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
            for i in indices {
                floats(&mut self.lumps[3], p[i].to_array());
            }
            for i in 0..4 {
                let e = self.lumps[12].len() / 4;
                self.lumps[12].extend(((base + i) as u16).to_le_bytes());
                self.lumps[12].extend(((base + (i + 1) % 4) as u16).to_le_bytes());
                self.lumps[13].extend((e as i32).to_le_bytes());
            }
            let mut face = [0u8; 56];
            put16(&mut face, 0, plane as u16);
            put32(&mut face, 4, edge as u32);
            put16(&mut face, 8, 4);
            put16(&mut face, 10, texinfo as u16);
            put16(&mut face, 12, u16::MAX);
            face[16..20].copy_from_slice(&[0, 255, 255, 255]);
            put32(&mut face, 20, self.lumps[8].len() as u32);
            let light = (110. + n.dot(Vec3::new(-0.4, -0.3, 0.86)).max(0.) * 115.) as u8;
            self.lumps[8].extend([light, light, light, 0]);
            self.lumps[7].extend(face);
            self.lumps[16].extend((self.faces as u16).to_le_bytes());
            self.faces += 1;
            self.lumps[19].extend((plane as u16).to_le_bytes());
            self.lumps[19].extend((texinfo as i16).to_le_bytes());
            self.lumps[19].extend((-1i16).to_le_bytes());
            self.lumps[19].extend([0, 0]);
        }
        // Source collision uses the six axial planes for brush broadphase.
        // Sloping top faces still need their non-rendered +Z bevel bound.
        let plane = self.lumps[1].len() / 20;
        floats(
            &mut self.lumps[1],
            [
                0.,
                0.,
                1.,
                p[4..].iter().map(|v| v.z).fold(f32::NEG_INFINITY, f32::max),
            ],
        );
        self.lumps[1].extend(2i32.to_le_bytes());
        self.lumps[19].extend((plane as u16).to_le_bytes());
        self.lumps[19].extend((-1i16).to_le_bytes());
        self.lumps[19].extend((-1i16).to_le_bytes());
        self.lumps[19].extend([1, 0]);
        self.lumps[18].extend((first_side as i32).to_le_bytes());
        self.lumps[18].extend(7i32.to_le_bytes());
        self.lumps[18].extend(1i32.to_le_bytes());
        self.lumps[17].extend((self.brushes as u16).to_le_bytes());
        self.brushes += 1;
    }
    fn finish(mut self) -> Vec<u8> {
        let mut entities = String::from(
            "{\n\"classname\" \"worldspawn\"\n\"skyname\" \"sky_dust\"\n}\n{\n\"classname\" \"light_environment\"\n\"angles\" \"0 35 0\"\n\"pitch\" \"-55\"\n\"_light\" \"255 245 225 230\"\n\"_ambient\" \"170 190 210 100\"\n}\n",
        );
        for team in ["info_player_counterterrorist", "info_player_terrorist"] {
            for i in 0..5 {
                entities += &format!(
                    "{{\n\"classname\" \"{team}\"\n\"origin\" \"{} -128 1602\"\n\"angles\" \"0 0 0\"\n}}\n",
                    -220 - i * 40
                );
            }
        }
        self.lumps[0] = entities.into_bytes();
        self.lumps[0].push(0);
        for name in ["ramp", "concrete", "wood", "metal"] {
            let start = self.lumps[43].len() as u32;
            self.lumps[44].extend(start.to_le_bytes());
            self.lumps[43].extend(format!("csr_course/{name}\0").as_bytes());
            floats(&mut self.lumps[2], [0.5; 3]);
            let index = (self.lumps[44].len() / 4 - 1) as i32;
            self.lumps[2].extend(index.to_le_bytes());
            for _ in 0..4 {
                self.lumps[2].extend(32i32.to_le_bytes());
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
        floats(
            &mut model,
            [-2048., -2048., -1800., 12000., 2048., 2400., 0., 0., 0.],
        );
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
fn bounds(b: &mut [u8], min: usize, max: usize) {
    for (i, (lo, hi)) in [(-2048i16, 12000i16), (-2048, 2048), (-1800, 2400)]
        .into_iter()
        .enumerate()
    {
        put16(b, min + i * 2, lo as u16);
        put16(b, max + i * 2, hi as u16);
    }
}
fn geometry() -> Vec<u8> {
    let mut b = Builder::new();
    b.block([-512., 0.], [-384., 384.], 1450., |_, _| 1600., 1);
    b.block(
        [0., 2600.],
        [-320., 320.],
        0.,
        |x, y| 1500. - 0.2 * x + 1.6 * y,
        0,
    );
    // One solid deck: internal vertical faces between touching brushes can
    // catch the native truck collider even when their tops are coplanar.
    b.block([2800., 5000.], [-768., 384.], 550., |_, _| 700., 2);
    b.block(
        [5032., 10000.],
        [-320., 320.],
        -1400.,
        |x, y| 400. - 0.2 * (x - 5032.) + 1.6 * y,
        0,
    );
    b.block([10000., 11200.], [-768., 384.], -700., |_, _| -550., 3);
    // A broad catch floor makes a missed line recoverable with the marker.
    b.block([-1024., 11500.], [-1800., 1800.], -1650., |_, _| -1500., 1);
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
        let ramp = world.trace_ray(
            Vec3::new(1000., 0., 2000.),
            Vec3::new(1000., 0., 500.),
            collision::contents::MASK_PLAYERSOLID,
        );
        assert!(
            ramp.fraction < 1. && (ramp.end.z - 1300.).abs() < 0.2,
            "{ramp:?}"
        );
        assert!(ramp.normal.z < 0.7 && ramp.normal.z > 0.4);
        let floor = world.trace_ray(
            Vec3::new(3800., 0., 900.),
            Vec3::new(3800., 0., 600.),
            collision::contents::MASK_PLAYERSOLID,
        );
        assert!((floor.end.z - 700.).abs() < 0.2);
        let path = world.trace_box(
            Vec3::new(3000., 0., 704.),
            Vec3::new(4900., 0., 704.),
            movement::STAND_MINS,
            movement::STAND_MAXS,
            collision::contents::MASK_PLAYERSOLID,
        );
        assert!(
            !path.start_solid && path.fraction == 1.,
            "the trick deck must have no internal collision walls"
        );
    }
}

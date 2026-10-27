//! Sky rendering.
//!
//! Source maps draw two skies behind the world:
//!
//! * the **3D skybox**: a miniature area of the map around `sky_camera`,
//!   drawn `scale`× larger (16 by default) as if the player stood at the
//!   sky camera. Distant buildings, terrain and cloud domes live here.
//! * the **2D skybox** (`skyname`), behind that.
//!
//! The 3D skybox geometry is transformed once into world space and drawn by
//! a second camera on [`SKY_LAYER`] that renders before the main camera, so
//! it is always behind the playable world.

use crate::coords::to_bevy;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

pub const SKY_LAYER: usize = 1;

pub fn sky_layer() -> RenderLayers {
    RenderLayers::layer(SKY_LAYER)
}

#[derive(Component)]
pub struct SkyFace;

/// (suffix, outward normal, texture right, texture down), in Source axes.
fn cube_faces() -> [(&'static str, glam::Vec3, glam::Vec3, glam::Vec3); 6] {
    let x = glam::Vec3::X; let y = glam::Vec3::Y; let z = glam::Vec3::Z;
    // Authored top-edge correspondence with the existing cardinal sides:
    // rt top = up bottom; bk top = up left; lf top = reversed up top;
    // ft top = reversed up right. This requires up's U=-Y, V=+X, not
    // U=+X, V=+Y. Verified against Legacy's Vertigo, daylight and Aztec
    // texture bytes. Uploads and Bevy UVs both retain top-left row order.
    // Keep the side/cardinal and bottom mappings until native parity or
    // distinguishable authored edges establish a separate correction.
    [("rt",x,-y,-z),("bk",y,x,-z),("lf",-x,y,-z),("ft",-y,-x,-z),("up",z,-y,x),("dn",-z,x,-y)]
}

fn face_corners(normal: glam::Vec3, right: glam::Vec3, down: glam::Vec3) -> [glam::Vec3; 4] {
    [normal-right-down, normal+right-down, normal+right+down, normal-right+down]
}

const FACE_UVS: [[f32; 2]; 4] = [[0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,1.0]];

/// One authored sky input. HDR half floats must bypass the RGBA8/sRGB path.
pub struct SkyTexture {
    pub name: String,
    pub vtf: vtf::Vtf,
    pub linear_hdr: bool,
}

/// Resolve only texture names declared by this face's material. Keep a working
/// base (and the existing compressed-sky path) unchanged, but try the authored
/// HDR input when the LDR file is absent, as in Legacy's vertigoBlue_hdr.
/// Shared with the headless probe so a missing declared sky cannot pass silently.
pub fn load_texture(fs: &vfs::Vfs, mat: &vmt::Material) -> Result<SkyTexture, String> {
    let mut errors = Vec::new();
    for (key, linear_hdr) in [("$basetexture", false), ("$hdrcompressedtexture", false), ("$hdrbasetexture", true)] {
        let Some(name) = mat.str(key) else { continue };
        let path = vmt::vtf_path(name);
        let Some(bytes) = fs.read(&path) else {
            errors.push(format!("missing {path}"));
            continue;
        };
        match vtf::Vtf::parse(&bytes) {
            Ok(vtf) => {
                if linear_hdr && vtf.format != vtf::Format::Rgba16161616F {
                    errors.push(format!("unsupported {key} format {:?}: {path}", vtf.format));
                    continue;
                }
                if vtf.width == 0 || vtf.height == 0 || vtf.depth != 1 || vtf.faces != 1 {
                    errors.push(format!("unsupported sky dimensions: {path}"));
                    continue;
                }
                return Ok(SkyTexture { name: name.into(), vtf, linear_hdr });
            }
            Err(error) => errors.push(format!("bad {path}: {error}")),
        }
    }
    Err(if errors.is_empty() { "no declared sky texture".into() } else { errors.join("; ") })
}

/// Original six sky textures surround the camera behind the 3D skybox.
#[allow(clippy::too_many_arguments)]
pub fn spawn_2d(commands: &mut Commands, level: &level::Level, meshes: &mut Assets<Mesh>, materials: &mut Assets<crate::material::WorldMaterial>, images: &mut Assets<Image>, textures: &mut crate::textures::TextureCache, shared: &crate::world_mesh::SharedHandles) {
    use bevy::{asset::RenderAssetUsages, mesh::{Indices, PrimitiveTopology}};
    let Some(name) = level.bsp.worldspawn().and_then(|w| w.str("skyname")) else { return };
    for (suffix, normal, right, down) in cube_faces() {
        let face = format!("skybox/{name}{suffix}");
        let Some(vmt) = vmt::Material::load(&face, &|p| level.vfs.read_string(p)) else {
            warn!("missing sky material: {face}");
            continue;
        };
        // The generic material translator selects a declared name before it
        // tries loading it. Resolve the sky separately so a missing LDR name
        // cannot mask an available authored HDR face or enter its missing cache.
        let selected = load_texture(&level.vfs, &vmt);
        let mut sky_vmt = vmt.clone();
        sky_vmt.params.entries.retain(|(key, _)| !matches!(key.as_str(), "$basetexture" | "$hdrcompressedtexture" | "$hdrbasetexture"));
        if let Ok(sky) = &selected {
            if !sky.linear_hdr {
                sky_vmt.params.set("$basetexture", kv::Value::Str(sky.name.clone()));
            }
        }
        // Keep the existing authored tint/alpha translation. HDR has no base
        // name here, avoiding even a temporary clamped RGBA8/sRGB upload.
        let mut material = crate::world_mesh::material_from_vmt(Some(&sky_vmt), &level.vfs, textures, images, shared, 1.0, false);
        match selected {
            Ok(sky) if sky.linear_hdr => {
                material.base = textures.upload_hdr_sky(&sky.vtf, images)
                    .map_or_else(|| textures.missing.clone(), |info| info.handle);
            }
            Ok(_) => {}
            Err(error) => {
                material.base = textures.missing.clone();
                textures.stats_missing += 1;
                warn!("sky face {face} unavailable: {error}");
            }
        }
        material.params.flags = crate::material::flags::UNLIT;
        material.cull = false;
        let corners = face_corners(normal, right, down);
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, corners.map(|p| to_bevy(p * 180_000.0).to_array()).to_vec());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![to_bevy(-normal).to_array();4]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, FACE_UVS.to_vec());
        mesh.insert_indices(Indices::U32(vec![0,1,2,0,2,3]));
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(materials.add(material)), Transform::default(), sky_layer(), SkyFace));
    }
}

pub fn follow_camera(local: Res<crate::player::LocalPlayer>, mut faces: Query<&mut Transform, With<SkyFace>>) {
    for mut face in &mut faces { face.translation = to_bevy(local.render_camera); }
}

/// Maps 3D-skybox space into world space: `p' = (p - origin) * scale`.
pub fn sky_transform(sky: &bsp::SkyCamera) -> Transform {
    Transform {
        translation: -to_bevy(sky.origin) * sky.scale,
        rotation: Quat::IDENTITY,
        scale: Vec3::splat(sky.scale),
    }
}

/// Average colour of the 2D sky's top face, used as the clear colour.
pub fn sky_color(fs: &vfs::Vfs, bsp: &bsp::Bsp) -> Option<Color> {
    let name = bsp.worldspawn()?.str("skyname")?;
    let read = |p: &str| fs.read_string(p);
    let mat = vmt::Material::load(&format!("skybox/{name}up"), &read)?;
    let sky = load_texture(fs, &mat).ok()?;
    average_color(&sky)
}

fn average_color(sky: &SkyTexture) -> Option<Color> {
    let v = &sky.vtf;
    // Smallest mip is a cheap average.
    let level = v.mip_count.saturating_sub(1);
    if sky.linear_hdr {
        let px = v.rgba_f32(level, 0, 0)?;
        let n = px.len().max(1) as f32;
        let mut sum = [0.0; 3];
        for c in px { for k in 0..3 { sum[k] += c[k]; } }
        return Some(Color::linear_rgb(sum[0] / n, sum[1] / n, sum[2] / n));
    }
    let px = v.rgba8(level, 0, 0)?;
    let n = (px.len() / 4).max(1) as f32;
    let mut sum = [0.0f32; 3];
    for c in px.chunks_exact(4) {
        for k in 0..3 {
            sum[k] += c[k] as f32;
        }
    }
    Some(Color::srgb(sum[0] / n / 255.0, sum[1] / n / 255.0, sum[2] / n / 255.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textures::TextureCache;
    use bevy::render::render_resource::TextureFormat;

    // Synthetic 7.2 VTF: 2x2 plus 1x1, stored smallest first. Half-float
    // channels deliberately include >1 radiance, fractional RGB and zero alpha.
    fn texture(hdr: bool) -> Vec<u8> {
        let mut bytes = vec![0; 80];
        bytes[..4].copy_from_slice(b"VTF\0");
        bytes[4..8].copy_from_slice(&7u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&2u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&80u32.to_le_bytes());
        bytes[16..18].copy_from_slice(&2u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&2u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&1u16.to_le_bytes());
        bytes[52..56].copy_from_slice(&(if hdr { 24i32 } else { 0i32 }).to_le_bytes());
        bytes[56] = 2;
        bytes[57..61].copy_from_slice(&(-1i32).to_le_bytes());
        bytes[63..65].copy_from_slice(&1u16.to_le_bytes());
        if hdr {
            for pixel in [[0x4000u16, 0x3800, 0x3400, 0], [0x4200, 0x3c00, 0x3800, 0],
                [0x4400, 0x3c00, 0x3800, 0], [0x4500, 0x3c00, 0x3800, 0], [0x4600, 0x3c00, 0x3800, 0]] {
                for channel in pixel { bytes.extend_from_slice(&channel.to_le_bytes()); }
            }
        } else {
            for _ in 0..5 { bytes.extend_from_slice(&[64, 128, 255, 255]); }
        }
        bytes
    }

    fn fs(files: &[(&str, Vec<u8>)]) -> vfs::Vfs {
        let mut fs = vfs::Vfs::new();
        fs.push_back("synthetic sky", vfs::Mount::Memory(files.iter()
            .map(|(path, bytes)| (vmt::vtf_path(path), std::sync::Arc::<[u8]>::from(bytes.clone())))
            .collect()));
        fs
    }

    fn material() -> vmt::Material {
        vmt::Material::parse_with(r#"Sky {
            $BaseTexture "test/ldr"
            // $hdrcompressedtexture "test/not-authored"
            $HdrBaseTexture "test/hdr"
        }"#, &|_| None, 0).unwrap()
    }

    /// Asset-free directional ramp in the authored five-face layout. These
    /// image-space rays encode the measured edge correspondences independently
    /// of cube_faces/face_corners; deriving the fixture from those helpers would
    /// make a rotated cap pass. Bottom orientation is outside this correction.
    fn directional_fixture(suffix: &str) -> Vec<u8> {
        let mut bytes = texture(false);
        bytes.truncate(80);
        bytes[16..18].copy_from_slice(&9u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&9u16.to_le_bytes());
        bytes[56] = 1;
        for row in 0..9 {
            for col in 0..9 {
                let u = col as f32 / 4.0 - 1.0;
                let v = row as f32 / 4.0 - 1.0;
                let ray = match suffix {
                    "rt" => [1.0, -u, -v],
                    "bk" => [u, 1.0, -v],
                    "lf" => [-1.0, u, -v],
                    "ft" => [-u, -1.0, -v],
                    "up" => [v, -u, 1.0],
                    _ => panic!("no directional fixture for {suffix}"),
                };
                bytes.extend(ray.map(|c| ((c + 1.0) * 127.5).round() as u8));
                bytes.push(255);
            }
        }
        bytes
    }

    fn sample_fixture(pixels: &[u8], uv: glam::Vec2) -> glam::Vec3 {
        let p = uv * 8.0;
        let x = (p.x.floor() as usize).min(7);
        let y = (p.y.floor() as usize).min(7);
        let pixel = |col, row| {
            let i = (row * 9 + col) * 4;
            glam::Vec3::new(pixels[i] as f32, pixels[i + 1] as f32, pixels[i + 2] as f32) / 255.0
        };
        pixel(x, y).lerp(pixel(x + 1, y), p.x - x as f32)
            .lerp(pixel(x, y + 1).lerp(pixel(x + 1, y + 1), p.x - x as f32), p.y - y as f32)
    }

    #[test]
    fn directional_sky_fixture_is_continuous_across_cap_and_side_mesh_edges() {
        let faces = cube_faces();
        let mut images = Assets::<Image>::default();
        let mut cache = TextureCache::new(&mut images);
        let mut pixels = Vec::new();
        for (suffix, _, _, _) in &faces[..5] {
            let name = format!("test/directional_{suffix}");
            let bytes = directional_fixture(suffix);
            let fixture_fs = fs(&[(&name, bytes)]);
            let info = cache.load(&fixture_fs, &mut images, &name, true).unwrap();
            let image = images.get(&info.handle).unwrap();
            assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
            pixels.push(image.data.clone().unwrap());
        }
        let mut cap_edges = 0;
        let mut side_edges = 0;
        for a in 0..5 {
            let (_, an, ar, ad) = faces[a];
            let ac = face_corners(an, ar, ad);
            for b in a + 1..5 {
                let (_, bn, br, bd) = faces[b];
                let bc = face_corners(bn, br, bd);
                let shared: Vec<_> = (0..4).flat_map(|i| (0..4).map(move |j| (i, j)))
                    .filter(|&(i, j)| ac[i] == bc[j]).collect();
                if shared.len() != 2 { continue; }
                if b == 4 { cap_edges += 1; } else { side_edges += 1; }
                let [(a0, b0), (a1, b1)] = shared.as_slice() else { unreachable!() };
                for t in [0.0, 0.13, 0.37, 0.69, 1.0] {
                    let uv = |i, j| glam::Vec2::from_array(FACE_UVS[i])
                        .lerp(glam::Vec2::from_array(FACE_UVS[j]), t);
                    let ca = sample_fixture(&pixels[a], uv(*a0, *a1));
                    let cb = sample_fixture(&pixels[b], uv(*b0, *b1));
                    assert!((ca - cb).abs().max_element() < 1.0 / 255.0,
                        "{} / {} at {t}: {ca:?} != {cb:?}", faces[a].0, faces[b].0);
                }
            }
        }
        assert_eq!((cap_edges, side_edges), (4, 4));
    }

    #[test]
    fn authored_hdr_fallback_preserves_radiance_mips_and_linear_clear_color() {
        for ldr in [None, Some(b"invalid VTF".to_vec())] {
            let mut files = vec![("test/hdr", texture(true))];
            if let Some(bytes) = ldr { files.push(("test/ldr", bytes)); }
            let sky = load_texture(&fs(&files), &material()).unwrap();
            assert_eq!(sky.name, "test/hdr");
            assert!(sky.linear_hdr);
            assert_eq!(average_color(&sky), Some(Color::linear_rgb(2.0, 0.5, 0.25)));
            assert_eq!(sky.vtf.rgba_f32(0, 0, 0).unwrap()[0], [3.0, 1.0, 0.5, 0.0]);

            let mut images = Assets::<Image>::default();
            let mut cache = TextureCache::new(&mut images);
            let info = cache.upload_hdr_sky(&sky.vtf, &mut images).unwrap();
            assert!(!info.has_alpha);
            let image = images.get(&info.handle).unwrap();
            assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba16Float);
            assert_eq!(image.texture_descriptor.mip_level_count, 2);
            let expected = [sky.vtf.level(0, 0, 0).unwrap(), sky.vtf.level(1, 0, 0).unwrap()].concat();
            assert_eq!(image.data.as_deref(), Some(expected.as_slice()));
            assert_eq!(cache.stats_missing, 0);
        }
    }

    #[test]
    fn existing_ldr_sky_keeps_selection_and_srgb_clear_color() {
        let sky = load_texture(&fs(&[("test/ldr", texture(false)), ("test/hdr", texture(true))]), &material()).unwrap();
        assert_eq!(sky.name, "test/ldr");
        assert!(!sky.linear_hdr);
        assert_eq!(average_color(&sky), Some(Color::srgb(64.0 / 255.0, 128.0 / 255.0, 1.0)));
    }

    #[test]
    fn missing_and_unsupported_declared_skies_fail_without_an_alias() {
        let error = load_texture(&fs(&[]), &material()).err().unwrap();
        assert!(error.contains("missing materials/test/ldr.vtf"));
        assert!(error.contains("missing materials/test/hdr.vtf"));
        assert!(!error.contains("not-authored"));
        let error = load_texture(&fs(&[("test/hdr", texture(false))]), &material()).err().unwrap();
        assert!(error.contains("unsupported $hdrbasetexture format Rgba8888"));
    }

    #[test]
    #[ignore = "requires CSR_SKY_FIXTURE_BSP pointing to private de_vertigo_2017.bsp and read-only Legacy; no native/GPU process"]
    fn installed_vertigo_six_authored_hdr_faces_upload_without_missing_textures() {
        let path = std::env::var_os("CSR_SKY_FIXTURE_BSP").expect("set CSR_SKY_FIXTURE_BSP to the private Vertigo BSP");
        let bsp = bsp::Bsp::parse(&std::fs::read(path).unwrap()).unwrap();
        let name = bsp.worldspawn().unwrap().str("skyname").unwrap();
        assert_eq!(name.to_ascii_lowercase(), "vertigoblue_hdr");
        let game_dir = vfs::find_csgo_dir().expect("installed Legacy or CSR_GAME_DIR");
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&game_dir).unwrap();
        fs.mount_map_pakfile(bsp.pakfile.clone()).unwrap();
        let mut images = Assets::<Image>::default();
        let mut cache = TextureCache::new(&mut images);
        for suffix in ["rt", "bk", "lf", "ft", "up", "dn"] {
            let face = format!("skybox/{name}{suffix}");
            let mat = vmt::Material::load(&face, &|p| fs.read_string(p)).unwrap();
            assert!(fs.read(&vmt::vtf_path(mat.base_texture().unwrap())).is_none());
            let sky = load_texture(&fs, &mat).unwrap();
            assert!(sky.linear_hdr, "{face}");
            assert_eq!(Some(sky.name.as_str()), mat.str("$hdrbasetexture"));
            assert_eq!((sky.vtf.width, sky.vtf.height, sky.vtf.mip_count), (1024, 1024, 11));
            for mip in 0..sky.vtf.mip_count {
                assert!(sky.vtf.rgba_f32(mip, 0, 0).unwrap().iter().flatten().all(|v| v.is_finite()));
            }
            let info = cache.upload_hdr_sky(&sky.vtf, &mut images).unwrap();
            let image = images.get(&info.handle).unwrap();
            assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba16Float);
            assert_eq!(image.texture_descriptor.mip_level_count, 11);
            assert_ne!(info.handle, cache.missing);
            images.remove(&info.handle);
        }
        assert_eq!(cache.stats_decoded, 6);
        assert_eq!(cache.stats_missing, 0);
        assert!(sky_color(&fs, &bsp).is_some());
    }
}

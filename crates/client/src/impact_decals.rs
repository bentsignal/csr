//! Runtime impact groups, subrect artwork and dimensions from the mounted game.
//! These are planar cosmetic decals; edge clipping and native projection remain open.
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use std::collections::HashMap;

#[derive(Clone)]
pub(crate) struct Decal {
    pub id: content::ContentId,
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    weight: f32,
}

#[derive(Default)]
pub(crate) struct Catalog {
    translations: HashMap<String, String>,
    groups: HashMap<String, Vec<Decal>>,
    surface_decals: HashMap<String, bool>,
    serial: u64,
}

#[derive(Debug, PartialEq)]
struct Artwork {
    texture: String,
    rect: Option<[usize; 4]>,
    scale: f32,
}

fn pair(value: &str) -> Option<[usize; 2]> {
    let numbers = kv::parse_vec(value);
    if numbers.len() != 2
        || numbers
            .iter()
            .any(|n| !n.is_finite() || *n < 0. || n.fract() != 0. || *n > 16384.)
    {
        return None;
    }
    Some([numbers[0] as usize, numbers[1] as usize])
}

fn artwork(fs: &vfs::Vfs, name: &str) -> Option<Artwork> {
    let mat = vmt::Material::load(name, &|p| fs.read_string(p))?;
    let scale = mat.f32("$decalscale")?;
    if !scale.is_finite() || !(0.0..=16.0).contains(&scale) || scale == 0. {
        return None;
    }
    match mat.shader.as_str() {
        "decalmodulate" => Some(Artwork {
            texture: mat.base_texture()?.into(),
            rect: None,
            scale,
        }),
        "subrect" => {
            let pos = pair(mat.str("$pos")?)?;
            let size = pair(mat.str("$size")?)?;
            if size.contains(&0) {
                return None;
            }
            let parent = vmt::Material::load(mat.str("$material")?, &|p| fs.read_string(p))?;
            if parent.shader != "decalmodulate" {
                return None;
            }
            Some(Artwork {
                texture: parent.base_texture()?.into(),
                rect: Some([pos[0], pos[1], size[0], size[1]]),
                scale,
            })
        }
        _ => None,
    }
}

fn crop(bytes: &[u8], rect: Option<[usize; 4]>) -> Option<(Vec<u8>, usize, usize)> {
    let v = vtf::Vtf::parse(bytes).ok()?;
    if v.width.checked_mul(v.height)? > 16_777_216 {
        return None;
    }
    let [x, y, width, height] = rect.unwrap_or([0, 0, v.width, v.height]);
    if width == 0
        || height == 0
        || width.checked_mul(height)? > 1_048_576
        || x.checked_add(width)? > v.width
        || y.checked_add(height)? > v.height
    {
        return None;
    }
    let rgba = v.rgba8(0, 0, 0)?;
    let mut pixels = Vec::with_capacity(width.checked_mul(height)?.checked_mul(4)?);
    for row in y..y + height {
        let start = (row * v.width + x) * 4;
        pixels.extend_from_slice(rgba.get(start..start + width * 4)?);
    }
    Some((pixels, width, height))
}

impl Catalog {
    pub(crate) fn load(
        fs: &vfs::Vfs,
        meshes: &mut Assets<Mesh>,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        let mut out = Self::default();
        let Some(doc) = fs
            .read_string("scripts/decals_subrect.txt")
            .and_then(|t| kv::parse(&t).ok())
        else {
            return out;
        };
        if let Some(table) = doc.block("TranslationData") {
            for (code, value) in table.iter().take(128) {
                if let Some(group) = value.as_str() {
                    out.translations
                        .insert(code.to_ascii_lowercase(), group.to_ascii_lowercase());
                }
            }
        }
        let mut cached: HashMap<String, Decal> = HashMap::new();
        let mut loaded = 0;
        for (name, value) in doc
            .iter()
            .filter(|(name, _)| name.to_ascii_lowercase().starts_with("impact."))
            .take(64)
        {
            let Some(group) = value.as_block() else {
                continue;
            };
            let mut variants = Vec::new();
            for (path, weight) in group.iter().take(128) {
                let Some(weight) = weight
                    .as_str()
                    .and_then(kv::parse_f32)
                    .filter(|v| v.is_finite() && *v > 0. && *v <= 1024.)
                else {
                    continue;
                };
                if let Some(decal) = cached.get(path) {
                    variants.push(Decal {
                        weight,
                        ..decal.clone()
                    });
                    continue;
                }
                if loaded >= 256 {
                    continue;
                }
                let Some(art) = artwork(fs, path) else {
                    continue;
                };
                let Some((pixels, w, h)) = fs
                    .read(&vmt::vtf_path(&art.texture))
                    .and_then(|b| crop(&b, art.rect))
                else {
                    continue;
                };
                let mut image = Image::new(
                    Extent3d {
                        width: w as u32,
                        height: h as u32,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    pixels,
                    TextureFormat::Rgba8Unorm,
                    RenderAssetUsages::RENDER_WORLD,
                );
                image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::ClampToEdge,
                    address_mode_v: ImageAddressMode::ClampToEdge,
                    mag_filter: bevy::image::ImageFilterMode::Linear,
                    min_filter: bevy::image::ImageFilterMode::Linear,
                    ..default()
                });
                // DecalModulate is a color multiplier, not a transparent dark
                // circle. Linear upload preserves the authored neutral midpoint;
                // the two-times factor makes that midpoint leave the wall alone.
                let material = materials.add(StandardMaterial {
                    base_color: Color::linear_rgba(2., 2., 2., 1.),
                    base_color_texture: Some(images.add(image)),
                    unlit: true,
                    alpha_mode: AlphaMode::Multiply,
                    cull_mode: None,
                    depth_bias: 1.,
                    ..default()
                });
                let decal = Decal {
                    id: content::ContentId::csgo(content::ContentKind::Material, path),
                    mesh: meshes.add(Rectangle::new(w as f32 * art.scale, h as f32 * art.scale)),
                    material,
                    weight,
                };
                cached.insert(path.to_owned(), decal.clone());
                variants.push(decal);
                loaded += 1;
            }
            if !variants.is_empty() {
                out.groups.insert(name.to_ascii_lowercase(), variants);
            }
        }
        info!(
            "impact decals: {} authored groups, {loaded} runtime material variants",
            out.groups.len()
        );
        out
    }

    /// One tiny startup entity warms the shared Multiply pipeline before fire.
    pub(crate) fn prewarm(&self) -> Option<&Decal> {
        self.groups.values().find_map(|variants| variants.first())
    }

    /// Respect decal exclusions and compiler-hidden receiving materials,
    /// including patches. Cache per level to avoid repeated VMT reads on fire.
    pub(crate) fn choose_for_surface(
        &mut self,
        fs: &vfs::Vfs,
        surface: &str,
        code: &str,
    ) -> Option<Decal> {
        let allowed = self.surface_decals.entry(vmt::normalize_name(surface)).or_insert_with(|| {
            // Model paths carry MDL surface properties, not a receiving VMT.
            // Preserve the existing model/door mark path in that case.
            surface.ends_with(".mdl") || vmt::Material::load(surface, &|p| fs.read_string(p))
                // Invisible collision faces can lack $nodecal (Legacy's
                // toolsnodraw is one). A planar mark on them floats in space.
                .is_none_or(|mat| !mat.flag("$nodecal") && !mat.is_nodraw() && !mat.is_sky())
        });
        if !*allowed {
            return None;
        }
        self.choose(code)
    }

    fn choose(&mut self, code: &str) -> Option<Decal> {
        let key = code.to_ascii_lowercase();
        let group = self.translations.get(&key)?;
        let variants = self.groups.get(group)?;
        let total: f32 = variants.iter().map(|v| v.weight).sum();
        // Local cosmetic randomness only; this never changes hit rays or sim RNG.
        self.serial = self.serial.wrapping_add(0x9e3779b97f4a7c15);
        let mut random = self.serial;
        random = (random ^ (random >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        random = (random ^ (random >> 27)).wrapping_mul(0x94d049bb133111eb);
        let mut at = ((random ^ (random >> 31)) >> 40) as f32 / 16777216. * total;
        for variant in variants {
            at -= variant.weight;
            if at < 0. {
                return Some(variant.clone());
            }
        }
        variants.last().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiving_material_can_disable_decals_without_consuming_a_choice() {
        let files: std::collections::BTreeMap<String, std::sync::Arc<[u8]>> = std::collections::BTreeMap::from([
            ("materials/test/fence.vmt".into(), std::sync::Arc::from(&br#"LightmappedGeneric { "$nodecal" "1" }"#[..])),
            ("materials/test/wall.vmt".into(), std::sync::Arc::from(&br#"LightmappedGeneric { "$nodecal" "0" }"#[..])),
            ("materials/test/patched.vmt".into(), std::sync::Arc::from(&br#"Patch { "include" "materials/test/wall.vmt" "replace" { "$nodecal" "1" } }"#[..])),
            ("materials/test/plain.vmt".into(), std::sync::Arc::from(&br#"LightmappedGeneric { "$surfaceprop" "fixture" }"#[..])),
        ]);
        let mut fs = vfs::Vfs::new();
        fs.push_back("synthetic", vfs::Mount::Memory(files));
        let mut catalog = Catalog::default();
        let id = content::ContentId::csgo(content::ContentKind::Material, "test/mark");
        catalog.translations.insert("fixture".into(), "impact.fixture".into());
        catalog.groups.insert("impact.fixture".into(), vec![Decal {
            id: id.clone(), mesh: default(), material: default(), weight: 1.,
        }]);
        for surface in ["test/fence", "test/patched"] {
            assert!(catalog.choose_for_surface(&fs, surface, "fixture").is_none());
        }
        assert_eq!(catalog.serial, 0);
        for surface in ["test/wall", "test/plain", "test/missing", "models/test/door.mdl"] {
            assert_eq!(catalog.choose_for_surface(&fs, surface, "fixture").unwrap().id, id);
        }
        assert!(catalog.choose_for_surface(&fs, "test/wall", "unknown").is_none());
    }

    #[test]
    fn compiler_hidden_receivers_do_not_draw_floating_bullet_marks() {
        let mut files = std::collections::BTreeMap::<String, std::sync::Arc<[u8]>>::new();
        // Synthetic declarations exercise the existing material visibility
        // predicates; no installed material text or artwork is embedded here.
        for (name, parameter) in [
            ("nodraw", "%compilenodraw"),
            ("clip", "%compileclip"),
            ("trigger", "%compiletrigger"),
            ("sky", "%compilesky"),
            ("sky2d", "%compile2dsky"),
        ] {
            let text = format!(r#"LightmappedGeneric {{ "{parameter}" "1" }}"#);
            files.insert(format!("materials/test/{name}.vmt"), std::sync::Arc::from(text.into_bytes()));
        }
        files.insert("materials/test/patched.vmt".into(), std::sync::Arc::from(
            &br#"Patch { "include" "materials/test/nodraw.vmt" "replace" { "$nodecal" "0" } }"#[..]));
        files.insert("materials/test/visible.vmt".into(), std::sync::Arc::from(
            &br#"LightmappedGeneric { "%compilenodraw" "0" "%compilesky" "0" }"#[..]));
        let mut fs = vfs::Vfs::new();
        fs.push_back("synthetic", vfs::Mount::Memory(files));
        let mut catalog = Catalog::default();
        catalog.translations.insert("c".into(), "impact.fixture".into());
        catalog.groups.insert("impact.fixture".into(), vec![Decal {
            id: content::ContentId::csgo(content::ContentKind::Material, "test/mark"),
            mesh: default(), material: default(), weight: 1.,
        }]);
        for name in ["nodraw", "clip", "trigger", "sky", "sky2d", "patched"] {
            // Repeat through the cached path too. Rejected marks must leave
            // the cosmetic choice sequence untouched.
            for _ in 0..2 {
                assert!(catalog.choose_for_surface(&fs, &format!("test/{name}"), "C").is_none());
            }
        }
        assert_eq!(catalog.serial, 0);
        assert!(catalog.choose_for_surface(&fs, "test/visible", "C").is_some());
    }

    #[test]
    fn subrect_dimensions_and_empty_no_decal_entries_are_respected() {
        let files: std::collections::BTreeMap<String, std::sync::Arc<[u8]>> = std::collections::BTreeMap::from([
            ("materials/test/mark.vmt".into(), std::sync::Arc::from(&br#"Subrect { "$material" "test/atlas" "$pos" "64 128" "$size" "32 64" "$decalscale" "0.1" }"#[..])),
            ("materials/test/atlas.vmt".into(), std::sync::Arc::from(&br#"DecalModulate { "$basetexture" "test/pixels" }"#[..])),
            ("materials/test/empty.vmt".into(), std::sync::Arc::from(&br#"Subrect { "$material" "test/atlas" "$pos" "0 0" "$size" "0 0" "$decalscale" "0.1" }"#[..])),
        ]);
        let mut fs = vfs::Vfs::new();
        fs.push_back("synthetic", vfs::Mount::Memory(files));
        assert_eq!(
            artwork(&fs, "test/mark"),
            Some(Artwork {
                texture: "test/pixels".into(),
                rect: Some([64, 128, 32, 64]),
                scale: 0.1
            })
        );
        assert!(artwork(&fs, "test/empty").is_none());
        assert!(pair("-1 64").is_none());
        assert!(pair("1.5 64").is_none());
    }
}

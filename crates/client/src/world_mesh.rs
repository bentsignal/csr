//! Map geometry: brush faces and displacements, batched into one mesh per
//! material, plus a lightmap atlas holding every face's baked lighting.

use crate::coords::to_bevy;
use crate::material::{flags, WorldMaterial, WorldParams};
use crate::textures::{f32_to_f16, TextureCache};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bsp::{surf, Bsp};
use std::collections::HashMap;

/// Luxel border around each lightmap in the atlas (stops bilinear bleeding).
const PAD: usize = 1;

pub struct LightmapAtlas {
    pub width: usize,
    pub height: usize,
    /// RGBA half floats.
    pub texels: Vec<u16>,
    /// Face index → top-left of the face's luxels (inside the padding).
    pub rects: HashMap<usize, (usize, usize)>,
    /// Texel centre of an always-white spot, for faces without lightmaps.
    pub white: (f32, f32),
}

impl LightmapAtlas {
    pub fn build(bsp: &Bsp) -> Self {
        let mut items: Vec<(usize, usize, usize)> = bsp
            .faces
            .iter()
            .enumerate()
            .filter_map(|(i, f)| bsp.lightmap_dims(f).map(|(w, h)| (i, w, h)))
            .collect();
        items.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)));

        let total: usize = items.iter().map(|(_, w, h)| (w + 2 * PAD) * (h + 2 * PAD)).sum();
        let mut width = 1024;
        while width * width < total * 2 && width < 8192 {
            width *= 2;
        }

        // Shelf packing, starting after a reserved 4x4 white block.
        let mut rects = HashMap::with_capacity(items.len());
        let (mut x, mut y, mut shelf_h) = (4usize, 0usize, 4usize);
        let mut placed = Vec::with_capacity(items.len());
        for &(face, w, h) in &items {
            let (pw, ph) = (w + 2 * PAD, h + 2 * PAD);
            if x + pw > width {
                x = 0;
                y += shelf_h;
                shelf_h = 0;
            }
            placed.push((face, x, y, w, h));
            rects.insert(face, (x + PAD, y + PAD));
            x += pw;
            shelf_h = shelf_h.max(ph);
        }
        let height = (y + shelf_h).next_power_of_two().max(4);

        let mut texels = vec![0u16; width * height * 4 * 4];
        for layer in 0..4 {
        let layer_offset = layer * width * height * 4;
        let one = f32_to_f16(1.0);
        for yy in 0..4 {
            for xx in 0..4 {
                let o = layer_offset + (yy * width + xx) * 4;
                texels[o..o + 4].copy_from_slice(&[one, one, one, one]);
            }
        }
        for &(face, px, py, w, h) in &placed {
            let Some((_, _, lux)) = bsp.face_lightmap_layer(&bsp.faces[face], layer).or_else(|| bsp.face_lightmap(&bsp.faces[face])) else { continue };
            for ty in 0..h + 2 * PAD {
                for tx in 0..w + 2 * PAD {
                    // Clamp into the face's luxels so the border repeats the edge.
                    let sx = tx.saturating_sub(PAD).min(w - 1);
                    let sy = ty.saturating_sub(PAD).min(h - 1);
                    let c = lux[sy * w + sx];
                    let o = layer_offset + ((py + ty) * width + px + tx) * 4;
                    texels[o] = f32_to_f16(c[0]);
                    texels[o + 1] = f32_to_f16(c[1]);
                    texels[o + 2] = f32_to_f16(c[2]);
                    texels[o + 3] = one;
                }
            }
        }
        }
        Self {
            width,
            height,
            texels,
            rects,
            white: (2.0, 2.0),
        }
    }

    /// Atlas UV for a luxel coordinate (relative to the face's lightmap mins).
    pub fn uv(&self, face: usize, s: f32, t: f32) -> [f32; 2] {
        match self.rects.get(&face) {
            Some(&(x, y)) => [
                (x as f32 + s + 0.5) / self.width as f32,
                (y as f32 + t + 0.5) / self.height as f32,
            ],
            None => [self.white.0 / self.width as f32, self.white.1 / self.height as f32],
        }
    }

    pub fn to_image(&self) -> Image {
        let bytes: Vec<u8> = self.texels.iter().flat_map(|h| h.to_le_bytes()).collect();
        let mut img = Image::new(
            Extent3d {
                width: self.width as u32,
                height: self.height as u32,
                depth_or_array_layers: 4,
            },
            TextureDimension::D2,
            bytes,
            TextureFormat::Rgba16Float,
            RenderAssetUsages::RENDER_WORLD,
        );
        img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            ..Default::default()
        });
        img
    }
}

#[derive(Default)]
struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uv0: Vec<[f32; 2]>,
    uv1: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    fn push(&mut self, pos: glam::Vec3, normal: glam::Vec3, uv0: [f32; 2], uv1: [f32; 2], alpha: f32) -> u32 {
        let i = self.positions.len() as u32;
        self.positions.push(to_bevy(pos).to_array());
        self.normals.push(to_bevy(normal).to_array());
        self.uv0.push(uv0);
        self.uv1.push(uv1);
        self.colors.push([1.0, 1.0, 1.0, alpha]);
        i
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uv0);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.uv1);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_indices(Indices::U32(self.indices));
        if let Err(e) = mesh.generate_tangents() { warn!("world tangents: {e}"); }
        mesh
    }
}

fn texture_uv(ti: &bsp::TexInfo, p: glam::Vec3, w: f32, h: f32) -> [f32; 2] {
    let tv = &ti.texture_vecs;
    [
        (tv[0].truncate().dot(p) + tv[0].w) / w,
        (tv[1].truncate().dot(p) + tv[1].w) / h,
    ]
}

const SKIP_SURF: i32 = surf::NODRAW | surf::SKY | surf::SKY2D | surf::TRIGGER | surf::HINT | surf::SKIP;

/// Brush models to draw: the world, plus brush entities with a `*N` model.
fn visible_models(bsp: &Bsp) -> Vec<(usize, glam::Vec3)> {
    let mut out = vec![(0usize, glam::Vec3::ZERO)];
    for ent in &bsp.entities {
        let class = ent.str("classname").unwrap_or("");
        if class.starts_with("trigger_") || class == "func_buyzone" || class == "func_bomb_target" {
            continue;
        }
        let Some(model) = ent.str("model").and_then(|m| m.strip_prefix('*')).and_then(|n| n.parse::<usize>().ok()) else {
            continue;
        };
        if model == 0 {
            continue;
        }
        // Mode 10 is an authored nonvisual brush (for example a lighting
        // helper behind shutters). Keep its collision, but do not draw it.
        if ent.i64("rendermode") == Some(10) {
            continue;
        }
        // Brushes a game mode enables later (Retakes walls) start hidden.
        if ent.i64("startdisabled") == Some(1) {
            continue;
        }
        let origin = ent.str("origin").and_then(bsp::parse_origin).unwrap_or(glam::Vec3::ZERO);
        out.push((model, origin));
    }
    out
}

pub struct WorldBuild {
    pub meshes: Vec<(String, Mesh)>,
    pub triangles: usize,
}

pub struct OverlayBuild {
    pub meshes: Vec<(String, u8, Mesh)>,
    pub triangles: usize,
    pub skipped: usize,
}

/// Authored overlays stay separate from opaque world batches: their alpha,
/// receiver lightmaps, and coplanar render order must survive material reuse.
pub fn build_overlays(bsp: &Bsp, atlas: &LightmapAtlas, include: &dyn Fn(usize) -> bool) -> OverlayBuild {
    let mut out = OverlayBuild { meshes: Vec::new(), triangles: 0, skipped: 0 };
    let Some(world) = bsp.models.first() else { return out };
    let Ok(first) = usize::try_from(world.first_face) else { return out };
    let Ok(count) = usize::try_from(world.num_faces) else { return out };
    let Some(end) = first.checked_add(count) else { return out };
    let mut vertices = 0;
    for overlay in &bsp.overlays {
        if !overlay.faces.iter().any(|&i| i >= 0 && include(i as usize)) { continue; }
        let Some(name) = usize::try_from(overlay.texinfo).ok().and_then(|i| bsp.texinfo_material(i)) else { out.skipped += 1; continue };
        // Moving brush overlays need entity transforms, which are not supported
        // by this initial static-world implementation.
        if overlay.faces.iter().any(|&i| i < 0 || !(first..end).contains(&(i as usize))) {
            out.skipped += 1; continue;
        }
        let receiver = |i| include(i) && bsp.faces.get(i)
            .and_then(|f| usize::try_from(f.texinfo).ok())
            .and_then(|t| bsp.texinfo.get(t))
            .is_some_and(|t| t.flags & SKIP_SURF == 0);
        let projected = match bsp.overlay_mesh(overlay, &receiver) {
            Ok(mesh) => mesh,
            Err(error) => { warn!("overlay {} skipped: {error:?}", overlay.id); out.skipped += 1; continue; }
        };
        if projected.indices.is_empty() { continue; }
        vertices += projected.vertices.len();
        if vertices > 524_288 {
            warn!("overlay map vertex budget reached");
            out.skipped += 1; break;
        }
        let mut mb = MeshBuilder::default();
        for v in projected.vertices {
            mb.push(v.position, v.normal, v.uv.to_array(), atlas.uv(v.face, v.luxel.x, v.luxel.y), 1.0);
        }
        out.triangles += projected.indices.len() / 3;
        mb.indices = projected.indices;
        out.meshes.push((name.to_ascii_lowercase(), overlay.render_order, mb.into_mesh()));
    }
    out
}

/// Overlay content goes through the same mounted VFS and texture cache as the
/// map, including private packs and the BSP's embedded ZIP. Missing or unsupported
/// materials are skipped instead of drawing replacement art over the receiver.
pub fn overlay_material(name: &str, order: u8, fs: &vfs::Vfs, textures: &mut TextureCache, images: &mut Assets<Image>, shared: &SharedHandles, exposure: f32) -> Option<WorldMaterial> {
    let mat = vmt::Material::load(name, &|p| fs.read_string(p))?;
    if !matches!(mat.shader.as_str(), "lightmappedgeneric" | "unlitgeneric") { return None; }
    let base = textures.load(fs, images, mat.base_texture()?, true)?;
    let mut out = material_from_vmt(Some(&mat), fs, textures, images, shared, exposure, true);
    // Projected decals can author coverage in the VTF without $translucent
    // (for example an oil stain). Retain explicit blend/cutout modes and alpha
    // used for another purpose; never infer opacity on ordinary world/props.
    if mat.flag("$decal") && base.has_alpha && out.alpha_mode == AlphaMode::Opaque
        && !mat.flag("$selfillum") && !mat.flag("$basealphaenvmapmask")
    {
        out.params.flags |= flags::TRANSLUCENT;
        out.alpha_mode = AlphaMode::Blend;
    }
    out.params.flags |= flags::OVERLAY | (u32::from(order & 3) << flags::OVERLAY_ORDER_SHIFT);
    Some(out)
}

/// Faces that belong to the 3D skybox area: faces listed by its leaves,
/// displacements whose surface lies in it, and brush-entity faces whose
/// model centre lies in it.
pub fn sky_faces(bsp: &Bsp, sky: &bsp::SkyCamera) -> std::collections::HashSet<usize> {
    let mut set = bsp.faces_in_area(sky.area);
    for (i, face) in bsp.faces.iter().enumerate() {
        if face.dispinfo < 0 {
            continue;
        }
        let Some(d) = bsp.displacement(i) else { continue };
        let center = d.positions.iter().copied().sum::<glam::Vec3>() / d.positions.len().max(1) as f32;
        let n = bsp.face_normal(face).unwrap_or(glam::Vec3::Z);
        if bsp.point_area(center + n * 4.0) == sky.area {
            set.insert(i);
        }
    }
    for (model_index, origin) in visible_models(bsp).into_iter().skip(1) {
        let Some(m) = bsp.models.get(model_index) else { continue };
        let center = (m.mins + m.maxs) * 0.5 + origin;
        if bsp.point_area(center) == sky.area {
            let first = m.first_face.max(0) as usize;
            set.extend(first..first + m.num_faces.max(0) as usize);
        }
    }
    set
}

/// Builds batched world meshes keyed by material name (lowercase), for the
/// faces `include` accepts.
pub fn build(bsp: &Bsp, atlas: &LightmapAtlas, include: &dyn Fn(usize) -> bool) -> WorldBuild {
    build_models(bsp, atlas, include, visible_models(bsp))
}

/// Same authored topology, UV/lightmaps and materials as world batches, but
/// isolated by brush entity for breakable lifecycle or authored window fade.
pub fn build_brush_model(bsp: &Bsp, atlas: &LightmapAtlas, model: usize, origin: glam::Vec3) -> WorldBuild {
    build_models(bsp, atlas, &|_| true, vec![(model, origin)])
}

fn build_models(bsp: &Bsp, atlas: &LightmapAtlas, include: &dyn Fn(usize) -> bool,
    models: Vec<(usize, glam::Vec3)>) -> WorldBuild {
    let mut groups: HashMap<(String, [i32; 3]), MeshBuilder> = HashMap::new();
    let mut triangles = 0usize;

    for (model_index, origin) in models {
        let Some(model) = bsp.models.get(model_index) else { continue };
        let first = model.first_face.max(0) as usize;
        let count = model.num_faces.max(0) as usize;
        for face_index in first..(first + count).min(bsp.faces.len()) {
            if !include(face_index) {
                continue;
            }
            let face = &bsp.faces[face_index];
            let Some(ti) = bsp.texinfo.get(face.texinfo.max(0) as usize) else { continue };
            if face.texinfo < 0 || ti.flags & SKIP_SURF != 0 {
                continue;
            }
            let Some(td) = bsp.texdata.get(ti.texdata.max(0) as usize) else { continue };
            let name = bsp.texinfo_material(face.texinfo as usize).unwrap_or("").to_ascii_lowercase();
            if name.starts_with("tools/") {
                continue;
            }
            let (tw, th) = (td.width.max(1) as f32, td.height.max(1) as f32);
            let planar = if face.dispinfo < 0 {
                match bsp.face_mesh(face) {
                    Ok(mesh) => Some(mesh),
                    Err(error) => { warn!("world face {face_index}: {error}"); continue; }
                }
            } else { None };
            let displacement_bounds;
            let verts_for_bounds = if let Some(mesh) = &planar {
                &mesh.positions
            } else {
                displacement_bounds = bsp.face_vertices(face);
                &displacement_bounds
            };
            let center = verts_for_bounds.iter().copied().sum::<glam::Vec3>() / verts_for_bounds.len().max(1) as f32 + origin;
            let mb = groups.entry((name, spatial_cell(center))).or_default();

            if face.dispinfo >= 0 {
                let Some(d) = bsp.displacement(face_index) else { continue };
                // Smooth normals from the displaced surface.
                let mut normals = vec![glam::Vec3::ZERO; d.positions.len()];
                for tri in d.indices.chunks_exact(3) {
                    let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                    let n = (d.positions[b] - d.positions[a]).cross(d.positions[c] - d.positions[a]);
                    normals[a] += n;
                    normals[b] += n;
                    normals[c] += n;
                }
                let base = mb.positions.len() as u32;
                for i in 0..d.positions.len() {
                    let lc = d.luxel_coords[i];
                    mb.push(
                        d.positions[i] + origin,
                        normals[i].normalize_or_zero(),
                        texture_uv(ti, d.base_positions[i], tw, th),
                        atlas.uv(face_index, lc.x, lc.y),
                        d.alphas[i],
                    );
                }
                mb.indices.extend(d.indices.iter().map(|i| base + i));
                triangles += d.indices.len() / 3;
                continue;
            }

            let Some(planar) = planar else { continue };
            let verts = planar.positions;
            let Some(normal) = bsp.face_normal(face) else { continue };
            let lv = &ti.lightmap_vecs;
            let base = mb.positions.len() as u32;
            for v in &verts {
                let s = lv[0].truncate().dot(*v) + lv[0].w - face.lightmap_mins[0] as f32;
                let t = lv[1].truncate().dot(*v) + lv[1].w - face.lightmap_mins[1] as f32;
                mb.push(*v + origin, normal, texture_uv(ti, *v, tw, th), atlas.uv(face_index, s, t), 0.0);
            }
            triangles += planar.indices.len() / 3;
            mb.indices.extend(planar.indices.into_iter().map(|i| base + i));
        }
    }

    let meshes = groups
        .into_iter()
        .filter(|(_, mb)| !mb.indices.is_empty())
        .map(|((name, _), mb)| (name, mb.into_mesh()))
        .collect();
    WorldBuild { meshes, triangles }
}

#[cfg(test)]
mod compiled_topology_tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn invisible_brushes_keep_metadata_but_never_enter_visual_batches() {
        let mut bsp = Bsp::default();
        bsp.entities = [
            r#"classname func_brush model *1 origin "1 2 3" rendermode 0"#,
            r#"classname func_brush model *2 origin "4 5 6" rendermode 10"#,
            r#"classname func_brush model *3 origin "7 8 9" StartDisabled 1"#,
            r#"classname func_brush model *4 origin "10 11 12""#,
        ].into_iter().map(|text| kv::parse(text).unwrap()).collect();
        assert_eq!(visible_models(&bsp), vec![
            (0, glam::Vec3::ZERO),
            (1, glam::Vec3::new(1., 2., 3.)),
            (4, glam::Vec3::new(10., 11., 12.)),
        ]);
        // Visual selection must not remove the entity from the map that owns
        // collision, outputs and authored lighting metadata.
        assert_eq!(bsp.entities[1].str("model"), Some("*2"));
        assert_eq!(bsp.entities.len(), 4);
    }

    #[test]
    fn renderer_keeps_compiled_boundary_split_and_original_uv_mapping() {
        let mut bsp = Bsp::default();
        bsp.planes.push(bsp::Plane {
            normal: glam::Vec3::Z,
            dist: 0.0,
            kind: 2,
        });
        bsp.vertices = vec![
            glam::Vec3::ZERO,
            glam::Vec3::X,
            glam::Vec3::X * 2.0,
            glam::Vec3::new(2., 2., 0.),
            glam::Vec3::Y * 2.0,
        ];
        bsp.edges = vec![[0, 1], [1, 2], [2, 3], [3, 4], [4, 0]];
        bsp.surfedges = vec![0, 1, 2, 3, 4];
        bsp.texdata.push(bsp::TexData {
            reflectivity: glam::Vec3::ONE,
            name_id: 0,
            width: 2,
            height: 2,
        });
        bsp.texdata_strings.push("synthetic/boundary".into());
        bsp.texinfo.push(bsp::TexInfo {
            texture_vecs: [glam::Vec4::X, glam::Vec4::Y],
            lightmap_vecs: [glam::Vec4::X, glam::Vec4::Y],
            flags: 0,
            texdata: 0,
        });
        bsp.faces.push(bsp::Face {
            plane: 0,
            side: 0,
            first_edge: 0,
            num_edges: 5,
            texinfo: 0,
            dispinfo: -1,
            styles: [255; 4],
            light_offset: -1,
            lightmap_mins: [0; 2],
            lightmap_size: [0; 2],
            num_prims: 1,
            first_prim_id: 0,
        });
        bsp.models.push(bsp::Model {
            mins: glam::Vec3::ZERO,
            maxs: glam::Vec3::splat(2.),
            origin: glam::Vec3::ZERO,
            head_node: 0,
            first_face: 0,
            num_faces: 1,
        });
        bsp.primitives.push(bsp::Primitive {
            kind: 0,
            first_index: 0,
            index_count: 9,
            first_vertex: 0,
            vertex_count: 0,
        });
        bsp.primitive_indices = vec![0, 1, 4, 1, 3, 4, 1, 2, 3];
        let built = build(&bsp, &LightmapAtlas::build(&bsp), &|_| true);
        assert_eq!(built.triangles, 3);
        assert_eq!(built.meshes.len(), 1);
        let mesh = &built.meshes[0].1;
        assert_eq!(
            mesh.indices().unwrap().iter().collect::<Vec<_>>(),
            vec![0, 1, 4, 1, 3, 4, 1, 2, 3]
        );
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("missing normals")
        };
        assert!(normals.iter().all(|n| *n == [0., 1., 0.]));
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("missing UVs")
        };
        assert_eq!(uv[1], [0.5, 0.]);
        // Breakable batches retain the compiled mesh, including its UVs and
        // lightmap addressing, while moving all vertices by the entity origin.
        let isolated = build_brush_model(&bsp, &LightmapAtlas::build(&bsp), 0, glam::Vec3::X * 100.0);
        assert_eq!(isolated.triangles, built.triangles);
        let isolated = &isolated.meshes[0].1;
        assert_eq!(isolated.indices().unwrap().iter().collect::<Vec<_>>(), mesh.indices().unwrap().iter().collect::<Vec<_>>());
        assert_eq!(isolated.attribute(Mesh::ATTRIBUTE_UV_0), mesh.attribute(Mesh::ATTRIBUTE_UV_0));
        assert_eq!(isolated.attribute(Mesh::ATTRIBUTE_UV_1), mesh.attribute(Mesh::ATTRIBUTE_UV_1));
        let Some(VertexAttributeValues::Float32x3(shifted)) = isolated.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!("missing positions") };
        let Some(VertexAttributeValues::Float32x3(original)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!("missing positions") };
        assert!(shifted.iter().zip(original).all(|(a, b)| (Vec3::from_array(*a) - Vec3::from_array(*b) - Vec3::X * 100.0).length() < 0.001));
        // A bad explicit index must not turn back into the old fan.
        bsp.primitive_indices[0] = 5;
        let rejected = build(&bsp, &LightmapAtlas::build(&bsp), &|_| true);
        assert_eq!(rejected.triangles, 0);
        assert!(rejected.meshes.is_empty());
    }
}

/// Creates the material for a world surface.
pub fn world_material(
    name: &str,
    fs: &vfs::Vfs,
    textures: &mut TextureCache,
    images: &mut Assets<Image>,
    shared: &SharedHandles,
    exposure: f32,
) -> WorldMaterial {
    let read = |p: &str| fs.read_string(p);
    let mat = vmt::Material::load(name, &read);
    material_from_vmt(mat.as_ref(), fs, textures, images, shared, exposure, true)
}

/// GPU resources every [`WorldMaterial`] binds.
#[derive(Clone)]
pub struct SharedHandles {
    pub lightmap: Handle<Image>,
}

/// Shared VMT → [`WorldMaterial`] translation for world and props.
/// `lightmapped` distinguishes BSP surfaces from models; the sky caller uses
/// `false` but always supplies a parsed VMT, so it never uses the absent-VMT path.
pub fn material_from_vmt(
    mat: Option<&vmt::Material>,
    fs: &vfs::Vfs,
    textures: &mut TextureCache,
    images: &mut Assets<Image>,
    shared: &SharedHandles,
    exposure: f32,
    lightmapped: bool,
) -> WorldMaterial {
    let mut params = WorldParams {
        exposure,
        ..Default::default()
    };
    let mut out = WorldMaterial {
        params,
        base: textures.missing.clone(),
        base2: textures.white.clone(),
        lightmap: shared.lightmap.clone(),
        modulate: textures.black.clone(),
        normal: textures.flat_normal.clone(),
        normal2: textures.flat_normal.clone(),
        prop_light: textures.white.clone(),
        phong_map: textures.white.clone(),
        skin_surface:textures.white.clone(),
        skin_position:textures.white.clone(),
        detail: textures.white.clone(),
        self_illum_mask: textures.black.clone(),
        alpha_mode: AlphaMode::Opaque,
        cull: true,
    };
    let Some(mat) = mat else {
        // The measured Legacy unresolved-model reference uses ___error /
        // UnlitGeneric and appears black. Reuse the neutral black texture for
        // that empirical model fallback; this does not reconstruct the native
        // runtime texture.
        // BSP surfaces retain their diagnostic checker. A missing texture in a
        // valid VMT follows the separate authored path below; strict missing-VMT
        // probes remain independent of this presentation choice.
        if !lightmapped {
            out.base = textures.black.clone();
        }
        out.params.flags = flags::UNLIT;
        return out;
    };
    if matches!(mat.shader.as_str(), "lightmappedgeneric" | "worldvertextransition" | "vertexlitgeneric" | "unlittwotexture") {
        // Transform only the primary color sampler, never another authored UV
        // channel. The shaders retain raw base UVs for dynamic/cosmetic paths.
        params.base_uv = authored_uv_transform(mat, "$basetexturetransform");
    }
    let base_name = mat
        .base_texture()
        .or_else(|| mat.str("$hdrcompressedtexture"))
        .or_else(|| mat.str("$hdrbasetexture"));
    if let Some(info) = base_name.and_then(|n| textures.load(fs, images, n, true)) {
        out.base = info.handle;
    } else if base_name.is_none() {
        out.base = textures.white.clone();
    }
    let mut f = 0u32;
    if lightmapped && !mat.is_unlit() {
        f |= flags::LIGHTMAPPED;
    } else if !lightmapped {
        f |= flags::VERTEX_LIT;
    }
    if mat.is_unlit() {
        f |= flags::UNLIT;
    }
    if mat.shader == "unlittwotexture" {
        // Reuse the secondary color binding without enabling terrain blending.
        // Both transforms start at raw mesh UVs, independently of each other.
        if let Some(name) = mat.unlit_texture2() {
            if let Some(info) = textures.load(fs, images, name, true) {
                out.base2 = info.handle;
                f |= flags::UNLIT_MULTIPLY;
                params.base2_uv = authored_uv_transform(mat, "$texture2transform");
            } else {
                warn!("UnlitTwoTexture second texture unavailable; retaining base material: {name}");
            }
        }
        // Static authored transforms remain useful without a proxy evaluator.
        // Do not invent time, scroll scale or phase from a snapshot of a VMT.
        if mat.params.get_all("proxies").any(references_texture_transform) {
            warn!("UnlitTwoTexture texture-transform proxy unsupported; retaining static authored UVs");
        }
    } else if let Some(info) = mat.base_texture2().and_then(|n| textures.load(fs, images, n, true)) {
        out.base2 = info.handle;
        f |= flags::BLEND;
        params.base2_uv = authored_uv_transform(mat, "$basetexturetransform2");
        if let Some(m) = mat
            .str("$blendmodulatetexture")
            .and_then(|n| textures.load(fs, images, n, false))
        {
            out.modulate = m.handle;
            f |= flags::BLEND_MODULATE;
            params.modulate_uv = authored_uv_transform(mat, "$blendmodulatetransform");
        }
    }
    if let Some(info) = mat.str("$bumpmap").and_then(|n| textures.load(fs, images, n, false)) {
        out.normal = info.handle;
        f |= flags::NORMAL;
        params.normal_uv = authored_uv_transform(mat, "$bumptransform");
        if mat.flag("$ssbump") { f |= flags::SSBUMP; }
        if mat.flag("$ssbumpmathfix") { f |= flags::SSBUMP_MATHFIX; }
    }
    if let Some(info) = mat.str("$bumpmap2").and_then(|n| textures.load(fs, images, n, false)) {
        out.normal2 = info.handle;
        f |= flags::NORMAL2;
        params.normal2_uv = authored_uv_transform(mat, "$bumptransform2");
    }
    if !lightmapped && mat.flag("$phong") {
        params.phong=Vec4::new(mat.f32("$phongexponent").unwrap_or(20.0).max(1.0),mat.f32("$phongboost").unwrap_or(1.0),0.0,if mat.flag("$basemapalphaphongmask"){1.0}else{0.0});
        if let Some(info)=mat.str("$phongexponenttexture").and_then(|n|textures.load(fs,images,n,false)) {
            out.phong_map=info.handle;params.phong.z=1.0;
        }
        let tint=mat.color("$phongtint").unwrap_or([1.0;3]);
        params.phong_tint=Vec4::new(tint[0],tint[1],tint[2],if mat.flag("$phongalbedotint"){1.0}else{0.0});
        let fresnel=mat.color("$phongfresnelranges").unwrap_or([0.0,0.5,1.0]);
        params.fresnel=Vec4::new(fresnel[0],fresnel[1],fresnel[2],0.0);
        params.rim=Vec4::new(mat.f32("$rimlightexponent").unwrap_or(4.0).max(1.0),if mat.flag("$rimlight"){mat.f32("$rimlightboost").unwrap_or(1.0)}else{0.0},if mat.flag("$rimmask"){1.0}else{0.0},if mat.flag("$halflambert")&&!mat.flag("$phongdisablehalflambert"){1.0}else{0.0});
    }
    if mat.alpha_test() {
        f |= flags::ALPHA_TEST;
        params.alpha_ref = mat.alpha_test_ref();
        out.alpha_mode = AlphaMode::Mask(params.alpha_ref);
    } else if mat.translucent() {
        f |= flags::TRANSLUCENT;
        out.alpha_mode = if mat.additive() { AlphaMode::Add } else { AlphaMode::Blend };
    }
    let debug_no_cull = std::env::var("CSR_NO_CULL").ok();
    let debug_no_cull = match debug_no_cull.as_deref() {
        Some("world") => lightmapped,
        Some("props") => !lightmapped,
        Some(_) => true,
        None => false,
    };
    if mat.no_cull() || debug_no_cull {
        out.cull = false;
    }
    let tint = mat.color("$color").or_else(|| mat.color("$color2")).unwrap_or([1.0; 3]);
    params.color = Vec4::new(tint[0], tint[1], tint[2], mat.alpha());
    if mat.shader == "vertexlitgeneric" && mat.flag("$blendtintbybasealpha") {
        f |= flags::TINT_BY_BASE_ALPHA;
    }
    match mat.detail_layer() {
        Ok(Some(detail)) if detail.factor > 0.0
            && (mat.shader != "worldvertextransition" || lightmapped) => {
            if let Some(info) = textures.load(fs, images, detail.texture, false) {
                out.detail = info.handle;
                params.detail = Vec4::new(detail.scale[0], detail.scale[1], detail.factor, 0.0);
                f |= flags::DETAIL;
                debug!("Authored ordinary detail: shader={} flags={f} texture={} scale={},{} factor={}", mat.shader, detail.texture, detail.scale[0], detail.scale[1], detail.factor);
            } else {
                warn!("authored detail texture unavailable; retaining base material: {}", detail.texture);
            }
        }
        Ok(None) | Ok(Some(_)) => {}
        Err(error) => warn!("unsupported authored detail layer ({error:?}); retaining base material"),
    }
    let env_alpha_illum = mat.shader == "vertexlitgeneric" && mat.flag("$selfillum_envmapmask_alpha");
    if mat.flag("$selfillum") || env_alpha_illum {
        // Legacy's Nuke window skins author emission through the env-mask alpha
        // without a separate $selfillum switch. Keep its RGB reflection mask
        // independent from the alpha emission mask. Only static opaque
        // composition is admitted here; animated/angle-dependent inputs remain
        // on the existing diffuse fallback.
        let supported = f & (flags::UNLIT | flags::ALPHA_TEST | flags::TRANSLUCENT | flags::BLEND) == 0
            && mat.shader != "worldvertextransition"
            && (!mat.flag("$selfillum_envmapmask_alpha") || env_alpha_illum
                && mat.params.get("$selfillummask").is_none()
                && mat.params.get("$envmapmasktransform").is_none()
                && mat.params.get("proxies").is_none())
            && !mat.flag("$selfillumfresnel")
            && mat.params.get("$selfillummaskscale").is_none()
            && !mat.params.get_all("proxies").any(references_self_illum);
        // An absent tint defaults to white; an explicit rejected value must
        // retain the previous diffuse path rather than inventing a tint.
        let tint = match mat.params.get("$selfillumtint") {
            None => Some([1.0; 3]),
            Some(_) => mat.color("$selfillumtint"),
        }.filter(|rgb| rgb.iter().all(|v| v.is_finite() && *v >= 0.0));
        if let (true, Some(tint)) = (supported, tint) {
            let mode = if env_alpha_illum {
                if let Some(mask) = mat.str("$envmapmask")
                    .and_then(|name| textures.load(fs, images, name, false)) {
                    out.self_illum_mask = mask.handle;
                    3.0
                } else {
                    warn!("authored env-alpha emission mask unavailable; retaining lit material");
                    0.0
                }
            } else if let Some(name) = mat.str("$selfillummask") {
                if let Some(mask) = textures.load(fs, images, name, false) {
                    out.self_illum_mask = mask.handle;
                    2.0
                } else {
                    warn!("authored self-illumination mask unavailable; retaining lit material: {name}");
                    0.0
                }
            } else { 1.0 };
            params.self_illum = Vec4::new(tint[0], tint[1], tint[2], mode);
        } else {
            warn!("unsupported authored self-illumination mode/tint/composition; retaining previous diffuse material");
        }
    }
    if !lightmapped {
        match scope_lens_fallback(mat) {
            Some(ScopeLensFallback::AdditiveReflection) => {
                // Installed AUG/SG glass has NO diffuse/emissive contribution.
                // The mask modulates an environment reflection; drawing the
                // mask as colour made a flat grey/pink disc when unscoped.
                // As with Scout glass, an unavailable cubemap contributes no
                // light. Keep the authored dirt layer and opaque housing.
                out.base = textures.black.clone();
                let tint = mat.color("$envmaptint").unwrap_or([0.0; 3]);
                params.color = Vec4::new(tint[0], tint[1], tint[2], mat.alpha());
            }
            Some(ScopeLensFallback::FramebufferReticle) => {
                // scope_dot_white encodes a signed coverage field around
                // middle grey, not ordinary diffuse RGBA. Its native
                // $crosshairmode combines that field with a framebuffer.
                // The accepted scoped UI dot already decodes this asset and
                // clips it to the posed lens. Do not also draw its grey field
                // on the model when the weapon is unscoped.
                params.color = Vec4::ZERO;
            }
            Some(ScopeLensFallback::ScoutRear) => {
                // The installed SSG08 MDL actually binds snip_ssg08/glass,
                // not shared/scope/scope_ssg08. Its scalar "$color" "0"
                // is rejected by color() (which expects three channels), so
                // the generic path previously added a white, sky-lit disc.
                // This reflection-only material has zero diffuse contribution.
                // Until cubemaps are implemented, black contributes nothing
                // through additive blending; do not fabricate a reflection.
                out.base = textures.black.clone();
                params.color = Vec4::new(0.0, 0.0, 0.0, mat.alpha());
            }
            None => {}
        }
    }
    params.flags = f;
    out.params = params;
    out
}

#[derive(Debug, PartialEq)]
enum ScopeLensFallback { AdditiveReflection, FramebufferReticle, ScoutRear }

fn scope_lens_fallback(mat: &vmt::Material) -> Option<ScopeLensFallback> {
    let path_is = |key, expected| mat.str(key).is_some_and(|path|
        path.replace('\\', "/").eq_ignore_ascii_case(expected));
    if mat.shader == "unlitgeneric" && mat.additive() && mat.base_texture().is_none()
        && path_is("$envmap", "env_cubemap")
        && path_is("$envmapmask", "models/weapons/shared/scope/scope_lens_reflect_mask") {
        Some(ScopeLensFallback::AdditiveReflection)
    } else if mat.shader == "unlittwotexture" && mat.translucent() && mat.flag("$crosshairmode")
        && path_is("$basetexture", "models/weapons/shared/scope/scope_dot_white")
        && path_is("$texture2", "_rt_poweroftwofb") {
        Some(ScopeLensFallback::FramebufferReticle)
    } else if mat.shader == "vertexlitgeneric" && mat.additive() && mat.base_texture().is_none()
        && path_is("$envmap", "env_cubemap")
        && mat.str("$color").and_then(|color| color.trim().parse::<f32>().ok()) == Some(0.0)
        && mat.color("$envmaptint") == Some([0.1; 3]) {
        Some(ScopeLensFallback::ScoutRear)
    } else { None }
}

/// Proxies can read or write inputs through nested blocks and component
/// references. Leave unrelated proxies alone; their presence is not a veto.
fn references_self_illum(value: &kv::Value) -> bool {
    match value {
        kv::Value::Str(text) => text.to_ascii_lowercase().contains("$selfillum"),
        kv::Value::Block(block) => block.iter().any(|(key, value)| {
            key.to_ascii_lowercase().contains("$selfillum") || references_self_illum(value)
        }),
    }
}

fn references_texture_transform(value: &kv::Value) -> bool {
    match value {
        kv::Value::Str(text) => {
            let text = text.to_ascii_lowercase();
            text.contains("$texture2transform") || text.contains("$basetexturetransform")
        }
        kv::Value::Block(block) => block.iter().any(|(_, value)| references_texture_transform(value)),
    }
}

fn authored_uv_transform(mat: &vmt::Material, key: &str) -> Vec4 {
    let identity = Vec4::new(1.0, 1.0, 0.0, 0.0);
    let Some(text) = mat.str(key) else { return identity };
    match vmt::AxisAlignedTextureTransform::parse(text) {
        Ok(transform) => Vec4::from_array(transform.scale_offset),
        Err(error) => {
            warn!("unsupported authored {key} transform ({error:?}); retaining identity UVs: {text}");
            identity
        }
    }
}

#[cfg(test)]
mod authored_transform_tests {
    use super::*;

    fn gpu_params(params: &WorldParams) -> Vec<u8> {
        let mut buffer = bevy::render::render_resource::encase::UniformBuffer::new(Vec::new());
        buffer.write(params).unwrap();
        buffer.into_inner()
    }

    fn assert_gpu_base_uv(params: &WorldParams, expected: Vec4) {
        assert_eq!(params.base_uv, expected);
        let bytes = gpu_params(params);
        let serialized: [f32; 4] = std::array::from_fn(|i| {
            f32::from_le_bytes(bytes[544 + i * 4..548 + i * 4].try_into().unwrap())
        });
        assert_eq!(serialized, expected.to_array());
    }

    // One synthetic RGBA texel exercises the real texture-load/material path
    // without an install, filesystem writes, renderer or game asset fixture.
    fn normal_fs() -> vfs::Vfs {
        let mut normal = vec![0u8; 84];
        normal[..4].copy_from_slice(b"VTF\0");
        normal[4..8].copy_from_slice(&7u32.to_le_bytes());
        normal[8..12].copy_from_slice(&2u32.to_le_bytes());
        normal[12..16].copy_from_slice(&80u32.to_le_bytes());
        for offset in [16, 18, 24, 63] { // width, height, frames, depth
            normal[offset..offset + 2].copy_from_slice(&1u16.to_le_bytes());
        }
        normal[52..56].copy_from_slice(&(vtf::Format::Rgba8888 as i32).to_le_bytes());
        normal[56] = 1; // one mip, no thumbnail
        normal[57..61].copy_from_slice(&(-1i32).to_le_bytes());
        normal[80..].copy_from_slice(&[128, 128, 255, 64]);
        let mut opaque = normal.clone();
        opaque[52..56].copy_from_slice(&(vtf::Format::Rgb888 as i32).to_le_bytes());
        opaque.truncate(83);
        let mut fs = vfs::Vfs::new();
        fs.push_back("synthetic normal", vfs::Mount::Memory(std::collections::BTreeMap::from([
            ("materials/test/normal.vtf".into(), normal.into()),
            ("materials/test/opaque.vtf".into(), opaque.into()),
        ])));
        fs
    }

    fn normal_material(mat: Option<&vmt::Material>, lightmapped: bool) -> WorldMaterial {
        let fs = normal_fs();
        let mut images = Assets::<Image>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        material_from_vmt(mat, &fs, &mut textures, &mut images, &shared, 1.0, lightmapped)
    }

    #[test]
    fn optic_fallback_does_not_turn_textureless_additive_glass_white() {
        let parse = |text: &str| vmt::Material::parse_with(text, &|_| None, 0).unwrap();
        let glass = parse(r#"UnlitGeneric { "$additive" "1" "$envmap" "env_cubemap"
            "$envmapmask" "models\weapons\shared\scope\scope_lens_reflect_mask"
            "$envmaptint" "[.25 .2 .2]" }"#);
        let fs = normal_fs();
        let mut images = Assets::<Image>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        let out = material_from_vmt(Some(&glass), &fs, &mut textures, &mut images, &shared, 1., false);
        assert_eq!(scope_lens_fallback(&glass), Some(ScopeLensFallback::AdditiveReflection));
        assert_eq!(out.base, textures.black); // unavailable mask must not glow
        assert_eq!(out.params.color, Vec4::new(0.25, 0.2, 0.2, 1.));
        assert_eq!(out.alpha_mode, AlphaMode::Add);
        let world = material_from_vmt(Some(&glass), &fs, &mut textures, &mut images, &shared, 1., true);
        assert_eq!(world.base, textures.white);
        // A present reflection mask must not become a diffuse/emissive disc
        // either. The fixture is synthetic, never game art.
        let mut masked_fs = normal_fs();
        let mask_bytes = masked_fs.read("materials/test/normal.vtf").unwrap();
        masked_fs.push_back("scope fixture", vfs::Mount::Memory(std::collections::BTreeMap::from([
            ("materials/models/weapons/shared/scope/scope_lens_reflect_mask.vtf".into(), mask_bytes.into()),
        ])));
        let mut masked_images = Assets::<Image>::default();
        let mut masked_textures = TextureCache::new(&mut masked_images);
        let masked_shared = SharedHandles { lightmap: masked_textures.white.clone() };
        let present = material_from_vmt(Some(&glass), &masked_fs, &mut masked_textures,
            &mut masked_images, &masked_shared, 1., false);
        assert_eq!(present.base, masked_textures.black);
        for text in [
            r#"UnlitGeneric { "$additive" "1" "$envmap" "env_cubemap" }"#,
            r#"UnlitGeneric { "$basetexture" "models/weapons/shared/scope/scope_lens_dirt" "$translucent" "1" }"#,
            r#"VertexLitGeneric { "$basetexture" "models/weapons/v_models/snip_ssg08/snip_ssg08_scope" "$phong" "1" }"#,
        ] { assert_eq!(scope_lens_fallback(&parse(text)), None); }
    }

    #[test]
    fn framebuffer_optic_dot_never_draws_its_grey_coverage_field_as_glass() {
        let text = r#"UnlitTwoTexture {
            "$basetexture" "models/weapons/shared/scope/scope_dot_white"
            "$texture2" "_rt_poweroftwofb" "$crosshairmode" "1" "$translucent" "1"
        }"#;
        let dot = vmt::Material::parse_with(text, &|_| None, 0).unwrap();
        assert_eq!(scope_lens_fallback(&dot), Some(ScopeLensFallback::FramebufferReticle));
        let out = normal_material(Some(&dot), false);
        assert_eq!(out.params.color, Vec4::ZERO);
        assert_eq!(out.alpha_mode, AlphaMode::Blend);
        assert_eq!(normal_material(Some(&dot), true).params.color, Vec4::ONE);
        let ordinary = vmt::Material::parse_with(&text.replace("\"$crosshairmode\" \"1\"", ""), &|_| None, 0).unwrap();
        assert_eq!(scope_lens_fallback(&ordinary), None);
    }

    #[test]
    fn scout_reflection_only_glass_keeps_authored_zero_diffuse() {
        let text = r#"VertexLitGeneric { "$color" "0" "$additive" "1"
            "$envmap" "env_cubemap" "$envmaptint" "[.1 .1 .1]" }"#;
        let scout = vmt::Material::parse_with(text, &|_| None, 0).unwrap();
        let other = vmt::Material::parse_with(&text.replace("\"0\"", "\"1\""), &|_| None, 0).unwrap();
        assert_eq!(scope_lens_fallback(&scout), Some(ScopeLensFallback::ScoutRear));
        assert_eq!(scope_lens_fallback(&other), None);
        let fs = normal_fs();
        let mut images = Assets::<Image>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        let out = material_from_vmt(Some(&scout), &fs, &mut textures, &mut images, &shared, 1., false);
        assert_eq!(out.base, textures.black);
        assert_eq!(out.params.color, Vec4::new(0., 0., 0., 1.));
        assert_eq!(out.alpha_mode, AlphaMode::Add);
        let unchanged = normal_material(Some(&scout), true);
        assert_eq!(unchanged.params.color, Vec4::ONE);
        // The unbound shared lens and painted housing are not this glass.
        let unused = vmt::Material::parse_with(r#"VertexLitGeneric {
            "$basetexture" "models/weapons/shared/scope/scope"
            "$bumpmap" "models/weapons/shared/scope/scope_normal" "$envmap" "env_cubemap"
            "$phong" "1" "$phongboost" "3" "$phongexponent" "200" "$phongtint" "[.5 1 .7]" }"#, &|_| None, 0).unwrap();
        assert_eq!(scope_lens_fallback(&unused), None);
    }

    #[test]
    fn missing_material_uses_unlit_caller_fallback_without_changing_valid_materials() {
        let fs = normal_fs();
        let mut images = Assets::<Image>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        let valid_base = textures.load(&fs, &mut images, "test/opaque", true).unwrap().handle;
        for lightmapped in [false, true] {
            let missing = material_from_vmt(None, &fs, &mut textures, &mut images, &shared, 7.0, lightmapped);
            assert_eq!(missing.base, if lightmapped { textures.missing.clone() } else { textures.black.clone() });
            assert_eq!(missing.params.flags, flags::UNLIT);
            assert_eq!(missing.params.color, Vec4::ONE);
            assert_eq!(missing.params.base_uv, WorldParams::default().base_uv);
            assert_eq!(missing.alpha_mode, AlphaMode::Opaque);
            assert!(missing.cull);

            let lit_flags = if lightmapped { flags::LIGHTMAPPED } else { flags::VERTEX_LIT };
            let lit_shader = if lightmapped { "LightmappedGeneric" } else { "VertexLitGeneric" };
            for (shader, expected_flags) in [
                (lit_shader, lit_flags),
                ("UnlitGeneric", flags::UNLIT | if lightmapped { 0 } else { flags::VERTEX_LIT }),
            ] {
                for (declaration, expected_base) in [
                    (r#""$basetexture" "test/opaque""#, valid_base.clone()),
                    (r#""$basetexture" "test/absent""#, textures.missing.clone()),
                    ("", textures.white.clone()),
                ] {
                    let valid = vmt::Material::parse_with(&format!("{shader} {{ {declaration} }}"), &|_| None, 0).unwrap();
                    let authored = material_from_vmt(Some(&valid), &fs, &mut textures, &mut images, &shared, 7.0, lightmapped);
                    assert_eq!(authored.base, expected_base, "{shader}: {declaration}");
                    assert_eq!(authored.params.flags, expected_flags, "{shader}: {declaration}");
                    assert_eq!(authored.alpha_mode, AlphaMode::Opaque);
                }
            }
        }
        // A BSP caller that loads an absent VMT also retains the checker.
        let world = world_material("test/absent_material", &fs, &mut textures, &mut images, &shared, 7.0);
        assert_eq!(world.base, textures.missing);
        assert_eq!(world.params.flags, flags::UNLIT);
        assert!(vmt::Material::load("test/absent_material", &|p| fs.read_string(p)).is_none());
        // The absent texture is still tracked, including when served from cache.
        assert_eq!(textures.stats_missing, 1);
    }

    #[test]
    fn unlit_two_texture_binds_authored_mask_and_independent_uvs_without_terrain_blend() {
        let mat = vmt::Material::parse_with(r#"UnlitTwoTexture {
            "$basetexture" "test/normal" "$texture2" "test/opaque"
            "$basetexturetransform" "center .5 .5 scale 2 .5 rotate 0 translate .125 -.25"
            "$texture2transform" "center 0 0 scale 3 4 rotate 0 translate .25 -.5"
            "$basetexture2" "missing/terrain"
            "$basetexturetransform2" "center 0 0 scale 99 99 rotate 0 translate 0 0"
            "$blendmodulatetexture" "test/normal"
            "$nocull" "1" "$additive" "1" "$alpha" ".25" "$color" "[.5 .75 1]"
        }"#, &|_| None, 0).unwrap();
        let fs = normal_fs();
        let mut images = Assets::<Image>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        let base = textures.load(&fs, &mut images, "test/normal", true).unwrap().handle;
        let second = textures.load(&fs, &mut images, "test/opaque", true).unwrap().handle;
        for lightmapped in [true, false] {
            let out = material_from_vmt(Some(&mat), &fs, &mut textures, &mut images, &shared, 1., lightmapped);
            assert_eq!(out.base, base);
            assert_eq!(out.base2, second);
            assert_ne!(out.base, out.base2);
            assert_eq!(out.params.flags & (flags::UNLIT | flags::UNLIT_MULTIPLY), flags::UNLIT | flags::UNLIT_MULTIPLY);
            assert_eq!(out.params.flags & (flags::BLEND | flags::BLEND_MODULATE | flags::LIGHTMAPPED), 0);
            assert_eq!(out.params.color, Vec4::new(0.5, 0.75, 1., 0.25));
            assert_eq!(out.alpha_mode, AlphaMode::Add);
            assert!(!out.cull);
            assert_gpu_base_uv(&out.params, Vec4::new(2., 0.5, -0.375, 0.));
            assert_eq!(out.params.base2_uv, Vec4::new(3., 4., 0.25, -0.5));
            // Verify the existing secondary transform actually reaches its
            // uniform slot; no new binding or uniform schema is introduced.
            let bytes = gpu_params(&out.params);
            let uv2: [f32; 4] = std::array::from_fn(|i| {
                f32::from_le_bytes(bytes[32 + i * 4..36 + i * 4].try_into().unwrap())
            });
            assert_eq!(uv2, [3., 4., 0.25, -0.5]);
            assert_eq!(bytes.len(), 560);
        }
        // The unsupported terrain name above must not even be loaded.
        assert_eq!(textures.stats_missing, 0);
    }

    #[test]
    fn unlit_two_texture_static_fallback_and_other_material_layers_stay_separate() {
        let identity = WorldParams::default().base2_uv;
        for declaration in [
            "",
            r#""$texture2" "missing/mask""#,
            r#""$basetexture2" "test/opaque""#,
            r#""$texture2" { invalid "block" }"#,
        ] {
            let mat = vmt::Material::parse_with(
                &format!(r#"UnlitTwoTexture {{ "$basetexture" "test/normal" {declaration} }}"#),
                &|_| None, 0,
            ).unwrap();
            let out = normal_material(Some(&mat), true);
            assert_eq!(out.params.flags, flags::UNLIT);
            assert_eq!(out.params.base2_uv, identity);
        }
        // Unsupported animated writes retain the authored static transform.
        // Invalid static transforms retain the existing identity fallback.
        for (transform, expected) in [
            ("", identity),
            (r#""$texture2transform" "center 0 0 scale 2 3 rotate 0 translate .25 -.5""#, Vec4::new(2., 3., 0.25, -0.5)),
            (r#""$texture2transform" "center 0 0 scale 2 3 rotate 90 translate 0 0""#, identity),
            (r#""$texture2transform" "scale 2 3""#, identity),
        ] {
            let mat = vmt::Material::parse_with(&format!(r#"UnlitTwoTexture {{
                "$basetexture" "test/normal" "$texture2" "test/opaque" {transform}
                "Proxies" {{ "TextureScroll" {{
                    "texturescrollvar" "$texture2transform" "texturescrollrate" ".03" "texturescrollangle" "-30"
                }} }}
            }}"#), &|_| None, 0).unwrap();
            let out = normal_material(Some(&mat), true);
            assert_eq!(out.params.flags, flags::UNLIT | flags::UNLIT_MULTIPLY);
            assert_eq!(out.params.base2_uv, expected);
            assert!(mat.params.get_all("proxies").any(references_texture_transform));
        }
        let fs = normal_fs();
        let mut images = Assets::<Image>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        for shader in ["WorldVertexTransition", "LightmappedGeneric", "VertexLitGeneric", "UnlitGeneric"] {
            let parse = |extra: &str| vmt::Material::parse_with(&format!(r#"{shader} {{
                "$basetexture" "test/normal" "$basetexture2" "test/opaque"
                "$basetexturetransform2" "center 0 0 scale 2 3 rotate 0 translate .25 -.5"
                "$blendmodulatetexture" "test/normal"
                "$bumpmap" "test/normal" "$bumpmap2" "test/normal"
                "$detail" "test/normal" "$detailscale" "4" "$selfillum" "1" {extra}
            }}"#), &|_| None, 0).unwrap();
            let before = material_from_vmt(Some(&parse("")), &fs, &mut textures, &mut images, &shared, 1., true);
            let after = material_from_vmt(Some(&parse(r#""$texture2" "test/normal"
                "$texture2transform" "center 0 0 scale 99 99 rotate 0 translate 0 0""#)),
                &fs, &mut textures, &mut images, &shared, 1., true);
            assert_eq!(gpu_params(&before.params), gpu_params(&after.params), "{shader}");
            assert_eq!(before.params.flags & flags::UNLIT_MULTIPLY, 0);
            assert_ne!(after.params.flags & flags::BLEND, 0);
            assert_eq!(after.params.base2_uv, Vec4::new(2., 3., 0.25, -0.5));
            assert_eq!(before.base2, after.base2);
            assert_eq!(before.normal, after.normal);
            assert_eq!(before.normal2, after.normal2);
            assert_eq!(before.detail, after.detail);
            assert_eq!(before.self_illum_mask, after.self_illum_mask);
        }
    }

    #[test]
    #[ignore = "requires read-only installed Legacy Cache pakfile; no native process"]
    fn installed_cache_unlit_two_texture_uses_second_mask_and_reports_static_proxy_limit() {
        let level = level::Level::load("de_cache", None).unwrap();
        let mat = vmt::Material::load("fmpone/newcache/nc_beam/vollight", &|p| level.vfs.read_string(p)).unwrap();
        assert_eq!(mat.shader, "unlittwotexture");
        assert_eq!(mat.base_texture(), Some("de_subzero/vol_lightmask02"));
        assert_eq!(mat.unlit_texture2().map(str::to_ascii_lowercase).as_deref(), Some("models/effects/dust01"));
        assert!(mat.params.get_all("proxies").any(references_texture_transform));
        let mut images = Assets::<Image>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        let out = material_from_vmt(Some(&mat), &level.vfs, &mut textures, &mut images, &shared, 1., true);
        assert_eq!(out.params.flags, flags::UNLIT | flags::UNLIT_MULTIPLY | flags::TRANSLUCENT);
        assert_ne!(out.base, textures.missing);
        assert_ne!(out.base2, textures.white);
        assert_ne!(out.base, out.base2);
        assert_eq!(textures.stats_missing, 0);
        assert_gpu_base_uv(&out.params, WorldParams::default().base_uv);
        assert_eq!(out.params.base2_uv, WorldParams::default().base2_uv);
        assert_eq!(out.alpha_mode, AlphaMode::Add);
        assert!(!out.cull);
    }

    #[test]
    fn additive_and_premultiplied_materials_cannot_share_a_pipeline_key() {
        let mat = vmt::Material::parse_with(
            r#"UnlitGeneric { "$basetexture" "test/normal" "$additive" "1" "$alpha" ".25" }"#,
            &|_| None, 0,
        ).unwrap();
        let mut out = normal_material(Some(&mat), false);
        assert_eq!(out.alpha_mode, AlphaMode::Add);
        assert_ne!(out.params.flags & flags::TRANSLUCENT, 0);
        assert_eq!(out.params.color.w, 0.25);
        let additive = crate::material::WorldMaterialKey::from(&out);
        assert!(additive.additive);
        assert!(!additive.depth_write);
        out.alpha_mode = AlphaMode::Premultiplied;
        let premultiplied = crate::material::WorldMaterialKey::from(&out);
        assert!(!premultiplied.additive);
        assert!(!premultiplied.depth_write);
        assert_ne!(additive, premultiplied);
    }

    #[test]
    fn projected_decal_coverage_keeps_explicit_modes_and_other_alpha_uses() {
        for (declaration, mode, translucent) in [
            (r#""$decal" "1""#, AlphaMode::Blend, true),
            (r#""$decal" "0""#, AlphaMode::Opaque, false),
            (r#""$decal" "1" "$basetexture" "test/opaque""#, AlphaMode::Opaque, false),
            (r#""$decal" "1" "$alphatest" "1" "$alphatestreference" ".3""#, AlphaMode::Mask(0.3), false),
            (r#""$decal" "1" "$additive" "1""#, AlphaMode::Add, true),
            (r#""$decal" "1" "$translucent" "1""#, AlphaMode::Blend, true),
            (r#""$decal" "1" "$selfillum" "1""#, AlphaMode::Opaque, false),
            (r#""$decal" "1" "$basealphaenvmapmask" "1""#, AlphaMode::Opaque, false),
        ] {
            let text = format!(r#"LightmappedGeneric {{
                "$basetexture" "test/normal" "$alpha" ".25"
                "$color" "[.5 .75 1]" {declaration}
            }}"#);
            let source = vmt::Material::parse_with(&text, &|_| None, 0).unwrap();
            let ordinary = normal_material(Some(&source), true);
            let mut fs = normal_fs();
            fs.push_back("synthetic decal", vfs::Mount::Memory(std::collections::BTreeMap::from([
                ("materials/test/decal.vmt".into(), text.into_bytes().into()),
            ])));
            let mut images = Assets::<Image>::default();
            let mut textures = TextureCache::new(&mut images);
            let shared = SharedHandles { lightmap: textures.white.clone() };
            let out = overlay_material("test/decal", 2, &fs, &mut textures, &mut images, &shared, 1.).unwrap();
            assert_eq!(out.alpha_mode, mode, "{declaration}");
            assert_eq!(out.params.flags & flags::TRANSLUCENT != 0, translucent);
            assert_eq!(out.params.color, Vec4::new(0.5, 0.75, 1., 0.25));
            assert_ne!(out.params.flags & flags::LIGHTMAPPED, 0);
            assert_eq!(crate::material::WorldMaterialKey::from(&out).overlay_order, Some(2));
            // The inference is confined to projected overlays. The same VMT
            // on an ordinary brush still has its explicit material mode.
            if source.flag("$decal") && !source.translucent() && !source.alpha_test() {
                assert_eq!(ordinary.alpha_mode, AlphaMode::Opaque);
            }
        }
    }

    #[test]
    fn primary_base_centered_mapping_and_unsupported_fallback_reach_gpu_params() {
        let identity = Vec4::new(1., 1., 0., 0.);
        assert_gpu_base_uv(&normal_material(None, true).params, identity);
        for (declaration, expected) in [
            ("", identity),
            (r#""$basetexturetransform" "center 0.5 0.5 scale 10 10 rotate 0 translate 0 0""#, Vec4::new(10., 10., -4.5, -4.5)),
            (r#""$basetexturetransform" "translate 0.125 -0.25 rotate 0 scale 2 0.5 center 0.5 0.5""#, Vec4::new(2., 0.5, -0.375, 0.)),
            (r#""$basetexturetransform" "center 0 0 scale 2 2 rotate 90 translate 0 0""#, identity),
            (r#""$basetexturetransform" "center 0 0 scale -2 2 rotate 0 translate 0 0""#, identity),
            (r#""$basetexturetransform" "center 0 0 scale NaN 2 rotate 0 translate 0 0""#, identity),
            (r#""$basetexturetransform" "[1 0 0 0 0 1 0 0]""#, identity),
            (r#""$basetexturetransform" "scale 2 2""#, identity),
            (r#""$basetexturetransform" { "scale" "2 2" }"#, identity),
        ] {
            for (shader, lightmapped) in [("LightmappedGeneric", true), ("VertexLitGeneric", false)] {
                let mat = vmt::Material::parse_with(
                    &format!(r#""{shader}" {{ "$basetexture" "test/normal" {declaration} }}"#),
                    &|_| None, 0,
                ).unwrap();
                let out = normal_material(Some(&mat), lightmapped);
                assert_gpu_base_uv(&out.params, expected);
                assert_eq!(out.alpha_mode, AlphaMode::Opaque);
            }
        }
        for shader in ["UnlitGeneric", "Water", "WeaponDecal"] {
            let mat = vmt::Material::parse_with(
                &format!(r#""{shader}" {{ "$basetexturetransform" "center 0.5 0.5 scale 10 10 rotate 0 translate 0 0" }}"#),
                &|_| None, 0,
            ).unwrap();
            assert_gpu_base_uv(&normal_material(Some(&mat), false).params, identity);
        }
    }

    #[test]
    fn primary_base_mapping_preserves_every_independently_authored_channel() {
        let parse = |transform: &str| vmt::Material::parse_with(
            &format!(r#""WorldVertexTransition" {{
                "$basetexture" "test/normal"
                "$basetexture2" "test/normal"
                "$basetexturetransform2" "center 0 0 scale 3 4 rotate 0 translate 0.25 -0.5"
                "$bumpmap" "test/normal"
                "$bumptransform" "center 0 0 scale 8 8 rotate 0 translate 0 0"
                "$bumpmap2" "test/normal"
                "$bumptransform2" "center 0.5 0.5 scale 2 0.5 rotate 0 translate 0.125 -0.25"
                "$blendmodulatetexture" "test/normal"
                "$blendmodulatetransform" "center 0 0 scale 0.65 0.65 rotate 0 translate 0 0"
                "$detail" "test/normal" "$detailscale" "[5 6]"
                {transform}
            }}"#), &|_| None, 0,
        ).unwrap();
        let before = normal_material(Some(&parse("")), true).params;
        let params = normal_material(Some(&parse(
            r#""$basetexturetransform" "center 0.5 0.5 scale 2 2 rotate 0 translate 0 0""#,
        )), true).params;
        assert_gpu_base_uv(&params, Vec4::new(2., 2., -0.5, -0.5));
        assert_eq!(params.base2_uv, Vec4::new(3., 4., 0.25, -0.5));
        assert_eq!(params.normal_uv, Vec4::new(8., 8., 0., 0.));
        assert_eq!(params.normal2_uv, Vec4::new(2., 0.5, -0.375, 0.));
        assert_eq!(params.modulate_uv, Vec4::new(0.65, 0.65, 0., 0.));
        assert_eq!(params.detail, Vec4::new(5., 6., 1., 0.));
        // Only the appended primary transform may differ in the GPU buffer.
        assert_eq!(gpu_params(&params)[..544], gpu_params(&before)[..544]);

        let shader: String = include_str!("shaders/world.wgsl")
            .chars().filter(|c| !c.is_ascii_whitespace()).collect();
        assert!(shader.contains("uv*params.base2_uv.xy+params.base2_uv.zw"));
        assert!(shader.contains("letnormal_uv=uv*params.normal_uv.xy+params.normal_uv.zw;"));
        assert!(shader.contains("uv*params.normal2_uv.xy+params.normal2_uv.zw"));
        assert!(shader.contains("uv*params.modulate_uv.xy+params.modulate_uv.zw"));
        assert!(shader.contains("textureSample(detail_tex,detail_samp,uv*params.detail.xy)"));
    }

    #[test]
    fn primary_base_alpha_consumers_share_color_mapping_and_dynamic_paths_keep_raw_uvs() {
        let mat = vmt::Material::parse_with(r#""VertexLitGeneric" {
            "$basetexture" "test/normal" "$blendtintbybasealpha" "1" "$selfillum" "1"
            "$basetexturetransform" "center 0.5 0.5 scale 2 2 rotate 0 translate 0 0"
        }"#, &|_| None, 0).unwrap();
        let params = normal_material(Some(&mat), false).params;
        assert_gpu_base_uv(&params, Vec4::new(2., 2., -0.5, -0.5));
        assert_ne!(params.flags & flags::TINT_BY_BASE_ALPHA, 0);
        assert_eq!(params.self_illum.w, 1.);
        let forward: String = include_str!("shaders/world.wgsl")
            .chars().filter(|c| !c.is_ascii_whitespace()).collect();
        let shadow: String = include_str!("shaders/world_shadow.wgsl")
            .chars().filter(|c| !c.is_ascii_whitespace()).collect();
        for (shader, dynamic) in [(&forward, "DYNAMIC_LIT"), (&shadow, "256u")] {
            assert!(shader.contains(&format!("letbase_uv=select(uv*params.base_uv.xy+params.base_uv.zw,uv,(params.flags&{dynamic})!=0u||params.skin.x>0.5);")));
            assert_eq!(shader.matches("textureSample(base_tex,base_samp,base_uv)").count(), 1);
            assert!(!shader.contains("textureSample(base_tex,base_samp,uv)"));
        }
        assert!(forward.contains("letself_illum_alpha=albedo.a;"));
        assert!(forward.contains("lettint_weight=select(1.0,albedo.a,(params.flags&TINT_BY_BASE_ALPHA)!=0u);"));
        assert!(forward.contains("varmask=vec3<f32>(self_illum_alpha);"));
        // A separately authored RGB mask retains its own existing coordinates.
        assert!(forward.contains("textureSample(self_illum_tex,self_illum_samp,uv).rgb"));
    }

    #[test]
    #[ignore = "requires read-only installed CS:GO Legacy fountain materials; no GPU required"]
    fn installed_inferno_fountain_primary_mapping_reaches_gpu_params() {
        let dir = vfs::find_csgo_dir().expect("CS:GO Legacy install");
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&dir).unwrap();
        let read = |path: &str| fs.read(path).and_then(|bytes| String::from_utf8(bytes).ok());
        let mut images = Assets::<Image>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        for (name, scale) in [("metal", 10.), ("fountain_a_basin", 2.), ("tiles_a2", 6.), ("brick_d1", 1.)] {
            let mat = vmt::Material::load(
                &format!("models/props/de_inferno/hr_i/fountain_a_basin/{name}"), &read,
            ).expect(name);
            assert_eq!(mat.shader, "vertexlitgeneric");
            let out = material_from_vmt(Some(&mat), &fs, &mut textures, &mut images, &shared, 1., false);
            assert_ne!(out.base, textures.missing, "{name}");
            let offset = 0.5 * (1. - scale);
            assert_gpu_base_uv(&out.params, Vec4::new(scale, scale, offset, offset));
            assert_eq!(out.params.base2_uv, Vec4::new(1., 1., 0., 0.));
            assert_eq!(out.params.normal_uv, Vec4::new(1., 1., 0., 0.));
            if name == "fountain_a_basin" {
                assert_ne!(out.params.flags & flags::DETAIL, 0);
                assert_eq!(out.params.detail, Vec4::new(1., 1., 1., 0.));
            }
        }
    }

    #[test]
    fn first_normal_uses_authored_nuke_scale8_independently_of_detail_and_base() {
        // Installed concrete/hr_c/hr_concrete_wall_001's evidenced transform.
        let mat = vmt::Material::parse_with(
            r#""LightmappedGeneric" {
                "$bumpmap" "test/normal"
                "$bumptransform" "center 0 0 scale 8 8 rotate 0 translate 0 0"
                "$detail" "test/normal" "$detailscale" "8"
            }"#,
            &|_| None, 0,
        ).unwrap();
        let params = normal_material(Some(&mat), true).params;
        assert_ne!(params.flags & flags::NORMAL, 0);
        assert_ne!(params.flags & flags::DETAIL, 0);
        assert_eq!(params.normal_uv, Vec4::new(8., 8., 0., 0.));
        assert_eq!(params.detail, Vec4::new(8., 8., 1., 0.));
        assert_eq!(params.base2_uv, WorldParams::default().base2_uv);
        let uv = Vec2::new(-0.25, 1.125);
        let [sx, sy, x, y] = params.normal_uv.to_array();
        assert_eq!(uv * Vec2::new(sx, sy) + Vec2::new(x, y), Vec2::new(-2., 9.));

        // Lock the actual shader wiring: RGB and Phong alpha share the same
        // transformed normal coordinates; color/detail retain independent UVs.
        let shader: String = include_str!("shaders/world.wgsl")
            .chars().filter(|c| !c.is_ascii_whitespace()).collect();
        assert!(shader.contains("letnormal_uv=uv*params.normal_uv.xy+params.normal_uv.zw;"));
        assert_eq!(shader.matches("textureSample(normal_tex,normal_samp,normal_uv)").count(), 2);
        assert!(shader.contains("textureSample(base_tex,base_samp,base_uv)"));
        assert!(shader.contains("textureSample(detail_tex,detail_samp,uv*params.detail.xy)"));
    }

    #[test]
    fn ordinary_world_transition_projects_its_blended_normal_once() {
        // Synthetic opposed slopes cancel to a flat surface. Under the RNM
        // contract, that surface weights each of the orthonormal basis axes
        // equally, independently of which pair of slopes produced it.
        let basis = [
            Vec3::new((2.0f32 / 3.0).sqrt(), 0., (1.0f32 / 3.0).sqrt()),
            Vec3::new(-(1.0f32 / 6.0).sqrt(), 0.5f32.sqrt(), (1.0f32 / 3.0).sqrt()),
            Vec3::new(-(1.0f32 / 6.0).sqrt(), -0.5f32.sqrt(), (1.0f32 / 3.0).sqrt()),
        ];
        let project = |normal: Vec3| {
            let weights = Vec3::from_array(basis.map(|b| normal.dot(b).max(0.).powi(2)));
            weights / weights.element_sum()
        };
        let first = Vec3::new(0.75f32.sqrt(), 0., 0.5);
        let second = Vec3::new(-0.75f32.sqrt(), 0., 0.5);
        let normal = first.lerp(second, 0.5).normalize();
        let weights = project(normal);
        assert!((normal - Vec3::Z).length() < 1e-6);
        assert!((weights - Vec3::splat(1. / 3.)).length() < 1e-6);
        let old_weights = project(first).lerp(project(second), 0.5);
        assert!((old_weights - Vec3::new(0.5, 0.25, 0.25)).length() < 1e-6);
        // It corrects directional response, not brightness: the same correction
        // can increase or decrease light according to its incoming direction.
        assert!(weights.dot(Vec3::new(0.2, 1., 1.)) > old_weights.dot(Vec3::new(0.2, 1., 1.)));
        assert!(weights.dot(Vec3::new(1., 0.2, 0.2)) < old_weights.dot(Vec3::new(1., 0.2, 0.2)));
        for t in [0., 1.] {
            let chosen = first.lerp(second, t).normalize();
            assert!((project(chosen) - project(if t == 0. { first } else { second })).length() < 1e-6);
        }

        // Protect the actual shader data flow as well as the numerical oracle:
        // only an ordinary lightmapped color transition replaces interpolated
        // weights with the projection of the normal it already selected.
        let shader: String = include_str!("shaders/world.wgsl")
            .chars().filter(|c| !c.is_ascii_whitespace()).collect();
        let (_, transition) = shader.split_once("if(params.flags&NORMAL2)!=0u{").unwrap();
        assert!(transition.contains("tangent_normal=normalize(mix(tangent_normal,bump_normal(b2),blend_weight));"));
        assert!(transition.contains("if(params.flags&(LIGHTMAPPED|BLEND|SSBUMP))==(LIGHTMAPPED|BLEND){weights=rnm_weights(tangent_normal);}"));

        let mat = vmt::Material::parse_with(
            r#"WorldVertexTransition {
                "$basetexture2" "test/normal"
                "$bumpmap" "test/normal" "$bumpmap2" "test/normal"
            }"#, &|_| None, 0,
        ).unwrap();
        let flags = normal_material(Some(&mat), true).params.flags;
        let required = flags::LIGHTMAPPED | flags::BLEND | flags::NORMAL | flags::NORMAL2;
        assert_eq!(flags & required, required);
        assert_eq!(flags & flags::SSBUMP, 0);
    }

    #[test]
    fn ssbump_transitions_keep_directional_visibility_and_mathfix() {
        // Visibility is not a normal vector. A shadowed texel must lose energy
        // even when its reconstructed direction matches the unshadowed one.
        let first = Vec3::new(0.15, 0.3, 0.45);
        let second = first * 0.2;
        let mixed = first.lerp(second, 0.5);
        assert!((mixed.element_sum() - 0.54).abs() < 1e-6);
        assert!(mixed.element_sum() < first.element_sum());
        let corrected = mixed * (1.0f32 / 3.0).sqrt();
        assert!((corrected.element_sum() - 0.54 / 3.0f32.sqrt()).abs() < 1e-6);

        let shader: String = include_str!("shaders/world.wgsl")
            .chars().filter(|c| !c.is_ascii_whitespace()).collect();
        assert!(shader.contains("weights=mix(weights,bump_weights(b2),blend_weight);"));
        assert!(shader.contains("if(params.flags&SSBUMP)!=0u{returnsample*select(1.0,0.57735027,(params.flags&SSBUMP_MATHFIX)!=0u);}"));
        let mat = vmt::Material::parse_with(
            r#"WorldVertexTransition {
                "$basetexture2" "test/normal"
                "$bumpmap" "test/normal" "$bumpmap2" "test/normal"
                "$ssbump" "1" "$ssbumpmathfix" "1"
            }"#, &|_| None, 0,
        ).unwrap();
        let flags = normal_material(Some(&mat), true).params.flags;
        assert_eq!(flags & (flags::SSBUMP | flags::SSBUMP_MATHFIX), flags::SSBUMP | flags::SSBUMP_MATHFIX);
    }

    #[test]
    fn terrain_detail_reaches_the_existing_lit_rgb_operator_without_changing_masks() {
        let parse = |detail: &str| vmt::Material::parse_with(
            &format!(r#"WorldVertexTransition {{
                "$basetexture" "test/normal" "$basetexture2" "test/normal"
                "$bumpmap" "test/normal" "$bumpmap2" "test/normal"
                "$blendmodulatetexture" "test/normal" "$alpha" ".75"
                {detail}
            }}"#),
            &|_| None, 0,
        ).unwrap();
        let before = normal_material(Some(&parse("")), true);
        let out = normal_material(Some(&parse(r#"
            "$detail" "test/normal" "$detailscale" "[8 8]"
            "$detailblendfactor" ".5" "$detailblendmode" "0"
        "#)), true);
        assert_eq!(out.params.flags, before.params.flags | flags::DETAIL);
        assert_eq!(out.params.detail, Vec4::new(8., 8., 0.5, 0.));
        assert_eq!(out.params.color, before.params.color);
        assert_eq!(out.alpha_mode, before.alpha_mode);
        assert_eq!(out.base, out.base2);
        assert_eq!(out.normal, out.normal2);
        assert_eq!(out.normal, out.modulate);
        // One VTF used as color and data must not share an sRGB resource.
        assert_ne!(out.detail, out.base);
        assert_eq!(out.detail, out.modulate);

        // The existing operator is RGB-only, after terrain mixing and before
        // lighting. With factor .5, .5 detail remains identity, .25 detail
        // multiplies by .75 and .75 detail by 1.25. This is not a gain fitted
        // to a map or an interpolation of normals/alpha as if they were color.
        let detail_rgb = |rgb: Vec3, grain: Vec3| rgb * (Vec3::ONE * 0.5 + grain);
        let a = Vec3::new(0.2, 0.4, 0.6);
        let b = Vec3::new(0.8, 0.3, 0.1);
        for weight in [0., 0.25, 1.] {
            let composed = a.lerp(b, weight);
            assert_eq!(detail_rgb(composed, Vec3::splat(0.5)), composed);
            assert!((detail_rgb(composed, Vec3::splat(0.25)) - composed * 0.75).length() < 1e-6);
            assert!((detail_rgb(composed, Vec3::splat(0.75)) - composed * 1.25).length() < 1e-6);
        }
        let shader: String = include_str!("shaders/world.wgsl")
            .chars().filter(|c| !c.is_ascii_whitespace()).collect();
        let blend = shader.find("albedo=mix(albedo,second,t);").unwrap();
        let detail = shader.find("letgrain=textureSample(detail_tex,detail_samp,uv*params.detail.xy).rgb;").unwrap();
        let lighting = shader.find("varrgb=albedo.rgb*light;").unwrap();
        assert!(blend < detail && detail < lighting);
        assert!(shader.contains("albedo=vec4<f32>(albedo.rgb*mix(vec3<f32>(1.0),2.0*grain,params.detail.z),albedo.a);"));

        let unsupported = normal_material(Some(&parse(r#""$detail" "test/normal" "$detail2" "test/normal""#)), true);
        assert_eq!(unsupported.params.flags, before.params.flags);
        assert_eq!(unsupported.params.detail, before.params.detail);
        let not_terrain = normal_material(Some(&parse(r#""$detail" "test/normal""#)), false);
        assert_eq!(not_terrain.params.flags & flags::DETAIL, 0);
    }

    #[test]
    fn uniform_detail_vector_preserves_prop_ao_uvs_and_lighting_inputs() {
        let parse = |scale: &str| vmt::Material::parse_with(
            &format!(r#"VertexLitGeneric {{
                "$basetexture" "test/normal" "$bumpmap" "test/normal"
                "$basetexturetransform" "center .5 .5 scale 10 10 rotate 0 translate 0 0"
                "$bumptransform" "center .5 .5 scale 10 10 rotate 0 translate 0 0"
                "$detail" "test/normal" "$detailscale" "{scale}"
                "$detailblendmode" "0" "$detailblendfactor" "1"
                "$blendtintbybasealpha" "1" "$color" "[.6 .7 .8]"
            }}"#), &|_| None, 0,
        ).unwrap();
        let scalar = normal_material(Some(&parse("1")), false);
        let vector = normal_material(Some(&parse("[1 1 1]")), false);
        assert_ne!(vector.params.flags & flags::DETAIL, 0);
        assert_ne!(vector.params.flags & flags::NORMAL, 0);
        assert_ne!(vector.params.flags & flags::VERTEX_LIT, 0);
        assert_ne!(vector.params.flags & flags::TINT_BY_BASE_ALPHA, 0);
        assert_eq!(vector.params.detail, Vec4::new(1., 1., 1., 0.));
        assert_eq!(vector.params.base_uv, Vec4::new(10., 10., -4.5, -4.5));
        assert_eq!(vector.params.normal_uv, vector.params.base_uv);
        assert_eq!(vector.params.flags, scalar.params.flags);
        assert_eq!(vector.params.detail, scalar.params.detail);
        assert_eq!(vector.params.color, scalar.params.color);
        assert_eq!(vector.alpha_mode, scalar.alpha_mode);
        // AO stays at raw model UVs and in the existing data texture binding;
        // it does not inherit the tiled color/normal scale or replace a bake.
        assert_ne!(vector.detail, vector.base);
        assert_eq!(vector.detail, vector.normal);
        assert_eq!(vector.params.exposure, scalar.params.exposure);
        let unsupported = normal_material(Some(&parse("[1 1 2]")), false);
        assert_eq!(unsupported.params.flags & flags::DETAIL, 0);
    }

    #[test]
    fn authored_luminous_regions_keep_alpha_and_separate_mask_contracts() {
        fn parse(text: &str) -> vmt::Material {
            vmt::Material::parse_with(text, &|_| None, 0).unwrap()
        }
        let alpha = parse(r#""VertexLitGeneric" {
            "$basetexture" "test/normal" "$selfillum" "1" "$selfillumtint" "[4 4 3]"
        }"#);
        let out = normal_material(Some(&alpha), false);
        assert_eq!(out.params.self_illum, Vec4::new(4., 4., 3., 1.));
        assert_eq!(out.alpha_mode, AlphaMode::Opaque);
        assert_eq!(out.params.flags & flags::UNLIT, 0);
        let separate = parse(r#""VertexLitGeneric" {
            "$basetexture" "test/normal" "$selfillum" "1" "$selfillummask" "test/normal"
        }"#);
        let out = normal_material(Some(&separate), false);
        assert_eq!(out.params.self_illum, Vec4::new(1., 1., 1., 2.));
        // Mask data and color have independent cache entries/color spaces.
        assert_ne!(out.self_illum_mask, out.base);
        for declaration in [
            r#""$selfillummask" "missing/mask""#,
            r#""$selfillumfresnel" "1""#,
            r#""$selfillum_envmapmask_alpha" "1""#,
            r#""$selfillumtint" "[-1 1 1]""#,
            r#""$selfillumtint" "[1 -0.5 1]""#,
            r#""$selfillumtint" "[NaN 1 1]""#,
            r#""$selfillumtint" "[1 inf 1]""#,
            r#""$selfillumtint" "[1 1 -inf]""#,
            r#""$selfillumtint" ".5""#,
            r#""$selfillumtint" "garbage""#,
            r#""$selfillumtint" "[1 1]""#,
            r#""$selfillumtint" { "unsupported" "1" }"#,
            r#""$selfillummaskscale" "0.2" "$selfillummask" "test/normal""#,
            // Presence is unsupported even for identity/zero/invalid values.
            r#""$selfillummaskscale" "1""#,
            r#""$selfillummaskscale" "0""#,
            r#""$selfillummaskscale" "garbage""#,
        ] {
            let mat = parse(&format!(r#""VertexLitGeneric" {{ "$selfillum" "1" {declaration} }}"#));
            assert_eq!(normal_material(Some(&mat), false).params.self_illum.w, 0., "{declaration}");
        }
        // Static lamps can author Phong; only dynamic composition is gated.
        let phong = parse(r#""VertexLitGeneric" {
            "$selfillum" "1" "$phong" "1" "$rimlight" "1"
        }"#);
        let params = normal_material(Some(&phong), false).params;
        assert_eq!(params.self_illum, Vec4::new(1., 1., 1., 1.));
        assert_eq!(params.flags & flags::DYNAMIC_LIT, 0);
        assert!(params.phong.y > 0. && params.rim.y > 0.);
        let world = parse(r#""LightmappedGeneric" { "$selfillum" "1" }"#);
        assert_eq!(normal_material(Some(&world), true).params.self_illum, Vec4::ONE);
        assert_eq!(normal_material(None, true).params.self_illum, Vec4::ZERO);
    }

    #[test]
    fn opaque_env_alpha_emission_uses_its_own_mask_without_a_selfillum_switch() {
        let parse = |text: &str| vmt::Material::parse_with(text, &|_| None, 0).unwrap();
        let declaration = r#""VertexLitGeneric" { "$envmapmask" "test/normal"
            "$selfillum_envmapmask_alpha" "1" "$selfillumtint" "[1.3 1.57 2.3]" }"#;
        let out = normal_material(Some(&parse(declaration)), false);
        assert_eq!(out.params.self_illum, Vec4::new(1.3, 1.57, 2.3, 3.));
        assert_ne!(out.self_illum_mask, out.base);
        for extra in [r#""$translucent" "1""#, r#""$alphatest" "1""#,
            r#""$selfillummask" "test/normal""#, r#""$envmapmasktransform" "center .5 .5 scale 1 1""#,
            r#""Proxies" { "TextureScroll" { "textureScrollVar" "$envmapmasktransform" } }"#] {
            let text = declaration.replacen('}', &format!("{extra} }}"), 1);
            assert_eq!(normal_material(Some(&parse(&text)), false).params.self_illum.w, 0., "{extra}");
        }
    }

    #[test]
    fn self_illum_proxies_reject_references_recursively_without_vetoing_unrelated_proxies() {
        for (declaration, enabled) in [
            (r#""Proxies" { "Sine" { "resultVar" "$color" } }"#, true),
            (r#""Proxies" { "Nested" { "Copy" { "srcVar1" "$time" "resultVar" "$alpha" } } }"#, true),
            (r#""Proxies" { "Sine" { "resultVar" "$selfIllumTint" } }"#, false),
            // Reading an input is also an unvalidated animated contract.
            (r#""PROXIES" { "Nested" { "Copy" { "srcVar1" "$SELFILLUMTINT[0]" "resultVar" "$color" } } }"#, false),
            (r#""Proxies" { "Nested" { "$SelfIllumMask" "test/normal" } }"#, false),
            (r#""Proxies" { "Sine" { "resultVar" "$SelfIllum" } }"#, false),
            (r#""Proxies" "$selfIllumTint""#, false),
        ] {
            let mat = vmt::Material::parse_with(
                &format!(r#""VertexLitGeneric" {{ "$selfillum" "1" {declaration} }}"#),
                &|_| None, 0,
            ).unwrap();
            let expected = if enabled { Vec4::ONE } else { Vec4::ZERO };
            assert_eq!(normal_material(Some(&mat), false).params.self_illum, expected, "{declaration}");
        }
    }

    #[test]
    fn self_illum_opacity_and_terrain_blends_preserve_previous_diffuse_material() {
        for (shader, declaration, lightmapped, alpha_mode, required_flag) in [
            ("VertexLitGeneric", r#""$alphatest" "1""#, false, AlphaMode::Mask(0.5), flags::ALPHA_TEST),
            ("VertexLitGeneric", r#""$translucent" "1""#, false, AlphaMode::Blend, flags::TRANSLUCENT),
            ("VertexLitGeneric", r#""$additive" "1""#, false, AlphaMode::Add, flags::TRANSLUCENT),
            ("UnlitGeneric", "", false, AlphaMode::Opaque, flags::UNLIT),
            ("WorldVertexTransition", r#""$basetexture2" "test/normal""#, true, AlphaMode::Opaque, flags::BLEND),
            // Missing second textures do not validate the terrain contract.
            ("WorldVertexTransition", r#""$basetexture2" "missing/texture""#, true, AlphaMode::Opaque, flags::LIGHTMAPPED),
            ("LightmappedGeneric", r#""$basetexture2" "test/normal""#, true, AlphaMode::Opaque, flags::BLEND),
        ] {
            let parse = |self_illum: u8| vmt::Material::parse_with(
                &format!(r#""{shader}" {{ "$basetexture" "test/normal" "$color" "[0.5 0.75 1]" "$selfillum" "{self_illum}" {declaration} }}"#),
                &|_| None, 0,
            ).unwrap();
            let before = normal_material(Some(&parse(0)), lightmapped);
            let out = normal_material(Some(&parse(1)), lightmapped);
            assert_eq!(out.params.self_illum, Vec4::ZERO, "{shader}: {declaration}");
            assert_eq!(out.alpha_mode, alpha_mode);
            assert_ne!(out.params.flags & required_flag, 0);
            assert_eq!(out.params.flags, before.params.flags);
            assert_eq!(out.params.color, before.params.color);
            assert_eq!(out.alpha_mode, before.alpha_mode);
        }
    }

    #[test]
    fn first_normal_nonuniform_centered_mapping_and_identity_fallbacks_reach_material() {
        let identity = Vec4::new(1., 1., 0., 0.);
        assert_eq!(WorldParams::default().normal_uv, identity);
        assert_eq!(normal_material(None, true).params.normal_uv, identity);
        for (declaration, expected) in [
            ("", identity),
            (r#""$bumptransform" "translate .125 -.25 rotate 0 scale 2 .5 center .5 .5""#, Vec4::new(2., 0.5, -0.375, 0.)),
            // Evidenced hazard-stripe scale8,4/rotation90 stays unsupported.
            (r#""$bumptransform" "center 0 0 scale 8 4 rotate 90 translate 0 0""#, identity),
            (r#""$bumptransform" "[1 0 0 0 0 1 0 0]""#, identity),
            (r#""$bumptransform" "scale 8 8""#, identity),
        ] {
            let text = format!(r#""VertexLitGeneric" {{ "$bumpmap" "test/normal" "$phong" "1" {declaration} }}"#);
            let mat = vmt::Material::parse_with(&text, &|_| None, 0).unwrap();
            let params = normal_material(Some(&mat), false).params;
            assert_ne!(params.flags & flags::NORMAL, 0, "{declaration}");
            assert_eq!(params.normal_uv, expected, "{declaration}");
            assert_eq!(params.normal2_uv, identity);
            assert!(params.phong.y > 0.0);
        }
    }

    #[test]
    fn independent_layer_parameters_and_rejected_rotation_keep_their_own_mapping() {
        let mat = vmt::Material::parse_with(
            r#""WorldVertexTransition" {
                "$basetexturetransform2" "center 0 0 scale 2 2 rotate 0 translate 0 0"
                "$bumptransform2" "center 0 0 scale 3 4 rotate 0 translate .25 -.5"
                "$blendmodulatetransform" "center 0 0 scale .65 .65 rotate 0 translate 0 0"
            }"#,
            &|_| None, 0,
        ).unwrap();
        assert_eq!(authored_uv_transform(&mat, "$basetexturetransform2"), Vec4::new(2., 2., 0., 0.));
        assert_eq!(authored_uv_transform(&mat, "$bumptransform2"), Vec4::new(3., 4., 0.25, -0.5));
        assert_eq!(authored_uv_transform(&mat, "$blendmodulatetransform"), Vec4::new(0.65, 0.65, 0., 0.));
        assert_eq!(authored_uv_transform(&mat, "$absent"), Vec4::new(1., 1., 0., 0.));
        let rotated = vmt::Material::parse_with(
            r#""WorldVertexTransition" { "$bumptransform2" "center 0 0 scale 2 2 rotate 90 translate 0 0" }"#,
            &|_| None, 0,
        ).unwrap();
        assert_eq!(authored_uv_transform(&rotated, "$bumptransform2"), Vec4::new(1., 1., 0., 0.));
    }
}

/// Keep batches local so their actual bounds can be rejected by the frustum.
/// Faces/props remain intact; a large object is never clipped at a cell edge.
pub fn spatial_cell(p: glam::Vec3) -> [i32; 3] {
    (p / 768.0).floor().as_ivec3().to_array()
}

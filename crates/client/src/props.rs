//! Static props: studio models placed by the map's `sprp` game lump.
//!
//! Static props never move, so every instance is transformed into world
//! space at load time and merged into one mesh per material (per sky/world
//! layer). Eligible authored VHV lighting or spatial ambient-cube/Sun lighting
//! is baked into vertex colours. Bumped ordinary props transport directional
//! coefficients instead, evaluated with the fragment normal. Special modes
//! keep their instance fallback. A few hundred draws replace thousands of entities.

use crate::coords::{rotation_to_bevy, to_bevy};
use crate::material::WorldMaterial;
use crate::textures::TextureCache;
use crate::world_mesh::{material_from_vmt, SharedHandles};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use collision::CollisionWorld;
use std::collections::HashMap;

#[path = "prop_vertex_lighting.rs"]
mod vertex_lighting;
#[path = "prop_irradiance.rs"]
pub(crate) mod irradiance;

/// The map's sun, from `light_environment`.
#[derive(Clone, Copy, Debug)]
pub struct Sun {
    /// Direction towards the sun, Source axes.
    pub to_sun: glam::Vec3,
    pub color: glam::Vec3,
    pub ambient: glam::Vec3,
}

impl Sun {
    pub fn from_bsp(bsp: &bsp::Bsp) -> Option<Sun> {
        let ent = bsp.entities_of_class("light_environment").next()?;
        let angles = kv::parse_vec(ent.str("angles").unwrap_or("0 0 0"));
        let yaw = angles.get(1).copied().unwrap_or(0.0);
        let pitch = ent
            .str("pitch")
            .and_then(kv::parse_f32)
            .or_else(|| angles.first().copied())
            .unwrap_or(-45.0);
        // Pitch is the light's downward angle (negative = pointing down).
        let elevation = (-pitch).to_radians();
        let (sy, cy) = yaw.to_radians().sin_cos();
        let travel = glam::Vec3::new(elevation.cos() * cy, elevation.cos() * sy, -elevation.sin());
        let color = parse_light(ent.str("_lighthdr").filter(|s| !s.starts_with("-1")).or(ent.str("_light")));
        let ambient = parse_light(ent.str("_ambienthdr").filter(|s| !s.starts_with("-1")).or(ent.str("_ambient")));
        Some(Sun {
            to_sun: -travel.normalize_or_zero(),
            color,
            ambient,
        })
    }
}

/// `"r g b brightness"` → linear colour.
fn parse_light(s: Option<&str>) -> glam::Vec3 {
    let v = kv::parse_vec(s.unwrap_or("255 255 255 200"));
    let srgb = |c: f32| (c / 255.0).powf(2.2);
    let rgb = glam::Vec3::new(
        srgb(v.first().copied().unwrap_or(255.0)),
        srgb(v.get(1).copied().unwrap_or(255.0)),
        srgb(v.get(2).copied().unwrap_or(255.0)),
    );
    let brightness = v.get(3).copied().unwrap_or(200.0) / 255.0;
    rgb * brightness
}

/// Lighting for one prop instance, in Source axes.
#[derive(Clone, Copy, Debug)]
pub struct PropLight {
    /// +X, -X, +Y, -Y, +Z, -Z.
    pub cube: [glam::Vec3; 6],
    pub to_sun: glam::Vec3,
    /// Sun colour, zero when the prop is in shadow.
    pub sun: glam::Vec3,
}

impl PropLight {
    /// Irradiance for a Source-space normal.
    pub fn eval(&self, n: glam::Vec3) -> glam::Vec3 {
        let n2 = n * n;
        let pick = |pos: bool, a: usize| if pos { self.cube[a] } else { self.cube[a + 1] };
        let ambient = pick(n.x >= 0.0, 0) * n2.x + pick(n.y >= 0.0, 2) * n2.y + pick(n.z >= 0.0, 4) * n2.z;
        ambient + self.sun * n.dot(self.to_sun).max(0.0)
    }
}

/// Fading (0x01) and disabled shadow casting (0x10) do not change how a
/// surface receives lighting. In particular, a no-shadow autocombined model
/// still spans independently exposed/covered surfaces. Keep lighting-origin,
/// ignored-normal and other unsupported receiver modes on their old fallback.
fn supports_surface_lighting(flags: u8) -> bool {
    flags & !(0x01 | 0x10) == 0
}

/// Map ambient cube at the prop, plus the sun if its center ray reaches sky.
/// This one-sample fallback remains approximate for large/combined models.
fn prop_light(
    bsp: &bsp::Bsp,
    collision: &mut collision::PropLightingQuery<'_>,
    sun: Option<Sun>,
    center: glam::Vec3,
    origin: glam::Vec3,
    prop_ordinal: usize,
) -> PropLight {
    let cube = bsp
        .ambient_cube(center + glam::Vec3::Z * 4.0)
        .or_else(|| bsp.ambient_cube(origin + glam::Vec3::Z * 16.0))
        .unwrap_or([[0.3; 3]; 6])
        .map(glam::Vec3::from);
    match sun {
        Some(s) => {
            // Exclude this placed prop's own hulls instead of jumping beyond
            // its radius, which can skip intervening ceilings and walls.
            let tr = collision.trace_prop_lighting_ray(center, center + s.to_sun * 16384.0, prop_ordinal);
            // A center inside any remaining solid is not a valid sky sample,
            // even if the ray exits it and subsequently hits a sky brush.
            let lit = !tr.start_solid && !tr.all_solid && (!tr.hit() || collision.is_sky(tr.surface));
            PropLight {
                cube,
                to_sun: s.to_sun,
                sun: if lit { s.color } else { glam::Vec3::ZERO },
            }
        }
        None => PropLight {
            cube,
            to_sun: glam::Vec3::Z,
            sun: glam::Vec3::ZERO,
        },
    }
}

/// Spatial fallback for an ordinary, non-bumped static-prop surface. A hull
/// center can lie in the gap of an arch, or in a different room from one of an
/// autocombined model's pieces. The actual rendered vertex determines the leaf
/// and the sun ray; neither the model radius nor a guessed lighting origin does.
/// Missing leaf samples retain the instance cube, without inheriting its sun.
fn surface_light(
    bsp: &bsp::Bsp,
    collision: &mut collision::PropLightingQuery<'_>,
    sun: Option<Sun>,
    fallback: &PropLight,
    position: glam::Vec3,
    normal: glam::Vec3,
    prop_ordinal: usize,
) -> glam::Vec3 {
    // Plain surfaces can skip a ray whose cosine contribution is zero.
    surface_directional_light(bsp, collision, sun.filter(|s| normal.dot(s.to_sun) > 0.0),
        fallback, position, normal, prop_ordinal).eval(normal)
}

/// Keep the complete cube and visible Sun for later evaluation with a bumped
/// normal. A geometric back face can still have a sun-facing perturbed normal.
fn surface_directional_light(
    bsp: &bsp::Bsp,
    collision: &mut collision::PropLightingQuery<'_>,
    sun: Option<Sun>,
    fallback: &PropLight,
    position: glam::Vec3,
    normal: glam::Vec3,
    prop_ordinal: usize,
) -> PropLight {
    // Resolve a surface exactly on a BSP split towards its visible side. This
    // is the collision tolerance, not the old multi-inch/radius ray teleport.
    let sample = position + normal * collision::DIST_EPSILON;
    let cube = bsp
        .ambient_cube(sample)
        .map(|cube| cube.map(glam::Vec3::from))
        .unwrap_or(fallback.cube);
    let mut light = PropLight {
        cube,
        to_sun: sun.map_or(glam::Vec3::Z, |s| s.to_sun),
        sun: glam::Vec3::ZERO,
    };
    if let Some(s) = sun {
        let tr = collision.trace_prop_lighting_ray(sample, sample + s.to_sun * 16384.0, prop_ordinal);
        if !tr.start_solid && !tr.all_solid && (!tr.hit() || collision.is_sky(tr.surface)) {
            light.sun = s.color;
        }
    }
    light
}

#[derive(Default)]
struct Batch {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    light_ids: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
    irradiance: [Vec<[u32; 4]>; 3],
    indices: Vec<u32>,
}

impl Batch {
    fn into_mesh(self) -> Mesh {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.light_ids);
        m.insert_indices(Indices::U32(self.indices));
        m.insert_attribute(Mesh::ATTRIBUTE_TANGENT, self.tangents);
        for (attribute, values) in irradiance::ATTRIBUTES.into_iter().zip(self.irradiance) {
            if !values.is_empty() { m.insert_attribute(attribute, values); }
        }
        m
    }
}

/// Experiment: merge depth-writing opaque/masked props in 1536-unit cells.
/// Keep sorted/uncertain modes at 768; overlays explicitly disable depth writes
/// in WorldMaterial even when their alpha mode is opaque. This key never clips
/// geometry: full merged vertices still determine frustum bounds and PVS.
fn prop_cell(origin: glam::Vec3, material: Option<&WorldMaterial>) -> [i32; 3] {
    let coarsen = material.is_some_and(|m|
        matches!(m.alpha_mode, AlphaMode::Opaque | AlphaMode::Mask(_))
            && m.params.flags & crate::material::flags::OVERLAY == 0);
    crate::world_mesh::spatial_cell(if coarsen { origin * 0.5 } else { origin })
}

/// Whether a studio mesh's triangles need flipping to wind counter-clockwise
/// around their vertex normals.
pub fn needs_flip(mesh: &mdl::Mesh) -> bool {
    let mut agree = 0i64;
    for tri in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let va = &mesh.vertices[a];
        let n = (mesh.vertices[b].position - va.position).cross(mesh.vertices[c].position - va.position);
        let vn = va.normal + mesh.vertices[b].normal + mesh.vertices[c].normal;
        agree += if n.dot(vn) >= 0.0 { 1 } else { -1 };
    }
    agree < 0
}

pub struct PropStats {
    pub instances: usize,
    pub draws: usize,
    pub materials: usize,
    pub failed_models: usize,
    pub sky_draws: usize,
    pub triangles: usize,
}

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    commands: &mut Commands,
    level: &level::Level,
    collision: &CollisionWorld,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WorldMaterial>,
    images: &mut Assets<Image>,
    textures: &mut TextureCache,
    shared: &SharedHandles,
    exposure: f32,
    sun: Option<Sun>,
    sky: Option<bsp::SkyCamera>,
) -> PropStats {
    let fs = &level.vfs;
    let bsp = &level.bsp;
    let read = |p: &str| fs.read_string(p);
    let mut stats = PropStats {
        instances: 0,
        draws: 0,
        materials: 0,
        failed_models: 0,
        sky_draws: 0,
        triangles: 0,
    };

    // Load every referenced model once, with per-mesh winding fixes.
    let mut models: HashMap<u16, Option<(mdl::Model, Vec<bool>)>> = HashMap::new();
    for prop in &bsp.static_props.props {
        models.entry(prop.model).or_insert_with(|| {
            let name = bsp.static_props.models.get(prop.model as usize)?;
            let base = name.trim_end_matches(".mdl");
            let m = fs.read(name)?;
            let v = fs.read(&format!("{base}.vvd"))?;
            let x = fs.read(&format!("{base}.dx90.vtx"))?;
            let model = mdl::Model::load(&m, &v, &x).ok()?;
            let flips = model.meshes.iter().map(needs_flip).collect();
            Some((model, flips))
        });
    }
    // Calculate tangents once per unique model mesh, then rotate per instance.
    // Doing MikkTSpace on all six million placed vertices stalls map loading.
    let mut tangent_cache = HashMap::new();
    for (&id, loaded) in &models {
        let Some((model, flips)) = loaded else { continue; };
        for (index, (mesh, flip)) in model.meshes.iter().zip(flips).enumerate() {
            if let Some(tangents) = mesh.vertices.iter().map(|v| v.tangent).collect::<Option<Vec<_>>>() {
                tangent_cache.insert((id, index), tangents);
                continue;
            }
            let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::MAIN_WORLD);
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, mesh.vertices.iter().map(|v| v.position.to_array()).collect::<Vec<_>>());
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, mesh.vertices.iter().map(|v| v.normal.to_array()).collect::<Vec<_>>());
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, mesh.vertices.iter().map(|v| v.uv.to_array()).collect::<Vec<_>>());
            let mut indices = mesh.indices.clone();
            if *flip { for tri in indices.chunks_exact_mut(3) { tri.swap(1,2); } }
            m.insert_indices(Indices::U32(indices));
            if m.generate_tangents().is_ok() {
                if let Some(bevy::mesh::VertexAttributeValues::Float32x4(t)) = m.attribute(Mesh::ATTRIBUTE_TANGENT) {
                    tangent_cache.insert((id, index), t.clone());
                }
            }
        }
    }
    stats.failed_models = models.values().filter(|m| m.is_none()).count();

    // One material per resolved VMT, shared by every prop that uses it.
    let mut material_cache: HashMap<String, Option<Handle<WorldMaterial>>> = HashMap::new();
    let mut batches: HashMap<(AssetId<WorldMaterial>, bool, [i32; 3], bool), (Handle<WorldMaterial>, Batch)> = HashMap::new();

    // Six ambient faces, Sun direction/color, then per-instance diffuse tint.
    const PROP_TEXELS: usize = 9;
    let mut light_data = vec![0u8; bsp.static_props.props.len().max(1) * PROP_TEXELS * 8];
    let mut authored_meshes = 0usize;
    let mut authored_vertices = 0usize;
    let mut surface_vertices = 0usize;
    let mut directional_vertices = 0usize;
    let mut detail_meshes = 0usize;
    let mut tinted_instances = 0usize;
    // Temporary, map-wide BVH and ray scratch; gameplay keeps the normal grid.
    let mut lighting_query = collision.prepare_prop_lighting();
    for (prop_index, prop) in bsp.static_props.props.iter().enumerate() {
        let Some(Some((model, flips))) = models.get(&prop.model) else { continue };
        let authored = vertex_lighting::load(level, prop_index, model);
        let rot = mdl::angles_to_mat3(prop.angles);
        let scale = if prop.scale > 0.0 { prop.scale } else { 1.0 };
        let center = prop.origin + rot * ((model.hull_min + model.hull_max) * 0.5 * scale);
        let light = prop_light(bsp, &mut lighting_query, sun, center, prop.origin, prop_index);
        // Exact geometric duplicates at UV/material seams share the query.
        // No spatial quantization: neighboring vertices can straddle a shadow
        // or leaf boundary. The cache dies with this placed instance, so another
        // instance of the same model cannot reuse its lighting/occlusion.
        let mut surface_cache = HashMap::<([u32; 3], [u32; 3]), glam::Vec3>::new();
        let mut directional_cache = HashMap::<([u32; 3], [u32; 3]), PropLight>::new();
        let cube = irradiance::CUBE_ORDER.map(|i| light.cube[i].extend(1.0));
        // Opacity changes need separate native admission and material modes.
        // Unsupported layouts/alpha retain the previous identity modulation.
        let tint = match prop.diffuse_modulation {
            Some([r, g, b, 255]) => {
                if [r, g, b] != [255; 3] {
                    tinted_instances += 1;
                    debug!("Authored prop diffuse tint: instance={prop_index} rgb_bytes={r},{g},{b} alpha=255");
                }
                glam::Vec3::new(r as f32, g as f32, b as f32) / 255.0
            }
            _ => glam::Vec3::ONE,
        };
        for (i, v) in cube.into_iter().chain([to_bevy(light.to_sun).extend(1.0), light.sun.extend(1.0), tint.extend(1.0)]).enumerate() {
            for (c, value) in v.to_array().into_iter().enumerate() {
                let offset = (prop_index * PROP_TEXELS + i) * 8 + c * 2;
                light_data[offset..offset + 2].copy_from_slice(&crate::textures::f32_to_f16(value).to_le_bytes());
            }
        }

        let in_sky = sky.filter(|s| bsp.point_area(prop.origin + glam::Vec3::Z * 4.0) == s.area);
        let mut transform = Transform {
            translation: to_bevy(prop.origin),
            rotation: rotation_to_bevy(rot),
            scale: Vec3::splat(scale),
        };
        if let Some(s) = in_sky {
            transform = crate::sky::sky_transform(&s) * transform;
        }
        let affine = transform.compute_affine();

        for (mesh_index, (mesh, &flip)) in model.meshes.iter().zip(flips).enumerate() {
            let tex = model.skin_texture(prop.skin.max(0) as usize, mesh.material);
            let candidates = model.material_candidates(tex);
            let key = candidates.first().cloned().unwrap_or_default();
            let material = material_cache
                .entry(key)
                .or_insert_with(|| {
                    let vmt = candidates.iter().find_map(|c| vmt::Material::load(c, &read));
                    // Unsupported water/refraction effects otherwise become
                    // opaque white geometry. Omit their visual mesh until the
                    // corresponding renderer exists; collision stays intact.
                    if vmt.as_ref().is_some_and(|m| m.is_water() || m.shader == "refract") {
                        return None;
                    }
                    stats.materials += 1;
                    Some(materials.add(material_from_vmt(vmt.as_ref(), fs, textures, images, shared, exposure, false)))
                })
                .clone();
            let Some(material) = material else { continue; };
            if let Some(m) = materials.get(&material).filter(|m| m.params.flags & crate::material::flags::DETAIL != 0) {
                detail_meshes += 1;
                debug!("Authored prop detail: instance={prop_index} mesh={mesh_index} flags={} scale={},{} factor={} model={}", m.params.flags, m.params.detail.x, m.params.detail.y, m.params.detail.z, bsp.static_props.models[prop.model as usize]);
            }
            let authored_colors = materials.get(&material)
                .filter(|m| m.params.flags & crate::material::flags::VERTEX_LIT != 0
                    && m.params.flags & crate::material::flags::NORMAL == 0)
                .and_then(|_| authored.as_ref()?.get(mesh_index)?.as_deref());
            // Special prop lighting modes retain their previous fallback. Only
            // ordinary/fading/no-cast-shadow props enter either spatial path.
            // Bumped meshes carry full coefficients, rather than colors the
            // shader discards.
            let spatial_fallback = authored_colors.is_none()
                && supports_surface_lighting(prop.flags)
                && materials.get(&material).is_some_and(|m|
                    m.params.flags & crate::material::flags::VERTEX_LIT != 0
                    && m.params.flags & crate::material::flags::NORMAL == 0);
            let directional_fallback = supports_surface_lighting(prop.flags)
                && materials.get(&material).is_some_and(|m|
                    m.params.flags & crate::material::flags::VERTEX_LIT != 0
                    && m.params.flags & crate::material::flags::NORMAL != 0);
            if let Some(colors) = authored_colors {
                // The scoped candidate checked mesh identity/counts; its
                // individual hardware-order interpretation still needs native
                // spatial admission. Never index the source VVD vector here.
                authored_meshes += 1;
                authored_vertices += colors.len();
                debug!("Authored equal-group prop: instance={prop_index} mesh={mesh_index} vertices={} model={}",
                    colors.len(), bsp.static_props.models[prop.model as usize]);
            }
            let (_, batch) = batches
                .entry((material.id(), in_sky.is_some(), prop_cell(prop.origin, materials.get(&material)), directional_fallback))
                .or_insert_with(|| (material.clone(), Batch::default()));
            let base = batch.positions.len() as u32;
            for (vertex_index, v) in mesh.vertices.iter().enumerate() {
                let n_source = (rot * v.normal).normalize_or_zero();
                let tangent = tangent_cache.get(&(prop.model, mesh_index)).and_then(|t| t.get(vertex_index)).copied().unwrap_or([1.0,0.0,0.0,1.0]);
                let t = to_bevy(rot * glam::Vec3::new(tangent[0], tangent[1], tangent[2]));
                batch.tangents.push([t.x,t.y,t.z,tangent[3]]);
                let lit = if let Some(colors) = authored_colors {
                    colors[vertex_index]
                } else if spatial_fallback {
                    surface_vertices += 1;
                    let position = prop.origin + rot * (v.position * scale);
                    let key = (position.to_array().map(f32::to_bits), n_source.to_array().map(f32::to_bits));
                    *surface_cache.entry(key).or_insert_with(||
                        surface_light(bsp, &mut lighting_query, sun, &light, position, n_source, prop_index))
                } else if directional_fallback {
                    directional_vertices += 1;
                    let position = prop.origin + rot * (v.position * scale);
                    let key = (position.to_array().map(f32::to_bits), n_source.to_array().map(f32::to_bits));
                    let surface = directional_cache.entry(key).or_insert_with(||
                        surface_directional_light(bsp, &mut lighting_query, sun, &light, position, n_source, prop_index));
                    for (values, packed) in batch.irradiance.iter_mut().zip(irradiance::pack(surface)) {
                        values.push(packed);
                    }
                    // Retain the color stream/layout; the directional shader
                    // replaces it once, after perturbing the normal.
                    surface.eval(n_source)
                } else {
                    light.eval(n_source)
                };
                batch.positions.push(affine.transform_point3(to_bevy(v.position)).to_array());
                batch.normals.push(to_bevy(n_source).to_array());
                batch.uvs.push(v.uv.to_array());
                batch.light_ids.push([prop_index as f32, 0.0]);
                batch.colors.push([lit.x, lit.y, lit.z, 1.0]);
            }
            for tri in mesh.indices.chunks_exact(3) {
                if flip {
                    batch.indices.extend_from_slice(&[base + tri[0], base + tri[2], base + tri[1]]);
                } else {
                    batch.indices.extend_from_slice(&[base + tri[0], base + tri[1], base + tri[2]]);
                }
            }
            stats.triangles += mesh.indices.len() / 3;
        }
        stats.instances += 1;
    }

    drop(lighting_query);

    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let lighting = images.add(Image::new(
        Extent3d { width: PROP_TEXELS as u32, height: bsp.static_props.props.len().max(1) as u32, depth_or_array_layers: 1 },
        TextureDimension::D2, light_data, TextureFormat::Rgba16Float, RenderAssetUsages::RENDER_WORLD,
    ));
    for handle in material_cache.values().flatten() {
        if let Some(mut mat) = materials.get_mut(handle) { mat.prop_light = lighting.clone(); }
    }
    for ((_, in_sky, _, _), (material, batch)) in batches {
        if batch.indices.is_empty() {
            continue;
        }
        let mesh = batch.into_mesh();
        let clusters = crate::visibility::MapClusters::mesh(bsp, &mesh);
        let mut e = commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            Transform::IDENTITY,
        ));
        if in_sky {
            e.insert(crate::sky::sky_layer());
            stats.sky_draws += 1;
        } else { e.insert(clusters); }
        stats.draws += 1;
    }
    info!("Authored equal-group prop lighting: meshes={authored_meshes} vertices={authored_vertices}; unsupported meshes use ambient/Sun fallback");
    info!("Spatial ambient/Sun prop fallback: plain_vertices={surface_vertices} directional_vertices={directional_vertices} directional_attribute_bytes={}; special-mode props retain instance lighting", directional_vertices * 48);
    info!("Authored static-prop diffuse tint: nonwhite_instances={tinted_instances}");
    info!("Authored ordinary static-prop detail: meshes={detail_meshes}; unsupported layers retain base material");
    stats
}

#[cfg(test)]
mod lighting_tests {
    use super::*;
    use glam::Vec3 as SourceVec3;

    fn cell_material(alpha_mode: AlphaMode) -> WorldMaterial {
        WorldMaterial {
            params: Default::default(),
            base: Default::default(),
            base2: Default::default(),
            lightmap: Default::default(),
            modulate: Default::default(),
            normal: Default::default(),
            normal2: Default::default(),
            prop_light: Default::default(),
            phong_map: Default::default(),
            skin_surface: Default::default(),
            skin_position: Default::default(),
            detail: Default::default(),
            self_illum_mask: Default::default(),
            alpha_mode,
            cull: true,
        }
    }

    #[test]
    fn opaque_and_masked_prop_cells_merge_across_old_boundaries() {
        for mode in [AlphaMode::Opaque, AlphaMode::Mask(0.5)] {
            let material = cell_material(mode);
            for (coordinate, expected) in [
                (-1537., -2), (-1536., -1), (-769., -1), (-767., -1),
                (-1., -1), (0., 0), (767., 0), (769., 0), (1535., 0), (1536., 1),
            ] {
                assert_eq!(prop_cell(SourceVec3::splat(coordinate), Some(&material)),
                    [expected; 3], "{mode:?} at {coordinate}");
            }
            for (a, b) in [(767., 769.), (-769., -767.)] {
                let (a, b) = (SourceVec3::splat(a), SourceVec3::splat(b));
                assert_ne!(crate::world_mesh::spatial_cell(a), crate::world_mesh::spatial_cell(b));
                assert_eq!(prop_cell(a, Some(&material)), prop_cell(b, Some(&material)));
            }
        }
    }

    #[test]
    fn translucent_uncertain_and_non_depth_writing_prop_cells_stay_local() {
        let mut modes: Vec<_> = [
            AlphaMode::Blend, AlphaMode::Premultiplied, AlphaMode::Add,
            AlphaMode::Multiply, AlphaMode::AlphaToCoverage,
        ].into_iter().map(cell_material).collect();
        for mode in [AlphaMode::Opaque, AlphaMode::Mask(0.5)] {
            let mut overlay = cell_material(mode);
            overlay.params.flags |= crate::material::flags::OVERLAY;
            modes.push(overlay);
        }
        for coordinate in [-1537., -1536., -769., -768., -1., 0., 767., 768., 1536.] {
            let origin = SourceVec3::splat(coordinate);
            let original = crate::world_mesh::spatial_cell(origin);
            for material in &modes {
                assert_eq!(prop_cell(origin, Some(material)), original, "{material:?}");
            }
            assert_eq!(prop_cell(origin, None), original);
        }
    }

    const SUN: Sun = Sun {
        to_sun: SourceVec3::Z,
        color: SourceVec3::ONE,
        ambient: SourceVec3::ZERO,
    };

    // Synthetic long, thin solid: the old bounding-sphere start would be
    // over 173 units above its center and skip a roof at z=33..44.
    fn fixture(second_instance: bool) -> (bsp::Bsp, CollisionWorld) {
        let prop = bsp::StaticProp {
            origin: SourceVec3::ZERO, angles: SourceVec3::ZERO, model: 0,
            solid: 6, flags: 0, skin: 0, fade_min: 0.0, fade_max: 0.0,
            scale: 1.0, diffuse_modulation: None,
        };
        let mut map = bsp::Bsp::default();
        map.static_props.models.push("synthetic_long_prop".into());
        map.static_props.props.push(prop.clone());
        if second_instance {
            map.static_props.props.push(bsp::StaticProp {
                origin: SourceVec3::Z * 60.0, ..prop
            });
        }
        let hull = mdl::ConvexHull {
            points: vec![
                SourceVec3::new(-169.0, -1.0, -1.0), SourceVec3::new(169.0, -1.0, -1.0),
                SourceVec3::new(169.0, 1.0, -1.0), SourceVec3::new(-169.0, 1.0, -1.0),
                SourceVec3::new(-169.0, -1.0, 1.0), SourceVec3::new(169.0, -1.0, 1.0),
                SourceVec3::new(169.0, 1.0, 1.0), SourceVec3::new(-169.0, 1.0, 1.0),
            ],
            triangles: vec![
                [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7],
                [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5],
                [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7],
            ],
        };
        let w = CollisionWorld::from_bsp(&map, &collision::MapCollisionOptions::default(), &|_| Some(vec![hull.clone()]));
        (map, w)
    }

    fn add_world_box(w: &mut CollisionWorld, bottom: f32, top: f32, material: &str) {
        add_world_bounds(w, SourceVec3::new(-256.0, -256.0, bottom), SourceVec3::new(256.0, 256.0, top), material);
    }

    fn add_world_bounds(w: &mut CollisionWorld, mins: SourceVec3, maxs: SourceVec3, material: &str) {
        let surface = w.surface_id(material);
        assert!(w.add_convex(collision::Convex {
            planes: vec![
                collision::Plane { normal: SourceVec3::X, dist: maxs.x },
                collision::Plane { normal: -SourceVec3::X, dist: -mins.x },
                collision::Plane { normal: SourceVec3::Y, dist: maxs.y },
                collision::Plane { normal: -SourceVec3::Y, dist: -mins.y },
                collision::Plane { normal: SourceVec3::Z, dist: maxs.z },
                collision::Plane { normal: -SourceVec3::Z, dist: -mins.z },
            ],
            mins,
            maxs,
            contents: collision::contents::SOLID,
            surface,
        }));
    }

    fn fallback(map: &bsp::Bsp, w: &CollisionWorld) -> PropLight {
        prop_light(map, &mut w.prepare_prop_lighting(), Some(SUN), SourceVec3::ZERO, SourceVec3::ZERO, 0)
    }

    #[test]
    fn owning_solid_prop_below_roof_does_not_skip_to_sky() {
        let (map, mut w) = fixture(false);
        add_world_box(&mut w, 33.0, 44.0, "synthetic_roof");
        add_world_box(&mut w, 1000.0, 1010.0, "tools/toolsskybox");
        w.finalize();
        let old_start = SourceVec3::Z * (SourceVec3::new(169.0, 1.0, 1.0).length() + 4.0);
        let old = w.trace_ray(old_start, old_start + SUN.to_sun * 16384.0, collision::contents::MASK_SOLID);
        assert!(!old.start_solid && w.is_sky(old.surface), "fixture must reproduce skipped roof");
        let corrected = w.trace_prop_lighting_ray(SourceVec3::ZERO, SUN.to_sun * 16384.0, 0);
        assert_eq!(w.surface_name(corrected.surface), "synthetic_roof");
        assert!(!corrected.start_solid);
        assert_eq!(fallback(&map, &w).sun, SourceVec3::ZERO);
    }

    #[test]
    fn exterior_owning_solid_prop_gets_sun_with_open_or_brush_sky() {
        for sky_brush in [false, true] {
            let (map, mut w) = fixture(false);
            if sky_brush { add_world_box(&mut w, 1000.0, 1010.0, "tools/toolsskybox"); }
            w.finalize();
            assert!(w.trace_ray(SourceVec3::ZERO, SUN.to_sun * 16384.0, collision::contents::MASK_SOLID).start_solid);
            assert_eq!(fallback(&map, &w).sun, SUN.color);
        }
    }

    #[test]
    fn separate_same_model_instance_still_shadows_owner() {
        let (map, mut w) = fixture(true);
        add_world_box(&mut w, 1000.0, 1010.0, "tools/toolsskybox");
        w.finalize();
        let tr = w.trace_prop_lighting_ray(SourceVec3::ZERO, SUN.to_sun * 16384.0, 0);
        assert!(!tr.start_solid && tr.hit());
        assert_eq!(w.surface_name(tr.surface), "synthetic_long_prop");
        assert!((tr.end.z - 59.0).abs() < 0.1, "{tr:?}");
        assert_eq!(fallback(&map, &w).sun, SourceVec3::ZERO);
    }

    #[test]
    fn center_inside_world_solid_stays_shadowed_after_exit_to_sky() {
        let (map, mut w) = fixture(false);
        add_world_box(&mut w, -8.0, 8.0, "synthetic_world_solid");
        add_world_box(&mut w, 1000.0, 1010.0, "tools/toolsskybox");
        w.finalize();
        let tr = w.trace_prop_lighting_ray(SourceVec3::ZERO, SUN.to_sun * 16384.0, 0);
        assert!(tr.start_solid && !tr.all_solid);
        assert!(w.is_sky(tr.surface), "ray exits the solid then hits sky");
        assert_eq!(fallback(&map, &w).sun, SourceVec3::ZERO);
    }

    #[test]
    fn displacement_triangle_still_shadows_owner() {
        let (map, mut w) = fixture(false);
        w.add_triangle([
            SourceVec3::new(-100.0, -100.0, 40.0),
            SourceVec3::new(0.0, 100.0, 40.0),
            SourceVec3::new(100.0, -100.0, 40.0),
        ], collision::contents::SOLID, 0);
        w.finalize();
        let tr = w.trace_prop_lighting_ray(SourceVec3::ZERO, SUN.to_sun * 16384.0, 0);
        assert!(!tr.start_solid && tr.hit());
        assert!((tr.end.z - 40.0).abs() < 0.1, "{tr:?}");
        assert_eq!(fallback(&map, &w).sun, SourceVec3::ZERO);
    }

    #[test]
    fn a_combined_prop_keeps_covered_and_exposed_surfaces_distinct() {
        let (map, mut w) = fixture(false);
        // The same long owning hull runs under this partial roof and outside.
        // Its center is covered, so an instance-wide decision loses the sun
        // on the exposed portion even after the skipped-roof correction.
        add_world_bounds(&mut w, SourceVec3::new(-256.0, -256.0, 33.0), SourceVec3::new(32.0, 256.0, 44.0), "partial_roof");
        w.finalize();
        let instance = fallback(&map, &w);
        assert_eq!(instance.sun, SourceVec3::ZERO);
        let covered = surface_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), &instance, SourceVec3::new(-80.0, 0.0, 1.0), SourceVec3::Z, 0);
        let exposed = surface_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), &instance, SourceVec3::new(80.0, 0.0, 1.0), SourceVec3::Z, 0);
        assert_eq!(covered, instance.cube[4]);
        assert_eq!(exposed, instance.cube[4] + SUN.color);
    }

    #[test]
    fn an_exposed_center_does_not_illuminate_a_covered_surface() {
        let (map, mut w) = fixture(false);
        add_world_bounds(&mut w, SourceVec3::new(32.0, -256.0, 33.0), SourceVec3::new(256.0, 256.0, 44.0), "partial_roof");
        w.finalize();
        let instance = fallback(&map, &w);
        assert_eq!(instance.sun, SUN.color);
        let covered = surface_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), &instance, SourceVec3::new(80.0, 0.0, 1.0), SourceVec3::Z, 0);
        assert_eq!(covered, instance.cube[4], "missing surface ambient must not reintroduce center sunlight");
    }

    #[test]
    fn a_surface_in_world_solid_cannot_borrow_center_sunlight() {
        let (map, mut w) = fixture(false);
        add_world_bounds(&mut w, SourceVec3::new(32.0, -256.0, -8.0), SourceVec3::new(256.0, 256.0, 8.0), "embedded_surface");
        w.finalize();
        let instance = fallback(&map, &w);
        assert_eq!(instance.sun, SUN.color);
        let embedded = surface_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), &instance, SourceVec3::new(80.0, 0.0, 1.0), SourceVec3::Z, 0);
        assert_eq!(embedded, instance.cube[4]);
    }

    #[test]
    fn surface_inside_own_collision_hull_does_not_self_shadow() {
        let (map, mut w) = fixture(false);
        w.finalize();
        let instance = fallback(&map, &w);
        // Render vertices need not lie on the outside of the simplified hull.
        // The ordinary ray starts solid here; the placed-owner exclusion is
        // still required after switching to surface lighting.
        assert!(w.trace_ray(SourceVec3::Z * collision::DIST_EPSILON, SourceVec3::Z * 16384.0, collision::contents::MASK_SOLID).start_solid);
        assert_eq!(surface_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), &instance, SourceVec3::ZERO, SourceVec3::Z, 0), instance.cube[4] + SUN.color);
    }

    #[test]
    fn surface_ray_keeps_other_instances_and_displacements_as_occluders() {
        for other_prop in [false, true] {
            let (map, mut w) = fixture(other_prop);
            if !other_prop {
                w.add_triangle([
                    SourceVec3::new(-200.0, -100.0, 40.0),
                    SourceVec3::new(-80.0, 100.0, 40.0),
                    SourceVec3::new(100.0, -100.0, 40.0),
                ], collision::contents::SOLID, 0);
            }
            w.finalize();
            let instance = PropLight { cube: [SourceVec3::splat(0.2); 6], to_sun: SUN.to_sun, sun: SUN.color };
            let actual = surface_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), &instance, SourceVec3::new(-80.0, 0.0, 1.0), SourceVec3::Z, 0);
            assert_eq!(actual, instance.cube[4], "other_prop={other_prop}");
        }
    }

    #[test]
    fn surface_ambient_follows_its_own_leaf_and_source_normal() {
        let (mut map, mut w) = fixture(false);
        // Two leaves separated at x=0, with independent directional colors.
        // A single placed mesh can cross this split without changing materials.
        map.planes.push(bsp::Plane { normal: SourceVec3::X, dist: 0.0, kind: 0 });
        map.nodes.push(bsp::Node {
            plane: 0, children: [-1, -2], mins: [-256; 3], maxs: [256; 3],
            first_face: 0, num_faces: 0,
        });
        let leaf = bsp::Leaf {
            contents: 0, cluster: 0, area: 0, mins: [-256; 3], maxs: [256; 3],
            first_leaf_face: 0, num_leaf_faces: 0, first_leaf_brush: 0, num_leaf_brushes: 0,
        };
        map.leafs = vec![leaf, leaf];
        map.leaf_ambient_index = vec![(1, 0), (1, 1)];
        let cube = |scale: f32| std::array::from_fn(|i| [scale * (i + 1) as f32; 3]);
        map.leaf_ambient = vec![
            bsp::AmbientSample { cube: cube(0.1), pos: [128; 3] },
            bsp::AmbientSample { cube: cube(0.01), pos: [128; 3] },
        ];
        w.finalize();
        let instance = fallback(&map, &w);
        for (x, scale) in [(80.0, 0.1), (-80.0, 0.01)] {
            for (normal, face) in [(SourceVec3::X, 0), (-SourceVec3::Y, 3), (SourceVec3::Z, 4)] {
                let actual = surface_light(&map, &mut w.prepare_prop_lighting(), None, &instance, SourceVec3::new(x, 0.0, 1.0), normal, 0);
                assert!((actual - SourceVec3::splat(scale * (face + 1) as f32)).abs().max_element() < 1e-6);
            }
        }
        // An absent local sample keeps the previous cube rather than choosing
        // an arbitrary neighboring leaf or an unrelated authored origin.
        map.leaf_ambient_index[1] = (0, 0);
        assert_eq!(surface_light(&map, &mut w.prepare_prop_lighting(), None, &instance, SourceVec3::new(-80.0, 0.0, 1.0), SourceVec3::Z, 0), instance.cube[4]);
    }

    #[test]
    fn bumped_surface_keeps_sun_for_perturbed_normals_but_rejects_local_occlusion() {
        let (map, mut w) = fixture(false);
        add_world_bounds(&mut w, SourceVec3::new(-256.0, -256.0, 33.0), SourceVec3::new(32.0, 256.0, 44.0), "partial_roof");
        w.finalize();
        let instance = fallback(&map, &w);
        assert_eq!(instance.sun, SourceVec3::ZERO);
        let sample = |x| surface_directional_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), &instance,
            SourceVec3::new(x, 0., 1.), -SourceVec3::Z, 0);
        let covered = sample(-80.);
        let exposed = sample(80.);
        assert_eq!(covered.sun, SourceVec3::ZERO);
        assert_eq!(exposed.sun, SUN.color, "geometric back face must not cull a later bumped Sun response");
        assert_eq!(covered.eval(-SourceVec3::Z), exposed.eval(-SourceVec3::Z));
        // Normalization and subtraction can round a unit contribution by one
        // float ULP; retain a tight comparison of the actual lighting result.
        assert!((exposed.eval(SourceVec3::Z) - covered.eval(SourceVec3::Z) - SUN.color).abs().max_element() < 1e-6);
        assert_eq!(surface_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), &instance,
            SourceVec3::new(80., 0., 1.), -SourceVec3::Z, 0), exposed.eval(-SourceVec3::Z));

        // A missing surface cube may retain ambient, but an exposed instance
        // must not leak its Sun through remaining world solid or other props.
        for other_prop in [false, true] {
            let (map, mut w) = fixture(other_prop);
            if !other_prop { add_world_box(&mut w, -8., 8., "world_solid"); }
            w.finalize();
            let fallback = PropLight { cube: instance.cube, sun: SUN.color, to_sun: SUN.to_sun };
            let local = surface_directional_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), &fallback,
                SourceVec3::ZERO, -SourceVec3::Z, 0);
            assert_eq!(local.sun, SourceVec3::ZERO, "other_prop={other_prop}");
            assert_eq!(local.cube, instance.cube);
        }
    }

    #[test]
    fn no_cast_shadow_props_receive_spatial_light_without_bypassing_occluders() {
        for flags in [0x00, 0x01, 0x10, 0x11] {
            let (mut map, mut w) = fixture(false);
            map.static_props.props[0].flags = flags;
            assert!(supports_surface_lighting(map.static_props.props[0].flags));
            add_world_bounds(&mut w, SourceVec3::new(-256., -256., 33.),
                SourceVec3::new(32., 256., 44.), "partial_cover");
            w.finalize();
            let instance = fallback(&map, &w);
            assert_eq!(instance.sun, SourceVec3::ZERO);
            let sample = |x| surface_directional_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), &instance,
                SourceVec3::new(x, 0., 1.), SourceVec3::Z, 0);
            assert_eq!(sample(-80.).sun, SourceVec3::ZERO,
                "cast-shadow flags must not disable receiving shadows: {flags:#x}");
            assert_eq!(sample(80.).sun, SUN.color,
                "a covered center must not darken an exposed surface: {flags:#x}");
        }
        for flags in 0u8..=u8::MAX {
            if ![0x00, 0x01, 0x10, 0x11].contains(&flags) {
                assert!(!supports_surface_lighting(flags),
                    "unsupported lighting modes must retain their fallback: {flags:#x}");
            }
        }
    }

    #[test]
    fn prop_batches_keep_all_packed_streams_aligned_without_changing_ordinary_layouts() {
        for directional in [false, true] {
            let mut batch = Batch::default();
            let light = PropLight { cube: [SourceVec3::ONE; 6], sun: SourceVec3::ZERO, to_sun: SUN.to_sun };
            for _ in 0..3 {
                batch.positions.push([0.; 3]);
                batch.normals.push([0., 1., 0.]);
                batch.uvs.push([0.; 2]);
                batch.light_ids.push([7., 0.]);
                batch.colors.push([1.; 4]);
                batch.tangents.push([1., 0., 0., 1.]);
                if directional {
                    for (values, packed) in batch.irradiance.iter_mut().zip(irradiance::pack(&light)) {
                        values.push(packed);
                    }
                }
            }
            batch.indices = vec![0, 1, 2];
            let mesh = batch.into_mesh();
            assert_eq!(mesh.count_vertices(), 3);
            for attribute in irradiance::ATTRIBUTES {
                assert_eq!(mesh.contains_attribute(attribute), directional);
                if directional {
                    let Some(bevy::mesh::VertexAttributeValues::Uint32x4(values)) = mesh.attribute(attribute) else { panic!("packed integer attribute lost"); };
                    assert_eq!(values.len(), 3);
                }
            }
        }
    }
}

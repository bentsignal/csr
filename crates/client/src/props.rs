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
use bevy::tasks::{ComputeTaskPool, TaskPool};
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

/// Fading (0x01), disabled shadow casting (0x10), and disabled self-shadowing
/// (0x80) do not prevent receiving spatial light from the rest of the map.
/// The lighting query already excludes the receiving prop's own shapes. Large
/// no-self-shadow receivers still span independently exposed/covered surfaces.
/// Keep lighting-origin, ignored-normal and other unsupported receiver modes
/// on their old fallback. Flag meanings: srctools' public BSP format docs,
/// https://srctools.readthedocs.io/en/latest/modules/bsp.html#srctools.bsp.StaticPropFlags
fn supports_surface_lighting(flags: u8) -> bool {
    flags & !(0x01 | 0x10 | 0x80) == 0
}

/// Map ambient cube at the prop, plus the sun if its sample ray reaches sky.
/// This one-sample fallback remains approximate for large/combined models.
fn prop_light(
    bsp: &bsp::Bsp,
    collision: &mut collision::PropLightingQuery<'_>,
    sun: Option<Sun>,
    center: glam::Vec3,
    prop: &bsp::StaticProp,
    prop_ordinal: usize,
) -> PropLight {
    // USE_LIGHTING_ORIGIN supplies a world-space point, not a local offset.
    // Keep other unsupported receiver modes and unflagged props unchanged.
    // An explicit point needs no guessed upward nudge: trim centers plus four
    // units can fall inside a ceiling while their authored sample is in air.
    let authored_origin = prop.lighting_origin.filter(|point|
        prop.flags & 0x02 != 0 && prop.flags & !(0x01 | 0x02 | 0x10) == 0 && point.is_finite());
    let sample = authored_origin.unwrap_or(center);
    let cube = authored_origin.and_then(|point| bsp.ambient_cube(point))
        .or_else(|| bsp.ambient_cube(center + glam::Vec3::Z * 4.0))
        .or_else(|| bsp.ambient_cube(prop.origin + glam::Vec3::Z * 16.0))
        .unwrap_or([[0.3; 3]; 6])
        .map(glam::Vec3::from);
    match sun {
        Some(s) => {
            // Exclude this placed prop's own hulls instead of jumping beyond
            // its radius, which can skip intervening ceilings and walls.
            let tr = collision.trace_prop_lighting_ray(sample, sample + s.to_sun * 16384.0, prop_ordinal);
            // A point inside any remaining solid is not a valid sky sample,
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

type SurfaceKey = ([u32; 3], [u32; 3]);
type PendingSurface<T> = (SurfaceKey, glam::Vec3, glam::Vec3, T);

/// Cache the exact data mesh assembly consumes, including half-float packing.
/// Seam duplicates need neither another conversion nor another light evaluation.
#[derive(Clone, Copy, Default)]
struct DirectionalSample {
    color: glam::Vec3,
    packed: [[u32; 4]; 3],
}

impl DirectionalSample {
    fn new(light: PropLight, normal: glam::Vec3) -> Self {
        Self { color: light.eval(normal), packed: irradiance::pack(&light) }
    }
}

fn surface_key(position: glam::Vec3, normal: glam::Vec3) -> SurfaceKey {
    (position.to_array().map(f32::to_bits), normal.to_array().map(f32::to_bits))
}

/// Fill the existing instance cache before serial mesh assembly. Each task
/// owns its ray scratch and writes disjoint results; no reduction or changed
/// ray/ambient arithmetic is involved. Exact seam duplicates, including those
/// from earlier meshes, still issue only one query per lighting mode.
#[allow(clippy::too_many_arguments)]
fn prepare_surface_lighting<'a, T: Copy + Send, F>(
    cache: &mut HashMap<SurfaceKey, T>,
    pending: &mut Vec<PendingSurface<T>>,
    vertices: &[mdl::Vertex],
    prop: &bsp::StaticProp,
    rot: glam::Mat3,
    scale: f32,
    initial: T,
    queries: &mut Vec<collision::PropLightingQuery<'a>>,
    pool: Option<&TaskPool>,
    calculate: F,
) where
    F: Fn(&mut collision::PropLightingQuery<'_>, glam::Vec3, glam::Vec3) -> T + Sync,
{
    const MIN_SAMPLES_PER_TASK: usize = 256;
    const MAX_TASKS: usize = 8;
    // Leave small meshes and callers without a parallel pool on the original
    // single-pass path. Query workspaces are allocated lazily and die with spawn.
    let Some(pool) = pool.filter(|p| p.thread_num() > 1) else { return; };
    if vertices.len() < MIN_SAMPLES_PER_TASK * 2 { return; }
    pending.clear();
    for v in vertices {
        let position = prop.origin + rot * (v.position * scale);
        let normal = (rot * v.normal).normalize_or_zero();
        let key = surface_key(position, normal);
        if let std::collections::hash_map::Entry::Vacant(entry) = cache.entry(key) {
            // Reserve the key to deduplicate this mesh. Every placeholder is
            // replaced below, before the cache can be read by mesh assembly.
            entry.insert(initial);
            pending.push((key, position, normal, initial));
        }
    }
    if pending.is_empty() { return; }
    let tasks = (pending.len() / MIN_SAMPLES_PER_TASK).clamp(1, pool.thread_num().min(MAX_TASKS));
    if tasks == 1 {
        for (_, position, normal, result) in pending.iter_mut() {
            *result = calculate(&mut queries[0], *position, *normal);
        }
    } else {
        while queries.len() < tasks {
            queries.push(queries[0].fork());
        }
        let chunk_size = pending.len().div_ceil(tasks);
        let calculate = &calculate;
        pool.scope(|scope| {
            for (chunk, query) in pending.chunks_mut(chunk_size).zip(queries.iter_mut()) {
                scope.spawn(async move {
                    for (_, position, normal, result) in chunk {
                        *result = calculate(query, *position, *normal);
                    }
                });
            }
        });
    }
    for (key, _, _, result) in pending.drain(..) {
        *cache.get_mut(&key).expect("reserved surface sample") = result;
    }
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

#[derive(Default)]
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
    let mut import = PropImport::new(level, collision.prepare_prop_lighting());
    while !import.step(commands, level, meshes, materials, images, textures, shared,
        exposure, sun, sky, std::time::Duration::MAX) {}
    import.stats
}

type BatchKey = (AssetId<WorldMaterial>, bool, [i32; 3], bool);

/// Purpose-built loading continuation. GPU assets stay on the main thread.
/// Cache keys, per-instance lighting scope and batch append order match spawn.
pub(crate) struct PropImport<'a> {
    models: HashMap<u16, Option<(mdl::Model, Vec<bool>)>>,
    model_cursor: usize,
    tangent_cursor: (usize, usize),
    tangent_done: std::collections::HashSet<u16>,
    tangent_cache: HashMap<(u16, usize), Vec<[f32; 4]>>,
    material_cache: HashMap<String, Option<Handle<WorldMaterial>>>,
    model_materials: HashMap<(u16, usize), Option<Handle<WorldMaterial>>>,
    batches: HashMap<BatchKey, (Handle<WorldMaterial>, Batch)>,
    uploads: Option<std::collections::hash_map::IntoIter<BatchKey, (Handle<WorldMaterial>, Batch)>>,
    light_data: Vec<u8>,
    lighting_queries: Vec<collision::PropLightingQuery<'a>>,
    surface_cache: HashMap<SurfaceKey, glam::Vec3>,
    directional_cache: HashMap<SurfaceKey, DirectionalSample>,
    surface_pending: Vec<PendingSurface<glam::Vec3>>,
    directional_pending: Vec<PendingSurface<DirectionalSample>>,
    instance: Option<InstanceImport>,
    prop_cursor: usize,
    mesh_cursor: usize,
    vertex_cursor: usize,
    index_cursor: usize,
    mesh_base: u32,
    authored_meshes: usize,
    authored_vertices: usize,
    surface_vertices: usize,
    directional_vertices: usize,
    detail_meshes: usize,
    tinted_instances: usize,
    pub stats: PropStats,
}

struct InstanceImport {
    authored: Option<Vec<Option<Vec<glam::Vec3>>>>,
    light: PropLight,
}

const PROP_TEXELS: usize = 9;
// Both limits are render preparation policy, independent of simulation ticks.
// Keep enough samples for the existing eight 256-sample lighting workers.
// Smaller chunks halve parallelism on large props and lengthen map loads.
const VERTICES_PER_CHUNK: usize = 2048;
const INDICES_PER_CHUNK: usize = 3072;

impl<'a> PropImport<'a> {
    pub fn new(level: &level::Level, query: collision::PropLightingQuery<'a>) -> Self {
        Self {
            models: HashMap::new(), model_cursor: 0, tangent_cursor: (0, 0),
            tangent_done: Default::default(),
            tangent_cache: HashMap::new(), material_cache: HashMap::new(),
            model_materials: HashMap::new(), batches: HashMap::new(), uploads: None,
            light_data: vec![0; level.bsp.static_props.props.len().max(1) * PROP_TEXELS * 8],
            lighting_queries: vec![query], surface_cache: HashMap::new(),
            directional_cache: HashMap::new(), surface_pending: Vec::new(),
            directional_pending: Vec::new(), instance: None, prop_cursor: 0,
            mesh_cursor: 0, vertex_cursor: 0, index_cursor: 0, mesh_base: 0,
            authored_meshes: 0, authored_vertices: 0, surface_vertices: 0,
            directional_vertices: 0, detail_meshes: 0, tinted_instances: 0,
            stats: PropStats::default(),
        }
    }

    pub fn progress(&self, level: &level::Level) -> String {
        format!("Preparing props — {} / {}", self.prop_cursor, level.bsp.static_props.props.len())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self, commands: &mut Commands, level: &level::Level,
        meshes: &mut Assets<Mesh>, materials: &mut Assets<WorldMaterial>,
        images: &mut Assets<Image>, textures: &mut TextureCache,
        shared: &SharedHandles, exposure: f32, sun: Option<Sun>,
        sky: Option<bsp::SkyCamera>, budget: std::time::Duration,
    ) -> bool {
    let started = std::time::Instant::now();
    let fs = &level.vfs;
    let bsp = &level.bsp;
    let read = |p: &str| fs.read_string(p);
    let Self { models, model_cursor, tangent_cursor, tangent_done, tangent_cache, material_cache,
        model_materials, batches, uploads, light_data, lighting_queries, surface_cache,
        directional_cache, surface_pending, directional_pending, instance, prop_cursor,
        mesh_cursor, vertex_cursor, index_cursor, mesh_base, authored_meshes, authored_vertices,
        surface_vertices, directional_vertices, detail_meshes, tinted_instances, stats } = self;

    // Load every referenced model once, with per-mesh winding fixes.
    while let Some(prop) = bsp.static_props.props.get(*model_cursor) {
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
        *model_cursor += 1;
        if started.elapsed() >= budget { return false; }
    }
    // Calculate tangents once per unique model mesh, then rotate per instance.
    // Doing MikkTSpace on all six million placed vertices stalls map loading.
    // Visit first references in BSP order; repeated model IDs reuse tangents.
    while let Some(prop) = bsp.static_props.props.get(tangent_cursor.0) {
        let id = prop.model;
        if tangent_done.contains(&id) { tangent_cursor.0 += 1; continue; }
        let Some(Some((model, flips))) = models.get(&id) else {
            tangent_cursor.0 += 1; tangent_cursor.1 = 0; continue;
        };
        while let Some((mesh, flip)) = model.meshes.get(tangent_cursor.1).zip(flips.get(tangent_cursor.1)) {
            let index = tangent_cursor.1;
            tangent_cursor.1 += 1;
            if tangent_cache.contains_key(&(id, index)) { continue; }
            if let Some(tangents) = mesh.vertices.iter().map(|v| v.tangent).collect::<Option<Vec<_>>>() {
                tangent_cache.insert((id, index), tangents);
                if started.elapsed() >= budget { return false; }
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
            if started.elapsed() >= budget { return false; }
        }
        tangent_done.insert(id);
        tangent_cursor.0 += 1; tangent_cursor.1 = 0;
    }
    stats.failed_models = models.values().filter(|m| m.is_none()).count();

    let lighting_pool = ComputeTaskPool::try_get().map(|pool| &**pool);
    // Retain peak-instance/mesh capacity only until this map's spawn finishes.
    // Clear both caches per instance: positions alone do not encode the owner
    // excluded from occlusion or the instance's missing-leaf fallback cube.
    while let Some(prop) = bsp.static_props.props.get(*prop_cursor) {
        let prop_index = *prop_cursor;
        let Some(Some((model, flips))) = models.get(&prop.model) else {
            *prop_cursor += 1; continue;
        };
        let rot = mdl::angles_to_mat3(prop.angles);
        let scale = if prop.scale > 0.0 { prop.scale } else { 1.0 };
        if instance.is_none() {
        let authored = vertex_lighting::load(level, prop_index, model);
        let center = prop.origin + rot * ((model.hull_min + model.hull_max) * 0.5 * scale);
        let light = prop_light(bsp, &mut lighting_queries[0], sun, center, prop, prop_index);
        // Exact geometric duplicates at UV/material seams share the query.
        // No spatial quantization: neighboring vertices can straddle a shadow
        // or leaf boundary. Clear between placed instances so another instance
        // of the same model cannot reuse its lighting/occlusion.
        surface_cache.clear();
        directional_cache.clear();
        let cube = irradiance::CUBE_ORDER.map(|i| light.cube[i].extend(1.0));
        // Opacity changes need separate native admission and material modes.
        // Unsupported layouts/alpha retain the previous identity modulation.
        let tint = match prop.diffuse_modulation {
            Some([r, g, b, 255]) => {
                if [r, g, b] != [255; 3] {
                    *tinted_instances += 1;
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
        *instance = Some(InstanceImport { authored, light });
        if started.elapsed() >= budget { return false; }
        }
        let InstanceImport { authored, light } = instance.as_ref().unwrap();

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

        while let Some((mesh, &flip)) = model.meshes.get(*mesh_cursor).zip(flips.get(*mesh_cursor)) {
            let mesh_index = *mesh_cursor;
            let tex = model.skin_texture(prop.skin.max(0) as usize, mesh.material);
            let material = model_materials.entry((prop.model, tex)).or_insert_with(|| {
                let candidates = model.material_candidates(tex);
                let key = candidates.first().cloned().unwrap_or_default();
                material_cache.entry(key).or_insert_with(|| {
                    let vmt = candidates.iter().find_map(|c| vmt::Material::load(c, &read));
                    // Unsupported water/refraction effects otherwise become
                    // opaque white geometry. Omit their visual mesh until the
                    // corresponding renderer exists; collision stays intact.
                    if vmt.as_ref().is_some_and(|m| m.is_water() || m.shader == "refract") {
                        return None;
                    }
                    stats.materials += 1;
                    Some(materials.add(material_from_vmt(vmt.as_ref(), fs, textures, images, shared, exposure, false)))
                }).clone()
            }).clone();
            let Some(material) = material else { *mesh_cursor += 1; continue; };
            if let Some(m) = materials.get(&material).filter(|m|
                *vertex_cursor == 0 && *index_cursor == 0 && m.params.flags & crate::material::flags::DETAIL != 0) {
                *detail_meshes += 1;
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
                if *vertex_cursor == 0 && *index_cursor == 0 {
                    *authored_meshes += 1;
                    *authored_vertices += colors.len();
                    debug!("Authored equal-group prop: instance={prop_index} mesh={mesh_index} vertices={} model={}",
                        colors.len(), bsp.static_props.models[prop.model as usize]);
                }
            }
            let end = (*vertex_cursor + VERTICES_PER_CHUNK).min(mesh.vertices.len());
            let vertices = &mesh.vertices[*vertex_cursor..end];
            if spatial_fallback {
                prepare_surface_lighting(surface_cache, surface_pending, vertices, prop, rot, scale, glam::Vec3::ZERO,
                    lighting_queries, lighting_pool, |query, position, normal|
                        surface_light(bsp, query, sun, light, position, normal, prop_index));
            } else if directional_fallback {
                prepare_surface_lighting(directional_cache, directional_pending, vertices, prop, rot, scale, DirectionalSample::default(),
                    lighting_queries, lighting_pool, |query, position, normal|
                        DirectionalSample::new(surface_directional_light(bsp, query, sun, light, position, normal, prop_index), normal));
            }
            let (_, batch) = batches
                .entry((material.id(), in_sky.is_some(), crate::world_mesh::spatial_cell(prop.origin), directional_fallback))
                .or_insert_with(|| (material.clone(), Batch::default()));
            if *vertex_cursor == 0 && *index_cursor == 0 { *mesh_base = batch.positions.len() as u32; }
            let base = *mesh_base;
            let tangents = tangent_cache.get(&(prop.model, mesh_index));
            for (offset, v) in vertices.iter().enumerate() {
                let vertex_index = *vertex_cursor + offset;
                let n_source = (rot * v.normal).normalize_or_zero();
                let tangent = tangents.and_then(|t| t.get(vertex_index)).copied().unwrap_or([1.0,0.0,0.0,1.0]);
                let t = to_bevy(rot * glam::Vec3::new(tangent[0], tangent[1], tangent[2]));
                batch.tangents.push([t.x,t.y,t.z,tangent[3]]);
                let lit = if let Some(colors) = authored_colors {
                    colors[vertex_index]
                } else if spatial_fallback {
                    *surface_vertices += 1;
                    let position = prop.origin + rot * (v.position * scale);
                    let key = surface_key(position, n_source);
                    *surface_cache.entry(key).or_insert_with(||
                        surface_light(bsp, &mut lighting_queries[0], sun, light, position, n_source, prop_index))
                } else if directional_fallback {
                    *directional_vertices += 1;
                    let position = prop.origin + rot * (v.position * scale);
                    let key = surface_key(position, n_source);
                    let surface = directional_cache.entry(key).or_insert_with(||
                        DirectionalSample::new(surface_directional_light(bsp, &mut lighting_queries[0], sun, light, position, n_source, prop_index), n_source));
                    for (values, packed) in batch.irradiance.iter_mut().zip(surface.packed) {
                        values.push(packed);
                    }
                    // Retain the color stream/layout; the directional shader
                    // replaces it once, after perturbing the normal.
                    surface.color
                } else {
                    light.eval(n_source)
                };
                batch.positions.push(affine.transform_point3(to_bevy(v.position)).to_array());
                batch.normals.push(to_bevy(n_source).to_array());
                batch.uvs.push(v.uv.to_array());
                batch.light_ids.push([prop_index as f32, 0.0]);
                batch.colors.push([lit.x, lit.y, lit.z, 1.0]);
            }
            *vertex_cursor = end;
            if end < mesh.vertices.len() {
                if started.elapsed() >= budget { return false; }
                continue;
            }
            let index_end = (*index_cursor + INDICES_PER_CHUNK).min(mesh.indices.len() / 3 * 3);
            for tri in mesh.indices[*index_cursor..index_end].chunks_exact(3) {
                if flip {
                    batch.indices.extend_from_slice(&[base + tri[0], base + tri[2], base + tri[1]]);
                } else {
                    batch.indices.extend_from_slice(&[base + tri[0], base + tri[1], base + tri[2]]);
                }
            }
            *index_cursor = index_end;
            if index_end < mesh.indices.len() / 3 * 3 {
                if started.elapsed() >= budget { return false; }
                continue;
            }
            stats.triangles += mesh.indices.len() / 3;
            *mesh_cursor += 1; *vertex_cursor = 0; *index_cursor = 0;
            if started.elapsed() >= budget { return false; }
        }
        stats.instances += 1;
        *instance = None; *prop_cursor += 1; *mesh_cursor = 0;
        if started.elapsed() >= budget { return false; }
    }

    if uploads.is_none() {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let lighting = images.add(Image::new(
        Extent3d { width: PROP_TEXELS as u32, height: bsp.static_props.props.len().max(1) as u32, depth_or_array_layers: 1 },
        TextureDimension::D2, std::mem::take(light_data), TextureFormat::Rgba16Float, RenderAssetUsages::RENDER_WORLD,
    ));
    for handle in material_cache.values().flatten() {
        if let Some(mut mat) = materials.get_mut(handle) { mat.prop_light = lighting.clone(); }
    }
    *uploads = Some(std::mem::take(batches).into_iter());
    if started.elapsed() >= budget { return false; }
    }
    for ((_, in_sky, _, _), (material, batch)) in uploads.as_mut().unwrap() {
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
        if started.elapsed() >= budget { return false; }
    }
    info!("Authored equal-group prop lighting: meshes={authored_meshes} vertices={authored_vertices}; unsupported meshes use ambient/Sun fallback");
    info!("Spatial ambient/Sun prop fallback: plain_vertices={surface_vertices} directional_vertices={directional_vertices} directional_attribute_bytes={}; special-mode props retain instance lighting", *directional_vertices * 48);
    info!("Authored static-prop diffuse tint: nonwhite_instances={tinted_instances}");
    info!("Authored ordinary static-prop detail: meshes={detail_meshes}; unsupported layers retain base material");
    true
    }
}

#[cfg(test)]
mod lighting_tests {
    use super::*;
    use glam::Vec3 as SourceVec3;

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
            scale: 1.0, lighting_origin: None, diffuse_modulation: None,
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
        prop_light(map, &mut w.prepare_prop_lighting(), Some(SUN), SourceVec3::ZERO, &map.static_props.props[0], 0)
    }

    #[test]
    fn chunked_prop_streams_match_serial_lighting_transforms_and_winding() {
        let mut level = crate::scene_import::fixture_level();
        let (mut map, mut collision) = fixture(true);
        map.static_props.props[1].angles.z = 30.;
        map.static_props.props[1].scale = 1.25;
        map.static_props.props[1].diffuse_modulation = Some([127, 191, 255, 255]);
        add_world_bounds(&mut collision, SourceVec3::new(-256., -256., 33.),
            SourceVec3::new(32., 256., 44.), "partial_cover");
        collision.finalize();
        level.bsp = map;
        level.collision = std::sync::Arc::new(collision);
        let mut mesh = mdl::Mesh::default();
        // Cross multiple vertex and index chunks, with seam duplicates across
        // the chunk boundary. Authored tangents must retain their handedness.
        for i in 0..2307 {
            mesh.vertices.push(mdl::Vertex {
                position: SourceVec3::new((i % 1000) as f32 * 0.25 - 128., 0., 1.),
                normal: SourceVec3::Z, uv: glam::Vec2::new(i as f32 / 2307., 0.5),
                tangent: Some([1., 0., 0., -1.]), ..Default::default()
            });
        }
        mesh.indices = (0..mesh.vertices.len() as u32).collect();
        mesh.indices.extend(mesh.indices.clone());
        let mut bumped = mesh.clone();
        bumped.material = 1;
        let model = mdl::Model { meshes: vec![mesh, bumped], ..Default::default() };
        let mut images = Assets::<Image>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        let vmt = vmt::Material::parse_with("VertexLitGeneric {}", &|_| None, 0).unwrap();
        let plain = material_from_vmt(Some(&vmt), &level.vfs, &mut textures, &mut images, &shared, 1., false);
        let mut bumped = plain.clone();
        bumped.params.flags |= crate::material::flags::NORMAL;
        let mut materials = Assets::<WorldMaterial>::default();
        let handles = [materials.add(plain), materials.add(bumped)];
        let mut import = PropImport::new(&level, collision::PropLightingQuery::owned(level.collision.clone()));
        import.models.insert(0, Some((model.clone(), vec![false, true])));
        for (index, handle) in handles.iter().enumerate() {
            import.material_cache.insert(index.to_string(), Some(handle.clone()));
            import.model_materials.insert((0, index), Some(handle.clone()));
        }
        import.stats.materials = 2;
        let mut world = World::new();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut slices = 0;
        let mut split_vertices = false;
        let mut split_indices = false;
        while import.prop_cursor < level.bsp.static_props.props.len() {
            assert!(!import.step(&mut commands, &level, &mut meshes, &mut materials,
                &mut images, &mut textures, &shared, 1., Some(SUN), None, std::time::Duration::ZERO));
            split_vertices |= import.vertex_cursor > 0 && import.vertex_cursor < model.meshes[0].vertices.len();
            split_indices |= import.index_cursor > 0 && import.index_cursor < model.meshes[0].indices.len();
            slices += 1;
            assert!(slices < 100, "cursor must advance without restarting a mesh");
        }
        assert!(split_vertices && split_indices);
        assert_eq!(import.stats.instances, 2);
        assert_eq!(import.stats.triangles, 2 * 2 * 2 * 769);
        assert_eq!((import.stats.materials, import.stats.failed_models), (2, 0));
        assert_eq!(import.tangent_cache.len(), 2);
        let mut offsets = HashMap::new();
        let mut query = level.collision.prepare_prop_lighting();
        for (ordinal, prop) in level.bsp.static_props.props.iter().enumerate() {
            let rot = mdl::angles_to_mat3(prop.angles);
            let scale = if prop.scale > 0. { prop.scale } else { 1. };
            let center = prop.origin + rot * ((model.hull_min + model.hull_max) * 0.5 * scale);
            let light = prop_light(&level.bsp, &mut query, Some(SUN), center, prop, ordinal);
            for (index, mesh) in model.meshes.iter().enumerate() {
                let key = (handles[index].id(), false, crate::world_mesh::spatial_cell(prop.origin), index == 1);
                let (_, batch) = &import.batches[&key];
                let base = *offsets.entry(key).or_insert(0usize);
                let transform = Transform { translation: to_bevy(prop.origin), rotation: rotation_to_bevy(rot),
                    scale: Vec3::splat(scale) }.compute_affine();
                for (i, v) in mesh.vertices.iter().enumerate() {
                    let normal = (rot * v.normal).normalize_or_zero();
                    let position = prop.origin + rot * (v.position * scale);
                    let serial = surface_directional_light(&level.bsp, &mut query, Some(SUN),
                        &light, position, normal, ordinal);
                    let color = if index == 0 { surface_light(&level.bsp, &mut query, Some(SUN),
                        &light, position, normal, ordinal) } else { serial.eval(normal) };
                    assert_eq!(batch.positions[base + i].map(f32::to_bits),
                        transform.transform_point3(to_bevy(v.position)).to_array().map(f32::to_bits));
                    assert_eq!(batch.normals[base + i].map(f32::to_bits), to_bevy(normal).to_array().map(f32::to_bits));
                    assert_eq!(batch.uvs[base + i], v.uv.to_array());
                    assert_eq!(batch.light_ids[base + i], [ordinal as f32, 0.]);
                    assert_eq!(batch.colors[base + i].map(f32::to_bits), color.extend(1.).to_array().map(f32::to_bits));
                    let tangent = to_bevy(rot * SourceVec3::X);
                    assert_eq!(batch.tangents[base + i], [tangent.x, tangent.y, tangent.z, -1.]);
                    if index == 1 {
                        for (stream, expected) in batch.irradiance.iter().zip(irradiance::pack(&serial)) {
                            assert_eq!(stream[base + i], expected);
                        }
                    }
                }
                for (tri_index, tri) in mesh.indices.chunks_exact(3).enumerate() {
                    let expected = if index == 1 { [tri[0], tri[2], tri[1]] } else { [tri[0], tri[1], tri[2]] };
                    assert_eq!(&batch.indices[base * 2 + tri_index * 3..base * 2 + tri_index * 3 + 3],
                        &expected.map(|i| base as u32 + i));
                }
                *offsets.get_mut(&key).unwrap() += mesh.vertices.len();
            }
        }
        for (key, count) in offsets {
            let (_, batch) = &import.batches[&key];
            assert_eq!(batch.positions.len(), count);
            assert_eq!(batch.indices.len(), count * 2);
        }
        let expected_draws = import.batches.len();
        while !import.step(&mut commands, &level, &mut meshes, &mut materials,
            &mut images, &mut textures, &shared, 1., Some(SUN), None, std::time::Duration::ZERO) {}
        drop(commands);
        queue.apply(&mut world);
        assert_eq!(import.stats.draws, expected_draws);
        assert_eq!(world.query::<&Mesh3d>().iter(&world).count(), expected_draws);
        let lighting = &materials.get(&handles[0]).unwrap().prop_light;
        assert_eq!(images.get(lighting).unwrap().data.as_ref().unwrap().len(), 2 * PROP_TEXELS * 8);
    }

    #[test]
    fn parallel_surface_cache_matches_serial_queries_and_keeps_instance_scope() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let pool = bevy::tasks::TaskPoolBuilder::new().num_threads(4).build();
        let (mut map, mut w) = fixture(true);
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
        map.leaf_ambient = [0.1, 0.01].map(|scale| bsp::AmbientSample {
            cube: std::array::from_fn(|i| [scale * (i + 1) as f32; 3]), pos: [128; 3],
        }).into();
        add_world_bounds(&mut w, SourceVec3::new(-256., -256., 33.),
            SourceVec3::new(32., 256., 44.), "partial_cover");
        w.finalize();
        let mut mesh = mdl::Mesh::default();
        for i in 0..1024 {
            mesh.vertices.push(mdl::Vertex {
                position: SourceVec3::new(-128. + i as f32 * 0.25, 0., 1.),
                normal: [SourceVec3::Z, -SourceVec3::Z, SourceVec3::X][i % 3],
                ..Default::default()
            });
        }
        // UV/material seam duplicates must not become extra parallel queries.
        mesh.vertices.extend(mesh.vertices.clone());
        let mut queries = vec![w.prepare_prop_lighting()];
        let mut plain_cache = HashMap::new();
        let mut directional_cache = HashMap::new();
        let mut plain_pending = Vec::new();
        let mut directional_pending = Vec::new();
        let mut previous_instance = None;
        for (ordinal, prop) in map.static_props.props.iter().enumerate() {
            let rot = mdl::angles_to_mat3(prop.angles);
            let scale = 1.25;
            // Different placed meshes may meet at exactly the same world
            // samples. Keep these points identical while changing the owner
            // excluded from the Sun ray, so stale instance data is detectable.
            let mut placed_mesh = mesh.clone();
            for v in &mut placed_mesh.vertices {
                v.position -= prop.origin / scale;
            }
            let center = prop.origin;
            let light = prop_light(&map, &mut queries[0], Some(SUN), center, prop, ordinal);
            plain_cache.clear();
            directional_cache.clear();
            let calls = AtomicUsize::new(0);
            // The second pass represents a later mesh of the same instance.
            for _ in 0..2 {
                prepare_surface_lighting(&mut plain_cache, &mut plain_pending, &placed_mesh.vertices, prop, rot, scale, SourceVec3::ZERO,
                    &mut queries, Some(&pool), |query, position, normal| {
                        calls.fetch_add(1, Ordering::Relaxed);
                        surface_light(&map, query, Some(SUN), &light, position, normal, ordinal)
                    });
                prepare_surface_lighting(&mut directional_cache, &mut directional_pending, &placed_mesh.vertices, prop, rot, scale, DirectionalSample::default(),
                    &mut queries, Some(&pool), |query, position, normal| {
                        calls.fetch_add(1, Ordering::Relaxed);
                        DirectionalSample::new(surface_directional_light(&map, query, Some(SUN), &light, position, normal, ordinal), normal)
                    });
            }
            assert_eq!(calls.load(Ordering::Relaxed), 2048);
            assert_eq!(queries.len(), 4, "reuse bounded query workspaces across instances");
            let mut actual = Vec::new();
            for v in &placed_mesh.vertices {
                let position = prop.origin + rot * (v.position * scale);
                let normal = (rot * v.normal).normalize_or_zero();
                let key = surface_key(position, normal);
                let expected = surface_directional_light(&map, &mut queries[0], Some(SUN), &light,
                    position, normal, ordinal);
                let cached = directional_cache[&key];
                // Compare emitted attributes with the original uncached serial
                // path, including full-float color bits and half-float words.
                assert_eq!(cached.color.to_array().map(f32::to_bits),
                    expected.eval(normal).to_array().map(f32::to_bits));
                assert_eq!(cached.packed, irradiance::pack(&expected));
                let plain = surface_light(&map, &mut queries[0], Some(SUN), &light,
                    position, normal, ordinal);
                assert_eq!(plain_cache[&key], plain);
                actual.push((plain, cached.packed));
            }
            if let Some(previous) = previous_instance {
                assert_ne!(actual, previous, "another placed instance must sample its own lighting");
            }
            previous_instance = Some(actual);
        }
    }

    #[test]
    fn decoded_lighting_origin_binds_flag_world_point_normal_and_sun_ray() {
        let (mut map, mut w) = fixture(false);
        // A synthetic sprp v11 record, with distinct placement and lighting
        // points. Decode through the same public parser as a loaded map.
        let mut bytes = vec![0u8; 12 + 80];
        bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
        for (offset, value) in [
            (0, 80.0f32), (4, -64.0), (16, 90.0),
            (44, -80.0), (48, 64.0), (52, 1.0), (76, 3.0),
        ] {
            bytes[12 + offset..16 + offset].copy_from_slice(&value.to_le_bytes());
        }
        let mut directory = vec![0u8; 20];
        directory[..4].copy_from_slice(&1u32.to_le_bytes());
        directory[4..8].copy_from_slice(&i32::from_be_bytes(*b"sprp").to_le_bytes());
        directory[10..12].copy_from_slice(&11u16.to_le_bytes());
        directory[16..20].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        let mut prop = bsp::staticprops::parse(&bytes, &directory).unwrap().props.remove(0);
        let authored = SourceVec3::new(-80.0, 64.0, 1.0);
        assert_eq!(prop.lighting_origin, Some(authored));

        // A four-unit nudge, a model transform, or a Bevy-axis conversion
        // sends the authored point into a different leaf. Cube faces differ
        // too, so evaluating a model-local/Bevy normal cannot accidentally pass.
        map.planes = vec![
            bsp::Plane { normal: SourceVec3::Z, dist: 2.0, kind: 2 },
            bsp::Plane { normal: SourceVec3::Y, dist: 32.0, kind: 1 },
            bsp::Plane { normal: SourceVec3::X, dist: 0.0, kind: 0 },
        ];
        let node = |plane, children| bsp::Node {
            plane, children, mins: [-256; 3], maxs: [256; 3], first_face: 0, num_faces: 0,
        };
        map.nodes = vec![node(0, [-1, 1]), node(1, [2, -1]), node(2, [-1, -2])];
        let leaf = bsp::Leaf {
            contents: 0, cluster: 0, area: 0, mins: [-256; 3], maxs: [256; 3],
            first_leaf_face: 0, num_leaf_faces: 0, first_leaf_brush: 0, num_leaf_brushes: 0,
        };
        map.leafs = vec![leaf, leaf];
        map.leaf_ambient_index = vec![(1, 0), (1, 1)];
        let warm: [[f32; 3]; 6] = std::array::from_fn(|i| [0.01 * (i + 1) as f32, 0.005, 0.002]);
        map.leaf_ambient = vec![
            bsp::AmbientSample { cube: [[0.3; 3]; 6], pos: [128; 3] },
            bsp::AmbientSample { cube: warm, pos: [128; 3] },
        ];
        add_world_bounds(&mut w, SourceVec3::new(-256.0, -256.0, 33.0),
            SourceVec3::new(-32.0, 256.0, 44.0), "authored_point_roof");
        w.finalize();
        let center = SourceVec3::new(80.0, 0.0, 0.0);
        let normal = mdl::angles_to_mat3(prop.angles) * SourceVec3::X;
        for flags in [2, 3, 18, 19] {
            prop.flags = flags;
            let light = prop_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), center, &prop, 0);
            assert_eq!(light.sun, SourceVec3::ZERO, "flags {flags}: ray must start at authored point");
            assert!((light.eval(normal) - SourceVec3::from(warm[2])).abs().max_element() < 1e-6);
            assert!(!supports_surface_lighting(flags), "authored point must survive the surface gate");
        }
        for flags in [0, 1, 16, 17, 0xff] {
            prop.flags = flags;
            let light = prop_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), center, &prop, 0);
            assert_eq!(light.sun, SUN.color, "flags {flags}: preserve center ray");
            assert!((light.cube[2] - SourceVec3::splat(0.3)).abs().max_element() < 1e-6);
        }
        // Missing layouts/non-finite points retain the existing fallback. A
        // valid authored point with no cube may borrow its old ambient cube,
        // but still uses the authored Sun ray.
        prop.flags = 3;
        for missing in [None, Some(SourceVec3::splat(f32::NAN))] {
            prop.lighting_origin = missing;
            let light = prop_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), center, &prop, 0);
            assert_eq!(light.sun, SUN.color);
        }
        prop.lighting_origin = Some(authored);
        map.leaf_ambient_index[1] = (0, 0);
        let light = prop_light(&map, &mut w.prepare_prop_lighting(), Some(SUN), center, &prop, 0);
        assert_eq!(light.sun, SourceVec3::ZERO);
        assert!((light.cube[2] - SourceVec3::splat(0.3)).abs().max_element() < 1e-6);
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
    fn no_cast_or_self_shadow_props_receive_spatial_light_without_bypassing_occluders() {
        for flags in [0x00, 0x01, 0x10, 0x11, 0x80, 0x81, 0x90, 0x91] {
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
            if ![0x00, 0x01, 0x10, 0x11, 0x80, 0x81, 0x90, 0x91].contains(&flags) {
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

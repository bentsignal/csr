//! Bounded, presentation-only func_areaportalwindow -> func_brush relations.
//! No portal culling, collision, simulation or entity I/O is implemented here.
//! Native admission must verify nearest-target-AABB distance and linear alpha;
//! authored endpoints alone do not establish the original distance law.
//! Only static authored name connections are checked. Arbitrary scripts and
//! dynamically added connections are outside this presentation subset.

use crate::{material::{flags, WorldMaterial}, player::LocalPlayer,
    textures::TextureCache, visibility::MapClusters, world_mesh::{self, LightmapAtlas, SharedHandles, WorldBuild}};
use bevy::prelude::*;
use std::{collections::HashSet, ops::Range};

const MAX_WINDOWS: usize = 256;
const MAX_FACES: usize = 4096;

#[derive(Clone, Debug)]
struct Window {
    model: usize,
    origin: glam::Vec3,
    mins: glam::Vec3,
    maxs: glam::Vec3,
    faces: Range<usize>,
    start: f32,
    end: f32,
    limit: f32,
}

impl Window {
    fn alpha(&self, eye: glam::Vec3) -> Option<f32> {
        if !eye.is_finite() { return None; }
        let distance = eye.distance(eye.clamp(self.mins, self.maxs));
        let fraction = ((distance - self.start) / (self.end - self.start)).clamp(0.0, 1.0);
        Some(self.limit + (1.0 - self.limit) * fraction)
    }
}

fn scalar<'a>(ent: &'a kv::Block, key: &str) -> Option<&'a str> {
    let mut values = ent.get_all(key);
    let value = values.next()?.as_str()?;
    values.next().is_none().then_some(value)
}

fn number(ent: &kv::Block, key: &str) -> Option<f32> {
    scalar(ent, key)?.parse::<f32>().ok().filter(|n| n.is_finite())
}

fn vector(text: &str) -> Option<glam::Vec3> {
    let mut values = text.split_whitespace();
    let value = glam::Vec3::new(values.next()?.parse().ok()?, values.next()?.parse().ok()?, values.next()?.parse().ok()?);
    (values.next().is_none() && value.is_finite()).then_some(value)
}

fn absent_or(ent: &kv::Block, key: &str, expected: &str) -> bool {
    ent.get(key).is_none() || scalar(ent, key) == Some(expected)
}

fn literal(name: &str) -> bool {
    !name.is_empty() && !name.chars().any(|c| c.is_whitespace() || matches!(c, '*' | '?' | '!' | ',' | '\u{1b}'))
}

// BSP output values use ESC; text/VMF fixtures can use commas. Do not execute
// any connection, including apparently harmless inputs on a controlled entity.
fn touches_io(block: &kv::Block, names: &[&str], own: bool) -> bool {
    touches_io_at(block, names, own, 0)
}

fn touches_io_at(block: &kv::Block, names: &[&str], own: bool, depth: usize) -> bool {
    if depth > 8 { return true; }
    block.iter().any(|(key, value)| match value {
        kv::Value::Block(child) => touches_io_at(child, names, own, depth + 1),
        kv::Value::Str(value) if key.to_ascii_lowercase().starts_with("on") => {
            let target = value.split([',', '\u{1b}']).next().unwrap_or("").trim();
            own || target.contains(['*', '?']) || names.iter().any(|name| target.eq_ignore_ascii_case(name))
                || target.eq_ignore_ascii_case("func_brush") || target.eq_ignore_ascii_case("func_areaportalwindow")
        }
        _ => false,
    })
}

fn relations(bsp: &bsp::Bsp) -> Vec<Window> {
    let controllers: Vec<_> = bsp.entities.iter().enumerate()
        .filter(|(_, ent)| ent.str("classname") == Some("func_areaportalwindow")).collect();
    if controllers.len() > MAX_WINDOWS { return Vec::new(); }
    controllers.iter().filter_map(|&(controller_index, controller)| {
        let target = scalar(controller, "target")?;
        if !literal(target) || scalar(controller, "PortalVersion") != Some("1")
            || !absent_or(controller, "BackgroundBModel", "")
            || !absent_or(controller, "parentname", "")
            || !absent_or(controller, "vscripts", "") { return None; }
        if controllers.iter().filter(|(_, ent)| ent.str("target").is_some_and(|name| name.eq_ignore_ascii_case(target))).count() != 1 { return None; }
        let mut targets = bsp.entities.iter().enumerate().filter(|(_, ent)|
            ent.str("targetname").is_some_and(|name| name.eq_ignore_ascii_case(target)));
        let (entity_index, entity) = targets.next()?;
        if targets.next().is_some() || !scalar(entity, "targetname").is_some_and(|name| name.eq_ignore_ascii_case(target))
            || scalar(entity, "classname") != Some("func_brush")
            || !absent_or(entity, "StartDisabled", "0") || !absent_or(entity, "rendermode", "0")
            || !absent_or(entity, "renderfx", "0") || !absent_or(entity, "renderamt", "255")
            || !absent_or(entity, "parentname", "") || !absent_or(entity, "vscripts", "") { return None; }
        let start = number(controller, "FadeStartDist")?;
        let end = number(controller, "FadeDist")?;
        let limit = number(controller, "TranslucencyLimit")?;
        if start < 0.0 || end <= start || !(0.0..=1.0).contains(&limit) { return None; }
        let model_name = scalar(entity, "model")?;
        let model = model_name.strip_prefix('*')?.parse::<usize>().ok()?;
        if model == 0 || bsp.entities.iter().filter(|ent| ent.str("model")
            .and_then(|name| name.strip_prefix('*')).and_then(|name| name.parse::<usize>().ok()) == Some(model)).count() != 1 { return None; }
        let def = bsp.models.get(model)?;
        let first = usize::try_from(def.first_face).ok()?;
        let count = usize::try_from(def.num_faces).ok()?;
        let last = first.checked_add(count)?;
        if count == 0 || count > MAX_FACES || last > bsp.faces.len()
            || !def.mins.is_finite() || !def.maxs.is_finite() || !def.mins.cmple(def.maxs).all() { return None; }
        if bsp.models.iter().enumerate().any(|(index, other)| index != model && other.first_face >= 0
            && other.num_faces > 0 && i64::from(other.first_face) < last as i64
            && i64::from(other.first_face) + i64::from(other.num_faces) > first as i64) { return None; }
        let origin = vector(scalar(entity, "origin")?)?;
        if entity.get("angles").is_some() && vector(scalar(entity, "angles")?)? != glam::Vec3::ZERO { return None; }
        let mins = def.mins + origin;
        let maxs = def.maxs + origin;
        if !mins.is_finite() || !maxs.is_finite() { return None; }
        let controller_name = if controller.get("targetname").is_some() { scalar(controller, "targetname")? } else { "" };
        if !controller_name.is_empty() && !literal(controller_name) { return None; }
        if bsp.entities.iter().enumerate().any(|(index, ent)| touches_io(ent, &[target, controller_name],
            index == entity_index || index == controller_index)) { return None; }
        Some(Window { model, origin, mins, maxs, faces: first..last, start, end, limit })
    }).collect()
}

pub(crate) struct WindowBuild {
    window: Window,
    build: WorldBuild,
}

impl WindowBuild {
    pub(crate) fn faces(&self) -> Range<usize> { self.window.faces.clone() }
    pub(crate) fn triangles(&self) -> usize { self.build.triangles }
}

// Restrict the initial material subset to opaque surfaces with opaque base
// textures: TRANSLUCENT must not reinterpret a specular/self-illumination mask
// as coverage. Unsupported content stays in its previous static batches.
fn supported_material(mat: &vmt::Material) -> bool {
    matches!(mat.shader.as_str(), "lightmappedgeneric" | "unlitgeneric")
        && !mat.translucent() && !mat.alpha_test() && !mat.additive()
        && mat.params.get("proxies").is_none()
        && mat.params.get("$detail").is_none()
        && (mat.params.get("$alpha").is_none() || number(&mat.params, "$alpha") == Some(1.0))
        && !mat.flag("$phong")
}

pub(crate) fn build(level: &level::Level, atlas: &LightmapAtlas, unavailable: &HashSet<usize>,
    textures: &mut TextureCache, images: &mut Assets<Image>) -> Vec<WindowBuild> {
    let mut material_support = std::collections::HashMap::new();
    relations(&level.bsp).into_iter().filter_map(|window| {
        if window.faces.clone().any(|face| unavailable.contains(&face)) { return None; }
        let build = world_mesh::build_brush_model(&level.bsp, atlas, window.model, window.origin);
        if build.meshes.is_empty() || !build.meshes.iter().all(|(name, _)| {
            *material_support.entry(name.clone()).or_insert_with(|| {
                let Some(mat) = vmt::Material::load(name, &|path| level.vfs.read_string(path)) else { return false; };
                supported_material(&mat) && mat.base_texture().and_then(|base|
                    textures.load(&level.vfs, images, base, true)).is_some_and(|texture| !texture.has_alpha)
            })
        }) { return None; }
        Some(WindowBuild { window, build })
    }).collect()
}

#[derive(Component)]
pub(crate) struct WindowFade {
    window: Window,
    base_alpha: f32,
    last_alpha: f32,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn(commands: &mut Commands, level: &level::Level, builds: &[WindowBuild],
    meshes: &mut Assets<Mesh>, materials: &mut Assets<WorldMaterial>, images: &mut Assets<Image>,
    textures: &mut TextureCache, shared: &SharedHandles, exposure: f32) -> usize {
    let mut draws = 0;
    for prepared in builds {
        for (name, mesh) in &prepared.build.meshes {
            let source = vmt::Material::load(name, &|path| level.vfs.read_string(path));
            let mut material = world_mesh::material_from_vmt(source.as_ref(), &level.vfs,
                textures, images, shared, exposure, true);
            let base_alpha = material.params.color.w;
            material.params.flags |= flags::TRANSLUCENT;
            // Stay in the transparent pipeline at both endpoints; material
            // updates change one uniform without rebuilding meshes/pipelines.
            material.alpha_mode = AlphaMode::Blend;
            // PVS owns the parent; fade owns only its child. Neither system can
            // accidentally unhide a surface rejected by the other.
            let parent = commands.spawn((Transform::IDENTITY, Visibility::Inherited, crate::session::MatchEntity,
                MapClusters::mesh(&level.bsp, mesh))).id();
            commands.spawn((Mesh3d(meshes.add(mesh.clone())), MeshMaterial3d(materials.add(material)),
                Transform::IDENTITY, ChildOf(parent), WindowFade {
                    window: prepared.window.clone(), base_alpha, last_alpha: f32::NAN,
                }));
            draws += 1;
        }
    }
    draws
}

pub(crate) fn update(local: Res<LocalPlayer>, mut materials: ResMut<Assets<WorldMaterial>>,
    mut windows: Query<(&mut WindowFade, &MeshMaterial3d<WorldMaterial>, &mut Visibility)>) {
    for (mut fade, handle, mut visibility) in &mut windows {
        // Invalid camera data retains the last valid state; never manufacture
        // a transparent capture from NaN/Infinity.
        let Some(alpha) = fade.window.alpha(local.render_eye) else { continue; };
        if fade.last_alpha == alpha { continue; }
        let Some(mut material) = materials.get_mut(&handle.0) else { continue; };
        material.params.color.w = fade.base_alpha * alpha;
        *visibility = if alpha == 0.0 { Visibility::Hidden } else { Visibility::Inherited };
        fade.last_alpha = alpha;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> bsp::Bsp {
        let mut bsp = bsp::Bsp::default();
        let model = bsp::Model { mins: glam::Vec3::new(-4., -64., -70.),
            maxs: glam::Vec3::new(4., 64., 70.), origin: glam::Vec3::ZERO,
            head_node: 0, first_face: 0, num_faces: 1 };
        bsp.models = vec![model; 2];
        bsp.models[0].num_faces = 0;
        // The relation parser only needs the checked face range.
        bsp.faces.resize_with(1, || bsp::Face {
            plane: 0, side: 0, first_edge: 0, num_edges: 0,
            texinfo: 0, dispinfo: -1, styles: [0; 4], light_offset: -1,
            lightmap_mins: [0; 2], lightmap_size: [0; 2], num_prims: 0, first_prim_id: 0,
        });
        bsp.entities = kv::parse_entities(r#"
            { "classname" "func_areaportalwindow" "target" "window"
              "PortalVersion" "1" "FadeStartDist" "1000" "FadeDist" "1200" "TranslucencyLimit" "0" }
            { "classname" "func_brush" "targetname" "window" "model" "*1"
              "origin" "-1180 -1224 70" "StartDisabled" "0" "rendermode" "0" }
        "#).unwrap();
        bsp
    }

    fn set(bsp: &mut bsp::Bsp, entity: usize, key: &str, value: &str) {
        bsp.entities[entity].set(key, kv::Value::Str(value.into()));
    }

    #[test]
    fn authored_bounds_and_endpoints_are_camera_dependent_without_mutating_entities() {
        let bsp = fixture();
        let before = bsp.entities.clone();
        let windows = relations(&bsp);
        assert_eq!(windows.len(), 1);
        let window = &windows[0];
        assert_eq!(window.faces, 0..1);
        assert_eq!(window.mins, glam::Vec3::new(-1184., -1288., 0.));
        assert_eq!(window.alpha(glam::Vec3::new(-1040., -1160., -32.)), Some(0.));
        let point = |distance| glam::Vec3::new(window.maxs.x + distance, -1224., 70.);
        assert_eq!(window.alpha(point(1000.)), Some(0.));
        assert_eq!(window.alpha(point(1100.)), Some(0.5));
        assert_eq!(window.alpha(point(1200.)), Some(1.));
        assert_eq!(window.alpha(point(2000.)), Some(1.));
        assert_eq!(window.alpha(glam::Vec3::splat(f32::NAN)), None);
        assert_eq!(window.alpha(glam::Vec3::splat(f32::INFINITY)), None);
        assert_eq!(bsp.entities, before);
        let mut limit_window = window.clone();
        limit_window.limit = 0.25;
        assert_eq!(limit_window.alpha(point(1000.)), Some(0.25));
        assert_eq!(limit_window.alpha(point(1100.)), Some(0.625));
    }

    #[test]
    fn strict_numbers_and_unsupported_entity_states_never_capture_faces() {
        for (entity, key, value) in [
            (0, "FadeStartDist", "1000junk"), (0, "FadeStartDist", "NaN"),
            (0, "FadeStartDist", "-1"), (0, "FadeDist", "inf"), (0, "FadeDist", "1000"),
            (0, "TranslucencyLimit", "-0.1"), (0, "TranslucencyLimit", "1.1"),
            (0, "PortalVersion", "0"), (0, "BackgroundBModel", "foreground"),
            (0, "target", "window*"), (0, "target", "!activator"),
            (1, "classname", "func_breakable"), (1, "StartDisabled", "1"),
            (1, "rendermode", "10"), (1, "renderfx", "1"), (1, "renderamt", "128"),
            (1, "origin", "0 NaN 0"), (1, "origin", "0 0 0 extra"),
            (1, "angles", "0 90 0"), (1, "parentname", "door"),
            (1, "vscripts", "map.nut"), (1, "model", "*0"), (1, "model", "*999"),
        ] {
            let mut bsp = fixture();
            set(&mut bsp, entity, key, value);
            assert!(relations(&bsp).is_empty(), "{entity} {key} {value}");
        }
        for key in ["FadeDist", "FadeStartDist", "TranslucencyLimit"] {
            let mut bsp = fixture();
            bsp.entities[0].entries.retain(|(name, _)| name != key);
            assert!(relations(&bsp).is_empty());
        }
        let mut bsp = fixture();
        bsp.models[1].num_faces = 2;
        assert!(relations(&bsp).is_empty());
        let mut bsp = fixture();
        bsp.models[0].num_faces = 1;
        assert!(relations(&bsp).is_empty());
    }

    #[test]
    fn duplicate_targets_controllers_models_and_keys_are_not_claimed() {
        for entity in [0, 1] {
            let mut bsp = fixture();
            bsp.entities.push(bsp.entities[entity].clone());
            assert!(relations(&bsp).is_empty());
        }
        let mut bsp = fixture();
        let mut alias = bsp.entities[1].clone();
        alias.set("targetname", kv::Value::Str("other".into()));
        alias.set("model", kv::Value::Str("*01".into()));
        bsp.entities.push(alias);
        assert!(relations(&bsp).is_empty());
        let mut bsp = fixture();
        bsp.entities[0].entries.push(("fadedist".into(), kv::Value::Str("1400".into())));
        assert!(relations(&bsp).is_empty());
    }

    #[test]
    fn only_the_named_brush_is_claimed_and_names_compare_without_ascii_case() {
        let mut bsp = fixture();
        set(&mut bsp, 0, "target", "WINDOW");
        bsp.entities.push(kv::parse(r#"classname func_brush targetname other model *2 origin "0 0 0""#).unwrap());
        let windows = relations(&bsp);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].model, 1);
    }

    #[test]
    fn incoming_and_owned_io_is_rejected_without_applying_any_input() {
        for connection in ["window,Kill,,0,-1", "WINDOW\u{1b}Disable\u{1b}\u{1b}0\u{1b}-1",
            "func_brush,Alpha,0,0,-1", "*,Enable,,0,-1", "window,FireUser1,,0,-1"] {
            let mut bsp = fixture();
            let mut source = kv::Block::new();
            source.set("OnTrigger", kv::Value::Str(connection.into()));
            // Both nested connections and the BSP's flattened output form.
            let mut entity = kv::Block::new();
            entity.set("connections", kv::Value::Block(source));
            bsp.entities.push(entity);
            let before = bsp.entities.clone();
            assert!(relations(&bsp).is_empty(), "{connection}");
            assert_eq!(bsp.entities, before);
        }
        for entity in [0, 1] {
            let mut bsp = fixture();
            set(&mut bsp, entity, "OnUser1", "other,Kill,,0,-1");
            assert!(relations(&bsp).is_empty());
        }
        let mut bsp = fixture();
        bsp.entities.push(kv::parse(r#"classname logic_relay OnTrigger "unrelated,Kill,,0,-1""#).unwrap());
        assert_eq!(relations(&bsp).len(), 1);
    }

    #[test]
    fn material_subset_preserves_unrelated_alpha_contracts() {
        let mat = |body| vmt::Material::parse_with(&format!("LightmappedGeneric {{ {body} }}"), &|_| None, 0).unwrap();
        assert!(supported_material(&mat(r#""$basetexture" "test/white""#)));
        for declaration in [r#""$translucent" "1""#, r#""$alphatest" "1""#,
            r#""$additive" "1""#, r#"Proxies { Sine { } }"#, r#""$detail" "test/detail""#,
            r#""$alpha" "0.5""#, r#""$alpha" "NaN""#, r#""$phong" "1""#] {
            assert!(!supported_material(&mat(declaration)), "{declaration}");
        }
    }

    #[test]
    fn frame_updates_retain_invalid_camera_state_and_leave_unrelated_materials_and_pvs_alone() {
        let mut app = App::new();
        let window = relations(&fixture()).pop().unwrap();
        let near = window.origin;
        let far = window.maxs + glam::Vec3::new(2000., 0., 0.);
        let mut images = Assets::<Image>::default();
        let mut textures = TextureCache::new(&mut images);
        let shared = SharedHandles { lightmap: textures.white.clone() };
        let material = world_mesh::material_from_vmt(None, &vfs::Vfs::new(), &mut textures,
            &mut images, &shared, 1., true);
        let mut materials = Assets::<WorldMaterial>::default();
        let unrelated = materials.add(material.clone());
        let handle = materials.add(material);
        let parent = app.world_mut().spawn(Visibility::Hidden).id();
        let entity = app.world_mut().spawn((WindowFade { window, base_alpha: 1., last_alpha: f32::NAN },
            MeshMaterial3d(handle.clone()), Visibility::Inherited, ChildOf(parent))).id();
        app.insert_resource(materials).insert_resource(LocalPlayer::new(1, near, 0.)).add_systems(Update, update);
        app.update();
        assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
        assert_eq!(app.world().resource::<Assets<WorldMaterial>>().get(&handle).unwrap().params.color.w, 0.);
        app.world_mut().resource_mut::<LocalPlayer>().render_eye = glam::Vec3::splat(f32::NAN);
        app.update();
        assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
        app.world_mut().resource_mut::<LocalPlayer>().render_eye = far;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Inherited);
        assert_eq!(*app.world().get::<Visibility>(parent).unwrap(), Visibility::Hidden);
        let materials = app.world().resource::<Assets<WorldMaterial>>();
        assert_eq!(materials.get(&handle).unwrap().params.color.w, 1.);
        assert_eq!(materials.get(&unrelated).unwrap().params.color, Vec4::ONE);
    }
}

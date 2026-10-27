use super::*;
use bevy::math::{Mat4, Vec3, Vec4};
use mechanics_link::{Frame, Pose, Rig, Scene, SceneState, Start};
use skate_data::skate_map::{Collision, Geometry, Material, SkateMap};
use std::sync::Arc;
const METERS: f32 = 0.0254;
pub(super) fn bind_pose(
    root: &std::path::Path,
    skater: &physics::SkaterRuntime,
) -> Result<Vec<mechanics_link::Matrix>, String> {
    let bytes = std::fs::read(root.join("private/skater.glb")).map_err(|e| e.to_string())?;
    let gltf = gltf::Gltf::from_slice(&bytes).map_err(|e| e.to_string())?;
    let blob = gltf.blob.as_deref().ok_or("skater GLB binary missing")?;
    let mut rest = skater
        .render_pose
        .iter()
        .copied()
        .map(|m| crate::animation::native_matrix(m).to_cols_array_2d())
        .collect::<Vec<_>>();
    let basis = Mat4::from_cols(Vec4::X, -Vec4::Z, Vec4::Y, Vec4::W);
    for skin in gltf.skins() {
        let reader = skin.reader(|buffer| (buffer.index() == 0).then_some(blob));
        let matrices = reader
            .read_inverse_bind_matrices()
            .ok_or("skater inverse binds missing")?;
        for (node, inverse) in skin.joints().zip(matrices) {
            if let Some(i) = skater
                .animation
                .evaluator
                .frames
                .bone_names
                .iter()
                .position(|name| Some(name.as_str()) == node.name())
            {
                rest[i] = (Mat4::from_cols_array_2d(&inverse).inverse() * basis.inverse())
                    .to_cols_array_2d();
            }
        }
    }
    Ok(rest)
}
fn native(p: [f32; 3]) -> [f32; 3] {
    [p[0] * METERS, p[2] * METERS, -p[1] * METERS]
}
fn source(p: [f32; 3]) -> [f32; 3] {
    [p[0] / METERS, -p[2] / METERS, p[1] / METERS]
}
fn map(scene: &Scene, start: &Start, compile: bool) -> SkateMap {
    // The upstream collision compiler welds at 1 mm. Remove triangles that
    // collapse at that exact boundary before asking it to compile the host
    // scene; convex sweep bevels can generate submillimetre slivers. Preserve
    // first-vertex identity, winding and the order of every retained surface.
    let mut vertices = std::collections::HashMap::new();
    let mut positions = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let collision = scene
        .triangles
        .iter()
        .filter(|_| compile)
        .filter_map(|t| {
            let ids = t.points.map(native).map(|p| {
                let inverse = 1.0 / f64::from(0.001_f32);
                let key = p.map(|v| (f64::from(v) * inverse).round() as i64);
                *vertices.entry(key).or_insert_with(|| {
                    let i = positions.len();
                    positions.push(p);
                    i
                })
            });
            let points = ids.map(|i| positions[i]);
            let [a, b, c] = points.map(Vec3::from_array);
            if (b - a).cross(c - a).length_squared() < 1e-12 {
                return None;
            }
            let min = ids.iter().enumerate().min_by_key(|(_, v)| **v).unwrap().0;
            let key = [ids[min], ids[(min + 1) % 3], ids[(min + 2) % 3]];
            if !seen.insert(key) {
                return None;
            }
            Some(Collision {
                points,
                surface: 0,
                material: 1,
                native_edges: None,
            })
        })
        .collect::<Vec<_>>();
    if compile {
        eprintln!(
            "CS collision compiled: {} welded triangles",
            collision.len()
        );
    }
    SkateMap {
        version: 15,
        name: "CS map collision".into(),
        spawn: native(start.position),
        heading: (start.yaw + 90.).to_radians(),
        environment: vec![],
        materials: vec![Material {
            name: "host".into(),
            flags: 0,
            friction: 1.,
            restitution: 0.,
            color: [1.; 3],
            roughness: 1.,
            emissive: 0.,
            textures: [0; 5],
            indirect_strength: 1.,
            alpha_mode: 0,
            alpha_cutoff: 0.5,
            audio: 0,
            physics: 0,
            pattern: 0,
            depth_layer: None,
            retail_definition: None,
        }],
        textures: vec![],
        geometry: Geometry {
            vertices: vec![],
            indices: vec![],
            collision,
        },
        rails: scene
            .rails
            .iter()
            .enumerate()
            .map(|(i, points)| skate_data::skate_map::Rail {
                name: format!("CS ledge {i}"),
                closed: false,
                points: points.iter().copied().map(native).collect(),
                native: None,
            })
            .collect(),
        doors: vec![],
        lights: vec![],
        routes: vec![],
        extensions: vec![],
    }
}
pub struct PreparedScene {
    pub(super) world: std::sync::Mutex<Option<skate_core::physics::board_world::BoardWorld>>,
    source: Scene,
    triangles: Vec<skate_core::physics::board_world::WorldTriangle>,
    metadata: skate_core::physics::board_world::query_metadata::QueryMetadata,
    graphs: Arc<graph_runtime::StockGraphs>,
    animation: Arc<skater_animation::AnimationSource>,
    bind_pose: Vec<mechanics_link::Matrix>,
    dynamic: crate::dynamic::Templates,
}
impl PreparedScene {
    pub fn new(runtime: &Runtime, scene: Scene) -> Result<Arc<Self>, String> {
        let map = map(
            &scene,
            &Start {
                checkpoint:None,
                position: [0.; 3],
                velocity: [0.; 3],
                yaw: 0.,
            },
            true,
        );
        let world =
            crate::skate_world::collision_world(&map, runtime.physics.csr_floor_material())?;
        let dynamic = crate::dynamic::Templates::new(&scene)?;
        let triangles = world.triangles().to_vec();
        let metadata = world.query_metadata()?.clone();
        Ok(Arc::new(Self {
            world: std::sync::Mutex::new(Some(world)),
            source: scene,
            triangles,
            metadata,
            graphs: runtime.graphs.clone(),
            animation: runtime.skater.animation.source.clone(),
            bind_pose: runtime.bind_pose.clone(),
            dynamic,
        }))
    }
}
pub(super) fn language(
    root: &std::path::Path,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    match std::fs::read(root.join("private/trick-language.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Default::default()),
        Err(e) => Err(e.to_string()),
    }
}
fn localize(label: &str, language: &std::collections::BTreeMap<String, String>) -> String {
    if let Some(literal) = label.strip_prefix('#') {
        return literal.into();
    }
    label
        .split_whitespace()
        .map(|part| {
            language.get(part).cloned().unwrap_or_else(|| {
                part.strip_prefix("ID_TRICK_")
                    .or_else(|| part.strip_prefix("ID_"))
                    .unwrap_or(part)
                    .split('_')
                    .map(|w| {
                        let mut c = w.chars();
                        c.next()
                            .map(|a| a.to_uppercase().to_string() + &c.as_str().to_lowercase())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}
impl Runtime {
    pub fn load_prepared(
        root: &std::path::Path,
        scene: Arc<PreparedScene>,
        start: &Start,
        state: &SceneState,
    ) -> Result<Self, String> {
        let timing = std::time::Instant::now();
        let report = |label: &str| {
            if std::env::var_os("CSR_SKATE_DIAGNOSTICS").is_some() {
                eprintln!("SKATE_PREPARE {label} {:?}", timing.elapsed());
            }
        };
        let graphs = scene.graphs.clone();
        let map = map(&scene.source, start, false);
        let world = match scene
            .world
            .lock()
            .map_err(|_| "scene cache poisoned")?
            .take()
        {
            Some(world) => world,
            None => skate_core::physics::board_world::BoardWorld::with_query_metadata(
                scene.triangles.clone(),
                scene.metadata.clone(),
            )?,
        };
        report("world");
        let mut physics = physics::GamePhysics::csr_load_prepared(root, &map, world)?;
        report("physics");
        let mut skater = physics::SkaterRuntime::load_for_world(
            root,
            &graphs,
            &physics,
            "easy",
            Some(scene.animation.clone()),
        )?;
        report("actor");
        // Original teleport owner transfers incoming CS momentum to the whole
        // physical assembly, including skeleton/board; no render correction.
        let transform = Mat4::from_rotation_translation(
            bevy::math::Quat::from_rotation_y(map.heading),
            Vec3::from_array(map.spawn),
        );
        skater.travel(transform.to_cols_array_2d(), Some(native(start.velocity)))?;
        let controls = physics::PlayerControls::load(root)?;
        let camera = camera::CameraRuntime::load(root)?;
        let bind_pose = scene.bind_pose.clone();
        scene
            .dynamic
            .apply(state, state, 1. / 128., &mut physics, &skater);
        let language = language(root)?;
        Ok(Self {
            physics,
            marker:crate::session_marker::SessionMarker::load(root,start.checkpoint.clone())?,
            skater,
            controls,
            graphs,
            camera,
            pad: skate_core::input::pad::Pad::new(),
            accumulator: 0.,
            scene: Some(scene),
            scene_state: state.clone(),
            bind_pose,
            language,
        })
    }
    pub fn rig(&self) -> Rig {
        let frames = &self.skater.animation.evaluator.frames;
        let basis = Mat4::from_cols(Vec4::X, Vec4::Z, -Vec4::Y, Vec4::W)
            * Mat4::from_scale(Vec3::splat(METERS.recip()));
        Rig {
            names: frames.bone_names.clone(),
            parents: frames.parents.clone(),
            rest: self.bind_pose.clone(),
            to_source: basis.to_cols_array_2d(),
            bone_basis: Mat4::from_cols(Vec4::X, -Vec4::Z, Vec4::Y, Vec4::W).to_cols_array_2d(),
        }
    }
    pub fn advance_host(&mut self, frame: &Frame) -> Result<Pose, String> {
        if !frame.dt.is_finite() || frame.dt <= 0. || frame.dt > 0.1 {
            return Err("invalid host timestep".into());
        }
        if let Some(scene) = &self.scene {
            scene.dynamic.apply(
                &frame.scene,
                &self.scene_state,
                frame.dt,
                &mut self.physics,
                &self.skater,
            );
        }
        self.scene_state = frame.scene.clone();
        // Marker UI reads every admitted host report, including reports in
        // between native physics periods. A short D-pad press must not vanish.
        self.marker.update(if frame.input.connected{frame.input.buttons}else{0},frame.dt,&self.physics,&mut self.skater);
        self.accumulator += f64::from(frame.dt);
        while self.accumulator + 1e-10 >= self.physics.period().as_secs_f64() {
            self.accumulator -= self.physics.period().as_secs_f64();
            let report = &frame.input;
            let state = skate_core::input::xbox::XboxState {
                buttons: report.buttons,
                triggers: report.triggers,
                left: report.left,
                right: report.right,
            };
            let values = skate_core::input::xbox::convert(&state, 0);
            self.pad
                .update(if report.connected { &values } else { &[] });
            let actions = skate_core::input::gameplay_map::GameplayActions::from_pad(&self.pad);
            self.step(*actions.values(), report.connected)?;
        }
        Ok(self.pose())
    }
    pub fn pose(&self) -> Pose {
        let root =
            crate::animation::native_matrix(self.skater.animated_skeleton.roots.animation_to_world);
        let deck = &self.physics.board.bodies()[skate_core::physics::board::BodyId::Deck.index()];
        let mut pos = deck.rates.position;
        let mut vel = deck.rates.linear_velocity;
        if self.skater.player_state.current().category() == 500 {
            let p = root.w_axis;
            pos = skate_core::math::Vector3::new(p.x, p.y, p.z);
            vel = self.skater.skeleton.bodies()[0].rates.linear_velocity;
        }
        let camera = self.camera.frame.map(|f| {
            let mut m = Mat4::from_cols_array_2d(&[
                [
                    f.basis.columns[0][0],
                    f.basis.columns[0][1],
                    f.basis.columns[0][2],
                    0.,
                ],
                [
                    f.basis.columns[1][0],
                    f.basis.columns[1][1],
                    f.basis.columns[1][2],
                    0.,
                ],
                [
                    f.basis.columns[2][0],
                    f.basis.columns[2][1],
                    f.basis.columns[2][2],
                    0.,
                ],
                f.position,
            ]);
            m.w_axis.w = 1.;
            // Native camera's +Z is its at vector; consumer selects the same
            // presentation sign convention as the standalone camera renderer.
            let to_source = Mat4::from_cols_array_2d(&self.rig().to_source);
            (to_source * m).to_cols_array_2d()
        });
        let hud = self.skater.scoring.hud_input();
        let period = self.physics.period().as_secs_f32();
        Pose {
            checkpoint:self.marker.checkpoint(),
            generation: self.skater.pose_generation,
            phase: (self.accumulator as f32 / period).clamp(0., 1.),
            period,
            discontinuity: self.camera.frame.is_some_and(|f| f.discontinuity),
            position: source([pos.x, pos.y, pos.z]),
            velocity: source([vel.x, vel.y, vel.z]),
            grounded: matches!(self.skater.player_state.current() as u32,100..=105|400..=405|500|502),
            bones: self
                .skater
                .render_pose
                .iter()
                .copied()
                .map(|bone| (root * crate::animation::native_matrix(bone)).to_cols_array_2d())
                .collect(),
            state: self.skater.player_state.current() as u32,
            camera,
            fov: self.camera.frame.map_or(60., |f| f.field_of_view_degrees),
            trick: localize(&hud.trick_name, &self.language),
            score: hud.sequence_score,
        }
    }
}

//! Runtime-authored utility particles, batched by layer rather than one draw per puff.
//! This implements a bounded PCF subset, not the Source particle runtime.
use crate::{
    coords::to_bevy,
    particle_data::{self, Element, Sheet},
    particle_material::{self, ParticleExtension, ParticleMaterial},
};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use std::collections::HashMap;

#[derive(Clone)]
pub struct Layer {
    pub material: Handle<ParticleMaterial>,
    sheets: HashMap<usize, Sheet>,
    max: usize,
    burst: usize,
    rate: f32,
    start: f32,
    duration: f32,
    scale_to_points: bool,
    alpha_distance: Option<[f32; 4]>,
    life: [f32; 2],
    life_fps: f32,
    radius: [f32; 2],
    scale: [f32; 2],
    alpha: [f32; 2],
    fade_in: [f32; 2],
    fade_out: [f32; 2],
    color: [[u8; 4]; 2],
    offset: [Vec3; 2],
    offset_scaled: bool,
    distance: f32,
    ring: f32,
    thickness: f32,
    bias: Vec3,
    box_position: Option<[Vec3; 2]>,
    absolute_source_z: bool,
    velocity: [Vec3; 2],
    speed: [f32; 2],
    gravity: Vec3,
    sequence: [usize; 2],
    animation_rate: f32,
    blend_frames: bool,
    sort_particles: bool,
    fps: bool,
    roll: [f32; 2],
    spin: f32,
    ground: bool,
    ground_normal: bool,
    upright: bool,
}

/// Last possible particle death for these instantaneous layers. The enclosing
/// emitter must outlive each layer's authored delay and lifetime; an unrelated
/// flash timeout would erase dust before its fade-in can develop.
pub fn burst_duration(layers: &[Layer]) -> f32 {
    layers
        .iter()
        .filter(|l| l.burst > 0)
        .map(|l| {
            let life = if l.life_fps > 0.0 {
                l.sheets
                    .iter()
                    .filter(|(id, _)| **id >= l.sequence[0] && **id <= l.sequence[1])
                    .map(|(_, sheet)| sheet.frames.len() as f32 / l.life_fps)
                    .reduce(f32::max)
                    .unwrap_or(1.0)
            } else {
                l.life[0].max(l.life[1])
            };
            l.start + life.max(0.01)
        })
        .fold(0.01, f32::max)
}

fn op<'a>(ops: &[&'a Element], name: &str) -> Option<&'a Element> {
    ops.iter().copied().find(|e| {
        e.text("functionName").eq_ignore_ascii_case(name) || e.name.eq_ignore_ascii_case(name)
    })
}
fn range(e: Option<&Element>, lo: &str, hi: &str, fallback: f32) -> [f32; 2] {
    e.map_or([fallback; 2], |e| {
        [e.number(lo, fallback), e.number(hi, fallback)]
    })
}
fn vector(e: Option<&Element>, name: &str, fallback: Vec3) -> Vec3 {
    e.map_or(fallback, |e| {
        to_bevy(glam::Vec3::from_array(
            e.vector(name, crate::coords::to_source(fallback).to_array()),
        ))
    })
}

/// Only explicitly selected layers opt into this supported geometry subset.
/// Other layers retain their existing interpretation until separately reviewed.
fn position_geometry(initializers: &[&Element], enabled: bool) -> (Option<[Vec3; 2]>, bool) {
    if !enabled {
        return (None, false);
    }
    let sphere = op(initializers, "Position Within Sphere Random");
    let box_position = if sphere.is_none() && op(initializers, "Position Along Ring").is_none() {
        op(initializers, "Position Within Box Random").map(|e| [
            vector(Some(e), "min", Vec3::ZERO),
            vector(Some(e), "max", Vec3::ZERO),
        ])
    } else {
        None // Mixed positional initializer ordering is not implemented.
    };
    let absolute_source_z = sphere.is_some_and(|e|
        e.vector("distance_bias_absolute_value", [0.; 3]) == [0., 0., 1.]);
    (box_position, absolute_source_z)
}

fn initial_position(mut sphere: Vec3, box_position: Option<[Vec3; 2]>, absolute_source_z: bool,
    samples: Vec3) -> Vec3 {
    if let Some([min, max]) = box_position {
        min + (max - min) * samples
    } else {
        if absolute_source_z {
            sphere.y = sphere.y.abs(); // Source Z is Bevy Y; preserve X/Y spread.
        }
        sphere
    }
}

pub fn load(
    fs: &vfs::Vfs,
    path: &str,
    names: &[&str],
    materials: &mut Assets<ParticleMaterial>,
    images: &mut Assets<Image>,
    authored_positions: &[&str],
) -> Vec<Layer> {
    let Some(elements) = fs.read(path).and_then(|data| particle_data::pcf(&data)) else {
        warn!("Cannot read particle definitions {path}");
        return Vec::new();
    };
    let mut layers = Vec::new();
    let mut textures: HashMap<String, (Handle<Image>, HashMap<usize, Sheet>)> = HashMap::new();
    for name in names {
        let Some(e) = elements
            .iter()
            .find(|e| e.kind == "DmeParticleSystemDefinition" && e.name == *name)
        else {
            continue;
        };
        let path = format!(
            "materials/{}",
            e.text("material").replace('\\', "/").to_ascii_lowercase()
        );
        let Some(text) = fs.read(&path).and_then(|b| String::from_utf8(b).ok()) else {
            continue;
        };
        let Some(root) = kv::parse(&text).ok() else {
            continue;
        };
        let Some(body) = root.entries.iter().find_map(|(_, v)| v.as_block()) else {
            continue;
        };
        let value = |name: &str| body.get(name).and_then(kv::Value::as_str).unwrap_or("");
        let texture_path = vmt::vtf_path(value("$basetexture"));
        let (image, sheets) = if let Some(cached) = textures.get(&texture_path) {
            cached.clone()
        } else {
            let Some(bytes) = fs.read(&texture_path) else {
                continue;
            };
            let Some(vtf) = vtf::Vtf::parse(&bytes).ok() else {
                continue;
            };
            let Some(rgba) = vtf.rgba8(0, 0, 0) else {
                continue;
            };
            let sheets = particle_data::sheets(&bytes).unwrap_or_default();
            use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
            let mut image = Image::new(
                Extent3d {
                    width: vtf.width as u32,
                    height: vtf.height as u32,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            );
            image.sampler = bevy::image::ImageSampler::linear();
            let image = images.add(image);
            textures.insert(texture_path, (image.clone(), sheets.clone()));
            (image, sheets)
        };
        let additive =
            value("$addself").parse::<f32>().unwrap_or(0.0) > 0.0 || value("$additive") == "1";
        let additive_over = value("$addoverblend").parse::<f32>().unwrap_or(0.0) > 0.0;
        // Dual-sequence/max-luminance modes need their own authored law;
        // this interpolation implements the explicit single-sequence flag.
        let blend_frames = value("$blendframes").parse::<f32>().unwrap_or(0.0) > 0.0
            && value("$dualsequence").parse::<f32>().unwrap_or(0.0) == 0.0
            && value("$maxlumframeblend1").parse::<f32>().unwrap_or(0.0) == 0.0;
        let material = materials.add(ParticleMaterial {
            base: StandardMaterial {
                base_color_texture: Some(image),
                base_color: Color::linear_rgb(
                    value("$overbrightfactor").parse().unwrap_or(1.0),
                    value("$overbrightfactor").parse().unwrap_or(1.0),
                    value("$overbrightfactor").parse().unwrap_or(1.0),
                ),
                unlit: true,
                alpha_mode: particle_material::alpha_mode(additive_over, additive),
                cull_mode: None,
                ..default()
            },
            extension: ParticleExtension {
                additive_over: u32::from(additive_over),
                blend_frames: u32::from(blend_frames),
            },
        });
        let init = e.refs("initializers", &elements);
        let operators = e.refs("operators", &elements);
        let emitters = e.refs("emitters", &elements);
        let renders = e.refs("renderers", &elements);
        let emit = op(&emitters, "emit_continuously");
        let burst = op(&emitters, "emit_instantaneously");
        let position = op(&init, "Position Within Sphere Random");
        let (box_position, absolute_source_z) = position_geometry(&init, authored_positions.contains(name));
        let ring = op(&init, "Position Along Ring");
        let offset = op(&init, "Position Modify Offset Random");
        let radius = e.number("radius", 1.0);
        let alpha = e.number("alpha", 255.0);
        let color = op(&init, "Color Random");
        let render = renders.first().copied();
        let animation_rate = render.map_or(1.0, |r| r.number("animation rate", 1.0));
        let radius_scale = op(&operators, "Radius Scale");
        let lifetime = op(&init, "Lifetime Random");
        let lifetime_sequence = op(&init, "Lifetime From Sequence");
        let content_id: content::ContentId = format!("csgo_particle_{}", name)
            .parse()
            .expect("static particle id");
        info!("Utility particle {content_id}: {} sequences", sheets.len());
        layers.push(Layer {
            material,
            sheets,
            max: e.number("max_particles", 100.0).clamp(1.0, 400.0) as usize,
            burst: burst
                .map_or(e.number("initial_particles", 0.0), |b| {
                    b.number("num_to_emit", 0.0)
                })
                .clamp(0.0, 400.0) as usize,
            rate: emit
                .map_or(0.0, |e| e.number("emission_rate", 0.0))
                .clamp(0.0, 100.0),
            start: emit
                .or(burst)
                .map_or(0.0, |e| e.number("emission_start_time", 0.0)),
            duration: emit.map_or(0.0, |e| e.number("emission_duration", 0.0)),
            scale_to_points: emit
                .is_some_and(|e| e.number("scale emission to used control points", 0.0) != 0.0),
            alpha_distance: init
                .iter()
                .find(|e| {
                    e.text("functionName")
                        .eq_ignore_ascii_case("Remap Initial Distance to Control Point to Scalar")
                        && e.number("output field", 1.0) == 7.0
                })
                .map(|e| {
                    [
                        e.number("distance minimum", 0.0),
                        e.number("distance maximum", 1.0),
                        e.number("output minimum", 0.0),
                        e.number("output maximum", 1.0),
                    ]
                }),
            life: range(lifetime, "lifetime_min", "lifetime_max", 1.0),
            life_fps: lifetime_sequence.map_or(0.0, |e| e.number("Frames Per Second", 0.0)),
            radius: range(
                op(&init, "Radius Random"),
                "radius_min",
                "radius_max",
                radius,
            ),
            scale: radius_scale.map_or([1.0; 2], |e| {
                [
                    e.number("radius_start_scale", 1.0),
                    e.number("radius_end_scale", 1.0),
                ]
            }),
            alpha: range(op(&init, "Alpha Random"), "alpha_min", "alpha_max", alpha)
                .map(|v| v / 255.0),
            fade_in: range(
                op(&operators, "Alpha Fade In Random"),
                "fade in time min",
                "fade in time max",
                0.0,
            ),
            fade_out: range(
                op(&operators, "Alpha Fade Out Random"),
                "fade out time min",
                "fade out time max",
                0.0,
            ),
            color: color.map_or([[255; 4]; 2], |e| {
                [e.color("color1", [255; 4]), e.color("color2", [255; 4])]
            }),
            offset: [
                vector(offset, "offset min", Vec3::ZERO),
                vector(offset, "offset max", Vec3::ZERO),
            ],
            offset_scaled: offset
                .is_some_and(|e| e.number("offset proportional to radius 0/1", 0.0) != 0.0),
            distance: position.map_or(0.0, |e| e.number("distance_max", 0.0)),
            ring: ring.map_or(0.0, |e| e.number("initial radius", 0.0)),
            thickness: ring.map_or(0.0, |e| e.number("thickness", 0.0)),
            bias: vector(position, "distance_bias", Vec3::ONE),
            box_position,
            absolute_source_z,
            velocity: [
                vector(position, "speed_in_local_coordinate_system_min", Vec3::ZERO),
                vector(position, "speed_in_local_coordinate_system_max", Vec3::ZERO),
            ],
            speed: range(position, "speed_min", "speed_max", 0.0),
            gravity: vector(op(&operators, "Movement Basic"), "gravity", Vec3::ZERO),
            sequence: range(
                op(&init, "Sequence Random"),
                "sequence_min",
                "sequence_max",
                0.0,
            )
            .map(|v| v.max(0.0) as usize),
            animation_rate,
            blend_frames,
            // Preserve the previous sorting behavior when this field is absent;
            // an explicit authored false keeps the particle emission order.
            sort_particles: sorting_enabled(e),
            fps: render.is_some_and(|r| r.number("use animation rate as FPS", 0.0) != 0.0),
            roll: range(
                op(&init, "Rotation Random"),
                "rotation_offset_min",
                "rotation_offset_max",
                0.0,
            )
            .map(f32::to_radians),
            spin: op(&operators, "Rotation Spin Roll")
                .map_or(0.0, |e| e.number("spin_rate_degrees", 0.0))
                .to_radians(),
            ground: render.is_some_and(|r| r.number("orientation_type", 0.0) == 3.0),
            ground_normal: op(&init, "Position Modify Place On Ground")
                .is_some_and(|o| o.number("set normal", 0.0) != 0.0),
            upright: render.is_some_and(|r| r.number("orientation_type", 0.0) == 1.0),
        });
    }
    layers
}

pub struct Batch {
    pub entity: Entity,
    pub mesh: Handle<Mesh>,
    pub layer: usize,
    particles: Vec<Particle>,
    next: usize,
}
struct Particle {
    born: f32,
    life: f32,
    pos: Vec3,
    velocity: Vec3,
    radius: f32,
    alpha: f32,
    color: [f32; 4],
    fade_in: f32,
    fade_out: f32,
    roll: f32,
    yaw: f32,
    normal: Vec3,
    sequence: usize,
}
impl Particle {
    fn center(&self, time: f32, gravity: Vec3) -> Vec3 {
        let age = (time - self.born).max(0.0);
        self.pos + self.velocity * age + gravity * 0.5 * age * age
    }
}

fn sorting_enabled(definition: &Element) -> bool {
    definition.number("sort particles", 1.0) != 0.0
}

fn sort_particles(
    particles: &mut [Particle],
    enabled: bool,
    time: f32,
    gravity: Vec3,
    origin: Vec3,
    eye: Vec3,
) {
    if enabled {
        particles.sort_by(|a, b| {
            (origin + b.center(time, gravity))
                .distance_squared(eye)
                .total_cmp(&(origin + a.center(time, gravity)).distance_squared(eye))
        });
    }
}

pub struct Emitter {
    pub batches: Vec<Batch>,
    pub origin: Vec3,
    pub born: f32,
    pub until: f32,
    pub seed: u32,
    pub fire: bool,
}
impl Emitter {
    /// The same batched layer can render in a dedicated first-person pass.
    pub fn render_layers(
        &self,
        commands: &mut Commands,
        layers: bevy::camera::visibility::RenderLayers,
    ) {
        for batch in &self.batches {
            commands.entity(batch.entity).insert(layers.clone());
        }
    }

    /// Move the complete emitter to an attachment sampled in this frame.
    /// Particle offsets stay local; mouse motion never trails a saved camera.
    pub fn follow(&mut self, origin: Vec3, transforms: &mut Query<&mut Transform>) {
        self.origin = origin;
        for batch in &self.batches {
            if let Ok(mut transform) = transforms.get_mut(batch.entity) {
                transform.translation = origin;
            }
        }
    }

    pub fn spawn(
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        layers: &[Layer],
        pos: glam::Vec3,
        born: f32,
        until: f32,
        seed: u32,
        fire: bool,
    ) -> Self {
        let origin = to_bevy(pos);
        let batches = layers
            .iter()
            .enumerate()
            .map(|(layer, l)| {
                let mesh = meshes.add(empty_mesh(l.max));
                let entity = commands
                    .spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(l.material.clone()),
                        Transform::from_translation(origin),
                        bevy::camera::visibility::NoFrustumCulling,
                        bevy::light::NotShadowCaster,
                        bevy::light::NotShadowReceiver,
                    ))
                    .id();
                Batch {
                    entity,
                    mesh,
                    layer,
                    particles: Vec::new(),
                    next: 0,
                }
            })
            .collect();
        Self {
            batches,
            origin,
            born,
            until,
            seed,
            fire,
        }
    }
    pub fn despawn(self, commands: &mut Commands, meshes: &mut Assets<Mesh>) {
        for b in self.batches {
            commands.entity(b.entity).despawn();
            meshes.remove(b.mesh.id());
        }
    }
    pub fn alive(&self, time: f32) -> bool {
        time < self.until
    }
    pub fn update(
        &mut self,
        time: f32,
        eye: Vec3,
        layers: &[Layer],
        meshes: &mut Assets<Mesh>,
        collision: &collision::CollisionWorld,
    ) {
        for b in &mut self.batches {
            let l = &layers[b.layer];
            b.particles
                .retain(|p| time - p.born < p.life && time < self.until);
            let age = (time - self.born - l.start).max(0.0);
            let rate = l.rate
                * if self.fire && l.scale_to_points {
                    19.0
                } else {
                    1.0
                };
            let emission_end =
                (self.until - self.born - l.start)
                    .max(0.0)
                    .min(if l.duration > 0.0 {
                        l.duration
                    } else {
                        f32::MAX
                    });
            let emitted = if time < self.born + l.start {
                0
            } else {
                l.burst + (age.min(emission_end) * rate).floor() as usize
            };
            // Late network snapshots advance the schedule without a one-frame burst.
            let first = b.next.max(emitted.saturating_sub(l.max));
            for i in first..emitted {
                let born = self.born
                    + l.start
                    + if i < l.burst {
                        0.0
                    } else {
                        (i - l.burst + 1) as f32 / rate.max(0.001)
                    };
                if born >= self.until || time >= self.until || b.particles.len() >= l.max {
                    continue;
                }
                let mut rng = Rng(self
                    .seed
                    .wrapping_mul(31)
                    .wrapping_add(i as u32)
                    .wrapping_add(b.layer as u32 * 7919));
                let sequence = (rng.between(l.sequence[0] as f32, (l.sequence[1] + 1) as f32)
                    as usize)
                    .min(l.sequence[1]);
                let sheet = l.sheets.get(&sequence);
                let life = if l.life_fps > 0.0 {
                    sheet.map_or(1.0, |s| s.frames.len() as f32 / l.life_fps)
                } else {
                    rng.range(l.life)
                }
                .max(0.01);
                if time - born >= life {
                    continue;
                }
                let radius = rng.range(l.radius);
                // Reuse the same three independent draws as the old sphere
                // path, preserving all later tint/fade/rotation draws.
                let angle_sample = rng.unit();
                let distance_sample = rng.unit();
                let angle = angle_sample * std::f32::consts::TAU;
                let distance = if l.ring > 0.0 {
                    l.ring + (distance_sample - 0.5) * l.thickness
                } else {
                    distance_sample.cbrt() * l.distance
                };
                let height_sample = if l.ring > 0.0 {
                    0.5
                } else {
                    rng.unit()
                };
                let y = height_sample * 2.0 - 1.0;
                let direction = Vec3::new(
                    angle.cos() * (1.0 - y * y).sqrt(),
                    y,
                    angle.sin() * (1.0 - y * y).sqrt(),
                );
                let mut pos = initial_position(direction * distance * l.bias.abs(),
                    l.box_position, l.absolute_source_z,
                    Vec3::new(angle_sample, distance_sample, height_sample))
                    + rng.vector(l.offset) * if l.offset_scaled { radius } else { 1.0 };
                let mut normal = Vec3::Y;
                if self.fire {
                    // PCF distributes to inferno control points. Until fire-cell
                    // authority exists, sample only the sim's occupied 130-inch disc.
                    pos += fire_control_point(i % 19);
                    let at = crate::coords::to_source(self.origin + pos);
                    let from = at + glam::Vec3::Z * 32.0;
                    let to = at - glam::Vec3::Z * 64.0;
                    let tr = collision.trace_box(
                        from,
                        to,
                        glam::Vec3::ZERO,
                        glam::Vec3::ZERO,
                        collision::contents::MASK_SOLID,
                    );
                    if !tr.hit() || tr.start_solid || tr.normal.z < 0.65 {
                        continue;
                    }
                    if l.ground_normal {
                        normal = to_bevy(tr.normal).normalize_or(Vec3::Y);
                    }
                    pos = anchor_fire_particle(pos,
                        to_bevy(from.lerp(to, tr.fraction)).y - self.origin.y, l.ground);
                }
                let tint = rng.unit();
                let color = Color::srgb_u8(l.color[0][0], l.color[0][1], l.color[0][2])
                    .mix(
                        &Color::srgb_u8(l.color[1][0], l.color[1][1], l.color[1][2]),
                        tint,
                    )
                    .to_linear()
                    .to_f32_array();
                b.particles.push(Particle {
                    born,
                    life,
                    pos,
                    velocity: rng.vector(l.velocity) + direction * rng.range(l.speed),
                    radius,
                    alpha: rng.range(l.alpha)
                        * l.alpha_distance.map_or(1.0, |r| {
                            let t =
                                ((pos.length() - r[0]) / (r[1] - r[0]).max(0.001)).clamp(0.0, 1.0);
                            r[2] + (r[3] - r[2]) * t
                        }),
                    color,
                    fade_in: rng.range(l.fade_in),
                    fade_out: rng.range(l.fade_out),
                    roll: rng.range(l.roll),
                    yaw: rng.unit() * std::f32::consts::TAU,
                    normal,
                    sequence,
                });
            }
            b.next = emitted;
            let mut positions = Vec::new();
            let mut normals = Vec::new();
            let mut uvs = Vec::new();
            let mut next_frames = Vec::new();
            let mut colors = Vec::new();
            // Only sort layers that permit it, using their drawn positions
            // rather than their initial positions before velocity/gravity.
            sort_particles(
                &mut b.particles,
                l.sort_particles,
                time,
                l.gravity,
                self.origin,
                eye,
            );
            for p in &b.particles {
                let age = (time - p.born).max(0.0);
                let center = p.center(time, l.gravity);
                let external_fade = ((self.until - time) / p.fade_out.max(0.25)).clamp(0.0, 1.0);
                let alpha = p.alpha * fade(age, p.life, p.fade_in, p.fade_out) * external_fade;
                if alpha <= 0.001 {
                    continue;
                }
                let radius = p.radius
                    * (l.scale[0] + (l.scale[1] - l.scale[0]) * (age / p.life).clamp(0.0, 1.0));
                let (uv, next_uv, blend) = l
                    .sheets
                    .get(&p.sequence)
                    .map(|s| {
                        if l.blend_frames {
                            let (current, next, blend) =
                                s.frame_blend(age, p.life, l.animation_rate, l.fps);
                            (s.frames[current], s.frames[next], blend)
                        } else {
                            let uv = s.frames[s.frame(age, p.life, l.animation_rate, l.fps)];
                            (uv, uv, 0.0)
                        }
                    })
                    .unwrap_or(([0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 1.0, 1.0], 0.0));
                let (right, up) = if l.ground {
                    ground_basis(p.normal, p.yaw)
                } else if self.fire {
                    (Vec3::new(p.yaw.cos(), 0.0, p.yaw.sin()), Vec3::Y)
                } else if l.upright {
                    upright_basis(eye - self.origin - center)
                } else {
                    let normal = (eye - self.origin - center).normalize_or(Vec3::Z);
                    let right = Vec3::Y.cross(normal).normalize_or(Vec3::X);
                    (right, normal.cross(right).normalize_or_zero())
                };
                let roll = p.roll + l.spin * age;
                let (sin, cos) = roll.sin_cos();
                let r = (right * cos + up * sin) * radius;
                let u = (up * cos - right * sin) * radius;
                let normal = right.cross(up).normalize_or_zero();
                let mut color = p.color;
                color[3] = alpha;
                for (position, uv, next_uv) in [
                    (center - r - u, [uv[0], uv[3]], [next_uv[0], next_uv[3]]),
                    (center + r - u, [uv[2], uv[3]], [next_uv[2], next_uv[3]]),
                    (center + r + u, [uv[2], uv[1]], [next_uv[2], next_uv[1]]),
                    (center - r + u, [uv[0], uv[1]], [next_uv[0], next_uv[1]]),
                ] {
                    positions.push(position.to_array());
                    normals.push(normal.to_array());
                    uvs.push(uv);
                    next_frames.push([next_uv[0], next_uv[1], blend]);
                    colors.push(color);
                }
            }
            // Keep a nonempty, fixed allocation even for delayed/expired layers.
            // Empty dynamic buffers hit Bevy's slab allocator copy path without
            // an allocated key; changing capacity also needlessly reallocates.
            positions.resize(l.max * 4, [0.0; 3]);
            normals.resize(l.max * 4, [0.0, 1.0, 0.0]);
            uvs.resize(l.max * 4, [0.0; 2]);
            next_frames.resize(l.max * 4, [0.0; 3]);
            colors.resize(l.max * 4, [0.0; 4]);
            if let Some(mut mesh) = meshes.get_mut(&b.mesh) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
                mesh.insert_attribute(particle_material::ATTRIBUTE_NEXT_FRAME, next_frames);
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            }
        }
    }
}

/// Floor sampling supplies the field's local ground height, not a replacement
/// for an upright layer's authored initial height. In particular, radius-scaled
/// offsets can deliberately put a flame center below its supporting surface.
/// Ground cards retain their existing one-unit surface separation.
fn anchor_fire_particle(mut pos: Vec3, floor_height: f32, ground_card: bool) -> Vec3 {
    pos.y = floor_height + if ground_card { 1.0 } else { pos.y };
    pos
}

/// Orientation 3 cards use their ground initializer's sampled surface normal.
/// Project the previous world-X heading onto that plane before applying yaw,
/// preserving the flat-floor layout without turning cards toward the camera.
fn ground_basis(normal: Vec3, yaw: f32) -> (Vec3, Vec3) {
    let normal = normal.normalize_or(Vec3::Y);
    let tangent = (Vec3::X - normal * normal.x).normalize_or(Vec3::Z);
    let bitangent = tangent.cross(normal);
    let (sin, cos) = yaw.sin_cos();
    (tangent * cos + bitangent * sin, bitangent * cos - tangent * sin)
}

/// Authored orientation 1 rotates toward the eye around Source Z (Bevy Y),
/// rather than tilting the entire card toward an elevated/lowered camera.
/// Particle roll still applies within this vertical plane afterward.
fn upright_basis(to_eye: Vec3) -> (Vec3, Vec3) {
    let normal = Vec3::new(to_eye.x, 0.0, to_eye.z).normalize_or(Vec3::Z);
    (Vec3::Y.cross(normal), Vec3::Y)
}

// Temporary render control points cover the authoritative circular field;
// authored PCF emission rates explicitly scale by used control-point count.
// These are not native inferno spread cells and never affect damage.
fn fire_control_point(i: usize) -> Vec3 {
    if i == 0 {
        return Vec3::ZERO;
    }
    let (n, r, index) = if i <= 6 {
        (6, 48.0, i - 1)
    } else {
        (12, 96.0, i - 7)
    };
    let angle = index as f32 / n as f32 * std::f32::consts::TAU;
    Vec3::new(angle.cos() * r, 0.0, angle.sin() * r)
}
fn empty_mesh(capacity: usize) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; capacity * 4]);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![[0.0_f32, 1.0, 0.0]; capacity * 4],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0_f32; 2]; capacity * 4]);
    mesh.insert_attribute(
        particle_material::ATTRIBUTE_NEXT_FRAME,
        vec![[0.0_f32; 3]; capacity * 4],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0_f32; 4]; capacity * 4]);
    mesh.insert_indices(Indices::U32(
        (0..capacity as u32)
            .flat_map(|i| {
                let b = i * 4;
                [b, b + 1, b + 2, b, b + 2, b + 3]
            })
            .collect(),
    ));
    mesh
}
fn fade(age: f32, life: f32, fade_in: f32, fade_out: f32) -> f32 {
    (if fade_in > 0.0 {
        (age / fade_in).clamp(0.0, 1.0)
    } else {
        1.0
    }) * (if fade_out > 0.0 {
        ((life - age) / fade_out).clamp(0.0, 1.0)
    } else {
        1.0
    })
}
struct Rng(u32);
impl Rng {
    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e3779b9);
        let mut x = self.0;
        x = (x ^ (x >> 16)).wrapping_mul(0x21f0aaad);
        x = (x ^ (x >> 15)).wrapping_mul(0x735a2d97);
        ((x ^ (x >> 15)) >> 8) as f32 / 16777216.0
    }
    fn between(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.unit()
    }
    fn range(&mut self, r: [f32; 2]) -> f32 {
        self.between(r[0], r[1])
    }
    fn vector(&mut self, r: [Vec3; 2]) -> Vec3 {
        Vec3::new(
            self.between(r[0].x, r[1].x),
            self.between(r[0].y, r[1].y),
            self.between(r[0].z, r[1].z),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_floor_anchor_keeps_authored_layer_heights_instead_of_radius_lift() {
        // Installed fancy fire: -0.08..-0.04 times its 35..45 radius.
        // Center fire: +5..15. Child flame: +0.1 times its 28..34 radius.
        // These are PCF initializer examples, not native appearance claims.
        for height in [-3.6, -1.4, 5.0, 15.0, 2.8, 3.4] {
            let authored = Vec3::new(17.0, height, -23.0);
            for floor in [-64.0, 0.0, 120.0] {
                let placed = anchor_fire_particle(authored, floor, false);
                assert_eq!(placed.x, authored.x);
                assert_eq!(placed.z, authored.z);
                assert!((placed.y - floor - height).abs() < 1e-5);
                assert_eq!(anchor_fire_particle(authored, floor, true),
                    Vec3::new(authored.x, floor + 1.0, authored.z));
            }
        }
    }

    #[test]
    fn authored_box_positions_preserve_source_bounds_and_axis_conversion() {
        let mut box_init = Element { name: "Position Within Box Random".into(), ..Default::default() };
        box_init.attrs.insert("min".into(), particle_data::Value::Vector([-20., -20., -80.]));
        box_init.attrs.insert("max".into(), particle_data::Value::Vector([20., 20., 100.]));
        let (bounds, absolute) = position_geometry(&[&box_init], true);
        assert!(!absolute);
        for (samples, expected) in [
            (Vec3::ZERO, glam::Vec3::new(-20., -20., -80.)),
            (Vec3::ONE, glam::Vec3::new(20., 20., 100.)),
            (Vec3::splat(0.5), glam::Vec3::new(0., 0., 10.)),
        ] {
            assert_eq!(crate::coords::to_source(initial_position(Vec3::ZERO, bounds, absolute, samples)), expected);
        }
        let mut rng = Rng(91);
        for _ in 0..128 {
            let samples = Vec3::new(rng.unit(), rng.unit(), rng.unit());
            let source = crate::coords::to_source(initial_position(Vec3::ZERO, bounds, absolute, samples));
            assert!((-20. ..=20.).contains(&source.x));
            assert!((-20. ..=20.).contains(&source.y));
            assert!((-80. ..=100.).contains(&source.z));
        }
        assert_eq!(position_geometry(&[&box_init], false), (None, false));
        let sphere = Element { name: "Position Within Sphere Random".into(), ..Default::default() };
        assert_eq!(position_geometry(&[&box_init, &sphere], true).0, None,
            "mixed positional initializer ordering remains unsupported");
    }

    #[test]
    fn explicit_source_z_hemisphere_keeps_other_axes_and_unopted_effects() {
        let mut sphere = Element { name: "Position Within Sphere Random".into(), ..Default::default() };
        sphere.attrs.insert("distance_bias_absolute_value".into(), particle_data::Value::Vector([0., 0., 1.]));
        let (bounds, absolute) = position_geometry(&[&sphere], true);
        assert!(absolute);
        for z in [-13., 0., 17.] {
            let old = to_bevy(glam::Vec3::new(-3., 4., z));
            let corrected = initial_position(old, bounds, absolute, Vec3::ZERO);
            assert_eq!(crate::coords::to_source(corrected), glam::Vec3::new(-3., 4., z.abs()));
            assert_eq!(initial_position(old, None, false, Vec3::ZERO), old);
        }
        assert_eq!(position_geometry(&[&sphere], false), (None, false));
        sphere.attrs.insert("distance_bias_absolute_value".into(), particle_data::Value::Vector([1., 1., 0.]));
        assert!(!position_geometry(&[&sphere], true).1,
            "other flag patterns do not silently change the held wick");
    }

    fn particle(sequence: usize, born: f32, pos: Vec3, velocity: Vec3) -> Particle {
        Particle {
            born,
            life: 20.0,
            pos,
            velocity,
            radius: 1.0,
            alpha: 1.0,
            color: [1.0; 4],
            fade_in: 0.0,
            fade_out: 0.0,
            roll: 0.0,
            yaw: 0.0,
            normal: Vec3::Y,
            sequence,
        }
    }

    #[test]
    fn explicit_unsorted_layers_keep_emission_order_as_the_eye_moves() {
        let mut definition = Element::default();
        assert!(sorting_enabled(&definition)); // Compatibility for absent fields.
        definition
            .attrs
            .insert("sort particles".into(), particle_data::Value::Number(0.0));
        assert!(!sorting_enabled(&definition));
        let mut particles = vec![
            particle(0, 0.0, Vec3::X, Vec3::ZERO),
            particle(1, 0.0, Vec3::X * 100.0, Vec3::ZERO),
        ];
        for eye in [Vec3::ZERO, Vec3::X * 200.0] {
            sort_particles(
                &mut particles,
                sorting_enabled(&definition),
                1.0,
                Vec3::ZERO,
                Vec3::ZERO,
                eye,
            );
            assert_eq!(
                particles.iter().map(|p| p.sequence).collect::<Vec<_>>(),
                [0, 1]
            );
        }
        particles.remove(0); // Expiry preserves survivors, then new emissions append.
        particles.push(particle(2, 1.0, Vec3::X * 300.0, Vec3::ZERO));
        sort_particles(
            &mut particles,
            false,
            2.0,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
        );
        assert_eq!(
            particles.iter().map(|p| p.sequence).collect::<Vec<_>>(),
            [1, 2]
        );
    }

    #[test]
    fn sorted_layers_follow_drawn_centers_after_velocity_and_gravity() {
        let mut particles = vec![
            particle(0, 0.0, Vec3::ZERO, Vec3::X * 20.0),
            particle(1, 1.0, Vec3::X * 10.0, Vec3::ZERO),
        ];
        let gravity = Vec3::Y * 6.0;
        let origin = Vec3::new(100.0, 50.0, -30.0);
        // Birth positions would put 1 first. The older particle has moved
        // farther and risen under the same authored gravity used for drawing.
        assert_eq!(
            particles[0].center(2.0, gravity),
            Vec3::new(40.0, 12.0, 0.0)
        );
        assert_eq!(particles[1].center(2.0, gravity), Vec3::new(10.0, 3.0, 0.0));
        sort_particles(&mut particles, true, 2.0, gravity, origin, origin);
        assert_eq!(
            particles.iter().map(|p| p.sequence).collect::<Vec<_>>(),
            [0, 1]
        );
        sort_particles(
            &mut particles,
            true,
            2.0,
            gravity,
            origin,
            origin + Vec3::X * 100.0,
        );
        assert_eq!(
            particles.iter().map(|p| p.sequence).collect::<Vec<_>>(),
            [1, 0]
        );
    }

    #[test]
    fn ground_cards_follow_slopes_without_changing_flat_floor_layout() {
        for yaw in [0.0_f32, 0.7, 2.4] {
            let (right, up) = ground_basis(Vec3::Y, yaw);
            assert_eq!(right, Vec3::new(yaw.cos(), 0.0, yaw.sin()));
            assert_eq!(up, Vec3::new(-yaw.sin(), 0.0, yaw.cos()));

            let normal = Vec3::new(0.3, 0.9, -0.2).normalize();
            let (right, up) = ground_basis(normal, yaw);
            for axis in [right, up] {
                assert!(axis.dot(normal).abs() < 1e-6);
                assert!((axis.length() - 1.0).abs() < 1e-6);
            }
            assert!(right.dot(up).abs() < 1e-6);
            // All four corners remain on the sampled floor plane.
            for corner in [-right - up, right - up, right + up, -right + up] {
                assert!(corner.dot(normal).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn authored_upright_cards_do_not_tilt_with_eye_height() {
        let level = upright_basis(Vec3::new(3.0, 0.0, 4.0));
        for height in [-200.0, 200.0] {
            let raised = upright_basis(Vec3::new(3.0, height, 4.0));
            assert_eq!(raised, level);
            assert_eq!(raised.1, Vec3::Y);
            assert!(raised.0.y.abs() < 1e-6);
            assert!((raised.0.length() - 1.0).abs() < 1e-6);
        }
        // Directly above the card must remain finite even without a bearing.
        assert_eq!(upright_basis(Vec3::Y), (Vec3::X, Vec3::Y));
        assert_ne!(upright_basis(Vec3::X).0, level.0);
    }

    #[test]
    fn expiry_fades_opacity_without_collapsing_positions() {
        assert_eq!(fade(4.0, 18.0, 0.5, 3.0), 1.0);
        assert!((fade(17.0, 18.0, 0.5, 3.0) - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(fade(18.0, 18.0, 0.5, 3.0), 0.0);
    }
}

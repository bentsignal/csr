//! Runtime-authored utility particles, batched by layer rather than one draw per puff.
//! This implements a bounded PCF subset, not the Source particle runtime.
use crate::{
    coords::to_bevy,
    particle_data::{self, Element, Sheet},
};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use std::collections::HashMap;

#[derive(Clone)]
pub struct Layer {
    pub material: Handle<StandardMaterial>,
    sheets: HashMap<usize, Sheet>,
    max: usize,
    burst: usize,
    rate: f32,
    start: f32,
    duration: f32,
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
    velocity: [Vec3; 2],
    speed: [f32; 2],
    gravity: Vec3,
    sequence: [usize; 2],
    animation_rate: f32,
    fps: bool,
    roll: [f32; 2],
    spin: f32,
    ground: bool,
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

pub fn load(
    fs: &vfs::Vfs,
    path: &str,
    names: &[&str],
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
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
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(image),
            base_color: Color::linear_rgb(
                value("$overbrightfactor").parse().unwrap_or(1.0),
                value("$overbrightfactor").parse().unwrap_or(1.0),
                value("$overbrightfactor").parse().unwrap_or(1.0),
            ),
            unlit: true,
            alpha_mode: if additive {
                AlphaMode::Add
            } else {
                AlphaMode::Blend
            },
            cull_mode: None,
            ..default()
        });
        let init = e.refs("initializers", &elements);
        let operators = e.refs("operators", &elements);
        let emitters = e.refs("emitters", &elements);
        let renders = e.refs("renderers", &elements);
        let emit = op(&emitters, "emit_continuously");
        let burst = op(&emitters, "emit_instantaneously");
        let position = op(&init, "Position Within Sphere Random");
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
        let content_id: content::ContentId = format!("csgo_material_{}", name)
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
    sequence: usize,
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
                let mesh = meshes.add(empty_mesh());
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
                l.burst + (age.min(emission_end) * l.rate).floor() as usize
            };
            // Late network snapshots advance the schedule without a one-frame burst.
            let first = b.next.max(emitted.saturating_sub(l.max));
            for i in first..emitted {
                let born = self.born
                    + l.start
                    + if i < l.burst {
                        0.0
                    } else {
                        (i - l.burst + 1) as f32 / l.rate.max(0.001)
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
                let angle = rng.unit() * std::f32::consts::TAU;
                let distance = if l.ring > 0.0 {
                    l.ring + (rng.unit() - 0.5) * l.thickness
                } else {
                    rng.unit().cbrt() * l.distance
                };
                let y = if l.ring > 0.0 {
                    0.0
                } else {
                    rng.between(-1.0, 1.0)
                };
                let direction = Vec3::new(
                    angle.cos() * (1.0 - y * y).sqrt(),
                    y,
                    angle.sin() * (1.0 - y * y).sqrt(),
                );
                let mut pos = direction * distance * l.bias.abs()
                    + rng.vector(l.offset) * if l.offset_scaled { radius } else { 1.0 };
                if self.fire {
                    // PCF distributes to inferno control points. Until fire-cell
                    // authority exists, sample only the sim's occupied 130-inch disc.
                    let a = rng.unit() * std::f32::consts::TAU;
                    let r = rng.unit().sqrt() * 100.0;
                    pos += Vec3::new(a.cos() * r, 0.0, a.sin() * r);
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
                    pos.y = to_bevy(from.lerp(to, tr.fraction)).y - self.origin.y
                        + if l.ground { 1.0 } else { radius * 0.65 };
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
                    alpha: rng.range(l.alpha),
                    color,
                    fade_in: rng.range(l.fade_in),
                    fade_out: rng.range(l.fade_out),
                    roll: rng.range(l.roll),
                    yaw: rng.unit() * std::f32::consts::TAU,
                    sequence,
                });
            }
            b.next = emitted;
            let mut positions = Vec::new();
            let mut normals = Vec::new();
            let mut uvs = Vec::new();
            let mut colors = Vec::new();
            let mut indices = Vec::new();
            // Transparent quads within one batched mesh still need back-to-front ordering.
            b.particles.sort_by(|a, b| {
                (self.origin + b.pos)
                    .distance_squared(eye)
                    .total_cmp(&(self.origin + a.pos).distance_squared(eye))
            });
            for p in &b.particles {
                let age = (time - p.born).max(0.0);
                let center = p.pos + p.velocity * age + l.gravity * 0.5 * age * age;
                let external_fade = ((self.until - time) / p.fade_out.max(0.25)).clamp(0.0, 1.0);
                let alpha = p.alpha * fade(age, p.life, p.fade_in, p.fade_out) * external_fade;
                if alpha <= 0.001 {
                    continue;
                }
                let radius = p.radius
                    * (l.scale[0] + (l.scale[1] - l.scale[0]) * (age / p.life).clamp(0.0, 1.0));
                let uv = l
                    .sheets
                    .get(&p.sequence)
                    .map(|s| s.frames[s.frame(age, p.life, l.animation_rate, l.fps)])
                    .unwrap_or([0.0, 0.0, 1.0, 1.0]);
                let (right, up) = if l.ground {
                    (
                        Vec3::new(p.yaw.cos(), 0.0, p.yaw.sin()),
                        Vec3::new(-p.yaw.sin(), 0.0, p.yaw.cos()),
                    )
                } else if self.fire {
                    (Vec3::new(p.yaw.cos(), 0.0, p.yaw.sin()), Vec3::Y)
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
                let base = positions.len() as u32;
                let mut color = p.color;
                color[3] = alpha;
                for (position, uv) in [
                    (center - r - u, [uv[0], uv[3]]),
                    (center + r - u, [uv[2], uv[3]]),
                    (center + r + u, [uv[2], uv[1]]),
                    (center - r + u, [uv[0], uv[1]]),
                ] {
                    positions.push(position.to_array());
                    normals.push(normal.to_array());
                    uvs.push(uv);
                    colors.push(color);
                }
                indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            if let Some(mut mesh) = meshes.get_mut(&b.mesh) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
                mesh.insert_indices(Indices::U32(indices));
            }
        }
    }
}
fn empty_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
    mesh.insert_indices(Indices::U32(Vec::new()));
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
    fn expiry_fades_opacity_without_collapsing_positions() {
        assert_eq!(fade(4.0, 18.0, 0.5, 3.0), 1.0);
        assert!((fade(17.0, 18.0, 0.5, 3.0) - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(fade(18.0, 18.0, 0.5, 3.0), 0.0);
    }
}

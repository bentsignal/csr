//! One short, material-specific sprite child from mounted Legacy impact PCFs.
//! This is deliberately not the parent effect runtime: trails, fragments,
//! surface/light tint, drag and native fade operators remain unsupported.
use crate::{coords::to_bevy, particle_data::Element};
use bevy::prelude::*;
use std::{collections::HashMap, sync::Arc};

const PCF: &str = "particles/impact_fx.pcf";
const CHILDREN: [(&str, &str); 3] = [
    ("C", "impact_concrete_child_base"),
    ("M", "impact_metal_child_base"),
    ("Y", "impact_glass_child_base"),
];

#[derive(Default)]
pub(super) struct Catalog {
    profiles: HashMap<String, Arc<Profile>>,
    serial: u32,
}

struct Profile {
    id: content::ContentId,
    material: Handle<StandardMaterial>,
    radius: [f32; 2],
    life: [f32; 2],
    alpha: [f32; 2],
    color: [[u8; 4]; 2],
    offset: [Vec3; 2],
    velocity: [Vec3; 2],
    scale: [f32; 2],
    spin: f32,
}

#[derive(Component)]
pub(crate) struct Puff {
    mesh: Handle<Mesh>,
    profile: Arc<Profile>,
    origin: Vec3,
    rotation: Quat,
    offset: Vec3,
    velocity: Vec3,
    radius: f32,
}

fn op<'a>(ops: &[&'a Element], name: &str) -> Option<&'a Element> {
    ops.iter().copied().find(|e| e.text("functionName").eq_ignore_ascii_case(name))
}
fn range(e: &Element, min: &str, max: &str) -> Option<[f32; 2]> {
    let r = [e.number(min, f32::NAN), e.number(max, f32::NAN)];
    r.iter().all(|v| v.is_finite()).then_some(r)
}
fn vectors(e: &Element, min: &str, max: &str) -> Option<[Vec3; 2]> {
    let r = [min, max].map(|key| to_bevy(glam::Vec3::from_array(e.vector(key, [f32::NAN; 3]))));
    r.iter().all(|v| v.is_finite()).then_some(r)
}

/// One entity remains one particle in the feedback budget. Never admit a parent
/// (24 metal sparks), a continuous emitter or a delayed child through this path.
fn single_puff(e: &Element, all: &[Element]) -> bool {
    let emitters = e.refs("emitters", all);
    let renderers = e.refs("renderers", all);
    emitters.len() == 1
        && emitters[0].text("functionName").eq_ignore_ascii_case("emit_instantaneously")
        && emitters[0].number("num_to_emit", 0.) == 1.
        && emitters[0].number("emission_start_time", 0.) == 0.
        && e.number("initial_particles", 0.) == 0.
        && (1. ..=20.).contains(&e.number("max_particles", 0.))
        && renderers.len() == 1
        && renderers[0].text("functionName").eq_ignore_ascii_case("render_animated_sprites")
        && e.refs("children", all).is_empty()
        && e.refs("forces", all).is_empty()
        && e.refs("constraints", all).is_empty()
}

fn contact_rotation(normal: Vec3) -> Option<Quat> {
    if !normal.is_finite() || normal.length_squared() < 0.5 { return None; }
    // Source Z becomes Bevy Y before aligning local motion to the receiver.
    Some(Quat::from_rotation_arc(Vec3::Y, normal.normalize()))
}

impl Catalog {
    pub(super) fn load(fs: &vfs::Vfs, materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>) -> Self {
        let mut out = Self::default();
        let Some(all) = fs.read(PCF).and_then(|bytes| crate::particle_data::pcf(&bytes)) else {
            return out;
        };
        let mut textures: HashMap<String, Handle<Image>> = HashMap::new();
        // C/M/Y are declared by installed surfaceproperties_cs.txt. Only the
        // named single child is selected, never the full native parent system.
        for (family, name) in CHILDREN {
            let Some(profile) = (|| {
                let e = all.iter().find(|e| e.kind == "DmeParticleSystemDefinition" && e.name == name)?;
                if !single_puff(e, &all) { return None; }
                let init = e.refs("initializers", &all);
                let ops = e.refs("operators", &all);
                let radius = range(op(&init, "Radius Random")?, "radius_min", "radius_max")?;
                let life = range(op(&init, "Lifetime Random")?, "lifetime_min", "lifetime_max")?;
                let alpha = range(op(&init, "Alpha Random")?, "alpha_min", "alpha_max")?;
                // Glass has no offset initializer: keep its emitter origin.
                let offset = if let Some(offset_op) = op(&init, "Position Modify Offset Random") {
                    if offset_op.number("offset proportional to radius 0/1", 0.) != 1.
                        || offset_op.number("offset in local space 0/1", 0.) != 1. { return None; }
                    vectors(offset_op, "offset min", "offset max")?
                } else { [Vec3::ZERO; 2] };
                let position = op(&init, "Position Within Sphere Random")?;
                let velocity = vectors(position, "speed_in_local_coordinate_system_min", "speed_in_local_coordinate_system_max")?;
                let scale = range(op(&ops, "Radius Scale")?, "radius_start_scale", "radius_end_scale")?;
                if !radius.iter().all(|v| (0. ..=16.).contains(v))
                    || !life.iter().all(|v| (0.01..=0.5).contains(v))
                    || !alpha.iter().all(|v| (0. ..=255.).contains(v))
                    || !scale.iter().all(|v| (0. ..=4.).contains(v))
                    || position.number("distance_max", 0.) != 0.
                    || offset.iter().any(|v| v.length() > 1.)
                    || velocity.iter().any(|v| v.length() > 100.)
                    || op(&ops, "Movement Basic")?.vector("gravity", [0.; 3]) != [0.; 3]
                { return None; }
                let color = op(&init, "Color Random")?;
                let base = e.color("color", [255; 4]);
                let mat = vmt::Material::load(e.text("material"), &|p| fs.read_string(p))?;
                if !["unlitgeneric", "spritecard"].contains(&mat.shader.as_str())
                    || !mat.flag("$translucent") || mat.additive() || mat.flag("$depthblend")
                    || mat.flag("$dualsequence") || mat.str("$color").is_some()
                    || mat.alpha() != 1. || mat.flag("$addself") || mat.flag("$addoverblend")
                { return None; }
                let path = vmt::vtf_path(mat.base_texture()?);
                let image = if let Some(image) = textures.get(&path) {
                    image.clone()
                } else {
                    let bytes = fs.read(&path)?;
                    let v = vtf::Vtf::parse(&bytes).ok()?;
                    // These children use a plain 64x64 smoke sprite, not an
                    // atlas or animated VTF. Reject richer replacement content.
                    if v.frames != 1 || v.width > 256 || v.height > 256
                        || !crate::particle_data::sheets(&bytes)?.is_empty() { return None; }
                    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
                    let mut image = Image::new(Extent3d { width: v.width as u32,
                        height: v.height as u32, depth_or_array_layers: 1 },
                        TextureDimension::D2, v.rgba8(0, 0, 0)?, TextureFormat::Rgba8UnormSrgb,
                        bevy::asset::RenderAssetUsages::RENDER_WORLD);
                    image.sampler = bevy::image::ImageSampler::linear();
                    let image = images.add(image);
                    textures.insert(path, image.clone());
                    image
                };
                Some(Profile {
                    id: format!("csgo_particle_{name}").parse().ok()?,
                    material: materials.add(StandardMaterial { base_color_texture: Some(image),
                        unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() }),
                    radius, life, alpha, color: [color.color("color1", base), color.color("color2", base)],
                    offset, velocity, scale,
                    spin: op(&ops, "Rotation Spin Roll")?.number("spin_rate_degrees", 0.).to_radians(),
                })
            })() else { continue; };
            out.profiles.insert(family.into(), Arc::new(profile));
        }
        out
    }

    pub(super) fn prewarm_material(&self) -> Option<Handle<StandardMaterial>> {
        self.profiles.values().next().map(|p| p.material.clone())
    }

    pub(super) fn spawn(&mut self, commands: &mut Commands, meshes: &mut Assets<Mesh>,
        family: &str, pos: glam::Vec3, normal: glam::Vec3, eye: Vec3)
        -> Option<(Entity, f32)> {
        if !pos.is_finite() { return None; }
        let rotation = contact_rotation(to_bevy(normal))?;
        let profile = self.profiles.get(&family.to_ascii_uppercase())?.clone();
        self.serial = self.serial.wrapping_add(1);
        let mut rng = Rng(self.serial);
        let radius = rng.range(profile.radius);
        let life = rng.range(profile.life);
        if std::env::var_os("CSR_WEAPON_TRACE").is_some() {
            info!("impact puff: {} at {pos:?}; radius={radius} life={life}", profile.id);
        }
        let alpha = rng.range(profile.alpha) / 255.;
        let mut color = Color::srgb_u8(profile.color[0][0], profile.color[0][1], profile.color[0][2])
            .mix(&Color::srgb_u8(profile.color[1][0], profile.color[1][1], profile.color[1][2]), rng.unit())
            .to_linear().to_f32_array();
        color[3] = alpha;
        let mut mesh = Mesh::from(Rectangle::new(1., 1.));
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![color; 4]);
        let puff = Puff { mesh: meshes.add(mesh), origin: to_bevy(pos), rotation,
            offset: rng.vector(profile.offset) * radius, velocity: rng.vector(profile.velocity),
            radius, profile };
        let entity = commands.spawn((Mesh3d(puff.mesh.clone()),
            MeshMaterial3d(puff.profile.material.clone()), puff.pose(0., life, eye),
            super::ImpactArtwork(puff.profile.id.clone()),
            bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver, puff)).id();
        Some((entity, life))
    }
}

impl Puff {
    pub(super) fn pose(&self, age: f32, life: f32, eye: Vec3) -> Transform {
        let scale = self.profile.scale[0] + (self.profile.scale[1] - self.profile.scale[0])
            * (age / life).clamp(0., 1.);
        let mut pose = Transform::from_translation(self.origin
            + self.rotation * (self.offset + self.velocity * age))
            .with_scale(Vec3::splat(2. * self.radius * scale));
        pose.look_at(eye, Vec3::Y);
        pose.rotation *= Quat::from_rotation_z(self.profile.spin * age);
        pose
    }
    /// Feedback owns despawns, including round reset and oldest-FX eviction.
    pub(super) fn release_mesh(&self, meshes: &mut Assets<Mesh>) {
        meshes.remove(self.mesh.id());
    }
}

// Presentation-local randomness never advances the simulation's RNG.
struct Rng(u32);
impl Rng {
    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e3779b9);
        let mut x = self.0;
        x = (x ^ (x >> 16)).wrapping_mul(0x21f0aaad);
        x = (x ^ (x >> 15)).wrapping_mul(0x735a2d97);
        ((x ^ (x >> 15)) >> 8) as f32 / 16777216.0
    }
    fn range(&mut self, r: [f32; 2]) -> f32 { r[0] + (r[1] - r[0]) * self.unit() }
    fn vector(&mut self, r: [Vec3; 2]) -> Vec3 {
        Vec3::new(self.range([r[0].x, r[1].x]), self.range([r[0].y, r[1].y]), self.range([r[0].z, r[1].z]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particle_data::Value;

    #[test]
    fn installed_material_children_load_with_distinct_authored_profiles() {
        let Ok((_, fs)) = level::mount_game(None) else { return; };
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();
        let catalog = Catalog::load(&fs, &mut materials, &mut images);
        for (family, child) in CHILDREN {
            let profile = catalog.profiles.get(family)
                .unwrap_or_else(|| panic!("installed impact child was rejected: {child}"));
            assert_eq!(profile.id.to_string(), format!("csgo_particle_{child}"));
            assert!(materials.get(&profile.material).unwrap().base_color_texture.is_some());
        }
        assert_ne!(catalog.profiles["C"].radius, catalog.profiles["M"].radius);
        assert_ne!(catalog.profiles["M"].color, catalog.profiles["Y"].color);
    }

    #[test]
    fn puff_budget_rejects_parent_sparks_continuous_and_delayed_emission() {
        let mut all = vec![Element::default(), Element::default()];
        all[0].attrs.insert("functionname".into(), Value::Text("emit_instantaneously".into()));
        all[0].attrs.insert("num_to_emit".into(), Value::Number(1.));
        all[1].attrs.insert("functionname".into(), Value::Text("render_animated_sprites".into()));
        let mut definition = Element::default();
        for (key, index) in [("emitters", 0), ("renderers", 1)] {
            definition.attrs.insert(key.into(), Value::Array(vec![Value::Ref(index)]));
        }
        definition.attrs.insert("max_particles".into(), Value::Number(20.));
        assert!(single_puff(&definition, &all));
        all[0].attrs.insert("num_to_emit".into(), Value::Number(24.));
        assert!(!single_puff(&definition, &all));
        all[0].attrs.insert("num_to_emit".into(), Value::Number(1.));
        all[0].attrs.insert("functionname".into(), Value::Text("emit_continuously".into()));
        assert!(!single_puff(&definition, &all));
        all[0].attrs.insert("functionname".into(), Value::Text("emit_instantaneously".into()));
        all[0].attrs.insert("emission_start_time".into(), Value::Number(0.1));
        assert!(!single_puff(&definition, &all));
    }

    #[test]
    fn local_puff_moves_out_of_wall_floor_and_ceiling() {
        for normal in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
            let rotation = contact_rotation(normal).unwrap();
            assert!((rotation * to_bevy(glam::Vec3::Z * 50.) - normal * 50.).length() < 1e-4);
        }
        assert!(contact_rotation(Vec3::ZERO).is_none());
        assert!(contact_rotation(Vec3::splat(f32::NAN)).is_none());
    }
}

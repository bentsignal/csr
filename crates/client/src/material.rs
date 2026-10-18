//! The one material used for map geometry and props.
//!
//! Covers Source's `LightmappedGeneric`, `WorldVertexTransition` (two
//! textures blended by vertex alpha, sharpened by `$blendmodulatetexture`),
//! `UnlitGeneric`, and `VertexLitGeneric` props lit by the map's ambient
//! cube plus the sun.
//! `UnlitTwoTexture` multiplies two independently mapped color textures.

use bevy::asset::Asset;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Face,
    RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

pub const SHADER_PATH: &str = "embedded://client/shaders/world.wgsl";

/// These cameras use WorldMaterial's lightmaps, ambient cubes and sun instead
/// of Bevy's clustered point/spot lights. Keep the default path available for
/// bounded renderer comparisons and future renderers that need those lights.
pub(crate) fn camera_clusters() -> bevy::light::cluster::ClusterConfig {
    if std::env::var_os("CSR_TEST_LIGHT_CLUSTERS").is_some() {
        default()
    } else {
        bevy::light::cluster::ClusterConfig::None
    }
}

pub(crate) fn install_lighting(app: &mut App) {
    if std::env::var_os("CSR_TEST_LIGHT_CLUSTERS").is_none() {
        app.add_systems(Startup, disable_unused_gpu_clusters);
    }
}

fn disable_unused_gpu_clusters(mut settings: ResMut<bevy::light::cluster::GlobalClusterSettings>) {
    // Bevy 0.19's GPU path allocates an invalid zero-width dummy texture for
    // a disabled camera. Its documented CPU fallback skips that empty grid.
    // Explicitly lit cameras in other renderers can still use CPU clusters.
    settings.gpu_clustering = None;
    info!("renderer: custom material cameras omit unused light clusters; CPU fallback selected");
}

pub mod flags {
    pub const ALPHA_TEST: u32 = 1 << 0;
    /// Blend `$basetexture2` by vertex alpha.
    pub const BLEND: u32 = 1 << 1;
    pub const LIGHTMAPPED: u32 = 1 << 2;
    pub const UNLIT: u32 = 1 << 3;
    pub const BLEND_MODULATE: u32 = 1 << 4;
    /// Lighting baked into vertex colours (static props).
    pub const VERTEX_LIT: u32 = 1 << 5;
    pub const TRANSLUCENT: u32 = 1 << 6;
    /// Use the base texture's alpha as blend weight (`$basealphaenvmapmask` etc. ignored).
    pub const ADDITIVE: u32 = 1 << 7;
    /// Lit per frame from the material's ambient cube and sun (characters, viewmodel).
    pub const DYNAMIC_LIT: u32 = 1 << 8;
    pub const NORMAL: u32 = 1 << 9;
    pub const NORMAL2: u32 = 1 << 10;
    pub const SSBUMP: u32 = 1 << 11;
    pub const SSBUMP_MATHFIX: u32 = 1 << 12;
    /// Developer-only factor visualization on an owned material variant.
    pub const SHADOW_DIAGNOSTIC: u32 = 1 << 13;
    /// Projected BSP overlay; the following two bits hold its authored order.
    pub const OVERLAY: u32 = 1 << 14;
    pub const OVERLAY_ORDER_SHIFT: u32 = 15;
    /// Use base-texture alpha to mask material/instance diffuse tint.
    pub const TINT_BY_BASE_ALPHA: u32 = 1 << 17;
    /// Ordinary authored detail modulation; opacity and lighting are separate.
    pub const DETAIL: u32 = 1 << 18;
    /// UnlitTwoTexture's `$texture2` multiplies RGBA; never terrain blending.
    pub const UNLIT_MULTIPLY: u32 = 1 << 19;
}

#[derive(Clone, Copy, Debug, ShaderType)]
pub struct WorldParams {
    /// RGB tint, A = alpha.
    pub color: Vec4,
    pub flags: u32,
    pub alpha_ref: f32,
    pub exposure: f32,
    pub lightmap_scale: f32,
    /// Authored secondary color, secondary normal, and blend-mask UV scale/offset.
    pub base2_uv: Vec4,
    pub normal2_uv: Vec4,
    pub modulate_uv: Vec4,
    /// First normal-map UV scale/offset, independent of base and detail UVs.
    pub normal_uv: Vec4,
    /// Dynamic lighting: +X, -X, +Y, -Y, +Z, -Z (Bevy axes).
    pub ambient_cube: [Vec4; 6],
    /// xyz: direction towards the sun (Bevy axes); w: sun visibility 0..1.
    pub sun_dir: Vec4,
    pub sun_color: Vec4,
    /// Viewmodel optic aperture: physical screen centre xy and radius z. Zero disables.
    pub scope_aperture: Vec4,
    /// Phong exponent, boost, exponent-map enabled, base-alpha specular mask.
    pub phong: Vec4,
    pub phong_tint: Vec4,
    pub fresnel: Vec4,
    /// Rim exponent, boost, exponent-alpha mask, half-Lambert.
    pub rim: Vec4,
    /// Cosmetic style, wear, rotation cosine/sine. Zero style disables.
    pub skin: Vec4,
    pub skin_transform: Vec4,
    pub skin_colors: [Vec4; 8],
    pub skin_details:Vec4,
    pub skin_properties:Vec4,
    pub skin_pattern_palette:Vec4,
    /// Detail UV scale xy and authored blend factor z.
    pub detail: Vec4,
    /// Authored self-illumination tint; w: 0 disabled, 1 base alpha, 2 RGB mask.
    pub self_illum: Vec4,
    /// Primary color UV scale/offset for ordinary lit surfaces and UnlitTwoTexture.
    /// Appended to preserve existing lighting and diagnostic-prefix offsets.
    pub base_uv: Vec4,
}

impl Default for WorldParams {
    fn default() -> Self {
        Self {
            color: Vec4::ONE,
            flags: 0,
            alpha_ref: 0.5,
            exposure: 1.0,
            lightmap_scale: 1.0,
            base2_uv: Vec4::new(1.0, 1.0, 0.0, 0.0),
            normal2_uv: Vec4::new(1.0, 1.0, 0.0, 0.0),
            modulate_uv: Vec4::new(1.0, 1.0, 0.0, 0.0),
            normal_uv: Vec4::new(1.0, 1.0, 0.0, 0.0),
            ambient_cube: [Vec4::splat(0.4); 6],
            sun_dir: Vec4::new(0.0, 1.0, 0.0, 0.0),
            sun_color: Vec4::ZERO,
            scope_aperture: Vec4::ZERO,
            phong: Vec4::new(20.0,0.0,0.0,0.0),
            phong_tint: Vec4::ONE,
            fresnel: Vec4::new(0.0,0.5,1.0,0.0),
            rim: Vec4::ZERO,
            skin: Vec4::ZERO,
            skin_transform: Vec4::new(1.0,1.0,0.0,0.0),
            skin_colors: [Vec4::ONE;8],
            skin_details:Vec4::ZERO,
            skin_properties:Vec4::ZERO,
            skin_pattern_palette:Vec4::new(1.,2.,3.,4.),
            detail: Vec4::new(1.0, 1.0, 0.0, 0.0),
            self_illum: Vec4::ZERO,
            base_uv: Vec4::new(1.0, 1.0, 0.0, 0.0),
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldMaterialKey {
    pub cull: bool,
    pub depth_write: bool,
    pub additive: bool,
    pub shadow_diagnostic: bool,
    pub overlay_order: Option<u8>,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(WorldMaterialKey)]
pub struct WorldMaterial {
    #[uniform(0)]
    pub params: WorldParams,
    #[texture(1)]
    #[sampler(2)]
    pub base: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub base2: Handle<Image>,
    #[texture(5, dimension = "2d_array")]
    #[sampler(6)]
    pub lightmap: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    pub modulate: Handle<Image>,
    #[texture(9)]
    #[sampler(10)]
    pub normal: Handle<Image>,
    #[texture(11)]
    #[sampler(12)]
    pub normal2: Handle<Image>,
    /// Nine texels per static prop: ambient cube, sun direction/color, diffuse tint.
    #[texture(13)]
    pub prop_light: Handle<Image>,
    #[texture(14)]
    #[sampler(15)]
    pub phong_map: Handle<Image>,
    #[texture(16)]
    #[sampler(17)]
    pub skin_surface:Handle<Image>,
    #[texture(18)]
    #[sampler(19)]
    pub skin_position:Handle<Image>,
    #[texture(20)]
    #[sampler(21)]
    pub detail: Handle<Image>,
    #[texture(22)]
    #[sampler(23)]
    pub self_illum_mask: Handle<Image>,
    pub alpha_mode: AlphaMode,
    pub cull: bool,
}

impl From<&WorldMaterial> for WorldMaterialKey {
    fn from(m: &WorldMaterial) -> Self {
        Self {
            cull: m.cull,
            depth_write: !matches!(m.alpha_mode, AlphaMode::Blend | AlphaMode::Add | AlphaMode::Premultiplied),
            additive: m.alpha_mode == AlphaMode::Add,
            shadow_diagnostic: crate::shadow_diagnostic::enabled(),
            overlay_order: (m.params.flags & flags::OVERLAY != 0)
                .then_some(((m.params.flags >> flags::OVERLAY_ORDER_SHIFT) & 3) as u8),
        }
    }
}

impl Material for WorldMaterial {
    fn vertex_shader() -> ShaderRef {
        // Specialization restores Bevy's own vertex shader for every mesh
        // without the complete static-prop directional attribute layout.
        SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    fn depth_bias(&self) -> f32 {
        // Retain BSP order as Bevy's transparent sort bias. General
        // intersecting translucent-surface sorting remains a renderer gap.
        if self.params.flags & flags::OVERLAY != 0 {
            1.0 + ((self.params.flags >> flags::OVERLAY_ORDER_SHIFT) & 3) as f32
        } else { 0.0 }
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        crate::shadow_diagnostic::enabled()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        if crate::shadow_diagnostic::enabled() {
            crate::shadow_diagnostic::SHADOW_SHADER.into()
        } else {
            ShaderRef::Default
        }
    }

    fn specialize(
        pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        specialize_prop_vertex(descriptor, layout, &pipeline.mesh_pipeline.shader)?;
        specialize_additive(descriptor, key.bind_group_data.additive);
        descriptor.primitive.cull_mode = if key.bind_group_data.cull { Some(Face::Back) } else { None };
        if key.bind_group_data.shadow_diagnostic {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("CSR_SHADOW_DIAGNOSTIC".into());
            }
        }
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = Some(key.bind_group_data.depth_write);
            if let Some(order) = key.bind_group_data.overlay_order {
                // Bevy uses reversed Z (GreaterEqual). A positive raster bias
                // keeps coplanar decals visible without moving world geometry.
                ds.depth_write_enabled = Some(false);
                ds.bias.constant = 1 + i32::from(order);
                ds.bias.slope_scale = 1.0;
            }
        }
        Ok(())
    }
}

/// Bevy's Add mode shares its premultiplied pipeline with Premultiplied mode.
/// Its PBR fragment shader distinguishes the two by premultiplying RGB and
/// zeroing output alpha. WorldMaterial emits straight RGBA instead, so use
/// SRC_ALPHA:ONE explicitly. Black and zero-opacity texels then preserve the
/// receiver, including its alpha, rather than darkening it. Keep this forward
/// only; diagnostic depth targets do not carry fragment color.
fn specialize_additive(descriptor: &mut RenderPipelineDescriptor, additive: bool) {
    if !additive || descriptor.vertex.shader_defs.contains(&"PREPASS_PIPELINE".into()) {
        return;
    }
    if let Some(target) = descriptor.fragment.as_mut()
        .and_then(|fragment| fragment.targets.first_mut())
        .and_then(Option::as_mut)
    {
        target.blend = Some(BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::Zero,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
        });
    }
}

/// Forward-only specialization: Bevy also invokes Material::specialize for its
/// depth/shadow prepass, whose vertex IO must retain the prepass contract.
fn specialize_prop_vertex(
    descriptor: &mut RenderPipelineDescriptor,
    layout: &MeshVertexBufferLayoutRef,
    default_mesh_shader: &Handle<Shader>,
) -> Result<(), SpecializedMeshPipelineError> {
    if descriptor.vertex.shader_defs.contains(&"PREPASS_PIPELINE".into()) {
        return Ok(());
    }
    let attributes = crate::props::irradiance::ATTRIBUTES;
    if layout.0.contains(attributes[0]) {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            Mesh::ATTRIBUTE_UV_1.at_shader_location(3),
            Mesh::ATTRIBUTE_TANGENT.at_shader_location(4),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(5),
            attributes[0].at_shader_location(8),
            attributes[1].at_shader_location(9),
            attributes[2].at_shader_location(10),
        ])?];
        descriptor.vertex.shader_defs.push("CSR_SPATIAL_PROP".into());
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.push("CSR_SPATIAL_PROP".into());
        }
    } else {
        // Preserve skinning, morphs and all other default mesh behavior.
        descriptor.vertex.shader = default_mesh_shader.clone();
    }
    Ok(())
}

#[cfg(test)]
mod prop_vertex_tests {
    use super::*;
    use bevy::{asset::RenderAssetUsages, mesh::{MeshVertexBufferLayouts, PrimitiveTopology},
        render::render_resource::FragmentState};

    #[test]
    fn additive_straight_rgba_preserves_receiver_and_scales_source_coverage() {
        use bevy::render::render_resource::{ColorTargetState, ColorWrites, TextureFormat};
        let mut descriptor = RenderPipelineDescriptor {
            fragment: Some(FragmentState {
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::Rgba16Float,
                    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        };
        let before = descriptor.clone();
        specialize_additive(&mut descriptor, false);
        assert_eq!(descriptor, before); // Blend/Premultiplied/opaque untouched.
        descriptor.vertex.shader_defs.push("PREPASS_PIPELINE".into());
        let depth = descriptor.clone();
        specialize_additive(&mut descriptor, true);
        assert_eq!(descriptor, depth);
        descriptor.vertex.shader_defs.clear();
        specialize_additive(&mut descriptor, true);
        let blend = descriptor.fragment.as_ref().unwrap().targets[0].as_ref().unwrap().blend.unwrap();

        // Evaluate the actual specialized fixed-function factors, not a second
        // implementation of a proposed shader. Include transparent bright RGB,
        // opaque black, and fractional texture * material opacity.
        let factor = |f: BlendFactor, alpha: f32| match f {
            BlendFactor::Zero => 0., BlendFactor::One => 1.,
            BlendFactor::SrcAlpha => alpha, BlendFactor::OneMinusSrcAlpha => 1. - alpha,
            _ => panic!("unexpected blend factor: {f:?}"),
        };
        let receiver = Vec4::new(0.2, 0.4, 0.6, 0.75);
        for (source, expected) in [
            (Vec4::new(0., 0., 0., 1.), receiver),
            (Vec4::new(1., 1., 1., 0.), receiver),
            (Vec4::new(0.8, 0.4, 0.2, 0.5 * 0.25), Vec4::new(0.3, 0.45, 0.625, 0.75)),
        ] {
            assert_eq!(blend.color.operation, BlendOperation::Add);
            assert_eq!(blend.alpha.operation, BlendOperation::Add);
            let rgb = source.truncate() * factor(blend.color.src_factor, source.w)
                + receiver.truncate() * factor(blend.color.dst_factor, source.w);
            let alpha = source.w * factor(blend.alpha.src_factor, source.w)
                + receiver.w * factor(blend.alpha.dst_factor, source.w);
            assert!((rgb.extend(alpha) - expected).abs().max_element() < 1e-6);
        }
    }

    fn layout(packed_attributes: usize) -> MeshVertexBufferLayoutRef {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::MAIN_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0f32; 3]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0f32, 0., 1.]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0f32; 2]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0f32; 2]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1f32, 0., 0., 1.]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1f32; 4]; 3]);
        for attribute in crate::props::irradiance::ATTRIBUTES.into_iter().take(packed_attributes) {
            mesh.insert_attribute(attribute, vec![[0u32; 4]; 3]);
        }
        mesh.get_mesh_vertex_buffer_layout(&mut MeshVertexBufferLayouts::default())
    }

    #[test]
    fn forward_props_require_complete_coefficients_and_stay_within_vertex_limits() {
        let mut descriptor = RenderPipelineDescriptor {
            fragment: Some(FragmentState::default()), ..default()
        };
        specialize_prop_vertex(&mut descriptor, &layout(3), &Handle::default()).unwrap();
        assert!(descriptor.vertex.shader_defs.contains(&"CSR_SPATIAL_PROP".into()));
        assert!(descriptor.fragment.as_ref().unwrap().shader_defs.contains(&"CSR_SPATIAL_PROP".into()));
        let buffer = &descriptor.vertex.buffers[0];
        assert_eq!(buffer.array_stride, 120); // existing72 + packed48 bytes
        assert_eq!(buffer.attributes.len(), 9);
        assert!(buffer.attributes.iter().all(|a| a.shader_location < 16));
        // Do not silently fill missing directional faces or truncate vertices.
        assert!(specialize_prop_vertex(&mut descriptor, &layout(2), &Handle::default()).is_err());
    }

    #[test]
    fn ordinary_meshes_keep_bevy_vertex_shader_and_prepass_io_is_untouched() {
        let mut shaders = Assets::<Shader>::default();
        let original = shaders.add(Shader::from_wgsl("", "original"));
        let bevy_default = shaders.add(Shader::from_wgsl("", "default"));
        let mut descriptor = RenderPipelineDescriptor::default();
        descriptor.vertex.shader = original.clone();
        specialize_prop_vertex(&mut descriptor, &layout(0), &bevy_default).unwrap();
        assert_eq!(descriptor.vertex.shader, bevy_default);
        assert!(!descriptor.vertex.shader_defs.contains(&"CSR_SPATIAL_PROP".into()));
        for count in [0, 3] {
            descriptor.vertex.shader = original.clone();
            descriptor.vertex.shader_defs = vec!["PREPASS_PIPELINE".into()];
            let before = descriptor.clone();
            specialize_prop_vertex(&mut descriptor, &layout(count), &bevy_default).unwrap();
            assert_eq!(descriptor, before);
        }
    }
}

#[cfg(test)]
mod uniform_tests {
    use super::*;
    use bevy::render::render_resource::encase::UniformBuffer;

    #[test]
    fn primary_base_mapping_is_appended_and_both_shader_layouts_match() {
        let params = WorldParams {
            base_uv: Vec4::new(10., 10., -4.5, -4.5),
            ..default()
        };
        let mut buffer = UniformBuffer::new(Vec::new());
        buffer.write(&params).unwrap();
        let bytes = buffer.into_inner();
        assert_eq!(bytes.len(), 560);
        let read_vec4 = |offset: usize| std::array::from_fn::<_, 4, _>(|i| {
            f32::from_le_bytes(bytes[offset + i * 4..offset + i * 4 + 4].try_into().unwrap())
        });
        // Appending retains all existing prefix/lighting/composition offsets.
        assert_eq!(read_vec4(96), params.ambient_cube[0].to_array());
        assert_eq!(read_vec4(528), params.self_illum.to_array());
        assert_eq!(read_vec4(544), params.base_uv.to_array());
        assert_eq!(WorldParams::default().base_uv, Vec4::new(1., 1., 0., 0.));

        let fields = |shader: &str| -> Vec<String> {
            shader.split_once("Params {").unwrap().1.split_once("};").unwrap().0
                .lines().map(|line| line.split_whitespace().collect::<String>())
                .filter(|line| !line.is_empty()).collect()
        };
        let forward = fields(include_str!("shaders/world.wgsl"));
        let shadow = fields(include_str!("shaders/world_shadow.wgsl"));
        assert_eq!(forward, shadow);
        assert_eq!(forward.last().unwrap(), "base_uv:vec4<f32>,");
    }
}

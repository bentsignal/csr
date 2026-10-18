// Developer diagnostic shadow depth. Mirror WorldParams through its appended
// base_uv field so depth alpha uses the same primary mapping as forward color.
// CPU serialization tests protect the layout; lighting is not evaluated here.
#import bevy_pbr::prepass_io::VertexOutput
// Bevy defines this output only when color prepass outputs or unclipped-depth
// emulation are required. Native depth-only shadow passes instead return void.
#ifdef PREPASS_FRAGMENT
#import bevy_pbr::prepass_io::FragmentOutput
#endif

struct ShadowParams {
    color: vec4<f32>,
    flags: u32,
    alpha_ref: f32,
    exposure: f32,
    lightmap_scale: f32,
    base2_uv: vec4<f32>,
    normal2_uv: vec4<f32>,
    modulate_uv: vec4<f32>,
    normal_uv: vec4<f32>,
    ambient_cube: array<vec4<f32>, 6>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    scope_aperture: vec4<f32>,
    phong: vec4<f32>,
    phong_tint: vec4<f32>,
    fresnel: vec4<f32>,
    rim: vec4<f32>,
    skin: vec4<f32>,
    skin_transform: vec4<f32>,
    skin_colors: array<vec4<f32>,8>,
    skin_details:vec4<f32>,
    skin_properties:vec4<f32>,
    skin_pattern_palette:vec4<f32>,
    detail: vec4<f32>,
    self_illum: vec4<f32>,
    base_uv: vec4<f32>,
};
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: ShadowParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var base_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var base_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var base2_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var base2_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var mod_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var mod_samp: sampler;

@fragment
fn fragment(in: VertexOutput)
#ifdef PREPASS_FRAGMENT
    -> FragmentOutput
#endif
{
#ifdef VERTEX_UVS_A
    let uv = in.uv;
#else
    let uv = vec2<f32>(0.0);
#endif
    let base_uv = select(uv * params.base_uv.xy + params.base_uv.zw, uv,
        (params.flags & 256u) != 0u || params.skin.x > 0.5);
    var alpha = textureSample(base_tex, base_samp, base_uv).a;
    if (params.flags & 2u) != 0u {
#ifdef VERTEX_COLORS
        var weight = in.color.a;
#else
        var weight = 1.0;
#endif
        if (params.flags & 16u) != 0u {
            let m = textureSample(mod_tex, mod_samp, uv * params.modulate_uv.xy + params.modulate_uv.zw);
            let lo = clamp(m.g - m.r, 0.0, 1.0);
            let hi = clamp(m.g + m.r, 0.0, 1.0);
            weight = smoothstep(lo, hi + 0.0001, weight);
        }
        alpha = mix(alpha, textureSample(base2_tex, base2_samp, uv * params.base2_uv.xy + params.base2_uv.zw).a, weight);
    }
    alpha *= params.color.a;
    if (params.flags & 1u) != 0u && alpha < params.alpha_ref { discard; }
#ifdef PREPASS_FRAGMENT
    var out: FragmentOutput;
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
    return out;
#endif // PREPASS_FRAGMENT
}

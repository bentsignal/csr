// Developer diagnostic shadow depth. This is the first32-byte prefix of
// WorldParams, not Bevy's StandardMaterial layout. CPU serialization tests
// protect color/flags/alpha_ref offsets; no lighting/composition policy here.
#import bevy_pbr::prepass_io::{VertexOutput, FragmentOutput}

struct ShadowParams {
    color: vec4<f32>,
    flags: u32,
    alpha_ref: f32,
    exposure: f32,
    lightmap_scale: f32,
};
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: ShadowParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var base_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var base_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var base2_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var base2_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var mod_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var mod_samp: sampler;

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
#ifdef VERTEX_UVS_A
    let uv = in.uv;
#else
    let uv = vec2<f32>(0.0);
#endif
    var alpha = textureSample(base_tex, base_samp, uv).a;
    if (params.flags & 2u) != 0u {
#ifdef VERTEX_COLORS
        var weight = in.color.a;
#else
        var weight = 1.0;
#endif
        if (params.flags & 16u) != 0u {
            let m = textureSample(mod_tex, mod_samp, uv);
            let lo = clamp(m.g - m.r, 0.0, 1.0);
            let hi = clamp(m.g + m.r, 0.0, 1.0);
            weight = smoothstep(lo, hi + 0.0001, weight);
        }
        alpha = mix(alpha, textureSample(base2_tex, base2_samp, uv).a, weight);
    }
    alpha *= params.color.a;
    if (params.flags & 1u) != 0u && alpha < params.alpha_ref { discard; }
    var out: FragmentOutput;
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
    return out;
}

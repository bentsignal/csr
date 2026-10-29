// Provider HUD draw: sRGB-decoded texel times multiply plus add, clamped,
// blended straight-alpha over the frame (see mechanics_link::HudDraw).
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct ColorTransform { multiply: vec4<f32>, add: vec4<f32> }
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> transform: ColorTransform;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var atlas_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(atlas, atlas_sampler, in.uv);
    return clamp(texel * transform.multiply + transform.add, vec4(0.0), vec4(1.0));
}

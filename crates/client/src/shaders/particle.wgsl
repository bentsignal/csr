// Unlit StandardMaterial sampling retains the runtime texture, vertex tint and
// opacity. Only explicitly authored ADDOVERBLEND changes the composition.
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> additive_over: u32;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let input = pbr_input_from_standard_material(in, is_front);
    var color = main_pass_post_lighting_processing(input, input.material.base_color);
    if additive_over != 0u {
        // Premultiplied mode uses ONE:INVSRCALPHA. Match the previous Add
        // shader's source contribution, including processing order, but retain
        // alpha so overlapping cards attenuate the destination. Both RGB and
        // alpha reach zero with particle/texture/authority opacity.
        color = vec4<f32>(color.rgb * color.a, color.a);
    }
    var out: FragmentOutput;
    out.color = color;
    return out;
}

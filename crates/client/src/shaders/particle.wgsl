// Blend adjacent sheet frames before opacity/composition. Retain Bevy's unlit
// material sampling and processing; only authored ADDOVER changes composition.
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_functions,
    view_transformations::position_world_to_clip,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::main_pass_post_lighting_processing,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> additive_over: u32;
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var<uniform> blend_frames: u32;

struct ParticleVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(5) color: vec4<f32>,
    @location(8) next_frame: vec3<f32>,
}

struct ParticleOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(5) color: vec4<f32>,
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    @location(6) @interpolate(flat) instance_index: u32,
#endif
#ifdef VISIBILITY_RANGE_DITHER
    @location(7) @interpolate(flat) visibility_range_dither: i32,
#endif
    @location(8) next_frame: vec3<f32>,
}

// Particles are unskinned dynamic cards with position, normal, UV and color.
// The extra attribute transports the next UV and the authored frame phase.
@vertex
fn vertex(in: ParticleVertex) -> ParticleOutput {
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    var out: ParticleOutput;
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local, vec4<f32>(in.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(in.normal, in.instance_index);
    out.uv = in.uv;
    out.color = in.color;
    out.next_frame = in.next_frame;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = in.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        in.instance_index, world_from_local[3]);
#endif
    return out;
}

fn standard_vertex(in: ParticleOutput) -> VertexOutput {
    var out: VertexOutput;
    out.position = in.position;
    out.world_position = in.world_position;
    out.world_normal = in.world_normal;
    out.uv = in.uv;
    out.color = in.color;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = in.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = in.visibility_range_dither;
#endif
    return out;
}

@fragment
fn fragment(in: ParticleOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var sample = standard_vertex(in);
    var input = pbr_input_from_standard_material(sample, is_front);
    if blend_frames != 0u {
        sample.uv = in.next_frame.xy;
        let next = pbr_input_from_standard_material(sample, is_front);
        // One RGBA crossfade, then one fog/opacity/premultiplication pass.
        // Two translucent cards would attenuate the background twice instead.
        input.material.base_color = mix(input.material.base_color, next.material.base_color, in.next_frame.z);
    }
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

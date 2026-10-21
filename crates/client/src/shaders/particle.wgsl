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
@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var<uniform> depth_fade_inverse_distance: f32;
@group(#{MATERIAL_BIND_GROUP}) @binding(103)
var<uniform> soften_card_edges: u32;

// Only the world player view opts into DepthPrepass. The held wick, sky and
// menu views compile without this binding and retain their existing opacity.
#ifdef DEPTH_PREPASS
#import bevy_pbr::{
    prepass_utils::prepass_depth,
    view_transformations::depth_ndc_to_view_z,
}

fn soft_intersection(frag_coord: vec4<f32>, sample_index: u32) -> f32 {
    if depth_fade_inverse_distance <= 0.0 {
        return 1.0;
    }
    let opaque_depth = prepass_depth(frag_coord, sample_index);
    // Reverse-Z zero is clear sky/infinity. Avoid dividing it into view z.
    if opaque_depth <= 0.0 || frag_coord.z <= 0.0 {
        return 1.0;
    }
    let separation = depth_ndc_to_view_z(frag_coord.z)
        - depth_ndc_to_view_z(opaque_depth);
    // Provisional linear view-depth fade, not an exact native Source law.
    return clamp(separation * depth_fade_inverse_distance, 0.0, 1.0);
}
#endif

struct ParticleVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(5) color: vec4<f32>,
    @location(8) next_frame: vec3<f32>,
    @location(9) card_uv: vec2<f32>,
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
    @location(9) card_uv: vec2<f32>,
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
    out.card_uv = in.card_uv;
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
fn fragment(in: ParticleOutput, @builtin(front_facing) is_front: bool,
#ifdef DEPTH_PREPASS
#ifdef MULTISAMPLED
    @builtin(sample_index) sample_index: u32,
#endif
#endif
) -> FragmentOutput {
    var sample = standard_vertex(in);
    var input = pbr_input_from_standard_material(sample, is_front);
    if blend_frames != 0u {
        sample.uv = in.next_frame.xy;
        let next = pbr_input_from_standard_material(sample, is_front);
        // One RGBA crossfade, then one fog/opacity/premultiplication pass.
        // Two translucent cards would attenuate the background twice instead.
        input.material.base_color = mix(input.material.base_color, next.material.base_color, in.next_frame.z);
    }
#ifdef DEPTH_PREPASS
#ifdef MULTISAMPLED
    let depth_fade = soft_intersection(in.position, sample_index);
#else
    let depth_fade = soft_intersection(in.position, 0u);
#endif
    // Fade before Add premultiplies RGB/zeros alpha and before ADDOVER's
    // explicit premultiplication below. Each composition applies fade once.
    input.material.base_color.a *= depth_fade;
#endif
    if soften_card_edges != 0u {
        // Provisional fire-only edge guard for this bounded SpriteCard renderer.
        // Fire needs a broader feather at its authored crop: transparent atlas
        // padding alone still leaves a hard edge where that crop ends. This
        // also softens the lower cropped edge of the upright flame sheets.
        // This is a provisional presentation feather, not a native shader law.
        let edge = min(min(in.card_uv.x, 1.0 - in.card_uv.x), min(in.card_uv.y, 1.0 - in.card_uv.y));
        input.material.base_color.a *= smoothstep(0.0, 0.125, edge);
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

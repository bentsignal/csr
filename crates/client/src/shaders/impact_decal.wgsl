// Bounded plane rejection, not native decal projection or a fade/parallax law.
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> max_plane_distance: f32;

// Only the world player view has the completed opaque-depth copy. Other
// views compile the previous unlit quad path without a sampled-depth binding.
#ifdef DEPTH_PREPASS
#import bevy_pbr::{
    prepass_utils::prepass_depth,
    view_transformations::{frag_coord_to_ndc, position_ndc_to_world},
}

fn has_receiver(in: VertexOutput, sample_index: u32) -> bool {
    let depth = prepass_depth(in.position, sample_index);
    // Reverse-Z clear sky has no receiver. Never reconstruct infinity.
    if depth <= 0.0 || in.position.z <= 0.0 {
        return false;
    }
    let receiver = position_ndc_to_world(frag_coord_to_ndc(vec4(in.position.xy, depth, 1.0)));
    let distance = abs(dot(receiver - in.world_position.xyz, normalize(in.world_normal)));
    // Signed plane distance avoids the view-depth widening at grazing angles.
    // Coplanar disconnected geometry inside this narrow slab is indistinguishable;
    // transparent receivers and exact receiver identity remain outside this path.
    return distance <= max_plane_distance;
}
#endif

#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither;
#endif

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool,
#ifdef DEPTH_PREPASS
#ifdef MULTISAMPLED
    @builtin(sample_index) sample_index: u32,
#endif
#endif
) -> FragmentOutput {
    // Use the existing StandardMaterial unlit sampling/processing, including
    // Multiply premultiplication, with unchanged artwork UVs and linear 2x tint.
    var input = pbr_input_from_standard_material(in, is_front);
    input.material.base_color = alpha_discard(input.material, input.material.base_color);
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
#ifdef DEPTH_PREPASS
#ifdef MULTISAMPLED
    if !has_receiver(in, sample_index) { discard; }
#else
    if !has_receiver(in, 0u) { discard; }
#endif
#endif
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(input, input.material.base_color);
    return out;
}

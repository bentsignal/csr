#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var<uniform> radar_view: vec4<f32>;
@group(1) @binding(1) var overview: texture_2d<f32>;
@group(1) @binding(2) var overview_sampler: sampler;

@fragment
fn fragment(input: UiVertexOutput) -> @location(0) vec4<f32> {
    let delta = input.uv - vec2<f32>(0.5);
    if dot(delta, delta) > 0.25 { discard; }
    let c = cos(radar_view.w);
    let s = sin(radar_view.w);
    // Inverse of the marker's clockwise screen rotation.
    let map_delta = vec2<f32>(c * delta.x + s * delta.y, -s * delta.x + c * delta.y);
    let uv = radar_view.xy + map_delta * radar_view.z;
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.7);
    }
    return textureSampleLevel(overview, overview_sampler, uv, 0.0);
}

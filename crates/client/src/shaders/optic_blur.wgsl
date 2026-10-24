#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct Settings { aperture: vec4<f32>, params: vec4<f32>, }
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var horizontal_blur: texture_2d<f32>;
@group(0) @binding(2) var scene_sampler: sampler;
#ifdef MULTISAMPLED
@group(0) @binding(3) var weapon_depth: texture_depth_multisampled_2d;
#else
@group(0) @binding(3) var weapon_depth: texture_depth_2d;
#endif
@group(0) @binding(4) var<uniform> settings: Settings;

fn depth_at(uv: vec2<f32>) -> f32 {
    let size = vec2<i32>(textureDimensions(weapon_depth));
    let pixel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2<i32>(0), size - vec2<i32>(1));
#ifdef MULTISAMPLED
    // The nearest covered sample drives close focus at silhouette edges.
    var depth = 0.0;
    for (var sample_index = 0u; sample_index < textureNumSamples(weapon_depth); sample_index++) {
        depth = max(depth, textureLoad(weapon_depth, pixel, i32(sample_index)));
    }
    return depth;
#else
    return textureLoad(weapon_depth, pixel, 0);
#endif
}

fn in_lens(uv: vec2<f32>) -> bool {
    let point = uv * vec2<f32>(textureDimensions(scene));
    return settings.aperture.z > 0.0 && distance(point, settings.aperture.xy) < settings.aperture.z;
}

fn sigma_at(uv: vec2<f32>) -> f32 {
    // Evaluate the same kernel outside the foreground too, so filtered
    // coverage extends beyond its silhouette instead of a hard depth cutout.
    return settings.params.z * f32(textureDimensions(scene).y) / 1080.0;
}

fn coverage_at(uv: vec2<f32>) -> f32 {
    if depth_at(uv) <= 0.0 || in_lens(uv) { return 0.0; }
    let distance_to_eye = settings.params.w / depth_at(uv);
    return 1.0 - smoothstep(7.0, 28.0, distance_to_eye);
}

@fragment
fn horizontal(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let sigma = sigma_at(in.uv);
    if sigma < 0.05 {
        let coverage = coverage_at(in.uv);
        if coverage == 0.0 { return vec4<f32>(0.0); }
        return vec4<f32>(textureSampleLevel(scene, scene_sampler, in.uv, 0.0).rgb * coverage, coverage);
    }
    let step = vec2<f32>(max(sigma, 0.01) * 0.25 / f32(textureDimensions(scene).x), 0.0);
    var result = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -12; i <= 12; i++) {
        let offset = f32(i) * 0.25;
        let weight = exp(-0.5 * offset * offset);
        let uv = in.uv + step * f32(i);
        let coverage = coverage_at(uv);
        result += vec4<f32>(textureSampleLevel(scene, scene_sampler, uv, 0.0).rgb * coverage, coverage) * weight;
        total += weight;
    }
    return result / total;
}

@fragment
fn composite(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let original = textureSampleLevel(scene, scene_sampler, in.uv, 0.0);
    let sigma = sigma_at(in.uv);
    // Exact world preservation at sniper aim, through rifle glass and beside
    // the gun. Do not replace reticle blur with a blurred center scene.
    if sigma < 0.05 || in_lens(in.uv) { return original; }
    let step = vec2<f32>(0.0, max(sigma, 0.01) * 0.25 / f32(textureDimensions(scene).y));
    var result = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -12; i <= 12; i++) {
        let offset = f32(i) * 0.25;
        let weight = exp(-0.5 * offset * offset);
        result += textureSampleLevel(horizontal_blur, scene_sampler, in.uv + step * f32(i), 0.0) * weight;
        total += weight;
    }
    result /= total;
    if result.a < 0.0001 { return original; }
    let blurred = result.rgb / result.a;
    // Covered foreground pixels replace their sharp source. Outside pixels
    // receive premultiplied defocused coverage, making the silhouette soft.
    let amount = max(coverage_at(in.uv), result.a);
    return vec4<f32>(mix(original.rgb, blurred, amount), original.a);
}

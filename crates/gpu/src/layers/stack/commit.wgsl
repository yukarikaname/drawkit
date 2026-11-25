// Composite one stroke's coverage into a resident layer tile. The tile is read as a texture and
// written to a fresh texture, because a storage texture cannot be sampled in the same pass.
//
// The layer texture holds straight alpha, so this repeats the core's `over()`:
//   a  = cov + old.a (1 - cov)
//   rgb = (colour cov + old.rgb old.a (1 - cov)) / a

struct Params {
    origin: vec2<u32>,
    size: vec2<u32>,
    canvas: vec2<u32>,
    alpha: f32,
    _pad: f32,
    color: vec4<f32>,
    wet: f32,
    _pad2: f32,
    _pad3: vec2<f32>,
    mode: u32,
    _pad4: vec3<u32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var old_tile: texture_2d<f32>;
@group(0) @binding(2) var<storage, read> coverage: array<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<u32>(u32(at.x), u32(at.y));
    if (pixel.x >= params.size.x || pixel.y >= params.size.y) {
        return vec4<f32>(0.0);
    }
    let global = params.origin + pixel;
    if (global.x >= params.canvas.x || global.y >= params.canvas.y) {
        return vec4<f32>(0.0);
    }
    let cov = clamp(coverage[global.y * params.canvas.x + global.x] * params.alpha, 0.0, 1.0);
    let old = textureLoad(old_tile, vec2<i32>(i32(pixel.x), i32(pixel.y)), 0);
    if (params.mode == 1u) {
        return vec4<f32>(old.rgb, old.a * (1.0 - cov));
    }
    if (params.mode == 2u) {
        // Alpha lock keeps the pixel's coverage and moves only its colour.
        return vec4<f32>(mix(old.rgb, params.color.rgb, cov), old.a);
    }
    let alpha = cov + old.a * (1.0 - cov);
    if (alpha <= 0.0) {
        return vec4<f32>(0.0);
    }
    let ink = mix(params.color.rgb, old.rgb, clamp(params.wet, 0.0, 1.0));
    let rgb = (ink * cov + old.rgb * old.a * (1.0 - cov)) / alpha;
    return vec4<f32>(rgb, alpha);
}

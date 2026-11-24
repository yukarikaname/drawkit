// Present the painting through a swapchain. The whole view is one full-screen triangle; the
// fragment shader decides between the dark surround, the paper, and the painting texture. The
// paper rectangle is in drawable pixels, with the origin at the top-left like the texture.

struct Params {
    paper: vec4<f32>,
    canvas: vec2<f32>,
    exposure: f32,
    padding: f32,
};

@group(0) @binding(0) var painting: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: Params;

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
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let paper = params.paper;
    let point = position.xy;
    if (point.x < paper.x || point.y < paper.y
        || point.x >= paper.x + paper.z || point.y >= paper.y + paper.w) {
        return vec4<f32>(0.106, 0.098, 0.090, 1.0);
    }
    let uv = (point - paper.xy) / paper.zw;
    let size = max(vec2<i32>(params.canvas), vec2<i32>(1));
    let coord = clamp(vec2<i32>(uv * params.canvas), vec2<i32>(0), size - vec2<i32>(1));
    let texel = textureLoad(painting, coord, 0);
    return vec4<f32>(texel.rgb + vec3<f32>(1.0 - texel.a), 1.0);
}

// Extended range: the texture is Rgba16Float and holds premultiplied linear Display P3. Paper is
// still one, so the same `painting + (1 - alpha)` puts the canvas on the sheet, and highlights
// stay above one for the layer's EDR presentation.
@fragment
fn fs_main_hdr(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let paper = params.paper;
    let point = position.xy;
    if (point.x < paper.x || point.y < paper.y
        || point.x >= paper.x + paper.z || point.y >= paper.y + paper.w) {
        return vec4<f32>(0.012, 0.011, 0.010, 1.0);
    }
    let uv = (point - paper.xy) / paper.zw;
    let size = max(vec2<i32>(params.canvas), vec2<i32>(1));
    let coord = clamp(vec2<i32>(uv * params.canvas), vec2<i32>(0), size - vec2<i32>(1));
    let texel = textureLoad(painting, coord, 0);
    let gain = exp2(params.exposure);
    return vec4<f32>(texel.rgb * gain + vec3<f32>(1.0 - texel.a), 1.0);
}

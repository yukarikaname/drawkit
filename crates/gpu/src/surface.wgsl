// Present the painting through a swapchain. The whole view is one full-screen triangle; the
// fragment shader decides between the dark surround, the paper, and the painting texture. The
// paper rectangle is in drawable pixels, with the origin at the top-left like the texture.

struct Params {
    paper: vec4<f32>,
    canvas: vec2<f32>,
    exposure: f32,
    // Clockwise view rotation around the paper centre, in radians.
    angle: f32,
};

// The paper rectangle stays axis-aligned. Rotate the fragment back into the unrotated sheet
// before the bounds test, so the surrounding area follows the rotated paper. Positive angles
// turn the sheet counter-clockwise on screen, matching the shell's CGContext transform.
fn paper_uv(point: vec2<f32>, paper: vec4<f32>, angle: f32) -> vec2<f32> {
    let centre = paper.xy + paper.zw * 0.5;
    let offset = point - centre;
    let c = cos(angle);
    let s = sin(angle);
    let local = vec2<f32>(offset.x * c - offset.y * s, offset.x * s + offset.y * c);
    return local / paper.zw + vec2<f32>(0.5);
}

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
    let uv = paper_uv(position.xy, params.paper, params.angle);
    if (uv.x < 0.0 || uv.y < 0.0 || uv.x >= 1.0 || uv.y >= 1.0) {
        return vec4<f32>(0.106, 0.098, 0.090, 1.0);
    }
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
    let uv = paper_uv(position.xy, params.paper, params.angle);
    if (uv.x < 0.0 || uv.y < 0.0 || uv.x >= 1.0 || uv.y >= 1.0) {
        return vec4<f32>(0.012, 0.011, 0.010, 1.0);
    }
    let size = max(vec2<i32>(params.canvas), vec2<i32>(1));
    let coord = clamp(vec2<i32>(uv * params.canvas), vec2<i32>(0), size - vec2<i32>(1));
    let texel = textureLoad(painting, coord, 0);
    let gain = exp2(params.exposure);
    return vec4<f32>(texel.rgb * gain + vec3<f32>(1.0 - texel.a), 1.0);
}

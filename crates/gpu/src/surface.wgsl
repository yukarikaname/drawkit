// Present the painting through a swapchain. The whole view is one full-screen triangle; the
// fragment shader decides between the dark surround, the paper, and the painting texture. The
// paper rectangle is in drawable pixels, with the origin at the top-left like the texture.

struct Params {
    paper: vec4<f32>,
    canvas: vec2<f32>,
    exposure: f32,
    // Clockwise view rotation around the paper centre, in radians.
    angle: f32,
    // x: 0 linear Display P3, 1 linear extended sRGB, 2 BT.2100 PQ.
    mode: vec4<f32>,
    // x enabled, yzw preview colour.
    preview: vec4<f32>,
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

fn p3_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        1.2249401 * c.r - 0.2249404 * c.g,
        -0.0420569 * c.r + 1.0420571 * c.g,
        -0.0196376 * c.r - 0.0786361 * c.g + 1.0982735 * c.b,
    );
}

fn p3_to_bt2020(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.7538330 * c.r + 0.1985974 * c.g + 0.0475697 * c.b,
        0.0457438 * c.r + 0.9417772 * c.g + 0.0124790 * c.b,
        -0.0012103 * c.r + 0.0176017 * c.g + 0.9836086 * c.b,
    );
}

// SMPTE ST 2084, with paper white at the 203 nit SDR reference.
fn pq_encode(value: f32) -> f32 {
    let nits = max(value, 0.0) * 203.0;
    let m1 = 0.1593017578125;
    let m2 = 78.84375;
    let c1 = 0.8359375;
    let c2 = 18.8515625;
    let c3 = 18.6875;
    let y = pow(nits / 10000.0, m1);
    return pow((c1 + c2 * y) / (1.0 + c3 * y), m2);
}

fn display_encode(c: vec3<f32>) -> vec3<f32> {
    let mode = i32(params.mode.x);
    if (mode == 1) {
        return max(p3_to_srgb(c), vec3<f32>(0.0));
    }
    if (mode == 2) {
        let bt = max(p3_to_bt2020(c), vec3<f32>(0.0));
        return vec3<f32>(pq_encode(bt.r), pq_encode(bt.g), pq_encode(bt.b));
    }
    return c;
}

@group(0) @binding(0) var painting: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> stroke: array<f32>;

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
    var rgb = texel.rgb + vec3<f32>(1.0 - texel.a);
    if (params.preview.x > 0.5) {
        let coverage = clamp(stroke[u32(coord.y) * u32(params.canvas.x) + u32(coord.x)], 0.0, 1.0);
        rgb = mix(rgb, params.preview.yzw, coverage);
    }
    return vec4<f32>(rgb, 1.0);
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
    let linear = texel.rgb * gain + vec3<f32>(1.0 - texel.a);
    return vec4<f32>(display_encode(linear), 1.0);
}

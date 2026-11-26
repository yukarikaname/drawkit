// Encode an extended-range composite for an eight-bit export.
//
// The stack stores premultiplied linear Display P3. An export is tagged with the document's own
// working space, so convert back to it, apply that space's transfer and clip at paper white,
// which is what the CPU export did before this path existed.

struct EncodeParams {
    // Linear Display P3 -> linear working RGB, one row per entry, xyz carries the matrix.
    rows: array<vec4<f32>, 3>,
    // 1 for Adobe RGB's gamma, 0 for the sRGB curve.
    gamma: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
};

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: EncodeParams;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let corner = positions[index];
    var out: VertexOut;
    out.position = vec4<f32>(corner, 0.0, 1.0);
    out.uv = vec2<f32>((corner.x + 1.0) * 0.5, (1.0 - corner.y) * 0.5);
    return out;
}

fn encode(value: f32) -> f32 {
    let v = clamp(value, 0.0, 1.0);
    if (params.gamma != 0u) {
        return pow(v, 1.0 / 2.19921875);
    }
    if (v <= 0.0031308) {
        return v * 12.92;
    }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

@fragment
fn fs_encode(in: VertexOut) -> @location(0) vec4<f32> {
    let sample = textureSample(source, source_sampler, in.uv);
    let alpha = clamp(sample.a, 0.0, 1.0);
    if (alpha <= 0.0001) {
        return vec4<f32>(0.0);
    }
    let straight = sample.rgb / alpha;
    let linear = vec3<f32>(
        dot(params.rows[0].xyz, straight),
        dot(params.rows[1].xyz, straight),
        dot(params.rows[2].xyz, straight),
    );
    let encoded = vec3<f32>(encode(linear.r), encode(linear.g), encode(linear.b));
    return vec4<f32>(encoded * alpha, alpha);
}

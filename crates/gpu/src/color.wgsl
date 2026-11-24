// Colour conversion: premultiply, unpremultiply, and encoded working RGB to linear Display P3.
// One thread per pixel; the matrix and transfer choice arrive in the uniform buffer.

struct Params {
    width: u32,
    height: u32,
    mode: u32,
    gamma: u32,
    row0: vec4<f32>,
    row1: vec4<f32>,
    row2: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> input: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> output: array<vec4<f32>>;

fn decode_srgb(value: f32) -> f32 {
    let v = max(value, 0.0);
    if (v <= 0.04045) {
        return v / 12.92;
    }
    return pow((v + 0.055) / 1.055, 2.4);
}

fn decode(value: f32) -> f32 {
    if (params.gamma == 1u) {
        return pow(max(value, 0.0), 2.1992188);
    }
    return decode_srgb(value);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }
    let index = id.y * params.width + id.x;
    let pixel = input[index];
    var result = pixel;
    if (params.mode == 0u) {
        result = vec4<f32>(pixel.rgb * pixel.a, pixel.a);
    } else if (params.mode == 1u) {
        if (pixel.a > 0.0) {
            result = vec4<f32>(pixel.rgb / pixel.a, pixel.a);
        } else {
            result = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }
    } else {
        let linear = vec3<f32>(decode(pixel.r), decode(pixel.g), decode(pixel.b));
        let p3 = vec3<f32>(
            dot(params.row0.xyz, linear),
            dot(params.row1.xyz, linear),
            dot(params.row2.xyz, linear),
        );
        result = vec4<f32>(max(p3, vec3<f32>(0.0)) * pixel.a, pixel.a);
    }
    output[index] = result;
}

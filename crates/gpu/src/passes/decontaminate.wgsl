// Colour decontamination: solve `pixel = a * foreground + (1 - a) * background` for foreground.
// The alpha is the coverage mask, not the pixel's own alpha; a cut-out's edge is not stored in the
// RGB values.

struct Params {
    background: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> pixels: array<u32>;
@group(0) @binding(2) var<storage, read> alpha: array<u32>;
@group(0) @binding(3) var<storage, read_write> output: array<u32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= arrayLength(&pixels)) {
        return;
    }
    let a = f32(alpha[index]) / 255.0;
    if (a <= 0.001) {
        output[index] = 0u;
        return;
    }
    let packed = pixels[index];
    let rgb = vec3<f32>(
        f32(packed & 0xffu),
        f32((packed >> 8u) & 0xffu),
        f32((packed >> 16u) & 0xffu),
    ) / 255.0;
    let solved = clamp((rgb - (1.0 - a) * params.background.rgb) / a,
                       vec3<f32>(0.0), vec3<f32>(1.0));
    let r = u32(round(solved.r * 255.0));
    let g = u32(round(solved.g * 255.0));
    let b = u32(round(solved.b * 255.0));
    let alpha_byte = u32(round(a * 255.0));
    output[index] = r | (g << 8u) | (b << 16u) | (alpha_byte << 24u);
}

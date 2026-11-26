// Layer thumbnails on the GPU: one thread per output pixel, box-averaging the coverage in
// premultiplied space so transparent pixels do not pull the colour towards black.

struct Params {
    width: u32,
    height: u32,
    out_width: u32,
    out_height: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var layer_texture: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> output: array<vec4<f32>>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.out_width || id.y >= params.out_height) {
        return;
    }
    let left = id.x * params.width / params.out_width;
    let right = max((id.x + 1u) * params.width / params.out_width, left + 1u);
    let top = id.y * params.height / params.out_height;
    let bottom = max((id.y + 1u) * params.height / params.out_height, top + 1u);
    var sum = vec4<f32>(0.0);
    var count = 0.0;
    for (var y = top; y < bottom; y = y + 1u) {
        for (var x = left; x < right; x = x + 1u) {
            let pixel = textureLoad(layer_texture, vec2<i32>(i32(x), i32(y)), 0);
            sum = sum + vec4<f32>(pixel.rgb * pixel.a, pixel.a);
            count = count + 1.0;
        }
    }
    output[id.y * params.out_width + id.x] = sum / max(count, 1.0);
}

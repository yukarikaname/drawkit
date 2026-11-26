// Separable box blur of a coverage mask. Two dispatches (horizontal, vertical) make an O(radius)
// kernel cost O(1) per sample in the shader; the edge is clamped, so a mask does not fade out at
// the canvas border.

struct Params {
    width: u32,
    height: u32,
    radius: i32,
    direction: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> input: array<f32>;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }
    var sum = 0.0;
    var count = 0.0;
    for (var step = -params.radius; step <= params.radius; step = step + 1) {
        var x = i32(id.x);
        var y = i32(id.y);
        if (params.direction == 0u) {
            x = x + step;
        } else {
            y = y + step;
        }
        x = clamp(x, 0, i32(params.width) - 1);
        y = clamp(y, 0, i32(params.height) - 1);
        sum = sum + input[u32(y) * params.width + u32(x)];
        count = count + 1.0;
    }
    output[id.y * params.width + id.x] = sum / count;
}

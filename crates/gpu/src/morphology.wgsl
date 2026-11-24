// Separable morphological grow/erode over a coverage mask.
//
// A square structuring element is separable: the maximum over a (2r+1) square is the maximum over
// a horizontal line, then over a vertical line. Two passes cost 2(2r+1) samples per pixel instead of
// (2r+1)^2, which is what makes radius 64 usable at 4K.
//
// Out-of-canvas samples count as empty for erosion (the same rule the CPU eroder applies at the
// border) and are ignored for growth.

struct Params {
    width: u32,
    height: u32,
    radius: i32,
    grow: u32,
    axis: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> input: array<f32>;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }
    let grow = params.grow == 1u;
    let horizontal = params.axis == 0u;
    var value = select(1.0, 0.0, grow);
    for (var offset = -params.radius; offset <= params.radius; offset = offset + 1) {
        var x = i32(id.x);
        var y = i32(id.y);
        if (horizontal) {
            x = x + offset;
        } else {
            y = y + offset;
        }
        if (x < 0 || y < 0 || x >= i32(params.width) || y >= i32(params.height)) {
            if (!grow) {
                value = 0.0;
            }
            continue;
        }
        let sample = input[u32(y) * params.width + u32(x)];
        if (grow) {
            value = max(value, sample);
        } else {
            value = min(value, sample);
        }
    }
    output[id.y * params.width + id.x] = value;
}

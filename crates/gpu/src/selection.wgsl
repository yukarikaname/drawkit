// Lasso rasterisation: one thread per canvas pixel, even-odd crossings against the polygon.
//
// The control points are sampled on the CPU once and uploaded; the loop over edges is what a
// compute shader does well. The result is a coverage mask that the existing GPU grow and feather
// passes can take.

struct Params {
    width: u32,
    height: u32,
    count: u32,
    padding: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> points: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }
    let point = vec2<f32>(f32(id.x) + 0.5, f32(id.y) + 0.5);
    var inside = false;
    for (var index = 0u; index < params.count; index = index + 1u) {
        let a = points[index];
        let b = points[(index + 1u) % params.count];
        if ((a.y > point.y) != (b.y > point.y)) {
            let x = (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x;
            if (point.x < x) {
                inside = !inside;
            }
        }
    }
    output[id.y * params.width + id.x] = select(0.0, 1.0, inside);
}

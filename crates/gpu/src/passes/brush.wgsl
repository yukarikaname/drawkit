// One brush segment, stamped by the GPU.
//
// The core builds a coverage mask by stamping discs along the segment at a fixed spacing and
// keeping the maximum coverage per pixel, then composites once. This does the same thing for a
// bounding box of the segment: each thread owns one canvas pixel, walks the stamps, and applies the
// single composite. Doing the stamps in parallel instead of blending each one in turn is what keeps
// the result identical to the CPU - overlapping discs must not double their alpha.

struct Params {
    width: u32,
    height: u32,
    min_x: u32,
    min_y: u32,
    mask_w: u32,
    mask_h: u32,
    erase: u32,
    steps: u32,
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    radius: f32,
    hardness: f32,
    pad0: f32,
    pad1: f32,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> layer: array<vec4<f32>>;

fn coverage(distance: f32) -> f32 {
    if (params.radius <= 0.0) {
        return 0.0;
    }
    let t = clamp(1.0 - distance / params.radius, 0.0, 1.0);
    let edge = max(1.0 - params.hardness, 0.04);
    if (t >= edge) {
        return 1.0;
    }
    let u = t / edge;
    return u * u * (3.0 - 2.0 * u);
}

fn stamp(centre: vec2<f32>, pixel: vec2<f32>) -> f32 {
    let distance = length(pixel - centre);
    return coverage(distance);
}

fn compose(top: f32, bottom: f32, top_alpha: f32, bottom_alpha: f32, alpha: f32) -> f32 {
    return (top * top_alpha + bottom * bottom_alpha * (1.0 - top_alpha)) / alpha;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.mask_w || id.y >= params.mask_h) {
        return;
    }
    let x = params.min_x + id.x;
    let y = params.min_y + id.y;
    if (x >= params.width || y >= params.height) {
        return;
    }
    let pixel = vec2<f32>(f32(x), f32(y));
    var amount = 0.0;
    if (params.steps == 0u) {
        amount = stamp(vec2<f32>(params.to_x, params.to_y), pixel);
    } else {
        for (var step = 0u; step <= params.steps; step = step + 1u) {
            let t = f32(step) / f32(params.steps);
            let centre = vec2<f32>(
                params.from_x + (params.to_x - params.from_x) * t,
                params.from_y + (params.to_y - params.from_y) * t,
            );
            amount = max(amount, stamp(centre, pixel));
        }
    }
    if (amount <= 0.0) {
        return;
    }
    let index = y * params.width + x;
    let existing = layer[index];
    if (params.erase == 1u) {
        var alpha = existing.a * (1.0 - clamp(amount, 0.0, 1.0));
        if (alpha <= 1e-4) {
            alpha = 0.0;
        }
        layer[index] = vec4<f32>(existing.rgb, alpha);
    } else {
        let top = vec4<f32>(params.color.rgb, params.color.a * amount);
        let alpha = top.a + existing.a * (1.0 - top.a);
        if (alpha <= 1e-6) {
            layer[index] = vec4<f32>(0.0);
            return;
        }
        layer[index] = vec4<f32>(
            compose(top.r, existing.r, top.a, existing.a, alpha),
            compose(top.g, existing.g, top.a, existing.a, alpha),
            compose(top.b, existing.b, top.a, existing.a, alpha),
            alpha,
        );
    }
}

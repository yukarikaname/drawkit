// Accumulate stroke coverage in a persistent buffer. One thread owns one canvas pixel and walks the
// stamps of one segment, so overlapping stamps within the segment cannot darken.

struct Params {
    width: u32,
    height: u32,
    min_x: u32,
    min_y: u32,
    mask_w: u32,
    mask_h: u32,
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    radius: f32,
    hardness: f32,
    spacing: f32,
    roundness: f32,
    angle: f32,
    phase: f32,
    alpha: f32,
    _pad: f32,
    pressure: f32,
    flow: f32,
    pressure_alpha: f32,
    paper: f32,
    tip: f32,
    _pad2: f32,
    dual: f32,
    dual_scale: f32,
    dual_hardness: f32,
    dual_roundness: f32,
    dual_angle: f32,
    dual_paper: f32,
    dual_tip: f32,
    _pad3: f32,
};

fn hash(x: u32, y: u32, seed: u32) -> f32 {
    var value = x * 0x9E3779B1u ^ y * 0x85EBCA77u ^ seed;
    value ^= value >> 16u;
    value = value * 0x7FEB352Du;
    value ^= value >> 15u;
    return f32(value & 0xFFFFu) / 65535.0;
}

fn paper_value(x: u32, y: u32) -> f32 {
    return hash(x / 3u, y / 3u, 0x51ED2701u) * 0.65
        + hash(x / 11u, y / 11u, 0x9E3779B9u) * 0.35;
}

fn tip_value(offset: vec2<f32>) -> f32 {
    return hash(bitcast<u32>(i32(offset.x * 3.0)), bitcast<u32>(i32(offset.y * 3.0)),
        0xC2B2AE35u);
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> coverage: array<f32>;

// The core stores the tip angle in degrees and measures the major axis as (cos, sin); this
// shader rotates the other way, so negate to land on the same oval.
fn tip_at(
    centre: vec2<f32>,
    pixel: vec2<f32>,
    radius: f32,
    hardness: f32,
    roundness: f32,
    degrees: f32,
    paper: f32,
    tip_weight: f32,
) -> f32 {
    let offset = pixel - centre;
    let angle = -degrees * 0.017453292;
    let c = cos(angle);
    let s = sin(angle);
    let local = vec2<f32>(offset.x * c - offset.y * s, offset.x * s + offset.y * c);
    let distance = length(vec2<f32>(local.x, local.y / max(roundness, 0.05)));
    let t = clamp(1.0 - distance / radius, 0.0, 1.0);
    let edge = max(1.0 - hardness, 0.04);
    var amount = smoothstep(0.0, 1.0, clamp(t / edge, 0.0, 1.0));
    if (paper > 0.0) {
        amount = amount * (1.0 - paper * paper_value(u32(pixel.x), u32(pixel.y)));
    }
    if (tip_weight > 0.0) {
        amount = amount * (1.0 - tip_weight * tip_value(offset));
    }
    return amount;
}

fn stamp(centre: vec2<f32>, pixel: vec2<f32>) -> f32 {
    return tip_at(centre, pixel, params.radius, params.hardness, params.roundness,
        params.angle, params.paper, params.tip);
}

fn dual_stamp(centre: vec2<f32>, pixel: vec2<f32>, direction: vec2<f32>) -> f32 {
    if (params.dual <= 0.0) {
        return 0.0;
    }
    let perpendicular = vec2<f32>(-direction.y, direction.x);
    let offset = params.radius * params.dual_scale;
    let secondary = centre + perpendicular * offset;
    return tip_at(secondary, pixel, params.radius * params.dual_scale,
        params.dual_hardness, params.dual_roundness, params.dual_angle,
        params.dual_paper, params.dual_tip) * params.dual;
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
    let start = vec2<f32>(params.from_x, params.from_y);
    let to = vec2<f32>(params.to_x, params.to_y);
    let segment = to - start;
    let distance = length(segment);
    let direction = select(vec2<f32>(0.0, 1.0), segment / distance,
        distance > 0.0001);
    let spacing = max(params.radius * max(params.spacing, 0.02), 0.5);
    var amount = 0.0;
    if (distance <= 0.0001) {
        if (params.phase <= 0.0001) {
            amount = max(stamp(to, pixel), dual_stamp(to, pixel, direction));
        }
    } else {
        var next = clamp(params.phase, 0.0, spacing);
        var travelled = 0.0;
        if (next <= 0.0001) {
            amount = max(amount, stamp(start, pixel));
            amount = max(amount, dual_stamp(start, pixel, direction));
            next = spacing;
        }
        while (travelled + next <= distance + 0.0001) {
            travelled = travelled + next;
            let t = clamp(travelled / distance, 0.0, 1.0);
            let centre = start + segment * t;
            amount = max(amount, stamp(centre, pixel));
            amount = max(amount, dual_stamp(centre, pixel, direction));
            next = spacing;
        }
    }
    let index = y * params.width + x;
    let alpha = params.alpha * params.flow
        * (1.0 - params.pressure_alpha * (1.0 - params.pressure));
    coverage[index] = max(coverage[index], amount * alpha);
}

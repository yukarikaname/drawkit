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
    has_map: u32,
    _pad4: u32,
    _pad5: vec2<u32>,
    jitter: f32,
    grain: f32,
    dual_jitter: f32,
    dual_grain: f32,
    seed_low: u32,
    seed_high: u32,
    _pad6: vec2<u32>,
};

fn mix32(value: u32) -> u32 {
    var mixed = value;
    mixed ^= mixed >> 16u;
    mixed = mixed * 0x7FEB352Du;
    mixed ^= mixed >> 15u;
    return mixed;
}

fn noise(x: u32, y: u32, salt: u32) -> f32 {
    let value = mix32((x ^ params.seed_low) * 0x85EBCA77u
        ^ (y ^ params.seed_high) * 0x9E3779B1u ^ salt);
    return f32(value & 0xFFFFu) / 65535.0;
}

// The stroke seed mixed with the segment endpoints, so successive segments do not repeat the same
// jitter stream. `brush::shape::segment_hash` does the same on the CPU.
fn segment_hash(first: vec2<f32>, second: vec2<f32>) -> u32 {
    var value = params.seed_low ^ mix32(params.seed_high ^ 0x9E3779B9u);
    value = mix32(value ^ bitcast<u32>(first.x));
    value = mix32(value ^ bitcast<u32>(first.y));
    value = mix32(value ^ bitcast<u32>(second.x));
    return mix32(value ^ bitcast<u32>(second.y));
}

fn stream_value(segment: u32, ordinal: u32, salt: u32) -> f32 {
    let value = mix32(segment ^ ordinal * 0x9E3779B1u ^ salt);
    return f32(value & 0xFFFFu) / 65535.0;
}

fn jittered(
    centre: vec2<f32>,
    amount: f32,
    salt: u32,
    ordinal: u32,
    segment: u32,
) -> vec2<f32> {
    if (amount <= 0.0) {
        return centre;
    }
    return centre + vec2<f32>(
        (stream_value(segment, ordinal, salt) * 2.0 - 1.0) * params.radius * amount,
        (stream_value(segment, ordinal, salt + 1u) * 2.0 - 1.0) * params.radius * amount,
    );
}

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
@group(0) @binding(2) var paper_map: texture_2d<f32>;
@group(0) @binding(3) var paper_sampler: sampler;

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
    grain: f32,
) -> f32 {
    let offset = pixel - centre;
    let angle = -degrees * 0.017453292;
    let c = cos(angle);
    let s = sin(angle);
    let local = vec2<f32>(offset.x * c - offset.y * s, offset.x * s + offset.y * c);
    let distance = length(vec2<f32>(local.x, local.y / clamp(roundness, 0.05, 1.0)));
    let t = clamp(1.0 - distance / radius, 0.0, 1.0);
    let edge = max(1.0 - hardness, 0.04);
    var amount = smoothstep(0.0, 1.0, clamp(t / edge, 0.0, 1.0));
    if (paper > 0.0) {
        let grain = select(paper_value(u32(pixel.x), u32(pixel.y)),
            textureSampleLevel(paper_map, paper_sampler,
                vec2<f32>(pixel.x / f32(params.width), pixel.y / f32(params.height)), 0.0).r,
            params.has_map != 0u);
        amount = amount * (1.0 - paper * grain);
    }
    if (tip_weight > 0.0) {
        let grain_value = select(tip_value(offset),
            textureSampleLevel(paper_map, paper_sampler,
                (offset / radius + vec2<f32>(1.0)) * 0.5, 0.0).r,
            params.has_map != 0u);
        amount = amount * (1.0 - tip_weight * grain_value);
    }
    if (grain > 0.0) {
        amount = amount * (1.0 - grain * noise(u32(pixel.x), u32(pixel.y), 0x51ED2701u));
    }
    return amount;
}

fn stamp(
    centre: vec2<f32>,
    pixel: vec2<f32>,
    ordinal: u32,
    segment: u32,
) -> f32 {
    let jittered_centre = jittered(centre, params.jitter, 0x9E3779B1u, ordinal, segment);
    return tip_at(jittered_centre, pixel, params.radius, params.hardness, params.roundness,
        params.angle, params.paper, params.tip, params.grain);
}

fn dual_stamp(
    centre: vec2<f32>,
    pixel: vec2<f32>,
    direction: vec2<f32>,
    ordinal: u32,
    segment: u32,
) -> f32 {
    if (params.dual <= 0.0) {
        return 0.0;
    }
    let perpendicular = vec2<f32>(-direction.y, direction.x);
    let offset = params.radius * params.dual_scale;
    let primary = jittered(centre, params.jitter, 0x9E3779B1u, ordinal, segment);
    let secondary = jittered(primary + perpendicular * offset, params.dual_jitter,
        0x85EBCA77u, ordinal, segment);
    return tip_at(secondary, pixel, params.radius * params.dual_scale,
        params.dual_hardness, params.dual_roundness, params.dual_angle,
        params.dual_paper, params.dual_tip, params.dual_grain) * params.dual;
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
    let span = to - start;
    let distance = length(span);
    let direction = select(vec2<f32>(0.0, 1.0), span / distance, distance > 0.0001);
    let spacing = max(params.radius * max(params.spacing, 0.02), 0.5);
    let segment = segment_hash(start, to);
    var ordinal = 0u;
    var amount = 0.0;
    if (distance <= 0.0001) {
        if (params.phase <= 0.0001) {
            amount = max(stamp(to, pixel, ordinal, segment),
                dual_stamp(to, pixel, direction, ordinal, segment));
        }
    } else {
        var next = clamp(params.phase, 0.0, spacing);
        var travelled = 0.0;
        if (next <= 0.0001) {
            amount = max(amount, stamp(start, pixel, ordinal, segment));
            amount = max(amount, dual_stamp(start, pixel, direction, ordinal, segment));
            ordinal = ordinal + 1u;
            next = spacing;
        }
        while (travelled + next <= distance + 0.0001) {
            travelled = travelled + next;
            let t = clamp(travelled / distance, 0.0, 1.0);
            let centre = start + span * t;
            amount = max(amount, stamp(centre, pixel, ordinal, segment));
            amount = max(amount, dual_stamp(centre, pixel, direction, ordinal, segment));
            ordinal = ordinal + 1u;
            next = spacing;
        }
    }
    let index = y * params.width + x;
    // The coverage buffer is the stroke's opacity: flow and the pressure term belong here, where
    // the preview and the commit both read them. The commit pass must not apply flow a second
    // time; `StrokePreview::commit_alpha` is the shared multiplier for that pass.
    let alpha = params.alpha * params.flow
        * (1.0 - params.pressure_alpha * (1.0 - params.pressure));
    // The tape replays one command per pointer sample and composites each sample's mask over the
    // pixels the previous ones left. A `max` here would hold a single sample's edge, which is
    // narrower than the replay at a sample join; the commit then disagrees with the tape and an
    // edit refuses the tile. Source-over keeps the same per-segment maximum inside a sample.
    coverage[index] = max(coverage[index], amount * alpha);
}

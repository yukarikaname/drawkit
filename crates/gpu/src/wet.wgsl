// Wet media field. Every pass is gather-only: a cell reads the current pair and writes the next
// pair, never a neighbour. Injection writes every cell as well, so the pair that is not current
// stays complete for the next step.

struct Params {
    width: u32,
    height: u32,
    dt: f32,
    diffusion: f32,
    evaporation: f32,
    absorption: f32,
    edge: f32,
    advection: f32,
    pigment_diffusion: f32,
    dry: f32,
    count: u32,
    colour_r: f32,
    colour_g: f32,
    colour_b: f32,
    colour_a: f32,
};

@group(0) @binding(0) var read0: texture_2d<f32>;
@group(0) @binding(1) var read1: texture_2d<f32>;
@group(0) @binding(2) var write0: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var write1: texture_storage_2d<rgba16float, write>;
@group(0) @binding(4) var<uniform> p: Params;
@group(0) @binding(5) var<storage, read> stamps: array<vec4<f32>>;

struct Cell {
    water: f32,
    wet: f32,
    pigment: vec3<f32>,
    deposited: vec3<f32>,
};

fn load(x: i32, y: i32) -> Cell {
    let limit = vec2<i32>(i32(p.width), i32(p.height)) - vec2<i32>(1, 1);
    let at = clamp(vec2<i32>(x, y), vec2<i32>(0, 0), limit);
    let a = textureLoad(read0, at, 0);
    let b = textureLoad(read1, at, 0);
    return Cell(a.x, a.y, vec3<f32>(a.z, a.w, b.x), vec3<f32>(b.y, b.z, b.w));
}

fn store(x: i32, y: i32, cell: Cell) {
    textureStore(write0, vec2<i32>(x, y),
        vec4<f32>(cell.water, cell.wet, cell.pigment.r, cell.pigment.g));
    textureStore(write1, vec2<i32>(x, y),
        vec4<f32>(cell.pigment.b, cell.deposited.r, cell.deposited.g, cell.deposited.b));
}

fn transport(here: f32, left: f32, right: f32, up: f32, down: f32,
             gx: f32, gy: f32) -> f32 {
    return max(gx, 0.0) * (here - left) + min(gx, 0.0) * (right - here)
        + max(gy, 0.0) * (here - up) + min(gy, 0.0) * (down - here);
}

@compute @workgroup_size(8, 8)
fn inject(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.width || id.y >= p.height) {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    let here = load(x, y);
    let at = vec2<f32>(f32(id.x), f32(id.y));
    var added = 0.0;
    var cover = 0.0;
    for (var i = 0u; i < p.count; i = i + 1u) {
        let stamp = stamps[i];
        let radius = max(stamp.z, 0.001);
        let distance = length(at - stamp.xy);
        if (distance <= radius) {
            let inside = clamp(1.0 - distance / radius, 0.0, 1.0);
            added = max(added, stamp.w * inside);
            cover = max(cover, inside);
        }
    }
    var cell = here;
    if (cover > 0.0) {
        cell.pigment = here.pigment
            + vec3<f32>(p.colour_r, p.colour_g, p.colour_b) * added;
        cell.water = clamp(max(here.water, added), 0.0, 1.0);
        cell.wet = clamp(max(here.wet, cover), 0.0, 1.0);
    }
    store(x, y, cell);
}

@compute @workgroup_size(8, 8)
fn step(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.width || id.y >= p.height) {
        return;
    }
    let x = i32(id.x);
    let y = i32(id.y);
    let here = load(x, y);
    let left = load(x - 1, y);
    let right = load(x + 1, y);
    let up = load(x, y - 1);
    let down = load(x, y + 1);
    // A dry cell with dry neighbours keeps its deposit and stops costing anything. A dry cell
    // next to water still has to step, or water could never spread into it.
    if (here.wet <= 0.0005 && here.water <= p.dry
        && left.water + right.water + up.water + down.water <= p.dry) {
        store(x, y, Cell(0.0, 0.0, here.pigment, here.deposited));
        return;
    }

    let lap_water = left.water + right.water + up.water + down.water - 4.0 * here.water;
    let water = clamp(
        here.water + p.dt * (p.diffusion * lap_water - p.evaporation), 0.0, 1.0);
    let wet = clamp(water / max(p.dry * 4.0, 0.001), 0.0, 1.0);

    // The water gradient moves pigment; a dry cell changes nothing.
    let gx = (right.water - left.water) * 0.5;
    let gy = (down.water - up.water) * 0.5;
    let moved = vec3<f32>(
        transport(here.pigment.r, left.pigment.r, right.pigment.r,
                  up.pigment.r, down.pigment.r, gx, gy),
        transport(here.pigment.g, left.pigment.g, right.pigment.g,
                  up.pigment.g, down.pigment.g, gx, gy),
        transport(here.pigment.b, left.pigment.b, right.pigment.b,
                  up.pigment.b, down.pigment.b, gx, gy));
    let lap_pigment = left.pigment + right.pigment + up.pigment + down.pigment
        - 4.0 * here.pigment;
    var pigment = max(here.pigment - p.dt * p.advection * moved, vec3<f32>(0.0));
    pigment = max(
        pigment + p.dt * p.pigment_diffusion * wet * lap_pigment, vec3<f32>(0.0));

    // Settle into the paper, faster where water is receding: the dark rim.
    let settle = clamp(
        p.dt * p.absorption * wet + p.dt * p.edge * max(-lap_water, 0.0) * wet,
        0.0, 1.0);
    let deposited = min(pigment, pigment * settle);
    pigment = pigment - deposited;
    store(x, y, Cell(water, wet, pigment, here.deposited + deposited));
}

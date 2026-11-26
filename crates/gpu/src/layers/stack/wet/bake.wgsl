// Bake one tile of the wet field's settled pigment into a resident layer.
//
// The deposit texture already holds straight `(settled rgb, coverage)` and is field-sized, so this
// shader only maps a canvas pixel to its nearest field texel. A pixel outside the deposit keeps the
// old tile byte-for-byte; that is what lets `apply_wet` touch a tile without wiping art the field
// never reached.
//
// The wet footprint is replaced, not source-overed. Baking the same dry field twice then produces
// the same pixels instead of a second, darker film.

struct Params {
    /// The tile's top-left corner in canvas pixels.
    origin: vec2<u32>,
    size: vec2<u32>,
    canvas: vec2<u32>,
    /// The field texel grid the deposit texture holds.
    field: vec2<u32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var old_tile: texture_2d<f32>;
@group(0) @binding(2) var deposit: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let local = vec2<u32>(u32(at.x), u32(at.y));
    if (local.x >= params.size.x || local.y >= params.size.y) {
        return vec4<f32>(0.0);
    }
    let canvas = params.origin + local;
    if (canvas.x >= params.canvas.x || canvas.y >= params.canvas.y) {
        return vec4<f32>(0.0);
    }
    let old = textureLoad(old_tile, vec2<i32>(i32(local.x), i32(local.y)), 0);
    let field = vec2<u32>(
        min(canvas.x * params.field.x / params.canvas.x, params.field.x - 1u),
        min(canvas.y * params.field.y / params.canvas.y, params.field.y - 1u),
    );
    let settled = textureLoad(deposit, vec2<i32>(i32(field.x), i32(field.y)), 0);
    if (settled.a <= 0.0) {
        return old;
    }
    return vec4<f32>(settled.rgb, settled.a);
}

// Blend one fill's coverage into a resident tile.
//
// The masked pixels are the only ones the core's formulas touch: an empty mask keeps the old
// pixel, and the tile padding outside the canvas is copied through unchanged. The output is
// straight (unpremultiplied) RGBA, which is how the tile backups and every readback store it.

struct FillParams {
    /// The tile's top-left corner in canvas pixels.
    origin: vec2<u32>,
    canvas: vec2<u32>,
    color: vec4<f32>,
    /// 0 source-over, 1 destination-over (under), 2 alpha lock.
    mode: u32,
};

@group(0) @binding(0) var<uniform> params: FillParams;
@group(0) @binding(1) var old_tile: texture_2d<f32>;
@group(0) @binding(2) var coverage: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    // One triangle that covers the whole tile.
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let local = vec2<i32>(at.xy);
    let old = textureLoad(old_tile, local, 0);
    let canvas = vec2<i32>(params.origin) + local;
    if canvas.x >= i32(params.canvas.x) || canvas.y >= i32(params.canvas.y) {
        return old;
    }
    let amount = clamp(textureLoad(coverage, canvas, 0).r, 0.0, 1.0);
    if amount <= 0.0 {
        return old;
    }
    let alpha = clamp(amount * params.color.a, 0.0, 1.0);
    var out_alpha: f32;
    var rgb: vec3<f32>;
    if params.mode == 2u {
        // Alpha lock keeps the old coverage and tints what is already there.
        if old.a <= 0.0 {
            return old;
        }
        out_alpha = old.a;
        rgb = old.rgb + (params.color.rgb - old.rgb) * alpha;
    } else if params.mode == 1u {
        out_alpha = old.a + alpha * (1.0 - old.a);
        if out_alpha <= 0.0 {
            return vec4<f32>(0.0);
        }
        rgb = (old.rgb * old.a + params.color.rgb * alpha * (1.0 - old.a)) * (1.0 / out_alpha);
    } else {
        out_alpha = alpha + old.a * (1.0 - alpha);
        if out_alpha <= 0.0 {
            return vec4<f32>(0.0);
        }
        rgb = (params.color.rgb * alpha + old.rgb * old.a * (1.0 - alpha)) * (1.0 / out_alpha);
    }
    return vec4<f32>(max(rgb, vec3<f32>(0.0)), out_alpha);
}

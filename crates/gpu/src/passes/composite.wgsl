// Layer compositing: one full-screen triangle per layer, ping-ponging between two render targets.
//
// The layer texture holds straight alpha. The backdrop texture holds premultiplied colour, so every
// pass samples it, applies the layer's blend mode, and writes a new premultiplied result. A single
// hardware alpha-blend target could only do normal, and a fragment shader cannot read the target it
// is writing, hence the two textures rather than a blend equation.
//
// The pass implements the W3C compositing formula:
//   Co = as(1-ab)Cs + as*ab*B(Cb, Cs) + (1-as)ab*Cb
//   ao = as + ab(1-as)
// with `As` already multiplied by the layer opacity.

struct Params {
    opacity: f32,
    mode: u32,
    use_fill: u32,
    use_clip: u32,
    fill: vec4<f32>,
    clip_fill: vec4<f32>,
};

@group(0) @binding(0) var layer_texture: texture_2d<f32>;
@group(0) @binding(1) var backdrop_texture: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var clip_texture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

fn luminance(pixel: vec3<f32>) -> f32 {
    return 0.30 * pixel.r + 0.59 * pixel.g + 0.11 * pixel.b;
}

// Give `pixel` the luminance `target`, keeping hue and saturation. Mirrors the core helper: after
// the shift the colour can leave the cube, so it is scaled toward the target instead of clamping
// each channel, which would change the hue.
fn with_luminance(pixel: vec3<f32>, level: f32) -> vec3<f32> {
    let shifted = pixel + (level - luminance(pixel));
    let low = min(shifted.r, min(shifted.g, shifted.b));
    let high = max(shifted.r, max(shifted.g, shifted.b));
    if (low < 0.0) {
        let scale = level / max(level - low, 1e-6);
        return vec3<f32>(level) + (shifted - vec3<f32>(level)) * scale;
    }
    if (high > 1.0) {
        let scale = (1.0 - level) / max(high - level, 1e-6);
        return vec3<f32>(level) + (shifted - vec3<f32>(level)) * scale;
    }
    return shifted;
}

// top is the layer, bottom is what is already painted below it. The numbering matches the core's
// `BlendMode::as_i32`.
fn blend_colour(mode: u32, top: vec3<f32>, bottom: vec3<f32>) -> vec3<f32> {
    switch mode {
        case 1u: {
            return top * bottom;
        }
        case 2u: {
            return vec3<f32>(1.0) - (vec3<f32>(1.0) - top) * (vec3<f32>(1.0) - bottom);
        }
        case 3u: {
            let low = 2.0 * top * bottom;
            let high = vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - top) * (vec3<f32>(1.0) - bottom);
            return select(high, low, bottom < vec3<f32>(0.5));
        }
        case 4u: {
            return min(top + bottom, vec3<f32>(1.0));
        }
        case 5u: {
            return min(top, bottom);
        }
        case 6u: {
            return max(top, bottom);
        }
        case 7u: {
            return with_luminance(top, clamp(luminance(bottom), 0.0, 1.0));
        }
        case 8u: {
            return with_luminance(bottom, clamp(luminance(top), 0.0, 1.0));
        }
        default: {
            return top;
        }
    }
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(position.xy);
    var source = textureLoad(layer_texture, coord, 0);
    if (params.use_fill == 1u) {
        source = params.fill;
    }
    let backdrop = textureLoad(backdrop_texture, coord, 0);
    var clip_alpha = 1.0;
    if (params.use_clip == 1u) {
        clip_alpha = textureLoad(clip_texture, coord, 0).a;
    } else if (params.use_clip == 2u) {
        clip_alpha = params.clip_fill.a;
    }
    let src_alpha = source.a * params.opacity * clip_alpha;
    let ab = backdrop.a;
    var bottom = vec3<f32>(0.0);
    if (ab > 0.0) {
        bottom = backdrop.rgb / ab;
    }
    let colour = blend_colour(params.mode, source.rgb, bottom);
    let co = src_alpha * (1.0 - ab) * source.rgb + src_alpha * ab * colour + (1.0 - src_alpha) * ab * bottom;
    let ao = src_alpha + ab * (1.0 - src_alpha);
    return vec4<f32>(co, ao);
}

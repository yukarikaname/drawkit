// One layer's resident tiles downscaled into a small RGBA8 preview.
//
// A tile is drawn as a quad covering its part of the thumbnail, so the right and bottom tiles,
// which are smaller than a full tile, stop at the canvas edge. Tiles hold straight alpha; the
// fragment premultiplies the sample because the palette expects premultiplied RGBA, and averaging
// a transparent pixel with an opaque one must not pull the colour towards black.

struct Params {
    // Target rectangle in output pixels: x, y, width, height.
    rect: vec4<f32>,
    // Source rectangle inside the tile in pixels: origin and valid size.
    src: vec4<f32>,
    out_size: vec2<f32>,
    tile: vec2<f32>,
    // 1 for half-float linear tiles, 0 for encoded eight-bit ones.
    linear: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var tile_texture: texture_2d<f32>;
@group(0) @binding(2) var tile_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let corner = corners[index];
    let pixel = params.rect.xy + corner * params.rect.zw;
    let ndc = vec2<f32>(
        pixel.x / params.out_size.x * 2.0 - 1.0,
        1.0 - pixel.y / params.out_size.y * 2.0,
    );
    let uv = (params.src.xy + corner * params.src.zw) / params.tile;
    var out: VertexOut;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = uv;
    return out;
}

// The same transfer the CPU preview applies to a half-float stack, which stores straight linear
// Display P3 and encodes it for an eight-bit palette.
fn encode_srgb(value: f32) -> f32 {
    let v = clamp(value, 0.0, 1.0);
    if (v <= 0.0031308) {
        return v * 12.92;
    }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Stay on the valid texels. A magnified preview would otherwise blend the zero padding of a
    // partial tile into its edge; a downscale never reaches this clamp.
    let low = params.src.xy + vec2<f32>(0.5);
    let high = params.src.xy + params.src.zw - vec2<f32>(0.5);
    let uv = clamp(in.uv * params.tile, low, high) / params.tile;
    let sample = textureSample(tile_texture, tile_sampler, uv);
    var colour = sample.rgb;
    if (params.linear != 0u) {
        colour = vec3<f32>(
            encode_srgb(colour.r),
            encode_srgb(colour.g),
            encode_srgb(colour.b),
        );
    }
    let alpha = clamp(sample.a, 0.0, 1.0);
    return vec4<f32>(colour * alpha, alpha);
}

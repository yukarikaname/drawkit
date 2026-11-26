// Zero the part of the coverage buffer the previous stroke painted. Clearing the whole canvas for
// every stroke costs more than the stroke itself at 4K, and the rest of the buffer is already zero.

struct Clear {
    width: u32,
    min_x: u32,
    min_y: u32,
    mask_w: u32,
    mask_h: u32,
};

@group(0) @binding(0) var<uniform> params: Clear;
@group(0) @binding(1) var<storage, read_write> coverage: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.mask_w || id.y >= params.mask_h) {
        return;
    }
    let x = params.min_x + id.x;
    let y = params.min_y + id.y;
    coverage[y * params.width + x] = 0.0;
}

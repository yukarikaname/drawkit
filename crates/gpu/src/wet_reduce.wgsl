// Largest water value in the field, as an atomic max of the positive f32 bit patterns. A shell
// asks this when it wants to decide that a document has dried, not every frame.

@group(0) @binding(0) var read0: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> max_water: atomic<u32>;

@compute @workgroup_size(8, 8)
fn peak(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(read0);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let water = textureLoad(read0, vec2<i32>(i32(id.x), i32(id.y)), 0).x;
    if (water > 0.0) {
        atomicMax(&max_water, bitcast<u32>(water));
    }
}

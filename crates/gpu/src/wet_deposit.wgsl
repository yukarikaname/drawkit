// Turn the current field pair into the two textures a layer bake or a wet preview can sample.
//
// `deposit` is what a bake lays down: the settled pigment as straight RGBA. The colour is the
// deposited pigment divided by its largest channel; the alpha is the optical coverage of that
// density, `1 - exp(-density)`. Kubelka-Munk replaces this approximation later, but until then the
// law is monotone, saturates at one and is exact for a dry field (density zero gives alpha zero).
//
// `mobile` is the pigment still suspended in the water. It stays in the field when the deposit is
// baked, so it is reported separately instead of being folded into the bake colour.

@group(0) @binding(0) var read0: texture_2d<f32>;
@group(0) @binding(1) var read1: texture_2d<f32>;
@group(0) @binding(2) var deposit_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var mobile_out: texture_storage_2d<rgba16float, write>;

fn largest(value: vec3<f32>) -> f32 {
    return max(value.r, max(value.g, value.b));
}

@compute @workgroup_size(8, 8)
fn deposit(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(read0);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let at = vec2<i32>(i32(id.x), i32(id.y));
    let a = textureLoad(read0, at, 0);
    let b = textureLoad(read1, at, 0);
    let mobile = vec3<f32>(a.z, a.w, b.x);
    let settled = vec3<f32>(b.y, b.z, b.w);

    let density = largest(settled);
    var alpha = 0.0;
    var colour = vec3<f32>(0.0);
    if (density > 0.0) {
        alpha = 1.0 - exp(-density);
        colour = settled / density;
    }
    textureStore(deposit_out, at, vec4<f32>(colour, alpha));
    textureStore(mobile_out, at, vec4<f32>(mobile, largest(mobile)));
}

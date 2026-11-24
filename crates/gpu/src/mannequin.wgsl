// The mannequin shader: world-space triangles, one directional light, two-sided shading.
//
// Two-sided because the mesh is made of overlapping balls and tapers, not a closed surface with a
// guaranteed winding; the depth buffer settles what is visible, and the normal settles the light.

struct Uniforms {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light: vec4<f32>,
    color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world: vec3<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = uniforms.view_proj * vec4<f32>(input.position, 1.0);
    out.normal = input.normal;
    out.world = input.position;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    var normal = normalize(input.normal);
    let view = normalize(uniforms.eye.xyz - input.world);
    if (dot(normal, view) < 0.0) {
        normal = -normal;
    }
    let shade = 0.30 + 0.70 * clamp(dot(normal, normalize(uniforms.light.xyz)), 0.0, 1.0);
    return vec4<f32>(uniforms.color.rgb * shade, 1.0);
}

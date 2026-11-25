// The mannequin shader: world-space triangles, one directional light, two-sided shading.
//
// Two-sided because the mesh is made of overlapping balls and tapers, not a closed surface with a
// guaranteed winding; the depth buffer settles what is visible, and the normal settles the light.

struct Uniforms {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    // xyz points toward the light; w is the intensity.
    key: vec4<f32>,
    fill: vec4<f32>,
    rim: vec4<f32>,
    // rgb is the albedo; w is the ambient term.
    color: vec4<f32>,
    // x is the toon flag; y/z are reserved.
    style: vec4<f32>,
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
    let key = clamp(dot(normal, normalize(uniforms.key.xyz)), 0.0, 1.0) * uniforms.key.w;
    let fill = clamp(dot(normal, normalize(uniforms.fill.xyz)), 0.0, 1.0) * uniforms.fill.w;
    let edge = pow(1.0 - clamp(dot(normal, view), 0.0, 1.0), 3.0) * uniforms.rim.w;
    let shade = uniforms.color.w + key + fill;
    var rgb = uniforms.color.rgb * shade + vec3<f32>(edge);
    if uniforms.style.x > 0.5 {
        let value = dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
        let toon = select(0.30, select(0.64, 0.92, value >= 0.64), value >= 0.30);
        rgb = vec3<f32>(toon);
    }
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

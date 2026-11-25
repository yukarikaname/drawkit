use super::*;

impl Renderer {
    pub(super) fn uniforms(
        &self,
        camera: &Camera,
        width: u32,
        height: u32,
        color: [f32; 3],
        lighting: &Lighting,
        style: &RenderStyle,
    ) -> Uniforms {
        let view = Mat4::look_at_rh(camera.eye, camera.target, camera.up);
        let projection =
            Mat4::perspective_rh(camera.fov_y, width as f32 / height as f32, 0.05, 100.0);
        let light = |light: drawkit_figure::Light| {
            light
                .direction
                .normalize_or_zero()
                .extend(light.intensity)
                .to_array()
        };
        Uniforms {
            view_proj: (projection * view).to_cols_array_2d(),
            eye: camera.eye.extend(1.0).to_array(),
            key: light(lighting.key),
            fill: light(lighting.fill),
            rim: light(lighting.rim),
            color: [color[0], color[1], color[2], lighting.ambient],
            style: [if style.toon { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        }
    }
}

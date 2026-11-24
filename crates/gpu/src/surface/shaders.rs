pub(super) fn dirty_region(
    recreated: bool,
    dirty: Option<(u32, u32, u32, u32)>,
    canvas_width: u32,
    canvas_height: u32,
) -> Option<(u32, u32, u32, u32)> {
    let dirty = if recreated {
        Some((0, 0, canvas_width, canvas_height))
    } else {
        dirty
    };
    let (x, y, width, height) = dirty?;
    let x = x.min(canvas_width.saturating_sub(1));
    let y = y.min(canvas_height.saturating_sub(1));
    let width = width.min(canvas_width - x);
    let height = height.min(canvas_height - y);
    (width > 0 && height > 0).then_some((x, y, width, height))
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(super) fn pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    hdr: bool,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("canvas surface"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../surface.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("canvas surface"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(if hdr { "fs_main_hdr" } else { "fs_main" }),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

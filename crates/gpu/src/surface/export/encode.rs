//! Encode a linear Display P3 composite into the document's working space for an eight-bit export.

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    rows: [[f32; 4]; 3],
    gamma: u32,
    pad: [u32; 3],
}

/// Linear working RGB to linear Display P3. The export needs the inverse.
fn to_display_p3(space: i32) -> [[f32; 3]; 3] {
    match space {
        1 => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        2 => [
            [1.150_217_9, -0.150_090_9, 0.0],
            [0.046_422_2, 0.953_560_7, 0.0],
            [0.023_890_2, 0.026_504_1, 0.949_388_2],
        ],
        _ => [
            [0.822_462_1, 0.177_537_9, 0.0],
            [0.033_194_1, 0.966_805_9, 0.0],
            [0.017_082_6, 0.072_397_4, 0.910_519_9],
        ],
    }
}

fn inverse(m: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let (a, b, c) = (m[0][0], m[0][1], m[0][2]);
    let (d, e, f) = (m[1][0], m[1][1], m[1][2]);
    let (g, h, i) = (m[2][0], m[2][1], m[2][2]);
    let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if determinant.abs() < 1e-12 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let scale = 1.0 / determinant;
    [
        [
            (e * i - f * h) * scale,
            (c * h - b * i) * scale,
            (b * f - c * e) * scale,
        ],
        [
            (f * g - d * i) * scale,
            (a * i - c * g) * scale,
            (c * d - a * f) * scale,
        ],
        [
            (d * h - e * g) * scale,
            (b * g - a * h) * scale,
            (a * e - b * d) * scale,
        ],
    ]
}

fn pipeline(device: &wgpu::Device) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("canvas export encode"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../export_encode.wgsl").into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("canvas export encode"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("canvas export encode"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("canvas export encode"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            }),
        ),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_encode"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, layout)
}

pub(crate) fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Texture,
    width: u32,
    height: u32,
    space: i32,
) -> Option<Vec<u8>> {
    let matrix = inverse(to_display_p3(space));
    let params = Params {
        rows: [
            [matrix[0][0], matrix[0][1], matrix[0][2], 0.0],
            [matrix[1][0], matrix[1][1], matrix[1][2], 0.0],
            [matrix[2][0], matrix[2][1], matrix[2][2], 0.0],
        ],
        gamma: u32::from(space == 2),
        pad: [0; 3],
    };
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("canvas export encode"),
        size: std::mem::size_of::<Params>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&params));
    let (pipeline, layout) = pipeline(device);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("canvas export encode"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("canvas export encode"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });
    let source_view = source.create_view(&Default::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("canvas export encode"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&source_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("canvas export encode"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    Some(crate::readback::read_texture(
        device, queue, &target, width, height,
    ))
}

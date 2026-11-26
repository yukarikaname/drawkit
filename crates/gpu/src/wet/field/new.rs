//! Construction of a field: textures, pipelines and the fixed bind groups.

use super::super::{DT, MAX_STAMPS, Params, STATE, WetSettings, field_size};
use super::WetField;
use super::entries::{storage_entry, texture_entry};

impl WetField {
    /// A field at one texel per document pixel.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        settings: WetSettings,
    ) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let texture = |label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: STATE,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let state0 = [texture("wet state 0a"), texture("wet state 0b")];
        let state1 = [texture("wet state 1a"), texture("wet state 1b")];
        let views: Vec<wgpu::TextureView> = state0
            .iter()
            .chain(state1.iter())
            .map(|texture| texture.create_view(&Default::default()))
            .collect();

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wet"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                storage_entry(2),
                storage_entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wet"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wet"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../wet.wgsl").into()),
        });
        let pipeline = |label: &str, entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let step_pipeline = pipeline("wet step", "step");
        let inject_pipeline = pipeline("wet inject", "inject");

        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wet params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let stamps = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wet stamps"),
            size: (MAX_STAMPS * 4 * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let step_groups = [
            bind_group(device, &layout, &views, &params, &stamps, 0),
            bind_group(device, &layout, &views, &params, &stamps, 1),
        ];
        let inject_groups = [
            bind_group(device, &layout, &views, &params, &stamps, 0),
            bind_group(device, &layout, &views, &params, &stamps, 1),
        ];
        queue.write_buffer(
            &params,
            0,
            bytemuck::bytes_of(&Params::step(width, height, DT, settings)),
        );

        let reduce_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wet reduce"),
            entries: &[
                texture_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let reduce_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wet reduce"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../wet_reduce.wgsl").into()),
        });
        let reduce_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("wet reduce"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("wet reduce"),
                    bind_group_layouts: &[Some(&reduce_layout)],
                    immediate_size: 0,
                }),
            ),
            module: &reduce_shader,
            entry_point: Some("peak"),
            compilation_options: Default::default(),
            cache: None,
        });
        let peak = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wet peak"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let reduce_groups = [
            reduce_group(device, &reduce_layout, &views[0], &peak),
            reduce_group(device, &reduce_layout, &views[1], &peak),
        ];

        // The deposit pass has its own layout: two state reads and two straight-RGBA outputs.
        let deposit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wet deposit"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                storage_entry(2),
                storage_entry(3),
            ],
        });
        let deposit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wet deposit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../wet_deposit.wgsl").into()),
        });
        let deposit_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("wet deposit"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("wet deposit"),
                    bind_group_layouts: &[Some(&deposit_layout)],
                    immediate_size: 0,
                }),
            ),
            module: &deposit_shader,
            entry_point: Some("deposit"),
            compilation_options: Default::default(),
            cache: None,
        });
        Some(Self {
            canvas: (width, height),
            scale: 1.0,
            width,
            height,
            state0,
            state1,
            current: 0,
            step_pipeline,
            inject_pipeline,
            reduce_pipeline,
            deposit_pipeline,
            params,
            stamps,
            peak,
            step_groups,
            inject_groups,
            reduce_groups,
            settings,
            pending: 0.0,
            stepped: false,
        })
    }

    /// A field for a document, pinned at `scale` texels per document pixel.
    pub fn for_canvas(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvas: (u32, u32),
        scale: f32,
        settings: WetSettings,
    ) -> Option<Self> {
        let scale = scale.clamp(0.1, 1.0);
        let (width, height) = field_size(canvas, scale);
        let mut field = Self::new(device, queue, width, height, settings)?;
        field.canvas = canvas;
        field.scale = scale;
        Some(field)
    }
}

fn reduce_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    peak: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wet reduce"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: peak.as_entire_binding(),
            },
        ],
    })
}

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    views: &[wgpu::TextureView],
    params: &wgpu::Buffer,
    stamps: &wgpu::Buffer,
    target: usize,
) -> wgpu::BindGroup {
    let read = |half: usize| &views[half * 2 + target];
    let write = |half: usize| &views[half * 2 + (1 - target)];
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wet"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(read(0)),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(read(1)),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(write(0)),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(write(1)),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: stamps.as_entire_binding(),
            },
        ],
    })
}

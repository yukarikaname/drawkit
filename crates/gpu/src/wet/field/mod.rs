//! The field itself: textures, pipelines, injection, stepping and reading.

mod entries;
mod read;

use entries::{storage_entry, texture_entry};

use super::{DT, MAX_STAMPS, Params, STATE, WORKGROUP, WetSettings, field_size};

pub struct WetField {
    canvas: (u32, u32),
    scale: f32,
    width: u32,
    height: u32,
    state0: [wgpu::Texture; 2],
    state1: [wgpu::Texture; 2],
    /// The pair being read; a pass writes the other one.
    current: usize,
    step_pipeline: wgpu::ComputePipeline,
    inject_pipeline: wgpu::ComputePipeline,
    reduce_pipeline: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    stamps: wgpu::Buffer,
    peak: wgpu::Buffer,
    /// `[a -> b, b -> a]` for each pipeline.
    step_groups: [wgpu::BindGroup; 2],
    inject_groups: [wgpu::BindGroup; 2],
    reduce_groups: [wgpu::BindGroup; 2],
    settings: WetSettings,
    /// Seconds that did not fill a whole step, so a caller can pass frame time.
    pending: f32,
}

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
            params,
            stamps,
            peak,
            step_groups,
            inject_groups,
            reduce_groups,
            settings,
            pending: 0.0,
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

    /// Field texels, not document pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn canvas_size(&self) -> (u32, u32) {
        self.canvas
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn settings(&self) -> WetSettings {
        self.settings
    }

    /// Add water and pigment in a soft disc. Coordinates and radius are document pixels.
    pub fn inject(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        centre: (u32, u32),
        radius: f32,
        water: f32,
        colour: [f32; 3],
    ) {
        let scale = self.scale;
        self.inject_stamps(
            device,
            queue,
            &[[
                centre.0 as f32 * scale,
                centre.1 as f32 * scale,
                (radius * scale).max(0.5),
                water,
            ]],
            colour,
        );
    }

    /// Add a stroke segment as a run of stamps. A segment longer than `MAX_STAMPS` stamps is
    /// spaced out rather than split; the brush splits its own segments.
    #[allow(clippy::too_many_arguments)]
    pub fn inject_stroke(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        water: f32,
        colour: [f32; 3],
    ) {
        let scale = self.scale;
        let (from, to) = (
            (from.0 * scale, from.1 * scale),
            (to.0 * scale, to.1 * scale),
        );
        let radius = (radius * scale).max(0.5);
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = (dx * dx + dy * dy).sqrt();
        let spacing = (radius * 0.35).max(0.75);
        let count = ((length / spacing).ceil() as usize + 1).min(MAX_STAMPS);
        let mut stamps = Vec::with_capacity(count);
        for index in 0..count {
            let t = if count <= 1 {
                0.0
            } else {
                index as f32 / (count - 1) as f32
            };
            stamps.push([from.0 + dx * t, from.1 + dy * t, radius, water]);
        }
        self.inject_stamps(device, queue, &stamps, colour);
    }

    fn inject_stamps(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        stamps: &[[f32; 4]],
        colour: [f32; 3],
    ) {
        if stamps.is_empty() {
            return;
        }
        let count = stamps.len().min(MAX_STAMPS);
        let mut packed = vec![0f32; MAX_STAMPS * 4];
        for (index, stamp) in stamps[..count].iter().enumerate() {
            packed[index * 4..index * 4 + 4].copy_from_slice(stamp);
        }
        queue.write_buffer(&self.stamps, 0, bytemuck::cast_slice(&packed));
        let mut params = Params::step(self.width, self.height, DT, self.settings);
        params.count = count as u32;
        params.colour_r = colour[0];
        params.colour_g = colour[1];
        params.colour_b = colour[2];
        params.colour_a = 1.0;
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("wet inject"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.inject_pipeline);
            pass.set_bind_group(0, &self.inject_groups[self.current], &[]);
            pass.dispatch_workgroups(
                self.width.div_ceil(WORKGROUP),
                self.height.div_ceil(WORKGROUP),
                1,
            );
        }
        queue.submit(Some(encoder.finish()));
        self.current = 1 - self.current;
    }

    /// Advance the field by fixed 1/60 s steps.
    pub fn advance(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, steps: usize) {
        if steps == 0 {
            return;
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        for _ in 0..steps {
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("wet step"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.step_pipeline);
                pass.set_bind_group(0, &self.step_groups[self.current], &[]);
                pass.dispatch_workgroups(
                    self.width.div_ceil(WORKGROUP),
                    self.height.div_ceil(WORKGROUP),
                    1,
                );
            }
            self.current = 1 - self.current;
        }
        queue.submit(Some(encoder.finish()));
    }

    /// Advance by elapsed seconds, keeping the remainder for the next call. At most eight steps
    /// run in one call so a stalled frame does not turn into a simulation spiral.
    pub fn advance_seconds(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, seconds: f32) {
        self.pending += seconds.clamp(0.0, 0.25);
        let steps = (self.pending / DT) as usize;
        let run = steps.min(8);
        if run == 0 {
            return;
        }
        self.pending -= run as f32 * DT;
        self.advance(device, queue, run);
    }

    /// The largest water value in the field. This reads back, so a shell should ask it when it
    /// wants to bake or stop stepping, not every frame.
    pub fn peak_water(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> f32 {
        queue.write_buffer(&self.peak, 0, &0u32.to_le_bytes());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("wet peak"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.reduce_pipeline);
            pass.set_bind_group(0, &self.reduce_groups[self.current], &[]);
            pass.dispatch_workgroups(
                self.width.div_ceil(WORKGROUP),
                self.height.div_ceil(WORKGROUP),
                1,
            );
        }
        queue.submit(Some(encoder.finish()));
        let bytes = crate::readback::read_buffer(device, queue, &self.peak, 4);
        f32::from_bits(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// True when nothing is wet enough to keep evolving.
    pub fn is_dry(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        self.peak_water(device, queue) <= self.settings.dry * 2.0
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

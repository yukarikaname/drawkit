//! Wet media: water and pigment on paper, stepped as a field.
//!
//! This is the first experiment for a watercolor runtime, not the final one. One field pair,
//! eight channels in two `Rgba16Float` textures, two operations (inject, step) and a readback.
//! The production shape (tiles, halo, a wet-region scheduler, checkpoints and the replay
//! contract) is in `docs/wet-media.md`.
//!
//! State, per cell:
//! - texture 0: water, wetness, mobile pigment red, mobile pigment green
//! - texture 1: mobile pigment blue, deposited red, deposited green, deposited blue
//!
//! The step is gather-only: each cell reads its four neighbours from the current pair and writes
//! itself into the next pair. Nothing writes a neighbour, so splitting this into tiles with a
//! read-only halo does not change the result.

use half::f16;

const STATE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const WORKGROUP: u32 = 8;

/// Physical knobs. The prototype keeps them fixed for a test.
#[derive(Clone, Copy, Debug)]
pub struct WetSettings {
    /// Water diffusion, in cells squared per second. Explicit Euler is stable while
    /// `diffusion * dt <= 0.25`.
    pub diffusion: f32,
    /// Water lost per second.
    pub evaporation: f32,
    /// Share of mobile pigment that settles per second where the paper is wet.
    pub absorption: f32,
    /// Extra deposition where water is receding, which is the dark rim.
    pub edge: f32,
    /// Pigment movement with the water gradient.
    pub advection: f32,
    /// Pigment diffusion while wet.
    pub pigment_diffusion: f32,
    /// Water below this is dry and the cell stops evolving.
    pub dry: f32,
}

impl Default for WetSettings {
    fn default() -> Self {
        Self {
            diffusion: 15.0,
            evaporation: 0.005,
            absorption: 0.03,
            edge: 1.5,
            advection: 2.0,
            pigment_diffusion: 6.0,
            dry: 0.001,
        }
    }
}

/// Scalars only, so the Rust struct and the WGSL struct have the same layout.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    width: u32,
    height: u32,
    dt: f32,
    diffusion: f32,
    evaporation: f32,
    absorption: f32,
    edge: f32,
    advection: f32,
    pigment_diffusion: f32,
    dry: f32,
    centre_x: u32,
    centre_y: u32,
    radius: f32,
    water: f32,
    colour_r: f32,
    colour_g: f32,
    colour_b: f32,
    colour_a: f32,
}

impl Params {
    fn step(width: u32, height: u32, dt: f32, settings: WetSettings) -> Self {
        Self {
            width,
            height,
            dt,
            diffusion: settings.diffusion,
            evaporation: settings.evaporation,
            absorption: settings.absorption,
            edge: settings.edge,
            advection: settings.advection,
            pigment_diffusion: settings.pigment_diffusion,
            dry: settings.dry,
            centre_x: 0,
            centre_y: 0,
            radius: 0.0,
            water: 0.0,
            colour_r: 0.0,
            colour_g: 0.0,
            colour_b: 0.0,
            colour_a: 0.0,
        }
    }
}

pub struct WetField {
    width: u32,
    height: u32,
    state0: [wgpu::Texture; 2],
    state1: [wgpu::Texture; 2],
    /// The pair being read; a pass writes the other one.
    current: usize,
    step_pipeline: wgpu::ComputePipeline,
    inject_pipeline: wgpu::ComputePipeline,
    inject_params: wgpu::Buffer,
    /// `[a -> b, b -> a]` for each pipeline.
    step_groups: [wgpu::BindGroup; 2],
    inject_groups: [wgpu::BindGroup; 2],
    settings: WetSettings,
}

impl WetField {
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
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: STATE,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: STATE,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
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
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wet"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wet"),
            source: wgpu::ShaderSource::Wgsl(include_str!("wet.wgsl").into()),
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
        let params_size = std::mem::size_of::<Params>() as u64;
        let uniform = |label: &str| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: params_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let step_params = uniform("wet step params");
        let inject_params = uniform("wet inject params");
        let step_groups = [
            bind_group(device, &layout, &views, &step_params, 0),
            bind_group(device, &layout, &views, &step_params, 1),
        ];
        let inject_groups = [
            bind_group(device, &layout, &views, &inject_params, 0),
            bind_group(device, &layout, &views, &inject_params, 1),
        ];
        queue.write_buffer(
            &step_params,
            0,
            bytemuck::bytes_of(&Params::step(width, height, 1.0 / 60.0, settings)),
        );
        Some(Self {
            width,
            height,
            state0,
            state1,
            current: 0,
            step_pipeline,
            inject_pipeline,
            inject_params,
            step_groups,
            inject_groups,
            settings,
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn settings(&self) -> WetSettings {
        self.settings
    }

    /// Add water and pigment in a soft disc. One injection per brush segment.
    pub fn inject(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        centre: (u32, u32),
        radius: f32,
        water: f32,
        colour: [f32; 3],
    ) {
        let mut params = Params::step(self.width, self.height, 1.0 / 60.0, self.settings);
        params.centre_x = centre.0;
        params.centre_y = centre.1;
        params.radius = radius.max(0.5);
        params.water = water.clamp(0.0, 1.0);
        params.colour_r = colour[0];
        params.colour_g = colour[1];
        params.colour_b = colour[2];
        queue.write_buffer(&self.inject_params, 0, bytemuck::bytes_of(&params));
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

    /// Both halves of the current state, row-major. For tests and the prototype report.
    pub fn read(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> [Vec<[f32; 4]>; 2] {
        let read = |texture: &wgpu::Texture| -> Vec<[f32; 4]> {
            let row = self.width as usize * 8;
            let padded = row.div_ceil(256) * 256;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wet read"),
                size: (padded * self.height as usize) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded as u32),
                        rows_per_image: Some(self.height),
                    },
                },
                wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(Some(encoder.finish()));
            buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let mapped = buffer.slice(..).get_mapped_range().expect("wet read map");
            let mut out = Vec::with_capacity(self.width as usize * self.height as usize);
            for y in 0..self.height as usize {
                for x in 0..self.width as usize {
                    let at = y * padded + x * 8;
                    let channel = |offset: usize| {
                        let bits =
                            u16::from_le_bytes([mapped[at + offset], mapped[at + offset + 1]]);
                        f16::from_bits(bits).to_f32()
                    };
                    out.push([channel(0), channel(2), channel(4), channel(6)]);
                }
            }
            drop(mapped);
            buffer.unmap();
            out
        };
        [
            read(&self.state0[self.current]),
            read(&self.state1[self.current]),
        ]
    }
}

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    views: &[wgpu::TextureView],
    uniform: &wgpu::Buffer,
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
                resource: uniform.as_entire_binding(),
            },
        ],
    })
}

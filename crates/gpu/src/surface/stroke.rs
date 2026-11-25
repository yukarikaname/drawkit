//! GPU stroke coverage for the live preview. The buffer lives on the GPU between segments; only
//! the present pass reads it, so a pointer sample never uploads or reads back the canvas.

use super::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct StrokeParams {
    width: u32,
    height: u32,
    min_x: u32,
    min_y: u32,
    mask_w: u32,
    mask_h: u32,
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    radius: f32,
    hardness: f32,
    spacing: f32,
    roundness: f32,
    angle: f32,
    phase: f32,
    alpha: f32,
    _pad: f32,
}

pub(super) struct StrokeCoverage {
    pub(super) buffer: wgpu::Buffer,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) color: [f32; 4],
    pub(super) phase: f32,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    slot: usize,
    /// Inclusive canvas rectangle touched so far, so a commit only reads back what was painted.
    dirty: Option<(u32, u32, u32, u32)>,
}

const UNIFORM_STRIDE: u64 = 256;
const UNIFORM_SLOTS: u64 = 128;

/// The bind group is built once per stroke and the segment parameters live in a ring, so a segment
/// only writes its uniform and rebinds with a dynamic offset.
pub(super) fn pipeline(device: &wgpu::Device) -> (wgpu::ComputePipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("stroke coverage"),
        source: wgpu::ShaderSource::Wgsl(include_str!("stroke.wgsl").into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("stroke coverage"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: std::num::NonZeroU64::new(
                        std::mem::size_of::<StrokeParams>() as u64,
                    ),
                },
                count: None,
            },
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
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("stroke coverage"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("stroke coverage"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    (pipeline, layout)
}

pub struct StrokePreview {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    memory: Option<(wgpu::Buffer, u32, u32)>,
    stroke: Option<StrokeCoverage>,
}

impl StrokePreview {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let (pipeline, layout) = pipeline(device);
        Self {
            device: device.clone(),
            queue: queue.clone(),
            pipeline,
            layout,
            memory: None,
            stroke: None,
        }
    }

    pub fn color(&self) -> Option<[f32; 4]> {
        self.stroke.as_ref().map(|stroke| stroke.color)
    }

    pub fn coverage(&self) -> Option<&wgpu::Buffer> {
        self.stroke.as_ref().map(|stroke| &stroke.buffer)
    }

    pub fn dirty(&self) -> Option<(u32, u32, u32, u32)> {
        self.stroke.as_ref()?.dirty
    }

    pub fn begin(&mut self, width: u32, height: u32, color: [f32; 4]) {
        if width == 0 || height == 0 {
            return;
        }
        let size = width as u64 * height as u64 * 4;
        let buffer = match &self.memory {
            Some((buffer, w, h)) if *w == width && *h == height => buffer.clone(),
            _ => {
                let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("stroke coverage"),
                    size,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
                self.memory = Some((buffer.clone(), width, height));
                buffer
            }
        };
        // Clear on the GPU: a 4K coverage buffer is 64 MB and a host-side zero fill would cost more
        // than the whole first sample.
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.clear_buffer(&buffer, 0, None);
        self.queue.submit(Some(encoder.finish()));
        let uniforms = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stroke uniforms"),
            size: UNIFORM_STRIDE * UNIFORM_SLOTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stroke coverage"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniforms,
                        offset: 0,
                        size: std::num::NonZeroU64::new(std::mem::size_of::<StrokeParams>() as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        self.stroke = Some(StrokeCoverage {
            buffer,
            width,
            height,
            color,
            phase: 0.0,
            uniforms,
            bind_group,
            slot: 0,
            dirty: None,
        });
    }

    pub fn end(&mut self) {
        self.stroke = None;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        hardness: f32,
        spacing: f32,
        roundness: f32,
        angle: f32,
    ) {
        let Some(stroke) = self.stroke.as_ref() else {
            return;
        };
        let spacing_distance = (radius * spacing.max(0.02)).max(0.5);
        let distance = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
        let phase = stroke.phase;
        if radius <= 0.0 {
            return;
        }
        let min_x = (from.0.min(to.0) - radius).floor().max(0.0) as u32;
        let min_y = (from.1.min(to.1) - radius).floor().max(0.0) as u32;
        let max_x = ((from.0.max(to.0) + radius).ceil().max(0.0) as u32).min(stroke.width - 1);
        let max_y = ((from.1.max(to.1) + radius).ceil().max(0.0) as u32).min(stroke.height - 1);
        if min_x > max_x || min_y > max_y {
            return;
        }
        let mask_w = max_x - min_x + 1;
        let mask_h = max_y - min_y + 1;
        let params = StrokeParams {
            width: stroke.width,
            height: stroke.height,
            min_x,
            min_y,
            mask_w,
            mask_h,
            from_x: from.0,
            from_y: from.1,
            to_x: to.0,
            to_y: to.1,
            radius,
            hardness,
            spacing,
            roundness,
            angle,
            phase: stroke.phase,
            alpha: stroke.color[3].clamp(0.0, 1.0),
            _pad: 0.0,
        };
        let Some(stroke) = self.stroke.as_ref() else {
            return;
        };
        let offset = stroke.slot as u64 * UNIFORM_STRIDE;
        self.queue
            .write_buffer(&stroke.uniforms, offset, bytemuck::bytes_of(&params));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("stroke coverage"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &stroke.bind_group, &[offset as u32]);
            pass.dispatch_workgroups(mask_w.div_ceil(8), mask_h.div_ceil(8), 1);
        }
        self.queue.submit(Some(encoder.finish()));
        let next_phase = if distance <= 0.0001 {
            if phase <= 0.0001 {
                spacing_distance
            } else {
                phase
            }
        } else {
            let mut next = phase.clamp(0.0, spacing_distance);
            let mut travelled = 0.0;
            if next <= 0.0001 {
                next = spacing_distance;
            }
            while travelled + next <= distance + 0.0001 {
                travelled += next;
                next = spacing_distance;
            }
            let value = (next - (distance - travelled)).clamp(0.0, spacing_distance);
            if value <= 0.0001 {
                spacing_distance
            } else {
                value
            }
        };
        if let Some(stroke) = self.stroke.as_mut() {
            stroke.phase = next_phase;
            stroke.slot = (stroke.slot + 1) % UNIFORM_SLOTS as usize;
            stroke.dirty = Some(match stroke.dirty {
                Some((x0, y0, x1, y1)) => {
                    (x0.min(min_x), y0.min(min_y), x1.max(max_x), y1.max(max_y))
                }
                None => (min_x, min_y, max_x, max_y),
            });
        }
    }
}

impl CanvasSurface {
    pub fn stroke_begin(&mut self, width: u32, height: u32, color: [f32; 4]) {
        self.stroke.begin(width, height, color);
    }

    pub fn stroke_end(&mut self) {
        self.stroke.end();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stroke_segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        hardness: f32,
        spacing: f32,
        roundness: f32,
        angle: f32,
    ) {
        self.stroke
            .segment(from, to, radius, hardness, spacing, roundness, angle);
    }

    pub fn stroke_dirty(&self) -> Option<(u32, u32, u32, u32)> {
        self.stroke.dirty()
    }
}

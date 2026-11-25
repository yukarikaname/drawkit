//! GPU stroke coverage for the live preview. The buffer lives on the GPU between segments; only
//! the present pass reads it, so a pointer sample never uploads or reads back the canvas.

use super::*;

pub(super) struct StrokeCoverage {
    pub(super) buffer: wgpu::Buffer,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) color: [f32; 4],
    pub(super) phase: f32,
}

pub(super) fn pipeline(device: &wgpu::Device) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("stroke coverage"),
        source: wgpu::ShaderSource::Wgsl(include_str!("stroke.wgsl").into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("stroke coverage"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

impl CanvasSurface {
    pub fn stroke_begin(&mut self, width: u32, height: u32, color: [f32; 4]) {
        if width == 0 || height == 0 {
            return;
        }
        let size = width as u64 * height as u64 * 4;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stroke coverage"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&buffer, 0, &vec![0_u8; size as usize]);
        self.stroke = Some(StrokeCoverage {
            buffer,
            width,
            height,
            color,
            phase: 0.0,
        });
    }

    pub fn stroke_end(&mut self) {
        self.stroke = None;
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
        let Some(stroke) = self.stroke.as_ref() else {
            return;
        };
        let spacing_distance = (spacing * radius).max(0.5);
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
        };
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stroke params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let layout = self.stroke_pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stroke coverage"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: stroke.buffer.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("stroke coverage"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.stroke_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
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
        }
    }
}

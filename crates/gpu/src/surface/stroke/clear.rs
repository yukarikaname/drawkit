//! Zeroing the coverage buffer between strokes. The buffer is a float per canvas pixel, so
//! clearing all of it every stroke costs more than the stroke; only the painted rectangle is reset.

use super::*;

/// Zero only the rectangle the previous stroke touched. The buffer is four bytes a pixel, so a
/// full clear is 16 MB at 2K and 64 MB at 4K, which dwarfs the coverage of a short stroke.
pub(super) fn clear_pipeline(
    device: &wgpu::Device,
) -> (wgpu::ComputePipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("coverage clear"),
        source: wgpu::ShaderSource::Wgsl(include_str!("clear.wgsl").into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("coverage clear"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
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
        label: Some("coverage clear"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("coverage clear"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    (pipeline, layout)
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct ClearParams {
    width: u32,
    min_x: u32,
    min_y: u32,
    mask_w: u32,
    mask_h: u32,
}

impl StrokePreview {
    /// Zero the painted rectangle of the coverage buffer. Without a rectangle (the first stroke on
    /// a new buffer) the whole buffer is cleared on the GPU.
    pub(super) fn clear_coverage(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
        dirty: Option<(u32, u32, u32, u32)>,
    ) {
        let Some(memory) = self.memory.as_ref() else {
            return;
        };
        match dirty {
            Some((x0, y0, x1, y1)) => {
                let right = (x1 + 1).min(width);
                let bottom = (y1 + 1).min(height);
                if x0 >= right || y0 >= bottom {
                    return;
                }
                let params = ClearParams {
                    width,
                    min_x: x0,
                    min_y: y0,
                    mask_w: right - x0,
                    mask_h: bottom - y0,
                };
                self.queue
                    .write_buffer(&memory.clear_uniform, 0, bytemuck::bytes_of(&params));
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("coverage clear"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.clear_pipeline);
                    pass.set_bind_group(0, &memory.clear_bind_group, &[]);
                    pass.dispatch_workgroups(
                        (right - x0).div_ceil(8),
                        (bottom - y0).div_ceil(8),
                        1,
                    );
                }
            }
            None => encoder.clear_buffer(buffer, 0, None),
        }
    }

    /// Clear the rectangle a commit just read, in the commit's own submission. `rect` is
    /// `(x, y, width, height)`; the clear takes inclusive bounds.
    pub(crate) fn clear_after_commit(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        rect: (u32, u32, u32, u32),
    ) {
        self.coverage_clean = false;
        let Some(buffer) = self.stroke.as_ref().map(|stroke| stroke.buffer.clone()) else {
            return;
        };
        let (x, y, width, height) = rect;
        let (canvas_width, canvas_height) = self
            .stroke
            .as_ref()
            .map_or((width, height), |stroke| (stroke.width, stroke.height));
        if width == 0 || height == 0 {
            self.coverage_clean = true;
            return;
        }
        self.clear_coverage(
            encoder,
            &buffer,
            canvas_width,
            canvas_height,
            Some((x, y, x + width - 1, y + height - 1)),
        );
        self.coverage_clean = true;
    }

    /// Encode the deferred segments into an encoder the caller submits, so a replay can put the
    /// coverage and the commit that reads it in one submission.
    pub fn encode(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if self.pending_segments.is_empty() {
            return;
        }
        let segments = std::mem::take(&mut self.pending_segments);
        let params = std::mem::take(&mut self.pending_params);
        let Some(stroke) = self.stroke.as_ref() else {
            return;
        };
        self.queue.write_buffer(&stroke.uniforms, 0, &params);
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("stroke coverage"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        for (slot, groups_x, groups_y) in segments {
            pass.set_bind_group(0, &stroke.bind_group, &[slot * UNIFORM_STRIDE as u32]);
            pass.dispatch_workgroups(groups_x, groups_y, 1);
        }
    }

    /// Encode and submit the deferred segments on their own.
    pub fn flush(&mut self) {
        if self.pending_segments.is_empty() {
            return;
        }
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.encode(&mut encoder);
        self.queue.submit(Some(encoder.finish()));
    }

    /// Replay mode: batch the whole stroke into one submission. A tape restart is thousands of
    /// segments, and one submission per segment costs more than the coverage itself.
    pub fn set_batching(&mut self, on: bool) {
        if on == self.batch {
            return;
        }
        self.batch = on;
        if !on {
            self.flush();
        }
    }
}

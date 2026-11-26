//! Buffers a stroke needs, kept between strokes. A replay starts thousands of them, and allocating
//! a coverage buffer, a uniform ring and their bind groups for each one costs more than the dabs.

use super::clear::ClearParams;
use super::*;

/// One size of coverage buffer and everything bound to it. A tip/paper map change bumps the epoch
/// because the bind group references the map view.
pub(super) struct StrokeMemory {
    buffer: wgpu::Buffer,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The rectangle-clear pass binds the same coverage buffer every stroke; only its parameters
    /// change, so the uniform buffer and bind group outlive the stroke.
    pub(super) clear_uniform: wgpu::Buffer,
    pub(super) clear_bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    map_epoch: u64,
}

impl StrokePreview {
    /// The coverage buffer, uniform ring and bind group for this size, reusing them when neither
    /// the size nor the tip map changed.
    pub(super) fn stroke_memory(
        &mut self,
        width: u32,
        height: u32,
    ) -> (wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup) {
        let reuse = self
            .memory
            .as_ref()
            .filter(|memory| {
                memory.width == width
                    && memory.height == height
                    && memory.map_epoch == self.map_epoch
            })
            .map(|memory| {
                (
                    memory.buffer.clone(),
                    memory.uniforms.clone(),
                    memory.bind_group.clone(),
                )
            });
        if let Some(found) = reuse {
            return found;
        }

        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stroke coverage"),
            size: width as u64 * height as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.map_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let clear_uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("coverage clear params"),
            size: std::mem::size_of::<ClearParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let clear_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("coverage clear"),
            layout: &self.clear_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: clear_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        self.memory = Some(StrokeMemory {
            buffer: buffer.clone(),
            uniforms: uniforms.clone(),
            bind_group: bind_group.clone(),
            clear_uniform,
            clear_bind_group,
            width,
            height,
            map_epoch: self.map_epoch,
        });
        (buffer, uniforms, bind_group)
    }
}

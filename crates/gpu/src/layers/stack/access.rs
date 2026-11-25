//! Reads and texture swaps shared by the window surface and the offscreen compositor.

use super::*;

impl LayerStack {
    /// Read one pixel of the last composed canvas, premultiplied RGBA8.
    pub fn read_output(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        x: u32,
        y: u32,
    ) -> Option<[u8; 4]> {
        let output = self.output.as_ref()?;
        if x >= self.width || y >= self.height {
            return None;
        }
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canvas pick"),
            size: 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: output,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let mapped = staging.slice(..).get_mapped_range().ok()?;
        let pixel = [mapped[0], mapped[1], mapped[2], mapped[3]];
        drop(mapped);
        staging.unmap();
        Some(pixel)
    }
}

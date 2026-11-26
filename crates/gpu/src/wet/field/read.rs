//! Reading the field back. Only tests, a bake and the prototype report use this; the live path
//! keeps the state on the GPU.

use half::f16;

use super::WetField;

impl WetField {
    /// Both halves of the current state, row-major in field texels. For tests, a bake and the
    /// prototype report; the live path never reads the field back.
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

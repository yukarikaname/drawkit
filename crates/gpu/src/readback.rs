//! Copy a rendered texture back to RGBA8. The row alignment of a texture copy is not the row
//! alignment a caller wants, so the padding is stripped here once instead of in every renderer.

use wgpu::{Device, Queue, Texture};

pub enum FramePixels {
    Rgba8(Vec<u8>),
    Linear(Vec<f32>),
}

/// Copy a texture back in the format the resident stack uses.
pub fn read_frame(
    device: &Device,
    queue: &Queue,
    texture: &Texture,
    width: u32,
    height: u32,
    format: crate::FrameFormat,
) -> FramePixels {
    if width == 0 || height == 0 {
        return match format {
            crate::FrameFormat::Rgba8 => FramePixels::Rgba8(Vec::new()),
            crate::FrameFormat::Rgba16 => FramePixels::Linear(Vec::new()),
        };
    }
    let bytes = format.bytes_per_pixel();
    let unpadded = format.bytes_per_pixel() * width as usize;
    let padded = unpadded.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * height as usize) as u64,
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
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let mapped = buffer.slice(..).get_mapped_range().expect("readback map");
    match format {
        crate::FrameFormat::Rgba8 => {
            let mut pixels = Vec::with_capacity(unpadded * height as usize);
            for row in 0..height as usize {
                let start = row * padded;
                pixels.extend_from_slice(&mapped[start..start + unpadded]);
            }
            FramePixels::Rgba8(pixels)
        }
        crate::FrameFormat::Rgba16 => {
            let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
            for row in 0..height as usize {
                let start = row * padded;
                for column in 0..width as usize {
                    for channel in 0..4 {
                        let at = start + column * bytes + channel * 2;
                        let bits = u16::from_le_bytes([mapped[at], mapped[at + 1]]);
                        pixels.push(half::f16::from_bits(bits).to_f32());
                    }
                }
            }
            FramePixels::Linear(pixels)
        }
    }
}

/// Copy a texture straight into the caller's buffer. A scrub asks for one frame per pointer move,
/// and a fresh frame-sized allocation each time is a copy the caller does not need.
pub fn read_texture_into(
    device: &Device,
    queue: &Queue,
    texture: &Texture,
    width: u32,
    height: u32,
    out: &mut [u8],
) -> bool {
    let unpadded = width as usize * 4;
    if width == 0 || height == 0 || out.len() < unpadded * height as usize {
        return false;
    }
    match read_frame(
        device,
        queue,
        texture,
        width,
        height,
        crate::FrameFormat::Rgba8,
    ) {
        FramePixels::Rgba8(pixels) => {
            let count = unpadded * height as usize;
            out[..count].copy_from_slice(&pixels[..count]);
            true
        }
        FramePixels::Linear(_) => false,
    }
}

pub fn read_texture(
    device: &Device,
    queue: &Queue,
    texture: &Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    match read_frame(
        device,
        queue,
        texture,
        width,
        height,
        crate::FrameFormat::Rgba8,
    ) {
        FramePixels::Rgba8(pixels) => pixels,
        FramePixels::Linear(_) => Vec::new(),
    }
}

pub fn read_texture_linear(
    device: &Device,
    queue: &Queue,
    texture: &Texture,
    width: u32,
    height: u32,
) -> Vec<f32> {
    match read_frame(
        device,
        queue,
        texture,
        width,
        height,
        crate::FrameFormat::Rgba16,
    ) {
        FramePixels::Linear(pixels) => pixels,
        FramePixels::Rgba8(_) => Vec::new(),
    }
}

/// Copy a storage buffer back to bytes.
pub fn read_buffer(device: &Device, queue: &Queue, buffer: &wgpu::Buffer, size: u64) -> Vec<u8> {
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("buffer readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(buffer, 0, &readback, 0, size);
    queue.submit(Some(encoder.finish()));
    readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let mapped = readback.slice(..).get_mapped_range().expect("buffer map");
    let bytes = mapped.to_vec();
    drop(mapped);
    readback.unmap();
    bytes
}

/// A texture copy that is in flight. The shell can submit it after a frame and look at it on a
/// later frame; only export, a thumbnail or an eyedropper should ever wait for one.
pub struct PendingReadback {
    buffer: wgpu::Buffer,
    ready: std::sync::Arc<std::sync::atomic::AtomicBool>,
    width: u32,
    height: u32,
    padded: usize,
}

impl PendingReadback {
    pub fn submit(
        device: &Device,
        queue: &Queue,
        texture: &Texture,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let unpadded = (width * 4) as usize;
        let padded = unpadded.div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pending readback"),
            size: (padded * height as usize) as u64,
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
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        let ready = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = ready.clone();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |_| {
            flag.store(true, std::sync::atomic::Ordering::Release);
        });
        Some(Self {
            buffer,
            ready,
            width,
            height,
            padded,
        })
    }

    /// True once the copy has completed and the bytes can be read without a wait.
    pub fn ready(&self, device: &Device) -> bool {
        let _ = device.poll(wgpu::PollType::Poll);
        self.ready.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Block until the copy is done and return unpadded RGBA rows.
    pub fn finish(self, device: &Device) -> Vec<u8> {
        if !self.ready.load(std::sync::atomic::Ordering::Acquire) {
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }
        let mapped = self
            .buffer
            .slice(..)
            .get_mapped_range()
            .expect("readback map");
        let unpadded = (self.width * 4) as usize;
        let mut pixels = Vec::with_capacity(unpadded * self.height as usize);
        for row in 0..self.height as usize {
            let start = row * self.padded;
            pixels.extend_from_slice(&mapped[start..start + unpadded]);
        }
        drop(mapped);
        self.buffer.unmap();
        pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pending_readback_delivers_the_texture() {
        let Some(renderer) = crate::Renderer::new() else {
            return;
        };
        let device = renderer.device();
        let queue = renderer.queue();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("readback source"),
            size: wgpu::Extent3d {
                width: 3,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bytes: Vec<u8> = (0..24).collect();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(12),
                rows_per_image: Some(2),
            },
            wgpu::Extent3d {
                width: 3,
                height: 2,
                depth_or_array_layers: 1,
            },
        );
        let pending = PendingReadback::submit(device, queue, &texture, 3, 2).expect("a copy");
        assert_eq!(pending.finish(device), bytes);
    }
}

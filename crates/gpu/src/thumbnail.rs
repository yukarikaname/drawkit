//! Downsample an RGBA8 layer to a small preview with a GPU box filter.
//!
//! The palette asks for one of these per layer after a stroke. On the CPU that is a walk over the
//! layer; on the GPU it is one thread per preview pixel, and the average is taken premultiplied so
//! the empty part of a layer does not darken the colour that is there.

use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    width: u32,
    height: u32,
    out_width: u32,
    out_height: u32,
}

pub fn pipeline(device: &wgpu::Device) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("thumbnail"),
        source: wgpu::ShaderSource::Wgsl(include_str!("thumbnail.wgsl").into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("thumbnail"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

pub struct Request<'a> {
    /// Straight RGBA8.
    pub pixels: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub out_width: u32,
    pub out_height: u32,
}

/// The result is premultiplied RGBA in `0..=1`.
pub fn run(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    request: Request<'_>,
) -> Vec<f32> {
    let Request {
        pixels,
        width,
        height,
        out_width,
        out_height,
    } = request;
    let expected = width as usize * height as usize * 4;
    if width == 0
        || height == 0
        || out_width == 0
        || out_height == 0
        || out_width > 512
        || out_height > 512
        || pixels.len() < expected
    {
        return Vec::new();
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("thumbnail source"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels[..expected],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&Default::default());
    run_view(
        device,
        queue,
        pipeline,
        ViewRequest {
            view: &view,
            width,
            height,
            out_width,
            out_height,
        },
    )
}

/// A texture that is already on the GPU, which is what a resident layer stack holds.
pub struct ViewRequest<'a> {
    pub view: &'a wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub out_width: u32,
    pub out_height: u32,
}

pub fn run_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    request: ViewRequest<'_>,
) -> Vec<f32> {
    let ViewRequest {
        view,
        width,
        height,
        out_width,
        out_height,
    } = request;
    if out_width == 0 || out_height == 0 {
        return Vec::new();
    }
    let params = Params {
        width,
        height,
        out_width,
        out_height,
    };
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("thumbnail params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let count = out_width as usize * out_height as usize * 4;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("thumbnail output"),
        size: (count * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("thumbnail"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("thumbnail"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(out_width.div_ceil(8), out_height.div_ceil(8), 1);
    }
    queue.submit(Some(encoder.finish()));
    let bytes = crate::readback::read_buffer(device, queue, &output, (count * 4) as u64);
    bytemuck::cast_slice(&bytes).to_vec()
}

#[cfg(test)]
mod tests {
    use crate::Renderer;

    fn cpu(
        pixels: &[u8],
        width: usize,
        height: usize,
        out_width: usize,
        out_height: usize,
    ) -> Vec<f32> {
        let mut result = Vec::with_capacity(out_width * out_height * 4);
        for oy in 0..out_height {
            let top = oy * height / out_height;
            let bottom = ((oy + 1) * height / out_height).max(top + 1);
            for ox in 0..out_width {
                let left = ox * width / out_width;
                let right = ((ox + 1) * width / out_width).max(left + 1);
                let mut sum = [0.0f32; 4];
                let mut count: f32 = 0.0;
                for y in top..bottom {
                    for x in left..right {
                        let index = (y * width + x) * 4;
                        let alpha = pixels[index + 3] as f32 / 255.0;
                        sum[0] += pixels[index] as f32 / 255.0 * alpha;
                        sum[1] += pixels[index + 1] as f32 / 255.0 * alpha;
                        sum[2] += pixels[index + 2] as f32 / 255.0 * alpha;
                        sum[3] += alpha;
                        count += 1.0;
                    }
                }
                for value in sum {
                    result.push(value / count.max(1.0));
                }
            }
        }
        result
    }

    #[test]
    fn the_gpu_thumbnail_matches_a_box_filter() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let (width, height) = (12, 9);
        let mut pixels = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) * 4;
                pixels[index] = (x * 17) as u8;
                pixels[index + 1] = (y * 23) as u8;
                pixels[index + 2] = 128;
                pixels[index + 3] = if x < 6 { 255 } else { 64 };
            }
        }
        let gpu = renderer.thumbnail(&pixels, width as u32, height as u32, 4, 3);
        let expected = cpu(&pixels, width, height, 4, 3);
        assert_eq!(gpu.len(), expected.len());
        for (index, (left, right)) in gpu.iter().zip(expected.iter()).enumerate() {
            assert!(
                (left - right).abs() < 1e-3,
                "sample {index}: gpu {left}, cpu {right}"
            );
        }
    }
}

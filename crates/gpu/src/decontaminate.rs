//! GPU colour decontamination, the operation that removes the old background from a cut-out's
//! anti-aliased edge. It is one thread per pixel and one arithmetic expression; the value is that
//! it scales to a full layer without a CPU loop.

use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    background: [f32; 4],
}

pub fn pipeline(device: &wgpu::Device) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("decontaminate"),
        source: wgpu::ShaderSource::Wgsl(include_str!("decontaminate.wgsl").into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("decontaminate"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

pub struct Request<'a> {
    pub pixels: &'a [u8],
    pub alpha: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub background: [f32; 3],
}

pub fn run(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    request: Request<'_>,
) -> Vec<u8> {
    let Request {
        pixels,
        alpha,
        width,
        height,
        background,
    } = request;
    let count = width as usize * height as usize;
    if width == 0 || height == 0 || pixels.len() < count * 4 || alpha.len() < count {
        return Vec::new();
    }
    let packed = bytemuck::cast_slice::<u8, u32>(&pixels[..count * 4]).to_vec();
    let mask: Vec<u32> = alpha[..count]
        .iter()
        .map(|value| u32::from(*value))
        .collect();
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("decontaminate pixels"),
        contents: bytemuck::cast_slice(&packed),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    let mask_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("decontaminate alpha"),
        contents: bytemuck::cast_slice(&mask),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("decontaminate output"),
        size: (count * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let params = Params {
        background: [background[0], background[1], background[2], 1.0],
    };
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("decontaminate params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("decontaminate"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: mask_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("decontaminate"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups((count as u32).div_ceil(64), 1, 1);
    }
    queue.submit(Some(encoder.finish()));
    crate::readback::read_buffer(device, queue, &output, (count * 4) as u64)
}

#[cfg(test)]
mod tests {
    use crate::Renderer;
    use drawkit_core::color::decontaminate;

    #[test]
    fn gpu_decontamination_matches_the_core() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let background = [1.0, 1.0, 1.0];
        let width = 4;
        let height = 1;
        let mut pixels = Vec::new();
        let mut alpha = Vec::new();
        for index in 0..width * height {
            let a = (index as u8 * 37).wrapping_add(12);
            pixels.extend_from_slice(&[200, 30, 90]);
            pixels.push(255);
            alpha.push(a);
        }
        let gpu = renderer.decontaminate(&pixels, &alpha, width, height, background);
        assert_eq!(gpu.len(), pixels.len());
        for index in 0..(width * height) as usize {
            let a = alpha[index] as f32 / 255.0;
            let cpu = decontaminate(
                [
                    pixels[index * 4] as f32 / 255.0,
                    pixels[index * 4 + 1] as f32 / 255.0,
                    pixels[index * 4 + 2] as f32 / 255.0,
                    1.0,
                ],
                [background[0], background[1], background[2], 1.0],
                a,
            );
            let expected = [
                (cpu[0] * 255.0).round() as i32,
                (cpu[1] * 255.0).round() as i32,
                (cpu[2] * 255.0).round() as i32,
                (a * 255.0).round() as i32,
            ];
            for channel in 0..4 {
                let actual = gpu[index * 4 + channel] as i32;
                assert!(
                    (actual - expected[channel]).abs() <= 2,
                    "pixel {index} channel {channel}: gpu {actual}, cpu {}",
                    expected[channel]
                );
            }
        }
    }
}

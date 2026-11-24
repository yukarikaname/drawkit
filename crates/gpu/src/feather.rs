//! Separable box blur for mask feathering, on the GPU.

use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    width: u32,
    height: u32,
    radius: i32,
    direction: u32,
}

pub fn pipeline(device: &wgpu::Device) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("feather"),
        source: wgpu::ShaderSource::Wgsl(include_str!("feather.wgsl").into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("feather"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

fn buffer(
    device: &wgpu::Device,
    label: &str,
    size: usize,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (size * 4).max(4) as u64,
        usage,
        mapped_at_creation: false,
    })
}

fn encode(
    device: &wgpu::Device,
    pipeline: &wgpu::ComputePipeline,
    encoder: &mut wgpu::CommandEncoder,
    input: &wgpu::Buffer,
    output: &wgpu::Buffer,
    params: Params,
    keep: &mut Vec<wgpu::Buffer>,
) {
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("feather params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("feather"),
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
                resource: output.as_entire_binding(),
            },
        ],
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("feather"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(params.width.div_ceil(8), params.height.div_ceil(8), 1);
    }
    keep.push(uniform);
}

pub fn run(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    mask: &[f32],
    width: u32,
    height: u32,
    radius: u32,
) -> Vec<f32> {
    let pixels = width as usize * height as usize;
    if width == 0 || height == 0 || mask.len() < pixels || radius > 64 {
        return Vec::new();
    }
    if radius == 0 {
        return mask[..pixels].to_vec();
    }
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("feather input"),
        contents: bytemuck::cast_slice(&mask[..pixels]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    let middle = buffer(
        device,
        "feather middle",
        pixels,
        wgpu::BufferUsages::STORAGE,
    );
    let output = buffer(
        device,
        "feather output",
        pixels,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let horizontal = Params {
        width,
        height,
        radius: radius as i32,
        direction: 0,
    };
    let vertical = Params {
        width,
        height,
        radius: radius as i32,
        direction: 1,
    };
    let mut encoder = device.create_command_encoder(&Default::default());
    let mut keep = Vec::new();
    encode(
        device,
        pipeline,
        &mut encoder,
        &input,
        &middle,
        horizontal,
        &mut keep,
    );
    encode(
        device,
        pipeline,
        &mut encoder,
        &middle,
        &output,
        vertical,
        &mut keep,
    );
    queue.submit(Some(encoder.finish()));
    let bytes = crate::readback::read_buffer(device, queue, &output, (pixels * 4) as u64);
    drop(keep);
    bytemuck::cast_slice(&bytes).to_vec()
}

#[cfg(test)]
mod tests {
    use crate::Renderer;

    fn cpu(mask: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
        let mut horizontal = vec![0.0; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut sum = 0.0;
                let mut count = 0.0;
                for step in -(radius as i32)..=radius as i32 {
                    let nx = (x as i32 + step).clamp(0, width as i32 - 1) as usize;
                    sum += mask[y * width + nx];
                    count += 1.0;
                }
                horizontal[y * width + x] = sum / count;
            }
        }
        let mut out = vec![0.0; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut sum = 0.0;
                let mut count = 0.0;
                for step in -(radius as i32)..=radius as i32 {
                    let ny = (y as i32 + step).clamp(0, height as i32 - 1) as usize;
                    sum += horizontal[ny * width + x];
                    count += 1.0;
                }
                out[y * width + x] = sum / count;
            }
        }
        out
    }

    #[test]
    fn feather_matches_a_cpu_box_blur() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mut mask = vec![0.0; 10 * 10];
        for y in 3..7 {
            for x in 4..6 {
                mask[y * 10 + x] = 1.0;
            }
        }
        let gpu = renderer.feather(&mask, 10, 10, 2);
        let expected = cpu(&mask, 10, 10, 2);
        for (index, value) in expected.iter().enumerate() {
            assert!(
                (gpu[index] - value).abs() < 1e-5,
                "pixel {index}: gpu {}, cpu {value}",
                gpu[index]
            );
        }
    }

    #[test]
    fn a_zero_radius_is_a_copy() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mask = vec![0.2; 16];
        assert_eq!(renderer.feather(&mask, 4, 4, 0), mask);
    }
}

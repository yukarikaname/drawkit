//! Grow and erode a coverage mask on the GPU.
//!
//! The flood fill itself is sequential and stays on the CPU for now. Everything around it is a
//! neighbourhood reduction, which is exactly what a compute shader is good at, and at 4K the CPU
//! versions stop being free.

use wgpu::util::DeviceExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Morphology {
    Grow,
    Erode,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    width: u32,
    height: u32,
    radius: i32,
    grow: u32,
    /// 0 is the horizontal pass, 1 the vertical one.
    axis: u32,
    padding: [u32; 3],
}

pub fn pipeline(device: &wgpu::Device) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("morphology"),
        source: wgpu::ShaderSource::Wgsl(include_str!("morphology.wgsl").into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("morphology"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

pub struct Request<'a> {
    pub mask: &'a [f32],
    pub width: u32,
    pub height: u32,
    pub radius: u32,
    pub operation: Morphology,
}

fn storage_buffer(
    device: &wgpu::Device,
    label: &str,
    size: usize,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(4) as u64,
        usage,
        mapped_at_creation: false,
    })
}

fn encode_pass(
    device: &wgpu::Device,
    pipeline: &wgpu::ComputePipeline,
    encoder: &mut wgpu::CommandEncoder,
    input: &wgpu::Buffer,
    output: &wgpu::Buffer,
    params: Params,
    keep: &mut Vec<wgpu::Buffer>,
) {
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("morphology params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("morphology"),
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
            label: Some("morphology"),
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
    request: Request<'_>,
) -> Vec<f32> {
    let Request {
        mask,
        width,
        height,
        radius,
        operation,
    } = request;
    let pixels = width as usize * height as usize;
    if width == 0 || height == 0 || mask.len() < pixels || radius > 64 {
        return Vec::new();
    }
    let horizontal = Params {
        width,
        height,
        radius: radius as i32,
        grow: u32::from(operation == Morphology::Grow),
        axis: 0,
        padding: [0; 3],
    };
    let vertical = Params {
        axis: 1,
        ..horizontal
    };
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("morphology input"),
        contents: bytemuck::cast_slice(mask),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    let middle = storage_buffer(
        device,
        "morphology middle",
        pixels * 4,
        wgpu::BufferUsages::STORAGE,
    );
    let output = storage_buffer(
        device,
        "morphology output",
        pixels * 4,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let mut encoder = device.create_command_encoder(&Default::default());
    let mut keep = Vec::new();
    encode_pass(
        device,
        pipeline,
        &mut encoder,
        &input,
        &middle,
        horizontal,
        &mut keep,
    );
    encode_pass(
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

/// Grow then erode in one submission: closing welds narrow gaps in a mask. There is no readback
/// between the two passes; the intermediate stays in a storage buffer.
pub fn close(
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
    let grow_x = Params {
        width,
        height,
        radius: radius as i32,
        grow: 1,
        axis: 0,
        padding: [0; 3],
    };
    let grow_y = Params { axis: 1, ..grow_x };
    let erode_x = Params { grow: 0, ..grow_x };
    let erode_y = Params { axis: 1, ..erode_x };
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("close input"),
        contents: bytemuck::cast_slice(mask),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    let middle = storage_buffer(
        device,
        "close middle",
        pixels * 4,
        wgpu::BufferUsages::STORAGE,
    );
    let grown = storage_buffer(
        device,
        "close grown",
        pixels * 4,
        wgpu::BufferUsages::STORAGE,
    );
    let output = storage_buffer(
        device,
        "close output",
        pixels * 4,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let mut encoder = device.create_command_encoder(&Default::default());
    let mut keep = Vec::new();
    encode_pass(
        device,
        pipeline,
        &mut encoder,
        &input,
        &middle,
        grow_x,
        &mut keep,
    );
    encode_pass(
        device,
        pipeline,
        &mut encoder,
        &middle,
        &grown,
        grow_y,
        &mut keep,
    );
    encode_pass(
        device,
        pipeline,
        &mut encoder,
        &grown,
        &middle,
        erode_x,
        &mut keep,
    );
    encode_pass(
        device,
        pipeline,
        &mut encoder,
        &middle,
        &output,
        erode_y,
        &mut keep,
    );
    queue.submit(Some(encoder.finish()));
    let bytes = crate::readback::read_buffer(device, queue, &output, (pixels * 4) as u64);
    drop(keep);
    bytemuck::cast_slice(&bytes).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;

    fn cpu(
        mask: &[f32],
        width: usize,
        height: usize,
        radius: usize,
        operation: Morphology,
    ) -> Vec<f32> {
        let grow = operation == Morphology::Grow;
        let mut out = vec![if grow { 0.0 } else { 1.0 }; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut value: f32 = if grow { 0.0 } else { 1.0 };
                for dy in -(radius as i32)..=radius as i32 {
                    for dx in -(radius as i32)..=radius as i32 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                            if !grow {
                                value = 0.0;
                            }
                            continue;
                        }
                        let sample = mask[ny as usize * width + nx as usize];
                        value = if grow {
                            value.max(sample)
                        } else {
                            value.min(sample)
                        };
                    }
                }
                out[y * width + x] = value;
            }
        }
        out
    }

    #[test]
    fn grow_matches_the_cpu_mask() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mut mask = vec![0.0; 8 * 8];
        mask[4 * 8 + 4] = 1.0;
        let gpu = renderer.morphology(&mask, 8, 8, 1, Morphology::Grow);
        assert_eq!(gpu, cpu(&mask, 8, 8, 1, Morphology::Grow));
    }

    #[test]
    fn erode_matches_the_cpu_mask() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mut mask = vec![0.0; 8 * 8];
        for y in 3..6 {
            for x in 3..6 {
                mask[y * 8 + x] = 1.0;
            }
        }
        let gpu = renderer.morphology(&mask, 8, 8, 1, Morphology::Erode);
        assert_eq!(gpu, cpu(&mask, 8, 8, 1, Morphology::Erode));
    }

    #[test]
    fn a_larger_radius_reaches_further() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mut mask = vec![0.0; 16 * 16];
        mask[8 * 16 + 8] = 1.0;
        let gpu = renderer.morphology(&mask, 16, 16, 3, Morphology::Grow);
        assert_eq!(gpu[8 * 16 + 11], 1.0);
        assert_eq!(gpu[8 * 16 + 12], 0.0);
    }
}

#[cfg(test)]
mod parity {
    use super::*;
    use crate::Renderer;
    use drawkit_core::fill::{Mask, erode, grow};

    #[test]
    fn gpu_grow_matches_the_core_mask() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mut mask = Mask::new(8, 8);
        mask.set(4, 4, 1.0);
        mask.set(1, 6, 0.5);
        let cpu = grow(&mask, 2);
        let gpu = renderer.morphology(&mask.data, 8, 8, 2, Morphology::Grow);
        for (index, value) in cpu.data.iter().enumerate() {
            assert!(
                (gpu[index] - value).abs() < 1e-6,
                "pixel {index}: gpu {}, cpu {value}",
                gpu[index]
            );
        }
    }

    #[test]
    fn a_large_radius_still_matches_the_core() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mut mask = Mask::new(20, 20);
        for y in 6..12 {
            for x in 5..15 {
                mask.set(x, y, 1.0);
            }
        }
        mask.set(9, 9, 0.0);
        for radius in [3, 5, 8] {
            let cpu = grow(&mask, radius);
            let gpu = renderer.morphology(&mask.data, 20, 20, radius as u32, Morphology::Grow);
            assert_eq!(gpu, cpu.data, "grow radius {radius}");
            let cpu = erode(&mask, radius);
            let gpu = renderer.morphology(&mask.data, 20, 20, radius as u32, Morphology::Erode);
            assert_eq!(gpu, cpu.data, "erode radius {radius}");
        }
    }

    #[test]
    fn gpu_close_matches_grow_then_erode() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mut mask = Mask::new(12, 12);
        for y in 2..10 {
            mask.set(6, y, 1.0);
        }
        mask.set(6, 5, 0.0);
        let cpu = erode(&grow(&mask, 1), 1);
        let gpu = renderer.close(&mask.data, 12, 12, 1);
        for (index, value) in cpu.data.iter().enumerate() {
            assert!(
                (gpu[index] - value).abs() < 1e-6,
                "pixel {index}: gpu {}, cpu {value}",
                gpu[index]
            );
        }
    }

    #[test]
    fn gpu_erode_matches_the_core_mask() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let mut mask = Mask::new(8, 8);
        for y in 2..7 {
            for x in 2..7 {
                mask.set(x, y, 1.0);
            }
        }
        let cpu = erode(&mask, 1);
        let gpu = renderer.morphology(&mask.data, 8, 8, 1, Morphology::Erode);
        for (index, value) in cpu.data.iter().enumerate() {
            assert!(
                (gpu[index] - value).abs() < 1e-6,
                "pixel {index}: gpu {}, cpu {value}",
                gpu[index]
            );
        }
    }
}

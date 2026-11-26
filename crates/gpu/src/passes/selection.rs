//! A lasso polygon as a coverage mask.
//!
//! The points arrive once, already sampled by the shell; the per-pixel even-odd test runs on the
//! GPU. `grow`, `close` and `feather` then operate on the mask without another CPU pass.

use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    width: u32,
    height: u32,
    count: u32,
    padding: u32,
}

pub fn pipeline(device: &wgpu::Device) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("selection"),
        source: wgpu::ShaderSource::Wgsl(include_str!("selection.wgsl").into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("selection"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

pub fn lasso(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    points: &[[f32; 2]],
    width: u32,
    height: u32,
) -> Vec<f32> {
    if width == 0 || height == 0 || points.len() < 3 || width > 16_384 || height > 16_384 {
        return Vec::new();
    }
    let params = Params {
        width,
        height,
        count: points.len() as u32,
        padding: 0,
    };
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("selection params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let points = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("selection points"),
        contents: bytemuck::cast_slice(points),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let count = width as usize * height as usize;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("selection output"),
        size: (count * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("selection"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: points.as_entire_binding(),
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
            label: Some("selection"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
    queue.submit(Some(encoder.finish()));
    let bytes = crate::readback::read_buffer(device, queue, &output, (count * 4) as u64);
    bytemuck::cast_slice(&bytes).to_vec()
}

#[cfg(test)]
mod tests {
    use crate::Renderer;

    fn cpu(points: &[[f32; 2]], width: usize, height: usize) -> Vec<f32> {
        let mut mask = vec![0.0; width * height];
        for y in 0..height {
            for x in 0..width {
                let point = [x as f32 + 0.5, y as f32 + 0.5];
                let mut inside = false;
                for index in 0..points.len() {
                    let a = points[index];
                    let b = points[(index + 1) % points.len()];
                    if (a[1] > point[1]) != (b[1] > point[1]) {
                        let cross = (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0];
                        if point[0] < cross {
                            inside = !inside;
                        }
                    }
                }
                mask[y * width + x] = f32::from(inside);
            }
        }
        mask
    }

    #[test]
    fn a_lasso_matches_the_even_odd_fill() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let points = [[2.0, 2.0], [13.0, 4.0], [10.0, 14.0], [3.0, 11.0]];
        let gpu = renderer.lasso(&points, 16, 16);
        let expected = cpu(&points, 16, 16);
        assert_eq!(gpu, expected);
        assert!(
            gpu[8 * 16 + 8] > 0.5,
            "the middle of the polygon is outside"
        );
        assert!(gpu[0] < 0.5);
    }

    #[test]
    fn grow_and_feather_take_the_lasso_mask() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let points = [[4.0, 4.0], [10.0, 4.0], [10.0, 10.0], [4.0, 10.0]];
        let mask = renderer.lasso(&points, 16, 16);
        let grown = renderer.morphology(&mask, 16, 16, 1, crate::Morphology::Grow);
        assert_eq!(grown[3 * 16 + 3], 1.0);
        let feathered = renderer.feather(&mask, 16, 16, 1);
        assert!(feathered[3 * 16 + 3] > 0.0 && feathered[3 * 16 + 3] < 1.0);
    }
}

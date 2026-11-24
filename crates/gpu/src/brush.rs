//! Brush segment rasterisation on the GPU.
//!
//! `Segment` describes one segment exactly as the core records it; the shader stamps the same discs
//! at the same spacing and composites each pixel once, so overlapping stamps cannot darken the rim.

use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    width: u32,
    height: u32,
    min_x: u32,
    min_y: u32,
    mask_w: u32,
    mask_h: u32,
    erase: u32,
    steps: u32,
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    radius: f32,
    hardness: f32,
    pad0: f32,
    pad1: f32,
    color: [f32; 4],
}

pub struct Segment<'a> {
    /// Straight RGBA, one `f32` per channel, row-major.
    pub layer: &'a [f32],
    pub width: u32,
    pub height: u32,
    pub from: (f32, f32),
    pub to: (f32, f32),
    pub radius: f32,
    pub hardness: f32,
    /// Straight RGBA; ignored when `erase` is true.
    pub color: [f32; 4],
    pub erase: bool,
}

pub fn pipeline(device: &wgpu::Device) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("brush"),
        source: wgpu::ShaderSource::Wgsl(include_str!("brush.wgsl").into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("brush"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

/// The bounding box of the stamps, clipped to the canvas.
fn bounds(width: u32, height: u32, segment: &Segment<'_>) -> Option<(u32, u32, u32, u32)> {
    if segment.radius <= 0.0 || width == 0 || height == 0 {
        return None;
    }
    let min_x = (segment.from.0.min(segment.to.0) - segment.radius)
        .floor()
        .max(0.0) as u32;
    let min_y = (segment.from.1.min(segment.to.1) - segment.radius)
        .floor()
        .max(0.0) as u32;
    let max_x = ((segment.from.0.max(segment.to.0) + segment.radius)
        .ceil()
        .max(0.0) as u32)
        .min(width - 1);
    let max_y = ((segment.from.1.max(segment.to.1) + segment.radius)
        .ceil()
        .max(0.0) as u32)
        .min(height - 1);
    if min_x > max_x || min_y > max_y {
        return None;
    }
    Some((min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
}

pub fn run(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    segment: Segment<'_>,
) -> Vec<f32> {
    let count = segment.width as usize * segment.height as usize * 4;
    if segment.layer.len() < count {
        return Vec::new();
    }
    let Some((min_x, min_y, mask_w, mask_h)) = bounds(segment.width, segment.height, &segment)
    else {
        return segment.layer[..count].to_vec();
    };
    let dx = segment.to.0 - segment.from.0;
    let dy = segment.to.1 - segment.from.1;
    let distance = (dx * dx + dy * dy).sqrt();
    let spacing = (segment.radius * 0.25).max(0.5);
    let steps = (distance / spacing).ceil() as u32;

    let params = Params {
        width: segment.width,
        height: segment.height,
        min_x,
        min_y,
        mask_w,
        mask_h,
        erase: u32::from(segment.erase),
        steps,
        from_x: segment.from.0,
        from_y: segment.from.1,
        to_x: segment.to.0,
        to_y: segment.to.1,
        radius: segment.radius,
        hardness: segment.hardness,
        pad0: 0.0,
        pad1: 0.0,
        color: segment.color,
    };
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("brush params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("brush layer"),
        contents: bytemuck::cast_slice(&segment.layer[..count]),
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
    });
    let layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("brush"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: buffer.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("brush"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(mask_w.div_ceil(8), mask_h.div_ceil(8), 1);
    }
    queue.submit(Some(encoder.finish()));
    let bytes = crate::readback::read_buffer(device, queue, &buffer, (count * 4) as u64);
    bytemuck::cast_slice(&bytes).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;
    use drawkit_core::brush;
    use drawkit_core::surface::Surface;

    fn flatten(surface: &Surface) -> Vec<f32> {
        let mut out = Vec::with_capacity(surface.width() * surface.height() * 4);
        for y in 0..surface.height() {
            for x in 0..surface.width() {
                out.extend_from_slice(&surface.get(x, y));
            }
        }
        out
    }

    #[test]
    fn one_segment_matches_the_core_brush() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let (width, height) = (64usize, 48usize);
        let mut cpu = Surface::new(width, height);
        cpu.set(10, 10, [0.2, 0.3, 0.4, 0.9]);
        let mut gpu_surface = Surface::new(width, height);
        gpu_surface.set(10, 10, [0.2, 0.3, 0.4, 0.9]);
        let color = [0.8, 0.1, 0.2, 0.7];
        let (from, to) = ((8.0, 20.0), (52.0, 28.0));
        brush::stroke(&mut cpu, from, to, 5.0, 0.6, color);

        let gpu = renderer.brush(Segment {
            layer: &flatten(&gpu_surface),
            width: width as u32,
            height: height as u32,
            from,
            to,
            radius: 5.0,
            hardness: 0.6,
            color,
            erase: false,
        });
        for y in 0..height {
            for x in 0..width {
                for channel in 0..4 {
                    let index = (y * width + x) * 4 + channel;
                    assert!(
                        (gpu[index] - cpu.get(x, y)[channel]).abs() < 1e-3,
                        "pixel {x},{y} channel {channel}: {} vs {}",
                        gpu[index],
                        cpu.get(x, y)[channel]
                    );
                }
            }
        }
    }

    #[test]
    fn erasing_a_segment_matches_the_core_eraser() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let (width, height) = (32usize, 32usize);
        let mut cpu = Surface::new(width, height);
        for y in 10..22 {
            for x in 8..26 {
                cpu.set(x, y, [0.4, 0.2, 0.7, 1.0]);
            }
        }
        let gpu = renderer.brush(Segment {
            layer: &flatten(&cpu),
            width: width as u32,
            height: height as u32,
            from: (10.0, 16.0),
            to: (24.0, 16.0),
            radius: 4.0,
            hardness: 0.8,
            color: [0.0; 4],
            erase: true,
        });
        brush::erase(&mut cpu, (10.0, 16.0), (24.0, 16.0), 4.0, 0.8);
        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) * 4;
                let alpha = cpu.get(x, y)[3];
                assert!(
                    (gpu[index + 3] - alpha).abs() < 1e-3,
                    "pixel {x},{y}: gpu alpha {} vs cpu {alpha}",
                    gpu[index + 3]
                );
            }
        }
    }
}

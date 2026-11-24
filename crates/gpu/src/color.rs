//! Colour conversion on the GPU: premultiply, and encoded working RGB to linear Display P3.
//!
//! The CPU does this per pixel for an HDR composite or an EXR export. A shader can do it for a
//! whole buffer in one pass, with the same transfer case and matrix the core uses.

use wgpu::util::DeviceExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Premultiply,
    Unpremultiply,
    /// Encoded working RGB to premultiplied linear Display P3.
    ToLinearP3,
}

impl Operation {
    fn as_u32(self) -> u32 {
        match self {
            Self::Premultiply => 0,
            Self::Unpremultiply => 1,
            Self::ToLinearP3 => 2,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    width: u32,
    height: u32,
    mode: u32,
    /// 0 for the sRGB curve, 1 for the Adobe RGB gamma curve.
    gamma: u32,
    row0: [f32; 4],
    row1: [f32; 4],
    row2: [f32; 4],
}

impl Params {
    fn new(
        width: u32,
        height: u32,
        operation: Operation,
        gamma: u32,
        matrix: [[f32; 3]; 3],
    ) -> Self {
        Self {
            width,
            height,
            mode: operation.as_u32(),
            gamma,
            row0: [matrix[0][0], matrix[0][1], matrix[0][2], 0.0],
            row1: [matrix[1][0], matrix[1][1], matrix[1][2], 0.0],
            row2: [matrix[2][0], matrix[2][1], matrix[2][2], 0.0],
        }
    }
}

pub fn pipeline(device: &wgpu::Device) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("color"),
        source: wgpu::ShaderSource::Wgsl(include_str!("color.wgsl").into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("color"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

pub struct Request<'a> {
    pub pixels: &'a [f32],
    pub width: u32,
    pub height: u32,
    pub operation: Operation,
    pub gamma: bool,
    pub matrix: [[f32; 3]; 3],
}

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
        operation,
        gamma,
        matrix,
    } = request;
    let expected = width as usize * height as usize * 4;
    if width == 0 || height == 0 || pixels.len() < expected {
        return Vec::new();
    }
    let params = Params::new(width, height, operation, u32::from(gamma), matrix);
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("color params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("color input"),
        contents: bytemuck::cast_slice(&pixels[..expected]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("color output"),
        size: (expected * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("color"),
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
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("color"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
    queue.submit(Some(encoder.finish()));
    let bytes = crate::readback::read_buffer(device, queue, &output, (expected * 4) as u64);
    bytemuck::cast_slice(&bytes).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;

    fn identity() -> [[f32; 3]; 3] {
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    }

    #[test]
    fn premultiply_matches_the_core() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        let pixels = [0.2, 0.4, 0.8, 0.5, 1.0, 0.0, 0.0, 1.0];
        let gpu = renderer.color(&pixels, 2, 1, Operation::Premultiply, false, identity());
        let expected: Vec<f32> = pixels
            .chunks(4)
            .flat_map(|pixel| {
                drawkit_core::color::premultiply([pixel[0], pixel[1], pixel[2], pixel[3]])
            })
            .collect();
        for (index, (left, right)) in gpu.iter().zip(expected.iter()).enumerate() {
            assert!(
                (left - right).abs() < 1e-6,
                "channel {index}: {left} vs {right}"
            );
        }

        let back = renderer.color(&gpu, 2, 1, Operation::Unpremultiply, false, identity());
        for (index, (left, right)) in back.iter().zip(pixels.iter()).enumerate() {
            assert!(
                (left - right).abs() < 1e-5,
                "channel {index}: {left} vs {right}"
            );
        }
    }

    #[test]
    fn encoded_colour_becomes_linear_display_p3() {
        let Some(renderer) = Renderer::new() else {
            return;
        };
        for space in [
            drawkit_core::document::ColorSpace::Srgb,
            drawkit_core::document::ColorSpace::DisplayP3,
            drawkit_core::document::ColorSpace::AdobeRgb,
        ] {
            let mut document = drawkit_core::document::Document::new(1, 1);
            document.color_space = space;
            document.layers[0].surface.set(0, 0, [0.3, 0.6, 0.2, 0.7]);
            let cpu = document.composite_linear_rect(drawkit_core::surface::Rect::new(0, 0, 1, 1));
            let encoded: Vec<f32> = document
                .composite_rect(drawkit_core::surface::Rect::new(0, 0, 1, 1))
                .iter()
                .flat_map(|pixel| pixel.iter().copied())
                .collect();
            let gpu = renderer.color(
                &encoded,
                1,
                1,
                Operation::ToLinearP3,
                space == drawkit_core::document::ColorSpace::AdobeRgb,
                space.to_display_p3(),
            );
            for (index, (left, right)) in gpu.iter().zip(cpu[0].iter()).enumerate() {
                assert!(
                    (left - right).abs() < 1e-3,
                    "{space:?} channel {index}: gpu {left}, cpu {right}"
                );
            }
        }
    }
}

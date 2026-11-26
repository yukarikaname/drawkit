// A headless wgpu renderer for the mannequin.
//
// Everything a pose needs is rebuilt per call: vertex buffer, index buffer, depth texture, readback
// buffer. At a few thousand vertices and one 512x768 target that is cheaper than the bookkeeping
// for caching, and it keeps the API to `render(&mesh, &camera, ...)`.

use std::mem::size_of;

use drawkit_figure::{Camera, Lighting, Mesh, RenderStyle, Vertex};
use glam::Mat4;

use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    key: [f32; 4],
    fill: [f32; 4],
    rim: [f32; 4],
    color: [f32; 4],
    style: [f32; 4],
}

mod uniforms;

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    morphology_pipeline: wgpu::ComputePipeline,
    decontaminate_pipeline: wgpu::ComputePipeline,
    feather_pipeline: wgpu::ComputePipeline,
    color_pipeline: wgpu::ComputePipeline,
    thumbnail_pipeline: wgpu::ComputePipeline,
    selection_pipeline: wgpu::ComputePipeline,
    brush_pipeline: wgpu::ComputePipeline,
}

impl Renderer {
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// A device and a mannequin pipeline, or `None` when the machine has no usable adapter.
    pub fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("drawkit figure"),
            // The adapter's own limits, not the downlevel 2048 texture cap: a 4K or 8K canvas has
            // to fit a texture before it can be composited or downsampled on the GPU.
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()?;
        Self::from_device(device, queue)
    }

    /// Build the pipelines on a device that already exists, so a replay compositor can share the
    /// canvas surface's device and hand it a texture instead of reading pixels back.
    pub fn from_device(device: wgpu::Device, queue: wgpu::Queue) -> Option<Self> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mannequin"),
            source: wgpu::ShaderSource::Wgsl(include_str!("mannequin.wgsl").into()),
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mannequin"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let composite_pipeline = crate::composite::pipeline(&device);
        let morphology_pipeline = crate::morphology::pipeline(&device);
        let decontaminate_pipeline = crate::decontaminate::pipeline(&device);
        let feather_pipeline = crate::feather::pipeline(&device);
        let color_pipeline = crate::color::pipeline(&device);
        let thumbnail_pipeline = crate::thumbnail::pipeline(&device);
        let selection_pipeline = crate::selection::pipeline(&device);
        let brush_pipeline = crate::brush::pipeline(&device);
        Some(Self {
            device,
            queue,
            pipeline,
            composite_pipeline,
            morphology_pipeline,
            decontaminate_pipeline,
            feather_pipeline,
            color_pipeline,
            thumbnail_pipeline,
            selection_pipeline,
            brush_pipeline,
        })
    }

    pub fn feather(&self, mask: &[f32], width: u32, height: u32, radius: u32) -> Vec<f32> {
        crate::feather::run(
            &self.device,
            &self.queue,
            &self.feather_pipeline,
            mask,
            width,
            height,
            radius,
        )
    }

    /// Convert an RGBA `f32` buffer: premultiply, unpremultiply, or encode-to-linear-P3.
    pub fn color(
        &self,
        pixels: &[f32],
        width: u32,
        height: u32,
        operation: crate::ColorOperation,
        gamma: bool,
        matrix: [[f32; 3]; 3],
    ) -> Vec<f32> {
        crate::color::run(
            &self.device,
            &self.queue,
            &self.color_pipeline,
            crate::color::Request {
                pixels,
                width,
                height,
                operation,
                gamma,
                matrix,
            },
        )
    }

    /// Box-downsample an RGBA8 layer to a premultiplied preview.
    pub fn thumbnail(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
        out_width: u32,
        out_height: u32,
    ) -> Vec<f32> {
        crate::thumbnail::run(
            &self.device,
            &self.queue,
            &self.thumbnail_pipeline,
            crate::thumbnail::Request {
                pixels,
                width,
                height,
                out_width,
                out_height,
            },
        )
    }

    /// Rasterise a closed lasso polygon into a `0..=1` coverage mask.
    pub fn lasso(&self, points: &[[f32; 2]], width: u32, height: u32) -> Vec<f32> {
        crate::selection::lasso(
            &self.device,
            &self.queue,
            &self.selection_pipeline,
            points,
            width,
            height,
        )
    }

    /// Paint or erase one brush segment into a straight RGBA layer.
    pub fn brush(&self, segment: crate::BrushSegment<'_>) -> Vec<f32> {
        crate::brush::run(&self.device, &self.queue, &self.brush_pipeline, segment)
    }

    pub fn decontaminate(
        &self,
        pixels: &[u8],
        alpha: &[u8],
        width: u32,
        height: u32,
        background: [f32; 3],
    ) -> Vec<u8> {
        crate::decontaminate::run(
            &self.device,
            &self.queue,
            &self.decontaminate_pipeline,
            crate::decontaminate::Request {
                pixels,
                alpha,
                width,
                height,
                background,
            },
        )
    }

    pub fn close(&self, mask: &[f32], width: u32, height: u32, radius: u32) -> Vec<f32> {
        crate::morphology::close(
            &self.device,
            &self.queue,
            &self.morphology_pipeline,
            mask,
            width,
            height,
            radius,
        )
    }

    pub fn morphology(
        &self,
        mask: &[f32],
        width: u32,
        height: u32,
        radius: u32,
        operation: crate::Morphology,
    ) -> Vec<f32> {
        crate::morphology::run(
            &self.device,
            &self.queue,
            &self.morphology_pipeline,
            crate::morphology::Request {
                mask,
                width,
                height,
                radius,
                operation,
            },
        )
    }

    pub fn layer_stack(&self, width: u32, height: u32) -> crate::LayerStack {
        crate::LayerStack::new(width, height)
    }

    /// Composite layers that already live on the GPU.
    pub fn composite_stack(
        &self,
        stack: &mut crate::LayerStack,
        specs: &[crate::LayerSpec],
    ) -> Vec<u8> {
        stack.composite(&self.device, &self.queue, &self.composite_pipeline, specs)
    }

    /// Composite resident layers into a texture that stays on the GPU.
    pub fn composite_stack_texture(
        &self,
        stack: &mut crate::LayerStack,
        specs: &[crate::LayerSpec],
    ) -> Option<wgpu::Texture> {
        stack.composite_texture(&self.device, &self.queue, &self.composite_pipeline, specs)
    }

    /// A preview of a layer that already lives on the GPU.
    pub fn layer_thumbnail(
        &self,
        stack: &crate::LayerStack,
        index: usize,
        out_width: u32,
        out_height: u32,
    ) -> Vec<f32> {
        stack.thumbnail(index, out_width, out_height)
    }

    /// Composite straight-alpha layers into one premultiplied RGBA8 image.
    pub fn composite(
        &self,
        layers: &[crate::composite::Layer<'_>],
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        crate::composite::composite(
            &self.device,
            &self.queue,
            &self.composite_pipeline,
            layers,
            width,
            height,
        )
    }

    /// Straight RGBA8, top row first, transparent where the mesh does not cover.
    pub fn render(
        &self,
        mesh: &Mesh,
        camera: &Camera,
        width: u32,
        height: u32,
        color: [f32; 3],
    ) -> Vec<u8> {
        self.render_with_lighting(mesh, camera, width, height, color, &Lighting::default())
    }

    pub fn render_with_lighting(
        &self,
        mesh: &Mesh,
        camera: &Camera,
        width: u32,
        height: u32,
        color: [f32; 3],
        lighting: &Lighting,
    ) -> Vec<u8> {
        self.render_styled(
            mesh,
            camera,
            width,
            height,
            color,
            lighting,
            &RenderStyle::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_styled(
        &self,
        mesh: &Mesh,
        camera: &Camera,
        width: u32,
        height: u32,
        color: [f32; 3],
        lighting: &Lighting,
        style: &RenderStyle,
    ) -> Vec<u8> {
        if width == 0 || height == 0 || mesh.is_empty() {
            return Vec::new();
        }
        let color_target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mannequin colour"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth_target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mannequin depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let color_view = color_target.create_view(&Default::default());
        let depth_view = depth_target.create_view(&Default::default());
        let uniforms = self.uniforms(camera, width, height, color, lighting, style);
        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mannequin uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let layout = self.pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mannequin uniforms"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mannequin vertices"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mannequin indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mannequin"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.indices.len() as u32, 0, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        let mut pixels =
            crate::readback::read_texture(&self.device, &self.queue, &color_target, width, height);
        if style.outline > 0 {
            crate::outline::darken(&mut pixels, width as usize, height as usize, style.outline);
        }
        pixels
    }
}

#[cfg(test)]
mod tests;

//! A small preview of the composed canvas, rendered and read back on the GPU.

use wgpu::util::DeviceExt;

use super::*;

pub(super) fn pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("thumbnail"),
        source: wgpu::ShaderSource::Wgsl(include_str!("thumbnail.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("thumbnail"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl CanvasSurface {
    /// Composite the layers into a `size` square preview and read it back.
    pub fn thumbnail(&mut self, width: u32, height: u32) -> Option<Vec<u8>> {
        // The preview shader composites in display space; an extended-range frame needs a tone
        // map, so the navigator keeps using the CPU overview there.
        if width == 0 || height == 0 {
            return None;
        }
        if self.hdr {
            // The navigator previews an extended document too; encode the same linear composite
            // into the working space at the preview's size.
            let source = self.layers.composite_texture(
                &self.device,
                &self.queue,
                &self.composite_hdr_pipeline,
                &self.layer_specs,
            )?;
            return super::export::encode::render(
                &self.device,
                &self.queue,
                &source,
                width,
                height,
                self.color_space,
            );
        }
        let source = self.layers.composite_texture(
            &self.device,
            &self.queue,
            &self.composite_pipeline,
            &self.layer_specs,
        )?;
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("thumbnail"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("thumbnail"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let source_view = source.create_view(&Default::default());
        let size_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("thumbnail size"),
                contents: bytemuck::cast_slice(&[width as f32, height as f32]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("thumbnail"),
            layout: &self.thumbnail_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: size_buffer.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("thumbnail"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.thumbnail_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        Some(crate::readback::read_texture(
            &self.device,
            &self.queue,
            &target,
            width,
            height,
        ))
    }

    /// One layer's pixels as premultiplied RGBA8, for the palette. The stack is not composed and
    /// no paper is added: the row shows what the layer holds, like the core's layer thumbnail.
    /// A group has no pixels of its own, so it returns `None` and the palette shows no preview.
    pub fn layer_thumbnail(&self, index: usize, width: u32, height: u32) -> Option<Vec<u8>> {
        if width == 0 || height == 0 || width > 512 || height > 512 {
            return None;
        }
        if self
            .layer_specs
            .iter()
            .any(|spec| spec.parent == Some(index))
        {
            return None;
        }
        let pixels: Vec<f32> = match self.layer_specs.get(index).and_then(|spec| spec.fill) {
            Some(fill) => {
                let opacity = self.layer_specs[index].opacity.clamp(0.0, 1.0);
                let pixel = [
                    fill[0],
                    fill[1],
                    fill[2],
                    (fill[3] * opacity).clamp(0.0, 1.0),
                ];
                (0..width * height).flat_map(|_| pixel).collect()
            }
            None => {
                // The live stack holds no CPU backup, so a palette refresh reads the resident
                // tiles on the GPU instead of materialising the whole layer to downsample it.
                if !self.layers.keeps_backups() {
                    return self.layers.layer_thumbnail(
                        &self.device,
                        &self.queue,
                        index,
                        width,
                        height,
                    );
                }
                self.layers.thumbnail(index, width, height)
            }
        };
        if pixels.is_empty() {
            return None;
        }
        Some(
            pixels
                .into_iter()
                .map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
        )
    }
}

//! Reading the composed canvas back for an export or a Copy Merged.
//!
//! The live stack keeps no CPU backups, so the old path reads every committed tile into the core
//! and composites it on the CPU before a save. The composite already exists on the GPU, so an
//! export only has to resample it into the requested size and read that one texture back.

use super::*;

pub(super) mod encode;

#[cfg(test)]
mod tests;

fn resample_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    resample_pipeline_for(device, wgpu::TextureFormat::Rgba8Unorm)
}

fn resample_pipeline_for(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("canvas export"),
        source: wgpu::ShaderSource::Wgsl(include_str!("export.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("canvas export"),
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
                format,
                // The source is already premultiplied and the shader returns it unchanged.
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

/// Where an export goes and which working space it is tagged with.
#[derive(Clone, Copy)]
pub(super) struct ExportRequest {
    pub width: u32,
    pub height: u32,
    pub space: i32,
}

/// Composite `specs` at the request's size and read the frame back as premultiplied RGBA8.
///
/// The stack's own composite stays in its cache, so a present after an export still has its frame.
/// Refuses an extended-range stack, a zero or oversized target, and an empty canvas.
pub(super) fn render_export(
    layers: &mut crate::LayerStack,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    specs: &[crate::LayerSpec],
    request: ExportRequest,
) -> Option<Vec<u8>> {
    let ExportRequest {
        width,
        height,
        space,
    } = request;
    let (canvas_width, canvas_height) = layers.size();
    let limit = device.limits().max_texture_dimension_2d;
    if width == 0
        || height == 0
        || canvas_width == 0
        || canvas_height == 0
        || width > limit
        || height > limit
    {
        return None;
    }
    let extended = layers.format() == crate::FrameFormat::Rgba16;
    let source = layers.composite_texture(device, queue, pipeline, specs)?;
    if extended {
        // One encode pass, then the same readback: the file is tagged with the document space.
        return encode::render(device, queue, &source, width, height, space);
    }
    if width == canvas_width && height == canvas_height {
        return Some(crate::readback::read_texture(
            device, queue, &source, width, height,
        ));
    }
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("canvas export"),
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
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("canvas export"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });
    let pipeline = resample_pipeline(device);
    let source_view = source.create_view(&Default::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("canvas export"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&source_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("canvas export"),
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    Some(crate::readback::read_texture(
        device, queue, &target, width, height,
    ))
}

/// The same frame in premultiplied linear Display P3, for an EXR.
pub(super) fn render_export_linear(
    layers: &mut crate::LayerStack,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    specs: &[crate::LayerSpec],
    width: u32,
    height: u32,
) -> Option<Vec<f32>> {
    let (canvas_width, canvas_height) = layers.size();
    let limit = device.limits().max_texture_dimension_2d;
    if width == 0
        || height == 0
        || width > limit
        || height > limit
        || layers.format() != crate::FrameFormat::Rgba16
    {
        return None;
    }
    let source = layers.composite_texture(device, queue, pipeline, specs)?;
    if width == canvas_width && height == canvas_height {
        return Some(crate::readback::read_texture_linear(
            device, queue, &source, width, height,
        ));
    }
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("canvas export linear"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("canvas export linear"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });
    let pipeline = resample_pipeline_for(device, wgpu::TextureFormat::Rgba16Float);
    let source_view = source.create_view(&Default::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("canvas export linear"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&source_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("canvas export linear"),
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    Some(crate::readback::read_texture_linear(
        device, queue, &target, width, height,
    ))
}

impl CanvasSurface {
    /// Composite the current layer specs into `width` by `height` premultiplied RGBA8.
    ///
    /// At the canvas size the cached composite is read directly. A scaled export renders through
    /// its own target, so the cache the next present copies from is left intact.
    pub fn export_frame(&mut self, width: u32, height: u32) -> Option<Vec<u8>> {
        let (specs, hidden) = self.export_specs();
        // Reference layers are on screen but not in an export, so their frames differ. Recompose
        // every tile for the export, then restore the on-screen frame below.
        if hidden {
            self.layers.mark_all_dirty();
        }
        let pipeline = if self.layers.format() == crate::FrameFormat::Rgba8 {
            &self.composite_pipeline
        } else {
            &self.composite_hdr_pipeline
        };
        let pixels = render_export(
            &mut self.layers,
            &self.device,
            &self.queue,
            pipeline,
            &specs,
            ExportRequest {
                width,
                height,
                space: self.color_space,
            },
        );
        if hidden {
            // Put the reference layers back into the cache now: a present or a pick must not see
            // the export's frame.
            self.layers.mark_all_dirty();
            let _ = self.layers.composite_texture(
                &self.device,
                &self.queue,
                &self.composite_pipeline,
                &self.layer_specs,
            );
        }
        pixels
    }

    /// Composite the current layer specs as premultiplied linear Display P3, for EXR.
    pub fn export_linear(&mut self, width: u32, height: u32) -> Option<Vec<f32>> {
        let (specs, hidden) = self.export_specs();
        if hidden {
            self.layers.mark_all_dirty();
        }
        let pixels = render_export_linear(
            &mut self.layers,
            &self.device,
            &self.queue,
            &self.composite_hdr_pipeline,
            &specs,
            width,
            height,
        );
        if hidden {
            self.layers.mark_all_dirty();
            let _ = self.layers.composite_texture(
                &self.device,
                &self.queue,
                &self.composite_hdr_pipeline,
                &self.layer_specs,
            );
        }
        pixels
    }

    /// Leave layer `index` out of an export. The canvas still shows it.
    pub fn set_export_reference(&mut self, index: usize, reference: bool) {
        if self.export_references.len() <= index {
            self.export_references.resize(index + 1, false);
        }
        self.export_references[index] = reference;
    }

    /// The specs to composite for an export, and whether a reference layer was hidden.
    fn export_specs(&self) -> (Vec<crate::LayerSpec>, bool) {
        let hidden = self
            .layer_specs
            .iter()
            .enumerate()
            .any(|(index, _)| self.export_references.get(index).copied().unwrap_or(false));
        if !hidden {
            return (self.layer_specs.clone(), false);
        }
        let mut specs = self.layer_specs.clone();
        for (index, spec) in specs.iter_mut().enumerate() {
            if self.export_references.get(index).copied().unwrap_or(false) {
                spec.visible = false;
            }
        }
        (specs, true)
    }
}

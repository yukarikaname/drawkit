//! Reused render targets for a tile-by-tile composite.
//!
//! A tiled layer stack composites one canvas tile at a time. Allocating two targets per tile
//! would make an 8K frame allocate hundreds of textures, so the targets live for the whole
//! composite and each tile clears and reuses them. The result of an isolated group needs its own
//! pair: a parent still samples it after other children have been rendered.

use super::*;

struct Pair {
    first: wgpu::Texture,
    second: wgpu::Texture,
    first_view: wgpu::TextureView,
    second_view: wgpu::TextureView,
}

impl Pair {
    fn new(device: &wgpu::Device, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        let first = target(device, "tile composite a", width, height, format);
        let second = target(device, "tile composite b", width, height, format);
        Self {
            first_view: first.create_view(&Default::default()),
            second_view: second.create_view(&Default::default()),
            first,
            second,
        }
    }

    fn result(&self, passes: usize) -> (&wgpu::Texture, &wgpu::TextureView) {
        if passes.is_multiple_of(2) {
            (&self.first, &self.first_view)
        } else {
            (&self.second, &self.second_view)
        }
    }
}

pub struct TileTargets {
    /// Slot zero is the final canvas tile; every isolated group owns one of the rest.
    slots: Vec<Pair>,
    fallback_view: wgpu::TextureView,
}

#[derive(Default)]
pub struct TileResources {
    _buffers: Vec<wgpu::Buffer>,
    _groups: Vec<wgpu::BindGroup>,
}

impl TileTargets {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        slots: usize,
    ) -> Self {
        let slots = (0..slots.max(1))
            .map(|_| Pair::new(device, width, height, format))
            .collect();
        let fallback = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile composite fill"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let empty = [0u8; 8];
        let bytes = if format == FORMAT { 4 } else { 8 };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &fallback,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &empty[..bytes],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes as u32),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let fallback_view = fallback.create_view(&Default::default());
        Self {
            slots,
            fallback_view,
        }
    }

    /// Clear one slot, for a tile that has no sources.
    pub fn clear(
        &self,
        slot: usize,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Option<(&wgpu::Texture, &wgpu::TextureView)> {
        let pair = self.slots.get(slot)?;
        clear(encoder, &pair.first_view, "tile composite clear");
        Some((&pair.first, &pair.first_view))
    }

    /// Record the composite passes for one slot and return the texture holding the result.
    pub fn render<'a>(
        &'a self,
        slot: usize,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        sources: &[Source<'a>],
        resources: &mut TileResources,
    ) -> Option<(&'a wgpu::Texture, &'a wgpu::TextureView)> {
        let pair = self.slots.get(slot)?;
        clear(encoder, &pair.first_view, "tile composite clear");
        let layout = pipeline.get_bind_group_layout(0);
        let mut passes: usize = 0;
        for source in sources {
            if !source.visible || source.opacity <= 0.0 {
                continue;
            }
            let (use_clip, clip_fill, clip_view) = match source.clip.as_ref() {
                Some(ClipSource::View(view)) => (1, [0.0; 4], *view),
                Some(ClipSource::Fill(fill)) => (2, *fill, &self.fallback_view),
                None => (0, [0.0; 4], &self.fallback_view),
            };
            let params = Params {
                opacity: source.opacity.clamp(0.0, 1.0),
                mode: source.blend.as_u32(),
                use_fill: u32::from(source.view.is_none()),
                use_clip,
                fill: source.fill,
                clip_fill,
            };
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("tile composite params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let view = source.view.unwrap_or(&self.fallback_view);
            let (backdrop, destination_view) = if passes.is_multiple_of(2) {
                (&pair.first_view, &pair.second_view)
            } else {
                (&pair.second_view, &pair.first_view)
            };
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("tile composite layer"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(backdrop),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(clip_view),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("tile composite layer"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: destination_view,
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
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            resources._buffers.push(uniform);
            resources._groups.push(bind_group);
            passes += 1;
        }
        (passes > 0).then(|| pair.result(passes))
    }
}

//! Reused render targets for a tile-by-tile composite.
//!
//! A tiled layer stack composites one canvas tile at a time. Allocating two targets per tile
//! would make an 8K frame allocate hundreds of textures, so the targets live for the whole
//! composite and each tile clears and reuses them. The uniforms and bind groups are kept in
//! `TileResources` until the command buffer is submitted.

use super::*;

pub struct TileTargets {
    first: wgpu::Texture,
    second: wgpu::Texture,
    first_view: wgpu::TextureView,
    second_view: wgpu::TextureView,
    fallback_view: wgpu::TextureView,
}

#[derive(Default)]
pub struct TileResources {
    _buffers: Vec<wgpu::Buffer>,
    _groups: Vec<wgpu::BindGroup>,
}

impl TileTargets {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) -> Self {
        let first = target(device, "tile composite a", width, height);
        let second = target(device, "tile composite b", width, height);
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
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &fallback,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0, 0, 0, 0],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
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
            first_view: first.create_view(&Default::default()),
            second_view: second.create_view(&Default::default()),
            first,
            second,
            fallback_view,
        }
    }

    /// Clear a tile that has no sources, so a dirty tile that became empty does not leave the old
    /// pixels in the cached output.
    pub fn clear_tile(&self, encoder: &mut wgpu::CommandEncoder) -> &wgpu::Texture {
        clear(encoder, &self.first_view, "tile composite clear");
        &self.first
    }

    /// Record the composite passes for one tile and return the texture holding the result.
    pub fn render(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        sources: &[Source<'_>],
        resources: &mut TileResources,
    ) -> Option<&wgpu::Texture> {
        clear(encoder, &self.first_view, "tile composite clear");
        let layout = pipeline.get_bind_group_layout(0);
        let mut passes = 0;
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
            let (backdrop, destination) = if passes % 2 == 0 {
                (&self.first_view, &self.second_view)
            } else {
                (&self.second_view, &self.first_view)
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
                        view: destination,
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
        (passes > 0).then_some(if passes % 2 == 0 {
            &self.first
        } else {
            &self.second
        })
    }
}

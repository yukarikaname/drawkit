//! One layer's resident tiles as a small premultiplied RGBA8 preview, without a CPU backup.
//!
//! A live stack has no backup to downsample, and materialising one just to refresh the palette
//! means reading every committed stroke back from the GPU. This pass draws each resident tile
//! straight into a thumbnail-sized target and reads back only that target; tiles the shell never
//! painted, or that eviction dropped, simply have no texture and are skipped.

use wgpu::util::DeviceExt;

use super::*;

/// One per-tile draw: the target rectangle and the valid part of the tile it samples.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    /// Target rectangle in output pixels: x, y, width, height.
    rect: [f32; 4],
    /// Source rectangle inside the tile in pixels: x, y, width, height.
    src: [f32; 4],
    out_size: [f32; 2],
    tile: [f32; 2],
    linear: u32,
    _pad: [u32; 3],
}

fn pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("layer thumbnail"),
        source: wgpu::ShaderSource::Wgsl(include_str!("thumbnail.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("layer thumbnail"),
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
                // The fragment returns premultiplied colour, so the quads composite over each
                // other where a scaled edge lands on the same pixel.
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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

impl LayerStack {
    /// Downscale one layer into `width` by `height` premultiplied RGBA8, reading only the resident
    /// tile textures. A missing or evicted tile is skipped and a layer with no tiles is fully
    /// transparent, so a palette refresh never needs the CPU backup.
    ///
    /// Refuses a stack that keeps backups (the offscreen compositor has the cheaper CPU path),
    /// an out-of-range layer, a canvas with no pixels and a size outside `1..=512`.
    pub fn layer_thumbnail(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        width: u32,
        height: u32,
    ) -> Option<Vec<u8>> {
        if self.backups
            || self.width == 0
            || self.height == 0
            || width == 0
            || height == 0
            || width > 512
            || height > 512
        {
            return None;
        }
        // An index past the layer list is a caller error; a hole or a layer with no tiles is an
        // empty layer and draws as transparent.
        let layer = self.layers.get(index)?;
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("layer thumbnail"),
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
            label: Some("layer thumbnail"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let pipeline = pipeline(device);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("layer thumbnail"),
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
            if let Some(layer) = layer {
                let mut coords: Vec<(u32, u32)> = layer.tiles.keys().copied().collect();
                coords.sort_unstable();
                for coord in coords {
                    let Some(tile) = layer.tiles.get(&coord) else {
                        continue;
                    };
                    // An evicted tile has no texture left; the shell uploads it again later.
                    let Some(gpu) = &tile.gpu else {
                        continue;
                    };
                    let Some(params) = self.thumbnail_params(coord, width, height) else {
                        continue;
                    };
                    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("layer thumbnail params"),
                        contents: bytemuck::bytes_of(&params),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("layer thumbnail"),
                        layout: &pipeline.get_bind_group_layout(0),
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: uniform.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&gpu.view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::Sampler(&sampler),
                            },
                        ],
                    });
                    pass.set_bind_group(0, &bind_group, &[]);
                    pass.draw(0..6, 0..1);
                }
            }
        }
        queue.submit(Some(encoder.finish()));
        Some(crate::readback::read_texture(
            device, queue, &target, width, height,
        ))
    }

    /// The canvas rectangle a tile covers, scaled into the preview, and the tile pixels it may
    /// sample. `None` when the tile is entirely outside the canvas.
    fn thumbnail_params(&self, (tx, ty): (u32, u32), width: u32, height: u32) -> Option<Params> {
        let x = tx * self.tile_width;
        let y = ty * self.tile_height;
        if x >= self.width || y >= self.height {
            return None;
        }
        let src_width = self.tile_width.min(self.width - x) as f32;
        let src_height = self.tile_height.min(self.height - y) as f32;
        let scale_x = width as f32 / self.width as f32;
        let scale_y = height as f32 / self.height as f32;
        let rect = [
            x as f32 * scale_x,
            y as f32 * scale_y,
            src_width * scale_x,
            src_height * scale_y,
        ];
        if rect[2] <= 0.0 || rect[3] <= 0.0 {
            return None;
        }
        Some(Params {
            rect,
            src: [0.0, 0.0, src_width, src_height],
            out_size: [width as f32, height as f32],
            tile: [self.tile_width as f32, self.tile_height as f32],
            linear: u32::from(self.format == crate::FrameFormat::Rgba16),
            _pad: [0; 3],
        })
    }
}

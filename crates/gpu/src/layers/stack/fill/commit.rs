//! A journalled fill into the resident tiles: the undo record is the replaced textures.
//!
//! The blend runs on the GPU: the coverage mask is uploaded once as `R8Unorm`, then every touched
//! tile is rendered from its old texture through the mask into a new texture. The replaced texture
//! becomes the journal swap and the new one stays resident and stale, so `materialize` refreshes
//! the CPU backup later instead of the live path reading the tiles back.

use wgpu::util::DeviceExt;

use super::*;
use crate::layers::TileSwap;

/// The rectangles a fill touched and the textures it replaced.
pub(crate) type FillResult = (Vec<(u32, u32, u32, u32)>, Vec<TileSwap>);

/// A touched tile and the canvas rectangle of the mask inside it.
pub(super) type FillTile = ((u32, u32), (u32, u32, u32, u32));

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FillParams {
    origin: [u32; 2],
    canvas: [u32; 2],
    color: [f32; 4],
    /// 0 source-over, 1 destination-over (under), 2 alpha lock.
    mode: u32,
    _pad: [u32; 3],
}

/// The blend pipeline for tiles of `format`. The surface owns one per range.
pub(super) fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("tile fill"),
        source: wgpu::ShaderSource::Wgsl(include_str!("blend.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("tile fill"),
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

impl LayerStack {
    /// The pipeline that blends a fill mask into a tile of `format`.
    pub fn fill_pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        create_pipeline(device, format)
    }

    /// Blend a fill mask into layer `index` on the GPU and keep the replaced tile textures.
    ///
    /// The mask is packed to `R8Unorm` and uploaded once. Every touched tile is rendered from its
    /// old texture into a fresh one, which becomes the resident texture and is marked stale; the
    /// old texture is the journal swap, exactly like a stroke commit.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_fill(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        index: usize,
        mask: &[f32],
        color: [f32; 4],
        alpha_locked: bool,
        under: bool,
    ) -> FillResult {
        let pixels = self.width as usize * self.height as usize;
        if self.width == 0 || self.height == 0 || mask.len() < pixels {
            return (Vec::new(), Vec::new());
        }
        let tiles = self.fill_tile_rects(mask);
        if tiles.is_empty() {
            return (Vec::new(), Vec::new());
        }
        if self.layers.len() <= index {
            self.layers.resize_with(index + 1, || None);
        }
        if self.layers[index].is_none() {
            self.layers[index] = Some(Layer {
                tiles: HashMap::new(),
                known: true,
            });
        }
        let coverage = upload_coverage(device, queue, mask, self.width, self.height);
        let coverage_view = coverage.create_view(&Default::default());
        let (tile_width, tile_height) = (self.tile_width, self.tile_height);
        let tile_bytes = self.tile_bytes();
        let mode = if alpha_locked {
            2
        } else if under {
            1
        } else {
            0
        };
        let mut swaps = Vec::with_capacity(tiles.len());
        let mut filled = Vec::with_capacity(tiles.len());
        for (coord, rect) in &tiles {
            // The old pixels have to be a texture before the fill replaces them, or undo would
            // have nothing to swap back in for a tile the fill was the first paint on. A
            // backup-less tile that was evicted has no pixels to blend into, so it is skipped
            // until the shell uploads it again.
            if !self.ensure_edit_tile(device, queue, index, *coord, tile_bytes) {
                continue;
            }
            let Some(old_texture) = self.layers[index]
                .as_ref()
                .and_then(|layer| layer.tiles.get(coord))
                .and_then(|tile| tile.gpu.as_ref())
                .map(|gpu| gpu.texture.clone())
            else {
                continue;
            };
            let target = self.take_tile_texture(device, tile_bytes);
            let old_view = old_texture.create_view(&Default::default());
            let params = FillParams {
                origin: [coord.0 * tile_width, coord.1 * tile_height],
                canvas: [self.width, self.height],
                color,
                mode,
                _pad: [0; 3],
            };
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("tile fill params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("tile fill"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&old_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&coverage_view),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("tile fill"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.view,
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
            let backups = self.backups;
            let replaced = self.layers[index]
                .as_mut()
                .and_then(|layer| layer.tiles.get_mut(coord))
                .and_then(|tile| {
                    // The GPU holds the fill now; a backup would be refreshed by `materialize`.
                    tile.stale = backups;
                    tile.gpu.replace(target)
                });
            if let Some(old) = replaced {
                self.resident_bytes = self.resident_bytes.saturating_sub(old.bytes) + tile_bytes;
                swaps.push(TileSwap {
                    coord: *coord,
                    old,
                    layer: index,
                    bytes: tile_bytes,
                });
                filled.push(*rect);
            }
            self.dirty_tiles.insert(*coord);
        }
        // A tile the fill skipped (evicted without a backup) is not reported as painted; the
        // shell already knows it is missing and must rebuild it from the core surface.
        (filled, swaps)
    }

    /// The canvas rectangle of every tile the mask covers, with the mask's own bounds inside it.
    pub(super) fn fill_tile_rects(&self, mask: &[f32]) -> Vec<FillTile> {
        let mut out = Vec::new();
        let width = self.width as usize;
        for ty in 0..self.height.div_ceil(self.tile_height) {
            for tx in 0..self.width.div_ceil(self.tile_width) {
                let left = tx * self.tile_width;
                let top = ty * self.tile_height;
                let right = (left + self.tile_width).min(self.width);
                let bottom = (top + self.tile_height).min(self.height);
                let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
                for y in top..bottom {
                    let start = y as usize * width + left as usize;
                    let row = &mask[start..start + (right - left) as usize];
                    // A covered row only needs its first and last set pixel; a full row is two
                    // probes instead of a walk over every pixel.
                    let Some(first) = row.iter().position(|value| *value > 0.0) else {
                        continue;
                    };
                    let last = row.iter().rposition(|value| *value > 0.0).unwrap_or(first);
                    let row_left = left + first as u32;
                    let row_right = left + last as u32 + 1;
                    x0 = if x0 == u32::MAX {
                        row_left
                    } else {
                        x0.min(row_left)
                    };
                    x1 = x1.max(row_right);
                    if y0 == u32::MAX {
                        y0 = y;
                    }
                    y1 = y + 1;
                }
                if x0 != u32::MAX {
                    out.push(((tx, ty), (x0, y0, x1 - x0, y1 - y0)));
                }
            }
        }
        out
    }
}

/// Upload the coverage mask once, as the exact floats the core produced. `textureLoad` does not
/// need a filterable format, so `R32Float` costs no conversion and no precision; packing to eight
/// bits measured no faster, so the mask stays exact.
fn upload_coverage(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mask: &[f32],
    width: u32,
    height: u32,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fill coverage"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let source = &mask[..(width as usize * height as usize)];
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(source),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture
}

//! Baking a wet field's settled pigment into resident tiles.
//!
//! The field lives at its own resolution and keeps its water and mobile pigment. A bake only takes
//! the settled part: `WetDeposit` has already packed it to straight RGBA on the GPU, so every
//! touched tile is rendered from its old texture into a fresh one with that deposit. The replaced
//! texture is the journal swap, exactly like `commit_fill`, and the new one is marked stale so the
//! CPU cache catches up through `materialize`.
//!
//! The deposit replaces the wet footprint rather than compositing over it. A pixel the field never
//! reached keeps the old tile byte-for-byte; a pixel the field did reach is the field's total
//! settled colour. That makes a second bake of a dry field a pixel-identical no-op instead of a
//! second film of pigment.

use wgpu::util::DeviceExt;

use super::*;
use crate::layers::TileSwap;

#[cfg(test)]
mod tests;

/// The rectangles a bake touched and the textures it replaced.
pub(crate) type WetResult = (Vec<(u32, u32, u32, u32)>, Vec<TileSwap>);

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WetParams {
    origin: [u32; 2],
    size: [u32; 2],
    canvas: [u32; 2],
    field: [u32; 2],
}

/// The bake pipeline for tiles of `format`. The surface owns one per range.
pub(super) fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("tile wet bake"),
        source: wgpu::ShaderSource::Wgsl(include_str!("wet/bake.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("tile wet bake"),
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
    /// The pipeline that bakes a wet deposit into a tile of `format`.
    pub fn wet_pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        create_pipeline(device, format)
    }

    /// Render `deposit` into every tile layer `index` has under `rect`, keeping the old textures.
    ///
    /// `rect` is in canvas pixels. Only the tiles it touches are rewritten; a tile it does not
    /// touch is neither swapped nor marked dirty. The deposit carries its own field size, so the
    /// shader maps canvas pixels to field texels without a CPU loop.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_wet(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        index: usize,
        deposit: &crate::WetDeposit,
        rect: (u32, u32, u32, u32),
    ) -> WetResult {
        if deposit.is_empty() || deposit.canvas_size() != (self.width, self.height) {
            return (Vec::new(), Vec::new());
        }
        let (field_width, field_height) = deposit.size();
        if field_width == 0 || field_height == 0 {
            return (Vec::new(), Vec::new());
        }
        let (rx, ry, rw, rh) = rect;
        let right = (rx + rw).min(self.width);
        let bottom = (ry + rh).min(self.height);
        if rx >= right || ry >= bottom {
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
        let (tile_width, tile_height) = (self.tile_width, self.tile_height);
        let tile_bytes = self.tile_bytes();
        let mut rects = Vec::new();
        let mut swaps = Vec::new();
        for ty in ry / tile_height..=(bottom - 1) / tile_height {
            for tx in rx / tile_width..=(right - 1) / tile_width {
                let coord = (tx, ty);
                let left = rx.max(tx * tile_width);
                let top = ry.max(ty * tile_height);
                let width = right.min((tx + 1) * tile_width) - left;
                let height = bottom.min((ty + 1) * tile_height) - top;
                if width == 0 || height == 0 {
                    continue;
                }
                if !self.ensure_edit_tile(device, queue, index, coord, tile_bytes) {
                    continue;
                }
                let old_texture = self.layers[index]
                    .as_ref()
                    .and_then(|layer| layer.tiles.get(&coord))
                    .and_then(|tile| tile.gpu.as_ref())
                    .map(|gpu| gpu.texture.clone());
                let Some(old_texture) = old_texture else {
                    continue;
                };
                let old_view = old_texture.create_view(&Default::default());
                let target = self.take_tile_texture(device, tile_bytes);
                let params = WetParams {
                    origin: [tx * tile_width, ty * tile_height],
                    size: [tile_width, tile_height],
                    canvas: [self.width, self.height],
                    field: [field_width, field_height],
                };
                let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("tile wet params"),
                    contents: bytemuck::bytes_of(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("tile wet bake"),
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
                            resource: wgpu::BindingResource::TextureView(deposit.view()),
                        },
                    ],
                });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("tile wet bake"),
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
                    .and_then(|layer| layer.tiles.get_mut(&coord))
                    .and_then(|tile| {
                        // The GPU holds the bake now; a backup is refreshed by `materialize`.
                        tile.stale = backups;
                        tile.gpu.replace(target)
                    });
                if let Some(old) = replaced {
                    self.resident_bytes =
                        self.resident_bytes.saturating_sub(old.bytes) + tile_bytes;
                    swaps.push(TileSwap {
                        coord,
                        old,
                        layer: index,
                        bytes: tile_bytes,
                    });
                    rects.push((left, top, width, height));
                }
                self.dirty_tiles.insert(coord);
            }
        }
        // A tile the bake skipped (evicted without a backup) is not reported as painted; the shell
        // already knows it is missing and rebuilds it from the core surface.
        (rects, swaps)
    }
}

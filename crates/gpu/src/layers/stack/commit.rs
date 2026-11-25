//! Composite a GPU stroke's coverage into the resident layer tiles. The pixels stay on the GPU:
//! the replaced tiles become the undo record, and the CPU surface is a cache that is rebuilt only
//! when a save, an export or a pixel tool asks for it.

use wgpu::util::DeviceExt;

use super::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CommitParams {
    origin: [u32; 2],
    size: [u32; 2],
    canvas: [u32; 2],
    alpha: f32,
    _pad: f32,
    color: [f32; 4],
}

/// One read-back tile rectangle, straight RGBA8.
pub struct CommittedTile {
    pub layer: usize,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A tile texture replaced by a commit, kept so undo can swap it back.
pub(crate) type TileRect = (u32, u32, u32, u32);
pub(crate) type CommitResult = (Vec<TileRect>, Vec<TileSwap>);

pub(crate) struct TileSwap {
    pub(crate) coord: (u32, u32),
    pub(crate) old: TileTexture,
    pub(crate) layer: usize,
    pub(crate) bytes: usize,
}

pub fn pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("tile commit"),
        source: wgpu::ShaderSource::Wgsl(include_str!("commit.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("tile commit"),
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
                format: FORMAT,
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
    /// Composite coverage into each tile of `index` that `rect` touches. Returns the touched
    /// rectangles and the replaced textures, which become the undo record.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_stroke(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &wgpu::RenderPipeline,
        index: usize,
        coverage: &wgpu::Buffer,
        rect: (u32, u32, u32, u32),
        color: [f32; 3],
        alpha: f32,
    ) -> CommitResult {
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
        let coords: Vec<(u32, u32)> = (ry / tile_height..=(bottom - 1) / tile_height)
            .flat_map(|ty| (rx / tile_width..=(right - 1) / tile_width).map(move |tx| (tx, ty)))
            .collect();
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut rects = Vec::new();
        let mut swaps = Vec::new();
        for coord in coords {
            let (tx, ty) = coord;
            let left = rx.max(tx * tile_width);
            let top = ry.max(ty * tile_height);
            let width = right.min((tx + 1) * tile_width) - left;
            let height = bottom.min((ty + 1) * tile_height) - top;
            if width == 0 || height == 0 {
                continue;
            }
            if !self.ensure_commit_tile(device, queue, index, coord, tile_bytes) {
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
            let target = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("committed tile"),
                size: wgpu::Extent3d {
                    width: tile_width,
                    height: tile_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let target_view = target.create_view(&Default::default());
            let params = CommitParams {
                origin: [tx * tile_width, ty * tile_height],
                size: [tile_width, tile_height],
                canvas: [self.width, self.height],
                alpha,
                _pad: 0.0,
                color: [color[0], color[1], color[2], 1.0],
            };
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("tile commit params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let layout = pipeline.get_bind_group_layout(0);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("tile commit"),
                layout: &layout,
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
                        resource: coverage.as_entire_binding(),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("tile commit"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target_view,
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
            let replaced = self.layers[index]
                .as_mut()
                .and_then(|layer| layer.tiles.get_mut(&coord))
                .and_then(|tile| {
                    tile.stale = true;
                    tile.gpu.replace(TileTexture {
                        texture: target.clone(),
                        view: target_view,
                        bytes: tile_bytes,
                    })
                });
            if let Some(old) = replaced {
                self.resident_bytes = self.resident_bytes.saturating_sub(old.bytes) + tile_bytes;
                swaps.push(TileSwap {
                    coord,
                    old,
                    layer: index,
                    bytes: tile_bytes,
                });
            }
            self.dirty_tiles.insert(coord);
            rects.push((left, top, width, height));
        }
        queue.submit(Some(encoder.finish()));
        (rects, swaps)
    }

    /// Materialise one layer rectangle into RGBA8 for the CPU cache and refresh its tile backups.
    pub fn read_layer_rect(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        rect: (u32, u32, u32, u32),
    ) -> Option<CommittedTile> {
        let (rx, ry, rw, rh) = rect;
        let right = (rx + rw).min(self.width);
        let bottom = (ry + rh).min(self.height);
        if rx >= right || ry >= bottom {
            return None;
        }
        let (tile_width, tile_height) = (self.tile_width, self.tile_height);
        let columns = self.width.div_ceil(tile_width);
        let _ = columns;
        let coords: Vec<(u32, u32)> = (ry / tile_height..=(bottom - 1) / tile_height)
            .flat_map(|ty| (rx / tile_width..=(right - 1) / tile_width).map(move |tx| (tx, ty)))
            .collect();
        let width = right - rx;
        let height = bottom - ry;
        let padded = (width as usize * 4).div_ceil(256) * 256;
        let mut pixels = vec![0_u8; padded * height as usize];
        let mut spans = Vec::new();
        for coord in coords {
            let (tx, ty) = coord;
            let left = rx.max(tx * tile_width);
            let top = ry.max(ty * tile_height);
            let span_x = right.min((tx + 1) * tile_width) - left;
            let span_y = bottom.min((ty + 1) * tile_height) - top;
            if span_x == 0 || span_y == 0 {
                continue;
            }
            if !self.ensure_tile(device, queue, index, coord) {
                continue;
            }
            let Some(texture) = self.layers[index]
                .as_ref()
                .and_then(|layer| layer.tiles.get(&coord))
                .and_then(|tile| tile.gpu.as_ref())
                .map(|gpu| gpu.texture.clone())
            else {
                continue;
            };
            let padded_row = (span_x as usize * 4).div_ceil(256) * 256;
            let staging = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tile materialize"),
                size: (padded_row * span_y as usize) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: left - tx * tile_width,
                        y: top - ty * tile_height,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &staging,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row as u32),
                        rows_per_image: Some(span_y),
                    },
                },
                wgpu::Extent3d {
                    width: span_x,
                    height: span_y,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(Some(encoder.finish()));
            staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
            let Ok(mapped) = staging.slice(..).get_mapped_range() else {
                continue;
            };
            for row in 0..span_y as usize {
                let source = row * padded_row;
                let target = ((top - ry) as usize + row) * width as usize + (left - rx) as usize;
                let target = target * 4;
                pixels[target..target + span_x as usize * 4]
                    .copy_from_slice(&mapped[source..source + span_x as usize * 4]);
            }
            drop(mapped);
            staging.unmap();
            spans.push((coord, left, top, span_x, span_y));
        }
        let mut tight = Vec::with_capacity(width as usize * height as usize * 4);
        for row in 0..height as usize {
            let start = row * padded;
            tight.extend_from_slice(&pixels[start..start + width as usize * 4]);
        }
        for (coord, left, top, span_x, span_y) in spans {
            self.store_backup(
                index, coord, left, top, span_x, span_y, rx, ry, width, &tight,
            );
        }
        Some(CommittedTile {
            layer: index,
            x: rx,
            y: ry,
            width,
            height,
            pixels: tight,
        })
    }

    /// Keep the CPU backup in step with what materialisation read from the tile.
    #[allow(clippy::too_many_arguments)]
    fn store_backup(
        &mut self,
        index: usize,
        coord: (u32, u32),
        left: u32,
        top: u32,
        span_x: u32,
        span_y: u32,
        rx: u32,
        ry: u32,
        width: u32,
        pixels: &[u8],
    ) {
        let columns = self.tile_width as usize;
        let local_x = (left - coord.0 * self.tile_width) as usize;
        let local_y = (top - coord.1 * self.tile_height) as usize;
        let Some(tile) = self.layers[index]
            .as_mut()
            .and_then(|layer| layer.tiles.get_mut(&coord))
        else {
            return;
        };
        let row = span_x as usize * 4;
        for line in 0..span_y as usize {
            let source_row = ((top - ry) as usize + line) * width as usize + (left - rx) as usize;
            let target = ((local_y + line) * columns + local_x) * 4;
            let source = source_row * 4;
            for column in 0..span_x as usize {
                let old = &tile.backup[target + column * 4..][..4];
                let new = &pixels[source + column * 4..][..4];
                match (nonzero_pixel(old), nonzero_pixel(new)) {
                    (false, true) => tile.nonzero += 1,
                    (true, false) => tile.nonzero = tile.nonzero.saturating_sub(1),
                    _ => {}
                }
            }
            tile.backup[target..target + row].copy_from_slice(&pixels[source..source + row]);
        }
        tile.stale = false;
    }

    /// A tile the shell never uploaded still has to exist before the stroke writes into it.
    fn ensure_commit_tile(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        coord: (u32, u32),
        tile_bytes: usize,
    ) -> bool {
        let Some(layer) = self.layers[index].as_mut() else {
            return false;
        };
        layer.tiles.entry(coord).or_insert_with(|| Tile {
            backup: vec![0; tile_bytes],
            nonzero: 0,
            gpu: None,
            used: 0,
            stale: false,
        });
        self.ensure_tile(device, queue, index, coord)
    }
}

impl LayerStack {
    /// Put a previous texture back and return the one it replaced, for redo.
    pub(crate) fn restore_swap(&mut self, swap: TileSwap) -> TileSwap {
        let current = self
            .layers
            .get_mut(swap.layer)
            .and_then(Option::as_mut)
            .and_then(|layer| layer.tiles.get_mut(&swap.coord))
            .and_then(|tile| {
                tile.stale = true;
                tile.gpu.replace(swap.old)
            })
            .expect("a committed tile stays resident");
        self.dirty_tiles.insert(swap.coord);
        TileSwap {
            coord: swap.coord,
            old: current,
            layer: swap.layer,
            bytes: swap.bytes,
        }
    }

    /// Texture bytes of one tile, for the journal budget.
    pub(crate) fn tile_bytes_hint(&self) -> usize {
        self.tile_bytes()
    }
}

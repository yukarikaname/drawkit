//! Composite a GPU stroke's coverage into the resident layer tiles, then read back only the tiles
//! the stroke touched. The CPU keeps the same pixels for history, export and replay; the brush is
//! never stamped again.

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
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
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

pub(crate) struct PendingTile {
    pub(crate) coord: (u32, u32),
    pub(crate) left: u32,
    pub(crate) top: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) padded: usize,
    pub(crate) staging: wgpu::Buffer,
}

impl LayerStack {
    /// Composite coverage into each tile of `index` that `rect` touches and return the new pixels.
    /// The tile textures are replaced rather than written in place, so the old tile can be sampled.
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
        progress: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Vec<PendingTile> {
        let (rx, ry, rw, rh) = rect;
        let right = (rx + rw).min(self.width);
        let bottom = (ry + rh).min(self.height);
        if rx >= right || ry >= bottom {
            return Vec::new();
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
        let mut pending: Vec<PendingTile> = Vec::new();
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
                    tile.gpu.replace(TileTexture {
                        texture: target.clone(),
                        view: target_view.clone(),
                        bytes: tile_bytes,
                    })
                });
            self.resident_bytes = self
                .resident_bytes
                .saturating_sub(replaced.map_or(0, |gpu| gpu.bytes))
                + tile_bytes;
            self.dirty_tiles.insert(coord);
            let padded = (width as usize * 4).div_ceil(256) * 256;
            let staging = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tile commit readback"),
                size: (padded * height as usize) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &target,
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
                        bytes_per_row: Some(padded as u32),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            pending.push(PendingTile {
                coord,
                left,
                top,
                width,
                height,
                padded,
                staging,
            });
        }
        if pending.is_empty() {
            return Vec::new();
        }
        queue.submit(Some(encoder.finish()));
        for tile in &pending {
            let done = progress.clone();
            tile.staging
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |_| {
                    done.fetch_add(1, std::sync::atomic::Ordering::Release);
                });
        }
        pending
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
        });
        self.ensure_tile(device, queue, index, coord)
    }

    /// Extract one readable tile and keep its backup in step with the new GPU texture.
    pub(crate) fn finish_tile(&mut self, index: usize, tile: PendingTile) -> Option<CommittedTile> {
        let mapped = tile.staging.slice(..).get_mapped_range().ok()?;
        let row = tile.width as usize * 4;
        let mut pixels = Vec::with_capacity(row * tile.height as usize);
        for line in 0..tile.height as usize {
            let start = line * tile.padded;
            pixels.extend_from_slice(&mapped[start..start + row]);
        }
        drop(mapped);
        tile.staging.unmap();
        let columns = self.tile_width as usize;
        let moved = self.layers.get_mut(index).and_then(Option::as_mut)?;
        let stored = moved.tiles.get_mut(&tile.coord)?;
        let local_x = (tile.left - tile.coord.0 * self.tile_width) as usize;
        let local_y = (tile.top - tile.coord.1 * self.tile_height) as usize;
        for line in 0..tile.height as usize {
            let target = ((local_y + line) * columns + local_x) * 4;
            let source = line * row;
            for column in 0..tile.width as usize {
                let old = &stored.backup[target + column * 4..][..4];
                let new = &pixels[source + column * 4..][..4];
                match (nonzero_pixel(old), nonzero_pixel(new)) {
                    (false, true) => stored.nonzero += 1,
                    (true, false) => stored.nonzero = stored.nonzero.saturating_sub(1),
                    _ => {}
                }
            }
            stored.backup[target..target + row].copy_from_slice(&pixels[source..source + row]);
        }
        Some(CommittedTile {
            x: tile.left,
            y: tile.top,
            width: tile.width,
            height: tile.height,
            pixels,
        })
    }
}

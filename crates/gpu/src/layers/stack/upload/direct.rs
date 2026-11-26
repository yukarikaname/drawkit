//! Uploads into a backup-less stack: the pixels go straight into the texture, and an evicted or
//! new tile is rebuilt while a full-tile zero upload drops it again.

use super::*;

/// The canvas rows and columns one tile covers, clipped at the right and bottom edges.
fn tile_extent(canvas: (u32, u32), tile: (u32, u32), coord: (u32, u32)) -> (u32, u32) {
    let (x, y) = (coord.0 * tile.0, coord.1 * tile.1);
    (tile.0.min(canvas.0 - x), tile.1.min(canvas.1 - y))
}

impl LayerStack {
    /// The backup-less twin of `set_pixels`: every painted tile becomes a texture right away.
    pub(super) fn set_pixels_direct(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        pixels: &[u8],
        bpp: usize,
    ) {
        self.layers[index] = Some(Layer {
            tiles: HashMap::new(),
            known: true,
        });
        let bytes = self.tile_bytes();
        let stride = self.width as usize * bpp;
        let tick = self.next_tick();
        let (canvas, tile) = (
            (self.width, self.height),
            (self.tile_width, self.tile_height),
        );
        for ty in 0..self.height.div_ceil(self.tile_height) {
            for tx in 0..self.width.div_ceil(self.tile_width) {
                let (width, height) = tile_extent(canvas, tile, (tx, ty));
                let start = ((ty * self.tile_height) as usize * self.width as usize
                    + (tx * self.tile_width) as usize)
                    * bpp;
                let mut nonzero = 0;
                for row in 0..height as usize {
                    let row_start = start + row * stride;
                    nonzero += (0..width as usize)
                        .filter(|column| pixel_nonzero(&pixels[row_start + column * bpp..][..bpp]))
                        .count();
                }
                if nonzero == 0 {
                    continue;
                }
                let texture = self.create_tile_texture(device, bytes);
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &pixels[start..],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(stride as u32),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
                let layer = self.layers[index].as_mut().expect("layer just inserted");
                layer.tiles.insert(
                    (tx, ty),
                    Tile {
                        backup: Vec::new(),
                        nonzero,
                        gpu: Some(texture),
                        used: tick,
                        stale: false,
                        missing: false,
                    },
                );
                self.resident_bytes += bytes;
                self.evict_cold(Some((tx, ty)));
            }
        }
    }

    /// The backup-less twin of the rectangle upload: an evicted or new tile gets a fresh texture,
    /// and a full-tile zero upload drops the tile so the layer stays sparse.
    pub(super) fn update_rect_direct(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        rect: (u32, u32, u32, u32),
        pixels: &[u8],
        bpp: usize,
    ) {
        let (x, y, width, height) = rect;
        let (tile_width, tile_height) = (self.tile_width, self.tile_height);
        let tile_bytes = self.tile_bytes();
        let tick = self.next_tick();
        let stride = width as usize * bpp;
        let right = x + width;
        let bottom = y + height;
        let canvas = (self.width, self.height);
        let tile = (tile_width, tile_height);
        let mut freed = 0;
        for ty in y / tile_height..=(bottom - 1) / tile_height {
            for tx in x / tile_width..=(right - 1) / tile_width {
                let left = x.max(tx * tile_width);
                let top = y.max(ty * tile_height);
                let clip_width = right.min((tx + 1) * tile_width) - left;
                let clip_height = bottom.min((ty + 1) * tile_height) - top;
                if clip_width == 0 || clip_height == 0 {
                    continue;
                }
                let key = (tx, ty);
                self.dirty_tiles.insert(key);
                let source = ((top - y) as usize * width as usize + (left - x) as usize) * bpp;
                let mut nonzero = 0;
                for row in 0..clip_height as usize {
                    let start = source + row * stride;
                    nonzero += (0..clip_width as usize)
                        .filter(|column| pixel_nonzero(&pixels[start + column * bpp..][..bpp]))
                        .count();
                }
                let (full_width, full_height) = tile_extent(canvas, tile, key);
                let full = left == tx * tile_width
                    && top == ty * tile_height
                    && clip_width == full_width
                    && clip_height == full_height;
                let resident = self.layers[index]
                    .as_ref()
                    .and_then(|layer| layer.tiles.get(&key))
                    .is_some_and(|tile| tile.gpu.is_some());
                let pinned = self.pinned.contains(&(index, key));
                if resident && full && nonzero == 0 && !pinned {
                    // The core surface says this whole tile is empty now.
                    if let Some(tile) = self.layers[index]
                        .as_mut()
                        .and_then(|layer| layer.tiles.remove(&key))
                    {
                        freed += tile.gpu.map_or(0, |gpu| gpu.bytes);
                    }
                    continue;
                }
                if !resident {
                    if nonzero == 0 {
                        // An evicted tile that is empty: forget it instead of allocating.
                        if !pinned && let Some(layer) = self.layers[index].as_mut() {
                            layer.tiles.remove(&key);
                        }
                        continue;
                    }
                    let texture = self.create_tile_texture(device, tile_bytes);
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture.texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: left - tx * tile_width,
                                y: top - ty * tile_height,
                                z: 0,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        &pixels[source..],
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(stride as u32),
                            rows_per_image: Some(clip_height),
                        },
                        wgpu::Extent3d {
                            width: clip_width,
                            height: clip_height,
                            depth_or_array_layers: 1,
                        },
                    );
                    let layer = self.layers[index].as_mut().expect("layer just inserted");
                    layer.tiles.insert(
                        key,
                        Tile {
                            backup: Vec::new(),
                            nonzero,
                            gpu: Some(texture),
                            used: tick,
                            stale: false,
                            missing: false,
                        },
                    );
                    self.resident_bytes += tile_bytes;
                    self.evict_cold(Some(key));
                    continue;
                }
                let layer = self.layers[index].as_mut().expect("layer just inserted");
                let Some(tile) = layer.tiles.get_mut(&key) else {
                    continue;
                };
                tile.used = tick;
                tile.missing = false;
                if let Some(gpu) = &tile.gpu {
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &gpu.texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: left - tx * tile_width,
                                y: top - ty * tile_height,
                                z: 0,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        &pixels[source..],
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(stride as u32),
                            rows_per_image: Some(clip_height),
                        },
                        wgpu::Extent3d {
                            width: clip_width,
                            height: clip_height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
        }
        self.resident_bytes = self.resident_bytes.saturating_sub(freed);
    }
}

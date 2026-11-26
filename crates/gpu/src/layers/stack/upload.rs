//! Tile uploads. An eight-bit document sends straight RGBA8; an extended-range one sends
//! straight linear Display P3 floats, which are stored as half floats.
//!
//! A backup-less stack (the live canvas) writes the pixels straight into the texture: there is no
//! CPU copy, so an upload of an evicted tile recreates its texture, and a full-tile zero upload
//! drops the tile again.

mod direct;

use super::*;

fn pixel_nonzero(pixel: &[u8]) -> bool {
    pixel.iter().any(|byte| *byte != 0)
}

/// Straight linear floats to the half-float bytes a tile holds.
fn linear_to_half(pixels: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() * 2);
    for value in pixels {
        out.extend_from_slice(&half::f16::from_f32(*value).to_bits().to_le_bytes());
    }
    out
}

impl LayerStack {
    /// Upload one layer's straight RGBA8 pixels. Only tiles with a non-zero byte are kept; a
    /// backup-less stack creates their textures right away, a backup stack lets the compositor
    /// materialise them lazily.
    pub fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        pixels: &[u8],
    ) -> bool {
        if self.format != crate::FrameFormat::Rgba8 {
            return false;
        }
        self.set_pixels(device, queue, index, pixels, 4)
    }

    /// The same for straight linear Display P3 floats on an extended-range stack.
    pub fn set_linear(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        pixels: &[f32],
    ) -> bool {
        if self.format == crate::FrameFormat::Rgba8 {
            return false;
        }
        let bytes = linear_to_half(pixels);
        self.set_pixels(device, queue, index, &bytes, 8)
    }

    fn set_pixels(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        pixels: &[u8],
        bpp: usize,
    ) -> bool {
        let expected = self.width as usize * self.height as usize * bpp;
        if self.width == 0 || self.height == 0 || pixels.len() < expected {
            return false;
        }
        if self.layers.len() <= index {
            self.layers.resize_with(index + 1, || None);
        }
        self.drop_layer(index);
        self.mark_all_dirty();
        if !self.backups {
            self.set_pixels_direct(device, queue, index, pixels, bpp);
            return true;
        }
        let tile_width = self.tile_width as usize;
        let tile_height = self.tile_height as usize;
        let tile_bytes = tile_width * tile_height * bpp;
        let mut layer = Layer {
            tiles: HashMap::new(),
            known: true,
        };
        for ty in 0..self.height.div_ceil(self.tile_height) {
            for tx in 0..self.width.div_ceil(self.tile_width) {
                let origin_x = tx * self.tile_width;
                let origin_y = ty * self.tile_height;
                let width = (self.width - origin_x).min(self.tile_width) as usize;
                let height = (self.height - origin_y).min(self.tile_height) as usize;
                let mut backup = vec![0u8; tile_bytes];
                let mut nonzero = 0;
                for row in 0..height {
                    let source =
                        ((origin_y as usize + row) * self.width as usize + origin_x as usize) * bpp;
                    let target = row * tile_width * bpp;
                    backup[target..target + width * bpp]
                        .copy_from_slice(&pixels[source..source + width * bpp]);
                    nonzero += (0..width)
                        .filter(|column| pixel_nonzero(&pixels[source + column * bpp..][..bpp]))
                        .count();
                }
                if nonzero > 0 {
                    layer.tiles.insert(
                        (tx, ty),
                        Tile {
                            backup,
                            nonzero,
                            gpu: None,
                            used: 0,
                            stale: false,
                            missing: false,
                        },
                    );
                }
            }
        }
        self.layers[index] = Some(layer);
        true
    }

    /// Upload one dirty rectangle of an eight-bit layer. Resident GPU tiles are written in place;
    /// a cold tile keeps the new pixels in its backup, or recreates its texture when the stack
    /// keeps no backups.
    pub fn update_rect(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        request: RectUpload<'_>,
    ) -> bool {
        if self.format != crate::FrameFormat::Rgba8 {
            return false;
        }
        self.update_rect_bytes(device, queue, request, 4)
    }

    /// The same for premultiplied linear floats on an extended-range stack.
    pub fn update_rect_linear(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        request: LinearUpload<'_>,
    ) -> bool {
        if self.format == crate::FrameFormat::Rgba8 {
            return false;
        }
        let bytes = linear_to_half(request.pixels);
        let request = RectUpload {
            index: request.index,
            x: request.x,
            y: request.y,
            width: request.width,
            height: request.height,
            pixels: &bytes,
        };
        self.update_rect_bytes(device, queue, request, 8)
    }

    fn update_rect_bytes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        request: RectUpload<'_>,
        bpp: usize,
    ) -> bool {
        let RectUpload {
            index,
            x,
            y,
            width,
            height,
            pixels,
        } = request;
        if width == 0
            || height == 0
            || x + width > self.width
            || y + height > self.height
            || pixels.len() < (width * height) as usize * bpp
        {
            return false;
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
        if !self.backups {
            self.update_rect_direct(device, queue, index, (x, y, width, height), pixels, bpp);
            return true;
        }
        let tile_width = self.tile_width;
        let tile_height = self.tile_height;
        let tile_columns = tile_width as usize;
        let tile_bytes = self.tile_bytes();
        let tick = self.next_tick();
        let stride = width as usize * bpp;
        let right = x + width;
        let bottom = y + height;
        let layer = self.layers[index].as_mut().expect("layer just inserted");
        layer.known = true;
        let mut freed = 0;
        for ty in y / tile_height..=(bottom - 1) / tile_height {
            for tx in x / tile_width..=(right - 1) / tile_width {
                let left = x.max(tx * tile_width);
                let top = y.max(ty * tile_height);
                let limit_x = right.min((tx + 1) * tile_width);
                let limit_y = bottom.min((ty + 1) * tile_height);
                if limit_x <= left || limit_y <= top {
                    continue;
                }
                let clip_width = (limit_x - left) as usize;
                let clip_height = (limit_y - top) as usize;
                self.dirty_tiles.insert((tx, ty));
                let key = (tx, ty);
                let remove;
                {
                    let tile = layer.tiles.entry(key).or_insert_with(|| Tile {
                        backup: vec![0; tile_bytes],
                        nonzero: 0,
                        gpu: None,
                        used: 0,
                        stale: false,
                        missing: false,
                    });
                    tile.stale = false;
                    for row in 0..clip_height {
                        let source = ((row + (top - y) as usize) * width as usize
                            + (left - x) as usize)
                            * bpp;
                        let local_y = (top - ty * tile_height) as usize + row;
                        let target =
                            (local_y * tile_columns + (left - tx * tile_width) as usize) * bpp;
                        for column in 0..clip_width {
                            let old = &tile.backup[target + column * bpp..][..bpp];
                            let new = &pixels[source + column * bpp..][..bpp];
                            match (pixel_nonzero(old), pixel_nonzero(new)) {
                                (false, true) => tile.nonzero += 1,
                                (true, false) => tile.nonzero -= 1,
                                _ => {}
                            }
                        }
                        tile.backup[target..target + clip_width * bpp]
                            .copy_from_slice(&pixels[source..source + clip_width * bpp]);
                    }
                    if let Some(gpu) = &tile.gpu {
                        tile.used = tick;
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
                            &pixels[((top - y) as usize * width as usize + (left - x) as usize)
                                * bpp..],
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(stride as u32),
                                rows_per_image: Some(clip_height as u32),
                            },
                            wgpu::Extent3d {
                                width: clip_width as u32,
                                height: clip_height as u32,
                                depth_or_array_layers: 1,
                            },
                        );
                    }
                    remove = tile.nonzero == 0;
                }
                if remove
                    && let Some(tile) = layer.tiles.remove(&key)
                    && let Some(gpu) = tile.gpu
                {
                    freed += gpu.bytes;
                }
            }
        }
        self.resident_bytes = self.resident_bytes.saturating_sub(freed);
        true
    }
}

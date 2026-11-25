//! Applying a fill mask to the resident eight-bit tiles.
//!
//! The mask comes from the core's flood/close/edge pipeline; what moves to the GPU side here is the
//! per-pixel write. The tile backup is updated and its texture dropped, so the next composite
//! rehydrates exactly the new pixels. An extended-range stack is not supported: the replay driver
//! puts HDR on the CPU path.

use super::*;

impl LayerStack {
    /// Blend `color` through `mask` into layer `index`. The mask is one float per canvas pixel.
    pub fn apply_fill_mask(
        &mut self,
        index: usize,
        mask: &[f32],
        color: [f32; 4],
        alpha_locked: bool,
    ) -> bool {
        let pixels = self.width as usize * self.height as usize;
        if self.format != crate::composite::FORMAT
            || self.width == 0
            || self.height == 0
            || mask.len() < pixels
            || self.layers.len() <= index
        {
            return false;
        }
        if self.layers[index].is_none() {
            self.layers[index] = Some(Layer {
                tiles: HashMap::new(),
                known: true,
            });
        }
        let tile_width = self.tile_width;
        let tile_height = self.tile_height;
        let tile_bytes = self.tile_bytes();
        let tick = self.next_tick();
        let layer = self.layers[index].as_mut().expect("layer just inserted");
        let mut changed = false;
        for ty in 0..self.height.div_ceil(tile_height) {
            for tx in 0..self.width.div_ceil(tile_width) {
                let left = tx * tile_width;
                let top = ty * tile_height;
                let right = (left + tile_width).min(self.width);
                let bottom = (top + tile_height).min(self.height);
                let mut touched = false;
                for y in top..bottom {
                    for x in left..right {
                        if mask[y as usize * self.width as usize + x as usize] > 0.0 {
                            touched = true;
                            break;
                        }
                    }
                    if touched {
                        break;
                    }
                }
                if !touched {
                    continue;
                }
                let coord = (tx, ty);
                let tile = layer.tiles.entry(coord).or_insert_with(|| Tile {
                    backup: vec![0; tile_bytes],
                    nonzero: 0,
                    gpu: None,
                    used: 0,
                    stale: false,
                });
                tile.used = tick;
                for y in top..bottom {
                    for x in left..right {
                        let amount = mask[y as usize * self.width as usize + x as usize];
                        if amount <= 0.0 {
                            continue;
                        }
                        let local_x = (x - left) as usize;
                        let local_y = (y - top) as usize;
                        let at = (local_y * tile_width as usize + local_x) * 4;
                        let old = [
                            tile.backup[at] as f32 / 255.0,
                            tile.backup[at + 1] as f32 / 255.0,
                            tile.backup[at + 2] as f32 / 255.0,
                            tile.backup[at + 3] as f32 / 255.0,
                        ];
                        let before = old[3] > 0.0;
                        let next = if alpha_locked {
                            if old[3] <= 0.0 {
                                old
                            } else {
                                let alpha = (amount * color[3].clamp(0.0, 1.0)).clamp(0.0, 1.0);
                                [
                                    old[0] + (color[0] - old[0]) * alpha,
                                    old[1] + (color[1] - old[1]) * alpha,
                                    old[2] + (color[2] - old[2]) * alpha,
                                    old[3],
                                ]
                            }
                        } else {
                            let alpha = (amount * color[3].clamp(0.0, 1.0)).clamp(0.0, 1.0);
                            let out_alpha = alpha + old[3] * (1.0 - alpha);
                            if out_alpha <= 0.0 {
                                [0.0; 4]
                            } else {
                                [
                                    (color[0] * alpha + old[0] * old[3] * (1.0 - alpha))
                                        / out_alpha,
                                    (color[1] * alpha + old[1] * old[3] * (1.0 - alpha))
                                        / out_alpha,
                                    (color[2] * alpha + old[2] * old[3] * (1.0 - alpha))
                                        / out_alpha,
                                    out_alpha,
                                ]
                            }
                        };
                        let after = next[3] > 0.0;
                        if before != after {
                            if after {
                                tile.nonzero += 1;
                            } else {
                                tile.nonzero = tile.nonzero.saturating_sub(1);
                            }
                        }
                        for channel in 0..4 {
                            tile.backup[at + channel] =
                                (next[channel].clamp(0.0, 1.0) * 255.0).round() as u8;
                        }
                        changed = true;
                    }
                }
                tile.stale = false;
                if let Some(gpu) = tile.gpu.take() {
                    self.resident_bytes = self.resident_bytes.saturating_sub(gpu.bytes);
                }
                self.dirty_tiles.insert(coord);
            }
        }
        changed
    }
}

impl LayerStack {
    /// The half-float twin of `apply_fill_mask`, used by the extended-range replay.
    pub fn apply_fill_mask_linear(
        &mut self,
        index: usize,
        mask: &[f32],
        color: [f32; 4],
        alpha_locked: bool,
    ) -> bool {
        let pixels = self.width as usize * self.height as usize;
        if self.format == crate::composite::FORMAT
            || self.width == 0
            || self.height == 0
            || mask.len() < pixels
            || self.layers.len() <= index
        {
            return false;
        }
        if self.layers[index].is_none() {
            self.layers[index] = Some(Layer {
                tiles: HashMap::new(),
                known: true,
            });
        }
        let tile_width = self.tile_width;
        let tile_height = self.tile_height;
        let tile_bytes = self.tile_bytes();
        let tick = self.next_tick();
        let layer = self.layers[index].as_mut().expect("layer just inserted");
        let mut changed = false;
        for ty in 0..self.height.div_ceil(tile_height) {
            for tx in 0..self.width.div_ceil(tile_width) {
                let left = tx * tile_width;
                let top = ty * tile_height;
                let right = (left + tile_width).min(self.width);
                let bottom = (top + tile_height).min(self.height);
                let mut touched = false;
                for y in top..bottom {
                    for x in left..right {
                        if mask[y as usize * self.width as usize + x as usize] > 0.0 {
                            touched = true;
                            break;
                        }
                    }
                    if touched {
                        break;
                    }
                }
                if !touched {
                    continue;
                }
                let coord = (tx, ty);
                let tile = layer.tiles.entry(coord).or_insert_with(|| Tile {
                    backup: vec![0; tile_bytes],
                    nonzero: 0,
                    gpu: None,
                    used: 0,
                    stale: false,
                });
                tile.used = tick;
                for y in top..bottom {
                    for x in left..right {
                        let amount = mask[y as usize * self.width as usize + x as usize];
                        if amount <= 0.0 {
                            continue;
                        }
                        let at = (((y - top) * tile_width + (x - left)) * 8) as usize;
                        let mut old = [0.0f32; 4];
                        for (channel, value) in old.iter_mut().enumerate() {
                            let bits = u16::from_le_bytes([
                                tile.backup[at + channel * 2],
                                tile.backup[at + channel * 2 + 1],
                            ]);
                            *value = half::f16::from_bits(bits).to_f32();
                        }
                        let before = old[3] > 0.0;
                        let next = if alpha_locked {
                            if old[3] <= 0.0 {
                                old
                            } else {
                                let alpha = (amount * color[3].clamp(0.0, 1.0)).clamp(0.0, 1.0);
                                [
                                    old[0] + (color[0] - old[0]) * alpha,
                                    old[1] + (color[1] - old[1]) * alpha,
                                    old[2] + (color[2] - old[2]) * alpha,
                                    old[3],
                                ]
                            }
                        } else {
                            let alpha = (amount * color[3].clamp(0.0, 1.0)).clamp(0.0, 1.0);
                            let out_alpha = alpha + old[3] * (1.0 - alpha);
                            if out_alpha <= 0.0 {
                                [0.0; 4]
                            } else {
                                [
                                    (color[0] * alpha + old[0] * old[3] * (1.0 - alpha))
                                        / out_alpha,
                                    (color[1] * alpha + old[1] * old[3] * (1.0 - alpha))
                                        / out_alpha,
                                    (color[2] * alpha + old[2] * old[3] * (1.0 - alpha))
                                        / out_alpha,
                                    out_alpha,
                                ]
                            }
                        };
                        let after = next[3] > 0.0;
                        if before != after {
                            if after {
                                tile.nonzero += 1;
                            } else {
                                tile.nonzero = tile.nonzero.saturating_sub(1);
                            }
                        }
                        for channel in 0..4 {
                            let bits = half::f16::from_f32(next[channel].max(0.0)).to_bits();
                            tile.backup[at + channel * 2..at + channel * 2 + 2]
                                .copy_from_slice(&bits.to_le_bytes());
                        }
                        changed = true;
                    }
                }
                tile.stale = false;
                if let Some(gpu) = tile.gpu.take() {
                    self.resident_bytes = self.resident_bytes.saturating_sub(gpu.bytes);
                }
                self.dirty_tiles.insert(coord);
            }
        }
        changed
    }
}

//! Applying a fill mask to the resident tiles.
//!
//! The mask comes from the core's flood/close/edge pipeline. A live fill blends it on the GPU
//! (`commit`), one render pass per touched tile with the replaced texture as the undo record; the
//! tile backup is left stale and refreshed by `materialize`. `apply_fill_mask` is the CPU twin the
//! offscreen replay compositor still uses: it writes straight into the backups and drops the tile
//! textures so the next composite rehydrates them. An extended-range stack is handled by
//! `apply_fill_mask_linear` for the replay and by the same GPU shader for the live path.

use super::*;

mod commit;
#[cfg(test)]
mod tests;

impl LayerStack {
    /// Blend `color` through `mask` into layer `index`. The mask is one float per canvas pixel.
    pub fn apply_fill_mask(
        &mut self,
        index: usize,
        mask: &[f32],
        color: [f32; 4],
        alpha_locked: bool,
        under: bool,
    ) -> bool {
        let pixels = self.width as usize * self.height as usize;
        if self.format != crate::FrameFormat::Rgba8
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
        let fill_alpha = color[3].clamp(0.0, 1.0);
        let opaque_bytes = [
            (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
            255,
        ];
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
                        let alpha = (amount * fill_alpha).clamp(0.0, 1.0);
                        let old_alpha = tile.backup[at + 3] as f32 / 255.0;
                        let before = old_alpha > 0.0;
                        // An opaque write over the pixel, or under an empty one, is the fill
                        // colour whatever the old pixels were. Skipping their decode is what
                        // makes a full-canvas fill cheap; the result is bit-identical because the
                        // general expression divides by an alpha of exactly one.
                        if alpha >= 1.0 && !alpha_locked && (!under || old_alpha <= 0.0) {
                            if !before {
                                tile.nonzero += 1;
                            }
                            tile.backup[at..at + 4].copy_from_slice(&opaque_bytes);
                            changed = true;
                            continue;
                        }
                        let next = {
                            let old = [
                                tile.backup[at] as f32 / 255.0,
                                tile.backup[at + 1] as f32 / 255.0,
                                tile.backup[at + 2] as f32 / 255.0,
                                old_alpha,
                            ];
                            if alpha_locked {
                                if old[3] <= 0.0 {
                                    old
                                } else {
                                    [
                                        old[0] + (color[0] - old[0]) * alpha,
                                        old[1] + (color[1] - old[1]) * alpha,
                                        old[2] + (color[2] - old[2]) * alpha,
                                        old[3],
                                    ]
                                }
                            } else if under {
                                if alpha >= 1.0 && old[3] >= 1.0 {
                                    // Going under an opaque pixel changes nothing.
                                    old
                                } else {
                                    let out_alpha = old[3] + alpha * (1.0 - old[3]);
                                    if out_alpha <= 0.0 {
                                        [0.0; 4]
                                    } else {
                                        [
                                            (old[0] * old[3] + color[0] * alpha * (1.0 - old[3]))
                                                / out_alpha,
                                            (old[1] * old[3] + color[1] * alpha * (1.0 - old[3]))
                                                / out_alpha,
                                            (old[2] * old[3] + color[2] * alpha * (1.0 - old[3]))
                                                / out_alpha,
                                            out_alpha,
                                        ]
                                    }
                                }
                            } else {
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

    /// One layer pixel's alpha from its CPU backup, used to decide whether a replayed fill goes
    /// under the existing line or over an opaque region.
    pub fn layer_alpha(&self, index: usize, x: u32, y: u32) -> Option<f32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let layer = self.layers.get(index)?.as_ref()?;
        let coord = (x / self.tile_width, y / self.tile_height);
        let tile = layer.tiles.get(&coord)?;
        let local = ((y % self.tile_height) * self.tile_width + x % self.tile_width) as usize;
        match self.format {
            crate::FrameFormat::Rgba8 => Some(tile.backup[local * 4 + 3] as f32 / 255.0),
            _ => {
                let at = local * 8 + 6;
                let bits = u16::from_le_bytes([tile.backup[at], tile.backup[at + 1]]);
                Some(half::f16::from_bits(bits).to_f32())
            }
        }
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
        under: bool,
    ) -> bool {
        let pixels = self.width as usize * self.height as usize;
        if self.format == crate::FrameFormat::Rgba8
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
        let fill_alpha = color[3].clamp(0.0, 1.0);
        let mut opaque_half = [0u8; 8];
        for (channel, value) in [color[0], color[1], color[2], 1.0].iter().enumerate() {
            let bits = half::f16::from_f32((*value).max(0.0)).to_bits();
            opaque_half[channel * 2..channel * 2 + 2].copy_from_slice(&bits.to_le_bytes());
        }
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
                        let alpha = (amount * fill_alpha).clamp(0.0, 1.0);
                        let old_alpha = half::f16::from_bits(u16::from_le_bytes([
                            tile.backup[at + 6],
                            tile.backup[at + 7],
                        ]))
                        .to_f32();
                        let before = old_alpha > 0.0;
                        if alpha >= 1.0 && !alpha_locked && (!under || old_alpha <= 0.0) {
                            if !before {
                                tile.nonzero += 1;
                            }
                            tile.backup[at..at + 8].copy_from_slice(&opaque_half);
                            changed = true;
                            continue;
                        }
                        let next = {
                            let mut old = [0.0f32; 4];
                            for (channel, value) in old.iter_mut().enumerate() {
                                let bits = u16::from_le_bytes([
                                    tile.backup[at + channel * 2],
                                    tile.backup[at + channel * 2 + 1],
                                ]);
                                *value = half::f16::from_bits(bits).to_f32();
                            }
                            if alpha_locked {
                                if old[3] <= 0.0 {
                                    old
                                } else {
                                    [
                                        old[0] + (color[0] - old[0]) * alpha,
                                        old[1] + (color[1] - old[1]) * alpha,
                                        old[2] + (color[2] - old[2]) * alpha,
                                        old[3],
                                    ]
                                }
                            } else if under {
                                if alpha >= 1.0 && old[3] >= 1.0 {
                                    old
                                } else {
                                    let out_alpha = old[3] + alpha * (1.0 - old[3]);
                                    if out_alpha <= 0.0 {
                                        [0.0; 4]
                                    } else {
                                        [
                                            (old[0] * old[3] + color[0] * alpha * (1.0 - old[3]))
                                                / out_alpha,
                                            (old[1] * old[3] + color[1] * alpha * (1.0 - old[3]))
                                                / out_alpha,
                                            (old[2] * old[3] + color[2] * alpha * (1.0 - old[3]))
                                                / out_alpha,
                                            out_alpha,
                                        ]
                                    }
                                }
                            } else {
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

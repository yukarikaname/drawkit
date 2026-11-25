//! Stroke-level coverage: overlap between pointer samples must not darken the join.

use super::*;

impl Surface {
    /// Start accumulating a brush stroke. Every segment updates one coverage map and recomputes
    /// the touched pixels from the pre-stroke pixels, so overlapping segments cannot darken the
    /// join between two pointer samples.
    pub fn set_brush_texture(
        &mut self,
        texture: Option<std::sync::Arc<crate::brush::BrushTexture>>,
    ) {
        self.brush_texture = texture;
    }

    pub fn brush_texture(&self) -> Option<&crate::brush::BrushTexture> {
        self.brush_texture.as_deref()
    }

    pub fn brush_texture_shared(&self) -> Option<std::sync::Arc<crate::brush::BrushTexture>> {
        self.brush_texture.clone()
    }

    pub fn begin_stroke_accumulation(&mut self) {
        self.stroke = Some(std::collections::HashMap::new());
        self.stroke_wet = 0.0;
    }

    pub fn end_stroke_accumulation(&mut self) {
        self.stroke = None;
        self.stroke_wet = 0.0;
    }

    /// Pick up existing paint under the next segments. One is a full colour transfer, zero leaves
    /// the picked colour alone.
    pub fn set_stroke_wet(&mut self, wet: f32) {
        self.stroke_wet = wet.clamp(0.0, 1.0);
    }

    /// A mask that belongs to a stroke already started with `begin_stroke_accumulation`. Without
    /// that start it falls back to the old per-segment blend.
    pub fn blend_stroke_mask_at(
        &mut self,
        mask: &crate::fill::Mask,
        x: usize,
        y: usize,
        color: Rgba,
        mode: StrokeMask,
    ) {
        if self.stroke.is_none() {
            match mode {
                StrokeMask::Paint => self.blend_mask_at(mask, x, y, color),
                StrokeMask::Locked => self.blend_mask_locked_at(mask, x, y, color),
                StrokeMask::Erase => self.erase_mask_at(mask, x, y),
            }
            return;
        }
        if x >= self.width || y >= self.height {
            return;
        }
        let width = mask.width.min(self.width - x);
        let height = mask.height.min(self.height - y);
        if width == 0 || height == 0 {
            return;
        }
        let mut delta = 0_isize;
        for my in 0..height {
            for mx in 0..width {
                let amount = mask.get(mx, my).clamp(0.0, 1.0);
                if amount <= 0.0 {
                    continue;
                }
                let px = x + mx;
                let py = y + my;
                let index = self.tile_of(px, py);
                if !self
                    .stroke
                    .as_ref()
                    .is_some_and(|tiles| tiles.contains_key(&index))
                {
                    let base = self.tiles.get(index).and_then(Option::as_ref).map_or_else(
                        || vec![[0.0; 4]; TILE * TILE].into_boxed_slice(),
                        |tile| tile.pixels.clone(),
                    );
                    if let Some(tiles) = self.stroke.as_mut() {
                        tiles.insert(
                            index,
                            StrokeTile {
                                base,
                                coverage: vec![0.0; TILE * TILE].into_boxed_slice(),
                            },
                        );
                    }
                }
                let Some(stroke) = self.stroke.as_mut() else {
                    continue;
                };
                let Some(accumulated) = stroke.get_mut(&index) else {
                    continue;
                };
                let local = (py % TILE) * TILE + px % TILE;
                let coverage = match mode {
                    StrokeMask::Erase => amount,
                    _ => amount * color[3].clamp(0.0, 1.0),
                };
                accumulated.coverage[local] = accumulated.coverage[local].max(coverage);
                let coverage = accumulated.coverage[local];
                let base = accumulated.base[local];
                let mut ink = color;
                if self.stroke_wet > 0.0 && base[3] > 0.0 {
                    for channel in 0..3 {
                        ink[channel] = base[channel] * self.stroke_wet
                            + color[channel] * (1.0 - self.stroke_wet);
                    }
                }
                let tile = self.tiles[index].get_or_insert_with(Tile::new);
                let slot = &mut tile.pixels[local];
                let before = slot[3] > 0.0;
                *slot = match mode {
                    StrokeMask::Paint => over([ink[0], ink[1], ink[2], coverage], base),
                    StrokeMask::Locked => {
                        if base[3] <= 0.0 {
                            base
                        } else {
                            let mut pixel = base;
                            for channel in 0..3 {
                                pixel[channel] += (ink[channel] - base[channel]) * coverage;
                            }
                            pixel
                        }
                    }
                    StrokeMask::Erase => {
                        let alpha = base[3] * (1.0 - coverage);
                        if alpha <= 1e-4 {
                            [0.0; 4]
                        } else {
                            [base[0], base[1], base[2], alpha]
                        }
                    }
                };
                let after = slot[3] > 0.0;
                if before != after {
                    delta += if after { 1 } else { -1 };
                }
            }
        }
        self.inked = (self.inked as isize + delta) as usize;
        self.mark(Rect::new(x, y, width, height));
    }
}

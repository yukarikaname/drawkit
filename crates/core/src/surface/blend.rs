//! Compositing a coverage mask into a layer surface.

use super::*;

impl Surface {
    /// Composite `color` through a coverage mask, which is what a fill or a soft brush does.
    pub fn blend_mask(&mut self, mask: &crate::fill::Mask, color: Rgba) {
        self.blend_mask_at(mask, 0, 0, color);
    }

    /// The same, with the mask's top-left at `(x, y)`. The brush builds a mask the size of one
    /// segment, not the size of the canvas.
    pub fn blend_mask_at(&mut self, mask: &crate::fill::Mask, x: usize, y: usize, color: Rgba) {
        if x >= self.width || y >= self.height {
            return;
        }
        let width = mask.width.min(self.width - x);
        let height = mask.height.min(self.height - y);
        let mut delta = 0_isize;
        for my in 0..height {
            for mx in 0..width {
                let amount = mask.get(mx, my);
                if amount <= 0.0 {
                    continue;
                }
                let px = x + mx;
                let py = y + my;
                let index = self.tile_of(px, py);
                let tile = self.tiles[index].get_or_insert_with(Tile::new);
                let slot = &mut tile.pixels[(py % TILE) * TILE + px % TILE];
                let before = slot[3] > 0.0;
                *slot = over([color[0], color[1], color[2], color[3] * amount], *slot);
                let after = slot[3] > 0.0;
                if before != after {
                    delta += if after { 1 } else { -1 };
                }
            }
        }
        self.inked = (self.inked as isize + delta) as usize;
        self.mark(Rect::new(x, y, width, height));
        self.bump_tiles(Rect::new(x, y, width, height));
    }

    /// Put `color` **under** the mask's region, keeping whatever is already there. A fill on a
    /// transparent paper layer uses this so the line keeps its anti-aliased edge and the fill
    /// reaches under it instead of leaving a pale rim.
    pub fn blend_mask_under_at(
        &mut self,
        mask: &crate::fill::Mask,
        x: usize,
        y: usize,
        color: Rgba,
    ) {
        if x >= self.width || y >= self.height {
            return;
        }
        let width = mask.width.min(self.width - x);
        let height = mask.height.min(self.height - y);
        let mut delta = 0_isize;
        for my in 0..height {
            for mx in 0..width {
                let amount = mask.get(mx, my);
                if amount <= 0.0 {
                    continue;
                }
                let px = x + mx;
                let py = y + my;
                let index = self.tile_of(px, py);
                let tile = self.tiles[index].get_or_insert_with(Tile::new);
                let slot = &mut tile.pixels[(py % TILE) * TILE + px % TILE];
                let before = slot[3] > 0.0;
                *slot = over(*slot, [color[0], color[1], color[2], color[3] * amount]);
                let after = slot[3] > 0.0;
                if before != after {
                    delta += if after { 1 } else { -1 };
                }
            }
        }
        self.inked = (self.inked as isize + delta) as usize;
        self.mark(Rect::new(x, y, width, height));
        self.bump_tiles(Rect::new(x, y, width, height));
    }

    /// Blend colour through a coverage mask without changing alpha, for an alpha-locked layer.
    /// Transparent pixels stay transparent; a semi-transparent pixel keeps its transparency and
    /// only its colour moves.
    pub fn blend_mask_locked_at(
        &mut self,
        mask: &crate::fill::Mask,
        x: usize,
        y: usize,
        color: Rgba,
    ) {
        if x >= self.width || y >= self.height {
            return;
        }
        let width = mask.width.min(self.width - x);
        let height = mask.height.min(self.height - y);
        for my in 0..height {
            for mx in 0..width {
                let amount = mask.get(mx, my);
                if amount <= 0.0 {
                    continue;
                }
                let px = x + mx;
                let py = y + my;
                let index = self.tile_of(px, py);
                let tile = self.tiles[index].get_or_insert_with(Tile::new);
                let slot = &mut tile.pixels[(py % TILE) * TILE + px % TILE];
                if slot[3] <= 0.0 {
                    continue;
                }
                let amount = (amount * color[3].clamp(0.0, 1.0)).clamp(0.0, 1.0);
                for channel in 0..3 {
                    slot[channel] += (color[channel] - slot[channel]) * amount;
                }
            }
        }
        self.mark(Rect::new(x, y, width, height));
        self.bump_tiles(Rect::new(x, y, width, height));
    }
}

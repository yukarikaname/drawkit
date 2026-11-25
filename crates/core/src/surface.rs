//! A canvas made of tiles, because most of a canvas is empty.
//!
//! An 8K layer is 33 million pixels and 500 MB at four floats a pixel. Almost none of it is drawn
//! on, and every stroke touches a few thousand pixels. Tiles make the cost follow the paint instead
//! of the canvas: a tile is allocated the first time something is written to it, and the dirty
//! rectangle is what the next upload and the next undo step are about.

use std::collections::HashMap;

use crate::color::{Rgba, over};

mod stroke;

pub const TILE: usize = 256;

/// A rectangle in pixels, half-open on the right and bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    pub fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// The smallest rectangle holding both. This is what one dirty rectangle per surface means:
    /// two distant strokes make one large box, and the alternative is a list that grows forever.
    pub fn union(&self, other: &Rect) -> Rect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Rect::new(x, y, right - x, bottom - y)
    }
}

#[derive(Clone)]
struct Tile {
    pixels: Box<[Rgba]>,
}

impl Tile {
    fn new() -> Self {
        Self {
            pixels: vec![[0.0; 4]; TILE * TILE].into_boxed_slice(),
        }
    }
}

#[derive(Clone)]
pub struct Surface {
    width: usize,
    height: usize,
    tiles_x: usize,
    tiles: Vec<Option<Tile>>,
    dirty: Option<Rect>,
    /// The dirty rectangle consumed by the GPU canvas, separate from the CPU composite's one.
    gpu_dirty: Option<Rect>,
    /// Pixels whose alpha is above zero. Keeping this count means "is the canvas blank?" is a
    /// comparison, not a walk over every allocated tile after every stroke.
    inked: usize,
    /// Pre-stroke pixels and accumulated coverage for the stroke in progress.
    stroke: Option<HashMap<usize, StrokeTile>>,
    /// How much existing paint the current stroke picks up.
    stroke_wet: f32,
    /// A texture map shared with the brush for this stroke.
    brush_texture: Option<std::sync::Arc<crate::brush::BrushTexture>>,
}

#[derive(Clone)]
struct StrokeTile {
    base: Box<[Rgba]>,
    coverage: Box<[f32]>,
}

#[derive(Clone, Copy, Debug)]
pub enum StrokeMask {
    Paint,
    Locked,
    Erase,
}

impl Surface {
    pub fn new(width: usize, height: usize) -> Self {
        let tiles_x = width.div_ceil(TILE);
        let tiles_y = height.div_ceil(TILE);
        Self {
            width,
            height,
            tiles_x,
            tiles: (0..tiles_x * tiles_y).map(|_| None).collect(),
            dirty: None,
            gpu_dirty: None,
            inked: 0,
            stroke: None,
            stroke_wet: 0.0,
            brush_texture: None,
        }
    }

    pub fn is_clear(&self) -> bool {
        self.inked == 0
    }

    /// The rectangle covered by allocated tiles. A project cache only needs to store this, not
    /// the empty margins of an 8K canvas.
    pub fn painted_bounds(&self) -> Option<Rect> {
        let tiles_y = self.tiles.len() / self.tiles_x.max(1);
        let mut bounds: Option<Rect> = None;
        for ty in 0..tiles_y {
            for tx in 0..self.tiles_x {
                if self.tiles[ty * self.tiles_x + tx].is_none() {
                    continue;
                }
                let tile = Rect::new(
                    tx * TILE,
                    ty * TILE,
                    TILE.min(self.width - tx * TILE),
                    TILE.min(self.height - ty * TILE),
                );
                bounds = Some(match bounds {
                    Some(current) => current.union(&tile),
                    None => tile,
                });
            }
        }
        bounds
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    fn tile_of(&self, x: usize, y: usize) -> usize {
        (y / TILE) * self.tiles_x + x / TILE
    }

    pub fn get(&self, x: usize, y: usize) -> Rgba {
        if x >= self.width || y >= self.height {
            return [0.0; 4];
        }
        let index = self.tile_of(x, y);
        match &self.tiles[index] {
            Some(tile) => tile.pixels[(y % TILE) * TILE + x % TILE],
            None => [0.0; 4],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, color: Rgba) {
        if x >= self.width || y >= self.height {
            return;
        }
        let index = self.tile_of(x, y);
        let tile = self.tiles[index].get_or_insert_with(Tile::new);
        let slot = &mut tile.pixels[(y % TILE) * TILE + x % TILE];
        let before = slot[3] > 0.0;
        *slot = color;
        let after = color[3] > 0.0;
        if before != after {
            if after {
                self.inked += 1;
            } else {
                self.inked -= 1;
            }
        }
        self.mark(Rect::new(x, y, 1, 1));
    }

    pub fn mark(&mut self, rect: Rect) {
        if rect.is_empty() {
            return;
        }
        self.dirty = Some(match self.dirty {
            Some(current) => current.union(&rect),
            None => rect,
        });
        self.gpu_dirty = Some(match self.gpu_dirty {
            Some(current) => current.union(&rect),
            None => rect,
        });
    }

    /// Take the rectangle the GPU layer texture still needs. It is independent of `dirty`, which
    /// the CPU compositor and its fallback still consume.
    pub fn take_gpu_dirty(&mut self) -> Option<Rect> {
        self.gpu_dirty.take()
    }

    pub fn dirty(&self) -> Option<Rect> {
        self.dirty
    }

    pub fn take_dirty(&mut self) -> Option<Rect> {
        self.dirty.take()
    }

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
    }

    /// Multiply the alpha of a masked region by `1 - coverage`, for the eraser.
    pub fn erase_mask_at(&mut self, mask: &crate::fill::Mask, x: usize, y: usize) {
        if x >= self.width || y >= self.height {
            return;
        }
        let width = mask.width.min(self.width - x);
        let height = mask.height.min(self.height - y);
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
                let tile = self.tiles[index].get_or_insert_with(Tile::new);
                let slot = &mut tile.pixels[(py % TILE) * TILE + px % TILE];
                let before = slot[3] > 0.0;
                slot[3] *= 1.0 - amount;
                if slot[3] <= 1e-4 {
                    *slot = [0.0; 4];
                }
                let after = slot[3] > 0.0;
                if before != after {
                    delta -= 1;
                }
            }
        }
        self.inked = (self.inked as isize + delta) as usize;
        self.mark(Rect::new(x, y, width, height));
    }

    /// Copy a region out, for an undo step.
    pub fn snapshot(&self, rect: Rect) -> Vec<Rgba> {
        let mut out = Vec::with_capacity(rect.width * rect.height);
        for y in rect.y..(rect.y + rect.height).min(self.height) {
            for x in rect.x..(rect.x + rect.width).min(self.width) {
                out.push(self.get(x, y));
            }
        }
        out
    }

    pub fn restore(&mut self, rect: Rect, pixels: &[Rgba]) {
        let mut cursor = 0;
        for y in rect.y..(rect.y + rect.height).min(self.height) {
            for x in rect.x..(rect.x + rect.width).min(self.width) {
                if let Some(color) = pixels.get(cursor) {
                    self.set(x, y, *color);
                }
                cursor += 1;
            }
        }
        self.mark(rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_tile_costs_nothing_and_reads_transparent() {
        let surface = Surface::new(1024, 1024);
        assert_eq!(surface.get(500, 500), [0.0; 4]);
        let allocated = surface.tiles.iter().filter(|tile| tile.is_some()).count();
        assert_eq!(allocated, 0);
    }

    #[test]
    fn writing_allocates_only_that_tile() {
        let mut surface = Surface::new(1024, 1024);
        surface.set(10, 10, [1.0, 0.0, 0.0, 1.0]);
        let allocated = surface.tiles.iter().filter(|tile| tile.is_some()).count();
        assert_eq!(allocated, 1);
        assert_eq!(surface.get(10, 10), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn a_wet_stroke_picks_up_the_colour_under_it() {
        let mut surface = Surface::new(8, 8);
        surface.set(4, 4, [1.0, 0.0, 0.0, 1.0]);
        surface.begin_stroke_accumulation();
        surface.set_stroke_wet(1.0);
        let mut mask = crate::fill::Mask::new(1, 1);
        mask.set(0, 0, 1.0);
        surface.blend_stroke_mask_at(&mask, 4, 4, [0.0, 0.0, 1.0, 1.0], StrokeMask::Paint);
        surface.end_stroke_accumulation();
        let pixel = surface.get(4, 4);
        assert!(pixel[0] > 0.9 && pixel[2] < 0.1, "wet pixel {pixel:?}");
    }

    #[test]
    fn the_dirty_rectangle_covers_every_write() {
        let mut surface = Surface::new(1024, 1024);
        surface.set(10, 10, [1.0; 4]);
        surface.set(600, 40, [1.0; 4]);
        let dirty = surface.take_dirty().unwrap();
        assert!(dirty.x <= 10 && dirty.y <= 10);
        assert!(dirty.x + dirty.width >= 601);
        assert!(dirty.y + dirty.height >= 41);
        assert!(
            surface.dirty().is_none(),
            "taking the rectangle did not clear it"
        );
    }

    #[test]
    fn the_ink_count_follows_paint_and_erase() {
        let mut surface = Surface::new(8, 8);
        assert!(surface.is_clear());
        surface.set(1, 1, [1.0, 0.0, 0.0, 1.0]);
        assert!(!surface.is_clear());

        let mut mask = crate::fill::Mask::new(8, 8);
        mask.set(2, 2, 1.0);
        surface.blend_mask(&mask, [0.0, 0.0, 1.0, 0.5]);
        surface.erase_mask_at(&mask, 0, 0);
        assert!(!surface.is_clear(), "the pixel at 1,1 is still painted");

        surface.set(1, 1, [0.0; 4]);
        assert!(surface.is_clear(), "erasing the last pixel left ink behind");
    }

    #[test]
    fn a_snapshot_round_trips_a_region() {
        let mut surface = Surface::new(64, 64);
        surface.set(3, 3, [0.2, 0.4, 0.6, 1.0]);
        let rect = Rect::new(0, 0, 8, 8);
        let saved = surface.snapshot(rect);
        surface.set(3, 3, [0.0; 4]);
        surface.restore(rect, &saved);
        assert_eq!(surface.get(3, 3), [0.2, 0.4, 0.6, 1.0]);
    }

    #[test]
    fn a_mask_blended_at_an_offset_lands_there() {
        let mut surface = Surface::new(64, 64);
        let mut mask = crate::fill::Mask::new(2, 2);
        mask.set(0, 0, 1.0);
        mask.set(1, 1, 1.0);
        surface.blend_mask_at(&mask, 10, 20, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(surface.get(10, 20), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(surface.get(11, 21), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(surface.get(12, 22), [0.0; 4]);
    }

    #[test]
    fn out_of_bounds_writes_are_ignored() {
        let mut surface = Surface::new(16, 16);
        surface.set(999, 999, [1.0; 4]);
        assert!(surface.dirty().is_none());
    }
}

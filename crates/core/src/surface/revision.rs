//! Content versions, so a consumer can ask "did this change?" without reading pixels.
//!
//! Every tile carries a revision and the surface carries a generation. Both are monotonic
//! counters compared for equality or order, never clocks: a write bumps them, a read does not,
//! and allocating a tile is not itself a change. The full model is in `docs/spatial-state.md`.

use super::{Rect, Surface, TILE};

impl Surface {
    /// The revision of the tile holding canvas pixel `x, y`, or zero when that tile is missing.
    pub fn tile_revision(&self, x: usize, y: usize) -> u64 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        let index = self.tile_of(x, y);
        self.tiles[index].as_ref().map_or(0, |tile| tile.revision)
    }

    /// The highest tile revision under `rect`, which is the content version of that region.
    /// Empty rects and rects outside the canvas are zero.
    pub fn tile_rect_revision(&self, rect: Rect) -> u64 {
        if rect.is_empty() || rect.x >= self.width || rect.y >= self.height {
            return 0;
        }
        let x_end = (rect.x + rect.width).min(self.width);
        let y_end = (rect.y + rect.height).min(self.height);
        let mut revision = 0;
        for ty in (rect.y / TILE)..=((y_end - 1) / TILE) {
            for tx in (rect.x / TILE)..=((x_end - 1) / TILE) {
                if let Some(tile) = &self.tiles[ty * self.tiles_x + tx] {
                    revision = revision.max(tile.revision);
                }
            }
        }
        revision
    }

    /// How many write operations this surface has seen. A checkpoint records this and walks the
    /// tiles only when it differs from the record.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Bump every allocated tile under `rect` once and the generation once for the operation.
    /// Missing tiles stay missing: allocation alone is not a content change.
    pub(crate) fn bump_tiles(&mut self, rect: Rect) {
        if rect.is_empty() || rect.x >= self.width || rect.y >= self.height {
            return;
        }
        let x_end = (rect.x + rect.width).min(self.width);
        let y_end = (rect.y + rect.height).min(self.height);
        let mut bumped = false;
        for ty in (rect.y / TILE)..=((y_end - 1) / TILE) {
            for tx in (rect.x / TILE)..=((x_end - 1) / TILE) {
                if let Some(tile) = self.tiles[ty * self.tiles_x + tx].as_mut() {
                    tile.revision = tile.revision.wrapping_add(1);
                    bumped = true;
                }
            }
        }
        if bumped {
            self.generation = self.generation.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    fn red() -> Rgba {
        [1.0, 0.0, 0.0, 1.0]
    }

    fn full_mask(width: usize, height: usize) -> crate::fill::Mask {
        let mut mask = crate::fill::Mask::new(width, height);
        for y in 0..height {
            for x in 0..width {
                mask.set(x, y, 1.0);
            }
        }
        mask
    }

    #[test]
    fn writing_one_pixel_bumps_only_that_tile() {
        let mut surface = Surface::new(1024, 1024);
        surface.set(10, 10, red());

        assert_eq!(surface.tile_revision(10, 10), 1);
        assert_eq!(
            surface.tile_revision(500, 10),
            0,
            "the next tile was untouched"
        );
        assert_eq!(
            surface.tile_revision(10, 500),
            0,
            "the tile below was untouched"
        );
    }

    #[test]
    fn a_full_canvas_blend_bumps_every_allocated_tile_once() {
        let mut surface = Surface::new(512, 512);
        surface.set(10, 10, red());
        surface.set(300, 300, red());
        let before: Vec<u64> = [(10, 10), (300, 300)]
            .iter()
            .map(|&(x, y)| surface.tile_revision(x, y))
            .collect();

        // One blend covers all four tiles; the two that already had pixels must move by one,
        // not by the number of pixels written.
        surface.blend_mask(&full_mask(512, 512), [0.0, 0.0, 1.0, 0.5]);

        assert_eq!(surface.tile_revision(10, 10), before[0] + 1);
        assert_eq!(surface.tile_revision(300, 300), before[1] + 1);
        assert!(
            surface.tile_revision(10, 300) > 0,
            "an empty tile was painted"
        );
        assert!(
            surface.tile_revision(300, 10) > 0,
            "an empty tile was painted"
        );
    }

    #[test]
    fn reading_does_not_bump() {
        let mut surface = Surface::new(512, 512);
        surface.set(10, 10, red());
        let revision = surface.tile_revision(10, 10);
        let generation = surface.generation();

        let _ = surface.get(10, 10);
        let _ = surface.snapshot(Rect::new(0, 0, 300, 300));
        let _ = surface.tile_rect_revision(Rect::new(0, 0, 512, 512));
        let _ = surface.painted_bounds();

        assert_eq!(surface.tile_revision(10, 10), revision);
        assert_eq!(surface.generation(), generation);
    }

    #[test]
    fn a_rect_spanning_two_tiles_returns_the_max() {
        let mut surface = Surface::new(1024, 1024);
        surface.set(10, 10, red());
        surface.set(20, 10, red());
        surface.set(300, 10, red());

        assert_eq!(surface.tile_rect_revision(Rect::new(0, 0, 10, 10)), 2);
        assert_eq!(
            surface.tile_rect_revision(Rect::new(300, 0, 50, 10)),
            1,
            "only the second tile is in this rect"
        );
        assert_eq!(
            surface.tile_rect_revision(Rect::new(250, 0, 300, 10)),
            2,
            "the wider rect takes the larger revision"
        );
        assert_eq!(surface.tile_rect_revision(Rect::new(0, 0, 0, 10)), 0);
    }

    #[test]
    fn an_unallocated_tile_has_no_revision() {
        let surface = Surface::new(256, 256);
        assert_eq!(surface.tile_revision(0, 0), 0);
        assert_eq!(surface.tile_rect_revision(Rect::new(0, 0, 256, 256)), 0);
        assert_eq!(surface.generation(), 0);
    }

    #[test]
    fn generation_increases_on_mutations() {
        let mut surface = Surface::new(512, 512);
        assert_eq!(surface.generation(), 0);

        surface.set(10, 10, red());
        assert_eq!(surface.generation(), 1);

        surface.blend_mask(&full_mask(512, 512), [0.0, 0.0, 1.0, 0.5]);
        assert_eq!(surface.generation(), 2);

        surface.restore(Rect::new(0, 0, 300, 300), &vec![red(); 300 * 300]);
        assert_eq!(surface.generation(), 3, "restore is one operation");
    }

    #[test]
    fn restore_bumps_each_tile_once_not_once_per_pixel() {
        let mut surface = Surface::new(512, 512);
        surface.set(10, 10, red());
        surface.set(300, 300, red());
        let left = surface.tile_revision(10, 10);
        let right = surface.tile_revision(300, 300);

        // This rect covers four tiles and 65 536 pixels; each tile moves by exactly one.
        surface.restore(Rect::new(0, 0, 512, 512), &vec![red(); 512 * 512]);

        assert_eq!(surface.tile_revision(10, 10), left + 1);
        assert_eq!(surface.tile_revision(300, 300), right + 1);
    }

    #[test]
    fn a_resized_copy_versions_its_own_tiles() {
        let mut surface = Surface::new(512, 512);
        surface.set(10, 10, red());

        let copy = surface.resized(512, 512, (0, 0));

        assert_eq!(copy.tile_revision(10, 10), 1);
        assert_eq!(copy.generation(), 1);
        assert_eq!(
            surface.tile_revision(10, 10),
            1,
            "the source was not written"
        );
    }
}

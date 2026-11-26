//! Copying a surface to another canvas size. This crops or pads; it never scales.

use super::*;

impl Surface {
    /// A copy of this surface at another canvas size, with the old content moved to `offset`.
    /// `offset` is the old top-left corner in the new canvas, in pixels.
    pub fn resized(&self, width: usize, height: usize, offset: (isize, isize)) -> Surface {
        let mut surface = Surface::new(width, height);
        if width == 0 || height == 0 {
            return surface;
        }
        let (dx, dy) = offset;
        let target_x = dx.max(0) as usize;
        let target_y = dy.max(0) as usize;
        let source_x = (-dx).max(0) as usize;
        let source_y = (-dy).max(0) as usize;
        let columns = self.width.saturating_sub(source_x).min(width - target_x);
        let rows = self.height.saturating_sub(source_y).min(height - target_y);
        for y in 0..rows {
            for x in 0..columns {
                let pixel = self.get(source_x + x, source_y + y);
                if pixel[3] > 0.0 {
                    surface.write_pixel(target_x + x, target_y + y, pixel);
                }
            }
        }
        surface.bump_tiles(Rect::new(target_x, target_y, columns, rows));
        surface
    }
}

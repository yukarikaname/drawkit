//! Apply the pixels a GPU stroke committed to the resident layer tiles. The CPU surface keeps the
//! same pixels for history, export and replay, and the stroke never stamps the brush again.

use super::*;

/// One undo step assembled from the non-overlapping tile rectangles a commit returned.
#[derive(Clone)]
pub(super) struct TileCommit {
    layer: usize,
    /// The deferred raster the commit replaced. An empty commit hands it back so the shell can
    /// still fall back to the CPU replay.
    pending: Option<PendingStroke>,
}

/// Split a rectangle into blocks the size of a core tile, in surface coordinates. The conversion
/// buffers then stay one tile large instead of one GPU rect large.
fn blocks(rect: Rect) -> impl Iterator<Item = (usize, usize, usize, usize)> {
    let step = crate::surface::TILE;
    (0..rect.height).step_by(step).flat_map(move |y| {
        (0..rect.width).step_by(step).map(move |x| {
            (rect.x + x, rect.y + y, step.min(rect.width - x), step.min(rect.height - y))
        })
    })
}

impl Editor {
    /// Prepare to collect committed tiles. Returns false for a brush whose GPU preview is not the
    /// stroke the CPU would have painted, so the shell keeps the sliced replay.
    pub fn begin_tile_commit(&mut self) -> bool {
        self.cancel_prediction();
        self.predictor.clear();
        self.smoothed = None;
        // The GPU tiles already hold the stroke. Rasterising the recorded segments here would
        // paint the same pixels a second time with the CPU brush and push a second history step.
        let pending = self.pending_stroke.take();
        let layer = self
            .stroke
            .take()
            .map_or(self.document.active, |session| session.layer);
        if let Some(target) = self.document.layers.get_mut(layer) {
            target.surface.end_stroke_accumulation();
        }
        self.tile_commit = Some(TileCommit { layer, pending });
        true
    }

    /// Apply pixels materialised from the GPU into the CPU cache. The GPU journal already holds
    /// the history step, so this never pushes a new one.
    pub fn apply_tiles(&mut self, layer: usize, rect: Rect, pixels: &[u8]) -> bool {
        let expected = rect.width * rect.height * 4;
        if rect.is_empty() || pixels.len() < expected {
            return false;
        }
        let Some(target) = self.document.layers.get_mut(layer) else {
            return false;
        };
        // Convert one core tile at a time. A whole 1024 px GPU tile is 16 MB of f32 at 4K, and a
        // stroke row touches several of them; the old whole-rect copy left that much behind in
        // the allocator for every stroke.
        for (x, y, width, height) in blocks(rect) {
            let mut colors = Vec::with_capacity(width * height);
            for row in y..y + height {
                for column in x..x + width {
                    let at = ((row - rect.y) * rect.width + (column - rect.x)) * 4;
                    colors.push([
                        f32::from(pixels[at]) / 255.0,
                        f32::from(pixels[at + 1]) / 255.0,
                        f32::from(pixels[at + 2]) / 255.0,
                        f32::from(pixels[at + 3]) / 255.0,
                    ]);
                }
            }
            target
                .surface
                .write_pixels_from_gpu(Rect::new(x, y, width, height), &colors);
        }
        true
    }

    /// Apply linear float tiles from an extended-range GPU commit. The surfaces store encoded
    /// working RGB, so each pixel goes back through the inverse matrix and the transfer curve.
    pub fn apply_tiles_linear(&mut self, layer: usize, rect: Rect, pixels: &[f32]) -> bool {
        let expected = rect.width * rect.height * 4;
        if rect.is_empty() || pixels.len() < expected {
            return false;
        }
        let Some(target) = self.document.layers.get_mut(layer) else {
            return false;
        };
        let space = self.document.color_space;
        let inverse = space.from_display_p3();
        for (x, y, width, height) in blocks(rect) {
            let mut colors = Vec::with_capacity(width * height);
            for row in y..y + height {
                for column in x..x + width {
                    let at = ((row - rect.y) * rect.width + (column - rect.x)) * 4;
                    let encoded = |channel: usize| {
                        let linear = inverse[channel][0] * pixels[at]
                            + inverse[channel][1] * pixels[at + 1]
                            + inverse[channel][2] * pixels[at + 2];
                        space.encode(linear.max(0.0))
                    };
                    colors.push([encoded(0), encoded(1), encoded(2), pixels[at + 3].clamp(0.0, 1.0)]);
                }
            }
            target
                .surface
                .write_pixels_from_gpu(Rect::new(x, y, width, height), &colors);
        }
        true
    }

    /// Close a GPU commit: the pixels stay in the resident tiles and the journal is their undo
    /// record, so the history only keeps the touched rectangles.
    pub fn end_gpu_commit(&mut self, rects: &[Rect]) -> bool {
        let Some(commit) = self.tile_commit.take() else {
            return false;
        };
        if let Some(layer) = self.document.layers.get_mut(commit.layer) {
            layer.surface.end_stroke_accumulation();
        }
        if rects.is_empty() {
            self.pending_stroke = commit.pending;
            return false;
        }
        self.record_gpu_step(commit.layer, rects);
        true
    }
}

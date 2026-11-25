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
        let colors: Vec<Rgba> = pixels[..expected]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| {
                [
                    f32::from(pixel[0]) / 255.0,
                    f32::from(pixel[1]) / 255.0,
                    f32::from(pixel[2]) / 255.0,
                    f32::from(pixel[3]) / 255.0,
                ]
            })
            .collect();
        target.surface.write_pixels(rect, &colors);
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

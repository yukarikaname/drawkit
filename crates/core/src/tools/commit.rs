//! Apply the pixels a GPU stroke committed to the resident layer tiles. The CPU surface keeps the
//! same pixels for history, export and replay, and the stroke never stamps the brush again.

use super::*;

/// One undo step assembled from the non-overlapping tile rectangles a commit returned.
#[derive(Clone)]
pub(super) struct TileCommit {
    layer: usize,
}

impl Editor {
    /// Prepare to collect committed tiles. Returns false for a brush whose GPU preview is not the
    /// stroke the CPU would have painted, so the shell keeps the sliced replay.
    pub fn begin_tile_commit(&mut self) -> bool {
        let brush = self.brush;
        if self.stabilization > 0.0
            || self.mirror != Mirror::NONE
            || self.brush_texture.is_some()
            || brush.grain > 0.0
            || brush.paper > 0.0
            || brush.texture > 0.0
            || brush.dual > 0.0
            || brush.jitter > 0.0
            || brush.wet > 0.0
            || brush.pressure_alpha > 0.0
        {
            return false;
        }
        self.cancel_prediction();
        self.predictor.clear();
        self.smoothed = None;
        self.pending_stroke = None;
        self.flush_deferred();
        let layer = self
            .stroke
            .take()
            .map_or(self.document.active, |session| session.layer);
        self.tile_commit = Some(TileCommit { layer });
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
            return false;
        }
        self.record_gpu_step(commit.layer, rects);
        true
    }
}

//! GPU-resident stroke commits. The pixels stay on the GPU: the replaced tile textures are the
//! undo record, and the CPU surface is a cache rebuilt only when a pixel tool or a save asks.

use super::*;
use crate::layers::CommittedTile;

impl CanvasSurface {
    /// Composite the live stroke into layer `index` and keep the replaced tiles for undo.
    ///
    /// `_alpha` is the brush flow the shell passes; the coverage buffer already carries it, so the
    /// commit does not apply it again. See `StrokePreview::commit_alpha`.
    pub fn stroke_commit(&mut self, index: usize, color: [f32; 3], _alpha: f32) -> usize {
        self.committed_rects.clear();
        let Some((x0, y0, x1, y1)) = self.stroke_dirty() else {
            return 0;
        };
        let Some(coverage) = self.stroke.coverage().cloned() else {
            return 0;
        };
        let alpha = self.stroke.commit_alpha();
        let rect = (x0, y0, x1 - x0 + 1, y1 - y0 + 1);
        let pipeline = if self.layers.format() == crate::FrameFormat::Rgba8 {
            &self.tile_pipeline
        } else {
            &self.tile_hdr_pipeline
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.stroke.encode(&mut encoder);
        let (rects, swaps) = self.layers.commit_stroke(
            &self.device,
            &self.queue,
            &mut encoder,
            pipeline,
            index,
            &coverage,
            rect,
            color,
            alpha,
            self.stroke.wet(),
            if self.stroke_smudge.is_some() {
                3
            } else if self.stroke.is_erase() {
                1
            } else if self.stroke_alpha_lock {
                2
            } else {
                0
            },
            self.stroke_smudge.unwrap_or([0.0; 2]),
        );
        self.stroke.clear_after_commit(&mut encoder, rect);
        self.queue.submit(Some(encoder.finish()));
        self.stroke_alpha_lock = false;
        self.stroke_smudge = None;
        self.committed_rects = rects.clone();
        if swaps.is_empty() {
            return 0;
        }
        let bytes = rects.len().saturating_mul(self.layers.tile_bytes_hint());
        if self.journal.grouping() {
            if let Some(group) = self.journal.push_grouped(&rects, swaps, bytes) {
                self.committed_rects = group;
                self.mark_stale(index, &rects);
            }
            self.sync_pins();
            return rects.len();
        }
        self.journal
            .push(index, self.committed_rects.clone(), swaps, bytes);
        self.sync_pins();
        self.mark_stale(index, &self.committed_rects.clone());
        self.committed_rects.len()
    }

    /// Tell the stack which tiles the journal's next step still needs resident.
    pub(super) fn sync_pins(&mut self) {
        let pins = self.journal.pending_keep();
        self.layers.set_pinned(pins);
    }

    pub fn committed_rect(&self, index: usize) -> Option<(u32, u32, u32, u32)> {
        self.committed_rects.get(index).copied()
    }

    /// Every rectangle the last commit, undo or redo touched, for a shell that re-uploads them.
    pub fn committed_rects(&self) -> &[(u32, u32, u32, u32)] {
        &self.committed_rects
    }

    /// Start collecting the per-segment commits of a smudge into one undo step.
    pub fn begin_smudge_journal(&mut self, layer: usize) {
        self.journal.begin_group(layer);
    }

    /// Close the smudge journal and return how many rectangles were committed.
    pub fn end_smudge_journal(&mut self) -> usize {
        let Some(rects) = self.journal.end_group() else {
            self.committed_rects.clear();
            return 0;
        };
        self.committed_rects = rects;
        self.sync_pins();
        self.committed_rects.len()
    }

    /// Forget the tile journal. Switching a history branch restores the CPU surfaces wholesale, so
    /// the replaced tiles no longer describe anything undo can walk back to.
    pub fn clear_tile_history(&mut self) {
        self.journal.clear();
        self.sync_pins();
        self.committed.clear();
        self.committed_rects.clear();
        self.stale.clear();
    }

    /// The rectangles the next undo would restore, or `None` when there is no step to undo.
    pub fn pending_undo_rects(&self) -> Option<&[(u32, u32, u32, u32)]> {
        self.journal.undo_rects()
    }

    /// The rectangles the next redo would restore, or `None` when there is no step to redo.
    pub fn pending_redo_rects(&self) -> Option<&[(u32, u32, u32, u32)]> {
        self.journal.redo_rects()
    }

    /// Swap every tile of the newest commit back and keep the current ones for redo.
    pub fn undo_tiles(&mut self) -> bool {
        let Some(mut step) = self.journal.begin_undo() else {
            return false;
        };
        for swap in step.swaps.drain(..) {
            let current = self.layers.restore_swap(swap);
            step.undone.push(current);
        }
        self.committed_rects = step.rects.clone();
        self.mark_stale(step.layer, &step.rects);
        self.journal.finish_undo(step);
        self.sync_pins();
        true
    }

    /// Swap the newest undone commit's tiles back in.
    pub fn redo_tiles(&mut self) -> bool {
        let Some(mut step) = self.journal.begin_redo() else {
            return false;
        };
        for swap in step.undone.drain(..) {
            let current = self.layers.restore_swap(swap);
            step.swaps.push(current);
        }
        self.committed_rects = step.rects.clone();
        self.mark_stale(step.layer, &step.rects);
        self.journal.finish_redo(step);
        self.sync_pins();
        true
    }

    /// The visible colour over paper at one canvas pixel, without reading the CPU cache.
    pub fn pick(&mut self, x: u32, y: u32) -> Option<[f32; 4]> {
        if self.layers.format() != crate::FrameFormat::Rgba8 {
            // Linear Display P3 over white paper; the shell converts it to the working space.
            let pixel = self
                .layers
                .read_output_linear(&self.device, &self.queue, x, y)?;
            let alpha = pixel[3].clamp(0.0, 1.0);
            return Some([
                pixel[0] + (1.0 - alpha),
                pixel[1] + (1.0 - alpha),
                pixel[2] + (1.0 - alpha),
                1.0,
            ]);
        }
        let pixel = self.layers.read_output(&self.device, &self.queue, x, y)?;
        let alpha = f32::from(pixel[3]) / 255.0;
        let channel = |value: u8| f32::from(value) / 255.0;
        Some([
            channel(pixel[0]) + (1.0 - alpha),
            channel(pixel[1]) + (1.0 - alpha),
            channel(pixel[2]) + (1.0 - alpha),
            1.0,
        ])
    }

    /// Rebuild the CPU cache for every rectangle the GPU touched since the last call.
    pub fn materialize(&mut self) -> usize {
        self.committed.clear();
        let stale = std::mem::take(&mut self.stale);
        for (layer, rect) in stale {
            if let Some(tile) = self
                .layers
                .read_layer_rect(&self.device, &self.queue, layer, rect)
            {
                self.committed.push(tile);
            }
        }
        self.committed.len()
    }

    /// The half-float twin of `materialize`, for an extended-range layer stack.
    pub fn materialize_linear(&mut self) -> usize {
        self.linear_committed.clear();
        let stale = std::mem::take(&mut self.stale);
        for (layer, rect) in stale {
            if let Some(tile) =
                self.layers
                    .read_layer_rect_linear(&self.device, &self.queue, layer, rect)
            {
                self.linear_committed.push(tile);
            }
        }
        self.linear_committed.len()
    }

    pub fn linear_tile_info(&self, index: usize) -> Option<(usize, u32, u32, u32, u32)> {
        let tile = self.linear_committed.get(index)?;
        Some((tile.layer, tile.x, tile.y, tile.width, tile.height))
    }

    pub fn linear_tile(&self, index: usize) -> Option<&crate::layers::LinearTile> {
        self.linear_committed.get(index)
    }

    pub fn clear_linear_committed(&mut self) {
        self.linear_committed.clear();
    }

    pub fn committed_tile(&self, index: usize) -> Option<&CommittedTile> {
        self.committed.get(index)
    }

    pub fn clear_committed(&mut self) {
        self.committed.clear();
    }

    fn mark_stale(&mut self, layer: usize, rects: &[(u32, u32, u32, u32)]) {
        for rect in rects {
            self.stale.push((layer, *rect));
        }
    }
}

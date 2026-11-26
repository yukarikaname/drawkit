//! Applying a coverage mask into a resident layer, for a live fill.
//!
//! The core still decides what the fill covers; the surface only writes those pixels. The blend is
//! one render pass per touched tile on the GPU, and the replaced textures become one journal step,
//! so undo is the same texture swap a GPU stroke uses. The shell's pixel cache is marked stale and
//! catches up through `materialize`; the live path never reads a tile back or uploads one.

use super::*;

impl CanvasSurface {
    /// Blend a canvas-sized coverage mask into layer `layer`.
    ///
    /// `mask` is one float per canvas pixel, the same mask the core hands the replay compositor.
    /// Returns false when the surface cannot take it (wrong size, no coverage); nothing is painted.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_fill(
        &mut self,
        layer: usize,
        mask: &[f32],
        width: u32,
        height: u32,
        color: [f32; 4],
        alpha_locked: bool,
        under: bool,
    ) -> bool {
        let (canvas_width, canvas_height) = self.layers.size();
        if width != canvas_width
            || height != canvas_height
            || mask.len() < canvas_width as usize * canvas_height as usize
        {
            return false;
        }
        let pipeline = if self.layers.format() == crate::FrameFormat::Rgba8 {
            &self.fill_pipeline
        } else {
            &self.fill_hdr_pipeline
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let (rects, swaps) = self.layers.commit_fill(
            &self.device,
            &self.queue,
            &mut encoder,
            pipeline,
            layer,
            mask,
            color,
            alpha_locked,
            under,
        );
        if rects.is_empty() {
            return false;
        }
        self.queue.submit(Some(encoder.finish()));
        self.committed_rects = rects.clone();
        if !swaps.is_empty() {
            let bytes = swaps.len().saturating_mul(self.layers.tile_bytes_hint());
            self.journal.push(layer, rects.clone(), swaps, bytes);
            self.sync_pins();
        }
        // The CPU surface did not change: the fill lives in the tiles, so the shell has to read
        // them back before a save or a pixel tool. The same list drives undo and redo.
        for rect in &rects {
            self.stale.push((layer, *rect));
        }
        true
    }

    /// The union of the rectangles the last fill touched, for a shell that reports only one dirty
    /// rectangle. `None` when the last fill touched nothing.
    pub fn fill_dirty(&self) -> Option<(u32, u32, u32, u32)> {
        let (x, y, right, bottom) =
            self.committed_rects
                .iter()
                .fold(None, |bounds, rect| match bounds {
                    None => Some((rect.0, rect.1, rect.0 + rect.2, rect.1 + rect.3)),
                    Some((x, y, right, bottom)) => Some((
                        x.min(rect.0),
                        y.min(rect.1),
                        right.max(rect.0 + rect.2),
                        bottom.max(rect.1 + rect.3),
                    )),
                })?;
        Some((x, y, right - x, bottom - y))
    }
}

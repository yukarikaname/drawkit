//! The live wet bake: settled field pigment into resident tiles, one journal step.
//!
//! The field keeps its water and mobile pigment on the GPU; `WetField::deposit` has already packed
//! the settled part into a texture. This module only schedules the tile passes, swaps the replaced
//! textures into the journal and marks the shell's cache stale. No tile comes back to the CPU and
//! no field state is read back.

use super::*;

impl CanvasSurface {
    /// Bake `deposit` into layer `layer` over `rect`, in canvas pixels.
    ///
    /// Returns the rectangles that were actually baked, one per touched tile. The replaced
    /// textures become one undo step; `undo_tiles` swaps them back. A deposit from a field that
    /// has not stepped yet, or whose canvas does not match, bakes nothing.
    pub fn apply_wet(
        &mut self,
        layer: usize,
        deposit: &crate::WetDeposit,
        rect: (u32, u32, u32, u32),
    ) -> Vec<(u32, u32, u32, u32)> {
        self.committed_rects.clear();
        let pipeline = if self.layers.format() == crate::FrameFormat::Rgba8 {
            &self.wet_pipeline
        } else {
            &self.wet_hdr_pipeline
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let (rects, swaps) = self.layers.commit_wet(
            &self.device,
            &self.queue,
            &mut encoder,
            pipeline,
            layer,
            deposit,
            rect,
        );
        if rects.is_empty() {
            return rects;
        }
        self.queue.submit(Some(encoder.finish()));
        self.committed_rects = rects.clone();
        if !swaps.is_empty() {
            let bytes = swaps.len().saturating_mul(self.layers.tile_bytes_hint());
            self.journal.push(layer, rects.clone(), swaps, bytes);
            self.sync_pins();
        }
        // The CPU cache did not change: the bake lives in the tiles, so the shell has to read
        // them back before a save or a pixel tool. The same list drives undo and redo.
        for rect in &rects {
            self.stale.push((layer, *rect));
        }
        rects
    }

    /// The union of the rectangles the last wet bake touched, for a shell that reports one dirty
    /// rectangle. `None` when the last bake touched nothing.
    pub fn wet_dirty(&self) -> Option<(u32, u32, u32, u32)> {
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

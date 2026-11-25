//! GPU-resident stroke commits. The pixels stay on the GPU: the replaced tile textures are the
//! undo record, and the CPU surface is a cache rebuilt only when a pixel tool or a save asks.

use super::*;
use crate::layers::{CommittedTile, TileSwap};

/// How many old tile textures the undo journal may keep before dropping the oldest step.
const JOURNAL_BUDGET: usize = 512 << 20;

pub(super) struct JournalStep {
    pub(super) layer: usize,
    pub(super) rects: Vec<(u32, u32, u32, u32)>,
    swaps: Vec<TileSwap>,
    undone: Vec<TileSwap>,
    bytes: usize,
}

impl CanvasSurface {
    /// Composite the live stroke into layer `index` and keep the replaced tiles for undo.
    pub fn stroke_commit(&mut self, index: usize, color: [f32; 3], alpha: f32) -> usize {
        self.committed_rects.clear();
        let Some((x0, y0, x1, y1)) = self.stroke_dirty() else {
            return 0;
        };
        let Some(coverage) = self.stroke.coverage().cloned() else {
            return 0;
        };
        let rect = (x0, y0, x1 - x0 + 1, y1 - y0 + 1);
        let pipeline = if self.layers.format() == crate::composite::FORMAT {
            &self.tile_pipeline
        } else {
            &self.tile_hdr_pipeline
        };
        let (rects, swaps) = self.layers.commit_stroke(
            &self.device,
            &self.queue,
            pipeline,
            index,
            &coverage,
            rect,
            color,
            alpha,
            self.stroke.wet(),
            if self.stroke.is_erase() {
                1
            } else if self.stroke_alpha_lock {
                2
            } else {
                0
            },
        );
        self.stroke_alpha_lock = false;
        self.committed_rects = rects.clone();
        if swaps.is_empty() {
            return 0;
        }
        let bytes = rects.len().saturating_mul(self.layers.tile_bytes_hint());
        self.tile_undo.push(JournalStep {
            layer: index,
            rects: self.committed_rects.clone(),
            swaps,
            undone: Vec::new(),
            bytes,
        });
        self.tile_redo.clear();
        self.journal_bytes += bytes;
        while self.journal_bytes > JOURNAL_BUDGET && self.tile_undo.len() > 1 {
            let dropped = self.tile_undo.remove(0);
            self.journal_bytes -= dropped.bytes;
        }
        self.mark_stale(index, &self.committed_rects.clone());
        self.committed_rects.len()
    }

    pub fn committed_rect(&self, index: usize) -> Option<(u32, u32, u32, u32)> {
        self.committed_rects.get(index).copied()
    }

    /// Forget the tile journal. Switching a history branch restores the CPU surfaces wholesale, so
    /// the replaced tiles no longer describe anything undo can walk back to.
    pub fn clear_tile_history(&mut self) {
        self.tile_undo.clear();
        self.tile_redo.clear();
        self.committed.clear();
        self.committed_rects.clear();
        self.stale.clear();
        self.journal_bytes = 0;
    }

    /// Swap every tile of the newest commit back and keep the current ones for redo.
    pub fn undo_tiles(&mut self) -> bool {
        let Some(mut step) = self.tile_undo.pop() else {
            return false;
        };
        for swap in step.swaps.drain(..) {
            let current = self.layers.restore_swap(swap);
            step.undone.push(current);
        }
        self.journal_bytes -= step.bytes;
        self.mark_stale(step.layer, &step.rects);
        self.tile_redo.push(step);
        true
    }

    /// Swap the newest undone commit's tiles back in.
    pub fn redo_tiles(&mut self) -> bool {
        let Some(mut step) = self.tile_redo.pop() else {
            return false;
        };
        for swap in step.undone.drain(..) {
            let current = self.layers.restore_swap(swap);
            step.swaps.push(current);
        }
        self.journal_bytes += step.bytes;
        self.mark_stale(step.layer, &step.rects);
        self.tile_undo.push(step);
        true
    }

    /// The visible colour over paper at one canvas pixel, without reading the CPU cache.
    pub fn pick(&mut self, x: u32, y: u32) -> Option<[f32; 4]> {
        // The composed half-float frame is not what the core stores; an extended-range pick reads
        // the CPU composite, which knows the working colour space.
        if self.layers.format() != crate::composite::FORMAT {
            return None;
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

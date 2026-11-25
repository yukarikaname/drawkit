//! Composite a finished GPU stroke into the resident layer tiles instead of replaying it on the
//! CPU. Only the tiles the stroke touched are read back, for history and the project cache.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::layers::{CommittedTile, PendingTile};

/// Tiles whose readback is still in flight. The GPU layer already holds the committed pixels, so
/// the preview can come down before the CPU receives them.
pub(super) struct CommitJob {
    layer: usize,
    tiles: Vec<PendingTile>,
    progress: Arc<AtomicUsize>,
}

impl CanvasSurface {
    /// Composite the live stroke into layer `index`, keeping the new pixels for the shell to apply
    /// to the core surface. Returns the number of tiles; zero means there was nothing to commit.
    pub fn stroke_commit(&mut self, index: usize, color: [f32; 3], alpha: f32) -> usize {
        self.committed.clear();
        let Some((x0, y0, x1, y1)) = self.stroke_dirty() else {
            return 0;
        };
        let Some(stroke) = self.stroke.as_ref() else {
            return 0;
        };
        let coverage = stroke.buffer.clone();
        let rect = (x0, y0, x1 - x0 + 1, y1 - y0 + 1);
        let progress = Arc::new(AtomicUsize::new(0));
        let tiles = self.layers.commit_stroke(
            &self.device,
            &self.queue,
            &self.tile_pipeline,
            index,
            &coverage,
            rect,
            color,
            alpha,
            progress.clone(),
        );
        let count = tiles.len();
        if count > 0 {
            self.pending_commit = Some(CommitJob {
                layer: index,
                tiles,
                progress,
            });
        }
        count
    }

    /// True once every tile copy has completed. Polling never blocks.
    pub fn commit_ready(&self) -> bool {
        let Some(job) = self.pending_commit.as_ref() else {
            return false;
        };
        let _ = self.device.poll(wgpu::PollType::Poll);
        job.progress.load(Ordering::Acquire) >= job.tiles.len()
    }

    /// Collect the finished tiles and update the layer backups. Blocks only if the caller ignored
    /// `commit_ready`.
    pub fn commit_finish(&mut self) -> bool {
        let Some(job) = self.pending_commit.take() else {
            return false;
        };
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        self.committed = job
            .tiles
            .into_iter()
            .filter_map(|tile| self.layers.finish_tile(job.layer, tile))
            .collect();
        true
    }

    pub fn committed_tile(&self, index: usize) -> Option<&CommittedTile> {
        self.committed.get(index)
    }

    pub fn clear_committed(&mut self) {
        self.committed.clear();
    }
}

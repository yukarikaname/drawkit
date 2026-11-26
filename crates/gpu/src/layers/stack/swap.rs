//! Journal texture swaps. Undo puts a previous texture back and keeps the one it replaced for
//! redo; a backup-less stack does the same swap, because the journal holds both textures.

use super::commit::TileSwap;
use super::*;

impl LayerStack {
    /// Put a previous texture back and return the one it replaced, for redo.
    pub(crate) fn restore_swap(&mut self, swap: TileSwap) -> TileSwap {
        let backups = self.backups;
        let mut restored = 0;
        let current = self
            .layers
            .get_mut(swap.layer)
            .and_then(Option::as_mut)
            .and_then(|layer| layer.tiles.get_mut(&swap.coord))
            .and_then(|tile| {
                // Without a backup the journal texture is the record, not a stale copy.
                tile.stale = backups;
                tile.missing = false;
                if tile.gpu.is_none() {
                    // The tile was evicted after the step was recorded; this texture is new
                    // residency that the accounting has not seen yet.
                    restored = swap.bytes;
                }
                tile.gpu.replace(swap.old)
            })
            .expect("a committed tile stays resident");
        self.resident_bytes += restored;
        self.evict_cold(Some(swap.coord));
        self.dirty_tiles.insert(swap.coord);
        TileSwap {
            coord: swap.coord,
            old: current,
            layer: swap.layer,
            bytes: swap.bytes,
        }
    }

    /// Texture bytes of one tile, for the journal budget.
    pub(crate) fn tile_bytes_hint(&self) -> usize {
        self.tile_bytes()
    }

    /// A tile whose texture was evicted on a backup-less stack.
    pub(super) fn tile_missing(&self, index: usize, coord: (u32, u32)) -> bool {
        self.layers
            .get(index)
            .and_then(Option::as_ref)
            .and_then(|layer| layer.tiles.get(&coord))
            .is_some_and(|tile| tile.missing && tile.gpu.is_none())
    }
}

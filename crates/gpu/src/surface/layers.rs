//! Incremental layer structure changes for the live surface.
//!
//! The stack rearranges its hosts without touching the other layers' textures. This module keeps
//! the `LayerSpec` list in step (group and clip references are positions, so they shift with the
//! layers), remaps the pending readbacks, and drops the tile journal: an undo step names layer
//! positions and tile swaps, so it cannot survive a reorder. The shell rebuilds its GPU history
//! after a structural edit, exactly as it already does after `reset_layers`.

use super::*;
use crate::layers::specs;

impl CanvasSurface {
    /// How many layers the resident stack holds. The shell compares this with the document
    /// instead of keeping its own count, so an incremental insert does not look like a rebuild.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Insert an empty layer at `index`. Returns false when the size or index does not fit.
    ///
    /// Group and clip references above `index` move up with their layers. Pixels of every other
    /// layer stay resident; only the tile journal is dropped.
    pub fn insert_layer(&mut self, index: usize, width: u32, height: u32) -> bool {
        let was = self.layers.len();
        if index > was || !self.layers.insert_layer(index, width, height) {
            return false;
        }
        self.pad_specs(was);
        specs::insert(&mut self.layer_specs, index);
        self.forget_tile_history();
        self.remap_layers(|layer| Some(layer + usize::from(layer >= index)));
        true
    }

    /// Remove layer `index` and shift the layers above it down.
    ///
    /// Refuses, without changing anything, when another layer groups under or clips through
    /// `index`; the shell has to ungroup or unclip those layers first. That is the only case where
    /// the position references cannot be kept consistent.
    pub fn remove_layer(&mut self, index: usize) -> bool {
        let len = self.layers.len();
        if index >= len {
            return false;
        }
        self.pad_specs(len);
        if specs::referenced(&self.layer_specs, index) {
            return false;
        }
        if !self.layers.remove_layer(index) {
            return false;
        }
        specs::remove(&mut self.layer_specs, index);
        self.forget_tile_history();
        self.remap_layers(|layer| match layer {
            layer if layer < index => Some(layer),
            layer if layer == index => None,
            layer => Some(layer - 1),
        });
        true
    }

    /// Move layer `from` to `to`. Only the draw order changes; pixels and group membership follow
    /// the layer.
    pub fn move_layer(&mut self, from: usize, to: usize) -> bool {
        let len = self.layers.len();
        if from >= len || to >= len {
            return false;
        }
        if from == to {
            return true;
        }
        self.pad_specs(len);
        if !self.layers.move_layer(from, to) {
            return false;
        }
        specs::move_layer(&mut self.layer_specs, from, to);
        self.forget_tile_history();
        self.remap_layers(|layer| Some(specs::moved(layer, from, to)));
        true
    }

    /// Give every existing layer a spec, so an insert or a move can address its position.
    fn pad_specs(&mut self, count: usize) {
        while self.layer_specs.len() < count {
            let layer = self.layer_specs.len();
            self.layer_specs.push(crate::LayerSpec::new(layer));
        }
    }

    fn forget_tile_history(&mut self) {
        self.journal.clear();
        self.sync_pins();
        self.committed_rects.clear();
    }

    /// Follow a layer renumbering through the stale rectangles and the pending readbacks.
    fn remap_layers(&mut self, mut map: impl FnMut(usize) -> Option<usize>) {
        let mut keep = |layer: &mut usize| match map(*layer) {
            Some(next) => {
                *layer = next;
                true
            }
            None => false,
        };
        self.stale.retain_mut(|(layer, _)| keep(layer));
        self.committed.retain_mut(|tile| keep(&mut tile.layer));
        self.linear_committed
            .retain_mut(|tile| keep(&mut tile.layer));
    }
}

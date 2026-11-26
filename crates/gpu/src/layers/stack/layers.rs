//! Incremental layer structure changes. Inserting, removing or moving a layer keeps every other
//! layer's tile textures and backups in place; only the layer list is rearranged, and the next
//! composite repaints every tile so the new draw order is visible.

use super::*;

impl LayerStack {
    /// Insert an empty resident layer at `index`, shifting the layers above it up.
    ///
    /// The new layer has no tiles and is marked known, so the shell does not have to upload a
    /// transparent one. `width` and `height` must be the canvas size the stack was built with.
    pub fn insert_layer(&mut self, index: usize, width: u32, height: u32) -> bool {
        if width != self.width || height != self.height || index > self.layers.len() {
            return false;
        }
        self.layers.insert(
            index,
            Some(Layer {
                tiles: HashMap::new(),
                known: true,
            }),
        );
        self.mark_all_dirty();
        true
    }

    /// Remove layer `index` and shift the layers above it down. Only that layer's tiles are
    /// dropped; the others keep their textures.
    pub fn remove_layer(&mut self, index: usize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        self.drop_layer(index);
        self.layers.remove(index);
        self.mark_all_dirty();
        true
    }

    /// Move layer `from` to `to`, reordering the layer hosts without touching their pixels.
    pub fn move_layer(&mut self, from: usize, to: usize) -> bool {
        let len = self.layers.len();
        if from >= len || to >= len {
            return false;
        }
        if from == to {
            return true;
        }
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);
        self.mark_all_dirty();
        true
    }
}

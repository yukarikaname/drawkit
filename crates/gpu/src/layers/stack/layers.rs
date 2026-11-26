//! Incremental layer structure changes. Inserting, removing or moving a layer keeps every other
//! layer's tile textures and backups in place. A reorder repaints every tile because the draw
//! order changed; an empty insert, or removing an empty layer, changes nothing on screen and
//! leaves the cached composite alone.

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
        // An empty layer composites to nothing, so the cached frame is still correct. This keeps
        // a layer add from repainting the whole canvas, which at 4K is every tile of every layer.
        true
    }

    /// Remove layer `index` and shift the layers above it down. Only that layer's tiles are
    /// dropped; the others keep their textures.
    pub fn remove_layer(&mut self, index: usize) -> bool {
        if index >= self.layers.len() {
            return false;
        }
        let had_pixels = self.layers[index]
            .as_ref()
            .is_some_and(|layer| !layer.tiles.is_empty());
        self.drop_layer(index);
        self.layers.remove(index);
        // Removing an empty layer leaves the frame identical; anything with tiles must repaint.
        if had_pixels {
            self.mark_all_dirty();
        }
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

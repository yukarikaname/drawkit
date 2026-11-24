use super::*;

impl Editor {
    pub fn selection_rect(&self) -> Option<Rect> {
        self.selection
    }

    /// Clamp a rectangle to the canvas and remember it. A zero-area drag clears the selection.
    pub fn select_rect(&mut self, rect: Rect) {
        let (width, height) = (self.document.width(), self.document.height());
        let x = rect.x.min(width);
        let y = rect.y.min(height);
        let right = (rect.x + rect.width).min(width);
        let bottom = (rect.y + rect.height).min(height);
        if right <= x || bottom <= y {
            self.selection = None;
            return;
        }
        self.selection = Some(Rect::new(x, y, right - x, bottom - y));
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Copy the selected pixels of the active layer into a new layer above it. The source stays
    /// intact, which is the non-destructive half of "extract"; a cut can follow with a clear.
    pub fn extract_selection(&mut self) -> Option<usize> {
        let selection = self.selection?;
        let source = self.document.active;
        if self.document.layers.get(source)?.kind != LayerKind::Paint {
            return None;
        }
        let before = self.document.active;
        let name = format!("Extracted {}", self.document.layers.len() + 1);
        let index = self.document.add_layer(name);
        for y in selection.y..selection.y + selection.height {
            for x in selection.x..selection.x + selection.width {
                let pixel = self.document.layers[source].surface.get(x, y);
                self.document.layers[index].surface.set(x, y, pixel);
            }
        }
        self.document.layers[index].surface.mark(selection);
        let layer = self.document.layers[index].clone();
        self.record_layer_insertion(index, layer, before, self.document.active);
        Some(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_only_touches_the_selected_pixels() {
        let mut editor = Editor::new(8, 8, 1 << 20);
        editor.clear([1.0, 1.0, 1.0, 1.0]);
        editor.select_rect(Rect::new(2, 2, 4, 4));
        editor.clear([0.0; 4]);
        assert_eq!(editor.document.layers[0].surface.get(1, 1)[3], 1.0);
        assert_eq!(editor.document.layers[0].surface.get(3, 3)[3], 0.0);
    }

    #[test]
    fn extraction_copies_the_selection_to_a_new_layer() {
        let mut editor = Editor::new(8, 8, 1 << 20);
        editor.document.layers[0]
            .surface
            .set(3, 3, [1.0, 0.0, 0.0, 1.0]);
        editor.select_rect(Rect::new(2, 2, 4, 4));
        let layer = editor.extract_selection().expect("a new layer");
        assert_eq!(editor.document.layers.len(), 2);
        assert_eq!(editor.document.layers[layer].surface.get(3, 3)[0], 1.0);
        assert_eq!(editor.document.layers[0].surface.get(3, 3)[0], 1.0);
    }
}

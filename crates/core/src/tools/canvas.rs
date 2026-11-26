//! Changing the canvas size. This crops or pads the art; it never scales it.

use super::*;

impl Editor {
    /// Resize every layer and every animation frame, keeping the art where `offset` puts it.
    ///
    /// This is a new history baseline: the old pixel steps describe a surface with different
    /// dimensions, and replaying them after a resize would write outside the canvas. The tape is
    /// untouched, so strokes keep their document coordinates.
    pub fn resize_canvas(&mut self, width: usize, height: usize, offset: (isize, isize)) -> bool {
        if !(16..=8192).contains(&width) || !(16..=8192).contains(&height) {
            return false;
        }
        if width == self.document.width() && height == self.document.height() && offset == (0, 0) {
            return false;
        }
        self.flush_deferred();
        self.finish_fill_animation();
        self.cancel_prediction();
        self.predictor.clear();
        self.stroke = None;
        self.smoothed = None;
        self.document.resize(width, height, offset);
        for frame in self.frames.iter_mut().flatten() {
            frame.document.resize(width, height, offset);
            frame.history.clear();
        }
        self.selection = None;
        self.clear_history();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizing_pads_and_crops_without_scaling() {
        let mut editor = Editor::new(32, 32, 1 << 20);
        editor.document.layers[0]
            .surface
            .set(8, 8, [1.0, 0.0, 0.0, 1.0]);
        assert!(editor.resize_canvas(40, 40, (4, 4)));
        assert_eq!(editor.document.width(), 40);
        assert_eq!(
            editor.document.layers[0].surface.get(12, 12),
            [1.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(editor.document.layers[0].surface.get(8, 8), [0.0; 4]);
        assert!(!editor.can_undo(), "the resize should start a new baseline");

        let mut editor = Editor::new(32, 32, 1 << 20);
        editor.document.layers[0]
            .surface
            .set(8, 8, [1.0, 0.0, 0.0, 1.0]);
        assert!(editor.resize_canvas(24, 24, (-4, -4)));
        assert_eq!(
            editor.document.layers[0].surface.get(4, 4),
            [1.0, 0.0, 0.0, 1.0]
        );
        assert!(
            !editor.resize_canvas(8, 8, (0, 0)),
            "too small was accepted"
        );
    }
}

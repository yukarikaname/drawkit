use super::*;

impl Editor {
    pub(super) fn next_serial(&mut self) -> u64 {
        let serial = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1);
        serial
    }
    pub(super) fn record_pixel_step(
        &mut self,
        layer: usize,
        rect: Rect,
        before: Vec<Rgba>,
        after: Vec<Rgba>,
    ) {
        self.begin_new_edit();
        let serial = self.next_serial();
        self.layer_redo.clear();
        self.history.push(serial, layer, rect, before, after);
    }

    /// Record a GPU stroke: the rectangles are known, the pixels stay in the tile journal.
    pub(super) fn record_gpu_step(&mut self, layer: usize, rects: &[Rect]) {
        self.begin_new_edit();
        let serial = self.next_serial();
        self.layer_redo.clear();
        self.history.push_gpu_step(serial, layer, rects.to_vec());
    }

    /// Whether the next undo would move GPU tiles rather than pixel data.
    pub fn undo_is_gpu(&self) -> bool {
        let layer_serial = self.layer_undo.last().map_or(0, LayerAction::serial);
        layer_serial <= self.history.top_serial() && self.history.top_is_gpu()
    }

    /// Whether the next redo would move GPU tiles.
    pub fn redo_is_gpu(&self) -> bool {
        let layer_serial = self.layer_redo.last().map_or(0, LayerAction::serial);
        let pixel_serial = self.history.redo_serial();
        let layer_wins = layer_serial != 0 && (pixel_serial == 0 || layer_serial < pixel_serial);
        !layer_wins && self.history.redo_is_gpu()
    }

    /// Record a palette/tree change. Consecutive changes of the same field on the same layer are
    /// one undo step, so dragging the opacity slider does not fill the history with one step per
    /// pixel of travel.
    pub fn record_layer_state(&mut self, index: usize, field: LayerField, before: LayerMeta) {
        let Some(layer) = self.document.layers.get(index) else {
            return;
        };
        let after = layer.meta();
        if before == after {
            return;
        }
        self.begin_new_edit();
        self.layer_redo.clear();
        self.history.clear_redo();
        if let Some(LayerAction::State {
            index: last_index,
            field: last_field,
            after: last_after,
            ..
        }) = self.layer_undo.last_mut()
            && *last_index == index
            && *last_field == field
            && *last_after == before
        {
            *last_after = after;
            return;
        }
        let serial = self.next_serial();
        self.layer_undo.push(LayerAction::State {
            serial,
            index,
            field,
            before,
            after,
        });
    }

    pub fn record_layer_insertion(
        &mut self,
        index: usize,
        layer: Layer,
        active_before: usize,
        active_after: usize,
    ) {
        self.begin_new_edit();
        self.layer_redo.clear();
        self.history.clear_redo();
        let serial = self.next_serial();
        self.layer_undo.push(LayerAction::Structure {
            serial,
            removed: Vec::new(),
            inserted: vec![(index, layer)],
            active_before,
            active_after,
        });
    }

    pub fn record_layer_removal(
        &mut self,
        removed: Vec<(usize, Layer)>,
        inserted: Vec<(usize, Layer)>,
        active_before: usize,
        active_after: usize,
    ) {
        self.begin_new_edit();
        self.layer_redo.clear();
        self.history.clear_redo();
        let serial = self.next_serial();
        self.layer_undo.push(LayerAction::Structure {
            serial,
            removed,
            inserted,
            active_before,
            active_after,
        });
    }

    pub fn record_layer_move(
        &mut self,
        before_index: usize,
        after_index: usize,
        steps: i32,
        active_before: usize,
        active_after: usize,
    ) {
        self.begin_new_edit();
        self.layer_redo.clear();
        self.history.clear_redo();
        let serial = self.next_serial();
        self.layer_undo.push(LayerAction::Reorder {
            serial,
            before_index,
            after_index,
            steps,
            active_before,
            active_after,
        });
    }

    /// Move a layer or group into another group, or out to the root. The whole subtree follows
    /// and the flat order is rebuilt, so this is a tree edit rather than a sibling swap.
    pub fn reparent_layer(&mut self, index: usize, parent: Option<usize>) -> Option<usize> {
        let old_parents: Vec<Option<usize>> = self
            .document
            .layers
            .iter()
            .map(|layer| layer.parent)
            .collect();
        let active_before = self.document.active;
        let (new_index, mapping) = self.document.reparent_layer(index, parent)?;
        let active_after = self.document.active;
        self.begin_new_edit();
        self.layer_redo.clear();
        self.history.clear_redo();
        let serial = self.next_serial();
        self.layer_undo.push(LayerAction::Reparent {
            serial,
            index,
            parent,
            mapping,
            old_parents,
            active_before,
            active_after,
        });
        Some(new_index)
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo() || !self.layer_undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo() || !self.layer_redo.is_empty()
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.layer_undo.clear();
        self.layer_redo.clear();
        self.clear_history_branches();
    }

    fn apply_layer_structure(&mut self, action: &LayerAction, before: bool) {
        let LayerAction::Structure {
            removed,
            inserted,
            active_before,
            active_after,
            ..
        } = action
        else {
            return;
        };
        let (to_remove, to_insert, active) = if before {
            (inserted, removed, *active_before)
        } else {
            (removed, inserted, *active_after)
        };
        for (index, _) in to_remove.iter().rev() {
            if *index < self.document.layers.len() {
                self.document.layers.remove(*index);
            }
        }
        for (index, layer) in to_insert {
            let at = (*index).min(self.document.layers.len());
            self.document.layers.insert(at, layer.clone());
        }
        self.document.active = active.min(self.document.layers.len().saturating_sub(1));
    }

    pub fn undo(&mut self) -> Option<Rect> {
        self.flush_deferred();
        self.finish_fill_animation();
        self.cancel_prediction();
        self.predictor.clear();
        self.stroke = None;
        self.smoothed = None;
        if !self.can_undo() {
            return None;
        }
        self.note_undo();
        let layer_serial = self.layer_undo.last().map_or(0, LayerAction::serial);
        if layer_serial > self.history.top_serial() {
            let action = self.layer_undo.pop()?;
            match &action {
                LayerAction::State { index, before, .. } => {
                    if let Some(layer) = self.document.layers.get_mut(*index) {
                        layer.apply_meta(before);
                    }
                }
                LayerAction::Structure { .. } => self.apply_layer_structure(&action, true),
                LayerAction::Reorder {
                    after_index,
                    steps,
                    active_before,
                    ..
                } => {
                    self.document.move_layer(*after_index, -*steps);
                    self.document.active = *active_before;
                }
                LayerAction::Reparent {
                    mapping,
                    old_parents,
                    active_before,
                    ..
                } => {
                    let count = self.document.layers.len();
                    let mut slots: Vec<Option<Layer>> =
                        self.document.layers.drain(..).map(Some).collect();
                    let mut restored = Vec::with_capacity(count);
                    for new in mapping.iter().copied() {
                        if let Some(layer) = slots.get_mut(new).and_then(Option::take) {
                            restored.push(layer);
                        }
                    }
                    for (index, layer) in restored.iter_mut().enumerate() {
                        layer.parent = old_parents[index];
                    }
                    self.document.layers = restored;
                    self.document.active = *active_before;
                }
            }
            self.layer_redo.push(action);
            let rect = Rect::new(0, 0, self.document.width(), self.document.height());
            return Some(rect);
        }
        // The tape is edited too: replaying a stroke the painter took back would be a recording of
        // something that never happened. A stroke edit carries its own tape action instead.
        if !self.history.top_has_tape() {
            self.recording.undo();
        }
        let (rect, tape) = self.history.undo(&mut self.document)?;
        if let Some(edit) = tape {
            self.recording.apply_edit(&edit, false);
        }
        Some(rect)
    }

    pub fn redo(&mut self) -> Option<Rect> {
        self.flush_deferred();
        self.finish_fill_animation();
        self.cancel_prediction();
        self.predictor.clear();
        self.stroke = None;
        self.smoothed = None;
        if !self.can_redo() {
            return None;
        }
        let layer_serial = self.layer_redo.last().map_or(0, LayerAction::serial);
        let pixel_serial = self.history.redo_serial();
        if layer_serial != 0 && (pixel_serial == 0 || layer_serial < pixel_serial) {
            let action = self.layer_redo.pop()?;
            match &action {
                LayerAction::State { index, after, .. } => {
                    if let Some(layer) = self.document.layers.get_mut(*index) {
                        layer.apply_meta(after);
                    }
                }
                LayerAction::Structure { .. } => self.apply_layer_structure(&action, false),
                LayerAction::Reorder {
                    before_index,
                    steps,
                    active_after,
                    ..
                } => {
                    self.document.move_layer(*before_index, *steps);
                    self.document.active = *active_after;
                }
                LayerAction::Reparent {
                    index,
                    parent,
                    active_after,
                    ..
                } => {
                    self.document.reparent_layer(*index, *parent);
                    self.document.active = *active_after;
                }
            }
            self.layer_undo.push(action);
            self.note_redo();
            let rect = Rect::new(0, 0, self.document.width(), self.document.height());
            return Some(rect);
        }
        let (rect, tape) = self.history.redo(&mut self.document)?;
        match tape {
            Some(edit) => self.recording.apply_edit(&edit, true),
            // The step was a new stroke or fill, so the tape half is the undo of the undo.
            None => {
                self.recording.redo();
            }
        }
        self.note_redo();
        Some(rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fill::FillOptions;

    /// The rectangle the fake GPU commit reports.
    fn rects() -> [Rect; 1] {
        [Rect::new(0, 0, 32, 32)]
    }

    /// One deferred stroke the GPU committed: the tape gets its group and the history gets the
    /// tile rectangles, as a real GPU stroke does.
    fn gpu_stroke(editor: &mut Editor, time_ms: u64) {
        editor.defer_raster = true;
        editor.begin_stroke(time_ms);
        editor.stroke_at(
            (4.0, 8.0),
            (28.0, 8.0),
            3.0,
            1.0,
            [1.0, 0.0, 0.0, 1.0],
            time_ms,
        );
        editor.end_stroke();
        assert!(editor.begin_tile_commit());
        assert!(editor.end_gpu_commit(&rects()));
    }

    #[test]
    fn a_gpu_fill_comes_back_on_redo() {
        let mut editor = Editor::new(32, 32, 1 << 20);
        editor
            .begin_gpu_fill(4, 4, 0.1, [1.0, 0.0, 0.0, 1.0], FillOptions::default(), 7)
            .expect("a mask");
        assert!(editor.finish_gpu_fill(&rects()));
        let original = editor.recording.events().to_vec();
        assert_eq!(original.len(), 1);

        editor.undo().expect("an undo step");
        assert!(editor.recording.is_empty(), "the tape kept the undone fill");
        editor.redo().expect("a redo step");
        assert_eq!(
            editor.recording.events(),
            original.as_slice(),
            "redo did not put the fill back with the same fields"
        );
    }

    #[test]
    fn a_gpu_stroke_comes_back_on_redo() {
        let mut editor = Editor::new(32, 32, 1 << 20);
        gpu_stroke(&mut editor, 0);
        let original = editor.recording.events().to_vec();
        assert!(!original.is_empty());

        assert!(editor.undo_is_gpu());
        editor.undo().expect("an undo step");
        assert!(
            editor.recording.is_empty(),
            "the tape kept the undone stroke"
        );
        assert!(editor.redo_is_gpu());
        editor.redo().expect("a redo step");
        assert_eq!(editor.recording.events(), original.as_slice());
    }

    #[test]
    fn a_grouped_stroke_undoes_and_redoes_as_one_unit() {
        let mut editor = Editor::new(32, 32, 1 << 20);
        editor.defer_raster = true;
        editor.begin_stroke(0);
        let mut x = 4.0;
        for step in 0..3 {
            let to = (x + 8.0, 16.0);
            editor.stroke_at((x, 16.0), to, 3.0, 1.0, [1.0, 0.0, 0.0, 1.0], step as u64);
            x = to.0;
        }
        editor.end_stroke();
        assert!(editor.begin_tile_commit());
        assert!(editor.end_gpu_commit(&rects()));
        let original = editor.recording.events().to_vec();
        assert_eq!(original.len(), 3);

        editor.undo().expect("an undo step");
        assert!(editor.recording.is_empty(), "a segment stayed on the tape");
        editor.redo().expect("a redo step");
        assert_eq!(editor.recording.events(), original.as_slice());
        assert!(
            editor
                .recording
                .events()
                .windows(2)
                .all(|pair| pair[0].time_ms <= pair[1].time_ms),
            "the restored group replays out of order"
        );
    }

    #[test]
    fn two_undos_and_two_redos_restore_the_tape() {
        let mut editor = Editor::new(32, 32, 1 << 20);
        gpu_stroke(&mut editor, 0);
        gpu_stroke(&mut editor, 50);
        let original = editor.recording.events().to_vec();

        editor.undo().expect("first undo");
        editor.undo().expect("second undo");
        assert!(editor.recording.is_empty());
        editor.redo().expect("first redo");
        editor.redo().expect("second redo");
        assert_eq!(editor.recording.events(), original.as_slice());
    }

    #[test]
    fn a_new_stroke_after_undo_abandons_the_undone_group() {
        let mut editor = Editor::new(32, 32, 1 << 20);
        gpu_stroke(&mut editor, 0);
        editor.undo().expect("an undo step");

        gpu_stroke(&mut editor, 50);
        let second = editor.recording.events().to_vec();
        editor.undo().expect("the new stroke undid");
        assert!(
            editor.recording.is_empty(),
            "undo resurrected the abandoned stroke"
        );
        editor.redo().expect("the new stroke redid");
        assert_eq!(
            editor.recording.events(),
            second.as_slice(),
            "redo brought back the abandoned stroke"
        );
        assert!(
            editor.redo().is_none(),
            "the abandoned branch was still redoable"
        );
    }
}

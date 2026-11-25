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
        // something that never happened.
        self.recording.undo();
        self.history.undo(&mut self.document)
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
        let rect = self.history.redo(&mut self.document);
        self.note_redo();
        rect
    }
}

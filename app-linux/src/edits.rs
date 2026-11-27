// Document edits behind the tool bar and the layer list. Every one of them ends in a full
// refresh, so the resident stack, the buffer and the status bar agree after the change.

use std::rc::Rc;

use drawkit_core::document::LayerKind;
use drawkit_core::tools::LayerField;
use gtk::prelude::*;

use crate::canvas::Canvas;

impl Canvas {
    pub fn undo(self: &Rc<Self>) {
        let mut guard = self.state.borrow_mut();
        let state = &mut *guard;
        if state.editor.undo_is_gpu() {
            if state.editor.undo().is_none() {
                return;
            }
            let swapped = state.paint.as_mut().is_some_and(|paint| paint.undo_tiles());
            if !swapped {
                return;
            }
            state.gpu_stale = true;
            state.materialize();
        } else {
            state.materialize();
            state.editor.undo();
        }
        drop(guard);
        self.refresh();
    }

    pub fn redo(self: &Rc<Self>) {
        let mut guard = self.state.borrow_mut();
        let state = &mut *guard;
        if state.editor.redo_is_gpu() {
            if state.editor.redo().is_none() {
                return;
            }
            let swapped = state.paint.as_mut().is_some_and(|paint| paint.redo_tiles());
            if !swapped {
                return;
            }
            state.gpu_stale = true;
            state.materialize();
        } else {
            state.materialize();
            state.editor.redo();
        }
        drop(guard);
        self.refresh();
    }

    pub fn active_opacity(&self) -> f64 {
        let state = self.state.borrow();
        state
            .editor
            .document
            .layers
            .get(state.editor.document.active)
            .map(|layer| layer.opacity as f64)
            .unwrap_or(1.0)
    }

    /// The selected layer's opacity, as one undoable layer-state change.
    pub fn set_layer_opacity(self: &Rc<Self>, opacity: f64) {
        let mut state = self.state.borrow_mut();
        let index = state.editor.document.active;
        let Some(layer) = state.editor.document.layers.get(index) else {
            return;
        };
        let before = layer.meta();
        state.editor.document.layers[index].opacity = opacity.clamp(0.0, 1.0) as f32;
        state
            .editor
            .record_layer_state(index, LayerField::Opacity, before);
        drop(state);
        self.refresh();
    }

    pub fn add_layer(self: &Rc<Self>) {
        let mut state = self.state.borrow_mut();
        let before = state.editor.document.active;
        let name = format!("Layer {}", state.editor.document.layers.len() + 1);
        let index = state.editor.document.add_layer(name);
        let layer = state.editor.document.layers[index].clone();
        let after = state.editor.document.active;
        state
            .editor
            .record_layer_insertion(index, layer, before, after);
        drop(state);
        self.refresh();
    }

    pub fn add_group(self: &Rc<Self>) {
        let mut state = self.state.borrow_mut();
        let before = state.editor.document.active;
        let name = format!("Group {}", state.editor.document.layers.len() + 1);
        let index = state.editor.document.add_group(name);
        let layer = state.editor.document.layers[index].clone();
        let after = state.editor.document.active;
        state
            .editor
            .record_layer_insertion(index, layer, before, after);
        drop(state);
        self.refresh();
    }

    pub fn extract_line_art(self: &Rc<Self>) {
        let mut state = self.state.borrow_mut();
        let added = state
            .editor
            .extract_line_art(drawkit_core::tools::LineArtOptions::default())
            .is_some();
        drop(state);
        if added {
            self.refresh();
        }
    }

    pub fn remove_layer(self: &Rc<Self>) {
        let mut state = self.state.borrow_mut();
        if let Some(removal) = state.editor.document.remove_active_collect() {
            state.editor.record_layer_removal(
                removal.removed,
                removal.inserted,
                removal.active_before,
                removal.active_after,
            );
        }
        drop(state);
        self.refresh();
    }

    pub fn move_layer(self: &Rc<Self>, steps: i32) {
        let mut state = self.state.borrow_mut();
        let before = state.editor.document.active;
        if let Some((after, applied)) = state.editor.document.move_layer(before, steps) {
            let active = state.editor.document.active;
            state
                .editor
                .record_layer_move(before, after, applied, before, active);
        }
        drop(state);
        self.refresh();
    }

    pub(crate) fn select_row(self: &Rc<Self>, row: Option<&gtk::ListBoxRow>) {
        let Some(row) = row else { return };
        let index = row.index().max(0) as usize;
        let mut state = self.state.borrow_mut();
        if index < state.editor.document.layers.len() {
            state.editor.document.active = index;
        }
        drop(state);
        self.update_status();
    }

    pub(crate) fn set_visible(self: &Rc<Self>, index: usize, visible: bool) {
        let mut state = self.state.borrow_mut();
        let Some(layer) = state.editor.document.layers.get(index) else {
            return;
        };
        let before = layer.meta();
        state.editor.document.layers[index].visible = visible;
        state
            .editor
            .record_layer_state(index, LayerField::Visible, before);
        drop(state);
        self.refresh();
    }

    pub(crate) fn layer_name(&self, index: usize) -> Option<(String, bool, bool)> {
        let state = self.state.borrow();
        let layer = state.editor.document.layers.get(index)?;
        Some((
            layer.name.clone(),
            layer.visible,
            layer.kind == LayerKind::Group,
        ))
    }

    pub(crate) fn layer_len(&self) -> (usize, usize) {
        let state = self.state.borrow();
        (
            state.editor.document.layers.len(),
            state.editor.document.active,
        )
    }
}

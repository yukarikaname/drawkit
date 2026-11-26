//! The C surface the platform shells call.
//!
//! Deliberately small: an editor, a stroke, a fill, undo, redo, and the composited pixels. Every
//! platform shell wants exactly these, and anything richer would be a second API to keep in step
//! with the first. Pointers are opaque; the only thing a shell may do with one is hand it back.

use std::ffi::CStr;
use std::os::raw::{c_char, c_void};

use crate::tools::{Editor, LayerField};

pub(crate) fn editor<'a>(handle: *mut c_void) -> Option<&'a mut Editor> {
    if handle.is_null() {
        return None;
    }
    // SAFETY: the pointer came from `dk_editor_new` and the shell frees it exactly once.
    Some(unsafe { &mut *(handle as *mut Editor) })
}

fn edit_layer(
    handle: *mut c_void,
    index: u32,
    field: LayerField,
    mutate: impl FnOnce(&mut crate::document::Layer),
) {
    let Some(editor) = editor(handle) else {
        return;
    };
    let Some(layer) = editor.document.layers.get_mut(index as usize) else {
        return;
    };
    let before = layer.meta();
    mutate(layer);
    let after = layer.meta();
    if before != after {
        editor.record_layer_state(index as usize, field, before);
    }
}

mod branches;
mod brush;
mod canvas_ops;
mod edit;
mod files;
mod fill_mask;
mod frames;
mod layer;
mod layer_pixels;
mod palette;
mod picker;
mod reference;
mod render;
mod replay_events;
mod selection;
mod stroke;
mod strokes;
mod tree;

//! Layer-tree edits that are not simple sibling swaps.

use std::os::raw::c_void;

use super::editor;

/// Move a layer or group into `parent`, or to the root when `parent` is negative. Returns the
/// new index, or -1 when the move would create a cycle or the target is not a group.
#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_set_parent(handle: *mut c_void, index: u32, parent: i32) -> i32 {
    let Some(editor) = editor(handle) else {
        return -1;
    };
    let parent = (parent >= 0).then_some(parent as usize);
    editor
        .reparent_layer(index as usize, parent)
        .map_or(-1, |new_index| new_index as i32)
}

/// The paint or fill layer a clipped layer uses as its alpha base. Returns -1 for no clipping
/// and -2 for a group base, which stays on the CPU composite.
#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_clip_base(handle: *mut c_void, index: u32) -> i32 {
    let Some(editor) = editor(handle) else {
        return -1;
    };
    let Some(layer) = editor.document.layers.get(index as usize) else {
        return -1;
    };
    if !layer.clipping {
        return -1;
    }
    editor
        .document
        .clip_base(index as usize)
        .map_or(-2, |base| base as i32)
}

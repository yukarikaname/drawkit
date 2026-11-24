use std::os::raw::c_void;

use super::editor;
use crate::surface::Rect;

/// Copy the active rectangular selection into `out` as x, y, width, height. Returns 1 when a
/// selection exists.
/// # Safety
///
/// `out` must point to four writable `u32`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_selection(handle: *mut c_void, out: *mut u32) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some(selection) = editor.selection_rect() else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    // SAFETY: the shell passed room for four integers.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, 4) };
    buffer[0] = selection.x as u32;
    buffer[1] = selection.y as u32;
    buffer[2] = selection.width as u32;
    buffer[3] = selection.height as u32;
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_selection(handle: *mut c_void, x: u32, y: u32, width: u32, height: u32) {
    if let Some(editor) = editor(handle) {
        editor.select_rect(Rect::new(
            x as usize,
            y as usize,
            width as usize,
            height as usize,
        ));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_clear_selection(handle: *mut c_void) {
    if let Some(editor) = editor(handle) {
        editor.clear_selection();
    }
}

/// Copy the selected pixels into a new layer above the active one. Returns 1 on success.
#[unsafe(no_mangle)]
pub extern "C" fn dk_extract_selection(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.extract_selection().is_some()) as i32
}

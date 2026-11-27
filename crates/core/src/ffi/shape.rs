//! The stroke-shape model and the ideal path it recognises.

use super::*;

/// Turn the tiny stroke-shape model on or off. It only replaces a finished stroke; an erase or
/// fill group is never rewritten.
#[unsafe(no_mangle)]
pub extern "C" fn dk_set_auto_shape(handle: *mut c_void, enabled: i32) {
    if let Some(editor) = editor(handle) {
        editor.auto_shape = enabled != 0;
    }
}

/// Allow a held stroke to snap even when Auto Shape is off. The shell calls `dk_auto_shape` only
/// when it saw the pen rest at the end of the stroke.
#[unsafe(no_mangle)]
pub extern "C" fn dk_set_quick_shape(handle: *mut c_void, enabled: i32) {
    if let Some(editor) = editor(handle) {
        editor.quick_shape = enabled != 0;
    }
}

/// Replace the last stroke with the ideal line, ellipse or rectangle when the model is confident.
#[unsafe(no_mangle)]
pub extern "C" fn dk_auto_shape(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.auto_shape(false)) as i32
}

/// The same, with Shift held: the ideal line is snapped to 45 degrees and the ideal oval and
/// rectangle become a circle and a square.
#[unsafe(no_mangle)]
pub extern "C" fn dk_auto_shape_snapped(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.auto_shape(true)) as i32
}

/// The ideal path the last stroke snaps to, as `x, y` pairs, without painting it. Returns the
/// point count, or zero when the stroke is freehand. A resident-surface shell replays the path
/// through its own stroke; a CPU shell uses `dk_auto_shape` instead.
///
/// # Safety
/// `out` must point at `capacity` writable `f32` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_shape_path(
    handle: *mut c_void,
    snap: i32,
    out: *mut f32,
    capacity: u32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let Some(path) = editor.shape_path(snap != 0) else {
        return 0;
    };
    if path.len() * 2 > capacity as usize {
        return 0;
    }
    for (index, (x, y)) in path.iter().enumerate() {
        unsafe {
            *out.add(index * 2) = *x;
            *out.add(index * 2 + 1) = *y;
        }
    }
    path.len() as i32
}

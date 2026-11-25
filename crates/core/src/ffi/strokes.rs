//! Picking a recorded stroke from the shell.

use super::*;

/// The top-most recorded stroke under a document point, or 0. Stroke groups start at 1, so zero
/// is unambiguous.
#[unsafe(no_mangle)]
pub extern "C" fn dk_stroke_at(handle: *mut c_void, x: f32, y: f32) -> u64 {
    editor(handle)
        .and_then(|editor| editor.pick_stroke((x, y)))
        .unwrap_or(0)
}

/// The document rectangle a stroke can reach: x, y, width, height.
///
/// # Safety
/// `out` must point at four writable `u32` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_stroke_bounds(handle: *mut c_void, group: u64, out: *mut u32) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let Some(rect) = editor.stroke_bounds(group) else {
        return 0;
    };
    // SAFETY: the shell passed four writable u32 values.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, 4) };
    buffer[0] = rect.x as u32;
    buffer[1] = rect.y as u32;
    buffer[2] = rect.width as u32;
    buffer[3] = rect.height as u32;
    1
}

/// Replace the ink of a recorded stroke. The layer is re-rendered from the tape, so the edge
/// pixels take the new colour instead of keeping a fringe of the old one.
#[unsafe(no_mangle)]
pub extern "C" fn dk_recolor_stroke(
    handle: *mut c_void,
    group: u64,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
) -> i32 {
    editor(handle).is_some_and(|editor| editor.recolor_stroke(group, [red, green, blue, alpha]))
        as i32
}

/// Remove a recorded stroke and its segments. Refused when the layer holds pixels the tape did
/// not paint.
#[unsafe(no_mangle)]
pub extern "C" fn dk_delete_stroke(handle: *mut c_void, group: u64) -> i32 {
    editor(handle).is_some_and(|editor| editor.delete_stroke(group)) as i32
}

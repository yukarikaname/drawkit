//! Canvas geometry operations.

use super::*;

/// Change the canvas size. `offset_x/offset_y` place the old top-left corner in the new canvas.
#[unsafe(no_mangle)]
pub extern "C" fn dk_resize_canvas(
    handle: *mut c_void,
    width: u32,
    height: u32,
    offset_x: i32,
    offset_y: i32,
) -> i32 {
    editor(handle).is_some_and(|editor| {
        editor.resize_canvas(
            width as usize,
            height as usize,
            (offset_x as isize, offset_y as isize),
        )
    }) as i32
}

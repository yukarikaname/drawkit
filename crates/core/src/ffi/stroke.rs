//! Coverage from the GPU stroke preview, applied instead of a CPU replay.

use super::*;

/// Start collecting the tiles a GPU stroke committed. False means the brush is not one the preview
/// reproduces exactly and the shell must replay the stroke instead.
#[unsafe(no_mangle)]
pub extern "C" fn dk_stroke_tile_begin(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.begin_tile_commit()) as i32
}

/// Store one tile of final RGBA8 pixels.
///
/// # Safety
/// `pixels` must hold `width * height * 4` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_stroke_tile_commit(
    handle: *mut c_void,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    pixels: *const u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if pixels.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let length = width as usize * height as usize * 4;
    // SAFETY: the shell passes one RGBA8 pixel per rectangle pixel.
    let pixels = unsafe { std::slice::from_raw_parts(pixels, length) };
    let rect = crate::surface::Rect::new(x as usize, y as usize, width as usize, height as usize);
    i32::from(editor.commit_tile(rect, pixels))
}

/// Assemble the collected tiles into one history step.
#[unsafe(no_mangle)]
pub extern "C" fn dk_stroke_tile_end(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.end_tile_commit()) as i32
}

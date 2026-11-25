//! Coverage from the GPU stroke preview, applied instead of a CPU replay.

use super::*;

/// Apply a GPU coverage rectangle to the stroke's layer and record one history step.
///
/// # Safety
/// `coverage` must point at `width * height` floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_stroke_commit_gpu(
    handle: *mut c_void,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    coverage: *const f32,
    red: f32,
    green: f32,
    blue: f32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if coverage.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let length = width as usize * height as usize;
    // SAFETY: the shell passes one float per pixel of the rectangle.
    let coverage = unsafe { std::slice::from_raw_parts(coverage, length) };
    let rect = crate::surface::Rect::new(x as usize, y as usize, width as usize, height as usize);
    i32::from(editor.commit_gpu_stroke(rect, coverage, [red, green, blue]))
}

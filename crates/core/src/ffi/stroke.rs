//! Coverage from the GPU stroke preview, applied instead of a CPU replay.

use super::*;

/// Start collecting the tiles a GPU stroke committed. False means the brush is not one the preview
/// reproduces exactly and the shell must replay the stroke instead.
#[unsafe(no_mangle)]
pub extern "C" fn dk_stroke_tile_begin(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.begin_tile_commit()) as i32
}

/// Close a GPU commit with the rectangles it touched.
///
/// # Safety
/// `rects` must hold `count * 4` u32 values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_stroke_gpu_end(
    handle: *mut c_void,
    rects: *const u32,
    count: u32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if rects.is_null() || count == 0 {
        let _ = editor.end_gpu_commit(&[]);
        return 0;
    }
    // SAFETY: the shell passes four values per rectangle.
    let values = unsafe { std::slice::from_raw_parts(rects, count as usize * 4) };
    let rects: Vec<crate::surface::Rect> = values
        .as_chunks::<4>()
        .0
        .iter()
        .map(|r| {
            crate::surface::Rect::new(r[0] as usize, r[1] as usize, r[2] as usize, r[3] as usize)
        })
        .collect();
    i32::from(editor.end_gpu_commit(&rects))
}

/// Write materialised pixels back into the active layer without a new history step.
///
/// # Safety
/// `pixels` must hold `width * height * 4` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_stroke_apply_tiles(
    handle: *mut c_void,
    layer: u32,
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
    i32::from(editor.apply_tiles(layer as usize, rect, pixels))
}

/// Convert one working colour into straight linear Display P3, for an extended-range stroke.
///
/// # Safety
/// `out` must point at three writable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_linear_colour(
    handle: *mut c_void,
    red: f32,
    green: f32,
    blue: f32,
    out: *mut f32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let space = editor.document.color_space;
    let matrix = space.to_display_p3();
    let linear = [space.decode(red), space.decode(green), space.decode(blue)];
    // SAFETY: the shell passed room for three floats.
    let values = unsafe { std::slice::from_raw_parts_mut(out, 3) };
    for row in 0..3 {
        values[row] =
            (matrix[row][0] * linear[0] + matrix[row][1] * linear[1] + matrix[row][2] * linear[2])
                .max(0.0);
    }
    1
}

/// Apply linear float tiles read back from an extended-range commit.
///
/// # Safety
/// `pixels` must hold `width * height * 4` floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_stroke_apply_tiles_linear(
    handle: *mut c_void,
    layer: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    pixels: *const f32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if pixels.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let length = width as usize * height as usize * 4;
    // SAFETY: the shell passes four floats per rectangle pixel.
    let pixels = unsafe { std::slice::from_raw_parts(pixels, length) };
    let rect = crate::surface::Rect::new(x as usize, y as usize, width as usize, height as usize);
    i32::from(editor.apply_tiles_linear(layer as usize, rect, pixels))
}

/// Whether the next undo moves GPU tiles.
#[unsafe(no_mangle)]
pub extern "C" fn dk_undo_is_gpu(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.undo_is_gpu()) as i32
}

/// Whether the next redo moves GPU tiles.
#[unsafe(no_mangle)]
pub extern "C" fn dk_redo_is_gpu(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.redo_is_gpu()) as i32
}

//! Layer pixels handed to a shell: dirty rectangles, eight-bit readback and linear readback.

use super::*;

/// The rectangle of a layer the GPU canvas still has to upload, or 0. The rectangle is cleared.
///
/// # Safety
/// `out` must point at four `u32` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_take_gpu_dirty(handle: *mut c_void, index: u32, out: *mut u32) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let Some(layer) = editor.document.layers.get_mut(index as usize) else {
        return 0;
    };
    let Some(rect) = layer.surface.take_gpu_dirty() else {
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

/// Copy a rectangle of one layer as straight RGBA8. Fill layers are a solid colour; groups and
/// out-of-range pixels are transparent.
///
/// # Safety
/// `out` must point at `width * height * 4` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_layer_rect(
    handle: *mut c_void,
    index: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    out: *mut u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let Some(layer) = editor.document.layers.get(index as usize) else {
        return 0;
    };
    let count = width as usize * height as usize * 4;
    // SAFETY: the shell allocated `width * height * 4` bytes.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, count) };
    for row in 0..height as usize {
        for column in 0..width as usize {
            let pixel = match layer.kind {
                crate::document::LayerKind::Fill => layer.fill,
                crate::document::LayerKind::Paint => {
                    layer.surface.get(x as usize + column, y as usize + row)
                }
                crate::document::LayerKind::Group => [0.0; 4],
            };
            let at = (row * width as usize + column) * 4;
            for channel in 0..4 {
                buffer[at + channel] = (pixel[channel].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    1
}

/// Copy a rectangle of one layer as straight linear Display P3 floats. This is what an
/// extended-range surface uploads: half floats of the same values, so highlights above paper
/// white survive the resident layer path.
///
/// # Safety
/// `out` must point at `width * height * 4` writable `f32` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_layer_rect_linear(
    handle: *mut c_void,
    index: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    out: *mut f32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let Some(layer) = editor.document.layers.get(index as usize) else {
        return 0;
    };
    let matrix = editor.document.color_space.to_display_p3();
    let space = editor.document.color_space;
    let count = width as usize * height as usize * 4;
    // SAFETY: the shell allocated four floats per pixel.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, count) };
    for row in 0..height as usize {
        for column in 0..width as usize {
            let pixel = match layer.kind {
                crate::document::LayerKind::Fill => layer.fill,
                crate::document::LayerKind::Paint => {
                    layer.surface.get(x as usize + column, y as usize + row)
                }
                crate::document::LayerKind::Group => [0.0; 4],
            };
            let alpha = pixel[3].clamp(0.0, 1.0);
            let r = space.decode(pixel[0]);
            let g = space.decode(pixel[1]);
            let b = space.decode(pixel[2]);
            let at = (row * width as usize + column) * 4;
            buffer[at] = (matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b).max(0.0);
            buffer[at + 1] = (matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b).max(0.0);
            buffer[at + 2] = (matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b).max(0.0);
            buffer[at + 3] = alpha;
        }
    }
    1
}

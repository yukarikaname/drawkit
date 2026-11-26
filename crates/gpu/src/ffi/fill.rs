//! GPU fill entry points. The canvas surface applies a mask into a resident layer.

use std::os::raw::c_void;

use super::canvas::canvas;

/// Blend a canvas-sized coverage mask into resident layer `layer`.
///
/// `mask` holds `width * height` floats for the whole canvas, the form the core builds. The write
/// is journaled, so the next `dk_canvas_surface_undo_tiles` restores the previous pixels. When
/// `dirty` is not null the union of the touched rectangles is written to it as x, y, width,
/// height.
///
/// Returns 1 when the fill landed, 0 when the surface refused it and the shell should fall back
/// to the CPU path.
///
/// # Safety
/// `mask` must hold `width * height` floats; `dirty`, when set, must hold four `u32`s.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn dk_canvas_surface_apply_fill(
    handle: *mut c_void,
    layer: u32,
    mask: *const f32,
    width: u32,
    height: u32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
    alpha_locked: i32,
    under: i32,
    dirty: *mut u32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    if mask.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let count = width as usize * height as usize;
    // SAFETY: the shell allocated one float per canvas pixel.
    let mask = unsafe { std::slice::from_raw_parts(mask, count) };
    let applied = surface.apply_fill(
        layer as usize,
        mask,
        width,
        height,
        [red, green, blue, alpha],
        alpha_locked != 0,
        under != 0,
    );
    if !applied {
        return 0;
    }
    if !dirty.is_null()
        && let Some((x, y, rect_width, rect_height)) = surface.fill_dirty()
    {
        // SAFETY: the shell promised room for four values.
        let out = unsafe { std::slice::from_raw_parts_mut(dirty, 4) };
        out.copy_from_slice(&[x, y, rect_width, rect_height]);
    }
    1
}

/// The alpha of layer `index` at one canvas pixel, for a fill that has to decide whether it goes
/// under an existing line. The mask itself is built from the composite, which the shell reads with
/// `dk_canvas_surface_export_gpu`.
///
/// Returns -1 when the surface cannot answer (out of range, a missing tile, no resident stack).
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_layer_alpha_at(
    handle: *mut c_void,
    index: u32,
    x: u32,
    y: u32,
) -> f32 {
    canvas(handle)
        .and_then(|surface| surface.layer_alpha_at(index as usize, x, y))
        .unwrap_or(-1.0)
}

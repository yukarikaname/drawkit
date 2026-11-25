//! GPU stroke preview and commit entry points.

use super::canvas::canvas;
use std::os::raw::c_void;

#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_stroke_begin(
    handle: *mut c_void,
    width: u32,
    height: u32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
    flow: f32,
    pressure_alpha: f32,
    paper: f32,
    tip: f32,
) {
    if let Some(surface) = canvas(handle) {
        surface.stroke_begin(
            width,
            height,
            [red, green, blue, alpha],
            flow,
            pressure_alpha,
            paper,
            tip,
        );
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_canvas_surface_stroke_segment(
    handle: *mut c_void,
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    radius: f32,
    hardness: f32,
    spacing: f32,
    roundness: f32,
    angle: f32,
    pressure: f32,
) {
    if let Some(surface) = canvas(handle) {
        surface.stroke_segment(
            (from_x, from_y),
            (to_x, to_y),
            radius,
            hardness,
            spacing,
            roundness,
            angle,
            pressure,
        );
    }
}

/// Composite the live stroke into a resident layer and keep the replaced tiles for undo. Returns
/// the number of touched tiles.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_stroke_commit(
    handle: *mut c_void,
    layer: u32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    surface.stroke_commit(layer as usize, [red, green, blue], alpha) as i32
}

/// One rectangle of the last commit, as x, y, width, height.
///
/// # Safety
/// `rect` must hold four u32.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_committed_rect(
    handle: *mut c_void,
    index: u32,
    rect: *mut u32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    let Some(tile) = surface.committed_rect(index as usize) else {
        return 0;
    };
    if rect.is_null() {
        return 0;
    }
    // SAFETY: the shell passes room for four u32 values.
    let out = unsafe { std::slice::from_raw_parts_mut(rect, 4) };
    out[0] = tile.0;
    out[1] = tile.1;
    out[2] = tile.2;
    out[3] = tile.3;
    1
}

/// Swap the newest commit's tiles back for undo.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_undo_tiles(handle: *mut c_void) -> i32 {
    canvas(handle).is_some_and(|surface| surface.undo_tiles()) as i32
}

/// Swap the newest undone commit's tiles back in.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_redo_tiles(handle: *mut c_void) -> i32 {
    canvas(handle).is_some_and(|surface| surface.redo_tiles()) as i32
}

/// Read the rectangles whose GPU pixels have not reached the CPU cache. The tiles are then fetched
/// with `dk_canvas_surface_committed_tile`.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_materialize(handle: *mut c_void) -> i32 {
    canvas(handle).is_some_and(|surface| surface.materialize() > 0) as i32
}

/// The rectangle and byte count of one materialised tile, without copying its pixels.
///
/// # Safety
/// `rect` must hold four u32.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_committed_info(
    handle: *mut c_void,
    index: u32,
    rect: *mut u32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    let Some(tile) = surface.committed_tile(index as usize) else {
        return 0;
    };
    if rect.is_null() {
        return 0;
    }
    // SAFETY: the shell promised room for four u32.
    let bounds = unsafe { std::slice::from_raw_parts_mut(rect, 4) };
    bounds[0] = tile.x;
    bounds[1] = tile.y;
    bounds[2] = tile.width;
    bounds[3] = tile.height;
    tile.pixels.len() as i32
}

/// The layer a materialised tile belongs to.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_committed_layer(handle: *mut c_void, index: u32) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    surface
        .committed_tile(index as usize)
        .map_or(-1, |tile| tile.layer as i32)
}

/// Copy one materialised tile into `out` as straight RGBA8. Returns 1 on success.
///
/// # Safety
/// `rect` must hold four u32 and `out` must hold `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_committed_tile(
    handle: *mut c_void,
    index: u32,
    rect: *mut u32,
    out: *mut u8,
    len: u32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    let Some(tile) = surface.committed_tile(index as usize) else {
        return 0;
    };
    if rect.is_null() || out.is_null() || tile.pixels.len() > len as usize {
        return 0;
    }
    // SAFETY: the shell promised room for four u32 and `len` bytes.
    let bounds = unsafe { std::slice::from_raw_parts_mut(rect, 4) };
    bounds[0] = tile.x;
    bounds[1] = tile.y;
    bounds[2] = tile.width;
    bounds[3] = tile.height;
    unsafe { std::ptr::copy_nonoverlapping(tile.pixels.as_ptr(), out, tile.pixels.len()) };
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_clear_committed(handle: *mut c_void) {
    if let Some(surface) = canvas(handle) {
        surface.clear_committed();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_stroke_end(handle: *mut c_void) {
    if let Some(surface) = canvas(handle) {
        surface.stroke_end();
    }
}

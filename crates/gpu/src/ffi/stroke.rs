//! GPU stroke preview and commit entry points.

use super::canvas::canvas;
use std::os::raw::c_void;

/// Begin a GPU stroke preview on a fresh coverage buffer.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_stroke_begin(
    handle: *mut c_void,
    width: u32,
    height: u32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
) {
    if let Some(surface) = canvas(handle) {
        surface.stroke_begin(width, height, [red, green, blue, alpha]);
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
        );
    }
}

/// Composite the live stroke into a resident layer and read the touched tiles back. Returns the
/// number of tiles.
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

/// True once the reads from the last commit have landed. Never blocks.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_commit_ready(handle: *mut c_void) -> i32 {
    canvas(handle).is_some_and(|surface| surface.commit_ready()) as i32
}

/// Collect the finished tiles for the shell and free the staging buffers blocked the next commit.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_commit_finish(handle: *mut c_void) -> i32 {
    canvas(handle).is_some_and(|surface| surface.commit_finish()) as i32
}

/// The rectangle and byte count of one committed tile, without copying its pixels.
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

/// Copy one tile from the last commit into `out` as straight RGBA8. Returns 1 on success.
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

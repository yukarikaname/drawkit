//! Incremental resident-layer entry points. A structural edit keeps the other layers' textures
//! and only rearranges the stack, so the shell does not rebuild every layer.

use std::os::raw::c_void;

use super::canvas::canvas;

/// Number of layers the resident stack holds.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_layer_count(handle: *mut c_void) -> u32 {
    canvas(handle).map_or(0, |surface| surface.layer_count() as u32)
}

/// Insert an empty resident layer at `index` on a surface whose canvas is `width * height`.
///
/// Returns 1 on success; 0 when the handle is null, `index` is past the end, the size does not
/// match the canvas, or the stack cannot take the layer. Group and clip references above the
/// insertion move up with their layers. The tile undo journal is dropped, because its steps name
/// layer positions; the shell rebuilds its GPU history, as after `reset_layers`.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_insert_layer(
    handle: *mut c_void,
    index: u32,
    width: u32,
    height: u32,
) -> i32 {
    canvas(handle).is_some_and(|surface| surface.insert_layer(index as usize, width, height)) as i32
}

/// Remove layer `index` and shift the layers above it down.
///
/// Returns 1 when the layer was removed. Returns 0, with the stack untouched, when the handle is
/// null, the index does not exist, or another layer groups under or clips through it; ungroup or
/// unclip that layer first.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_remove_layer(handle: *mut c_void, index: u32) -> i32 {
    canvas(handle).is_some_and(|surface| surface.remove_layer(index as usize)) as i32
}

/// Move layer `from` to `to`. Only the draw order changes; pixels, groups and clips follow their
/// layer.
///
/// Returns 1 on success; 0 when the handle is null or either index does not exist.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_move_layer(handle: *mut c_void, from: u32, to: u32) -> i32 {
    canvas(handle).is_some_and(|surface| surface.move_layer(from as usize, to as usize)) as i32
}

fn write_count(count_out: *mut u32, count: usize) {
    if !count_out.is_null() {
        // SAFETY: the caller promised one writable `u32`.
        unsafe { *count_out = count as u32 };
    }
}

fn write_missing(rects: &[(usize, u32, u32, u32, u32)], out: *mut u32) {
    // SAFETY: the caller promised room for one packed rectangle per entry.
    let words = unsafe { std::slice::from_raw_parts_mut(out, rects.len() * 5) };
    for (index, (layer, x, y, width, height)) in rects.iter().enumerate() {
        let at = index * 5;
        words[at..at + 5].copy_from_slice(&[*layer as u32, *x, *y, *width, *height]);
    }
}

/// Write the tiles the byte budget evicted as packed `[layer, x, y, width, height]`.
///
/// The shell re-uploads each rectangle from the core surface before the next present. Returns 1
/// when every rectangle was written, 0 when the handle is null, nothing is missing, or `capacity`
/// is too small. On a short buffer `*count_out` holds the needed capacity and nothing is written;
/// `out` may be null for that query.
///
/// # Safety
/// `out`, when not null, must hold `capacity * 5` `u32` values; `count_out`, when not null, must
/// point at one writable `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_missing_rects(
    handle: *mut c_void,
    out: *mut u32,
    capacity: u32,
    count_out: *mut u32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        write_count(count_out, 0);
        return 0;
    };
    let missing = surface.missing_rects();
    if missing.is_empty() || out.is_null() || (capacity as usize) < missing.len() {
        write_count(count_out, missing.len());
        return 0;
    }
    write_missing(&missing, out);
    write_count(count_out, missing.len());
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_null_handle_has_no_layers() {
        assert_eq!(dk_canvas_surface_layer_count(std::ptr::null_mut()), 0);
    }

    #[test]
    fn a_null_handle_refuses_every_edit() {
        assert_eq!(
            dk_canvas_surface_insert_layer(std::ptr::null_mut(), 0, 1, 1),
            0
        );
        assert_eq!(dk_canvas_surface_remove_layer(std::ptr::null_mut(), 0), 0);
        assert_eq!(dk_canvas_surface_move_layer(std::ptr::null_mut(), 0, 1), 0);
    }

    #[test]
    fn missing_rects_pack_the_layer_before_the_bounds() {
        let rects = [(2usize, 0, 0, 1024, 1024), (5, 1024, 0, 1024, 1024)];
        let mut words = [0u32; 10];
        write_missing(&rects, words.as_mut_ptr());
        assert_eq!(
            words,
            [2, 0, 0, 1024, 1024, 5, 1024, 0, 1024, 1024]
        );
    }

    #[test]
    fn a_null_handle_reports_no_missing_tiles() {
        let mut count = 7u32;
        assert_eq!(
            unsafe {
                dk_canvas_surface_missing_rects(
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                    &mut count,
                )
            },
            0
        );
        assert_eq!(count, 0);
    }
}

//! Undo and redo that report the rectangles they restored, so a shell re-uploads those tiles
//! instead of rebuilding the whole layer. The step is only taken when the shell's buffer can hold
//! every rectangle; a short buffer leaves the history untouched and reports the needed count.

use std::os::raw::c_void;

use super::canvas::canvas;

fn write_rects(rects: &[(u32, u32, u32, u32)], out: *mut u32) {
    // SAFETY: the caller promised room for `rects.len()` packed rectangles.
    let words = unsafe { std::slice::from_raw_parts_mut(out, rects.len() * 4) };
    for (index, rect) in rects.iter().enumerate() {
        let at = index * 4;
        words[at..at + 4].copy_from_slice(&[rect.0, rect.1, rect.2, rect.3]);
    }
}

fn write_count(count_out: *mut u32, count: usize) {
    if !count_out.is_null() {
        // SAFETY: the caller promised one writable `u32`.
        unsafe { *count_out = count as u32 };
    }
}

/// Undo the newest GPU commit and write the rectangles it restored as packed `[x, y, w, h]`.
///
/// Returns 1 when the step was undone and every rectangle was written, 0 when the handle is null,
/// there is nothing to undo, or `capacity` is too small. On a short buffer `*count_out` holds the
/// needed capacity and nothing is undone; `out` may be null for that query.
///
/// # Safety
/// `out`, when not null, must hold `capacity * 4` `u32` values; `count_out`, when not null, must
/// point at one writable `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_undo_tiles_rects(
    handle: *mut c_void,
    out: *mut u32,
    capacity: u32,
    count_out: *mut u32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        write_count(count_out, 0);
        return 0;
    };
    let total = surface.pending_undo_rects().map_or(0, |rects| rects.len());
    if total == 0 || out.is_null() || (capacity as usize) < total {
        write_count(count_out, total);
        return 0;
    }
    if !surface.undo_tiles() {
        write_count(count_out, 0);
        return 0;
    }
    let rects = surface.committed_rects();
    write_rects(rects, out);
    write_count(count_out, rects.len());
    1
}

/// The redo twin of `dk_canvas_surface_undo_tiles_rects`, with the same capacity rule.
///
/// # Safety
/// `out`, when not null, must hold `capacity * 4` `u32` values; `count_out`, when not null, must
/// point at one writable `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_redo_tiles_rects(
    handle: *mut c_void,
    out: *mut u32,
    capacity: u32,
    count_out: *mut u32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        write_count(count_out, 0);
        return 0;
    };
    let total = surface.pending_redo_rects().map_or(0, |rects| rects.len());
    if total == 0 || out.is_null() || (capacity as usize) < total {
        write_count(count_out, total);
        return 0;
    }
    if !surface.redo_tiles() {
        write_count(count_out, 0);
        return 0;
    }
    let rects = surface.committed_rects();
    write_rects(rects, out);
    write_count(count_out, rects.len());
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rectangles_are_packed_in_order() {
        let rects = [(1, 2, 3, 4), (5, 6, 7, 8)];
        let mut words = [0u32; 8];
        write_rects(&rects, words.as_mut_ptr());
        assert_eq!(words, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn a_null_handle_reports_nothing() {
        let mut count = 7u32;
        assert_eq!(
            unsafe {
                dk_canvas_surface_undo_tiles_rects(
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

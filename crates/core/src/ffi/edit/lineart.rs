//! Line-art extraction entry points. Split out of `edit.rs` to keep every shell-facing file under
//! the line budget; the symbols themselves are unchanged.

use super::*;

/// Extract the visible painting into a new line layer. Returns 1 when a layer was added.
#[unsafe(no_mangle)]
pub extern "C" fn dk_extract_lineart(handle: *mut c_void, threshold: f32, softness: f32) -> i32 {
    editor(handle).is_some_and(|editor| {
        editor
            .extract_line_art(drawkit_lineart::LineArtOptions {
                threshold,
                softness,
            })
            .is_some()
    }) as i32
}

/// Extract the visible painting and clean the result before inserting the layer. The three cleanup
/// distances are in document pixels: gap close, endpoint join and minimum speck size. All zero is
/// the same work as [`dk_extract_lineart`]. Returns 1 when a layer was added.
#[unsafe(no_mangle)]
pub extern "C" fn dk_extract_lineart_clean(
    handle: *mut c_void,
    threshold: f32,
    softness: f32,
    close: u32,
    join: u32,
    speck: u32,
) -> i32 {
    editor(handle).is_some_and(|editor| {
        editor
            .extract_line_art_clean(
                drawkit_lineart::LineArtOptions {
                    threshold,
                    softness,
                },
                crate::tools::CleanOptions {
                    close_px: close,
                    join_px: join,
                    speck_px: speck,
                },
            )
            .is_some()
    }) as i32
}

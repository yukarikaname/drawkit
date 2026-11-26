//! Per-layer GPU thumbnails for the layer palette.

use super::canvas::canvas;
use std::os::raw::c_void;

/// Downscale one resident layer into premultiplied RGBA8 for the palette.
///
/// # Safety
/// `out` must point to `width * height * 4` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_layer_thumbnail(
    handle: *mut c_void,
    layer: u32,
    width: u32,
    height: u32,
    out: *mut u8,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let Some(pixels) = surface.layer_thumbnail(layer as usize, width, height) else {
        return 0;
    };
    let expected = width as usize * height as usize * 4;
    if pixels.len() != expected {
        return 0;
    }
    // SAFETY: the shell promised room for the whole thumbnail.
    unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), out, expected) };
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_null_handle_draws_nothing() {
        let mut out = [0u8; 4];
        // SAFETY: `out` holds the four bytes of one pixel and the handle is null.
        let drawn = unsafe {
            dk_canvas_surface_layer_thumbnail(std::ptr::null_mut(), 0, 1, 1, out.as_mut_ptr())
        };
        assert_eq!(drawn, 0);
    }
}

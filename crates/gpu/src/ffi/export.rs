//! Read the resident composite back at export size.

use super::canvas::canvas;
use std::os::raw::c_void;

/// Composite the resident layers into `width` by `height` premultiplied RGBA8.
///
/// Returns 0 for a null handle or buffer, a short buffer, or a surface that cannot export (an
/// extended-range stack). The shell then falls back to its CPU composite.
///
/// # Safety
/// `out` must point to `capacity` writable bytes, at least `width * height * 4`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_export_gpu(
    handle: *mut c_void,
    width: u32,
    height: u32,
    out: *mut u8,
    capacity: u32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let expected = width as usize * height as usize * 4;
    if (capacity as usize) < expected {
        return 0;
    }
    let Some(pixels) = surface.export_frame(width, height) else {
        return 0;
    };
    if pixels.len() != expected {
        return 0;
    }
    // SAFETY: the shell promised `capacity` bytes, which covers the whole frame.
    unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), out, expected) };
    1
}

/// Mark layer `index` as a reference: the canvas shows it, an export leaves it out.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_set_export_reference(
    handle: *mut c_void,
    index: u32,
    reference: i32,
) {
    if let Some(surface) = canvas(handle) {
        surface.set_export_reference(index as usize, reference != 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_null_handle_exports_nothing() {
        let mut out = [0u8; 4];
        // SAFETY: `out` holds one pixel and the handle is null.
        let exported = unsafe {
            dk_canvas_surface_export_gpu(
                std::ptr::null_mut(),
                1,
                1,
                out.as_mut_ptr(),
                out.len() as u32,
            )
        };
        assert_eq!(exported, 0);
    }

    #[test]
    fn a_null_buffer_is_refused() {
        // SAFETY: the handle is null, so no surface is touched.
        let exported = unsafe {
            dk_canvas_surface_export_gpu(std::ptr::null_mut(), 4, 4, std::ptr::null_mut(), 64)
        };
        assert_eq!(exported, 0);
    }
}

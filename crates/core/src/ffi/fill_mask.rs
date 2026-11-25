//! A fill mask for a host that has the composed frame but no CPU document.

use crate::fill::FillOptions;
use crate::fill_mask::mask_from_composite;

/// Build a fill mask from premultiplied RGBA8 over paper and write one float per canvas pixel.
///
/// # Safety
/// `composite` must hold `width * height * 4` bytes and `out` `width * height` floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_fill_mask(
    composite: *const u8,
    width: u32,
    height: u32,
    seed_x: u32,
    seed_y: u32,
    tolerance: f32,
    close: u32,
    expand: u32,
    feather: u32,
    out: *mut f32,
    len: u32,
) -> i32 {
    if composite.is_null() || out.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let pixels = width as usize * height as usize;
    if (len as usize) < pixels {
        return 0;
    }
    // SAFETY: the shell allocated the composite and the mask for this canvas.
    let source = unsafe { std::slice::from_raw_parts(composite, pixels * 4) };
    let Some(mask) = mask_from_composite(
        source,
        width as usize,
        height as usize,
        (seed_x as usize, seed_y as usize),
        tolerance,
        FillOptions {
            close: close as usize,
            expand: expand as usize,
            feather: feather as usize,
        },
    ) else {
        return 0;
    };
    // SAFETY: the shell promised room for one float per pixel.
    let target = unsafe { std::slice::from_raw_parts_mut(out, pixels) };
    target.copy_from_slice(&mask.data);
    1
}

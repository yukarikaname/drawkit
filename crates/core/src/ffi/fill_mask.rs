//! A fill mask for a host that has the composed frame but no CPU document, and the mask a
//! resident-layer surface applies itself.

use super::editor;
use crate::fill::FillOptions;
use crate::fill_mask::mask_from_composite;
use crate::surface::Rect;

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
    smart: i32,
    under: i32,
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
            smart: smart != 0,
        },
        under != 0,
    ) else {
        return 0;
    };
    // SAFETY: the shell promised room for one float per pixel.
    let target = unsafe { std::slice::from_raw_parts_mut(out, pixels) };
    target.copy_from_slice(&mask.data);
    1
}

/// Build a fill for a surface that applies the mask into its resident tiles.
///
/// Writes one float per canvas pixel to `mask`, and the fill's layer, bounds and blending state to
/// `rect` as `[layer, x, y, width, height, under, alpha_locked]`. `colour` receives the colour the
/// fill will use, after snapping, as four floats. Nothing is written to the document.
///
/// Returns 1 on success. The mask's length is the canvas area the editor was created with, so
/// `len` has to match it exactly; a mismatch cancels the prepared fill.
///
/// # Safety
/// `mask` must hold `len` floats, `rect` seven `u32`s and `colour` four floats.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn dk_fill_gpu_begin(
    handle: *mut std::ffi::c_void,
    x: u32,
    y: u32,
    tolerance: f32,
    close: u32,
    expand: u32,
    feather: u32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
    time_ms: u64,
    mask: *mut f32,
    len: u32,
    rect: *mut u32,
    colour: *mut f32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if mask.is_null() || rect.is_null() || colour.is_null() || len == 0 {
        return 0;
    }
    let options = FillOptions {
        close: close as usize,
        expand: expand as usize,
        feather: feather as usize,
        smart: editor.fill_smart,
    };
    if editor
        .begin_gpu_fill(
            x as usize,
            y as usize,
            tolerance,
            [red, green, blue, alpha],
            options,
            time_ms,
        )
        .is_none()
    {
        return 0;
    }
    let count = editor.gpu_fill_mask().map_or(0, <[f32]>::len);
    if count == 0 || len as usize != count {
        editor.cancel_gpu_fill();
        return 0;
    }
    let Some((layer, bounds, color, under, alpha_locked)) = editor.gpu_fill_state() else {
        editor.cancel_gpu_fill();
        return 0;
    };
    // SAFETY: the shell promised `len` floats, seven u32s and four floats. The mask is copied out
    // before the editor is touched again.
    unsafe {
        let source = editor.gpu_fill_mask().expect("the plan was just checked");
        std::ptr::copy_nonoverlapping(source.as_ptr(), mask, count);
        let out = std::slice::from_raw_parts_mut(rect, 7);
        out[0] = layer as u32;
        out[1] = bounds.x as u32;
        out[2] = bounds.y as u32;
        out[3] = bounds.width as u32;
        out[4] = bounds.height as u32;
        out[5] = u32::from(under);
        out[6] = u32::from(alpha_locked);
        std::slice::from_raw_parts_mut(colour, 4).copy_from_slice(&color);
    }
    1
}

/// Record a GPU fill the surface has written into the resident tiles.
///
/// `rects` holds `count` rectangles of four `u32`s each. The pixels stay in the tile journal; the
/// history keeps only these rectangles, exactly as for a GPU stroke.
///
/// # Safety
/// `rects` must hold `count * 4` `u32`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_fill_gpu_finish(
    handle: *mut std::ffi::c_void,
    rects: *const u32,
    count: u32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if rects.is_null() || count == 0 {
        editor.cancel_gpu_fill();
        return 0;
    }
    // SAFETY: the shell passes four values per rectangle.
    let values = unsafe { std::slice::from_raw_parts(rects, count as usize * 4) };
    let rects: Vec<Rect> = values
        .as_chunks::<4>()
        .0
        .iter()
        .map(|r| Rect::new(r[0] as usize, r[1] as usize, r[2] as usize, r[3] as usize))
        .collect();
    i32::from(editor.finish_gpu_fill(&rects))
}

/// Drop a prepared GPU fill without recording it.
#[unsafe(no_mangle)]
pub extern "C" fn dk_fill_gpu_cancel(handle: *mut std::ffi::c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.cancel_gpu_fill()) as i32
}

use super::*;

/// The visible colour at `x, y`, written as four floats, for an eyedropper.
/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `out` must point to four writable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_pick(handle: *mut c_void, x: u32, y: u32, out: *mut f32) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let Some(color) = editor.pick(x as usize, y as usize) else {
        return 0;
    };
    // SAFETY: the shell passed room for four floats, which is the whole contract.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, 4) };
    buffer.copy_from_slice(&color);
    1
}

/// Write a premultiplied RGBA8 hue wheel of `size` pixels square.
/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `out` must point to `size * size * 4` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_picker_wheel(size: u32, out: *mut u8) -> i32 {
    if out.is_null() || size == 0 || size > 1024 {
        return 0;
    }
    let bytes = crate::picker::wheel_rgba(size as usize);
    // SAFETY: the shell allocated size * size * 4 bytes.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, size as usize * size as usize * 4) };
    buffer.copy_from_slice(&bytes);
    1
}

/// Write a premultiplied RGBA8 saturation/value square for one hue.
/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `out` must point to `width * height * 4` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_picker_sv(hue: f32, width: u32, height: u32, out: *mut u8) -> i32 {
    if out.is_null() || width == 0 || height == 0 || width > 1024 || height > 1024 {
        return 0;
    }
    let bytes = crate::picker::sv_rgba(hue, width as usize, height as usize);
    // SAFETY: the shell allocated width * height * 4 bytes.
    let buffer =
        unsafe { std::slice::from_raw_parts_mut(out, width as usize * height as usize * 4) };
    buffer.copy_from_slice(&bytes);
    1
}

/// Write up to `max` dominant painted colours as straight RGBA floats.
/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `out` must point to `max * 4` writable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_palette(handle: *mut c_void, max: u32, out: *mut f32) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() || max == 0 || max > 32 {
        return 0;
    }
    let (width, height) = (editor.document.width(), editor.document.height());
    let composite: Vec<crate::color::Rgba> = editor
        .document
        .composite_rect(crate::surface::Rect::new(0, 0, width, height))
        .into_iter()
        .map(|pixel| crate::color::over(pixel, crate::tools::PAPER))
        .collect();
    let colors =
        crate::picker::palette(&composite, width, height, crate::tools::PAPER, max as usize);
    // SAFETY: the shell allocated max * 4 floats.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, max as usize * 4) };
    let mut written = 0;
    for (index, color) in colors.iter().enumerate() {
        buffer[index * 4..index * 4 + 4].copy_from_slice(color);
        written = index + 1;
    }
    written as i32
}

use super::*;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_composite(handle: *mut c_void, out: *mut u8) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let (width, height) = (editor.document.width(), editor.document.height());
    let pixels = editor
        .document
        .composite_rect(crate::surface::Rect::new(0, 0, width, height));
    // SAFETY: the shell allocated `width * height * 4` bytes for exactly this.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, width * height * 4) };
    for (index, pixel) in pixels.iter().enumerate() {
        let alpha = pixel[3].clamp(0.0, 1.0);
        buffer[index * 4] = (pixel[0].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 1] = (pixel[1].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 2] = (pixel[2].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 3] = (alpha * 255.0).round() as u8;
    }
    1
}

/// Composite the visible canvas without reference layers, for a file export.
/// # Safety
///
/// `out` must point to `width * height * 4` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_composite_export(handle: *mut c_void, out: *mut u8) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let (width, height) = (editor.document.width(), editor.document.height());
    let pixels = editor
        .document
        .composite_rect_export(crate::surface::Rect::new(0, 0, width, height));
    // SAFETY: the shell allocated `width * height * 4` bytes for exactly this.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, width * height * 4) };
    for (index, pixel) in pixels.iter().enumerate() {
        let alpha = pixel[3].clamp(0.0, 1.0);
        buffer[index * 4] = (pixel[0].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 1] = (pixel[1].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 2] = (pixel[2].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 3] = (alpha * 255.0).round() as u8;
    }
    1
}

/// Fill the active layer with one colour, for "new document" and "clear".
#[unsafe(no_mangle)]
pub extern "C" fn dk_record_enabled(handle: *mut c_void, enabled: i32) {
    if let Some(editor) = editor(handle) {
        editor.recording_enabled = enabled != 0;
    }
}

/// How many events the tape holds, so a shell can tell an empty recording from a blank one.
#[unsafe(no_mangle)]
pub extern "C" fn dk_recording_events(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| editor.recording.events().len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_recording_ms(handle: *mut c_void) -> u64 {
    editor(handle).map_or(0, |editor| editor.recording.duration_ms())
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_recording_clear(handle: *mut c_void) {
    if let Some(editor) = editor(handle) {
        editor.recording.clear();
    }
}

/// Composite the recording played up to `time_ms` into `out`, without touching the document.
/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `out` must point to `width * height * 4` writable bytes for the document size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_replay_composite(
    handle: *mut c_void,
    time_ms: u64,
    out: *mut u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let pixels = editor.replay_composite(time_ms);
    // SAFETY: the shell allocated `width * height * 4` bytes for exactly this.
    unsafe { write_rgba8(&pixels, out) };
    1
}

/// The same replay at `width` by `height`, for a scrub preview smaller than the canvas.
///
/// # Safety
///
/// `out` must point at `width * height * 4` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_replay_preview(
    handle: *mut c_void,
    time_ms: u64,
    width: u32,
    height: u32,
    out: *mut u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 || width > 8192 || height > 8192 {
        return 0;
    }
    let pixels = editor.replay_composite_scaled(time_ms, width as usize, height as usize);
    if pixels.len() < width as usize * height as usize {
        return 0;
    }
    // SAFETY: the shell allocated `width * height * 4` bytes for exactly this.
    unsafe { write_rgba8(&pixels, out) };
    1
}

/// The whole painting at `width` by `height`, opaque RGBA8 over paper, for a navigator.
///
/// # Safety
/// `out` must point at `width * height * 4` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_overview(
    handle: *mut c_void,
    width: u32,
    height: u32,
    out: *mut u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 || width > 512 || height > 512 {
        return 0;
    }
    let pixels = editor.overview(width as usize, height as usize);
    let count = width as usize * height as usize * 4;
    if pixels.len() < count {
        return 0;
    }
    // SAFETY: the shell allocated `width * height * 4` bytes.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, count) };
    buffer.copy_from_slice(&pixels[..count]);
    1
}

/// One timelapse frame at `width` by `height`, composited over paper and written as opaque BGRA,
/// which is the layout a video encoder's pixel buffer wants.
///
/// # Safety
///
/// `out` must point at `width * height * 4` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_video_frame(
    handle: *mut c_void,
    time_ms: u64,
    width: u32,
    height: u32,
    out: *mut u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 || width > 8192 || height > 8192 {
        return 0;
    }
    let pixels = editor.video_frame(time_ms, width as usize, height as usize);
    let count = width as usize * height as usize * 4;
    if pixels.len() < count {
        return 0;
    }
    // SAFETY: the shell allocated `width * height * 4` bytes for exactly this.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, count) };
    buffer.copy_from_slice(&pixels[..count]);
    1
}

/// Write straight, premultiplied RGBA as the 8-bit bytes the canvas texture takes.
///
/// # Safety
/// `out` must hold four bytes per pixel in `pixels`.
unsafe fn write_rgba8(pixels: &[[f32; 4]], out: *mut u8) {
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, pixels.len() * 4) };
    for (index, pixel) in pixels.iter().enumerate() {
        let alpha = pixel[3].clamp(0.0, 1.0);
        buffer[index * 4] = (pixel[0].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 1] = (pixel[1].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 2] = (pixel[2].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 3] = (alpha * 255.0).round() as u8;
    }
}

/// Composite one rectangle, `width * height * 4` bytes, for a shell that uploads only what changed.
/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `out` must point to `width * height * 4` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_composite_rect(
    handle: *mut c_void,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    out: *mut u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let rect = crate::surface::Rect::new(x as usize, y as usize, width as usize, height as usize);
    let pixels = editor.document.composite_rect(rect);
    // SAFETY: the shell allocated `width * height * 4` bytes for exactly this.
    let buffer =
        unsafe { std::slice::from_raw_parts_mut(out, width as usize * height as usize * 4) };
    for (index, pixel) in pixels.iter().enumerate() {
        let alpha = pixel[3].clamp(0.0, 1.0);
        buffer[index * 4] = (pixel[0].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 1] = (pixel[1].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 2] = (pixel[2].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        buffer[index * 4 + 3] = (alpha * 255.0).round() as u8;
    }
    1
}

/// Take the rectangle the last operation dirtied, as `x, y, width, height`, and clear it.
///
/// The shell uploads what changed instead of the canvas. A shell that misses the rectangle gets
/// zero back and should composite everything: a stale frame is worse than a slow one.
/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `out` must point to four writable `u32` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_take_dirty(handle: *mut c_void, out: *mut u32) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let Some(rect) = editor.document.take_dirty() else {
        return 0;
    };
    // SAFETY: the shell passed room for four u32, which is the whole contract.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, 4) };
    buffer[0] = rect.x as u32;
    buffer[1] = rect.y as u32;
    buffer[2] = rect.width as u32;
    buffer[3] = rect.height as u32;
    1
}

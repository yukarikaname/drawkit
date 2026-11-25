use super::*;

#[unsafe(no_mangle)]
pub extern "C" fn dk_frame_count(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| editor.frame_count() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_frame_active(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| editor.active_frame() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_frame_add(handle: *mut c_void, duplicate: i32) -> i32 {
    editor(handle).map_or(-1, |editor| editor.add_frame(duplicate != 0) as i32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_frame_select(handle: *mut c_void, index: u32) -> i32 {
    editor(handle).is_some_and(|editor| editor.select_frame(index as usize)) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_frame_remove(handle: *mut c_void, index: u32) -> i32 {
    editor(handle).is_some_and(|editor| editor.remove_frame(index as usize)) as i32
}

/// Premultiplied RGBA8 for one frame, `width * height * 4` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_frame_composite(handle: *mut c_void, index: u32, out: *mut u8) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let length = editor.document.width() * editor.document.height() * 4;
    // SAFETY: the shell allocated exactly one canvas of RGBA8.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, length) };
    editor.frame_image(index as usize, buffer) as i32
}

/// Premultiplied RGBA8 onion skin: previous frames red, next frames green.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_onion_composite(
    handle: *mut c_void,
    before: u32,
    after: u32,
    out: *mut u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let length = editor.document.width() * editor.document.height() * 4;
    // SAFETY: the shell allocated exactly one canvas of RGBA8.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, length) };
    editor.onion_image(before as usize, after as usize, buffer) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_fps(handle: *mut c_void) -> u32 {
    editor(handle).map_or(12, |editor| editor.fps as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_fps(handle: *mut c_void, fps: u32) {
    if let Some(editor) = editor(handle) {
        editor.fps = fps.clamp(1, 60) as u16;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_frame_duration(handle: *mut c_void) -> u32 {
    editor(handle).map_or(83, |editor| editor.frame_duration)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_frame_duration(handle: *mut c_void, duration_ms: u32) {
    if let Some(editor) = editor(handle) {
        editor.frame_duration = duration_ms.clamp(10, 10_000);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_frame_duration_at(handle: *mut c_void, index: u32) -> u32 {
    editor(handle)
        .and_then(|editor| editor.frame_duration_at(index as usize))
        .unwrap_or(83)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_loop_enabled(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.loop_enabled) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_loop_enabled(handle: *mut c_void, enabled: i32) {
    if let Some(editor) = editor(handle) {
        editor.loop_enabled = enabled != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_loop_start(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| editor.loop_range().0 as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_loop_start(handle: *mut c_void, index: u32) {
    if let Some(editor) = editor(handle) {
        let (_, end) = editor.loop_range();
        editor.loop_start = (index as usize).min(end.saturating_sub(1));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_loop_end(handle: *mut c_void) -> u32 {
    editor(handle).map_or(1, |editor| editor.loop_range().1 as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_loop_end(handle: *mut c_void, end: u32) {
    if let Some(editor) = editor(handle) {
        let (start, _) = editor.loop_range();
        editor.loop_end = (end as usize).max(start + 1);
    }
}

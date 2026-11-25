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

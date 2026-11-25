//! Brush files: import replaces the current brush, export writes the current one.

use std::os::raw::c_char;

use super::*;

/// Write the current brush as a MyPaint `.myb`.
///
/// # Safety
/// `path` must be a NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_export_brush(handle: *mut c_void, path: *const c_char) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if path.is_null() {
        return 0;
    }
    // SAFETY: the shell passed a NUL-terminated path.
    let Ok(path) = unsafe { CStr::from_ptr(path) }.to_str() else {
        return 0;
    };
    let preset = crate::brush::BrushPreset::from_current(editor.brush.kind.name(), &editor.brush);
    i32::from(preset.save(std::path::Path::new(path)).is_ok())
}

/// Load a MyPaint brush and make it current. Returns the brush kind, or -1 when the file is not a brush.
///
/// # Safety
/// `path` must be a NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_import_brush(handle: *mut c_void, path: *const c_char) -> i32 {
    let Some(editor) = editor(handle) else {
        return -1;
    };
    if path.is_null() {
        return -1;
    }
    // SAFETY: the shell passed a NUL-terminated path.
    let Ok(path) = unsafe { CStr::from_ptr(path) }.to_str() else {
        return -1;
    };
    let Ok(preset) = crate::brush::BrushPreset::load(std::path::Path::new(path)) else {
        return -1;
    };
    editor.brush = preset.brush;
    editor.brush.kind.as_i32()
}

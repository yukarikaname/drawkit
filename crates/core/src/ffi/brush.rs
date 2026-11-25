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
    let slot = editor.brush.kind.as_i32() as usize;
    if let Some(stored) = editor.brushes.get_mut(slot) {
        *stored = editor.brush;
    }
    editor.brush.kind.as_i32()
}

/// Render a preview stroke. `kind` is a preset index, or -1 for the editor's current brush.
///
/// # Safety
/// `out` must hold `width * height * 4` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_brush_preview(
    handle: *mut c_void,
    kind: i32,
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
    let brush = if kind < 0 {
        editor.brush
    } else {
        editor
            .brushes
            .get(kind as usize)
            .copied()
            .unwrap_or_else(|| {
                crate::brush::Brush::for_kind(crate::brush::BrushKind::from_i32(kind))
            })
    };
    let pixels = crate::brush::preview(&brush, width as usize, height as usize);
    // SAFETY: the shell allocated exactly one RGBA8 preview.
    unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), out, pixels.len()) };
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_brush_param(handle: *mut c_void, id: u32) -> f32 {
    editor(handle)
        .and_then(|editor| editor.brush.param(id as u8))
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_brush_param(handle: *mut c_void, id: u32, value: f32) {
    if let Some(editor) = editor(handle) {
        editor.set_brush_param(id as u8, value);
    }
}

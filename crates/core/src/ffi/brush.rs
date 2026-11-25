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
    let pixels = crate::brush::preview(
        &brush,
        editor.brush_texture.as_deref(),
        width as usize,
        height as usize,
    );
    // SAFETY: the shell allocated exactly one RGBA8 preview.
    unsafe { std::ptr::copy_nonoverlapping(pixels.as_ptr(), out, pixels.len()) };
    1
}

/// The mirrored copies of one segment, four floats each: from x, from y, to x, to y. The first
/// entry is the original segment, so a shell can always draw what this returns.
///
/// # Safety
/// `out` must hold room for 16 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_mirror_segments(
    handle: *mut c_void,
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    out: *mut f32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let width = editor.document.width();
    let height = editor.document.height();
    let (from, to) = editor
        .last_segment()
        .unwrap_or(((from_x, from_y), (to_x, to_y)));
    let (variants, count) = editor.mirror.variants(width, height, from, to);
    // SAFETY: the shell passes room for four variants.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, 16) };
    for (index, ((fx, fy), (tx, ty))) in variants[..count].iter().enumerate() {
        buffer[index * 4] = *fx;
        buffer[index * 4 + 1] = *fy;
        buffer[index * 4 + 2] = *tx;
        buffer[index * 4 + 3] = *ty;
    }
    count as i32
}

/// The seed of the stroke in progress, as low and high halves.
///
/// # Safety
/// `out` must hold two u32 values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_stroke_seed(handle: *mut c_void, out: *mut u32) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some(seed) = editor.stroke_seed() else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    // SAFETY: the shell passes room for two u32 values.
    let halves = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    halves[0] = seed as u32;
    halves[1] = (seed >> 32) as u32;
    1
}

/// The imported map's size, as width and height.
///
/// # Safety
/// `out` must hold two u32 values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_brush_texture_size(handle: *mut c_void, out: *mut u32) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some(texture) = editor.brush_texture.as_ref() else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    // SAFETY: the shell passes room for two u32 values.
    let size = unsafe { std::slice::from_raw_parts_mut(out, 2) };
    size[0] = texture.width as u32;
    size[1] = texture.height as u32;
    1
}

/// One parameter of a brush preset slot, without switching the current brush.
#[unsafe(no_mangle)]
pub extern "C" fn dk_brush_kind_param(kind: i32, id: u32) -> f32 {
    let kind = crate::brush::BrushKind::from_i32(kind);
    crate::brush::Brush::for_kind(kind)
        .param(id as u8)
        .unwrap_or(0.0)
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

/// Replace the current slot with the built-in preset again.
#[unsafe(no_mangle)]
pub extern "C" fn dk_reset_brush(handle: *mut c_void) {
    if let Some(editor) = editor(handle) {
        let kind = editor.brush.kind;
        let slot = kind.as_i32() as usize;
        let preset = crate::brush::Brush::for_kind(kind);
        editor.brush = preset;
        if let Some(stored) = editor.brushes.get_mut(slot) {
            *stored = preset;
        }
    }
}

/// Load a PNG/JPEG as the shared paper/tip texture, scaled to at most 128×128 grayscale.
///
/// # Safety
/// `path` must be a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_load_brush_texture(handle: *mut c_void, path: *const c_char) -> i32 {
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
    let Ok(imported) = drawkit_formats::read_image(std::path::Path::new(path)) else {
        return 0;
    };
    let Some(layer) = imported.layers.first() else {
        return 0;
    };
    let Some(texture) =
        crate::brush::BrushTexture::from_rgba(layer.width, layer.height, &layer.rgba, 128)
    else {
        return 0;
    };
    editor.brush_texture = Some(std::sync::Arc::new(texture));
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_clear_brush_texture(handle: *mut c_void) {
    if let Some(editor) = editor(handle) {
        editor.brush_texture = None;
    }
}

/// Opaque RGBA render of the shared texture, for the studio thumbnail.
///
/// # Safety
/// `out` must hold `width * height * 4` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_brush_texture_preview(
    handle: *mut c_void,
    width: u32,
    height: u32,
    out: *mut u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some(texture) = editor.brush_texture.as_deref() else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let length = width as usize * height as usize * 4;
    // SAFETY: the shell allocated exactly one RGBA thumbnail.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, length) };
    for y in 0..height as usize {
        for x in 0..width as usize {
            let value = texture.sample(x as f32 / width as f32, y as f32 / height as f32);
            let gray = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            let at = (y * width as usize + x) * 4;
            buffer[at] = gray;
            buffer[at + 1] = gray;
            buffer[at + 2] = gray;
            buffer[at + 3] = 255;
        }
    }
    1
}

/// Record strokes without rasterising them; end_stroke rebuilds the CPU pixels once. The shell
/// uses this while a GPU preview owns the live stroke.
#[unsafe(no_mangle)]
pub extern "C" fn dk_set_defer_raster(handle: *mut c_void, enabled: i32) {
    if let Some(editor) = editor(handle) {
        editor.defer_raster = enabled != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_deferred_pending(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.deferred_pending()) as i32
}

/// Apply at most `commands` recorded segments. 1 means complete, 0 still pending.
#[unsafe(no_mangle)]
pub extern "C" fn dk_deferred_step(handle: *mut c_void, commands: u32) -> i32 {
    editor(handle).is_none_or(|editor| editor.advance_deferred(commands.max(1) as usize)) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_flush_deferred(handle: *mut c_void) {
    if let Some(editor) = editor(handle) {
        editor.flush_deferred();
    }
}

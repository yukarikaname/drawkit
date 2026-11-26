//! Extended range on the C surface: the dynamic range flag, the linear composite and EXR export.
//!
//! The 8-bit composite stays where it is. An HDR shell asks for the same rectangle in linear light
//! instead, so the extended-range screen path and the EXR file are the only consumers of floats.

use std::os::raw::{c_char, c_void};

use crate::ffi::editor;
use crate::surface::Rect;

#[unsafe(no_mangle)]
pub extern "C" fn dk_dynamic_range(handle: *mut c_void) -> i32 {
    editor(handle).map_or(0, |editor| editor.document.dynamic_range.as_i32())
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_dynamic_range(handle: *mut c_void, range: i32) {
    if let Some(editor) = editor(handle) {
        editor.document.dynamic_range = crate::document::DynamicRange::from_i32(range);
    }
}

/// The whole canvas as premultiplied linear RGBA in extended linear Display P3.
///
/// # Safety
/// `out` must point at `width * height * 4` `f32` values for this editor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_composite_linear(handle: *mut c_void, out: *mut f32) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let (width, height) = (editor.document.width(), editor.document.height());
    let pixels = editor
        .document
        .composite_linear_rect(Rect::new(0, 0, width, height));
    write_linear(&pixels, out);
    1
}

/// # Safety
/// `out` must point at `width * height * 4` `f32` values for this editor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_composite_linear_rect(
    handle: *mut c_void,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    out: *mut f32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let rect = Rect::new(x as usize, y as usize, width as usize, height as usize);
    let pixels = editor.document.composite_linear_rect(rect);
    write_linear(&pixels, out);
    1
}

fn write_linear(pixels: &[[f32; 4]], out: *mut f32) {
    // SAFETY: the caller allocated four floats per pixel, which is what `pixels` holds.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, pixels.len() * 4) };
    for (index, pixel) in pixels.iter().enumerate() {
        buffer[index * 4..index * 4 + 4].copy_from_slice(pixel);
    }
}

/// Write the visible canvas as a JPEG. `quality` is clamped to 1..=100.
///
/// # Safety
/// `path` must be a NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_export_jpeg(
    handle: *mut c_void,
    path: *const c_char,
    quality: i32,
) -> i32 {
    unsafe {
        export_raster(
            handle,
            path,
            drawkit_formats::RasterFormat::Jpeg,
            quality.clamp(1, 100) as u8,
        )
    }
}

/// Write the visible canvas as a lossless WebP.
///
/// # Safety
/// `path` must be a NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_export_webp(handle: *mut c_void, path: *const c_char) -> i32 {
    unsafe { export_raster(handle, path, drawkit_formats::RasterFormat::WebP, 100) }
}

unsafe fn export_raster(
    handle: *mut c_void,
    path: *const c_char,
    format: drawkit_formats::RasterFormat,
    quality: u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if path.is_null() {
        return 0;
    }
    // SAFETY: the shell passed a NUL-terminated path.
    let path = unsafe { std::ffi::CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else {
        return 0;
    };
    i32::from(editor.export_raster(std::path::Path::new(path), format, quality))
}

/// Write every paint and fill layer as a canvas-sized PNG in `directory`. Returns the count.
///
/// # Safety
/// `directory` must be a NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_export_layers(handle: *mut c_void, directory: *const c_char) -> u32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if directory.is_null() {
        return 0;
    }
    // SAFETY: the shell passed a NUL-terminated path.
    let directory = unsafe { std::ffi::CStr::from_ptr(directory) };
    let Ok(directory) = directory.to_str() else {
        return 0;
    };
    editor.export_layers(std::path::Path::new(directory)) as u32
}

/// Write a premultiplied linear Display P3 frame as an EXR, for pixels the GPU composited.
///
/// # Safety
/// `path` must be NUL-terminated, and `pixels` must hold `width * height * 4` floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_write_exr_linear(
    path: *const c_char,
    width: u32,
    height: u32,
    pixels: *const f32,
    length: u32,
) -> i32 {
    if path.is_null() || pixels.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let expected = width as usize * height as usize * 4;
    if (length as usize) < expected {
        return 0;
    }
    // SAFETY: the shell passed a NUL-terminated path and the promised number of floats.
    let path = unsafe { std::ffi::CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else {
        return 0;
    };
    let values = unsafe { std::slice::from_raw_parts(pixels, expected) };
    i32::from(
        drawkit_formats::exr::write_f32(path, width as usize, height as usize, values).is_ok(),
    )
}

/// Write the visible canvas as a linear OpenEXR file, which keeps the highlights PNG would clip.
///
/// # Safety
/// `path` must be a NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_export_exr(handle: *mut c_void, path: *const c_char) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if path.is_null() {
        return 0;
    }
    // SAFETY: the shell passed a NUL-terminated path.
    let path = unsafe { std::ffi::CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else {
        return 0;
    };
    let document = &editor.document;
    let (width, height) = (document.width(), document.height());
    let pixels = document.composite_linear_rect_export(Rect::new(0, 0, width, height));
    i32::from(drawkit_formats::exr::write(path, width, height, &pixels).is_ok())
}

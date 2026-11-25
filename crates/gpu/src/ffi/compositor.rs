//! C surface for the headless compositor used by bitmap shells.
//!
//! The Windows and Linux shells already own a bitmap; these calls keep the layers resident on the
//! GPU, composite there, and hand back one premultiplied RGBA image. No window handle is needed,
//! so the same API works with WinUI's WriteableBitmap and a GTK texture.

use std::os::raw::c_void;

use crate::compositor::GpuCompositor;

fn compositor<'a>(handle: *mut c_void) -> Option<&'a mut GpuCompositor> {
    if handle.is_null() {
        return None;
    }
    // SAFETY: the pointer came from `dk_gpu_compositor_new` and is freed once.
    Some(unsafe { &mut *(handle as *mut GpuCompositor) })
}

/// Create a compositor, or null when the machine has no usable adapter.
#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_new(width: u32, height: u32) -> *mut c_void {
    let created = std::panic::catch_unwind(|| GpuCompositor::new(width, height));
    match created {
        Ok(Some(compositor)) => Box::into_raw(Box::new(compositor)) as *mut c_void,
        Ok(None) => std::ptr::null_mut(),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown panic");
            eprintln!("gpu compositor: {message}");
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_free(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    // SAFETY: owned by this call from here on, and never handed out again.
    unsafe { drop(Box::from_raw(handle as *mut GpuCompositor)) };
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_reset(
    handle: *mut c_void,
    count: u32,
    width: u32,
    height: u32,
) {
    if let Some(compositor) = compositor(handle) {
        compositor.reset(count as usize, width, height);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_set_budget(handle: *mut c_void, bytes: u64) {
    if let Some(compositor) = compositor(handle) {
        compositor.set_budget(bytes as usize);
    }
}

/// Upload one rectangle of a layer as straight RGBA8.
///
/// # Safety
/// `pixels` must hold `width * height * 4` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_gpu_compositor_upload_layer(
    handle: *mut c_void,
    index: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    pixels: *const u8,
) -> i32 {
    let Some(compositor) = compositor(handle) else {
        return 0;
    };
    if pixels.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let size = width as usize * height as usize * 4;
    // SAFETY: the shell allocated `width * height * 4` bytes.
    let slice = unsafe { std::slice::from_raw_parts(pixels, size) };
    i32::from(compositor.upload_layer(crate::layers::RectUpload {
        index: index as usize,
        x,
        y,
        width,
        height,
        pixels: slice,
    }))
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_gpu_compositor_set_layer(
    handle: *mut c_void,
    index: u32,
    opacity: f32,
    blend: i32,
    visible: i32,
    fill: i32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
) {
    let Some(compositor) = compositor(handle) else {
        return;
    };
    let mut spec = crate::LayerSpec::new(index as usize);
    spec.opacity = opacity.clamp(0.0, 1.0);
    spec.blend = crate::Blend::from_i32(blend);
    spec.visible = visible != 0;
    if fill != 0 {
        spec.fill = Some([red, green, blue, alpha]);
    }
    compositor.set_layer(index as usize, spec);
}

/// Set the layer whose alpha clips `index`, or -1 for no clip.
#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_set_layer_clip(handle: *mut c_void, index: u32, clip: i32) {
    if let Some(compositor) = compositor(handle) {
        compositor.set_layer_clip(index as usize, (clip >= 0).then_some(clip as usize));
    }
}

/// Composite into `out` as premultiplied RGBA8.
///
/// # Safety
/// `out` must hold `width * height * 4` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_gpu_compositor_composite(
    handle: *mut c_void,
    out: *mut u8,
    width: u32,
    height: u32,
) -> i32 {
    let Some(compositor) = compositor(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 || compositor.size() != (width, height) {
        return 0;
    }
    let size = width as usize * height as usize * 4;
    let pixels = compositor.composite();
    if pixels.len() < size {
        return 0;
    }
    // SAFETY: the shell allocated `width * height * 4` bytes.
    let target = unsafe { std::slice::from_raw_parts_mut(out, size) };
    target.copy_from_slice(&pixels[..size]);
    1
}

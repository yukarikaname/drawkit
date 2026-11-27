//! The Android swapchain: range, layer uploads, present mode and the per-frame split.

use super::*;

/// Whether the created swapchain took the extended-range path. A driver that cannot present
/// half floats degrades to eight-bit, and the shell then keeps the document in that range.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasHdr(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    surface(handle).map_or(0, |canvas| canvas.hdr() as jint)
}

/// Upload a layer rectangle as premultiplied linear Display P3 floats, for an extended-range
/// resident stack.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasUploadLayerLinear(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    x: jint,
    y: jint,
    width: jint,
    height: jint,
    pixels: JFloatArray,
) -> jint {
    let Some(canvas) = surface(handle) else {
        return 0;
    };
    let Ok(length) = env.get_array_length(&pixels) else {
        return 0;
    };
    let (width, height) = (width.max(0) as usize, height.max(0) as usize);
    if width == 0 || height == 0 || (length as usize) < width * height * 4 {
        return 0;
    }
    let mut values = vec![0.0_f32; length as usize];
    if env.get_float_array_region(&pixels, 0, &mut values).is_err() {
        return 0;
    }
    canvas.upload_layer_linear(
        index.max(0) as usize,
        x.max(0) as u32,
        y.max(0) as u32,
        width as u32,
        height as u32,
        &values,
    ) as jint
}

/// Pick the swapchain's present mode: 0 fifo, 1 immediate, 2 mailbox. The bench uses this to
/// separate display pacing from frame cost.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasSetPresentMode(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    mode: jint,
) -> jint {
    surface(handle).is_some_and(|canvas| canvas.set_present_mode(mode.max(0) as u32)) as jint
}

/// The present mode in use, so a bench line can say which one it measured.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasPresentMode(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    surface(handle).map_or(0, |canvas| canvas.present_mode() as jint)
}

/// The last present's split in milliseconds: composite, copy, draw, acquire, encode.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasPresentStats(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    out: JFloatArray,
) {
    if out.is_null() {
        return;
    }
    let mut values = [0.0f32; 8];
    // SAFETY: the surface keeps the stats; `values` is eight floats, as the entry expects.
    unsafe {
        dk_canvas_surface_present_stats(handle as *mut std::ffi::c_void, values.as_mut_ptr())
    };
    let _ = env.set_float_array_region(out, 0, &values);
}

unsafe extern "C" {
    fn dk_canvas_surface_present_stats(handle: *mut std::ffi::c_void, out: *mut f32);
}

//! Stroke, commit, undo and materialise entry points.

use super::*;

/// One layer's rectangle as straight RGBA8, for the first upload to the resident stack.
///
/// # Safety
/// `out` must hold `width * height * 4` bytes.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerRect(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    x: jint,
    y: jint,
    width: jint,
    height: jint,
    out: JByteArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let rect = Rect::new(
        x.max(0) as usize,
        y.max(0) as usize,
        width.max(0) as usize,
        height.max(0) as usize,
    );
    let Some(layer) = editor.document.layers.get(index.max(0) as usize) else {
        return 0;
    };
    let pixels = layer.surface.snapshot(rect);
    let mut bytes = Vec::with_capacity(rect.width * rect.height * 4);
    for pixel in pixels {
        for channel in pixel {
            bytes.push((channel.clamp(0.0, 1.0) * 255.0).round() as i8 as u8 as i8);
        }
    }
    env.set_byte_array_region(&out, 0, &bytes).is_ok() as jint
}

/// A fill layer's colour, if it has one.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerFill(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    out: JFloatArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some(layer) = editor.document.layers.get(index.max(0) as usize) else {
        return 0;
    };
    if layer.kind != drawkit_core::document::LayerKind::Fill {
        return 0;
    }
    env.set_float_array_region(&out, 0, &layer.fill).is_ok() as jint
}

/// One editable brush parameter.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasBrushParam(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    id: jint,
) -> jfloat {
    editor(handle)
        .and_then(|editor| editor.brush.param(id.max(0) as u8))
        .unwrap_or(0.0)
}

/// The imported map's size, as width and height.
///
/// # Safety
/// `out` is a `IntArray` with room for two values.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasBrushTextureSize(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    out: JIntArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some(texture) = editor.brush_texture.as_ref() else {
        return 0;
    };
    let size = [texture.width as jint, texture.height as jint];
    env.set_int_array_region(&out, 0, &size).is_ok() as jint
}

/// One grayscale byte per texel of the imported map.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasBrushTexture(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    out: JByteArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some(texture) = editor.brush_texture.as_ref() else {
        return 0;
    };
    let data: Vec<i8> = texture.data.iter().map(|byte| *byte as i8).collect();
    env.set_byte_array_region(&out, 0, &data).is_ok() as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasBrushKindParam(
    _env: JNIEnv,
    _this: JObject,
    kind: jint,
    id: jint,
) -> jfloat {
    drawkit_core::brush::Brush::for_kind(drawkit_core::brush::BrushKind::from_i32(kind))
        .param(id.max(0) as u8)
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasSetDeferRaster(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    enabled: jboolean,
) {
    if let Some(editor) = editor(handle) {
        editor.defer_raster = enabled != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasFlushDeferred(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    if let Some(editor) = editor(handle) {
        editor.flush_deferred();
    }
}

/// The random seed of the stroke in progress, as two halves. The surface needs the same seed the
/// core recorded, or grain and jitter would differ from a replay.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasStrokeSeed(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    out: JIntArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let seed = editor.stroke_seed().unwrap_or(0);
    let halves = [seed as u32 as jint, (seed >> 32) as u32 as jint];
    env.set_int_array_region(&out, 0, &halves).is_ok() as jint
}

/// The mirror variants of one segment, `x0, y0, x1, y1` each. The surface draws them in one pass.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasMirrorSegments(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    from_x: jfloat,
    from_y: jfloat,
    to_x: jfloat,
    to_y: jfloat,
    out: JFloatArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let (points, count) = editor.mirror.variants(
        editor.document.width(),
        editor.document.height(),
        (from_x, from_y),
        (to_x, to_y),
    );
    let mut values = [0.0_f32; 16];
    for (index, ((ax, ay), (bx, by))) in points.iter().take(count).enumerate() {
        values[index * 4..index * 4 + 4].copy_from_slice(&[*ax, *ay, *bx, *by]);
    }
    env.set_float_array_region(&out, 0, &values).is_ok() as jint * count as jint
}

/// The visible colour at one canvas pixel, over paper, as four floats.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasPick(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jint,
    y: jint,
    out: JFloatArray,
) -> jint {
    let mut values = [0.0_f32; 4];
    let picked = unsafe {
        dk_canvas_surface_pick(
            handle as *mut std::ffi::c_void,
            x.max(0) as u32,
            y.max(0) as u32,
            values.as_mut_ptr(),
        )
    };
    if picked == 0 {
        return 0;
    }
    if env.set_float_array_region(&out, 0, &values).is_err() {
        return 0;
    }
    1
}

unsafe extern "C" {
    fn dk_canvas_surface_pick(handle: *mut std::ffi::c_void, x: u32, y: u32, out: *mut f32) -> i32;
}

/// The ideal line, oval or rectangle the last stroke snaps to, as `x, y` pairs. Zero means the
/// gesture stays freehand. The shell replays the path through the GPU stroke.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_shapePath(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    snap: jboolean,
    out: JFloatArray,
) -> jint {
    let Ok(length) = env.get_array_length(&out) else {
        return 0;
    };
    let mut values = vec![0.0_f32; length as usize];
    let copied = unsafe {
        dk_shape_path(
            handle as *mut std::ffi::c_void,
            snap as i32,
            values.as_mut_ptr(),
            length as u32,
        )
    };
    if copied <= 0 {
        return 0;
    }
    let used = copied as usize * 2;
    let _ = env.set_float_array_region(&out, 0, &values[..used]);
    copied
}

unsafe extern "C" {
    fn dk_shape_path(handle: *mut std::ffi::c_void, snap: i32, out: *mut f32, capacity: u32)
    -> i32;
}

/// Install the imported paper/tip map the live stroke runs with.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasSetBrushTexture(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    width: jint,
    height: jint,
    data: JByteArray,
) -> jint {
    if width <= 0 || height <= 0 {
        return 0;
    }
    let Ok(bytes) = env.convert_byte_array(&data) else {
        return 0;
    };
    let width = width as usize;
    let height = height as usize;
    if bytes.len() < width * height {
        return 0;
    }
    unsafe {
        dk_canvas_surface_set_brush_texture(
            handle as *mut std::ffi::c_void,
            width as u32,
            height as u32,
            bytes.as_ptr(),
            bytes.len() as u32,
        ) as jint
    }
}

/// Drop the imported map and paint with the procedural tip again.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasClearBrushTexture(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    unsafe { dk_canvas_surface_clear_brush_texture(handle as *mut std::ffi::c_void) }
}

unsafe extern "C" {
    fn dk_canvas_surface_set_brush_texture(
        handle: *mut std::ffi::c_void,
        width: u32,
        height: u32,
        data: *const u8,
        len: u32,
    ) -> i32;
    fn dk_canvas_surface_clear_brush_texture(handle: *mut std::ffi::c_void);
}

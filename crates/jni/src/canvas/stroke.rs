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

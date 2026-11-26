use super::*;

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setAutoShape(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    enabled: jboolean,
) {
    if let Some(editor) = editor(handle) {
        editor.auto_shape = enabled != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_autoShape(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jboolean {
    editor(handle).is_some_and(|editor| editor.auto_shape(false)) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setSelection(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jint,
    y: jint,
    width: jint,
    height: jint,
) {
    if let Some(editor) = editor(handle) {
        editor.select_rect(drawkit_core::surface::Rect::new(
            x.max(0) as usize,
            y.max(0) as usize,
            width.max(0) as usize,
            height.max(0) as usize,
        ));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_clearSelection(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    if let Some(editor) = editor(handle) {
        editor.clear_selection();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_extractSelection(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jboolean {
    editor(handle).is_some_and(|editor| editor.extract_selection().is_some()) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setBrush(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    kind: jint,
) {
    if let Some(editor) = editor(handle) {
        editor.set_brush(drawkit_core::brush::BrushKind::from_i32(kind));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setQuickShape(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    enabled: jboolean,
) {
    if let Some(editor) = editor(handle) {
        editor.quick_shape = enabled != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_fillBegin(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jint,
    y: jint,
    tolerance: jfloat,
    close: jint,
    expand: jint,
    feather: jint,
    red: jfloat,
    green: jfloat,
    blue: jfloat,
    alpha: jfloat,
    time_ms: jlong,
) -> jboolean {
    editor(handle).is_some_and(|editor| {
        editor
            .begin_fill_animation(
                x.max(0) as usize,
                y.max(0) as usize,
                tolerance,
                [red, green, blue, alpha],
                drawkit_core::fill::FillOptions {
                    close: close.max(0) as usize,
                    expand: expand.max(0) as usize,
                    feather: feather.max(0) as usize,
                    smart: false,
                },
                time_ms.max(0) as u64,
            )
            .is_some()
    }) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_fillStep(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    progress: jfloat,
) -> jboolean {
    editor(handle).is_some_and(|editor| editor.fill_animation_step(progress).is_some()) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_fillFinish(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jboolean {
    editor(handle).is_some_and(|editor| editor.finish_fill_animation()) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_fillCancel(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jboolean {
    editor(handle).is_some_and(|editor| editor.cancel_fill_animation()) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_extractLineArt(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    threshold: jfloat,
    softness: jfloat,
) -> jboolean {
    editor(handle).is_some_and(|editor| {
        editor
            .extract_line_art(drawkit_lineart::LineArtOptions {
                threshold,
                softness,
            })
            .is_some()
    }) as jboolean
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_fillEx(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jint,
    y: jint,
    tolerance: jfloat,
    close: jint,
    expand: jint,
    feather: jint,
    red: jfloat,
    green: jfloat,
    blue: jfloat,
    alpha: jfloat,
    time_ms: jlong,
) {
    if let Some(editor) = editor(handle) {
        editor.fill_with(
            x.max(0) as usize,
            y.max(0) as usize,
            tolerance,
            [red, green, blue, alpha],
            drawkit_core::fill::FillOptions {
                close: close.max(0) as usize,
                expand: expand.max(0) as usize,
                feather: feather.max(0) as usize,
                smart: false,
            },
            time_ms as u64,
        );
    }
}

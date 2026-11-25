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
    editor(handle).is_some_and(|editor| editor.auto_shape()) as jboolean
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

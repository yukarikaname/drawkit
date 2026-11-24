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

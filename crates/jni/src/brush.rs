//! Brush files on Android: the same MyPaint `.myb` bridge as the desktop shells.

use jni::JNIEnv;
use jni::objects::{JObject, JString};
use jni::sys::{jboolean, jint, jlong};

use crate::editor;

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_exportBrush(
    mut env: JNIEnv,
    _this: JObject,
    handle: jlong,
    path: JString,
) -> jboolean {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Ok(path) = env.get_string(&path) else {
        return 0;
    };
    let Ok(path) = path.to_str() else {
        return 0;
    };
    let preset =
        drawkit_core::brush::BrushPreset::from_current(editor.brush.kind.name(), &editor.brush);
    u8::from(preset.save(std::path::Path::new(path)).is_ok())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_importBrush(
    mut env: JNIEnv,
    _this: JObject,
    handle: jlong,
    path: JString,
) -> jint {
    let Some(editor) = editor(handle) else {
        return -1;
    };
    let Ok(path) = env.get_string(&path) else {
        return -1;
    };
    let Ok(path) = path.to_str() else {
        return -1;
    };
    let Ok(preset) = drawkit_core::brush::BrushPreset::load(std::path::Path::new(path)) else {
        return -1;
    };
    editor.brush = preset.brush;
    editor.brush.kind.as_i32()
}

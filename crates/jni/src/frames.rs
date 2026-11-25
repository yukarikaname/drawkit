//! Animation frames on Android: the frame list, thumbnails and onion compositing.

use jni::JNIEnv;
use jni::objects::{JIntArray, JObject};
use jni::sys::{jboolean, jint, jlong};

use crate::{argb, editor};

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_frameCount(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(1, |editor| editor.frame_count() as jint)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_frameActive(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(0, |editor| editor.active_frame() as jint)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_frameAdd(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    duplicate: jboolean,
) -> jint {
    editor(handle).map_or(-1, |editor| editor.add_frame(duplicate != 0) as jint)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_frameSelect(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) -> jboolean {
    u8::from(editor(handle).is_some_and(|editor| editor.select_frame(index.max(0) as usize)))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_frameRemove(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) -> jboolean {
    u8::from(editor(handle).is_some_and(|editor| editor.remove_frame(index.max(0) as usize)))
}

fn write_pixels(env: &JNIEnv, out: JIntArray, pixels: Vec<drawkit_core::color::Rgba>) -> jint {
    let colors: Vec<jint> = pixels.into_iter().map(argb).collect();
    env.set_int_array_region(&out, 0, &colors).is_ok() as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_frameComposite(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    out: JIntArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some((pixels, _, _)) = editor.frame_rgba(index.max(0) as usize) else {
        return 0;
    };
    write_pixels(&env, out, pixels)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_onionComposite(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    before: jint,
    after: jint,
    out: JIntArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let (width, height) = (editor.document.width(), editor.document.height());
    let mut pixels = vec![0_u8; width * height * 4];
    if !editor.onion_image(before.max(0) as usize, after.max(0) as usize, &mut pixels) {
        return 0;
    }
    let colors: Vec<jint> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| {
            crate::argb_premultiplied([
                pixel[0] as f32 / 255.0,
                pixel[1] as f32 / 255.0,
                pixel[2] as f32 / 255.0,
                pixel[3] as f32 / 255.0,
            ])
        })
        .collect();
    env.set_int_array_region(&out, 0, &colors).is_ok() as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_fps(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(12, |editor| editor.fps as jint)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setFps(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    fps: jint,
) {
    if let Some(editor) = editor(handle) {
        editor.fps = fps.clamp(1, 60) as u16;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_frameDuration(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(83, |editor| editor.frame_duration as jint)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setFrameDuration(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    duration: jint,
) {
    if let Some(editor) = editor(handle) {
        editor.frame_duration = duration.clamp(10, 10_000) as u32;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_loopEnabled(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jboolean {
    u8::from(editor(handle).is_some_and(|editor| editor.loop_enabled))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setLoopEnabled(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    enabled: jboolean,
) {
    if let Some(editor) = editor(handle) {
        editor.loop_enabled = enabled != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_loopStart(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(0, |editor| editor.loop_range().0 as jint)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setLoopStart(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) {
    if let Some(editor) = editor(handle) {
        let (_, end) = editor.loop_range();
        editor.loop_start = (index.max(0) as usize).min(end.saturating_sub(1));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_loopEnd(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(1, |editor| editor.loop_range().1 as jint)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setLoopEnd(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    end: jint,
) {
    if let Some(editor) = editor(handle) {
        let (start, _) = editor.loop_range();
        editor.loop_end = (end.max(0) as usize).max(start + 1);
    }
}

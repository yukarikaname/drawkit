//! The JNI bridge between the Android shell and the painting core.
//!
//! The shell owns no pixels: it asks for a rectangle of the composite and copies it into one
//! `Bitmap`. Colours cross the boundary as premultiplied ARGB ints, which is what Android stores
//! for `ARGB_8888`, and so no second conversion is needed in Kotlin.
//!
//! Every function takes a `Long` handle from `newEditor`. The UI thread is the only caller, and
//! `free` is called when the drawing view goes away; a null handle is a no-op rather than a crash.

use drawkit_core::color::Rgba;
use drawkit_core::surface::Rect;
use drawkit_core::tools::Editor;
use jni::JNIEnv;
use jni::objects::{JFloatArray, JIntArray, JObject, JString};
use jni::sys::{jboolean, jfloat, jint, jlong};

fn editor(handle: jlong) -> Option<&'static mut Editor> {
    if handle == 0 {
        return None;
    }
    // SAFETY: the handle came from `Box::into_raw` in `newEditor` and is freed once in `free`.
    Some(unsafe { &mut *(handle as *mut Editor) })
}

/// Straight RGBA to premultiplied ARGB, the layout `ARGB_8888` keeps.
pub(crate) fn argb(pixel: Rgba) -> jint {
    let alpha = pixel[3].clamp(0.0, 1.0);
    let red = (pixel[0].clamp(0.0, 1.0) * alpha * 255.0).round() as u32;
    let green = (pixel[1].clamp(0.0, 1.0) * alpha * 255.0).round() as u32;
    let blue = (pixel[2].clamp(0.0, 1.0) * alpha * 255.0).round() as u32;
    let a = (alpha * 255.0).round() as u32;
    ((a << 24) | (red << 16) | (green << 8) | blue) as jint
}

/// The thumbnail is already premultiplied; multiplying by alpha again would darken its edges.
pub(crate) fn argb_premultiplied(pixel: Rgba) -> jint {
    let red = (pixel[0].clamp(0.0, 1.0) * 255.0).round() as u32;
    let green = (pixel[1].clamp(0.0, 1.0) * 255.0).round() as u32;
    let blue = (pixel[2].clamp(0.0, 1.0) * 255.0).round() as u32;
    let a = (pixel[3].clamp(0.0, 1.0) * 255.0).round() as u32;
    ((a << 24) | (red << 16) | (green << 8) | blue) as jint
}

fn rect(
    handle: jlong,
    x: jint,
    y: jint,
    width: jint,
    height: jint,
) -> Option<(Rect, &'static Editor)> {
    let editor = editor(handle)?;
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((
        Rect::new(x as usize, y as usize, width as usize, height as usize),
        editor,
    ))
}

mod brush;
mod frames;
mod hdr;
mod layer;

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_newEditor(
    _env: JNIEnv,
    _this: JObject,
    width: jint,
    height: jint,
) -> jlong {
    let width = width.max(1) as usize;
    let height = height.max(1) as usize;
    let editor = Editor::new(width, height, 256 * 1024 * 1024);
    Box::into_raw(Box::new(editor)) as jlong
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_free(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    if handle != 0 {
        // SAFETY: the handle is the one and only allocation from `newEditor`.
        drop(unsafe { Box::from_raw(handle as *mut Editor) });
    }
}

/// Copy `width * height` premultiplied ARGB pixels into `out`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_compositeRect(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jint,
    y: jint,
    width: jint,
    height: jint,
    out: JIntArray,
) -> jint {
    let Some((rect, editor)) = rect(handle, x, y, width, height) else {
        return 0;
    };
    let pixels = editor.document.composite_rect(rect);
    let colors: Vec<jint> = pixels.into_iter().map(argb).collect();
    env.set_int_array_region(&out, 0, &colors).is_ok() as jint
}

/// Read and clear the rectangle the last operation dirtied: `x, y, width, height`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_takeDirty(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    out: JIntArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some(rect) = editor.document.take_dirty() else {
        return 0;
    };
    let values = [
        rect.x as jint,
        rect.y as jint,
        rect.width as jint,
        rect.height as jint,
    ];
    env.set_int_array_region(&out, 0, &values).is_ok() as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_strokeBegin(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    time_ms: jlong,
) {
    if let Some(editor) = editor(handle) {
        editor.begin_stroke(time_ms as u64);
    }
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_stroke(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    from_x: jfloat,
    from_y: jfloat,
    to_x: jfloat,
    to_y: jfloat,
    radius: jfloat,
    pressure: jfloat,
    red: jfloat,
    green: jfloat,
    blue: jfloat,
    alpha: jfloat,
    time_ms: jlong,
) {
    if let Some(editor) = editor(handle) {
        editor.stroke_at(
            (from_x, from_y),
            (to_x, to_y),
            radius,
            pressure,
            [red, green, blue, alpha],
            time_ms as u64,
        );
    }
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_erase(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    from_x: jfloat,
    from_y: jfloat,
    to_x: jfloat,
    to_y: jfloat,
    radius: jfloat,
    pressure: jfloat,
    time_ms: jlong,
) {
    if let Some(editor) = editor(handle) {
        editor.erase_at(
            (from_x, from_y),
            (to_x, to_y),
            radius,
            pressure,
            time_ms as u64,
        );
    }
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_strokeEx(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    from_x: jfloat,
    from_y: jfloat,
    to_x: jfloat,
    to_y: jfloat,
    radius: jfloat,
    pressure: jfloat,
    tilt_x: jfloat,
    tilt_y: jfloat,
    azimuth: jfloat,
    roll: jfloat,
    red: jfloat,
    green: jfloat,
    blue: jfloat,
    alpha: jfloat,
    time_ms: jlong,
) {
    if let Some(editor) = editor(handle) {
        editor.stroke_sample(
            (from_x, from_y),
            (to_x, to_y),
            radius,
            drawkit_core::tools::Dynamics {
                pressure,
                tilt: [tilt_x, tilt_y],
                azimuth,
                roll,
            },
            [red, green, blue, alpha],
            time_ms as u64,
        );
    }
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_eraseEx(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    from_x: jfloat,
    from_y: jfloat,
    to_x: jfloat,
    to_y: jfloat,
    radius: jfloat,
    pressure: jfloat,
    tilt_x: jfloat,
    tilt_y: jfloat,
    azimuth: jfloat,
    roll: jfloat,
    time_ms: jlong,
) {
    if let Some(editor) = editor(handle) {
        editor.erase_sample(
            (from_x, from_y),
            (to_x, to_y),
            radius,
            drawkit_core::tools::Dynamics {
                pressure,
                tilt: [tilt_x, tilt_y],
                azimuth,
                roll,
            },
            time_ms as u64,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_strokeEnd(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    if let Some(editor) = editor(handle) {
        editor.end_stroke();
    }
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_fill(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jint,
    y: jint,
    tolerance: jfloat,
    red: jfloat,
    green: jfloat,
    blue: jfloat,
    alpha: jfloat,
    time_ms: jlong,
) {
    if let Some(editor) = editor(handle) {
        editor.fill_at(
            x.max(0) as usize,
            y.max(0) as usize,
            tolerance,
            [red, green, blue, alpha],
            time_ms as u64,
        );
    }
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
            },
            time_ms as u64,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setStabilization(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    strength: jfloat,
) {
    if let Some(editor) = editor(handle) {
        editor.set_stabilization(strength);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_undo(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).and_then(|editor| editor.undo()).is_some() as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_redo(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).and_then(|editor| editor.redo()).is_some() as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canUndo(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jboolean {
    editor(handle).is_some_and(|editor| editor.history.can_undo()) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canRedo(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jboolean {
    editor(handle).is_some_and(|editor| editor.history.can_redo()) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_clear(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    red: jfloat,
    green: jfloat,
    blue: jfloat,
    alpha: jfloat,
) {
    if let Some(editor) = editor(handle) {
        editor.clear([red, green, blue, alpha]);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_isClear(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jboolean {
    editor(handle).is_some_and(|editor| editor.document.is_clear()) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_pick(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jint,
    y: jint,
    out: JFloatArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some(color) = editor.pick(x.max(0) as usize, y.max(0) as usize) else {
        return 0;
    };
    env.set_float_array_region(&out, 0, &color).is_ok() as jint
}

/// Write up to `max` dominant painted colours as straight RGBA floats.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_palette(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    max: jint,
    out: JFloatArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if max <= 0 || max > 32 {
        return 0;
    }
    let (width, height) = (editor.document.width(), editor.document.height());
    let composite: Vec<Rgba> = editor
        .document
        .composite_rect(Rect::new(0, 0, width, height))
        .into_iter()
        .map(|pixel| drawkit_core::color::over(pixel, drawkit_core::tools::PAPER))
        .collect();
    let colors = drawkit_core::picker::palette(
        &composite,
        width,
        height,
        drawkit_core::tools::PAPER,
        max as usize,
    );
    let values: Vec<jfloat> = colors.iter().flatten().copied().collect();
    env.set_float_array_region(&out, 0, &values).is_ok() as jint
}

/// One translated label. The table lives in the core (`shared/strings.json`), so Kotlin does not
/// carry a copy.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_text(
    mut env: JNIEnv,
    _this: JObject,
    key: JString,
    language: JString,
) -> jni::sys::jstring {
    let key: String = env.get_string(&key).map(Into::into).unwrap_or_default();
    let language: String = env
        .get_string(&language)
        .map(Into::into)
        .unwrap_or_default();
    let text = drawkit_core::l10n::text(&key, &language);
    env.new_string(text)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setMirror(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    mode: jint,
) {
    if let Some(editor) = editor(handle) {
        editor.set_mirror(drawkit_core::tools::Mirror::from_i32(mode));
    }
}

mod assist;

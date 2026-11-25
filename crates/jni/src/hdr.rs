//! Extended range on Android: the flag, the half-float linear buffer and EXR export.

use drawkit_core::surface::Rect;
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JObject, JString};
use jni::sys::{jboolean, jint, jlong};

use crate::{editor, rect};

/// Set the document's extended range. The shell asks for `compositeLinearHalfRect` after this.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_dynamicRange(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(0, |editor| editor.document.dynamic_range.as_i32())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_setDynamicRange(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    range: jint,
) {
    if let Some(editor) = editor(handle) {
        editor.document.dynamic_range = drawkit_core::document::DynamicRange::from_i32(range);
    }
}

/// The rectangle as premultiplied linear extended sRGB, half floats for `Bitmap.Config.RGBA_F16`.
///
/// The core composites in linear Display P3; the drawable on Android is extended linear sRGB, so
/// this is the one place the primaries change. Values above one stay above one.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_compositeLinearHalfRect(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    x: jint,
    y: jint,
    width: jint,
    height: jint,
    out: JByteBuffer,
) -> jint {
    let Some((rect, editor)) = rect(handle, x, y, width, height) else {
        return 0;
    };
    let Ok(capacity) = env.get_direct_buffer_capacity(&out) else {
        return 0;
    };
    if capacity < rect.width * rect.height * 8 {
        return 0;
    }
    let Ok(address) = env.get_direct_buffer_address(&out) else {
        return 0;
    };
    let pixels = editor.document.composite_linear_rect(rect);
    let to_p3 = editor.document.color_space.to_display_p3();
    let mut bytes = vec![0_u8; rect.width * rect.height * 8];
    for (index, pixel) in pixels.iter().enumerate() {
        let p3 = [
            to_p3[0][0] * pixel[0] + to_p3[0][1] * pixel[1] + to_p3[0][2] * pixel[2],
            to_p3[1][0] * pixel[0] + to_p3[1][1] * pixel[1] + to_p3[1][2] * pixel[2],
            to_p3[2][0] * pixel[0] + to_p3[2][1] * pixel[1] + to_p3[2][2] * pixel[2],
        ];
        let srgb = [
            (1.224_940_1 * p3[0] - 0.224_940_4 * p3[1]).max(0.0),
            (-0.042_056_9 * p3[0] + 1.042_057_1 * p3[1]).max(0.0),
            (-0.019_637_6 * p3[0] - 0.078_636_1 * p3[1] + 1.098_273_5 * p3[2]).max(0.0),
            pixel[3].clamp(0.0, 1.0),
        ];
        for (channel, value) in srgb.iter().enumerate() {
            let half = half::f16::from_f32(*value).to_bits().to_le_bytes();
            let at = index * 8 + channel * 2;
            bytes[at..at + 2].copy_from_slice(&half);
        }
    }
    // SAFETY: the capacity check above covers the exact byte count written here.
    let target = unsafe { std::slice::from_raw_parts_mut(address, bytes.len()) };
    target.copy_from_slice(&bytes);
    1
}

/// Write the visible canvas as OpenEXR, keeping highlights above paper white.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_exportExr(
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
    let document = &editor.document;
    let (width, height) = (document.width(), document.height());
    let pixels = document.composite_linear_rect_export(Rect::new(0, 0, width, height));
    u8::from(drawkit_formats::exr::write(path, width, height, &pixels).is_ok())
}

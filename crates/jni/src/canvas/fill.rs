//! The resident fill for the Android shell.
//!
//! The flood needs the composed frame, and the frame lives in the resident tiles. The shell reads
//! it back, the core builds the mask from it, and the surface blends that mask into the layer in
//! one pass. The core records the fill afterwards, so undo and the tape see a single step.

use super::*;
use drawkit_core::fill::FillOptions;

/// Composite the resident layers into `out` as premultiplied RGBA8 at canvas size.
///
/// Returns 0 when the surface has no frame to read, which is the shell's signal to report that the
/// fill needs the GPU canvas rather than paint the CPU cache the tiles no longer match.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasExportGpu(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    width: jint,
    height: jint,
    out: JByteArray,
) -> jint {
    let Some(canvas) = surface(handle) else {
        return 0;
    };
    if width <= 0 || height <= 0 {
        return 0;
    }
    let Some(pixels) = canvas.export_frame(width as u32, height as u32) else {
        return 0;
    };
    let bytes: Vec<i8> = pixels.iter().map(|byte| *byte as i8).collect();
    env.set_byte_array_region(&out, 0, &bytes).is_ok() as jint
}

/// One layer pixel's alpha, for deciding whether a fill goes under the line at the tap.
///
/// A negative result means the surface cannot answer (no tile, no layer); the shell then treats the
/// tap as opaque and seeds the flood with a transparent pixel, as macOS does.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasLayerAlpha(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    x: jint,
    y: jint,
) -> jfloat {
    surface(handle)
        .and_then(|canvas| {
            canvas.layer_alpha_at(index.max(0) as usize, x.max(0) as u32, y.max(0) as u32)
        })
        .unwrap_or(-1.0)
}

/// Blend a canvas-sized mask into a resident layer. `dirty` receives the union of the touched
/// rectangles as `x, y, width, height`, which is what the core records for undo and the tape.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasApplyFill(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    layer: jint,
    mask: JFloatArray,
    width: jint,
    height: jint,
    red: jfloat,
    green: jfloat,
    blue: jfloat,
    alpha: jfloat,
    alpha_locked: jboolean,
    under: jboolean,
    dirty: JIntArray,
) -> jint {
    let Some(canvas) = surface(handle) else {
        return 0;
    };
    if width <= 0 || height <= 0 {
        return 0;
    }
    let (width, height) = (width as u32, height as u32);
    let Ok(length) = env.get_array_length(&mask) else {
        return 0;
    };
    if (length as usize) < width as usize * height as usize {
        return 0;
    }
    let mut values = vec![0.0_f32; length as usize];
    if env.get_float_array_region(&mask, 0, &mut values).is_err() {
        return 0;
    }
    let applied = canvas.apply_fill(
        layer.max(0) as usize,
        &values,
        width,
        height,
        [red, green, blue, alpha],
        alpha_locked != 0,
        under != 0,
    );
    if !applied {
        return 0;
    }
    if let Some((x, y, width, height)) = canvas.fill_dirty() {
        let rect = [x as jint, y as jint, width as jint, height as jint];
        let _ = env.set_int_array_region(&dirty, 0, &rect);
    }
    1
}

/// Build the fill mask from the composite the surface exported.
///
/// `rect` receives `[layer, x, y, width, height, under, alpha_locked]` and `colour` the four
/// straight floats the surface will blend. Nothing is written to the document here.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_fillGpuBeginFrom(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    composite: JByteArray,
    width: jint,
    height: jint,
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
    seed_red: jfloat,
    seed_green: jfloat,
    seed_blue: jfloat,
    seed_alpha: jfloat,
    under: jboolean,
    time_ms: jlong,
    mask: JFloatArray,
    rect: JIntArray,
    colour: JFloatArray,
) -> jboolean {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if width <= 0 || height <= 0 {
        return 0;
    }
    let (width, height) = (width as usize, height as usize);
    let Ok(composite) = env.convert_byte_array(&composite) else {
        return 0;
    };
    let Ok(length) = env.get_array_length(&mask) else {
        return 0;
    };
    if composite.len() < width * height * 4 || (length as usize) < width * height {
        return 0;
    }
    let options = FillOptions {
        close: close.max(0) as usize,
        expand: expand.max(0) as usize,
        feather: feather.max(0) as usize,
        smart: editor.fill_smart,
    };
    let begun = editor
        .begin_gpu_fill_from(
            x.max(0) as usize,
            y.max(0) as usize,
            tolerance,
            [red, green, blue, alpha],
            options,
            time_ms.max(0) as u64,
            &composite,
            under != 0,
            [seed_red, seed_green, seed_blue, seed_alpha],
        )
        .is_some();
    if !begun {
        return 0;
    }
    let count = editor.gpu_fill_mask().map_or(0, <[f32]>::len);
    if count == 0 || (length as usize) < count {
        editor.cancel_gpu_fill();
        return 0;
    }
    let Some((layer, bounds, color, under, alpha_locked)) = editor.gpu_fill_state() else {
        editor.cancel_gpu_fill();
        return 0;
    };
    let source = editor.gpu_fill_mask().expect("the plan was just checked");
    if env.set_float_array_region(&mask, 0, source).is_err() {
        editor.cancel_gpu_fill();
        return 0;
    }
    let values = [
        layer as jint,
        bounds.x as jint,
        bounds.y as jint,
        bounds.width as jint,
        bounds.height as jint,
        under as jint,
        alpha_locked as jint,
    ];
    if env.set_int_array_region(&rect, 0, &values).is_err() {
        editor.cancel_gpu_fill();
        return 0;
    }
    if env.set_float_array_region(&colour, 0, &color).is_err() {
        editor.cancel_gpu_fill();
        return 0;
    }
    jboolean::from(true)
}

/// Record the fill the surface wrote as one GPU undo step and one tape event.
///
/// `rects` holds `count` rectangles of four values each; the pixels stay in the tile journal.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_fillGpuFinish(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    rects: JIntArray,
    count: jint,
) -> jboolean {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if count <= 0 {
        editor.cancel_gpu_fill();
        return 0;
    }
    let Ok(length) = env.get_array_length(&rects) else {
        editor.cancel_gpu_fill();
        return 0;
    };
    if (length as usize) < count as usize * 4 {
        editor.cancel_gpu_fill();
        return 0;
    }
    let mut values = vec![0_i32; count as usize * 4];
    if env.get_int_array_region(&rects, 0, &mut values).is_err() {
        editor.cancel_gpu_fill();
        return 0;
    }
    let rects: Vec<Rect> = values
        .as_chunks::<4>()
        .0
        .iter()
        .map(|rect| {
            Rect::new(
                rect[0].max(0) as usize,
                rect[1].max(0) as usize,
                rect[2].max(0) as usize,
                rect[3].max(0) as usize,
            )
        })
        .collect();
    i32::from(editor.finish_gpu_fill(&rects)) as jboolean
}

/// Drop a prepared fill without recording it. No pixel moved, so nothing has to be put back.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_fillGpuCancel(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jboolean {
    editor(handle).is_some_and(|editor| editor.cancel_gpu_fill()) as jboolean
}

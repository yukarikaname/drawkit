//! The resident GPU surface for the Android shell.
//!
//! The window comes from a `SurfaceView`, so the layers and the stroke stay on the GPU. The Kotlin
//! side only hands over the surface once and then uploads layer pixels and touch samples.

use std::ffi::c_void;

use drawkit_core::surface::Rect;
use drawkit_core::tools::Editor;
use drawkit_gpu::{CanvasSurface, LayerSpec};

mod fill;
mod stroke;
use jni::JNIEnv;
use jni::objects::{JByteArray, JFloatArray, JIntArray, JObject};
use jni::sys::{jboolean, jfloat, jint, jlong};

#[link(name = "android")]
unsafe extern "C" {
    fn ANativeWindow_fromSurface(env: *mut c_void, surface: *mut c_void) -> *mut c_void;
    fn ANativeWindow_release(window: *mut c_void);
}

pub(super) fn editor(handle: jlong) -> Option<&'static mut Editor> {
    if handle == 0 {
        return None;
    }
    // SAFETY: the handle came from `newEditor` and is freed once.
    Some(unsafe { &mut *(handle as *mut Editor) })
}

pub(super) fn surface(handle: jlong) -> Option<&'static mut CanvasSurface> {
    if handle == 0 {
        return None;
    }
    // SAFETY: the handle came from `canvasCreate` and is freed once.
    Some(unsafe { &mut *(handle as *mut CanvasSurface) })
}

/// Create the swapchain on the surface `SurfaceView` handed over.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasCreate(
    env: JNIEnv,
    _this: JObject,
    surface: JObject,
    width: jint,
    height: jint,
    hdr: jboolean,
) -> jlong {
    if surface.is_null() || width <= 0 || height <= 0 {
        return 0;
    }
    let env_raw = env.get_native_interface() as *mut c_void;
    let surface_raw = surface.as_raw() as *mut c_void;
    let window = unsafe { ANativeWindow_fromSurface(env_raw, surface_raw) };
    if window.is_null() {
        return 0;
    }
    match unsafe { CanvasSurface::new_android(window, width as u32, height as u32, hdr != 0) } {
        Some(canvas) => Box::into_raw(Box::new(canvas)) as jlong,
        None => {
            unsafe { ANativeWindow_release(window) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasFree(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    if handle == 0 {
        return;
    }
    // SAFETY: owned by this call from here on.
    unsafe { drop(Box::from_raw(handle as *mut CanvasSurface)) };
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasResize(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    width: jint,
    height: jint,
) {
    if let Some(canvas) = surface(handle) {
        canvas.resize(width.max(1) as u32, height.max(1) as u32);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasResetLayers(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    count: jint,
    width: jint,
    height: jint,
) {
    if let Some(canvas) = surface(handle) {
        canvas.reset_layers(
            count.max(0) as usize,
            width.max(1) as u32,
            height.max(1) as u32,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasSetBudget(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    bytes: jlong,
) {
    if let Some(canvas) = surface(handle) {
        canvas.set_layer_budget(bytes.max(0) as u64);
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasSetLayer(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    opacity: jfloat,
    blend: jint,
    visible: jboolean,
    fill: jboolean,
    red: jfloat,
    green: jfloat,
    blue: jfloat,
    alpha: jfloat,
) {
    let Some(canvas) = surface(handle) else {
        return;
    };
    let mut spec = LayerSpec::new(index.max(0) as usize);
    spec.opacity = opacity;
    spec.visible = visible != 0;
    spec.blend = drawkit_gpu::Blend::from_i32(blend);
    spec.fill = (fill != 0).then_some([red, green, blue, alpha]);
    canvas.set_layer_spec(index.max(0) as usize, spec);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasSetLayerClip(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    clip: jint,
) {
    if let Some(canvas) = surface(handle) {
        canvas.set_layer_clip(index.max(0) as usize, (clip >= 0).then_some(clip as usize));
    }
}

/// Upload one rectangle of straight RGBA8 layer pixels.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasUploadLayer(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    x: jint,
    y: jint,
    width: jint,
    height: jint,
    pixels: JByteArray,
) -> jint {
    let Some(canvas) = surface(handle) else {
        return 0;
    };
    let Ok(bytes) = env.convert_byte_array(&pixels) else {
        return 0;
    };
    canvas.upload_layer_rect(
        index.max(0) as usize,
        x.max(0) as u32,
        y.max(0) as u32,
        width.max(0) as u32,
        height.max(0) as u32,
        &bytes,
    ) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasPresent(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    paper_x: jfloat,
    paper_y: jfloat,
    paper_width: jfloat,
    paper_height: jfloat,
) -> jint {
    let Some(canvas) = surface(handle) else {
        return 0;
    };
    canvas.present_layers([paper_x, paper_y, paper_width, paper_height])
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasStrokeBegin(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    width: jint,
    height: jint,
    red: jfloat,
    green: jfloat,
    blue: jfloat,
    alpha: jfloat,
    flow: jfloat,
    pressure_alpha: jfloat,
    paper: jfloat,
    tip: jfloat,
    dual: jfloat,
    dual_scale: jfloat,
    dual_hardness: jfloat,
    dual_roundness: jfloat,
    dual_angle: jfloat,
    dual_paper: jfloat,
    dual_tip: jfloat,
    wet: jfloat,
    jitter: jfloat,
    grain: jfloat,
    dual_jitter: jfloat,
    dual_grain: jfloat,
    seed_low: jint,
    seed_high: jint,
    erase: jint,
    preview_layer: jint,
) {
    if let Some(canvas) = surface(handle) {
        canvas.stroke_begin(
            width.max(1) as u32,
            height.max(1) as u32,
            [red, green, blue, alpha],
            flow,
            pressure_alpha,
            paper,
            tip,
            [
                dual,
                dual_scale,
                dual_hardness,
                dual_roundness,
                dual_angle,
                dual_paper,
                dual_tip,
            ],
            wet,
            jitter,
            grain,
            dual_jitter,
            dual_grain,
            (seed_low as u32 as u64) | ((seed_high as u32 as u64) << 32),
            erase != 0,
            preview_layer as u32,
        );
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasStrokeSegment(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    from_x: jfloat,
    from_y: jfloat,
    to_x: jfloat,
    to_y: jfloat,
    radius: jfloat,
    hardness: jfloat,
    spacing: jfloat,
    roundness: jfloat,
    angle: jfloat,
    pressure: jfloat,
) {
    if let Some(canvas) = surface(handle) {
        canvas.stroke_segment(
            (from_x, from_y),
            (to_x, to_y),
            radius,
            hardness,
            spacing,
            roundness,
            angle,
            pressure,
        );
    }
}

/// Queue stroke segments instead of submitting per sample; the shell flushes once a frame.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasSetStrokeBatch(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    on: jboolean,
) {
    if let Some(canvas) = surface(handle) {
        canvas.set_stroke_batch(on != 0);
    }
}

/// Submit the segments queued since the last frame.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasFlushStroke(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    if let Some(canvas) = surface(handle) {
        canvas.flush_stroke();
    }
}

/// Commit the stroke into the resident tiles. Returns the tile count, or -1 when the core refuses
/// the brush and the shell must replay the recording.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasCommitStroke(
    _env: JNIEnv,
    _this: JObject,
    editor_handle: jlong,
    canvas_handle: jlong,
    layer: jint,
    red: jfloat,
    green: jfloat,
    blue: jfloat,
    flow: jfloat,
) -> jint {
    let (Some(editor), Some(canvas)) = (editor(editor_handle), surface(canvas_handle)) else {
        return -1;
    };
    if !editor.begin_tile_commit() {
        return -1;
    }
    let count = canvas.stroke_commit(layer.max(0) as usize, [red, green, blue], flow);
    let mut rects = Vec::with_capacity(count);
    for index in 0..count {
        if let Some((x, y, width, height)) = canvas.committed_rect(index) {
            rects.push(Rect::new(
                x as usize,
                y as usize,
                width as usize,
                height as usize,
            ));
        }
    }
    editor.end_gpu_commit(&rects);
    count as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasStrokeEnd(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    if let Some(canvas) = surface(handle) {
        canvas.stroke_end();
    }
}

/// Rebuild the CPU cache for everything the GPU committed. Returns the number of rectangles.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasMaterialize(
    _env: JNIEnv,
    _this: JObject,
    editor_handle: jlong,
    canvas_handle: jlong,
) -> jint {
    let (Some(editor), Some(canvas)) = (editor(editor_handle), surface(canvas_handle)) else {
        return 0;
    };
    let total = canvas.materialize();
    let mut applied = 0;
    for index in 0..total {
        let Some(tile) = canvas.committed_tile(index) else {
            continue;
        };
        let rect = Rect::new(
            tile.x as usize,
            tile.y as usize,
            tile.width as usize,
            tile.height as usize,
        );
        if editor.apply_tiles(tile.layer, rect, &tile.pixels) {
            applied += 1;
        }
    }
    canvas.clear_committed();
    applied
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasUndoTiles(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    surface(handle).is_some_and(|canvas| canvas.undo_tiles()) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasRedoTiles(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    surface(handle).is_some_and(|canvas| canvas.redo_tiles()) as jint
}

/// The next undo moves GPU tiles rather than CPU pixels.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_undoIsGpu(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).is_some_and(|editor| editor.undo_is_gpu()) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_redoIsGpu(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).is_some_and(|editor| editor.redo_is_gpu()) as jint
}

/// The tiles the last commit touched, as flat `x, y, width, height` values.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasCommittedRects(
    env: JNIEnv,
    _this: JObject,
    canvas_handle: jlong,
    count: jint,
    out: JIntArray,
) -> jint {
    let Some(canvas) = surface(canvas_handle) else {
        return 0;
    };
    let mut values = Vec::with_capacity(count.max(0) as usize * 4);
    for index in 0..count.max(0) as usize {
        if let Some((x, y, width, height)) = canvas.committed_rect(index) {
            values.extend_from_slice(&[x as jint, y as jint, width as jint, height as jint]);
        }
    }
    env.set_int_array_region(&out, 0, &values).is_ok() as jint
}

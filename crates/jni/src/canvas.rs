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

/// The last present's split in milliseconds: composite, copy, draw, acquire, encode.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasPresentStats(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    out: JFloatArray,
) {
    if out.is_null() {
        return;
    }
    let mut values = [0.0f32; 8];
    // SAFETY: the surface keeps the stats; `values` is five floats as the entry expects.
    unsafe {
        dk_canvas_surface_present_stats(handle as *mut std::ffi::c_void, values.as_mut_ptr())
    };
    let _ = env.set_float_array_region(out, 0, &values);
}

unsafe extern "C" {
    fn dk_canvas_surface_present_stats(handle: *mut std::ffi::c_void, out: *mut f32);
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
    if canvas.hdr() {
        // An extended-range stack reads back half floats, and the core re-encodes them into the
        // document's working space; the eight-bit loop below cannot express that.
        let total = canvas.materialize_linear();
        let mut applied = 0;
        for index in 0..total {
            let Some((layer, x, y, width, height)) = canvas.linear_tile_info(index) else {
                continue;
            };
            let Some(tile) = canvas.linear_tile(index) else {
                continue;
            };
            let rect = Rect::new(x as usize, y as usize, width as usize, height as usize);
            if editor.apply_tiles_linear(layer, rect, &tile.pixels) {
                applied += 1;
            }
        }
        canvas.clear_linear_committed();
        return applied;
    }
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

/// Whether the created swapchain took the extended-range path. A driver that cannot present
/// half floats degrades to eight-bit, and the shell then keeps the document in that range.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasHdr(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    surface(handle).map_or(0, |canvas| canvas.hdr() as jint)
}

/// Upload a layer rectangle as premultiplied linear Display P3 floats, for an extended-range
/// resident stack.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasUploadLayerLinear(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    x: jint,
    y: jint,
    width: jint,
    height: jint,
    pixels: JFloatArray,
) -> jint {
    let Some(canvas) = surface(handle) else {
        return 0;
    };
    let Ok(length) = env.get_array_length(&pixels) else {
        return 0;
    };
    let (width, height) = (width.max(0) as usize, height.max(0) as usize);
    if width == 0 || height == 0 || (length as usize) < width * height * 4 {
        return 0;
    }
    let mut values = vec![0.0_f32; length as usize];
    if env.get_float_array_region(&pixels, 0, &mut values).is_err() {
        return 0;
    }
    canvas.upload_layer_linear(
        index.max(0) as usize,
        x.max(0) as u32,
        y.max(0) as u32,
        width as u32,
        height as u32,
        &values,
    ) as jint
}

/// The ideal line, oval or rectangle the last stroke snaps to, as `x, y` pairs. Zero means the
/// gesture stays freehand. The shell replays the path through the GPU stroke.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_shapePath(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    snap: jboolean,
    out: JFloatArray,
) -> jint {
    let Ok(length) = env.get_array_length(&out) else {
        return 0;
    };
    let mut values = vec![0.0_f32; length as usize];
    let copied = unsafe {
        dk_shape_path(
            handle as *mut std::ffi::c_void,
            snap as i32,
            values.as_mut_ptr(),
            length as u32,
        )
    };
    if copied <= 0 {
        return 0;
    }
    let used = copied as usize * 2;
    let _ = env.set_float_array_region(&out, 0, &values[..used]);
    copied
}

unsafe extern "C" {
    fn dk_shape_path(handle: *mut std::ffi::c_void, snap: i32, out: *mut f32, capacity: u32)
    -> i32;
}

/// Install the imported paper/tip map the live stroke runs with.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasSetBrushTexture(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    width: jint,
    height: jint,
    data: JByteArray,
) -> jint {
    if width <= 0 || height <= 0 {
        return 0;
    }
    let Ok(bytes) = env.convert_byte_array(&data) else {
        return 0;
    };
    let width = width as usize;
    let height = height as usize;
    if bytes.len() < width * height {
        return 0;
    }
    unsafe {
        dk_canvas_surface_set_brush_texture(
            handle as *mut std::ffi::c_void,
            width as u32,
            height as u32,
            bytes.as_ptr(),
            bytes.len() as u32,
        ) as jint
    }
}

/// Drop the imported map and paint with the procedural tip again.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasClearBrushTexture(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) {
    unsafe { dk_canvas_surface_clear_brush_texture(handle as *mut std::ffi::c_void) }
}

unsafe extern "C" {
    fn dk_canvas_surface_set_brush_texture(
        handle: *mut std::ffi::c_void,
        width: u32,
        height: u32,
        data: *const u8,
        len: u32,
    ) -> i32;
    fn dk_canvas_surface_clear_brush_texture(handle: *mut std::ffi::c_void);
}

/// Pick the swapchain's present mode: 0 fifo, 1 immediate, 2 mailbox. The bench uses this to
/// separate display pacing from frame cost.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasSetPresentMode(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    mode: jint,
) -> jint {
    surface(handle).is_some_and(|canvas| canvas.set_present_mode(mode.max(0) as u32)) as jint
}

/// The present mode in use, so a bench line can say which one it measured.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_canvasPresentMode(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    surface(handle).map_or(0, |canvas| canvas.present_mode() as jint)
}

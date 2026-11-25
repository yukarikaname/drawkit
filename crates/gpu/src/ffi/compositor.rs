//! C surface for the headless compositor used by bitmap shells.
//!
//! The Windows and Linux shells already own a bitmap; these calls keep the layers resident on the
//! GPU, composite there, and hand back one premultiplied RGBA image. No window handle is needed,
//! so the same API works with WinUI's WriteableBitmap and a GTK texture.

use std::os::raw::c_void;

use crate::compositor::GpuCompositor;

fn compositor<'a>(handle: *mut c_void) -> Option<&'a mut GpuCompositor> {
    if handle.is_null() {
        return None;
    }
    // SAFETY: the pointer came from `dk_gpu_compositor_new` and is freed once.
    Some(unsafe { &mut *(handle as *mut GpuCompositor) })
}

/// Create a half-float compositor for an extended-range document.
#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_new_hdr(width: u32, height: u32) -> *mut c_void {
    let created = std::panic::catch_unwind(|| GpuCompositor::new_hdr(width, height));
    match created {
        Ok(Some(compositor)) => Box::into_raw(Box::new(compositor)) as *mut c_void,
        Ok(None) => std::ptr::null_mut(),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown panic");
            eprintln!("gpu compositor: {message}");
            std::ptr::null_mut()
        }
    }
}

/// Create a compositor, or null when the machine has no usable adapter.
#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_new(width: u32, height: u32) -> *mut c_void {
    let created = std::panic::catch_unwind(|| GpuCompositor::new(width, height));
    match created {
        Ok(Some(compositor)) => Box::into_raw(Box::new(compositor)) as *mut c_void,
        Ok(None) => std::ptr::null_mut(),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown panic");
            eprintln!("gpu compositor: {message}");
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_free(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    // SAFETY: owned by this call from here on, and never handed out again.
    unsafe { drop(Box::from_raw(handle as *mut GpuCompositor)) };
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_reset(
    handle: *mut c_void,
    count: u32,
    width: u32,
    height: u32,
) {
    if let Some(compositor) = compositor(handle) {
        compositor.reset(count as usize, width, height);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_set_budget(handle: *mut c_void, bytes: u64) {
    if let Some(compositor) = compositor(handle) {
        compositor.set_budget(bytes as usize);
    }
}

/// Upload one rectangle of a layer as straight RGBA8.
///
/// # Safety
/// `pixels` must hold `width * height * 4` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_gpu_compositor_upload_layer(
    handle: *mut c_void,
    index: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    pixels: *const u8,
) -> i32 {
    let Some(compositor) = compositor(handle) else {
        return 0;
    };
    if pixels.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let size = width as usize * height as usize * 4;
    // SAFETY: the shell allocated `width * height * 4` bytes.
    let slice = unsafe { std::slice::from_raw_parts(pixels, size) };
    i32::from(compositor.upload_layer(crate::layers::RectUpload {
        index: index as usize,
        x,
        y,
        width,
        height,
        pixels: slice,
    }))
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_gpu_compositor_set_layer(
    handle: *mut c_void,
    index: u32,
    opacity: f32,
    blend: i32,
    visible: i32,
    fill: i32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
) {
    let Some(compositor) = compositor(handle) else {
        return;
    };
    let mut spec = crate::LayerSpec::new(index as usize);
    spec.opacity = opacity.clamp(0.0, 1.0);
    spec.blend = crate::Blend::from_i32(blend);
    spec.visible = visible != 0;
    if fill != 0 {
        spec.fill = Some([red, green, blue, alpha]);
    }
    compositor.set_layer(index as usize, spec);
}

/// Set the group a layer belongs to, or -1 for the root.
#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_set_layer_parent(handle: *mut c_void, index: u32, parent: i32) {
    if let Some(compositor) = compositor(handle) {
        compositor.set_layer_parent(index as usize, (parent >= 0).then_some(parent as usize));
    }
}

/// Set the layer whose alpha clips `index`, or -1 for no clip.
#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_set_layer_clip(handle: *mut c_void, index: u32, clip: i32) {
    if let Some(compositor) = compositor(handle) {
        compositor.set_layer_clip(index as usize, (clip >= 0).then_some(clip as usize));
    }
}

/// Drop the undo journal and pending readbacks. Replay uses this after every stroke.
#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_clear_history(handle: *mut c_void) {
    if let Some(compositor) = compositor(handle) {
        compositor.clear_history();
    }
}

/// Upload a full layer as straight linear floats.
///
/// # Safety
/// `pixels` must hold `width * height * 4` floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_gpu_compositor_upload_layer_linear(
    handle: *mut c_void,
    index: u32,
    width: u32,
    height: u32,
    pixels: *const f32,
) -> i32 {
    let Some(compositor) = compositor(handle) else {
        return 0;
    };
    if pixels.is_null() || compositor.size() != (width, height) {
        return 0;
    }
    let count = width as usize * height as usize * 4;
    // SAFETY: the shell allocated four floats per pixel.
    let pixels = unsafe { std::slice::from_raw_parts(pixels, count) };
    i32::from(compositor.upload_layer_linear(index as usize, pixels))
}

/// Composite into `out` as premultiplied linear Display P3 floats.
///
/// # Safety
/// `out` must hold `width * height * 4` floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_gpu_compositor_composite_linear(
    handle: *mut c_void,
    out: *mut f32,
    width: u32,
    height: u32,
) -> i32 {
    let Some(compositor) = compositor(handle) else {
        return 0;
    };
    if out.is_null() || compositor.size() != (width, height) {
        return 0;
    }
    let pixels = compositor.composite_linear();
    let count = width as usize * height as usize * 4;
    if pixels.len() < count {
        return 0;
    }
    // SAFETY: the shell allocated four floats per pixel.
    let target = unsafe { std::slice::from_raw_parts_mut(out, count) };
    target.copy_from_slice(&pixels[..count]);
    1
}

/// Blend a fill mask (one float per canvas pixel) into a resident layer.
///
/// # Safety
/// `mask` must hold `width * height` floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_gpu_compositor_apply_fill(
    handle: *mut c_void,
    layer: u32,
    mask: *const f32,
    width: u32,
    height: u32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
    alpha_locked: i32,
) -> i32 {
    let Some(compositor) = compositor(handle) else {
        return 0;
    };
    if mask.is_null() || compositor.size() != (width, height) {
        return 0;
    }
    let count = width as usize * height as usize;
    // SAFETY: the shell allocated one float per canvas pixel.
    let mask = unsafe { std::slice::from_raw_parts(mask, count) };
    i32::from(compositor.apply_fill(
        layer as usize,
        mask,
        [red, green, blue, alpha],
        alpha_locked != 0,
    ))
}

/// Apply the stale rectangles to the layer backups before a fill reads the composite.
#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_materialize(handle: *mut c_void) -> i32 {
    compositor(handle).map_or(0, |compositor| compositor.materialize().len() as i32)
}

/// Composite into `out` as premultiplied RGBA8.
///
/// # Safety
/// `out` must hold `width * height * 4` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_gpu_compositor_composite(
    handle: *mut c_void,
    out: *mut u8,
    width: u32,
    height: u32,
) -> i32 {
    let Some(compositor) = compositor(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 || compositor.size() != (width, height) {
        return 0;
    }
    let size = width as usize * height as usize * 4;
    let pixels = compositor.composite();
    if pixels.len() < size {
        return 0;
    }
    // SAFETY: the shell allocated `width * height * 4` bytes.
    let target = unsafe { std::slice::from_raw_parts_mut(out, size) };
    target.copy_from_slice(&pixels[..size]);
    1
}

/// Begin a stroke on the headless compositor. The arguments mirror the window surface; a replay
/// driver feeds recorded samples here instead of stamping them on the CPU.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_gpu_compositor_stroke_begin(
    handle: *mut c_void,
    width: u32,
    height: u32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
    flow: f32,
    pressure_alpha: f32,
    paper: f32,
    tip: f32,
    dual: f32,
    dual_scale: f32,
    dual_hardness: f32,
    dual_roundness: f32,
    dual_angle: f32,
    dual_paper: f32,
    dual_tip: f32,
    wet: f32,
    jitter: f32,
    grain: f32,
    dual_jitter: f32,
    dual_grain: f32,
    seed_low: u32,
    seed_high: u32,
    erase: i32,
) {
    if let Some(compositor) = compositor(handle) {
        compositor.stroke_begin(
            width,
            height,
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
            (seed_low as u64) | ((seed_high as u64) << 32),
            erase != 0,
            u32::MAX,
        );
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_gpu_compositor_stroke_segment(
    handle: *mut c_void,
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    radius: f32,
    hardness: f32,
    spacing: f32,
    roundness: f32,
    angle: f32,
    pressure: f32,
) {
    if let Some(compositor) = compositor(handle) {
        compositor.stroke_segment(
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

#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_stroke_end(handle: *mut c_void) {
    if let Some(compositor) = compositor(handle) {
        compositor.stroke_end();
    }
}

/// Follow the layer's alpha for the next compositor commit.
#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_set_stroke_alpha_lock(handle: *mut c_void, locked: i32) {
    if let Some(compositor) = compositor(handle) {
        compositor.set_stroke_alpha_lock(locked != 0);
    }
}

/// Make the next compositor commit a smudge with this pixel offset.
#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_set_stroke_smudge(handle: *mut c_void, dx: f32, dy: f32) {
    if let Some(compositor) = compositor(handle) {
        compositor.set_stroke_smudge([dx, dy]);
    }
}

/// Composite the live stroke into a layer. Returns the number of touched tiles.
#[unsafe(no_mangle)]
pub extern "C" fn dk_gpu_compositor_stroke_commit(
    handle: *mut c_void,
    layer: u32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
) -> i32 {
    let Some(compositor) = compositor(handle) else {
        return 0;
    };
    compositor.commit_stroke(layer as usize, [red, green, blue], alpha) as i32
}

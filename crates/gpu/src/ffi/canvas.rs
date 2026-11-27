use std::os::raw::c_void;

pub(super) fn canvas<'a>(handle: *mut c_void) -> Option<&'a mut crate::surface::CanvasSurface> {
    if handle.is_null() {
        return None;
    }
    // SAFETY: the pointer came from `dk_canvas_surface_new` and is freed once.
    Some(unsafe { &mut *(handle as *mut crate::surface::CanvasSurface) })
}

/// Create a swapchain on a live `CAMetalLayer`. The layer must outlive the returned handle.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_new(
    layer: *mut c_void,
    width: u32,
    height: u32,
    hdr: i32,
    color_space: i32,
) -> *mut c_void {
    if layer.is_null() || width == 0 || height == 0 {
        return std::ptr::null_mut();
    }
    // wgpu can panic on a backend that cannot make an adapter for the layer; the shell must get a
    // null handle and fall back to its CPU path instead of aborting the process.
    let created = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        crate::surface::CanvasSurface::new(layer, width, height, hdr != 0, color_space)
    }));
    match created {
        Ok(Some(surface)) => Box::into_raw(Box::new(surface)) as *mut c_void,
        Ok(None) => std::ptr::null_mut(),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown panic");
            eprintln!("canvas surface: {message}");
            std::ptr::null_mut()
        }
    }
}

/// Create a swapchain on an Android `ANativeWindow`. The window must outlive the handle.
///
/// # Safety
/// `window` must come from `ANativeWindow_fromSurface` and be released by the shell.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_new_android(
    window: *mut c_void,
    width: u32,
    height: u32,
    hdr: i32,
) -> *mut c_void {
    match unsafe { crate::surface::CanvasSurface::new_android(window, width, height, hdr != 0) } {
        Some(surface) => Box::into_raw(Box::new(surface)) as *mut c_void,
        None => std::ptr::null_mut(),
    }
}

/// Create a swapchain on a native window instead of a CAMetalLayer.
///
/// `platform` is 0 for a WinUI `SwapChainPanel`, 1 for a Win32 `HWND`, 2 for Xlib, 3 for XCB and
/// 4 for Wayland. The display and window must outlive the returned handle.
///
/// The shell must own the window's drawing. A toolkit that composites the same surface itself
/// (GTK4 snapshots every widget into the toplevel `wl_surface` or X window) cannot use this: two
/// renderers would attach buffers to one surface. Such a shell composites offscreen instead, as
/// `app-linux` does.
///
/// # Safety
/// The raw handles must point at a live window/display pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_new_native(
    display: *mut c_void,
    window: u64,
    platform: i32,
    width: u32,
    height: u32,
    hdr: i32,
    color_space: i32,
) -> *mut c_void {
    if width == 0 || height == 0 {
        return std::ptr::null_mut();
    }
    let created = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        crate::surface::CanvasSurface::new_native(
            display,
            window,
            platform,
            width,
            height,
            hdr != 0,
            color_space,
        )
    }));
    match created {
        Ok(Some(surface)) => Box::into_raw(Box::new(surface)) as *mut c_void,
        Ok(None) => std::ptr::null_mut(),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown panic");
            eprintln!("canvas surface: {message}");
            std::ptr::null_mut()
        }
    }
}

/// Display exposure in stops for the extended-range preview.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_set_exposure(handle: *mut c_void, exposure: f32) {
    if let Some(surface) = canvas(handle) {
        surface.set_exposure(exposure);
    }
}

/// Whether the surface really took the extended-range format.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_hdr(handle: *mut c_void) -> i32 {
    canvas(handle).map_or(0, |surface| i32::from(surface.hdr()))
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_free(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    // SAFETY: owned by this call from here on, and never handed out again.
    unsafe { drop(Box::from_raw(handle as *mut crate::surface::CanvasSurface)) };
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_resize(handle: *mut c_void, width: u32, height: u32) {
    if let Some(surface) = canvas(handle) {
        surface.resize(width, height);
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_canvas_surface_present(
    handle: *mut c_void,
    pixels: *const u8,
    canvas_width: u32,
    canvas_height: u32,
    paper_x: f32,
    paper_y: f32,
    paper_width: f32,
    paper_height: f32,
    dirty_x: u32,
    dirty_y: u32,
    dirty_width: u32,
    dirty_height: u32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    if pixels.is_null() || canvas_width == 0 || canvas_height == 0 {
        return 0;
    }
    let size = canvas_width as usize * canvas_height as usize * 4;
    // SAFETY: the shell passed `canvas_width * canvas_height * 4` bytes.
    let slice = unsafe { std::slice::from_raw_parts(pixels, size) };
    let dirty = if dirty_width == 0 || dirty_height == 0 {
        None
    } else {
        Some((dirty_x, dirty_y, dirty_width, dirty_height))
    };
    i32::from(surface.present(
        slice,
        canvas_width,
        canvas_height,
        [paper_x, paper_y, paper_width, paper_height],
        dirty,
    ))
}

/// Rotate the presented paper around its centre. Zero is upright.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_set_rotation(handle: *mut c_void, radians: f32) {
    if let Some(surface) = canvas(handle) {
        surface.set_rotation(radians);
    }
}

/// Whether the resident-layer path is available on this surface.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_supports_layers(handle: *mut c_void) -> i32 {
    canvas(handle).map_or(0, |surface| i32::from(surface.supports_layers()))
}

/// Start a canvas-sized layer stack with `count` slots.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_reset_layers(
    handle: *mut c_void,
    count: u32,
    canvas_width: u32,
    canvas_height: u32,
) {
    if let Some(surface) = canvas(handle) {
        surface.reset_layers(count as usize, canvas_width, canvas_height);
    }
}

/// Whether a resident layer was evicted by the memory budget.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_layer_missing(handle: *mut c_void, index: u32) -> i32 {
    canvas(handle).map_or(0, |surface| {
        i32::from(surface.layer_missing(index as usize))
    })
}

/// Set the resident-layer memory budget. Older layers are evicted least-recently-used first.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_set_layer_budget(handle: *mut c_void, bytes: u64) {
    if let Some(surface) = canvas(handle) {
        surface.set_layer_budget(bytes);
    }
}

/// Cap the bytes the GPU undo journal keeps. Dropping the oldest steps is the existing
/// over-budget behaviour, so the shell never sees a history that outgrew its texture budget.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_set_journal_budget(handle: *mut c_void, bytes: u64) {
    if let Some(surface) = canvas(handle) {
        surface.set_journal_budget(bytes);
    }
}

/// Upload one rectangle of a layer as straight RGBA8.
///
/// # Safety
/// `pixels` must hold `width * height * 4` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_upload_layer(
    handle: *mut c_void,
    index: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    pixels: *const u8,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    if pixels.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let size = width as usize * height as usize * 4;
    // SAFETY: the shell allocated `width * height * 4` bytes.
    let slice = unsafe { std::slice::from_raw_parts(pixels, size) };
    i32::from(surface.upload_layer_rect(index as usize, x, y, width, height, slice))
}

/// Upload one rectangle of a layer as straight linear Display P3 floats.
///
/// # Safety
/// `pixels` must hold `width * height * 4` floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_canvas_surface_upload_layer_linear(
    handle: *mut c_void,
    index: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    pixels: *const f32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    if pixels.is_null() || width == 0 || height == 0 {
        return 0;
    }
    let size = width as usize * height as usize * 4;
    // SAFETY: the shell allocated four floats per pixel.
    let slice = unsafe { std::slice::from_raw_parts(pixels, size) };
    i32::from(surface.upload_layer_linear(index as usize, x, y, width, height, slice))
}

/// Set opacity, blend mode, visibility and the fill colour of one resident layer.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_canvas_surface_set_layer(
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
    let Some(surface) = canvas(handle) else {
        return;
    };
    let mut spec = crate::LayerSpec::new(index as usize);
    spec.opacity = opacity.clamp(0.0, 1.0);
    spec.blend = crate::Blend::from_i32(blend);
    spec.visible = visible != 0;
    if fill != 0 {
        spec.fill = Some([red, green, blue, alpha]);
    }
    surface.set_layer_spec(index as usize, spec);
}

/// Set the group a resident layer belongs to, or -1 for the root.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_set_layer_parent(handle: *mut c_void, index: u32, parent: i32) {
    if let Some(surface) = canvas(handle) {
        surface.set_layer_parent(index as usize, (parent >= 0).then_some(parent as usize));
    }
}

/// Set the layer whose alpha clips `index`, or -1 for no clip.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_set_layer_clip(handle: *mut c_void, index: u32, clip: i32) {
    if let Some(surface) = canvas(handle) {
        surface.set_layer_clip(index as usize, (clip >= 0).then_some(clip as usize));
    }
}

/// Composite the resident layers and draw the result. Returns 0 when the surface cannot run the
/// layer path and the shell should present its CPU pixels instead.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_present_layers(
    handle: *mut c_void,
    paper_x: f32,
    paper_y: f32,
    paper_width: f32,
    paper_height: f32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    surface.present_layers([paper_x, paper_y, paper_width, paper_height])
}

/// Create a replay compositor on the surface's device. The pointer is a normal compositor handle
/// for the `dk_gpu_compositor_*` calls and is freed with `dk_gpu_compositor_free`.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_new_compositor(
    handle: *mut c_void,
    width: u32,
    height: u32,
) -> *mut c_void {
    let Some(surface) = canvas(handle) else {
        return std::ptr::null_mut();
    };
    match surface.new_replay(width, height) {
        Some(compositor) => Box::into_raw(Box::new(compositor)).cast(),
        None => std::ptr::null_mut(),
    }
}

/// Present a replay compositor's frame on this surface, on the GPU.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_present_compositor(
    handle: *mut c_void,
    compositor: *mut c_void,
    paper_x: f32,
    paper_y: f32,
    paper_width: f32,
    paper_height: f32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    if compositor.is_null() {
        return 0;
    }
    // SAFETY: the pointer came from `dk_canvas_surface_new_compositor`.
    let compositor = unsafe { &mut *(compositor as *mut crate::GpuCompositor) };
    surface.present_replay(compositor, [paper_x, paper_y, paper_width, paper_height])
}

/// Present premultiplied linear `f32` values; the surface converts the dirty rectangle to halves.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_canvas_surface_present_hdr(
    handle: *mut c_void,
    pixels: *const f32,
    canvas_width: u32,
    canvas_height: u32,
    paper_x: f32,
    paper_y: f32,
    paper_width: f32,
    paper_height: f32,
    dirty_x: u32,
    dirty_y: u32,
    dirty_width: u32,
    dirty_height: u32,
) -> i32 {
    let Some(surface) = canvas(handle) else {
        return 0;
    };
    if pixels.is_null() || canvas_width == 0 || canvas_height == 0 {
        return 0;
    }
    let size = canvas_width as usize * canvas_height as usize * 4;
    // SAFETY: the shell passed `canvas_width * canvas_height * 4` floats.
    let slice = unsafe { std::slice::from_raw_parts(pixels, size) };
    let dirty = if dirty_width == 0 || dirty_height == 0 {
        None
    } else {
        Some((dirty_x, dirty_y, dirty_width, dirty_height))
    };
    i32::from(surface.present_linear(
        slice,
        canvas_width,
        canvas_height,
        [paper_x, paper_y, paper_width, paper_height],
        dirty,
    ))
}

#[cfg(test)]
mod surface_tests {
    use super::*;

    #[test]
    fn a_null_layer_is_rejected() {
        assert!(dk_canvas_surface_new(std::ptr::null_mut(), 10, 10, 0, 0).is_null());
    }
}

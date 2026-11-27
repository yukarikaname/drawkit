// The C entry points of `drawkit-gpu` the GTK shell calls. They mirror `crates/gpu/src/ffi`; the
// shell owns the surface handle, the GBM buffer and the GTK texture, so the Rust side never owns
// the presentation buffer.

use std::os::raw::c_void;

unsafe extern "C" {
    pub(crate) fn dk_canvas_surface_new_native(
        display: *mut c_void,
        window: u64,
        platform: i32,
        width: u32,
        height: u32,
        hdr: i32,
        color_space: i32,
    ) -> *mut c_void;
    pub(crate) fn dk_canvas_surface_free(handle: *mut c_void);
    pub(crate) fn dk_canvas_surface_supports_layers(handle: *mut c_void) -> i32;
    pub(crate) fn dk_canvas_surface_reset_layers(
        handle: *mut c_void,
        count: u32,
        canvas_width: u32,
        canvas_height: u32,
    );
    pub(crate) fn dk_canvas_surface_set_layer_budget(handle: *mut c_void, bytes: u64);
    pub(crate) fn dk_canvas_surface_upload_layer(
        handle: *mut c_void,
        index: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        pixels: *const u8,
    ) -> i32;
    pub(crate) fn dk_canvas_surface_set_layer(
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
    );
    pub(crate) fn dk_canvas_surface_set_layer_parent(handle: *mut c_void, index: u32, parent: i32);
    pub(crate) fn dk_canvas_surface_set_layer_clip(handle: *mut c_void, index: u32, clip: i32);
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn dk_canvas_surface_stroke_begin(
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
        preview_layer: u32,
    );
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn dk_canvas_surface_stroke_segment(
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
    );
    pub(crate) fn dk_canvas_surface_stroke_commit(
        handle: *mut c_void,
        layer: u32,
        red: f32,
        green: f32,
        blue: f32,
        alpha: f32,
    ) -> i32;
    pub(crate) fn dk_canvas_surface_committed_rect(
        handle: *mut c_void,
        index: u32,
        rect: *mut u32,
    ) -> i32;
    pub(crate) fn dk_canvas_surface_stroke_end(handle: *mut c_void);
    pub(crate) fn dk_canvas_surface_materialize(handle: *mut c_void) -> i32;
    pub(crate) fn dk_canvas_surface_committed_layer(handle: *mut c_void, index: u32) -> i32;
    pub(crate) fn dk_canvas_surface_committed_info(
        handle: *mut c_void,
        index: u32,
        rect: *mut u32,
    ) -> i32;
    pub(crate) fn dk_canvas_surface_committed_tile(
        handle: *mut c_void,
        index: u32,
        rect: *mut u32,
        out: *mut u8,
        len: u32,
    ) -> i32;
    pub(crate) fn dk_canvas_surface_clear_committed(handle: *mut c_void);
    pub(crate) fn dk_canvas_surface_undo_tiles(handle: *mut c_void) -> i32;
    pub(crate) fn dk_canvas_surface_redo_tiles(handle: *mut c_void) -> i32;
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn dk_canvas_surface_apply_fill(
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
        under: i32,
        dirty: *mut u32,
    ) -> i32;
    pub(crate) fn dk_canvas_surface_set_brush_texture(
        handle: *mut c_void,
        width: u32,
        height: u32,
        data: *const u8,
        len: u32,
    ) -> i32;
    pub(crate) fn dk_canvas_surface_clear_brush_texture(handle: *mut c_void);
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn dk_canvas_surface_present_dmabuf(
        handle: *mut c_void,
        fd: i32,
        width: u32,
        height: u32,
        stride: u32,
        offset: u64,
        modifier: u64,
        paper_x: f32,
        paper_y: f32,
        paper_width: f32,
        paper_height: f32,
    ) -> i32;
}

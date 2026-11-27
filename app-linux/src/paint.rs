// The canvas surface handle the GTK shell owns. It is created on the window's Wayland surface
// (for the device and the adapter), but nothing is ever presented there: the pixels leave through
// the GBM buffer instead, so GTK and wgpu never share a `wl_surface`.

use std::ffi::c_void;
use std::ptr::NonNull;

use glib::prelude::*;
use glib::translate::ToGlibPtr;
use gtk::gdk;
use gtk::gdk::prelude::*;

use crate::ffi::*;

/// The handles `dk_canvas_surface_new_native` wants for a Wayland window: a `wl_display *` and a
/// `wl_surface *`, the same pair `raw-window-handle` defines.
pub(crate) struct Target {
    display: NonNull<c_void>,
    surface: NonNull<c_void>,
}

impl Target {
    pub(crate) fn from_surface(surface: &gdk::Surface) -> Option<Self> {
        let wayland = surface.downcast_ref::<gdk4_wayland::WaylandSurface>()?;
        let display = surface.display();
        let display = display.downcast_ref::<gdk4_wayland::WaylandDisplay>()?;
        // SAFETY: both objects outlive the window this handle is attached to.
        let wl_display = unsafe {
            gdk4_wayland::ffi::gdk_wayland_display_get_wl_display(display.to_glib_none().0)
        };
        let wl_surface = unsafe {
            gdk4_wayland::ffi::gdk_wayland_surface_get_wl_surface(wayland.to_glib_none().0)
        };
        Some(Self {
            display: NonNull::new(wl_display)?,
            surface: NonNull::new(wl_surface)?,
        })
    }
}

/// The brush parameters of one stroke, as the surface takes them.
pub(crate) struct StrokeStart {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) colour: [f32; 4],
    pub(crate) flow: f32,
    pub(crate) pressure_alpha: f32,
    pub(crate) paper: f32,
    pub(crate) tip: f32,
    pub(crate) dual: [f32; 7],
    pub(crate) wet: f32,
    pub(crate) jitter: f32,
    pub(crate) grain: f32,
    pub(crate) dual_jitter: f32,
    pub(crate) dual_grain: f32,
    pub(crate) seed: u64,
}

/// One rectangle a GPU commit read back, straight RGBA8.
pub(crate) struct CommittedTile {
    pub(crate) layer: usize,
    pub(crate) rect: (u32, u32, u32, u32),
    pub(crate) pixels: Vec<u8>,
}

pub(crate) struct PaintSurface {
    handle: *mut c_void,
}

impl PaintSurface {
    pub(crate) fn new(target: &Target, width: u32, height: u32) -> Result<Self, String> {
        let handle = unsafe {
            dk_canvas_surface_new_native(
                target.display.as_ptr(),
                target.surface.as_ptr() as u64,
                4,
                width,
                height,
                0,
                0,
            )
        };
        if handle.is_null() {
            return Err("no Vulkan device".to_string());
        }
        Ok(Self { handle })
    }

    pub(crate) fn supports_layers(&self) -> bool {
        unsafe { dk_canvas_surface_supports_layers(self.handle) != 0 }
    }

    pub(crate) fn reset_layers(&mut self, count: usize, width: u32, height: u32) {
        unsafe { dk_canvas_surface_reset_layers(self.handle, count as u32, width, height) };
    }

    pub(crate) fn set_layer_budget(&mut self, bytes: u64) {
        unsafe { dk_canvas_surface_set_layer_budget(self.handle, bytes) };
    }

    pub(crate) fn upload_layer(
        &self,
        index: usize,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> bool {
        if pixels.len() < width as usize * height as usize * 4 || width == 0 || height == 0 {
            return false;
        }
        unsafe {
            dk_canvas_surface_upload_layer(
                self.handle,
                index as u32,
                x,
                y,
                width,
                height,
                pixels.as_ptr(),
            ) != 0
        }
    }

    pub(crate) fn set_layer(
        &mut self,
        index: usize,
        opacity: f32,
        blend: i32,
        visible: bool,
        fill: Option<[f32; 4]>,
    ) {
        let has_fill = fill.is_some();
        let fill = fill.unwrap_or([0.0; 4]);
        unsafe {
            dk_canvas_surface_set_layer(
                self.handle,
                index as u32,
                opacity,
                blend,
                i32::from(visible),
                i32::from(has_fill),
                fill[0],
                fill[1],
                fill[2],
                fill[3],
            );
        }
    }

    pub(crate) fn set_layer_parent(&mut self, index: usize, parent: Option<usize>) {
        let parent = parent.map_or(-1, |parent| parent as i32);
        unsafe { dk_canvas_surface_set_layer_parent(self.handle, index as u32, parent) };
    }

    pub(crate) fn set_layer_clip(&mut self, index: usize, clip: Option<usize>) {
        let clip = clip.map_or(-1, |clip| clip as i32);
        unsafe { dk_canvas_surface_set_layer_clip(self.handle, index as u32, clip) };
    }

    pub(crate) fn set_brush_texture(&mut self, width: u32, height: u32, data: &[u8]) {
        if width == 0 || height == 0 || data.len() < width as usize * height as usize {
            return;
        }
        unsafe {
            dk_canvas_surface_set_brush_texture(
                self.handle,
                width,
                height,
                data.as_ptr(),
                data.len() as u32,
            );
        }
    }

    pub(crate) fn clear_brush_texture(&mut self) {
        unsafe { dk_canvas_surface_clear_brush_texture(self.handle) };
    }

    pub(crate) fn stroke_begin(&mut self, stroke: &StrokeStart) {
        let colour = stroke.colour;
        let dual = stroke.dual;
        unsafe {
            dk_canvas_surface_stroke_begin(
                self.handle,
                stroke.width,
                stroke.height,
                colour[0],
                colour[1],
                colour[2],
                colour[3],
                stroke.flow,
                stroke.pressure_alpha,
                stroke.paper,
                stroke.tip,
                dual[0],
                dual[1],
                dual[2],
                dual[3],
                dual[4],
                dual[5],
                dual[6],
                stroke.wet,
                stroke.jitter,
                stroke.grain,
                stroke.dual_jitter,
                stroke.dual_grain,
                stroke.seed as u32,
                (stroke.seed >> 32) as u32,
                0,
                u32::MAX,
            );
        }
    }

    // The surface's C shape is flat; grouping the numbers would only hide it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stroke_segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        hardness: f32,
        spacing: f32,
        roundness: f32,
        angle: f32,
    ) {
        unsafe {
            dk_canvas_surface_stroke_segment(
                self.handle,
                from.0,
                from.1,
                to.0,
                to.1,
                radius,
                hardness,
                spacing,
                roundness,
                angle,
                1.0,
            );
        }
    }

    /// Composite the live stroke into `layer`; returns the rectangles kept for undo.
    pub(crate) fn stroke_commit(
        &mut self,
        layer: usize,
        colour: [f32; 3],
        flow: f32,
    ) -> Vec<(u32, u32, u32, u32)> {
        let count = unsafe {
            dk_canvas_surface_stroke_commit(
                self.handle,
                layer as u32,
                colour[0],
                colour[1],
                colour[2],
                flow,
            )
        };
        let mut rects = Vec::with_capacity(count.max(0) as usize);
        for index in 0..count {
            let mut rect = [0u32; 4];
            let found = unsafe {
                dk_canvas_surface_committed_rect(self.handle, index as u32, rect.as_mut_ptr())
            };
            if found != 0 {
                rects.push((rect[0], rect[1], rect[2], rect[3]));
            }
        }
        rects
    }

    pub(crate) fn stroke_end(&mut self) {
        unsafe { dk_canvas_surface_stroke_end(self.handle) };
    }

    pub(crate) fn undo_tiles(&mut self) -> bool {
        unsafe { dk_canvas_surface_undo_tiles(self.handle) != 0 }
    }

    pub(crate) fn redo_tiles(&mut self) -> bool {
        unsafe { dk_canvas_surface_redo_tiles(self.handle) != 0 }
    }

    /// Read the GPU pixels the last commit did not reach the CPU cache with.
    pub(crate) fn materialize(&mut self) -> Vec<CommittedTile> {
        let mut tiles = Vec::new();
        if unsafe { dk_canvas_surface_materialize(self.handle) } == 0 {
            return tiles;
        }
        let mut index = 0u32;
        loop {
            let mut rect = [0u32; 4];
            let bytes =
                unsafe { dk_canvas_surface_committed_info(self.handle, index, rect.as_mut_ptr()) };
            if bytes <= 0 {
                break;
            }
            let layer = unsafe { dk_canvas_surface_committed_layer(self.handle, index) };
            let mut pixels = vec![0u8; bytes as usize];
            let copied = unsafe {
                dk_canvas_surface_committed_tile(
                    self.handle,
                    index,
                    rect.as_mut_ptr(),
                    pixels.as_mut_ptr(),
                    pixels.len() as u32,
                )
            };
            if copied == 0 {
                break;
            }
            tiles.push(CommittedTile {
                layer: layer.max(0) as usize,
                rect: (rect[0], rect[1], rect[2], rect[3]),
                pixels,
            });
            index += 1;
        }
        unsafe { dk_canvas_surface_clear_committed(self.handle) };
        tiles
    }

    /// Write a fill mask into `layer`. Returns the union of the touched rectangles.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_fill(
        &mut self,
        layer: usize,
        mask: &[f32],
        width: u32,
        height: u32,
        colour: [f32; 3],
        alpha: f32,
        alpha_locked: bool,
        under: bool,
    ) -> Option<(u32, u32, u32, u32)> {
        if mask.len() < width as usize * height as usize {
            return None;
        }
        let mut dirty = [0u32; 4];
        let applied = unsafe {
            dk_canvas_surface_apply_fill(
                self.handle,
                layer as u32,
                mask.as_ptr(),
                width,
                height,
                colour[0],
                colour[1],
                colour[2],
                alpha,
                i32::from(alpha_locked),
                i32::from(under),
                dirty.as_mut_ptr(),
            )
        };
        (applied != 0).then_some((dirty[0], dirty[1], dirty[2], dirty[3]))
    }

    /// Import a GBM buffer once. The fd is owned by this call.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn import_dmabuf(
        &mut self,
        index: u32,
        fd: i32,
        width: u32,
        height: u32,
        stride: u32,
        offset: u64,
        modifier: u64,
    ) -> i32 {
        unsafe {
            dk_canvas_surface_import_dmabuf(
                self.handle, index, fd, width, height, stride, offset, modifier,
            )
        }
    }

    /// Render the canvas into an imported buffer.
    pub(crate) fn present_dmabuf(&mut self, index: u32, paper: [f32; 4]) -> i32 {
        unsafe {
            dk_canvas_surface_present_dmabuf(
                self.handle,
                index,
                paper[0],
                paper[1],
                paper[2],
                paper[3],
            )
        }
    }
}

impl Drop for PaintSurface {
    fn drop(&mut self) {
        unsafe { dk_canvas_surface_free(self.handle) };
    }
}

// The GTK4 canvas. The painting, the resident layers and the paper pass are the same Rust code the
// Apple shells link; GTK supplies the window, the pointer events and the layer list.
//
// Presentation is one dmabuf: `dmabuf.rs` owns a GBM buffer, the Rust surface renders into it, and
// a `GdkDmabufTexture` inside a `GtkGraphicsOffload` shows it. No swapchain is created on the
// window's surface and nothing is read back.
//
// The canvas is pinned left-to-right in every layout: the document, the pointer mapping and every
// saved coordinate are document space, so mirroring the widget would move the ink.

use std::cell::{Cell, RefCell};
use std::os::fd::FromRawFd;
use std::path::Path;
use std::rc::Rc;

use gtk::prelude::*;

use crate::compositor::{State, now_ms};
use crate::dmabuf::BufferSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Eraser,
    Fill,
}

impl Tool {
    fn label(self) -> &'static str {
        match self {
            Self::Brush => "Brush",
            Self::Eraser => "Eraser",
            Self::Fill => "Fill",
        }
    }
}

pub struct Canvas {
    pub(crate) picture: gtk::Picture,
    pub(crate) offload: gtk::GraphicsOffload,
    pub(crate) layers: gtk::ListBox,
    pub(crate) status: gtk::Label,
    pub(crate) state: RefCell<State>,
    buffers: RefCell<Option<BufferSet>>,
    pending: Cell<bool>,
    ink_probe: Cell<Option<(usize, usize)>>,
}

impl Canvas {
    pub fn new() -> Rc<Self> {
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Contain);
        picture.set_can_shrink(true);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        picture.set_direction(gtk::TextDirection::Ltr);
        let offload = gtk::GraphicsOffload::new(Some(&picture));
        // Offload makes the compositor scan our dma-buf out directly. Compositors differ in what
        // they allow on such a subsurface — one reports "surface already exists" for the syncobj
        // surface — so `DRAWKIT_NO_OFFLOAD=1` puts the same dma-buf texture through GTK's renderer
        // instead: still no CPU copy, one GPU pass more.
        let offload_on = !std::env::var("DRAWKIT_NO_OFFLOAD").is_ok_and(|value| !value.is_empty());
        offload.set_enabled(if offload_on {
            gtk::GraphicsOffloadEnabled::Enabled
        } else {
            gtk::GraphicsOffloadEnabled::Disabled
        });
        let layers = gtk::ListBox::new();
        layers.set_selection_mode(gtk::SelectionMode::Single);
        let status = gtk::Label::new(Some("Brush · 18 px"));
        status.set_halign(gtk::Align::Start);
        let canvas = Rc::new(Self {
            picture,
            offload,
            layers,
            status,
            state: RefCell::new(State::new()),
            buffers: RefCell::new(None),
            pending: Cell::new(false),
            ink_probe: Cell::new(None),
        });

        // The first present usually runs before the picture has an allocation; a map means it is
        // on screen and worth a frame, and `present` reschedules itself while it has no size.
        {
            let weak = Rc::downgrade(&canvas);
            canvas.picture.connect_map(move |_| {
                if let Some(canvas) = weak.upgrade() {
                    canvas.schedule();
                }
            });
        }

        let drag = gtk::GestureDrag::new();
        let weak = Rc::downgrade(&canvas);
        drag.connect_drag_begin(move |gesture, x, y| {
            if let Some(canvas) = weak.upgrade() {
                canvas.drag_begin(gesture, x, y);
            }
        });
        let weak = Rc::downgrade(&canvas);
        drag.connect_drag_update(move |gesture, x, y| {
            if let Some(canvas) = weak.upgrade() {
                canvas.drag_update(gesture, x, y);
            }
        });
        let weak = Rc::downgrade(&canvas);
        drag.connect_drag_end(move |_, _, _| {
            if let Some(canvas) = weak.upgrade() {
                canvas.drag_end();
            }
        });
        canvas.picture.add_controller(drag);

        let weak = Rc::downgrade(&canvas);
        canvas.layers.connect_row_selected(move |_, row| {
            if let Some(canvas) = weak.upgrade() {
                canvas.select_row(row);
            }
        });

        canvas.refresh();
        canvas
    }

    pub fn widget(&self) -> gtk::Widget {
        self.offload.clone().upcast()
    }

    pub fn layer_list(&self) -> gtk::ListBox {
        self.layers.clone()
    }

    pub fn status_label(&self) -> gtk::Label {
        self.status.clone()
    }

    pub fn set_tool(self: &Rc<Self>, tool: Tool) {
        self.state.borrow_mut().tool = tool;
        self.update_status();
    }

    pub fn set_colour(self: &Rc<Self>, colour: gtk::gdk::RGBA) {
        self.state.borrow_mut().colour =
            [colour.red(), colour.green(), colour.blue(), colour.alpha()];
    }

    pub fn set_size(self: &Rc<Self>, size: f64) {
        self.state.borrow_mut().size = size.max(1.0);
        self.update_status();
    }

    /// Take the window's Wayland surface once the window is realized. The surface only supplies
    /// the Vulkan device and the adapter; presentation goes through the GBM buffer.
    pub fn attach(self: &Rc<Self>, window: &gtk::Window) {
        let Some(surface) = window.surface() else {
            self.state.borrow_mut().error = Some("the window has no native surface".to_string());
            self.update_status();
            return;
        };
        let Some(target) = crate::paint::Target::from_surface(&surface) else {
            self.state.borrow_mut().error = Some("needs a Wayland session".to_string());
            self.update_status();
            return;
        };
        {
            let mut state = self.state.borrow_mut();
            state.attach(&target);
        }
        self.update_status();
        self.schedule();
    }

    /// Queue one frame-clock redraw. Input never renders directly, so a burst of pen samples
    /// coalesces into the next frame instead of a buffer import per event.
    pub(crate) fn schedule(self: &Rc<Self>) {
        if self.pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        self.picture.add_tick_callback(move |_, _| {
            let Some(canvas) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            canvas.pending.set(false);
            canvas.present();
            glib::ControlFlow::Break
        });
    }

    /// Render the canvas into the current buffer and make GTK display it.
    fn present(self: &Rc<Self>) -> i32 {
        let bench_started = std::time::Instant::now();
        let (logical_width, logical_height) = (self.picture.width(), self.picture.height());
        if std::env::var("DRAWKIT_TRACE_PRESENT").is_ok() {
            eprintln!("drawkit: present {}x{}", logical_width, logical_height);
        }
        if logical_width <= 0 || logical_height <= 0 {
            // The first tick after realize runs before GTK has allocated the picture, so the frame
            // is not drawn and nothing else would ask for one. Try again next frame.
            self.schedule();
            return 0;
        }
        let scale = self.picture.scale_factor().max(1) as u32;
        let (width, height) = (logical_width as u32 * scale, logical_height as u32 * scale);
        let mut buffers = self.buffers.borrow_mut();
        if buffers
            .as_ref()
            .is_none_or(|set| set.size() != (width, height))
        {
            match BufferSet::new(width, height) {
                Ok(set) => *buffers = Some(set),
                Err(reason) => {
                    drop(buffers);
                    self.state.borrow_mut().error = Some(reason);
                    self.update_status();
                    return -1;
                }
            }
        }
        let Some(set) = buffers.as_mut() else {
            return 0;
        };
        // Both buffers still on screen: skipping one frame is better than tearing.
        let Some(frame) = set.acquire() else {
            return 0;
        };
        let paper = self.paper(width, height);
        let code = {
            let mut state = self.state.borrow_mut();
            if set.is_imported(frame.buffer) {
                // Nothing to import; the fd this frame carries is only a dup to close.
                // SAFETY: the fd came from `gbm_bo_get_fd` and is owned by this frame.
                drop(unsafe { std::os::fd::OwnedFd::from_raw_fd(frame.fd) });
                0
            } else {
                let code = state.import_dmabuf(
                    frame.buffer as u32,
                    frame.fd,
                    width,
                    height,
                    frame.stride,
                    frame.offset,
                    frame.modifier,
                );
                if code > 0 {
                    set.mark_imported(frame.buffer);
                }
                code
            }
        };
        let code = if code >= 0 {
            self.state.borrow_mut().present_dmabuf(frame.buffer as u32, paper)
        } else {
            code
        };
        if code != 1 {
            eprintln!("drawkit: dmabuf present code {code} (1 drawn, 0 no layers, <0 error)");
        }
        if code == 1 {
            match set.texture(&frame) {
                Ok(texture) => self.picture.set_paintable(Some(&texture)),
                Err(reason) => self.state.borrow_mut().error = Some(reason),
            }
        }
        drop(buffers);
        self.update_status();
        if code == 1 && std::env::var("DRAWKIT_BENCH_POINTER").is_ok_and(|value| value == "1") {
            crate::bench::record_present(self, bench_started.elapsed().as_secs_f64() * 1000.0);
        }
        code
    }

    /// The paper rectangle inside a buffer of `width` x `height` device pixels.
    fn paper(&self, width: u32, height: u32) -> [f32; 4] {
        let (canvas_width, canvas_height) = {
            let state = self.state.borrow();
            (state.width as f32, state.height as f32)
        };
        let (width, height) = (width as f32, height as f32);
        let zoom = (width / canvas_width).min(height / canvas_height);
        [
            (width - canvas_width * zoom) / 2.0,
            (height - canvas_height * zoom) / 2.0,
            canvas_width * zoom,
            canvas_height * zoom,
        ]
    }

    // The bench drives the same three calls the drag handler makes, plus one present.
    pub(crate) fn bench_begin_stroke(&self, point: (f32, f32)) {
        let mut state = self.state.borrow_mut();
        state.begin_stroke(point);
        state.last = Some(point);
    }

    pub(crate) fn bench_segment(&self, from: (f32, f32), to: (f32, f32)) {
        self.state.borrow_mut().segment(from, to);
    }

    pub(crate) fn bench_end_stroke(&self) {
        self.state.borrow_mut().end_stroke();
    }

    /// One present, and whether it drew: a skipped frame (no free buffer) must not be measured as
    /// a cheap one.
    pub(crate) fn bench_present(self: &Rc<Self>) -> i32 {
        self.present()
    }

    pub(crate) fn bench_set_ink_probe(&self, point: (usize, usize)) {
        self.ink_probe.set(Some(point));
    }

    /// The pixel under the probe, after the tiles are read back: proves the stroke reached the
    /// document, not just the preview.
    pub(crate) fn bench_report_ink(&self) {
        let Some((x, y)) = self.ink_probe.get() else {
            return;
        };
        self.state.borrow_mut().materialize();
        let state = self.state.borrow();
        let layer = state.editor.document.active;
        let pixel = state.editor.document.layers[layer].surface.get(x, y);
        eprintln!("DRAWKIT_LINUX_POINTER ink at the stroke {pixel:?}");
    }

    pub(crate) fn bench_materialize(&self) {
        self.state.borrow_mut().materialize();
    }

    /// The core's pixel after the read-back: proves the stroke reached the tiles, not just the
    /// preview.
    pub(crate) fn bench_pixel(&self, x: usize, y: usize) -> [f32; 4] {
        let state = self.state.borrow();
        let layer = state.editor.document.active;
        state.editor.document.layers[layer].surface.get(x, y)
    }

    pub(crate) fn bench_status(&self) -> String {
        let state = self.state.borrow();
        match &state.error {
            Some(error) => format!("error {error}"),
            None => format!("ok gpu {} dmabuf present", state.gpu_ready),
        }
    }

    pub(crate) fn refresh(self: &Rc<Self>) {
        {
            let mut state = self.state.borrow_mut();
            state.gpu_ready = state.sync_gpu();
        }
        self.refresh_layers();
        self.schedule();
    }

    fn drag_begin(self: &Rc<Self>, _gesture: &gtk::GestureDrag, x: f64, y: f64) {
        let Some(point) = self.to_document(x, y) else {
            return;
        };
        let mut state = self.state.borrow_mut();
        state.time = now_ms();
        if state.tool == Tool::Fill {
            drop(state);
            self.begin_fill(point.0 as usize, point.1 as usize);
            return;
        }
        let mut state = self.state.borrow_mut();
        state.begin_stroke(point);
        state.last = Some(point);
        drop(state);
        self.schedule();
    }

    fn drag_update(self: &Rc<Self>, gesture: &gtk::GestureDrag, ox: f64, oy: f64) {
        let Some((sx, sy)) = gesture.start_point() else {
            return;
        };
        let Some(point) = self.to_document(sx + ox, sy + oy) else {
            return;
        };
        let mut state = self.state.borrow_mut();
        let Some(last) = state.last else {
            return;
        };
        state.segment(last, point);
        state.last = Some(point);
        drop(state);
        self.schedule();
    }

    fn drag_end(self: &Rc<Self>) {
        let mut state = self.state.borrow_mut();
        state.end_stroke();
        state.last = None;
        drop(state);
        self.refresh_layers();
        self.schedule();
    }

    fn to_document(&self, x: f64, y: f64) -> Option<(f32, f32)> {
        let state = self.state.borrow();
        let width = self.picture.width() as f64;
        let height = self.picture.height() as f64;
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let scale = (width / state.width as f64).min(height / state.height as f64);
        let origin_x = (width - state.width as f64 * scale) / 2.0;
        let origin_y = (height - state.height as f64 * scale) / 2.0;
        let x = ((x - origin_x) / scale).clamp(0.0, state.width as f64 - 1.0);
        let y = ((y - origin_y) / scale).clamp(0.0, state.height as f64 - 1.0);
        Some((x as f32, y as f32))
    }

    pub(crate) fn update_status(self: &Rc<Self>) {
        let state = self.state.borrow();
        let text = match &state.error {
            Some(error) => format!("{} · {:.0} px · {error}", state.tool.label(), state.size),
            None => format!(
                "{} · {:.0} px · {}% · GPU canvas (dmabuf)",
                state.tool.label(),
                state.size,
                (state.opacity * 100.0).round()
            ),
        };
        self.status.set_text(&text);
    }

    /// Opening from the command line (a double-clicked MIME file) and from the dialog share this.
    pub fn open_path(self: &Rc<Self>, path: &Path) -> bool {
        let opened = self.state.borrow_mut().open(path);
        if opened {
            self.refresh();
        }
        opened
    }

    pub fn save_to(self: &Rc<Self>, path: &Path) -> bool {
        self.state.borrow_mut().save(path)
    }

    pub fn save(self: &Rc<Self>) -> bool {
        let Some(path) = self.state.borrow().file().map(Path::to_path_buf) else {
            return false;
        };
        self.save_to(&path)
    }

    pub fn has_file(&self) -> bool {
        self.state.borrow().file().is_some()
    }

    pub fn file_name(&self) -> Option<String> {
        let state = self.state.borrow();
        let path = state.file()?;
        Some(path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ))
    }
}

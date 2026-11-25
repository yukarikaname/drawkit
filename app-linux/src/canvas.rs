// The GTK4 canvas. The painting core and the GPU compositor are the same libraries the Apple
// shells link; GTK only supplies the window, the pointer events and the layer list.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use drawkit_core::document::LayerKind;
use drawkit_core::tools::LayerField;
use gtk::prelude::*;

use crate::compositor::{State, now_ms};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Eraser,
    Fill,
}

pub struct Canvas {
    picture: gtk::Picture,
    layers: gtk::ListBox,
    status: gtk::Label,
    state: RefCell<State>,
    fill: RefCell<Option<glib::SourceId>>,
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

impl Canvas {
    pub fn new() -> Rc<Self> {
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Contain);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        let layers = gtk::ListBox::new();
        layers.set_selection_mode(gtk::SelectionMode::Single);
        let status = gtk::Label::new(Some("Brush · 18 px"));
        let canvas = Rc::new(Self {
            picture,
            layers,
            status,
            state: RefCell::new(State::new()),
            fill: RefCell::new(None),
        });

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
        self.picture.clone().upcast()
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
        self.state.borrow_mut().colour = [
            colour.red() as f32,
            colour.green() as f32,
            colour.blue() as f32,
            colour.alpha() as f32,
        ];
    }

    pub fn set_size(self: &Rc<Self>, size: f64) {
        self.state.borrow_mut().size = size.max(1.0);
        self.update_status();
    }

    pub fn active_opacity(&self) -> f64 {
        let state = self.state.borrow();
        state
            .editor
            .document
            .layers
            .get(state.editor.document.active)
            .map(|layer| layer.opacity as f64)
            .unwrap_or(1.0)
    }

    /// The selected layer's opacity, as one undoable layer-state change.
    pub fn set_layer_opacity(self: &Rc<Self>, opacity: f64) {
        self.finish_fill();
        let mut state = self.state.borrow_mut();
        let index = state.editor.document.active;
        let Some(layer) = state.editor.document.layers.get(index) else {
            return;
        };
        let before = layer.meta();
        state.editor.document.layers[index].opacity = opacity.clamp(0.0, 1.0) as f32;
        state
            .editor
            .record_layer_state(index, LayerField::Opacity, before);
        drop(state);
        self.refresh();
    }

    pub fn set_opacity(self: &Rc<Self>, opacity: f64) {
        self.state.borrow_mut().opacity = opacity.clamp(0.0, 1.0);
        self.update_status();
    }

    pub fn undo(self: &Rc<Self>) {
        self.finish_fill();
        let mut state = self.state.borrow_mut();
        if state.editor.undo_is_gpu() {
            if state.editor.undo().is_none() {
                return;
            }
            if let Some(gpu) = state.gpu.as_mut() {
                gpu.undo_tiles();
            }
            state.gpu_stale = true;
            drop(state);
            self.present();
            return;
        }
        state.materialize();
        state.editor.undo();
        drop(state);
        self.refresh();
    }

    pub fn redo(self: &Rc<Self>) {
        self.finish_fill();
        let mut state = self.state.borrow_mut();
        if state.editor.redo_is_gpu() {
            if state.editor.redo().is_none() {
                return;
            }
            if let Some(gpu) = state.gpu.as_mut() {
                gpu.redo_tiles();
            }
            state.gpu_stale = true;
            drop(state);
            self.present();
            return;
        }
        state.materialize();
        state.editor.redo();
        drop(state);
        self.refresh();
    }

    /// Commit a fill that is still spreading, so undo, a stroke or a layer change sees the
    /// complete step.
    pub fn finish_fill(self: &Rc<Self>) {
        let Some(source) = self.fill.borrow_mut().take() else {
            return;
        };
        source.remove();
        self.state.borrow_mut().editor.finish_fill_animation();
        self.refresh();
    }

    /// Start the fill at the tap and reveal it in eased distance bands. The source stops itself
    /// once the reveal is complete and the core has recorded the single undo step.
    fn begin_fill(self: &Rc<Self>, x: usize, y: usize) -> bool {
        self.finish_fill();
        let mut state = self.state.borrow_mut();
        state.materialize();
        let time = now_ms();
        let colour = state.colour;
        state.time = time;
        let started = state
            .editor
            .begin_fill_animation(
                x,
                y,
                0.12,
                colour,
                drawkit_core::fill::FillOptions::default(),
                time,
            )
            .is_some();
        drop(state);
        if !started {
            return false;
        }
        let weak = Rc::downgrade(self);
        let start = Instant::now();
        let source = glib::timeout_add_local(Duration::from_millis(16), move || {
            let Some(canvas) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let progress = (start.elapsed().as_secs_f64() / 0.22).min(1.0);
            let eased = 1.0 - (1.0 - progress).powi(3);
            let changed = canvas
                .state
                .borrow_mut()
                .editor
                .fill_animation_step(eased as f32)
                .is_some();
            if changed {
                canvas.refresh();
            }
            if progress < 1.0 {
                return glib::ControlFlow::Continue;
            }
            let _ = canvas.fill.borrow_mut().take();
            canvas.state.borrow_mut().editor.finish_fill_animation();
            canvas.refresh();
            glib::ControlFlow::Break
        });
        *self.fill.borrow_mut() = Some(source);
        true
    }

    pub fn add_layer(self: &Rc<Self>) {
        self.finish_fill();
        let mut state = self.state.borrow_mut();
        let before = state.editor.document.active;
        let name = format!("Layer {}", state.editor.document.layers.len() + 1);
        let index = state.editor.document.add_layer(name);
        let layer = state.editor.document.layers[index].clone();
        let after = state.editor.document.active;
        state
            .editor
            .record_layer_insertion(index, layer, before, after);
        drop(state);
        self.refresh();
    }

    pub fn extract_line_art(self: &Rc<Self>) {
        self.finish_fill();
        let mut state = self.state.borrow_mut();
        let added = state
            .editor
            .extract_line_art(drawkit_core::tools::LineArtOptions::default())
            .is_some();
        drop(state);
        if added {
            self.refresh();
        }
    }

    pub fn add_group(self: &Rc<Self>) {
        self.finish_fill();
        let mut state = self.state.borrow_mut();
        let before = state.editor.document.active;
        let name = format!("Group {}", state.editor.document.layers.len() + 1);
        let index = state.editor.document.add_group(name);
        let layer = state.editor.document.layers[index].clone();
        let after = state.editor.document.active;
        state
            .editor
            .record_layer_insertion(index, layer, before, after);
        drop(state);
        self.refresh();
    }

    pub fn remove_layer(self: &Rc<Self>) {
        self.finish_fill();
        let mut state = self.state.borrow_mut();
        if let Some(removal) = state.editor.document.remove_active_collect() {
            state.editor.record_layer_removal(
                removal.removed,
                removal.inserted,
                removal.active_before,
                removal.active_after,
            );
        }
        drop(state);
        self.refresh();
    }

    pub fn move_layer(self: &Rc<Self>, steps: i32) {
        self.finish_fill();
        let mut state = self.state.borrow_mut();
        let before = state.editor.document.active;
        if let Some((after, applied)) = state.editor.document.move_layer(before, steps) {
            let active = state.editor.document.active;
            state
                .editor
                .record_layer_move(before, after, applied, before, active);
        }
        drop(state);
        self.refresh();
    }

    fn select_row(self: &Rc<Self>, row: Option<&gtk::ListBoxRow>) {
        let Some(row) = row else { return };
        let index = row.index().max(0) as usize;
        let mut state = self.state.borrow_mut();
        if index < state.editor.document.layers.len() {
            state.editor.document.active = index;
        }
        drop(state);
        self.update_status();
    }

    fn set_visible(self: &Rc<Self>, index: usize, visible: bool) {
        let mut state = self.state.borrow_mut();
        let Some(layer) = state.editor.document.layers.get(index) else {
            return;
        };
        let before = layer.meta();
        state.editor.document.layers[index].visible = visible;
        state
            .editor
            .record_layer_state(index, LayerField::Visible, before);
        drop(state);
        self.refresh();
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
        drop(state);
        self.finish_fill();
        let mut state = self.state.borrow_mut();
        state.begin_stroke(point);
        state.last = Some(point);
        drop(state);
        self.present();
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
        self.present();
    }

    fn drag_end(self: &Rc<Self>) {
        let mut state = self.state.borrow_mut();
        state.end_stroke();
        state.last = None;
        drop(state);
        self.present();
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

    fn refresh(self: &Rc<Self>) {
        let mut state = self.state.borrow_mut();
        let ready = state.sync_gpu();
        state.gpu_ready = ready;
        state.compose();
        let texture = state.make_texture();
        self.picture.set_paintable(Some(&texture));
        drop(state);
        self.refresh_layers();
        self.update_status();
    }

    fn present(self: &Rc<Self>) {
        let mut state = self.state.borrow_mut();
        state.compose();
        let texture = state.make_texture();
        self.picture.set_paintable(Some(&texture));
    }

    fn refresh_layers(self: &Rc<Self>) {
        while let Some(child) = self.layers.first_child() {
            self.layers.remove(&child);
        }
        let state = self.state.borrow();
        let count = state.editor.document.layers.len();
        let active = state.editor.document.active;
        drop(state);
        for index in (0..count).rev() {
            let state = self.state.borrow();
            let layer = &state.editor.document.layers[index];
            let name = layer.name.clone();
            let visible = layer.visible;
            let kind = layer.kind;
            drop(state);
            let row = gtk::ListBoxRow::new();
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let eye = gtk::ToggleButton::with_label(if visible { "◉" } else { "○" });
            eye.set_active(visible);
            let weak = Rc::downgrade(self);
            eye.connect_toggled(move |button| {
                if let Some(canvas) = weak.upgrade() {
                    canvas.set_visible(index, button.is_active());
                }
            });
            let text = format!(
                "{}{}",
                if kind == LayerKind::Group { "▸ " } else { "" },
                name
            );
            let label = gtk::Label::new(Some(&text));
            label.set_xalign(0.0);
            hbox.append(&eye);
            hbox.append(&label);
            row.set_child(Some(&hbox));
            self.layers.append(&row);
            if index == active {
                self.layers.select_row(Some(&row));
            }
        }
    }

    fn update_status(self: &Rc<Self>) {
        let state = self.state.borrow();
        self.status.set_text(&format!(
            "{} · {:.0} px · {}%",
            state.tool.label(),
            state.size,
            (state.opacity * 100.0).round()
        ));
    }
}

// The painting document behind the GTK canvas. GTK never owns pixels: this state keeps the
// core Editor, the resident GPU layers and the one RGBA8 buffer the picture texture reads.

use drawkit_core::color::Rgba;
use drawkit_core::document::{BlendMode, Layer, LayerKind};
use drawkit_core::surface::Rect;
use drawkit_core::tools::Editor;
use drawkit_gpu::{Blend, GpuCompositor, LayerSpec, RectUpload};

use crate::canvas::Tool;

pub(crate) struct State {
    pub(crate) editor: Editor,
    pub(crate) gpu: Option<GpuCompositor>,
    pub(crate) gpu_ready: bool,
    pixels: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) size: f64,
    pub(crate) opacity: f64,
    pub(crate) colour: Rgba,
    pub(crate) tool: Tool,
    pub(crate) last: Option<(f32, f32)>,
    pub(crate) time: u64,
    gpu_stroke: bool,
    pub(crate) gpu_stale: bool,
}

impl State {
    pub(crate) fn new() -> Self {
        let width = 1024;
        let height = 1024;
        Self {
            editor: Editor::new(width as usize, height as usize, 512 << 20),
            gpu: GpuCompositor::new(width, height),
            gpu_ready: false,
            pixels: vec![0; width as usize * height as usize * 4],
            width,
            height,
            size: 18.0,
            opacity: 1.0,
            colour: [0.10, 0.09, 0.08, 1.0],
            tool: Tool::Brush,
            last: None,
            time: 0,
            gpu_stroke: false,
            gpu_stale: false,
        }
    }

    fn ink(&self) -> [f32; 4] {
        [
            self.colour[0],
            self.colour[1],
            self.colour[2],
            self.colour[3] * self.opacity as f32,
        ]
    }

    /// Start a stroke. A brush on a ready GPU surface stays there: the core only records the
    /// events and the coverage accumulates in the preview.
    pub(crate) fn begin_stroke(&mut self, point: (f32, f32)) {
        let eligible = self.gpu_ready && self.tool == Tool::Brush;
        if eligible {
            self.editor.defer_raster = true;
            let (width, height, ink) = (self.width, self.height, self.ink());
            let brush = self.editor.brush;
            let secondary = drawkit_core::brush::Brush::for_kind(
                drawkit_core::brush::BrushKind::from_i32(brush.dual_kind as i32),
            );
            if let Some(gpu) = self.gpu.as_mut() {
                gpu.stroke_begin(width, height, ink, brush.flow, brush.pressure_alpha,
                    brush.paper, brush.texture,
                    [brush.dual, brush.dual_scale, secondary.hardness, secondary.roundness,
                     secondary.angle, secondary.paper, secondary.texture],
                    brush.wet);
            }
            self.gpu_stroke = true;
        }
        if !eligible {
            self.materialize();
        }
        let time = self.time;
        self.editor.begin_stroke(time);
        self.segment(point, point);
    }

    /// Finish the stroke on whichever path it started.
    pub(crate) fn end_stroke(&mut self) {
        self.editor.end_stroke();
        if !self.gpu_stroke {
            return;
        }
        self.gpu_stroke = false;
        self.editor.defer_raster = false;
        if !self.editor.begin_tile_commit() {
            self.editor.flush_deferred();
            return;
        }
        let active = self.editor.document.active;
        let ink = self.ink();
        let flow = self.editor.brush.flow;
        let rects = if let Some(gpu) = self.gpu.as_mut() {
            let count = gpu.commit_stroke(active, [ink[0], ink[1], ink[2]], flow);
            let mut rects = Vec::with_capacity(count);
            for index in 0..count {
                if let Some(rect) = gpu.committed_rect(index) {
                    rects.push(Rect::new(
                        rect.0 as usize,
                        rect.1 as usize,
                        rect.2 as usize,
                        rect.3 as usize,
                    ));
                }
            }
            gpu.stroke_end();
            rects
        } else {
            Vec::new()
        };
        self.editor.end_gpu_commit(&rects);
        if !rects.is_empty() {
            self.gpu_stale = true;
        }
    }

    /// Rebuild the CPU pixels for anything a stroke left on the GPU.
    pub(crate) fn materialize(&mut self) {
        if !self.gpu_stale {
            return;
        }
        self.gpu_stale = false;
        let tiles = match self.gpu.as_mut() {
            Some(gpu) => gpu.materialize(),
            None => Vec::new(),
        };
        for (layer, rect, pixels) in tiles {
            self.editor.apply_tiles(
                layer,
                Rect::new(
                    rect.0 as usize,
                    rect.1 as usize,
                    rect.2 as usize,
                    rect.3 as usize,
                ),
                &pixels,
            );
        }
    }

    pub(crate) fn segment(&mut self, from: (f32, f32), to: (f32, f32)) {
        let radius = (self.size / 2.0) as f32;
        let colour = self.ink();
        let time = self.time;
        if self.gpu_stroke {
            self.editor.stroke_at(from, to, radius, 1.0, colour, time);
            self.time = self.time.wrapping_add(1);
            let brush = self.editor.brush;
            let hardness = brush.param(0).unwrap_or(0.8);
            let spacing = brush.param(4).unwrap_or(0.1);
            let roundness = brush.param(7).unwrap_or(1.0);
            let angle = brush.param(6).unwrap_or(0.0);
            if let Some(gpu) = self.gpu.as_mut() {
                gpu.stroke_segment(from, to, radius, hardness, spacing, roundness, angle, 1.0);
            }
            return;
        }
        if self.tool == Tool::Eraser {
            self.editor.erase_at(from, to, radius, 1.0, time);
        } else {
            self.editor.stroke_at(from, to, radius, 1.0, colour, time);
        }
        self.time = self.time.wrapping_add(1);
        let Some(rect) = self.editor.document.take_dirty() else {
            return;
        };
        if !self.gpu_ready {
            return;
        }
        let active = self.editor.document.active;
        let Some(layer) = self.editor.document.layers.get(active) else {
            return;
        };
        let pixels = layer_rgba(layer, rect);
        let Some(gpu) = self.gpu.as_mut() else { return };
        gpu.upload_layer(RectUpload {
            index: active,
            x: rect.x as u32,
            y: rect.y as u32,
            width: rect.width as u32,
            height: rect.height as u32,
            pixels: &pixels,
        });
    }

    pub(crate) fn sync_gpu(&mut self) -> bool {
        self.materialize();
        let document = &self.editor.document;
        let count = document.layers.len();
        for (index, layer) in document.layers.iter().enumerate() {
            if layer.kind == LayerKind::Group
                && (layer.opacity < 0.999 || layer.blend != BlendMode::Normal || layer.clipping)
            {
                return false;
            }
            if layer.parent.is_some() && layer.blend != BlendMode::Normal {
                return false;
            }
            if layer.clipping && document.clip_base(index).is_none() {
                return false;
            }
        }
        let mut specs = Vec::with_capacity(count);
        let mut uploads = Vec::new();
        for (index, layer) in document.layers.iter().enumerate() {
            let mut spec = LayerSpec::new(index);
            spec.opacity = document.effective_opacity(index);
            spec.visible = document.effective_visible(index);
            spec.blend = Blend::from_i32(layer.blend.as_i32());
            spec.fill = (layer.kind == LayerKind::Fill).then_some(layer.fill);
            spec.clip = document.clip_base(index);
            specs.push((index, spec));
            if layer.kind == LayerKind::Paint {
                let rect = Rect::new(0, 0, self.width as usize, self.height as usize);
                uploads.push((index, layer_rgba(layer, rect)));
            }
        }
        let (width, height) = (self.width, self.height);
        let Some(gpu) = self.gpu.as_mut() else {
            return false;
        };
        gpu.reset(count, width, height);
        gpu.set_budget(1536 << 20);
        for (index, pixels) in &uploads {
            gpu.upload_layer(RectUpload {
                index: *index,
                x: 0,
                y: 0,
                width,
                height,
                pixels,
            });
        }
        for (index, spec) in specs {
            gpu.set_layer(index, spec);
        }
        true
    }

    pub(crate) fn compose(&mut self) {
        let expected = self.width as usize * self.height as usize * 4;
        if self.pixels.len() != expected {
            self.pixels.resize(expected, 0);
        }
        if self.gpu_ready {
            let composed = self.gpu.as_mut().map(|gpu| gpu.composite());
            if let Some(composed) = composed
                && composed.len() == self.pixels.len()
            {
                self.pixels.copy_from_slice(&composed);
                return;
            }
            self.gpu_ready = false;
        }
        let rect = Rect::new(0, 0, self.width as usize, self.height as usize);
        let pixels = self.editor.document.composite_rect(rect);
        for (index, pixel) in pixels.iter().enumerate() {
            let alpha = pixel[3].clamp(0.0, 1.0);
            self.pixels[index * 4] = (pixel[0].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
            self.pixels[index * 4 + 1] = (pixel[1].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
            self.pixels[index * 4 + 2] = (pixel[2].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
            self.pixels[index * 4 + 3] = (alpha * 255.0).round() as u8;
        }
    }

    /// Hand the composite to GTK without copying it: the buffer is taken and owned by the texture,
    /// and the paper is laid over it in place.
    pub(crate) fn make_texture(&mut self) -> gtk::gdk::MemoryTexture {
        let mut pixels = std::mem::take(&mut self.pixels);
        pixels.resize(self.width as usize * self.height as usize * 4, 0);
        for pixel in pixels.chunks_exact_mut(4) {
            let paper = 255 - pixel[3];
            pixel[0] = pixel[0].saturating_add(paper);
            pixel[1] = pixel[1].saturating_add(paper);
            pixel[2] = pixel[2].saturating_add(paper);
            pixel[3] = 255;
        }
        let bytes = glib::Bytes::from_owned(pixels);
        gtk::gdk::MemoryTexture::new(
            self.width as i32,
            self.height as i32,
            gtk::gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            self.width as usize * 4,
        )
    }
}

fn layer_rgba(layer: &Layer, rect: Rect) -> Vec<u8> {
    let mut pixels = vec![0; rect.width * rect.height * 4];
    for y in 0..rect.height {
        for x in 0..rect.width {
            let pixel = layer.surface.get(rect.x + x, rect.y + y);
            let index = (y * rect.width + x) * 4;
            for channel in 0..4 {
                pixels[index + channel] = (pixel[channel].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    pixels
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}

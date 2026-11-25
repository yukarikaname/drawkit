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
    gpu: Option<GpuCompositor>,
    pub(crate) gpu_ready: bool,
    pixels: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) size: f64,
    pub(crate) opacity: f64,
    pub(crate) colour: Rgba,
    pub(crate) tool: Tool,
    pub(crate) last: Option<(f32, f32)>,
    pub(crate) texture: Option<gtk::gdk::MemoryTexture>,
    pub(crate) time: u64,
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
            texture: None,
            time: 0,
        }
    }

    pub(crate) fn segment(&mut self, from: (f32, f32), to: (f32, f32)) {
        let radius = (self.size / 2.0) as f32;
        let colour = [
            self.colour[0],
            self.colour[1],
            self.colour[2],
            self.colour[3] * self.opacity as f32,
        ];
        let time = self.time;
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
        let Some(gpu) = self.gpu.as_mut() else { return };
        let active = self.editor.document.active;
        let Some(layer) = self.editor.document.layers.get(active) else {
            return;
        };
        let pixels = layer_rgba(layer, rect);
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
        let Some(gpu) = self.gpu.as_mut() else {
            return false;
        };
        let document = &self.editor.document;
        let count = document.layers.len();
        for (index, layer) in document.layers.iter().enumerate() {
            if layer.kind == LayerKind::Group
                && (layer.opacity < 0.999 || layer.blend != BlendMode::Normal)
            {
                return false;
            }
            if layer.clipping && document.clip_base(index).is_none() {
                return false;
            }
        }
        gpu.reset(count, self.width, self.height);
        gpu.set_budget(1536 << 20);
        for (index, layer) in document.layers.iter().enumerate() {
            let mut spec = LayerSpec::new(index);
            spec.opacity = document.effective_opacity(index);
            spec.visible = document.effective_visible(index);
            spec.blend = Blend::from_i32(layer.blend.as_i32());
            spec.fill = (layer.kind == LayerKind::Fill).then_some(layer.fill);
            spec.clip = document.clip_base(index);
            if layer.kind == LayerKind::Paint {
                let rect = Rect::new(0, 0, self.width as usize, self.height as usize);
                let pixels = layer_rgba(layer, rect);
                gpu.upload_layer(RectUpload {
                    index,
                    x: 0,
                    y: 0,
                    width: self.width,
                    height: self.height,
                    pixels: &pixels,
                });
            }
            gpu.set_layer(index, spec);
        }
        true
    }

    pub(crate) fn compose(&mut self) {
        if self.gpu_ready {
            if let Some(gpu) = self.gpu.as_mut() {
                let composed = gpu.composite();
                if composed.len() == self.pixels.len() {
                    self.pixels.copy_from_slice(&composed);
                    return;
                }
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

    pub(crate) fn make_texture(&self) -> gtk::gdk::MemoryTexture {
        let mut pixels = self.pixels.clone();
        for pixel in pixels.chunks_exact_mut(4) {
            let paper = 255 - pixel[3];
            pixel[0] = pixel[0].saturating_add(paper);
            pixel[1] = pixel[1].saturating_add(paper);
            pixel[2] = pixel[2].saturating_add(paper);
            pixel[3] = 255;
        }
        let bytes = glib::Bytes::from(pixels.as_slice());
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

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}

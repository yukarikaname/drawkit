//! A compositor for shells that present through their own bitmap.
//!
//! macOS and iPad present the wgpu swapchain directly. WinUI's `WriteableBitmap` and GTK's
//! `GdkTexture` are not swapchains, but the expensive half is still the layer stack: upload the
//! dirty rectangle, blend on the GPU, read one premultiplied RGBA image back. This type owns the
//! renderer and the resident layers for exactly that use, with no window and no surface.

use crate::Renderer;
use crate::layers::{LayerSpec, LayerStack, RectUpload, TileJournal};
use crate::surface::StrokePreview;

/// One materialised rectangle: layer, x/y/width/height, straight RGBA8.
pub type MaterializedTile = (usize, (u32, u32, u32, u32), Vec<u8>);

pub struct GpuCompositor {
    renderer: Renderer,
    stack: LayerStack,
    specs: Vec<LayerSpec>,
    width: u32,
    height: u32,
    budget: usize,
    stroke: StrokePreview,
    stroke_smudge: Option<[f32; 2]>,
    stroke_alpha_lock: bool,
    tile_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    journal: TileJournal,
    stale: Vec<(usize, (u32, u32, u32, u32))>,
    rects: Vec<(u32, u32, u32, u32)>,
}

impl GpuCompositor {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Self::new_with_format(width, height, crate::composite::FORMAT)
    }

    /// An offscreen compositor whose tiles and output are half floats.
    pub fn new_hdr(width: u32, height: u32) -> Option<Self> {
        Self::new_with_format(width, height, wgpu::TextureFormat::Rgba16Float)
    }

    fn new_with_format(width: u32, height: u32, format: wgpu::TextureFormat) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let renderer = Renderer::new()?;
        let mut stack = LayerStack::with_format(width, height, format);
        let budget = 1 << 30;
        stack.set_budget(budget);
        let stroke = StrokePreview::new(renderer.device(), renderer.queue());
        let tile_pipeline = crate::layers::commit_pipeline_for(renderer.device(), format);
        let composite_pipeline = crate::composite::pipeline_for(renderer.device(), format);
        Some(Self {
            renderer,
            stack,
            specs: Vec::new(),
            width,
            height,
            budget,
            stroke,
            stroke_smudge: None,
            stroke_alpha_lock: false,
            tile_pipeline,
            composite_pipeline,
            format,
            journal: TileJournal::new(512 << 20),
            stale: Vec::new(),
            rects: Vec::new(),
        })
    }

    pub fn is_linear(&self) -> bool {
        self.format != crate::composite::FORMAT
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Start a new layer stack. The byte budget survives the reset.
    pub fn reset(&mut self, count: usize, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        self.stack = LayerStack::with_format(width, height, self.format);
        self.stack.set_budget(self.budget);
        self.specs = (0..count).map(LayerSpec::new).collect();
    }

    pub fn set_budget(&mut self, bytes: usize) {
        self.budget = bytes.max(1 << 20);
        self.stack.set_budget(self.budget);
    }

    pub fn upload_layer(&mut self, request: RectUpload<'_>) -> bool {
        self.stack
            .update_rect(self.renderer.device(), self.renderer.queue(), request)
    }

    /// Upload a full layer as straight linear floats, for a half-float compositor.
    pub fn upload_layer_linear(&mut self, index: usize, pixels: &[f32]) -> bool {
        self.stack
            .set_linear(self.renderer.device(), self.renderer.queue(), index, pixels)
    }

    pub fn set_layer_parent(&mut self, index: usize, parent: Option<usize>) {
        while self.specs.len() <= index {
            let layer = self.specs.len();
            self.specs.push(LayerSpec::new(layer));
        }
        self.specs[index].parent = parent;
        self.stack.mark_all_dirty();
    }

    pub fn set_layer_clip(&mut self, index: usize, clip: Option<usize>) {
        while self.specs.len() <= index {
            let layer = self.specs.len();
            self.specs.push(LayerSpec::new(layer));
        }
        self.specs[index].clip = clip;
        self.stack.mark_all_dirty();
    }

    pub fn set_layer(&mut self, index: usize, spec: LayerSpec) {
        while self.specs.len() <= index {
            let layer = self.specs.len();
            self.specs.push(LayerSpec::new(layer));
        }
        self.specs[index] = spec;
        self.stack.mark_all_dirty();
    }

    /// Composite the resident layers into straight, premultiplied RGBA8, with the live stroke
    /// overlaid while the pen is down.
    /// Blend a fill mask into a resident layer. The mask is one float per canvas pixel.
    pub fn apply_fill(
        &mut self,
        layer: usize,
        mask: &[f32],
        color: [f32; 4],
        alpha_locked: bool,
    ) -> bool {
        if self.is_linear() {
            self.stack
                .apply_fill_mask_linear(layer, mask, color, alpha_locked)
        } else {
            self.stack.apply_fill_mask(layer, mask, color, alpha_locked)
        }
    }

    pub fn composite(&mut self) -> Vec<u8> {
        let mut pixels = self.renderer.composite_stack(&mut self.stack, &self.specs);
        self.overlay_stroke(&mut pixels);
        pixels
    }

    /// The composed frame as premultiplied linear Display P3 floats.
    pub fn composite_linear(&mut self) -> Vec<f32> {
        let Some(texture) = self.stack.composite_texture(
            self.renderer.device(),
            self.renderer.queue(),
            &self.composite_pipeline,
            &self.specs,
        ) else {
            return vec![0.0; self.width as usize * self.height as usize * 4];
        };
        crate::readback::read_texture_linear(
            self.renderer.device(),
            self.renderer.queue(),
            &texture,
            self.width,
            self.height,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stroke_begin(
        &mut self,
        width: u32,
        height: u32,
        color: [f32; 4],
        flow: f32,
        pressure_alpha: f32,
        paper: f32,
        tip: f32,
        dual: [f32; 7],
        wet: f32,
        jitter: f32,
        grain: f32,
        dual_jitter: f32,
        dual_grain: f32,
        seed: u64,
        erase: bool,
        preview_layer: u32,
    ) {
        self.stroke.begin(
            width,
            height,
            color,
            flow,
            pressure_alpha,
            paper,
            tip,
            dual,
            wet,
            jitter,
            grain,
            dual_jitter,
            dual_grain,
            seed,
            erase,
            preview_layer,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stroke_segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        radius: f32,
        hardness: f32,
        spacing: f32,
        roundness: f32,
        angle: f32,
        pressure: f32,
    ) {
        self.stroke.segment(
            from, to, radius, hardness, spacing, roundness, angle, pressure,
        );
    }

    pub fn stroke_end(&mut self) {
        self.stroke.end();
    }

    /// Follow the layer's alpha for the next commit.
    pub fn set_stroke_alpha_lock(&mut self, locked: bool) {
        self.stroke_alpha_lock = locked;
    }

    /// Make the next commit a smudge with this pixel offset.
    pub fn set_stroke_smudge(&mut self, offset: [f32; 2]) {
        self.stroke_smudge = Some(offset);
    }

    /// Blend the accumulated coverage over the composited frame. Premultiplied in, premultiplied
    /// out, which is what both the GTK texture and the WinUI bitmap want.
    pub fn overlay_stroke(&mut self, pixels: &mut [u8]) {
        let Some(((x0, y0, width, height), coverage)) = self.stroke.read() else {
            return;
        };
        let Some(color) = self.stroke.color() else {
            return;
        };
        let frame = self.width as usize;
        for row in 0..height as usize {
            let y = y0 as usize + row;
            if y >= self.height as usize {
                break;
            }
            for column in 0..width as usize {
                let x = x0 as usize + column;
                if x >= frame || x >= self.width as usize {
                    break;
                }
                let amount = coverage[row * width as usize + column].clamp(0.0, 1.0);
                if amount <= 0.0 {
                    continue;
                }
                let at = (y * frame + x) * 4;
                for channel in 0..3 {
                    let base = f32::from(pixels[at + channel]);
                    let top = color[channel].clamp(0.0, 1.0) * amount * 255.0;
                    pixels[at + channel] = (top + base * (1.0 - amount)).round() as u8;
                }
                let base = f32::from(pixels[at + 3]);
                pixels[at + 3] = (amount * 255.0 + base * (1.0 - amount)).round() as u8;
            }
        }
    }

    /// Composite the live coverage into layer `index` and keep the replaced tiles for undo.
    pub fn commit_stroke(&mut self, index: usize, color: [f32; 3], alpha: f32) -> usize {
        self.rects.clear();
        let Some((x0, y0, x1, y1)) = self.stroke.dirty() else {
            return 0;
        };
        let Some(coverage) = self.stroke.coverage().cloned() else {
            return 0;
        };
        let rect = (x0, y0, x1 - x0 + 1, y1 - y0 + 1);
        let mode = if self.stroke_smudge.is_some() {
            3
        } else if self.stroke.is_erase() {
            1
        } else if self.stroke_alpha_lock {
            2
        } else {
            0
        };
        let smudge = self.stroke_smudge.take().unwrap_or([0.0; 2]);
        self.stroke_alpha_lock = false;
        let (rects, swaps) = self.stack.commit_stroke(
            self.renderer.device(),
            self.renderer.queue(),
            &self.tile_pipeline,
            index,
            &coverage,
            rect,
            color,
            alpha,
            self.stroke.wet(),
            mode,
            smudge,
        );
        self.rects = rects.clone();
        if swaps.is_empty() {
            return 0;
        }
        let bytes = rects.len() * self.stack.tile_bytes_hint();
        self.journal.push(index, rects, swaps, bytes);
        for rect in &self.rects {
            self.stale.push((index, *rect));
        }
        self.rects.len()
    }

    /// Drop the undo journal and the readback list. A replay driver does not undo, and keeping
    /// every replaced tile would double the memory a long recording needs.
    pub fn clear_history(&mut self) {
        self.journal.clear();
        self.stale.clear();
    }

    pub fn committed_rect(&self, index: usize) -> Option<(u32, u32, u32, u32)> {
        self.rects.get(index).copied()
    }

    pub fn undo_tiles(&mut self) -> bool {
        let Some(mut step) = self.journal.begin_undo() else {
            return false;
        };
        for swap in step.swaps.drain(..) {
            step.undone.push(self.stack.restore_swap(swap));
        }
        for rect in &step.rects {
            self.stale.push((step.layer, *rect));
        }
        self.journal.finish_undo(step);
        true
    }

    pub fn redo_tiles(&mut self) -> bool {
        let Some(mut step) = self.journal.begin_redo() else {
            return false;
        };
        for swap in step.undone.drain(..) {
            step.swaps.push(self.stack.restore_swap(swap));
        }
        for rect in &step.rects {
            self.stale.push((step.layer, *rect));
        }
        self.journal.finish_redo(step);
        true
    }

    pub fn has_stale(&self) -> bool {
        !self.stale.is_empty()
    }

    /// Read the rectangles the GPU painted since the last call, for the CPU cache.
    pub fn materialize(&mut self) -> Vec<MaterializedTile> {
        let stale = std::mem::take(&mut self.stale);
        let mut out = Vec::new();
        for (layer, rect) in stale {
            if self.is_linear() {
                self.stack.read_layer_rect_linear(
                    self.renderer.device(),
                    self.renderer.queue(),
                    layer,
                    rect,
                );
                continue;
            }
            if let Some(tile) = self.stack.read_layer_rect(
                self.renderer.device(),
                self.renderer.queue(),
                layer,
                rect,
            ) {
                out.push((
                    layer,
                    (tile.x, tile.y, tile.width, tile.height),
                    tile.pixels,
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;

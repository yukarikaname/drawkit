//! A compositor for shells that present through their own bitmap.
//!
//! macOS and iPad present the wgpu swapchain directly. WinUI's `WriteableBitmap` and GTK's
//! `GdkTexture` are not swapchains, but the expensive half is still the layer stack: upload the
//! dirty rectangle, blend on the GPU, read one premultiplied RGBA image back. This type owns the
//! renderer and the resident layers for exactly that use, with no window and no surface.

use crate::Renderer;
use crate::layers::{LayerSpec, LayerStack, RectUpload, TileSwap};
use crate::surface::StrokePreview;

/// How many replaced tile textures the undo journal keeps for a bitmap shell.
const JOURNAL_BUDGET: usize = 512 << 20;

struct JournalStep {
    layer: usize,
    rects: Vec<(u32, u32, u32, u32)>,
    swaps: Vec<TileSwap>,
    undone: Vec<TileSwap>,
    bytes: usize,
}

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
    tile_pipeline: wgpu::RenderPipeline,
    undo: Vec<JournalStep>,
    redo: Vec<JournalStep>,
    journal_bytes: usize,
    stale: Vec<(usize, (u32, u32, u32, u32))>,
    rects: Vec<(u32, u32, u32, u32)>,
}

impl GpuCompositor {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let renderer = Renderer::new()?;
        let mut stack = LayerStack::new(width, height);
        let budget = 1 << 30;
        stack.set_budget(budget);
        let stroke = StrokePreview::new(renderer.device(), renderer.queue());
        let tile_pipeline = crate::layers::commit_pipeline(renderer.device());
        Some(Self {
            renderer,
            stack,
            specs: Vec::new(),
            width,
            height,
            budget,
            stroke,
            tile_pipeline,
            undo: Vec::new(),
            redo: Vec::new(),
            journal_bytes: 0,
            stale: Vec::new(),
            rects: Vec::new(),
        })
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
        self.stack = LayerStack::new(width, height);
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
    pub fn composite(&mut self) -> Vec<u8> {
        let mut pixels = self.renderer.composite_stack(&mut self.stack, &self.specs);
        self.overlay_stroke(&mut pixels);
        pixels
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
            if self.stroke.is_erase() { 1 } else { 0 },
        );
        self.rects = rects.clone();
        if swaps.is_empty() {
            return 0;
        }
        let bytes = rects.len() * self.stack.tile_bytes_hint();
        self.undo.push(JournalStep {
            layer: index,
            rects,
            swaps,
            undone: Vec::new(),
            bytes,
        });
        self.redo.clear();
        self.journal_bytes += bytes;
        while self.journal_bytes > JOURNAL_BUDGET && self.undo.len() > 1 {
            let dropped = self.undo.remove(0);
            self.journal_bytes -= dropped.bytes;
        }
        for rect in &self.rects {
            self.stale.push((index, *rect));
        }
        self.rects.len()
    }

    /// Drop the undo journal and the readback list. A replay driver does not undo, and keeping
    /// every replaced tile would double the memory a long recording needs.
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.journal_bytes = 0;
        self.stale.clear();
    }

    pub fn committed_rect(&self, index: usize) -> Option<(u32, u32, u32, u32)> {
        self.rects.get(index).copied()
    }

    pub fn undo_tiles(&mut self) -> bool {
        let Some(mut step) = self.undo.pop() else {
            return false;
        };
        for swap in step.swaps.drain(..) {
            step.undone.push(self.stack.restore_swap(swap));
        }
        self.journal_bytes -= step.bytes;
        for rect in &step.rects {
            self.stale.push((step.layer, *rect));
        }
        self.redo.push(step);
        true
    }

    pub fn redo_tiles(&mut self) -> bool {
        let Some(mut step) = self.redo.pop() else {
            return false;
        };
        for swap in step.undone.drain(..) {
            step.swaps.push(self.stack.restore_swap(swap));
        }
        self.journal_bytes += step.bytes;
        for rect in &step.rects {
            self.stale.push((step.layer, *rect));
        }
        self.undo.push(step);
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
mod tests {
    use super::*;

    #[test]
    fn a_headless_compositor_blends_two_layers() {
        let Some(mut compositor) = GpuCompositor::new(2, 2) else {
            return;
        };
        compositor.reset(2, 2, 2);
        let red = [255, 0, 0, 255].repeat(4);
        let blue = [0, 0, 255, 128].repeat(4);
        assert!(compositor.upload_layer(RectUpload {
            index: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            pixels: &red,
        }));
        assert!(compositor.upload_layer(RectUpload {
            index: 1,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            pixels: &blue,
        }));
        compositor.set_layer(0, LayerSpec::new(0));
        compositor.set_layer(1, LayerSpec::new(1));
        let out = compositor.composite();
        assert!((out[0] as i32 - 127).abs() <= 2, "red {}", out[0]);
        assert!((out[2] as i32 - 128).abs() <= 2, "blue {}", out[2]);
    }
}

#[test]
fn a_replayed_stroke_commits_into_a_resident_tile() {
    let Some(mut compositor) = GpuCompositor::new(16, 16) else {
        return;
    };
    compositor.reset(1, 16, 16);
    compositor.stroke_begin(
        16,
        16,
        [1.0, 0.0, 0.0, 1.0],
        1.0,
        0.0,
        0.0,
        0.0,
        [0.0; 7],
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        7,
        false,
        u32::MAX,
    );
    compositor.stroke_segment((2.0, 8.0), (14.0, 8.0), 4.0, 1.0, 0.1, 1.0, 0.0, 1.0);
    assert!(compositor.commit_stroke(0, [1.0, 0.0, 0.0], 1.0) > 0);
    compositor.stroke_end();
    let pixels = compositor.composite();
    let centre = (8 * 16 + 8) * 4;
    assert!(pixels[centre] > 200, "red {}", pixels[centre]);
    assert!(pixels[centre + 3] > 200, "alpha {}", pixels[centre + 3]);
}

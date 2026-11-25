//! A compositor for shells that present through their own bitmap.
//!
//! macOS and iPad present the wgpu swapchain directly. WinUI's `WriteableBitmap` and GTK's
//! `GdkTexture` are not swapchains, but the expensive half is still the layer stack: upload the
//! dirty rectangle, blend on the GPU, read one premultiplied RGBA image back. This type owns the
//! renderer and the resident layers for exactly that use, with no window and no surface.

use crate::Renderer;
use crate::layers::{LayerSpec, LayerStack, RectUpload};

pub struct GpuCompositor {
    renderer: Renderer,
    stack: LayerStack,
    specs: Vec<LayerSpec>,
    width: u32,
    height: u32,
    budget: usize,
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
        Some(Self {
            renderer,
            stack,
            specs: Vec::new(),
            width,
            height,
            budget,
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

    pub fn set_layer(&mut self, index: usize, spec: LayerSpec) {
        while self.specs.len() <= index {
            let layer = self.specs.len();
            self.specs.push(LayerSpec::new(layer));
        }
        self.specs[index] = spec;
        self.stack.mark_all_dirty();
    }

    /// Composite the resident layers into straight, premultiplied RGBA8.
    pub fn composite(&mut self) -> Vec<u8> {
        self.renderer.composite_stack(&mut self.stack, &self.specs)
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

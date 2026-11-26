//! Constructing a compositor, either on its own device or on one the canvas surface owns.

use super::*;
use crate::Renderer;

impl GpuCompositor {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Self::new_with_format(width, height, crate::FrameFormat::Rgba8)
    }

    /// An offscreen compositor whose tiles and output are half floats.
    pub fn new_hdr(width: u32, height: u32) -> Option<Self> {
        Self::new_with_format(width, height, crate::FrameFormat::Rgba16)
    }

    fn new_with_format(width: u32, height: u32, format: crate::FrameFormat) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let renderer = Renderer::new()?;
        Self::with_renderer(renderer, width, height, format)
    }

    /// An offscreen compositor on an existing device, so its finished texture can be presented by
    /// the canvas surface without a readback.
    pub fn new_with_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
        format: crate::FrameFormat,
    ) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let renderer = Renderer::from_device(device, queue)?;
        Self::with_renderer(renderer, width, height, format)
    }

    /// The composed frame as a texture on the shared device.
    pub fn output_texture(&mut self) -> Option<wgpu::Texture> {
        self.stack.composite_texture(
            self.renderer.device(),
            self.renderer.queue(),
            &self.composite_pipeline,
            &self.specs,
        )
    }

    fn with_renderer(
        renderer: Renderer,
        width: u32,
        height: u32,
        format: crate::FrameFormat,
    ) -> Option<Self> {
        let mut stack = LayerStack::with_format(width, height, format);
        let budget = 1 << 30;
        stack.set_budget(budget);
        let stroke = StrokePreview::new(renderer.device(), renderer.queue());
        let tile_pipeline = crate::layers::commit_pipeline_for(renderer.device(), format.texture());
        let composite_pipeline =
            crate::composite::pipeline_for(renderer.device(), format.texture());
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
        self.format != crate::FrameFormat::Rgba8
    }
}

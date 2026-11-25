use super::*;

impl CanvasSurface {
    pub fn hdr(&self) -> bool {
        self.hdr
    }

    /// Display exposure in stops. It changes the preview, not the document.
    pub fn set_exposure(&mut self, exposure: f32) {
        self.exposure = exposure.clamp(-8.0, 8.0);
    }

    /// The resident-layer path runs in both ranges: eight-bit stacks compose RGBA8 textures and
    /// an extended-range surface composes premultiplied half floats.
    pub fn supports_layers(&self) -> bool {
        true
    }

    /// Whether the stroke being committed follows the active layer's alpha. The shell sets this
    /// before a commit and the flag resets when the commit is taken.
    pub fn set_stroke_alpha_lock(&mut self, locked: bool) {
        self.stroke_alpha_lock = locked;
    }

    /// Make the next commit a smudge that samples the tile at `offset` pixels.
    pub fn set_stroke_smudge(&mut self, offset: [f32; 2]) {
        self.stroke_smudge = Some(offset);
    }

    /// Whether the resident texture for a layer was evicted by the memory budget.
    pub fn layer_missing(&self, index: usize) -> bool {
        self.layers.layer_missing(index)
    }

    /// Keep the resident layer textures under this byte budget, evicting the coldest layers.
    pub fn set_layer_budget(&mut self, bytes: u64) {
        self.layers.set_budget(bytes as usize);
    }

    /// Start a new layer stack at the canvas size. The shell uploads every layer afterwards.
    pub fn reset_layers(&mut self, count: usize, width: u32, height: u32) {
        let budget = self.layers.budget();
        let format = if self.hdr {
            wgpu::TextureFormat::Rgba16Float
        } else {
            crate::composite::FORMAT
        };
        self.layers = crate::LayerStack::with_format(width, height, format);
        self.layers.set_budget(budget);
        self.layer_specs = (0..count).map(crate::LayerSpec::new).collect();
    }
}

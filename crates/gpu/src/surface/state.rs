use super::*;

impl CanvasSurface {
    pub fn hdr(&self) -> bool {
        self.hdr
    }

    /// Display exposure in stops. It changes the preview, not the document.
    pub fn set_exposure(&mut self, exposure: f32) {
        self.exposure = exposure.clamp(-8.0, 8.0);
    }

    /// Whether the resident-layer path can run: only the eight-bit painting path composes RGBA8
    /// layer textures; HDR still uses the CPU composite until its textures are half floats.
    pub fn supports_layers(&self) -> bool {
        !self.hdr
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
        self.layers = crate::LayerStack::new(width, height);
        self.layers.set_budget(budget);
        self.layer_specs = (0..count).map(crate::LayerSpec::new).collect();
    }
}

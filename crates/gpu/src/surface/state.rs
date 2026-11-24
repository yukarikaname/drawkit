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

    /// Start a new layer stack at the canvas size. The shell uploads every layer afterwards.
    pub fn reset_layers(&mut self, count: usize, width: u32, height: u32) {
        self.layers = crate::LayerStack::new(width, height);
        self.layer_specs = (0..count).map(crate::LayerSpec::new).collect();
    }
}

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

    /// Queue stroke segments instead of submitting one compute pass per sample, and flush once a
    /// frame: the coverage is the same, the submits are not.
    pub fn set_stroke_batch(&mut self, on: bool) {
        self.stroke.set_batching(on);
    }

    /// Submit whatever the batch has queued. The commit flushes too, so a shell that only ever
    /// calls this on a frame is correct.
    pub fn flush_stroke(&mut self) {
        self.stroke.flush();
    }

    /// Rotate the presented paper around its centre. The canvas texture and the paper rectangle
    /// stay axis-aligned; only the shader swizzles the sample position.
    pub fn set_rotation(&mut self, radians: f32) {
        self.rotation = radians;
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

    /// Whether the shell still has to upload this layer at all. Individual tiles evicted from a
    /// known layer are reported by `missing_rects` instead.
    pub fn layer_missing(&self, index: usize) -> bool {
        self.layers.layer_missing(index)
    }

    /// Tiles the memory budget evicted; the shell re-uploads them from the core surface before
    /// the next present. Empty for the offscreen compositor, which keeps backups.
    pub fn missing_rects(&self) -> Vec<(usize, u32, u32, u32, u32)> {
        self.layers.missing_rects()
    }

    /// Keep the resident layer textures under this byte budget, evicting the coldest layers.
    pub fn set_layer_budget(&mut self, bytes: u64) {
        self.layers.set_budget(bytes as usize);
    }

    /// Cap the bytes the GPU undo journal keeps. `trim` drops the oldest steps, as it does on a
    /// commit that crosses the budget.
    pub fn set_journal_budget(&mut self, bytes: u64) {
        self.journal.set_budget(bytes as usize);
        self.sync_pins();
    }

    /// Start a new layer stack at the canvas size. The shell uploads every layer afterwards.
    pub fn reset_layers(&mut self, count: usize, width: u32, height: u32) {
        let budget = self.layers.budget();
        let format = if self.hdr {
            crate::FrameFormat::Rgba16
        } else {
            crate::FrameFormat::Rgba8
        };
        // The live canvas must not hold a CPU copy: the core surface is the source of truth,
        // and `missing_rects` tells the shell what to upload again.
        self.layers = crate::LayerStack::without_backups(width, height, format);
        self.layers.set_budget(budget);
        self.layer_specs = (0..count).map(crate::LayerSpec::new).collect();
    }
}

//! Preview compositing that keeps the live stroke at its layer's place in the stack.
//!
//! The paper shader used to mix the stroke over the finished frame, which is only right for the
//! topmost, fully opaque, normal-blend layer. This composes the stroke into a scratch copy of the
//! active layer's tile instead, then lets the regular stack walk apply the layer's opacity, blend,
//! clip and group, so a middle layer or a translucent one previews what the commit will paint.

use super::*;

impl CanvasSurface {
    /// Composite the stack with the live stroke, or `None` when no stroke is previewing, the layer
    /// has no coverage yet, or the surface cannot address it.
    pub(super) fn preview_texture(&mut self) -> Option<wgpu::Texture> {
        let layer = self.stroke.preview_layer()?;
        let color = self.stroke.color()?;
        let coverage = self.stroke.coverage()?;
        let rect = self.stroke.dirty()?;
        if layer >= self.layer_specs.len() {
            return None;
        }
        let mode = if self.stroke_smudge.is_some() {
            3
        } else if self.stroke.is_erase() {
            1
        } else if self.stroke_alpha_lock {
            2
        } else {
            0
        };
        // A smudge commits with full coverage; a paint stroke commits with the brush flow, which
        // the coverage buffer has not applied yet.
        let alpha = if mode == 3 {
            1.0
        } else {
            self.stroke.commit_alpha()
        };
        let rgba8 = self.layers.format() == crate::FrameFormat::Rgba8;
        let (pipeline, tile_pipeline) = if rgba8 {
            (&self.composite_pipeline, &self.tile_pipeline)
        } else {
            (&self.composite_hdr_pipeline, &self.tile_hdr_pipeline)
        };
        self.layers.composite_stroke_preview(
            &self.device,
            &self.queue,
            pipeline,
            &self.layer_specs,
            layer,
            rect,
            coverage,
            tile_pipeline,
            [color[0], color[1], color[2], alpha],
            self.stroke.wet(),
            mode,
            self.stroke_smudge.unwrap_or([0.0; 2]),
            self.texture_region,
        )
    }
}

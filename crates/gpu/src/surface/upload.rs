//! Uploading layer rectangles into the resident stack.

use super::*;

impl CanvasSurface {
    /// Upload one layer rectangle as straight eight-bit pixels.
    pub fn upload_layer_rect(
        &mut self,
        index: usize,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> bool {
        self.layers.update_rect(
            &self.device,
            &self.queue,
            crate::layers::RectUpload {
                index,
                x,
                y,
                width,
                height,
                pixels,
            },
        )
    }

    /// The same rectangle as straight linear Display P3 floats, for an extended-range stack.
    pub fn upload_layer_linear(
        &mut self,
        index: usize,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        pixels: &[f32],
    ) -> bool {
        self.layers.update_rect_linear(
            &self.device,
            &self.queue,
            crate::layers::LinearUpload {
                index,
                x,
                y,
                width,
                height,
                pixels,
            },
        )
    }
}

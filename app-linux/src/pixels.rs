// CPU pixels for the layer store. The GPU owns the frame; this is only the source for an upload
// (a new layer, a reopened document or the CPU eraser) and never the presentation path.

use drawkit_core::document::Layer;
use drawkit_core::surface::Rect;

/// Straight RGBA8 for one rectangle of a layer. The layer stores f32 in the working space.
pub(crate) fn layer_rgba(layer: &Layer, rect: Rect) -> Vec<u8> {
    let mut pixels = vec![0; rect.width * rect.height * 4];
    for y in 0..rect.height {
        for x in 0..rect.width {
            let pixel = layer.surface.get(rect.x + x, rect.y + y);
            let index = (y * rect.width + x) * 4;
            for channel in 0..4 {
                pixels[index + channel] = (pixel[channel].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    pixels
}

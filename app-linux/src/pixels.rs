// CPU pixels for the layer store. The GPU owns the frame; this is only the source for an upload
// (a new layer, a reopened document or the CPU eraser) and never the presentation path.

use drawkit_core::document::{ColorSpace, Layer, LayerKind};
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

/// Straight linear Display P3 floats for one rectangle, the form an extended-range stack
/// composites: highlights above paper white survive instead of clipping at one.
pub(crate) fn layer_linear(layer: &Layer, rect: Rect, space: ColorSpace) -> Vec<f32> {
    let matrix = space.to_display_p3();
    let mut pixels = vec![0.0f32; rect.width * rect.height * 4];
    for y in 0..rect.height {
        for x in 0..rect.width {
            let pixel = match layer.kind {
                LayerKind::Fill => layer.fill,
                LayerKind::Paint => layer.surface.get(rect.x + x, rect.y + y),
                LayerKind::Group => [0.0; 4],
            };
            let (r, g, b) = (space.decode(pixel[0]), space.decode(pixel[1]), space.decode(pixel[2]));
            let at = (y * rect.width + x) * 4;
            pixels[at] = (matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b).max(0.0);
            pixels[at + 1] = (matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b).max(0.0);
            pixels[at + 2] = (matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b).max(0.0);
            pixels[at + 3] = pixel[3].clamp(0.0, 1.0);
        }
    }
    pixels
}

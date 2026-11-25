use super::*;

impl Editor {
    /// The whole painting, resampled for a navigator or another small overview. Opaque RGBA8 with
    /// the paper behind it; the canvas itself stays transparent.
    pub fn overview(&self, width: usize, height: usize) -> Vec<u8> {
        if width == 0 || height == 0 {
            return Vec::new();
        }
        let pixels = self.document.composite_scaled(width, height);
        let mut out = Vec::with_capacity(pixels.len() * 4);
        for pixel in pixels {
            let shown = over(pixel, PAPER);
            for channel in shown {
                out.push((channel.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        out
    }
}

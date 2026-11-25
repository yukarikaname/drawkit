use super::*;

impl Editor {
    /// Extract the visible painting into a black line layer. The model runs locally; the result
    /// is one layer insertion step and a raster cache, not a tape command.
    pub fn extract_line_art(&mut self, options: drawkit_lineart::LineArtOptions) -> Option<usize> {
        let (width, height) = (self.document.width(), self.document.height());
        if width == 0 || height == 0 {
            return None;
        }
        let rect = Rect::new(0, 0, width, height);
        let composite = self.document.composite_rect(rect);
        let mut rgba = Vec::with_capacity(width * height * 4);
        for pixel in composite {
            let alpha = pixel[3].clamp(0.0, 1.0);
            for value in pixel.iter().take(3) {
                let over_paper = value.clamp(0.0, 1.0) * alpha + (1.0 - alpha);
                rgba.push((over_paper * 255.0).round() as u8);
            }
            rgba.push(255);
        }
        let lines = drawkit_lineart::extract(&rgba, width, height, options)?;
        if !lines.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 8) {
            return None;
        }
        let before = self.document.active;
        let index = self.document.add_layer("Line art");
        for y in 0..height {
            for x in 0..width {
                let alpha = lines[(y * width + x) * 4 + 3] as f32 / 255.0;
                self.document.layers[index]
                    .surface
                    .set(x, y, [0.0, 0.0, 0.0, alpha]);
            }
        }
        self.document.layers[index].surface.mark(rect);
        let layer = self.document.layers[index].clone();
        self.record_layer_insertion(index, layer, before, self.document.active);
        Some(index)
    }
}

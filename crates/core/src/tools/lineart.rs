use super::*;

pub mod clean;

pub use clean::{CleanOptions, clean_mask};

impl Editor {
    /// Extract the visible painting into a black line layer with no cleanup. Kept as the short entry
    /// point for shells that only expose threshold and softness.
    pub fn extract_line_art(&mut self, options: drawkit_lineart::LineArtOptions) -> Option<usize> {
        self.extract_line_art_clean(options, CleanOptions::disabled())
    }

    /// Extract the visible painting and run the deterministic line cleanup before anything is
    /// written. The result is still one layer insertion and one undo step: the cleanup changes the
    /// mask in memory, never the tape.
    pub fn extract_line_art_clean(
        &mut self,
        options: drawkit_lineart::LineArtOptions,
        clean: CleanOptions,
    ) -> Option<usize> {
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
        let mut mask: Vec<f32> = lines
            .as_chunks::<4>()
            .0
            .iter()
            .take(width * height)
            .map(|pixel| pixel[3] as f32 / 255.0)
            .collect();
        clean_mask(&mut mask, width, height, clean);
        if !mask.iter().any(|alpha| *alpha > 8.0 / 255.0) {
            return None;
        }
        let before = self.document.active;
        let index = self.document.add_layer("Line art");
        for y in 0..height {
            for x in 0..width {
                let alpha = mask[y * width + x];
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

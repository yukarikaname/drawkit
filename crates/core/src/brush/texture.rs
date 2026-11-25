//! A grayscale texture map for paper or a brush tip.
//!
//! The map is small (at most 128×128) so it can live beside the brush table in a project. Sampling
//! wraps, which lets a small paper tile cover a large canvas.

#[derive(Clone, Debug)]
pub struct BrushTexture {
    pub width: usize,
    pub height: usize,
    /// Row-major luminance, 0 is dark, 255 is clear.
    pub data: Vec<u8>,
}

impl BrushTexture {
    pub fn new(width: usize, height: usize, data: Vec<u8>) -> Option<Self> {
        if width == 0 || height == 0 || data.len() < width * height {
            return None;
        }
        Some(Self {
            width,
            height,
            data,
        })
    }

    /// Convert an imported RGBA image into a small grayscale tile.
    pub fn from_rgba(width: usize, height: usize, rgba: &[u8], max: usize) -> Option<Self> {
        if width == 0 || height == 0 || rgba.len() < width * height * 4 {
            return None;
        }
        let scale = (width.max(height) as f32 / max as f32).max(1.0);
        let out_width = ((width as f32 / scale).round() as usize).max(1).min(max);
        let out_height = ((height as f32 / scale).round() as usize).max(1).min(max);
        let mut data = Vec::with_capacity(out_width * out_height);
        for y in 0..out_height {
            let sy = (y * height / out_height).min(height - 1);
            for x in 0..out_width {
                let sx = (x * width / out_width).min(width - 1);
                let at = (sy * width + sx) * 4;
                let alpha = rgba[at + 3] as f32 / 255.0;
                let value = 0.299 * rgba[at] as f32
                    + 0.587 * rgba[at + 1] as f32
                    + 0.114 * rgba[at + 2] as f32;
                data.push((value * alpha).round().clamp(0.0, 255.0) as u8);
            }
        }
        Self::new(out_width, out_height, data)
    }

    /// Bilinear sample with wrapped coordinates in 0...1.
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        let x = u.rem_euclid(1.0) * self.width as f32 - 0.5;
        let y = v.rem_euclid(1.0) * self.height as f32 - 0.5;
        let x0 = x.floor() as isize;
        let y0 = y.floor() as isize;
        let tx = x - x0 as f32;
        let ty = y - y0 as f32;
        let wrap = |value: isize, size: usize| value.rem_euclid(size as isize) as usize;
        let at = |x: isize, y: isize| {
            self.data[wrap(y, self.height) * self.width + wrap(x, self.width)] as f32 / 255.0
        };
        let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1, y0) * tx;
        let bottom = at(x0, y0 + 1) * (1.0 - tx) + at(x0 + 1, y0 + 1) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

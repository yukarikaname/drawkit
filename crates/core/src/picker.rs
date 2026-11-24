//! Colour-picker images, in straight colour but premultiplied bytes.
//!
//! A painting app needs a hue wheel and an SV square, not a grid of system swatches. The generation
//! lives in the core because every shell needs the same image: the macOS view draws it, a future
//! shell view can draw it, and the tests can check its corners without a window.

use std::f32::consts::TAU;

use crate::color::Rgba;

/// Hue, saturation, value and alpha, the painter-facing colour model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
    pub a: f32,
}

impl Default for Hsv {
    fn default() -> Self {
        Self {
            h: 0.0,
            s: 0.0,
            v: 0.08,
            a: 1.0,
        }
    }
}

impl Hsv {
    pub fn rgba(self) -> Rgba {
        let [r, g, b] = hsv_to_rgb(self.h, self.s, self.v);
        [r, g, b, self.a]
    }

    pub fn hex(self) -> String {
        let [r, g, b] = hsv_to_rgb(self.h, self.s, self.v);
        format!(
            "#{:02X}{:02X}{:02X}",
            (r * 255.0) as u8,
            (g * 255.0) as u8,
            (b * 255.0) as u8
        )
    }
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let hue = h.rem_euclid(1.0) * 6.0;
    let c = v * s.clamp(0.0, 1.0);
    let x = c * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// A hue wheel at full saturation and value. Bytes are premultiplied RGBA8.
pub fn wheel_rgba(size: usize) -> Vec<u8> {
    let mut bytes = vec![0_u8; size * size * 4];
    if size < 4 {
        return bytes;
    }
    let centre = size as f32 / 2.0;
    let radius = centre - 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5 - centre) / radius;
            let dy = (y as f32 + 0.5 - centre) / radius;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > 1.0 {
                continue;
            }
            let hue = (dy.atan2(dx) / TAU + 1.0) % 1.0;
            let [r, g, b] = hsv_to_rgb(hue, 1.0, 1.0);
            let alpha = if distance > 0.97 {
                (1.0 - distance) / 0.03
            } else {
                1.0
            };
            let index = (y * size + x) * 4;
            bytes[index] = (r * alpha * 255.0).round() as u8;
            bytes[index + 1] = (g * alpha * 255.0).round() as u8;
            bytes[index + 2] = (b * alpha * 255.0).round() as u8;
            bytes[index + 3] = (alpha * 255.0).round() as u8;
        }
    }
    bytes
}

/// A saturation/value square for one hue. x is saturation, bottom is black.
pub fn sv_rgba(hue: f32, width: usize, height: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(width * height * 4);
    let rows = height.saturating_sub(1).max(1) as f32;
    let columns = width.saturating_sub(1).max(1) as f32;
    for y in 0..height {
        let value = 1.0 - y as f32 / rows;
        for x in 0..width {
            let saturation = x as f32 / columns;
            let [r, g, b] = hsv_to_rgb(hue, saturation, value);
            bytes.extend_from_slice(&[
                (r * 255.0).round() as u8,
                (g * 255.0).round() as u8,
                (b * 255.0).round() as u8,
                255,
            ]);
        }
    }
    bytes
}

/// The dominant painted colours, ignoring paper and transparent pixels.
///
/// One pass over a strided sample is enough for a palette; the painter wants the colours they
/// actually used, not a histogram of every anti-aliased edge. Colours are grouped into a 5-bit cube
/// and returned by frequency.
pub fn palette(image: &[Rgba], width: usize, height: usize, paper: Rgba, max: usize) -> Vec<Rgba> {
    if width == 0 || height == 0 || max == 0 {
        return Vec::new();
    }
    let mut bins: std::collections::HashMap<u32, ([f32; 3], usize)> =
        std::collections::HashMap::new();
    let step = (width * height / 65_536).clamp(1, 8);
    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            let pixel = image[y * width + x];
            let alpha = pixel[3].clamp(0.0, 1.0);
            if alpha < 0.2 {
                continue;
            }
            let distance = (pixel[0] - paper[0])
                .abs()
                .max((pixel[1] - paper[1]).abs())
                .max((pixel[2] - paper[2]).abs());
            if distance < 0.08 {
                continue;
            }
            let r = (pixel[0].clamp(0.0, 1.0) * 31.0).round() as u32;
            let g = (pixel[1].clamp(0.0, 1.0) * 31.0).round() as u32;
            let b = (pixel[2].clamp(0.0, 1.0) * 31.0).round() as u32;
            let key = (r << 10) | (g << 5) | b;
            let entry = bins.entry(key).or_insert(([0.0; 3], 0));
            entry.0[0] += pixel[0].clamp(0.0, 1.0);
            entry.0[1] += pixel[1].clamp(0.0, 1.0);
            entry.0[2] += pixel[2].clamp(0.0, 1.0);
            entry.1 += 1;
        }
    }
    let mut ranked: Vec<([f32; 4], usize)> = bins
        .into_values()
        .map(|(sum, count)| {
            (
                [
                    sum[0] / count as f32,
                    sum[1] / count as f32,
                    sum[2] / count as f32,
                    1.0,
                ],
                count,
            )
        })
        .collect();
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    ranked
        .into_iter()
        .take(max)
        .map(|(color, _)| color)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_hues_are_primary_colours() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), [1.0, 0.0, 0.0]);
        assert_eq!(hsv_to_rgb(1.0 / 3.0, 1.0, 1.0), [0.0, 1.0, 0.0]);
        assert_eq!(hsv_to_rgb(2.0 / 3.0, 1.0, 1.0), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn the_wheel_is_red_at_zero_hue_and_empty_outside() {
        let size = 64;
        let bytes = wheel_rgba(size);
        let centre = size / 2;
        let index = (centre * size + (size - 8)) * 4;
        assert!(
            bytes[index] > 220 && bytes[index + 1] < 30,
            "not red: {:?}",
            &bytes[index..index + 4]
        );
        assert_eq!(bytes[0], 0, "the corner is not transparent");
    }

    #[test]
    fn a_palette_keeps_paint_and_drops_paper() {
        let paper = [1.0, 1.0, 1.0, 1.0];
        let mut image = vec![paper; 8 * 8];
        for y in 1..4 {
            for x in 1..4 {
                image[y * 8 + x] = [0.9, 0.1, 0.1, 1.0];
            }
        }
        for y in 4..7 {
            for x in 4..7 {
                image[y * 8 + x] = [0.1, 0.2, 0.9, 1.0];
            }
        }
        let colors = palette(&image, 8, 8, paper, 4);
        assert_eq!(colors.len(), 2, "paper became a palette colour: {colors:?}");
        assert!(colors.iter().any(|c| c[0] > 0.8 && c[1] < 0.2));
        assert!(colors.iter().any(|c| c[2] > 0.8 && c[0] < 0.2));
    }

    #[test]
    fn the_sv_square_runs_white_to_hue_to_black() {
        let width = 32;
        let height = 32;
        let bytes = sv_rgba(0.0, width, height);
        let top_left = 0;
        let top_right = (width - 1) * 4;
        let bottom = (height - 1) * width * 4;
        assert_eq!(&bytes[top_left..top_left + 3], &[255, 255, 255]);
        assert_eq!(&bytes[top_right..top_right + 3], &[255, 0, 0]);
        assert_eq!(&bytes[bottom..bottom + 3], &[0, 0, 0]);
    }
}

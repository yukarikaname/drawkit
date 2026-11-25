//! A small, local line-art extractor.
//!
//! TEED is a 58K-parameter edge detector (about 249 KB as ONNX). The model is embedded; nothing
//! is downloaded and no image leaves the process. The painting core asks for a line layer, the
//! shell decides when to run it.

use std::io::Cursor;
use std::sync::{Arc, OnceLock};

use image::imageops::FilterType;
use image::{GrayImage, RgbaImage};
use tract_onnx::prelude::*;

const MODEL: &[u8] = include_bytes!("../assets/teed.onnx");
const SIZE: u32 = 512;
const PLANE: usize = (SIZE * SIZE) as usize;

/// The edge probability is mapped to line alpha with this threshold and softness.
#[derive(Clone, Copy, Debug)]
pub struct LineArtOptions {
    pub threshold: f32,
    pub softness: f32,
}

impl Default for LineArtOptions {
    fn default() -> Self {
        Self {
            threshold: 0.55,
            softness: 0.12,
        }
    }
}

type Model = TypedRunnableModel;

static RUNNABLE: OnceLock<Option<Arc<Model>>> = OnceLock::new();

/// Extract black lines on transparent RGBA8 from an image already composited over paper.
pub fn extract(
    pixels: &[u8],
    width: usize,
    height: usize,
    options: LineArtOptions,
) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return None;
    }
    let image = RgbaImage::from_raw(
        width as u32,
        height as u32,
        pixels[..width * height * 4].to_vec(),
    )?;
    let image = image::imageops::resize(&image, SIZE, SIZE, FilterType::Triangle);
    let mut input = vec![0.0f32; 3 * PLANE];
    for (x, y, pixel) in image.enumerate_pixels() {
        let alpha = pixel[3] as f32 / 255.0;
        let red = pixel[0] as f32 * alpha + 255.0 * (1.0 - alpha);
        let green = pixel[1] as f32 * alpha + 255.0 * (1.0 - alpha);
        let blue = pixel[2] as f32 * alpha + 255.0 * (1.0 - alpha);
        let index = y as usize * SIZE as usize + x as usize;
        input[index] = blue - 104.007;
        input[PLANE + index] = green - 116.669;
        input[2 * PLANE + index] = red - 122.679;
    }
    let model = RUNNABLE.get_or_init(load).as_ref()?;
    let tensor = Tensor::from_shape(&[1, 3, SIZE as usize, SIZE as usize], &input).ok()?;
    let outputs = model.run(tvec!(tensor.into_tvalue())).ok()?;
    let edges = outputs.first()?.view().as_slice::<f32>().ok()?;
    if edges.len() < PLANE {
        return None;
    }
    let mut map = GrayImage::new(SIZE, SIZE);
    let softness = options.softness.max(0.005);
    for (index, value) in edges.iter().take(PLANE).enumerate() {
        let t = ((value - options.threshold) / softness).clamp(0.0, 1.0);
        let alpha = (t * t * (3.0 - 2.0 * t) * 255.0).round() as u8;
        map.put_pixel(
            (index as u32) % SIZE,
            (index as u32) / SIZE,
            image::Luma([alpha]),
        );
    }
    let map = image::imageops::resize(&map, width as u32, height as u32, FilterType::Triangle);
    let mut lines = vec![0u8; width * height * 4];
    for (index, pixel) in map.pixels().enumerate() {
        lines[index * 4 + 3] = pixel[0];
    }
    Some(lines)
}

fn load() -> Option<Arc<Model>> {
    let model = tract_onnx::onnx()
        .model_for_read(&mut Cursor::new(MODEL))
        .ok()?
        .with_input_fact(0, f32::fact([1, 3, SIZE as usize, SIZE as usize]).into())
        .ok()?
        .into_optimized()
        .ok()?
        .into_runnable()
        .ok()?;
    Some(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_sheet_produces_no_lines() {
        let pixels = vec![255u8; 64 * 64 * 4];
        let lines = extract(&pixels, 64, 64, LineArtOptions::default()).expect("model runs");
        assert_eq!(lines.len(), 64 * 64 * 4);
        assert!(lines.as_chunks::<4>().0.iter().all(|pixel| pixel[3] < 32));
    }
}

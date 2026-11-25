//! The on-device light estimator.
//!
//! A four-layer stride-2 CNN trained in MLX (`training/lighting`) is embedded as a flat weight
//! file and evaluated with plain Rust. It takes a shaded reference image and proposes a key
//! light, an ambient term and a warmth. Line art has no shading, so the shell keeps the manual
//! controls and the heuristic fallback for that case.

use std::sync::OnceLock;

use glam::Vec3;

use crate::light::{Light, Lighting};

const INPUT: usize = 96;
const CHANNELS: [usize; 5] = [1, 16, 32, 64, 96];
const HIDDEN: usize = 64;
const POOLED: usize = 9 * 96;
const OUTPUT: usize = 5;
const WEIGHTS: &[u8] = include_bytes!("../assets/lighting_weights.bin");
const MAGIC: &[u8; 4] = b"DKLW";

/// The proposal plus the scalar values the shell can show or edit.
#[derive(Clone, Copy, Debug)]
pub struct LightingEstimate {
    pub lighting: Lighting,
    pub direction: Vec3,
    pub intensity: f32,
    pub ambient: f32,
    pub warmth: f32,
}

struct Net {
    tensors: Vec<Vec<f32>>,
}

static NET: OnceLock<Option<Net>> = OnceLock::new();

fn net() -> Option<&'static Net> {
    NET.get_or_init(load).as_ref()
}

fn load() -> Option<Net> {
    if WEIGHTS.len() < 8 || &WEIGHTS[..4] != MAGIC {
        return None;
    }
    let mut cursor = 8;
    let mut tensors = Vec::new();
    let sizes = [
        CHANNELS[1] * 9 * CHANNELS[0],
        CHANNELS[1],
        CHANNELS[2] * 9 * CHANNELS[1],
        CHANNELS[2],
        CHANNELS[3] * 9 * CHANNELS[2],
        CHANNELS[3],
        CHANNELS[4] * 9 * CHANNELS[3],
        CHANNELS[4],
        HIDDEN * POOLED,
        HIDDEN,
        OUTPUT * HIDDEN,
        OUTPUT,
    ];
    for size in sizes {
        let end = cursor + 4;
        let length = u32::from_le_bytes(WEIGHTS.get(cursor..end)?.try_into().ok()?) as usize;
        cursor = end;
        if length != size {
            return None;
        }
        let bytes = WEIGHTS.get(cursor..cursor + length * 4)?;
        cursor += length * 4;
        tensors.push(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|chunk| f32::from_le_bytes(*chunk))
                .collect(),
        );
    }
    Some(Net { tensors })
}

/// Estimate a key light from grayscale pixels in `0..=1`, top row first.
pub fn estimate(gray: &[f32], width: usize, height: usize) -> Option<LightingEstimate> {
    if width == 0 || height == 0 || gray.len() < width * height {
        return None;
    }
    let model = net()?;
    let mut image = vec![0.0f32; INPUT * INPUT];
    for y in 0..INPUT {
        let sy = (y as f32 + 0.5) * height as f32 / INPUT as f32 - 0.5;
        let y0 = sy.floor().max(0.0) as usize;
        let y1 = (y0 + 1).min(height - 1);
        let fy = (sy - y0 as f32).clamp(0.0, 1.0);
        for x in 0..INPUT {
            let sx = (x as f32 + 0.5) * width as f32 / INPUT as f32 - 0.5;
            let x0 = sx.floor().max(0.0) as usize;
            let x1 = (x0 + 1).min(width - 1);
            let fx = (sx - x0 as f32).clamp(0.0, 1.0);
            let top = gray[y0 * width + x0] * (1.0 - fx) + gray[y0 * width + x1] * fx;
            let bottom = gray[y1 * width + x0] * (1.0 - fx) + gray[y1 * width + x1] * fx;
            image[y * INPUT + x] = (top * (1.0 - fy) + bottom * fy).clamp(0.0, 1.0);
        }
    }

    let mean = image.iter().sum::<f32>() / image.len() as f32;
    let variance = image
        .iter()
        .map(|value| (value - mean) * (value - mean))
        .sum::<f32>()
        / image.len() as f32;
    let deviation = variance.sqrt() + 1e-4;
    for value in &mut image {
        *value = (*value - mean) / deviation;
    }

    let t = &model.tensors;
    let mut layer = conv(&image, INPUT, INPUT, 1, &t[0], &t[1], 16);
    layer = conv(&layer, 48, 48, 16, &t[2], &t[3], 32);
    layer = conv(&layer, 24, 24, 32, &t[4], &t[5], 64);
    layer = conv(&layer, 12, 12, 64, &t[6], &t[7], 96);

    let mut pooled = vec![0.0f32; POOLED];
    for bin_y in 0..3 {
        for bin_x in 0..3 {
            for channel in 0..96 {
                let mut sum = 0.0;
                for oy in 0..2 {
                    for ox in 0..2 {
                        sum += layer[((bin_y * 2 + oy) * 6 + bin_x * 2 + ox) * 96 + channel];
                    }
                }
                pooled[(bin_y * 3 + bin_x) * 96 + channel] = sum / 4.0;
            }
        }
    }
    let mut hidden = vec![0.0f32; HIDDEN];
    for (index, value) in hidden.iter_mut().enumerate() {
        let mut sum = t[9][index];
        for (input, weight) in pooled
            .iter()
            .zip(&t[8][index * POOLED..index * POOLED + POOLED])
        {
            sum += input * weight;
        }
        *value = sum.max(0.0);
    }
    let mut output = [0.0f32; OUTPUT];
    for (index, value) in output.iter_mut().enumerate() {
        let mut sum = t[11][index];
        for (input, weight) in hidden
            .iter()
            .zip(&t[10][index * HIDDEN..index * HIDDEN + HIDDEN])
        {
            sum += input * weight;
        }
        *value = sum;
    }

    let direction = Vec3::new(output[0], output[1], output[2]);
    if direction.length_squared() < 1e-6 || output.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let direction = direction.normalize();
    let intensity = output[3].clamp(0.0, 2.0);
    let ambient = output[4].clamp(0.0, 1.0);
    // A grayscale input cannot identify colour temperature; the shell keeps its manual warmth.
    let warmth = 0.0;
    let lighting = Lighting::new(
        Light::new(direction, Light::tint(warmth), intensity),
        Light::NONE,
        Light::NONE,
        ambient,
    );
    Some(LightingEstimate {
        lighting,
        direction,
        intensity,
        ambient,
        warmth,
    })
}

/// One 3x3 stride-2 convolution with padding 1, NHWC in and out, ReLU applied here.
fn conv(
    input: &[f32],
    width: usize,
    height: usize,
    channels: usize,
    weight: &[f32],
    bias: &[f32],
    output_channels: usize,
) -> Vec<f32> {
    let out_width = width.div_ceil(2);
    let out_height = height.div_ceil(2);
    let mut output = vec![0.0f32; out_width * out_height * output_channels];
    for out_y in 0..out_height {
        for out_x in 0..out_width {
            for out_c in 0..output_channels {
                let mut sum = bias[out_c];
                for kernel_y in 0..3 {
                    let in_y = out_y * 2 + kernel_y;
                    if in_y == 0 || in_y > height {
                        continue;
                    }
                    for kernel_x in 0..3 {
                        let in_x = out_x * 2 + kernel_x;
                        if in_x == 0 || in_x > width {
                            continue;
                        }
                        let base = ((in_y - 1) * width + in_x - 1) * channels;
                        let kernel = ((out_c * 3 + kernel_y) * 3 + kernel_x) * channels;
                        for in_c in 0..channels {
                            sum += input[base + in_c] * weight[kernel + in_c];
                        }
                    }
                }
                output[(out_y * out_width + out_x) * output_channels + out_c] = sum.max(0.0);
            }
        }
    }
    output
}

/// `estimate` for the RGBA8 composite the shell already has.
pub fn estimate_rgba8(pixels: &[u8], width: usize, height: usize) -> Option<LightingEstimate> {
    if width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return None;
    }
    // A 4K or 8K canvas has far more pixels than the 96x96 model needs. Sample a bounded grid
    // instead of allocating one f32 for every canvas pixel.
    let step = width.max(height).div_ceil(512).max(1);
    let sampled_width = width.div_ceil(step);
    let sampled_height = height.div_ceil(step);
    let mut gray = Vec::with_capacity(sampled_width * sampled_height);
    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            let index = (y * width + x) * 4;
            let luma = 0.2126 * pixels[index] as f32
                + 0.7152 * pixels[index + 1] as f32
                + 0.0722 * pixels[index + 2] as f32;
            gray.push(luma / 255.0);
        }
    }
    estimate(&gray, sampled_width, sampled_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_image_does_not_panic() {
        let estimate = estimate(&[0.5; 96 * 96], 96, 96);
        assert!(estimate.is_some());
    }

    #[test]
    fn the_output_is_a_usable_rig() {
        let estimate = estimate(&[0.5; 32 * 32], 32, 32).expect("model");
        assert!((estimate.direction.length() - 1.0).abs() < 1e-3);
        assert!((0.0..=2.0).contains(&estimate.intensity));
        assert!((0.0..=1.0).contains(&estimate.ambient));
        assert!((-1.0..=1.0).contains(&estimate.warmth));
    }
}

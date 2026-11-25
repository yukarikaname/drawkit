//! A small preview stroke, rendered through the real brush engine.

use super::*;
use crate::surface::Surface;

const PAPER: [f32; 3] = [0.94, 0.92, 0.87];

/// Paint a pressure-tapered curve on paper and return opaque RGBA8.
pub fn preview(
    brush: &Brush,
    texture: Option<&BrushTexture>,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let mut surface = Surface::new(width.max(1), height.max(1));
    if width < 4 || height < 4 {
        return vec![0; width * height * 4];
    }
    surface.begin_stroke_accumulation();
    if let Some(texture) = texture {
        surface.set_brush_texture(Some(std::sync::Arc::new(texture.clone())));
    }
    let color = [0.08, 0.07, 0.06, 1.0];
    let samples = 36;
    let mut phase = 0.0;
    let mut previous = (6.0, height as f32 / 2.0);
    for sample in 1..=samples {
        let t = sample as f32 / samples as f32;
        let pressure = 0.25 + 0.75 * (t * std::f32::consts::PI).sin();
        let x = 6.0 + t * (width as f32 - 12.0);
        let y = height as f32 / 2.0 + (t * std::f32::consts::TAU).sin() * height as f32 * 0.14;
        let radius = height as f32 * 0.20 * (0.45 + 0.55 * pressure);
        let ink = [color[0], color[1], color[2], brush.alpha_at(pressure)];
        stroke_with_phase(
            &mut surface,
            (previous, (x, y)),
            radius,
            brush,
            ink,
            7,
            &mut phase,
        );
        previous = (x, y);
    }
    surface.end_stroke_accumulation();
    let mut out = vec![0_u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let pixel = surface.get(x, y);
            let alpha = pixel[3].clamp(0.0, 1.0);
            for channel in 0..3 {
                let value = pixel[channel] * alpha + PAPER[channel] * (1.0 - alpha);
                out[(y * width + x) * 4 + channel] = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            out[(y * width + x) * 4 + 3] = 255;
        }
    }
    out
}

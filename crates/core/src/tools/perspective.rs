//! Perspective guides inferred from line art.
//!
//! The detector is the classical offline pipeline: downscale the alpha mask, vote for lines in a
//! Hough accumulator, fit each peak with total least squares, then let line pairs vote for
//! vanishing points. There is no model and no random number generator: the same mask always
//! produces the same guides, so a shell can infer once and store the result.
//!
//! The shell owns the overlay. This module only answers where the guides are.

use super::*;

mod detect;
mod vanish;

/// Settings for [`infer`]. Distances are document pixels; the detector converts them to its
/// downscaled grid before measuring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerspectiveOptions {
    /// Shortest segment kept, in document pixels. A line shorter than this cannot define a guide.
    pub min_segment_length: f32,
    /// How many vanishing points to return, clamped to 1..=3.
    pub max_points: usize,
    /// Lines whose directions differ by no more than this are treated as parallel.
    pub angle_tolerance_degrees: f32,
    /// A line supports a point when its distance to that point is at most this, in document pixels.
    pub distance_tolerance: f32,
}

impl Default for PerspectiveOptions {
    fn default() -> Self {
        Self {
            min_segment_length: 15.0,
            max_points: 3,
            angle_tolerance_degrees: 2.5,
            distance_tolerance: 4.0,
        }
    }
}

/// One recovered vanishing point in document pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VanishingPoint {
    pub x: f32,
    pub y: f32,
    /// A parallel family has no finite intersection; `direction_degrees` then carries its angle
    /// and `x`/`y` are the canvas centre.
    pub at_infinity: bool,
    /// Line direction in degrees, in `[0, 180)`. Meaningful only when `at_infinity`.
    pub direction_degrees: f32,
    /// Supporting lines divided by considered lines, in `0..=1`.
    pub confidence: f32,
    pub support: usize,
    /// Lines the detector considered, including the ones that voted for other points.
    pub lines: usize,
}

/// A line through the two strongest finite points: `a*x + b*y + c = 0`, with `a*a + b*b == 1`.
/// Only present when at least two finite vanishing points were recovered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Horizon {
    pub a: f32,
    pub b: f32,
    pub c: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Perspective {
    /// Strongest first.
    pub points: Vec<VanishingPoint>,
    pub horizon: Option<Horizon>,
}

/// Infer perspective guides from an alpha mask: one byte per pixel, zero is blank.
///
/// Returns `None` for an empty canvas, a canvas with fewer than two usable lines, or input whose
/// buffer is shorter than `width * height`.
pub fn infer(
    mask: &[u8],
    width: usize,
    height: usize,
    options: PerspectiveOptions,
) -> Option<Perspective> {
    if width == 0 || height == 0 {
        return None;
    }
    let area = width.checked_mul(height)?;
    if mask.len() < area {
        return None;
    }
    let max_points = options.max_points.clamp(1, 3);
    let detection = detect::Detection::new(mask, width, height);
    let scale = detection.scale;
    let min_length = (options.min_segment_length / scale).max(2.0);
    let distance = (options.distance_tolerance / scale).max(1.0);
    let segments = detection.segments(min_length, distance);
    if segments.len() < 2 {
        return None;
    }
    let lines = segments.len();
    let found = vanish::estimate(
        &segments,
        detection.width,
        detection.height,
        options.angle_tolerance_degrees,
        (distance * 1.5).max(1.5),
        max_points,
    );
    if found.is_empty() {
        return None;
    }
    let centre_x = width as f32 * 0.5;
    let centre_y = height as f32 * 0.5;
    let points: Vec<VanishingPoint> = found
        .into_iter()
        .map(|point| VanishingPoint {
            x: if point.at_infinity {
                centre_x
            } else {
                (point.x + 0.5) * detection.scale_x - 0.5
            },
            y: if point.at_infinity {
                centre_y
            } else {
                (point.y + 0.5) * detection.scale_y - 0.5
            },
            at_infinity: point.at_infinity,
            direction_degrees: point.direction_degrees,
            confidence: point.support as f32 / lines as f32,
            support: point.support,
            lines,
        })
        .collect();
    let horizon = horizon(&points);
    Some(Perspective { points, horizon })
}

fn horizon(points: &[VanishingPoint]) -> Option<Horizon> {
    let mut finite = points.iter().filter(|point| !point.at_infinity);
    let first = finite.next()?;
    let second = finite.next()?;
    let dx = second.x - first.x;
    let dy = second.y - first.y;
    let length = dx.hypot(dy);
    if length < 1e-3 {
        return None;
    }
    let a = -dy / length;
    let b = dx / length;
    Some(Horizon {
        a,
        b,
        c: -(a * first.x + b * first.y),
    })
}

impl Editor {
    /// Infer guides from one layer's alpha, or from the visible composite when `layer` is `None`.
    /// A fill or group layer has no line art of its own and returns `None`.
    pub fn infer_perspective(
        &self,
        layer: Option<usize>,
        options: PerspectiveOptions,
    ) -> Option<Perspective> {
        let (width, height) = (self.document.width(), self.document.height());
        if width == 0 || height == 0 {
            return None;
        }
        let mut mask = vec![0u8; width * height];
        match layer {
            Some(index) => {
                let layer = self.document.layers.get(index)?;
                if layer.kind != LayerKind::Paint {
                    return None;
                }
                for y in 0..height {
                    for x in 0..width {
                        let alpha = layer.surface.get(x, y)[3].clamp(0.0, 1.0);
                        mask[y * width + x] = (alpha * 255.0).round() as u8;
                    }
                }
            }
            None => {
                let pixels = self.document.composite_rect(Rect::new(0, 0, width, height));
                for (index, pixel) in pixels.iter().enumerate() {
                    let alpha = pixel[3].clamp(0.0, 1.0);
                    mask[index] = (alpha * 255.0).round() as u8;
                }
            }
        }
        infer(&mask, width, height, options)
    }
}

#[cfg(test)]
mod tests;

//! Mask preparation, Hough voting and line fitting for the perspective detector.

use std::f32::consts::PI;

/// Detection runs on at most this many pixels on the long side.
const MAX_SIDE: usize = 1024;
/// Upper bound on the pixels that vote, so a dense 4K mask cannot make the transform unbounded.
const MAX_VOTERS: usize = 350_000;
/// One degree per bin. Finer bins spread a hand-drawn line over more peaks for little gain.
const THETA_BINS: usize = 180;
const MAX_PEAKS: usize = 96;
const MAX_SEGMENTS: usize = 64;
/// Alpha at or above this is ink. Downscaling a one-pixel line over a 4x4 box still yields about
/// 64, so anti-aliased line art survives the reduction.
const INK_LEVEL: u8 = 16;

pub(super) struct Detection {
    pub width: usize,
    pub height: usize,
    pub scale_x: f32,
    pub scale_y: f32,
    /// Mean of the two axes, for lengths and tolerances.
    pub scale: f32,
    ink: Vec<u8>,
}

/// A fitted line. `(x, y)` is the centroid, `(dx, dy)` a unit direction with `dx >= 0`, and
/// `length` the extent of the inliers along that direction.
#[derive(Clone, Copy, Debug)]
pub(super) struct Segment {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub length: f32,
}

impl Detection {
    pub(super) fn new(mask: &[u8], width: usize, height: usize) -> Self {
        let scale = (width.max(height) as f32 / MAX_SIDE as f32).max(1.0);
        let dw = ((width as f32 / scale).round() as usize).max(1);
        let dh = ((height as f32 / scale).round() as usize).max(1);
        let mut ink = vec![0u8; dw * dh];
        if dw == width && dh == height {
            for (out, &alpha) in ink.iter_mut().zip(mask.iter()) {
                *out = if alpha >= INK_LEVEL { 255 } else { 0 };
            }
        } else {
            for y in 0..dh {
                let y0 = y * height / dh;
                let y1 = ((y + 1) * height / dh).max(y0 + 1).min(height);
                for x in 0..dw {
                    let x0 = x * width / dw;
                    let x1 = ((x + 1) * width / dw).max(x0 + 1).min(width);
                    let mut sum = 0u32;
                    for sy in y0..y1 {
                        for sx in x0..x1 {
                            sum += mask[sy * width + sx] as u32;
                        }
                    }
                    let count = (x1 - x0) * (y1 - y0);
                    if sum / count as u32 >= INK_LEVEL as u32 {
                        ink[y * dw + x] = 255;
                    }
                }
            }
        }
        let scale_x = width as f32 / dw as f32;
        let scale_y = height as f32 / dh as f32;
        Self {
            width: dw,
            height: dh,
            scale_x,
            scale_y,
            scale: (scale_x + scale_y) * 0.5,
            ink,
        }
    }

    /// Hough peaks fitted as segments, longest first and without near-duplicates.
    ///
    /// `min_length` and `distance_tolerance` are in detection pixels. The tolerance widens the
    /// band used to collect a stroke's pixels, which centres the fit on a thick line instead of
    /// keeping the first edge the accumulator happened to peak on.
    pub(super) fn segments(&self, min_length: f32, distance_tolerance: f32) -> Vec<Segment> {
        let mut points = Vec::new();
        let total = self.ink.iter().filter(|&&value| value > 0).count();
        if total < 3 {
            return Vec::new();
        }
        let stride = total.div_ceil(MAX_VOTERS).max(1);
        let mut seen = 0usize;
        for y in 0..self.height {
            let row = &self.ink[y * self.width..(y + 1) * self.width];
            for (x, &value) in row.iter().enumerate() {
                if value == 0 {
                    continue;
                }
                if seen.is_multiple_of(stride) {
                    points.push((x as f32, y as f32));
                }
                seen += 1;
            }
        }
        let diag = (((self.width * self.width + self.height * self.height) as f32).sqrt()).max(1.0);
        let rho_count = (2.0 * diag).ceil() as usize + 3;
        let mut accumulator = vec![0u32; THETA_BINS * rho_count];
        for bin in 0..THETA_BINS {
            let (sin, cos) = (bin as f32 * PI / THETA_BINS as f32).sin_cos();
            let row = &mut accumulator[bin * rho_count..(bin + 1) * rho_count];
            for &(x, y) in &points {
                let rho = x * cos + y * sin + diag;
                let index = rho.round() as usize;
                if index < row.len() {
                    row[index] += 1;
                }
            }
        }

        let threshold = (min_length * 0.4).clamp(5.0, 12.0) as u32;
        let mut peaks = Vec::new();
        for bin in 0..THETA_BINS {
            for rho in 0..rho_count {
                let flat = bin * rho_count + rho;
                let votes = accumulator[flat];
                if votes < threshold {
                    continue;
                }
                if is_local_max(&accumulator, rho_count, bin, rho, votes) {
                    peaks.push((votes, bin, rho));
                }
            }
        }
        peaks.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        peaks.truncate(MAX_PEAKS);

        let band = (distance_tolerance * 0.75).clamp(1.5, 4.0);
        let merge = (distance_tolerance * 0.5).clamp(1.5, 3.0);
        let mut candidates: Vec<(Segment, Vec<usize>)> = Vec::new();
        for (_, bin, rho) in peaks {
            let (sin, cos) = (bin as f32 * PI / THETA_BINS as f32).sin_cos();
            let centre = centred_rho(&accumulator, rho_count, bin, rho).unwrap_or(rho as f32);
            let rho = centre - diag;
            let mut inliers = Vec::new();
            for (index, &(x, y)) in points.iter().enumerate() {
                if (x * cos + y * sin - rho).abs() <= band {
                    inliers.push(index);
                }
            }
            if inliers.len() < 3 {
                continue;
            }
            if let Some(segment) = fit(&points, &inliers, min_length) {
                candidates.push((segment, inliers));
            }
        }

        // A thick stroke also peaks at neighbouring angles, where the vote is a short chord
        // through the same pixels. Longest first, then claim the stroke's pixels so the chords
        // have nothing left to fit.
        candidates.sort_by(|a, b| {
            b.1.len()
                .cmp(&a.1.len())
                .then(b.0.length.total_cmp(&a.0.length))
        });
        let mut claimed = vec![false; points.len()];
        let mut segments: Vec<Segment> = Vec::new();
        for (_, inliers) in candidates {
            let live: Vec<usize> = inliers
                .into_iter()
                .filter(|&index| !claimed[index])
                .collect();
            if live.len() < 3 {
                continue;
            }
            let Some(segment) = fit(&points, &live, min_length) else {
                continue;
            };
            if segments.iter().any(|kept| same_line(kept, &segment, merge)) {
                claim_near(&points, &segment, band * 1.5, &mut claimed);
                continue;
            }
            claim_near(&points, &segment, band * 1.5, &mut claimed);
            segments.push(segment);
            if segments.len() == MAX_SEGMENTS {
                break;
            }
        }
        segments
    }
}

fn claim_near(points: &[(f32, f32)], segment: &Segment, band: f32, claimed: &mut [bool]) {
    for (index, &(x, y)) in points.iter().enumerate() {
        if claimed[index] {
            continue;
        }
        if ((x - segment.x) * segment.dy - (y - segment.y) * segment.dx).abs() <= band {
            claimed[index] = true;
        }
    }
}

fn is_local_max(accumulator: &[u32], rho_count: usize, bin: usize, rho: usize, votes: u32) -> bool {
    let bins = THETA_BINS as i32;
    let rhos = rho_count as i32;
    let flat = bin * rho_count + rho;
    for dt in -1i32..=1 {
        let neighbour_bin = bin as i32 + dt;
        if neighbour_bin < 0 || neighbour_bin >= bins {
            continue;
        }
        for dr in -2i32..=2 {
            if dt == 0 && dr == 0 {
                continue;
            }
            let neighbour_rho = rho as i32 + dr;
            if neighbour_rho < 0 || neighbour_rho >= rhos {
                continue;
            }
            let neighbour = neighbour_bin as usize * rho_count + neighbour_rho as usize;
            let other = accumulator[neighbour];
            if other > votes || (other == votes && neighbour < flat) {
                return false;
            }
        }
    }
    true
}

fn centred_rho(accumulator: &[u32], rho_count: usize, bin: usize, rho: usize) -> Option<f32> {
    let row = &accumulator[bin * rho_count..(bin + 1) * rho_count];
    let low = rho.saturating_sub(3);
    let high = (rho + 4).min(rho_count);
    let mut weighted = 0f64;
    let mut weight = 0f64;
    for (index, &votes) in row.iter().enumerate().take(high).skip(low) {
        let votes = votes as f64;
        weighted += index as f64 * votes;
        weight += votes;
    }
    if weight <= 0.0 {
        None
    } else {
        Some((weighted / weight) as f32)
    }
}

fn fit(points: &[(f32, f32)], indices: &[usize], min_length: f32) -> Option<Segment> {
    let count = indices.len() as f64;
    let mut mean_x = 0f64;
    let mut mean_y = 0f64;
    for &index in indices {
        let (x, y) = points[index];
        mean_x += x as f64;
        mean_y += y as f64;
    }
    mean_x /= count;
    mean_y /= count;
    let mut xx = 0f64;
    let mut yy = 0f64;
    let mut xy = 0f64;
    for &index in indices {
        let (x, y) = points[index];
        let dx = x as f64 - mean_x;
        let dy = y as f64 - mean_y;
        xx += dx * dx;
        yy += dy * dy;
        xy += dx * dy;
    }
    let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
    let mut dx = angle.cos() as f32;
    let mut dy = angle.sin() as f32;
    if dx < -1e-6 || (dx.abs() <= 1e-6 && dy < 0.0) {
        dx = -dx;
        dy = -dy;
    }
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for &index in indices {
        let (x, y) = points[index];
        let along = x * dx + y * dy;
        low = low.min(along);
        high = high.max(along);
    }
    let length = high - low;
    if !length.is_finite() || length < min_length {
        return None;
    }
    Some(Segment {
        x: mean_x as f32,
        y: mean_y as f32,
        dx,
        dy,
        length,
    })
}

fn same_line(a: &Segment, b: &Segment, rho_tolerance: f32) -> bool {
    if a.dx * b.dx + a.dy * b.dy <= 0.0 {
        return false;
    }
    let angle = (a.dx * b.dy - a.dy * b.dx).abs();
    if angle > 1.5f32.to_radians().sin() {
        return false;
    }
    let rho_a = -a.dy * a.x + a.dx * a.y;
    let rho_b = -b.dy * b.x + b.dx * b.y;
    (rho_a - rho_b).abs() <= rho_tolerance
}

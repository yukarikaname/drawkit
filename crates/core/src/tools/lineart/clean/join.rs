//! Bridging collinear line ends across a break.
//!
//! A break is a pair of skeleton tips close enough to be one stroke and pointing at each other. The
//! direction test is what keeps two genuinely parallel lines apart: for those the vector between
//! the tips is perpendicular to both tangents, so the dot products fail even when the tips are only
//! a few pixels apart.

use std::collections::HashMap;

use super::mask::Mask;
use super::skeleton::{self, Endpoint};

/// The cosine of the largest angle still treated as "continuing". About 44 degrees.
const COLLINEAR: f32 = 0.72;

/// Ink this close to an end may belong to the line's own cap, so the bridge is allowed to cross it.
const CAP_ALLOWANCE: f32 = 2.5;

pub(super) fn bridge(mask: &mut Mask, join_px: usize) {
    if join_px == 0 {
        return;
    }
    let endpoints = skeleton::endpoints(&mask.bits, mask.width, mask.height);
    if endpoints.len() < 2 {
        return;
    }
    let cell = join_px.max(1) as i32;
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (index, endpoint) in endpoints.iter().enumerate() {
        grid.entry((endpoint.x / cell, endpoint.y / cell))
            .or_default()
            .push(index);
    }

    let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
    for (index, endpoint) in endpoints.iter().enumerate() {
        let cx = endpoint.x / cell;
        let cy = endpoint.y / cell;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let Some(bucket) = grid.get(&(cx + dx, cy + dy)) else {
                    continue;
                };
                for &other in bucket {
                    if other <= index {
                        continue;
                    }
                    if let Some(distance) = candidate(endpoint, &endpoints[other], join_px, mask) {
                        pairs.push((distance, index, other));
                    }
                }
            }
        }
    }
    pairs.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    let mut used = vec![false; endpoints.len()];
    for (_, from, to) in pairs {
        if used[from] || used[to] {
            continue;
        }
        used[from] = true;
        used[to] = true;
        draw(mask, endpoints[from], endpoints[to]);
    }
}

/// Distance when the pair is a plausible break, otherwise `None`.
fn candidate(a: &Endpoint, b: &Endpoint, join_px: usize, mask: &Mask) -> Option<f32> {
    let dx = (b.x - a.x) as f32;
    let dy = (b.y - a.y) as f32;
    let squared = dx * dx + dy * dy;
    let limit = join_px as f32;
    if squared <= 0.0 || squared > limit * limit {
        return None;
    }
    let distance = squared.sqrt();
    let ux = dx / distance;
    let uy = dy / distance;
    let ax = a.outward[0];
    let ay = a.outward[1];
    let bx = b.outward[0];
    let by = b.outward[1];
    // A continues toward B, B continues toward A, and the two outward tangents oppose each other.
    let a_toward = ax * ux + ay * uy;
    let b_toward = -(bx * ux + by * uy);
    let opposed = -(ax * bx + ay * by);
    if a_toward < COLLINEAR || b_toward < COLLINEAR || opposed < COLLINEAR {
        return None;
    }
    if !clear(a, b, mask) {
        return None;
    }
    Some(distance)
}

/// True when nothing but the two caps lies on the straight path, so the bridge does not cut through
/// an unrelated stroke that happens to cross the gap.
fn clear(a: &Endpoint, b: &Endpoint, mask: &Mask) -> bool {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let steps = dx.abs().max(dy.abs());
    for step in 1..steps {
        let x = a.x + dx * step / steps;
        let y = a.y + dy * step / steps;
        if mask.bits[y as usize * mask.width + x as usize] != 0 {
            let da = ((x - a.x).pow(2) + (y - a.y).pow(2)) as f32;
            let db = ((x - b.x).pow(2) + (y - b.y).pow(2)) as f32;
            if da.sqrt() > CAP_ALLOWANCE && db.sqrt() > CAP_ALLOWANCE {
                return false;
            }
        }
    }
    true
}

fn draw(mask: &mut Mask, a: Endpoint, b: Endpoint) {
    let (mut x, mut y) = (a.x, a.y);
    let dx = (b.x - a.x).abs();
    let dy = -(b.y - a.y).abs();
    let sx = if a.x < b.x { 1 } else { -1 };
    let sy = if a.y < b.y { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        mask.bits[y as usize * mask.width + x as usize] = 1;
        if x == b.x && y == b.y {
            break;
        }
        let doubled = 2 * error;
        if doubled >= dy {
            error += dy;
            x += sx;
        }
        if doubled <= dx {
            error += dx;
            y += sy;
        }
    }
}

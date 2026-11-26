//! Skeleton endpoints and their outward tangents.
//!
//! Endpoint detection is the only part of cleanup that needs a topology, so the ink is thinned with
//! Zhang-Suen and the tips of the one-pixel skeleton are the candidates. Thinning runs over a list
//! of ink indices rather than all pixels, which keeps a sparse 4K line drawing cheap.

use std::collections::VecDeque;

/// Clockwise from north: the P2..P9 order Zhang-Suen is written in, reused for walks.
const NEIGHBOURS: [(isize, isize); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

/// A skeleton tip and the direction the stroke would continue in: a unit vector pointing out of the
/// line, into the gap.
#[derive(Clone, Copy, Debug)]
pub(super) struct Endpoint {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) outward: [f32; 2],
}

/// Thin `bits` and return every tip. Order follows the skeleton's pixel order, so the caller's
/// pairing is deterministic.
pub(super) fn endpoints(bits: &[u8], width: usize, height: usize) -> Vec<Endpoint> {
    let skeleton = thin(bits, width, height);
    let mut points = Vec::new();
    for (index, value) in skeleton.iter().enumerate() {
        if *value == 0 {
            continue;
        }
        let x = index % width;
        let y = index / width;
        let mut degree = 0;
        for (dx, dy) in NEIGHBOURS {
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx >= 0 && ny >= 0 && (nx as usize) < width && (ny as usize) < height {
                degree += u32::from(skeleton[ny as usize * width + nx as usize] != 0);
            }
        }
        if degree == 1 {
            let outward = tangent(&skeleton, width, height, x, y);
            if outward[0] != 0.0 || outward[1] != 0.0 {
                points.push(Endpoint {
                    x: x as i32,
                    y: y as i32,
                    outward,
                });
            }
        }
    }
    points
}

fn at(bits: &[u8], width: usize, height: usize, x: isize, y: isize) -> u8 {
    if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
        0
    } else {
        bits[y as usize * width + x as usize]
    }
}

fn thin(bits: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut skeleton = bits.to_vec();
    let mut active: Vec<u32> = skeleton
        .iter()
        .enumerate()
        .filter(|(_, value)| **value != 0)
        .map(|(index, _)| index as u32)
        .collect();
    // Line art is a few pixels wide; the cap only protects against a solid blob.
    for _ in 0..64 {
        let mut changed = false;
        for phase in 0..2 {
            let mut doomed = Vec::new();
            for &index in &active {
                let index = index as usize;
                if skeleton[index] != 0 && remove(&skeleton, width, height, index, phase) {
                    doomed.push(index as u32);
                }
            }
            for index in doomed {
                skeleton[index as usize] = 0;
                changed = true;
            }
        }
        active.retain(|&index| skeleton[index as usize] != 0);
        if !changed || active.is_empty() {
            break;
        }
    }
    skeleton
}

fn remove(bits: &[u8], width: usize, height: usize, index: usize, phase: u8) -> bool {
    let x = (index % width) as isize;
    let y = (index / width) as isize;
    let mut p = [0u8; 8];
    for (slot, (dx, dy)) in NEIGHBOURS.iter().enumerate() {
        p[slot] = at(bits, width, height, x + dx, y + dy);
    }
    let neighbours: u32 = p.iter().map(|value| u32::from(*value)).sum();
    if !(2..=6).contains(&neighbours) {
        return false;
    }
    // Count 0 -> 1 transitions around the ring, including the wrap from P9 back to P2.
    let mut transitions = 0;
    for slot in 0..8 {
        if p[slot] == 0 && p[(slot + 1) % 8] == 1 {
            transitions += 1;
        }
    }
    if transitions != 1 {
        return false;
    }
    if phase == 0 {
        p[0] * p[2] * p[4] == 0 && p[2] * p[4] * p[6] == 0
    } else {
        p[0] * p[2] * p[6] == 0 && p[0] * p[4] * p[6] == 0
    }
}

/// Walk the skeleton a few steps away from the tip and point from their centroid back to the tip.
fn tangent(skeleton: &[u8], width: usize, height: usize, x: usize, y: usize) -> [f32; 2] {
    let start = y * width + x;
    let mut queue = VecDeque::from([(start, 0u32)]);
    let mut seen = vec![start];
    let (mut sum_x, mut sum_y, mut count) = (0.0f32, 0.0f32, 0.0f32);
    while let Some((index, depth)) = queue.pop_front() {
        if depth > 0 {
            sum_x += (index % width) as f32;
            sum_y += (index / width) as f32;
            count += 1.0;
        }
        if depth == 4 {
            continue;
        }
        let px = index % width;
        let py = index / width;
        for (dx, dy) in NEIGHBOURS {
            let nx = px as isize + dx;
            let ny = py as isize + dy;
            if nx < 0 || ny < 0 || nx as usize >= width || ny as usize >= height {
                continue;
            }
            let next = ny as usize * width + nx as usize;
            if skeleton[next] != 0 && !seen.contains(&next) {
                seen.push(next);
                queue.push_back((next, depth + 1));
            }
        }
    }
    if count == 0.0 {
        return [0.0, 0.0];
    }
    let dx = x as f32 - sum_x / count;
    let dy = y as f32 - sum_y / count;
    let length = (dx * dx + dy * dy).sqrt();
    if length < 0.5 {
        [0.0, 0.0]
    } else {
        [dx / length, dy / length]
    }
}

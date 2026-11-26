//! The byte maps the plan scans: ink, the pass test, the walls and the span flood.

use super::*;

pub(super) fn ink_map(composite: Source<'_>, flat: bool, ink: &mut [u8]) -> bool {
    let mut found = false;
    for (index, slot) in ink.iter_mut().enumerate() {
        if composite.ink(index, flat) > crate::fill::INK_THRESHOLD {
            *slot = 1;
            found = true;
        }
    }
    found
}

/// Pixels reachable from the fill region through ink, used to put the fill under the whole
/// anti-aliased edge. A stack over the ink band, not a fixed-radius dilation, so a soft brush and
/// a hard one both stop at paper.
const CORE_ALPHA: f32 = 0.9;

pub(super) fn grow_into_ink(
    region: &[u8],
    composite: Source<'_>,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let mut out = vec![0u8; region.len()];
    let mut stack: Vec<usize> = Vec::new();
    // Only the partial pixels of the edge are covered. The opaque core stops the walk: crossing
    // it would put the fill under the far side of the stroke, a coloured halo outside the shape.
    let push = |index: usize, out: &mut Vec<u8>, stack: &mut Vec<usize>| {
        let alpha = composite.alpha(index);
        if out[index] == 0 && alpha > 0.004 && alpha < CORE_ALPHA {
            out[index] = 1;
            stack.push(index);
        }
    };
    for (index, &value) in region.iter().enumerate() {
        if value == 0 {
            continue;
        }
        let (x, y) = (index % width, index / width);
        if x > 0 {
            push(index - 1, &mut out, &mut stack);
        }
        if x + 1 < width {
            push(index + 1, &mut out, &mut stack);
        }
        if y > 0 {
            push(index - width, &mut out, &mut stack);
        }
        if y + 1 < height {
            push(index + width, &mut out, &mut stack);
        }
    }
    while let Some(index) = stack.pop() {
        let (x, y) = (index % width, index / width);
        if x > 0 {
            push(index - 1, &mut out, &mut stack);
        }
        if x + 1 < width {
            push(index + 1, &mut out, &mut stack);
        }
        if y > 0 {
            push(index - width, &mut out, &mut stack);
        }
        if y + 1 < height {
            push(index + width, &mut out, &mut stack);
        }
    }
    out
}

pub(super) fn pass_map(
    composite: Source<'_>,
    flat: bool,
    target: &Rgba,
    tolerance: f32,
    pass: &mut [u8],
) -> bool {
    let mut all = true;
    for (index, slot) in pass.iter_mut().enumerate() {
        let inside = within(&composite.shaded(index, flat), target, tolerance);
        *slot = u8::from(inside);
        all &= inside;
    }
    all
}

pub(super) fn merge(welded: Option<Vec<u8>>, edges: Option<&[f32]>) -> Option<Vec<u8>> {
    let walls = match (welded, edges) {
        (None, None) => return None,
        (Some(walls), None) => walls,
        (None, Some(edges)) => {
            let mut walls = vec![0u8; edges.len()];
            add_edges(&mut walls, edges);
            walls
        }
        (Some(mut walls), Some(edges)) => {
            add_edges(&mut walls, edges);
            walls
        }
    };
    if walls.iter().all(|wall| *wall == 0) {
        None
    } else {
        Some(walls)
    }
}

pub(super) fn add_edges(walls: &mut [u8], edges: &[f32]) {
    for (wall, edge) in walls.iter_mut().zip(edges) {
        if *edge >= EDGE_THRESHOLD {
            *wall = 1;
        }
    }
}

pub(super) fn finish(mask: Mask, radius: usize) -> Mask {
    if radius == 0 {
        return mask;
    }
    crate::fill::feather_owned(mask, radius)
}

/// Scanline component fill over a 0/1 eligibility map. The stack holds span seeds, not pixels,
/// and `region` doubles as the visited set and the result.
pub(super) fn flood_spans(
    pass: &[u8],
    region: &mut [u8],
    width: usize,
    height: usize,
    seed: (usize, usize),
) {
    let mut stack = vec![(seed.0 as isize, seed.1 as isize)];
    while let Some((x, y)) = stack.pop() {
        let row = y as usize * width;
        let mut left = x;
        while left >= 0 && region[row + left as usize] == 0 && pass[row + left as usize] != 0 {
            left -= 1;
        }
        left += 1;
        let mut right = x;
        while right < width as isize
            && region[row + right as usize] == 0
            && pass[row + right as usize] != 0
        {
            right += 1;
        }
        right -= 1;
        if right < left {
            continue;
        }
        for column in left..=right {
            region[row + column as usize] = 1;
        }
        for neighbour in [y - 1, y + 1] {
            if neighbour < 0 || neighbour >= height as isize {
                continue;
            }
            let neighbour_row = neighbour as usize * width;
            let mut column = left;
            while column <= right {
                if region[neighbour_row + column as usize] == 0
                    && pass[neighbour_row + column as usize] != 0
                {
                    let start = column;
                    while column <= right
                        && region[neighbour_row + column as usize] == 0
                        && pass[neighbour_row + column as usize] != 0
                    {
                        column += 1;
                    }
                    stack.push((start, neighbour));
                }
                column += 1;
            }
        }
    }
}

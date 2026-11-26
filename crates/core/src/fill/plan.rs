//! The fill plan: classify every pixel once, flood a span map, then build the coverage.
//!
//! The old plan copied the composite, ran four morphology passes on float masks and then asked
//! the flood to measure colour distance again for every pixel it looked at. Here the colour test
//! is one pass into bytes, the barriers share that representation, and the flood scans bytes. The
//! result is the same mask; only the bookkeeping changed.

use super::morph;
use super::{FillOptions, Mask, feather};
use crate::color::Rgba;
use crate::tools::PAPER;

const EDGE_THRESHOLD: f32 = 0.4;

/// Plan from a composite that already has the paper under it.
pub(crate) fn plan_mask(
    composite: &[Rgba],
    size: (usize, usize),
    seed: (usize, usize),
    tolerance: f32,
    options: FillOptions,
    under: bool,
    edges: Option<&[f32]>,
) -> Option<Mask> {
    plan(
        composite, false, size, seed, tolerance, options, under, edges,
    )
}

/// Plan from a straight-alpha layer composite. Shading it with the paper per pixel avoids a
/// second canvas-sized allocation; the paper is opaque, so this is the whole blend.
pub(crate) fn plan_flat(
    composite: &[Rgba],
    size: (usize, usize),
    seed: (usize, usize),
    tolerance: f32,
    options: FillOptions,
    under: bool,
    edges: Option<&[f32]>,
) -> Option<Mask> {
    plan(
        composite, true, size, seed, tolerance, options, under, edges,
    )
}

#[allow(clippy::too_many_arguments)]
fn plan(
    composite: &[Rgba],
    flat: bool,
    size: (usize, usize),
    seed: (usize, usize),
    tolerance: f32,
    options: FillOptions,
    under: bool,
    cached_edges: Option<&[f32]>,
) -> Option<Mask> {
    let (width, height) = size;
    let pixels = width.checked_mul(height)?;
    if width == 0 || height == 0 || composite.len() < pixels {
        return None;
    }
    if seed.0 >= width || seed.1 >= height {
        return None;
    }
    let seeded = seed.1 * width + seed.0;
    // The flood target is the seed's pixel unless the seed itself is a welded wall, which is
    // rare; start with the common case and correct it after the walls are known.
    let mut target = shade(composite[seeded], flat);

    // A one-pixel break in the ink is open to a flood, so weld gaps narrower than two radii
    // before it runs. The morphology is only worth doing when there is ink at all.
    let mut welded: Option<Vec<u8>> = None;
    let mut pass = vec![0u8; pixels];
    if options.close > 0 {
        let mut ink = vec![0u8; pixels];
        if ink_map(composite, flat, &mut ink) {
            let grown = morph::dilate_binary(&ink, width, height, options.close);
            let closed = morph::erode_binary(&grown, width, height, options.close);
            let mut walls = vec![0u8; pixels];
            let mut any = false;
            for index in 0..pixels {
                if closed[index] != 0 && ink[index] == 0 {
                    walls[index] = 1;
                    any = true;
                }
            }
            if any {
                welded = Some(walls);
            }
        }
    }
    let mut all_pass = pass_map(composite, flat, &target, tolerance, &mut pass);
    // The line model sees a continuation the morphology cannot; it is a second wall source. The
    // editor hands the same map back for every fill at one document revision, so the model runs
    // once per picture rather than once per click.
    let cached = if options.smart {
        cached_edges.filter(|edges| edges.len() >= pixels)
    } else {
        None
    };
    let computed = if options.smart && cached.is_none() {
        if flat {
            super::smart::flat_edges(composite, width, height)
        } else {
            super::smart::model_edges(composite, width, height)
        }
    } else {
        None
    };
    let edges = cached.or(computed.as_deref());
    let barrier = merge(welded, edges);

    // A welded pixel is black in the image the old flood saw. When the seed is one, the flood's
    // target is that black, and every wall gets the same tolerance test.
    let sealed_seed = barrier.as_ref().is_some_and(|walls| walls[seeded] != 0);
    if sealed_seed {
        target = [0.0, 0.0, 0.0, 1.0];
        all_pass = pass_map(composite, flat, &target, tolerance, &mut pass);
    }
    let wall_passes = within(&[0.0, 0.0, 0.0, 1.0], &target, tolerance);
    if let Some(walls) = barrier.as_deref() {
        for (index, wall) in walls.iter().enumerate() {
            if *wall != 0 {
                pass[index] = u8::from(wall_passes);
            }
        }
        // A wall turned a failing pixel into a passing one, so the earlier answer is stale.
        all_pass = all_pass || pass.iter().all(|value| *value != 0);
    }
    if pass[seeded] == 0 {
        return None;
    }

    let region = if all_pass {
        vec![1u8; pixels]
    } else {
        let mut region = vec![0u8; pixels];
        flood_spans(&pass, &mut region, width, height, seed);
        region
    };

    let data: Vec<f32> = if under {
        if all_pass {
            vec![1.0; pixels]
        } else {
            let grown = morph::dilate_binary(&region, width, height, options.expand);
            grown.iter().map(|value| f32::from(*value != 0)).collect()
        }
    } else {
        let core = morph::erode_binary(&region, width, height, 1);
        let grown = if all_pass {
            region
        } else {
            morph::dilate_binary(&region, width, height, options.expand)
        };
        grown
            .iter()
            .zip(&core)
            .zip(composite.iter())
            .map(|((&grown, &core), pixel)| {
                if grown == 0 {
                    0.0
                } else if core != 0 {
                    1.0
                } else {
                    (1.0 - super::rgb_strength(shade(*pixel, flat), PAPER)).clamp(0.0, 1.0)
                }
            })
            .collect()
    };
    let mask = Mask {
        width,
        height,
        data,
    };
    Some(finish(mask, options.feather))
}

/// A straight-alpha pixel over the opaque paper, the value the flood and coverage measure.
pub(super) fn shade(pixel: Rgba, flat: bool) -> Rgba {
    if !flat {
        return pixel;
    }
    let alpha = pixel[3];
    let paper = 1.0 - alpha;
    [
        pixel[0] * alpha + PAPER[0] * paper,
        pixel[1] * alpha + PAPER[1] * paper,
        pixel[2] * alpha + PAPER[2] * paper,
        PAPER[3],
    ]
}

/// How far a straight-alpha pixel's colour lands from the paper once laid over it.
fn flat_strength(pixel: Rgba) -> f32 {
    pixel[3]
        * (pixel[0] - PAPER[0])
            .abs()
            .max((pixel[1] - PAPER[1]).abs())
            .max((pixel[2] - PAPER[2]).abs())
}

/// The tolerance test on squared distance; the square root cannot change the comparison.
fn within(a: &Rgba, b: &Rgba, tolerance: f32) -> bool {
    if tolerance < 0.0 {
        return false;
    }
    super::distance_squared(a, b) <= tolerance * tolerance
}

fn ink_map(composite: &[Rgba], flat: bool, ink: &mut [u8]) -> bool {
    let mut found = false;
    if flat {
        for (slot, pixel) in ink.iter_mut().zip(composite) {
            if flat_strength(*pixel) > super::INK_THRESHOLD {
                *slot = 1;
                found = true;
            }
        }
    } else {
        for (slot, pixel) in ink.iter_mut().zip(composite) {
            if super::ink_strength(*pixel, PAPER) > super::INK_THRESHOLD {
                *slot = 1;
                found = true;
            }
        }
    }
    found
}

fn pass_map(
    composite: &[Rgba],
    flat: bool,
    target: &Rgba,
    tolerance: f32,
    pass: &mut [u8],
) -> bool {
    let mut all = true;
    if flat {
        for (slot, pixel) in pass.iter_mut().zip(composite) {
            let inside = within(&shade(*pixel, true), target, tolerance);
            *slot = u8::from(inside);
            all &= inside;
        }
    } else {
        for (slot, pixel) in pass.iter_mut().zip(composite) {
            let inside = within(pixel, target, tolerance);
            *slot = u8::from(inside);
            all &= inside;
        }
    }
    all
}

fn merge(welded: Option<Vec<u8>>, edges: Option<&[f32]>) -> Option<Vec<u8>> {
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

fn add_edges(walls: &mut [u8], edges: &[f32]) {
    for (wall, edge) in walls.iter_mut().zip(edges) {
        if *edge >= EDGE_THRESHOLD {
            *wall = 1;
        }
    }
}

fn finish(mask: Mask, radius: usize) -> Mask {
    if radius == 0 {
        return mask;
    }
    feather(&mask, radius)
}

/// Scanline component fill over a 0/1 eligibility map. The stack holds span seeds, not pixels,
/// and `region` doubles as the visited set and the result.
fn flood_spans(pass: &[u8], region: &mut [u8], width: usize, height: usize, seed: (usize, usize)) {
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

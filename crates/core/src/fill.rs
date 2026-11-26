//! Filling a region, and doing it without a rim.
//!
//! Two problems, one cause. A flood fill decides "inside" or "outside" per pixel, but the edge of
//! an anti-aliased shape is neither: it is a fraction. So the fill either stops short - the shape
//! keeps a one-pixel band of paper around it, the white fringe - or spills past it. The fix is to
//! carry a **coverage** per pixel instead of a bit: grow the hard mask by a pixel or two to catch the
//! blended edge, then read the anti-aliasing back out of the pixels with `color::coverage`.

mod morph;
mod plan;
mod smart;

pub(crate) use smart::flat_edges;
pub use smart::{seal_with_edges, sealed_for_fill};

use crate::color::Rgba;

/// A per-pixel coverage, 0 to 1. The mask a fill hands to the layer.
#[derive(Clone, Debug)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Mask {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![0.0; width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.width + x]
    }

    pub fn set(&mut self, x: usize, y: usize, value: f32) {
        self.data[y * self.width + x] = value;
    }

    pub fn is_empty(&self) -> bool {
        self.data.iter().all(|value| *value <= 0.0)
    }
}

fn distance_squared(a: &Rgba, b: &Rgba) -> f32 {
    // Premultiplied, so a fully transparent pixel is a zero vector whatever colour it claims to
    // have: comparing straight colour would make transparent-of-one-colour and transparent-of-
    // another different, which is how fills leak across an empty layer.
    let pa = [a[0] * a[3], a[1] * a[3], a[2] * a[3], a[3]];
    let pb = [b[0] * b[3], b[1] * b[3], b[2] * b[3], b[3]];
    let d = [pa[0] - pb[0], pa[1] - pb[1], pa[2] - pb[2], pa[3] - pb[3]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] + d[3] * d[3]
}

fn distance(a: &Rgba, b: &Rgba) -> f32 {
    distance_squared(a, b).sqrt()
}

/// The connected region around `seed` whose pixels are within `tolerance` of it.
///
/// Scanline flood fill: the stack holds spans, not pixels, so a 4K region costs a few thousand
/// pushes rather than a few million.
pub fn flood(
    image: &[Rgba],
    width: usize,
    height: usize,
    seed: (usize, usize),
    tolerance: f32,
) -> Mask {
    let mut mask = Mask::new(width, height);
    if seed.0 >= width || seed.1 >= height {
        return mask;
    }
    let target = image[seed.1 * width + seed.0];
    let mut stack = vec![(seed.0 as isize, seed.1 as isize)];
    while let Some((x, y)) = stack.pop() {
        if y < 0 || y >= height as isize {
            continue;
        }
        let mut left = x;
        while left >= 0
            && mask.data[y as usize * width + left as usize] == 0.0
            && distance(&image[y as usize * width + left as usize], &target) <= tolerance
        {
            left -= 1;
        }
        left += 1;
        let mut right = x;
        while right < width as isize
            && mask.data[y as usize * width + right as usize] == 0.0
            && distance(&image[y as usize * width + right as usize], &target) <= tolerance
        {
            right += 1;
        }
        right -= 1;
        for column in left..=right {
            mask.set(column as usize, y as usize, 1.0);
        }
        for neighbour in [y - 1, y + 1] {
            if neighbour < 0 || neighbour >= height as isize {
                continue;
            }
            let mut column = left;
            while column <= right {
                let index = neighbour as usize * width + column as usize;
                if mask.data[index] == 0.0 && distance(&image[index], &target) <= tolerance {
                    stack.push((column, neighbour));
                    while column <= right
                        && mask.data[neighbour as usize * width + column as usize] == 0.0
                        && distance(
                            &image[neighbour as usize * width + column as usize],
                            &target,
                        ) <= tolerance
                    {
                        column += 1;
                    }
                }
                column += 1;
            }
        }
    }
    mask
}

/// Grow a hard mask by `radius` pixels, which is how the blended edge gets included.
pub fn grow(mask: &Mask, radius: usize) -> Mask {
    if radius == 0 || mask.width == 0 || mask.height == 0 {
        return mask.clone();
    }
    let mut out = Mask::new(mask.width, mask.height);
    morph::grow_into(&mask.data, mask.width, mask.height, radius, &mut out.data);
    out
}

/// Shrink a hard mask by `radius` pixels: the part that is unambiguously inside.
pub fn erode(mask: &Mask, radius: usize) -> Mask {
    if radius == 0 || mask.width == 0 || mask.height == 0 {
        return mask.clone();
    }
    let occupied: Vec<u8> = mask.data.iter().map(|v| u8::from(*v > 0.0)).collect();
    let keep = morph::erode_binary(&occupied, mask.width, mask.height, radius);
    let mut out = Mask::new(mask.width, mask.height);
    for (index, value) in mask.data.iter().enumerate() {
        // The old loop left a non-positive pixel untouched and a positive one unchanged only
        // when every in-window neighbour was positive. Keep that exact rule for soft masks.
        out.data[index] = if *value > 0.0 && keep[index] == 0 {
            0.0
        } else {
            *value
        };
    }
    out
}

/// The composite with gaps in the ink sealed up.
///
/// A flood fill sees open or closed, but a one-pixel break in an outline is open, so the fill drains
/// through it and the shape becomes a bucket with no bottom. Closing the ink - grow it by `radius`,
/// then shrink it back - welds breaks narrower than two radii and leaves the line's thickness alone.
/// The welded pixels are only walls for `flood`; the caller still reads coverage from the real image.
/// What the fill does beyond the flood result.
///
/// The defaults are the behaviour the app had before the settings existed: close one-pixel gaps in
/// the ink, grow one pixel to reach the anti-aliased edge, no blur. Exposing them lets a painter
/// seal a wider gap, bleed under a soft line, or soften the new edge without changing the fill's
/// tolerance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FillOptions {
    pub close: usize,
    pub expand: usize,
    pub feather: usize,
    /// Add the local edge model's lines to the barrier, so a break in the drawn line does not
    /// leak the fill into the next region.
    pub smart: bool,
}

impl Default for FillOptions {
    fn default() -> Self {
        Self {
            close: 1,
            expand: 1,
            feather: 0,
            smart: false,
        }
    }
}

impl FillOptions {
    pub fn clamped(self) -> Self {
        Self {
            close: self.close.min(8),
            expand: self.expand.min(16),
            feather: self.feather.min(32),
            smart: self.smart,
        }
    }
}

pub fn feather(mask: &Mask, radius: usize) -> Mask {
    feather_owned(mask.clone(), radius)
}

/// `feather` for a mask the caller already owns: a flat or empty mask comes back as it is, with no
/// copy and no passes.
pub fn feather_owned(mask: Mask, radius: usize) -> Mask {
    let radius = radius.min(32);
    if radius == 0 || mask.width == 0 || mask.height == 0 {
        return mask;
    }
    let (canvas_w, canvas_h) = (mask.width, mask.height);
    let Some(active) = active_pixels(&mask) else {
        return Mask::new(canvas_w, canvas_h);
    };
    if active.flat {
        return mask;
    }
    // The output is the painted pixels grown by the radius; the box that produces an output pixel
    // reads one more radius, so the source rows and columns grow again. Everything outside stays
    // paper, which is why a small fill no longer pays for the whole document.
    let ox0 = active.min_x.saturating_sub(radius);
    let oy0 = active.min_y.saturating_sub(radius);
    let ox1 = (active.max_x + radius).min(canvas_w - 1);
    let oy1 = (active.max_y + radius).min(canvas_h - 1);
    let (width, sx0) = (ox1 - ox0 + 1, ox0.saturating_sub(radius));
    let sx1 = (ox1 + radius).min(canvas_w - 1);
    let (sy0, sy1) = (oy0.saturating_sub(radius), (oy1 + radius).min(canvas_h - 1));
    let rows = sy1 - sy0 + 1;

    let mut horizontal = vec![0.0f32; width * rows];
    for y in oy0..=oy1 {
        let row = y * canvas_w;
        box_row(
            &mask.data[row + sx0..row + sx1 + 1],
            sx0,
            &mut horizontal[(y - sy0) * width..(y - sy0 + 1) * width],
            ox0,
            ox1,
            radius,
            canvas_w,
        );
    }
    let mut out = Mask::new(canvas_w, canvas_h);
    for column in 0..width {
        // Rows outside the buffer are zeros, but they still count in the window, so the window
        // bounds stay in canvas coordinates and the divisor is always the canvas-clamped length.
        let mut start = sy0;
        let mut end = (oy0 + radius + 1).min(canvas_h);
        let mut sum: f64 = (start..end)
            .map(|row| horizontal[(row - sy0) * width + column] as f64)
            .sum();
        for y in oy0..=oy1 {
            let want_start = y.saturating_sub(radius);
            let want_end = (y + radius + 1).min(canvas_h);
            while end < want_end {
                sum += horizontal[(end - sy0) * width + column] as f64;
                end += 1;
            }
            while start < want_start {
                sum -= horizontal[(start - sy0) * width + column] as f64;
                start += 1;
            }
            let at = y * canvas_w + ox0 + column;
            out.data[at] = (sum / (end - start) as f64) as f32;
        }
    }
    out
}

/// One horizontal box pass, output columns `g0..=g1`, from a source slice that starts at column
/// `s0`. The window is clamped to the canvas, not to the slice, so the divisor is the true sample
/// count even where the slice stops short of the canvas edge.
fn box_row(
    source: &[f32],
    s0: usize,
    out: &mut [f32],
    g0: usize,
    g1: usize,
    radius: usize,
    canvas_w: usize,
) {
    let mut start = g0.saturating_sub(radius);
    let mut end = (g0 + radius + 1).min(canvas_w);
    // A sliding sum: reading a fresh window per pixel repeated the same additions 2r+1 times, which
    // cost 180 ms at 4K. f64 keeps sliding from drifting past an f32 ulp.
    let mut sum: f64 = source[start - s0..end - s0].iter().map(|v| *v as f64).sum();
    for (slot, g) in out.iter_mut().zip(g0..=g1) {
        let want_start = g.saturating_sub(radius);
        let want_end = (g + radius + 1).min(canvas_w);
        while end < want_end {
            sum += source[end - s0] as f64;
            end += 1;
        }
        while start < want_start {
            sum -= source[start - s0] as f64;
            start += 1;
        }
        *slot = (sum / (end - start) as f64) as f32;
    }
}

/// The painted pixels, and whether the whole mask is one 0 or 1.
struct Active {
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
    flat: bool,
}

fn active_pixels(mask: &Mask) -> Option<Active> {
    let first = *mask.data.first()?;
    let mut flat = first == 0.0 || first == 1.0;
    let (mut min_x, mut min_y) = (usize::MAX, usize::MAX);
    let (mut max_x, mut max_y) = (0, 0);
    for (y, row) in mask.data.chunks_exact(mask.width).enumerate() {
        for (x, value) in row.iter().enumerate() {
            if *value != first {
                flat = false;
            }
            if *value > 0.0 {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x == usize::MAX {
        return None;
    }
    Some(Active {
        min_x,
        min_y,
        max_x,
        max_y,
        flat,
    })
}

pub fn close_ink(
    image: &[Rgba],
    width: usize,
    height: usize,
    paper: Rgba,
    radius: usize,
) -> Vec<Rgba> {
    let mut sealed = image.to_vec();
    if radius == 0 || width == 0 || height == 0 {
        return sealed;
    }
    let pixels = width * height;
    let mut ink = vec![0u8; pixels];
    let mut found = false;
    for (index, pixel) in image.iter().take(pixels).enumerate() {
        if ink_strength(*pixel, paper) > INK_THRESHOLD {
            ink[index] = 1;
            found = true;
        }
    }
    if !found {
        return sealed;
    }
    let grown = morph::dilate_binary(&ink, width, height, radius);
    let welded = morph::erode_binary(&grown, width, height, radius);
    for (index, wall) in welded.iter().enumerate() {
        if *wall != 0 && ink[index] == 0 {
            sealed[index] = [0.0, 0.0, 0.0, 1.0];
        }
    }
    sealed
}

/// The largest per-channel distance from the paper, over all four channels.
fn ink_strength(pixel: Rgba, paper: Rgba) -> f32 {
    (pixel[0] - paper[0])
        .abs()
        .max((pixel[1] - paper[1]).abs())
        .max((pixel[2] - paper[2]).abs())
        .max((pixel[3] - paper[3]).abs())
}

/// The same distance over colour only: alpha is the line's own, not ink seen on paper.
fn rgb_strength(pixel: Rgba, paper: Rgba) -> f32 {
    (pixel[0] - paper[0])
        .abs()
        .max((pixel[1] - paper[1]).abs())
        .max((pixel[2] - paper[2]).abs())
}

const INK_THRESHOLD: f32 = 0.08;

/// The soft coverage a fill writes. `under` is true when the seed sits on transparent paper:
/// the fill is then placed underneath the existing line, full to its edge, instead of faded over
/// the anti-aliased rim.
pub fn mask_for_fill(
    composite: &[Rgba],
    width: usize,
    height: usize,
    seed: (usize, usize),
    tolerance: f32,
    options: FillOptions,
    under: bool,
) -> Option<Mask> {
    plan::plan_mask(
        composite,
        (width, height),
        seed,
        tolerance,
        options.clamped(),
        under,
        None,
    )
}

/// The fill mask from a straight-alpha layer composite: the paper is laid under each pixel as
/// the plan reads it, which saves a canvas-sized copy of the shaded image.
pub(crate) fn mask_for_flat(
    composite: &[Rgba],
    size: (usize, usize),
    seed: (usize, usize),
    tolerance: f32,
    options: FillOptions,
    under: bool,
) -> Option<Mask> {
    mask_for_flat_cached(composite, size, seed, tolerance, options, under, None)
}

/// `mask_for_flat` with the edge model's map already in hand. A cached map produces the same mask
/// as computing it here, so a smart fill only runs TEED when the document moved.
pub(crate) fn mask_for_flat_cached(
    composite: &[Rgba],
    size: (usize, usize),
    seed: (usize, usize),
    tolerance: f32,
    options: FillOptions,
    under: bool,
    edges: Option<&[f32]>,
) -> Option<Mask> {
    plan::plan_flat(
        composite,
        size,
        seed,
        tolerance,
        options.clamped(),
        under,
        edges,
    )
}

pub fn edge_coverage(image: &[Rgba], grown: &Mask, core: &Mask, paper: Rgba) -> Mask {
    let mut out = grown.clone();
    for y in 0..grown.height {
        for x in 0..grown.width {
            if grown.get(x, y) <= 0.0 {
                continue;
            }
            if core.get(x, y) > 0.0 {
                out.set(x, y, 1.0);
                continue;
            }
            let pixel = image[y * grown.width + x];
            out.set(x, y, (1.0 - rgb_strength(pixel, paper)).clamp(0.0, 1.0));
        }
    }
    out
}

#[cfg(test)]
mod tests;

//! Filling a region, and doing it without a rim.
//!
//! Two problems, one cause. A flood fill decides "inside" or "outside" per pixel, but the edge of
//! an anti-aliased shape is neither: it is a fraction. So the fill either stops short - the shape
//! keeps a one-pixel band of paper around it, the white fringe - or spills past it. The fix is to
//! carry a **coverage** per pixel instead of a bit: grow the hard mask by a pixel or two to catch the
//! blended edge, then read the anti-aliasing back out of the pixels with `color::coverage`.

use crate::color::{Rgba, over};

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

fn distance(a: &Rgba, b: &Rgba) -> f32 {
    // Premultiplied, so a fully transparent pixel is a zero vector whatever colour it claims to
    // have: comparing straight colour would make transparent-of-one-colour and transparent-of-
    // another different, which is how fills leak across an empty layer.
    let pa = [a[0] * a[3], a[1] * a[3], a[2] * a[3], a[3]];
    let pb = [b[0] * b[3], b[1] * b[3], b[2] * b[3], b[3]];
    let d = [pa[0] - pb[0], pa[1] - pb[1], pa[2] - pb[2], pa[3] - pb[3]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2] + d[3] * d[3]).sqrt()
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
    let mut current = mask.clone();
    for _ in 0..radius {
        let mut next = current.clone();
        for y in 0..mask.height {
            for x in 0..mask.width {
                // The current value participates in the maximum. Skipping a pixel just because it
                // is already partly covered lets an early 0.5 block a later 1.0 from reaching it,
                // which makes a soft mask grow unevenly.
                let mut hit = current.get(x, y);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx >= 0
                            && ny >= 0
                            && (nx as usize) < mask.width
                            && (ny as usize) < mask.height
                        {
                            hit = hit.max(current.get(nx as usize, ny as usize));
                        }
                    }
                }
                next.set(x, y, hit);
            }
        }
        current = next;
    }
    current
}

/// Shrink a hard mask by `radius` pixels: the part that is unambiguously inside.
pub fn erode(mask: &Mask, radius: usize) -> Mask {
    let mut current = mask.clone();
    for _ in 0..radius {
        let mut next = current.clone();
        for y in 0..mask.height {
            for x in 0..mask.width {
                if current.get(x, y) <= 0.0 {
                    continue;
                }
                let mut all = true;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        let inside = nx >= 0
                            && ny >= 0
                            && (nx as usize) < mask.width
                            && (ny as usize) < mask.height
                            && current.get(nx as usize, ny as usize) > 0.0;
                        all &= inside;
                    }
                }
                if !all {
                    next.set(x, y, 0.0);
                }
            }
        }
        current = next;
    }
    current
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
}

impl Default for FillOptions {
    fn default() -> Self {
        Self {
            close: 1,
            expand: 1,
            feather: 0,
        }
    }
}

impl FillOptions {
    pub fn clamped(self) -> Self {
        Self {
            close: self.close.min(8),
            expand: self.expand.min(16),
            feather: self.feather.min(32),
        }
    }
}

pub fn feather(mask: &Mask, radius: usize) -> Mask {
    let radius = radius.min(32);
    if radius == 0 || mask.width == 0 || mask.height == 0 {
        return mask.clone();
    }
    let (width, height) = (mask.width, mask.height);
    let mut horizontal = Mask::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let start = x.saturating_sub(radius);
            let end = (x + radius).min(width - 1);
            let mut sum = 0.0;
            for sample in start..=end {
                sum += mask.get(sample, y);
            }
            horizontal.set(x, y, sum / (end - start + 1) as f32);
        }
    }
    let mut out = Mask::new(width, height);
    for y in 0..height {
        let start = y.saturating_sub(radius);
        let end = (y + radius).min(height - 1);
        for x in 0..width {
            let mut sum = 0.0;
            for sample in start..=end {
                sum += horizontal.get(x, sample);
            }
            out.set(x, y, sum / (end - start + 1) as f32);
        }
    }
    out
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
    let mut ink = Mask::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let pixel = image[y * width + x];
            let difference = (pixel[0] - paper[0])
                .abs()
                .max((pixel[1] - paper[1]).abs())
                .max((pixel[2] - paper[2]).abs())
                .max((pixel[3] - paper[3]).abs());
            if difference > 0.08 {
                ink.set(x, y, 1.0);
            }
        }
    }
    let welded = erode(&grow(&ink, radius), radius);
    for y in 0..height {
        for x in 0..width {
            if welded.get(x, y) > 0.0 && ink.get(x, y) <= 0.0 {
                sealed[y * width + x] = [0.0, 0.0, 0.0, 1.0];
            }
        }
    }
    sealed
}

/// Coverage for a fill: certain inside the core, fading out across the one-pixel edge.
///
/// The interior is paper and the edge is a blend of paper and the line, so the amount to paint is
/// "how much this pixel is *not* the line" - `1 - ink`. Reading it the other way round (how much
/// the pixel is the paint) paints nothing, because the area being filled is paper. That is the
/// mistake that left the rim in the first version.
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
            let ink = (pixel[0] - paper[0])
                .abs()
                .max((pixel[1] - paper[1]).abs())
                .max((pixel[2] - paper[2]).abs());
            out.set(x, y, (1.0 - ink).clamp(0.0, 1.0));
        }
    }
    out
}

/// Composite `paint` onto `image` through a mask.
pub fn apply(image: &mut [Rgba], mask: &Mask, paint: Rgba) {
    for y in 0..mask.height {
        for x in 0..mask.width {
            let amount = mask.get(x, y);
            if amount <= 0.0 {
                continue;
            }
            let index = y * mask.width + x;
            image[index] = over(
                [paint[0], paint[1], paint[2], paint[3] * amount],
                image[index],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(width: usize, height: usize, color: Rgba) -> Vec<Rgba> {
        vec![color; width * height]
    }

    #[test]
    fn flood_stops_at_a_wall() {
        let mut image = canvas(9, 9, [1.0, 1.0, 1.0, 1.0]);
        for y in 0..9 {
            image[y * 9 + 4] = [0.0, 0.0, 0.0, 1.0];
        }
        let mask = flood(&image, 9, 9, (0, 0), 0.1);
        assert!(mask.get(0, 0) > 0.0);
        assert!(mask.get(3, 0) > 0.0);
        assert_eq!(mask.get(4, 0), 0.0, "the wall was filled through");
        assert_eq!(mask.get(5, 0), 0.0, "the far side was filled");
    }

    #[test]
    fn growing_the_mask_reaches_the_edge() {
        let mut image = canvas(5, 5, [1.0, 1.0, 1.0, 1.0]);
        image[2 * 5 + 2] = [1.0, 0.0, 0.0, 1.0];
        let mask = flood(&image, 5, 5, (2, 2), 0.1);
        let grown = grow(&mask, 1);
        assert!(
            grown.get(1, 1) > 0.0,
            "the blend around the shape was not reached"
        );
    }

    #[test]
    fn closing_the_ink_seals_a_one_pixel_gap() {
        let paper = [1.0, 1.0, 1.0, 1.0];
        let mut image = vec![paper; 9 * 9];
        for y in 0..9 {
            if y != 4 {
                image[y * 9 + 4] = [0.0, 0.0, 0.0, 1.0];
            }
        }
        let leaky = flood(&image, 9, 9, (0, 0), 0.1);
        assert!(
            leaky.get(8, 0) > 0.0,
            "the test needs a wall with a hole in it"
        );
        let sealed = close_ink(&image, 9, 9, paper, 1);
        let mask = flood(&sealed, 9, 9, (0, 0), 0.1);
        assert_eq!(mask.get(8, 0), 0.0, "the fill drained through the gap");
    }

    #[test]
    fn closing_the_ink_leaves_a_solid_wall_alone() {
        let paper = [1.0, 1.0, 1.0, 1.0];
        let mut image = vec![paper; 9 * 9];
        for y in 0..9 {
            image[y * 9 + 4] = [0.0, 0.0, 0.0, 1.0];
        }
        let sealed = close_ink(&image, 9, 9, paper, 1);
        let mask = flood(&sealed, 9, 9, (0, 0), 0.1);
        assert!(mask.get(3, 0) > 0.0);
        assert_eq!(mask.get(4, 0), 0.0, "the wall was eaten");
    }

    #[test]
    fn a_partial_coverage_does_not_block_a_stronger_neighbour() {
        let mut mask = Mask::new(8, 8);
        mask.set(4, 4, 1.0);
        mask.set(1, 6, 0.5);
        let grown = grow(&mask, 2);
        assert!(
            (grown.get(2, 5) - 1.0).abs() < 1e-6,
            "a 0.5 pixel stopped the 1.0 from reaching (2,5): {}",
            grown.get(2, 5)
        );
    }

    #[test]
    fn the_interior_is_solid_and_the_edge_fades_by_how_much_line_it_carries() {
        let paper = [1.0, 1.0, 1.0, 1.0];
        let half = over([0.0, 0.0, 0.0, 0.5], paper);
        let image = [paper, half, paper, paper];
        let mut grown = Mask::new(2, 2);
        grown.set(0, 0, 1.0);
        grown.set(1, 0, 1.0);
        let mut core = Mask::new(2, 2);
        core.set(0, 0, 1.0);
        let coverage = edge_coverage(&image, &grown, &core, paper);
        assert!(
            (coverage.get(0, 0) - 1.0).abs() < 1e-4,
            "the interior faded"
        );
        assert!(
            (coverage.get(1, 0) - 0.5).abs() < 1e-4,
            "{}",
            coverage.get(1, 0)
        );
    }

    #[test]
    fn feathering_a_single_pixel_spreads_it() {
        let mut mask = Mask::new(5, 5);
        mask.set(2, 2, 1.0);
        let soft = feather(&mask, 1);
        assert!(soft.get(2, 2) > 0.05 && soft.get(2, 2) < 0.5);
        assert!(soft.get(1, 2) > 0.0);
        assert_eq!(soft.get(0, 0), 0.0);
    }
}

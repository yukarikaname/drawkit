//! The fill plan: classify every pixel once, flood a span map, then build the coverage.
//!
//! The old plan copied the composite, ran four morphology passes on float masks and then asked
//! the flood to measure colour distance again for every pixel it looked at. Here the colour test
//! is one pass into bytes, the barriers share that representation, and the flood scans bytes. The
//! result is the same mask; only the bookkeeping changed.

use super::morph;
use super::{FillOptions, Mask};
use crate::color::Rgba;
use crate::tools::PAPER;

mod maps;

use maps::{finish, flood_spans, grow_into_ink, ink_map, merge, pass_map};

/// Pixels this opaque are the core of a line; a fill does not write under them.
const CORE_ALPHA: f32 = 0.9;

const EDGE_THRESHOLD: f32 = 0.4;

/// The pixels a fill plan reads.
///
/// The replay and the resident canvas both have a premultiplied RGBA8 frame, and copying it into
/// floats costs a canvas-sized allocation (268 MB at 4K) plus the conversion. Reading the bytes
/// where they are, with the paper laid under each pixel on the way out, is the same result: the
/// blend, the ink strength and the straight colour are all reachable from premultiplied bytes
/// without a division.
#[derive(Clone, Copy)]
pub(crate) enum Source<'a> {
    /// Straight-alpha floats, the form the core's own composite produces.
    Floats(&'a [Rgba]),
    /// Premultiplied RGBA8 over transparency, the form a resident stack reads back.
    Bytes(&'a [u8]),
}

impl Source<'_> {
    fn len(&self) -> usize {
        match self {
            Source::Floats(pixels) => pixels.len(),
            Source::Bytes(pixels) => pixels.len() / 4,
        }
    }

    /// The pixel laid over the opaque paper, which is the value the flood compares.
    fn shaded(&self, index: usize, flat: bool) -> Rgba {
        match self {
            Source::Floats(pixels) => shade(pixels[index], flat),
            Source::Bytes(pixels) => {
                let at = index * 4;
                let alpha = f32::from(pixels[at + 3]) / 255.0;
                let paper = 1.0 - alpha;
                [
                    f32::from(pixels[at]) / 255.0 + paper,
                    f32::from(pixels[at + 1]) / 255.0 + paper,
                    f32::from(pixels[at + 2]) / 255.0 + paper,
                    PAPER[3],
                ]
            }
        }
    }

    /// How much ink the pixel carries, the strength the gap-closing map tests.
    fn ink(&self, index: usize, flat: bool) -> f32 {
        match self {
            Source::Floats(pixels) => {
                if flat {
                    flat_strength(pixels[index])
                } else {
                    super::ink_strength(pixels[index], PAPER)
                }
            }
            Source::Bytes(pixels) => {
                // Premultiplied p = c * a, so a * |c - 1| is |p - a|: no division needed.
                let at = index * 4;
                let alpha = f32::from(pixels[at + 3]) / 255.0;
                let channel = |slot: usize| (f32::from(pixels[at + slot]) / 255.0 - alpha).abs();
                channel(0).max(channel(1)).max(channel(2))
            }
        }
    }

    fn alpha(&self, index: usize) -> f32 {
        match self {
            Source::Floats(pixels) => pixels[index][3],
            Source::Bytes(pixels) => f32::from(pixels[index * 4 + 3]) / 255.0,
        }
    }
}

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
        Source::Floats(composite),
        false,
        size,
        seed,
        tolerance,
        options,
        under,
        edges,
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
        Source::Floats(composite),
        true,
        size,
        seed,
        tolerance,
        options,
        under,
        edges,
    )
}

/// Plan from the premultiplied RGBA8 frame a resident stack reads back.
pub(crate) fn plan_flat_bytes(
    composite: &[u8],
    size: (usize, usize),
    seed: (usize, usize),
    tolerance: f32,
    options: FillOptions,
    under: bool,
) -> Option<Mask> {
    if options.smart {
        // The line model reads floats. Decode for it alone rather than for the flood, which reads
        // the bytes in place: a smart fill still costs the copy, a plain one no longer does.
        let image = straight_from_bytes(composite, size)?;
        return plan_flat(&image, size, seed, tolerance, options, under, None);
    }
    plan(
        Source::Bytes(composite),
        true,
        size,
        seed,
        tolerance,
        options,
        under,
        None,
    )
}

/// Premultiplied RGBA8 over transparency to straight alpha, the form the edge model wants.
fn straight_from_bytes(composite: &[u8], size: (usize, usize)) -> Option<Vec<Rgba>> {
    let pixels = size.0.checked_mul(size.1)?;
    if composite.len() < pixels * 4 {
        return None;
    }
    Some(
        composite[..pixels * 4]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| {
                let alpha = f32::from(pixel[3]) / 255.0;
                if alpha <= 0.0 {
                    return [0.0, 0.0, 0.0, 0.0];
                }
                [
                    f32::from(pixel[0]) / 255.0 / alpha,
                    f32::from(pixel[1]) / 255.0 / alpha,
                    f32::from(pixel[2]) / 255.0 / alpha,
                    alpha,
                ]
            })
            .collect(),
    )
}

#[allow(clippy::too_many_arguments)]
fn plan(
    composite: Source<'_>,
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
    let mut target = composite.shaded(seeded, flat);

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
    let computed = match composite {
        // The edge model reads floats; the byte source only reaches here with `smart` off, which
        // its entry point guarantees by decoding first when it is on.
        Source::Floats(pixels) if options.smart && cached.is_none() => {
            if flat {
                super::smart::flat_edges(pixels, width, height)
            } else {
                super::smart::model_edges(pixels, width, height)
            }
        }
        _ => None,
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
            // Fill under the anti-aliased edge of the line, not just one pixel past the flood:
            // the pixels where the brush has partial coverage still show paper, and that ring is
            // the pale rim. Grow into any pixel that carries ink, and stop at the first one that
            // does not, so the fill cannot escape past the stroke it is bounded by.
            let grown = morph::dilate_binary(&region, width, height, options.expand);
            // The anti-aliased edge has no fixed width: it follows the brush, so a soft 8 px
            // stroke has a four-pixel ramp. Walk through every ink pixel connected to the
            // region instead of guessing a radius, and stop at paper. The fill then lies under
            // the whole ramp, not under its first two pixels.
            let ink_grown = grow_into_ink(&region, composite, width, height);
            grown
                .iter()
                .zip(&region)
                .zip(&ink_grown)
                .enumerate()
                .map(|(index, ((&grown, &core), &ink_grown))| {
                    // The user's expand also stops at an opaque core: writing under it is invisible
                    // on an opaque line but tints a thin or translucent one.
                    let is_core = composite.alpha(index) >= CORE_ALPHA;
                    let covered = core != 0 || (grown != 0 && !is_core) || ink_grown != 0;
                    if covered { 1.0 } else { 0.0 }
                })
                .collect()
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
            .enumerate()
            .map(|(index, (&grown, &core))| {
                if grown == 0 {
                    0.0
                } else if core != 0 {
                    1.0
                } else {
                    let pixel = composite.shaded(index, flat);
                    (1.0 - super::rgb_strength(pixel, PAPER)).clamp(0.0, 1.0)
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

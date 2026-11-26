//! Local, deterministic cleanup for extracted line art.
//!
//! TEED leaves three kinds of dirt on the sheet: dust the model mistook for an edge, breaks where
//! two line ends should meet, and pinch points where the edge map doubles back on itself. All three
//! are geometry, so this pass is plain array work: no model, no randomness, no network.
//!
//! The work happens on a binary ink mask derived from the alpha. Original alpha is preserved for
//! pixels that survive; removed pixels go to zero and bridged pixels become opaque black.

mod join;
mod mask;
mod skeleton;

use mask::Mask;

/// Alpha at or above this counts as ink. Line art is premultiplied black, so alpha is coverage.
const INK_ALPHA: f32 = 0.5;

/// How far the binary close reaches, in pixels.
pub const DEFAULT_CLOSE_PX: u32 = 2;
/// The longest break between two collinear ends that the bridge will cross.
pub const DEFAULT_JOIN_PX: u32 = 8;
/// Components smaller than this many pixels are dust and disappear.
pub const DEFAULT_SPECK_PX: u32 = 6;

/// Cleanup knobs, all in document pixels. Zero disables an individual step; all three at zero make
/// [`clean_mask`] a no-op so the extractor can share one code path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanOptions {
    pub close_px: u32,
    pub join_px: u32,
    pub speck_px: u32,
}

impl Default for CleanOptions {
    fn default() -> Self {
        Self {
            close_px: DEFAULT_CLOSE_PX,
            join_px: DEFAULT_JOIN_PX,
            speck_px: DEFAULT_SPECK_PX,
        }
    }
}

impl CleanOptions {
    /// The pass does nothing and returns the mask bit for bit.
    pub const fn disabled() -> Self {
        Self {
            close_px: 0,
            join_px: 0,
            speck_px: 0,
        }
    }

    pub const fn is_disabled(&self) -> bool {
        self.close_px == 0 && self.join_px == 0 && self.speck_px == 0
    }
}

/// Clean an alpha mask in place. `width * height` samples are expected; a short mask is ignored.
///
/// Steps run in order: remove dust, bridge collinear line ends, then close very small gaps. Closing
/// last means the bridge can leave a clean line for the close to thicken rather than the other way
/// around.
pub fn clean_mask(alpha: &mut [f32], width: usize, height: usize, options: CleanOptions) {
    if options.is_disabled() || width == 0 || height == 0 {
        return;
    }
    let Some(samples) = width.checked_mul(height) else {
        return;
    };
    if alpha.len() < samples {
        return;
    }

    let mut mask = Mask::from_alpha(&alpha[..samples], width, height);
    if options.speck_px > 1 {
        mask.despeckle(options.speck_px as usize);
    }
    if options.join_px > 0 {
        join::bridge(&mut mask, options.join_px as usize);
    }
    if options.close_px > 0 {
        mask.close_gaps(options.close_px as usize);
    }
    mask.write_back(&mut alpha[..samples]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(width: usize, height: usize) -> Vec<f32> {
        vec![0.0; width * height]
    }

    fn segment(mask: &mut [f32], width: usize, x0: usize, x1: usize, y: usize) {
        for x in x0..=x1 {
            mask[y * width + x] = 1.0;
        }
    }

    fn ink(mask: &[f32], width: usize, x: usize, y: usize) -> bool {
        mask[y * width + x] >= INK_ALPHA
    }

    #[test]
    fn a_four_pixel_gap_between_collinear_segments_closes() {
        let width = 64;
        let mut mask = canvas(width, 24);
        segment(&mut mask, width, 4, 24, 12);
        segment(&mut mask, width, 29, 49, 12);
        clean_mask(&mut mask, width, 24, CleanOptions::default());
        for x in 4..=49 {
            assert!(ink(&mask, width, x, 12), "gap left at x = {x}");
        }
    }

    #[test]
    fn a_forty_pixel_gap_stays_open() {
        let width = 96;
        let mut mask = canvas(width, 24);
        segment(&mut mask, width, 4, 24, 12);
        segment(&mut mask, width, 65, 85, 12);
        clean_mask(&mut mask, width, 24, CleanOptions::default());
        assert!(!ink(&mask, width, 44, 12), "the long gap was bridged");
    }

    #[test]
    fn two_parallel_lines_six_pixels_apart_do_not_join() {
        let width = 64;
        let mut mask = canvas(width, 32);
        segment(&mut mask, width, 10, 50, 10);
        segment(&mut mask, width, 10, 50, 16);
        clean_mask(&mut mask, width, 32, CleanOptions::default());
        for x in 10..=50 {
            assert!(
                !ink(&mask, width, x, 13),
                "parallel lines joined at x = {x}"
            );
        }
    }

    #[test]
    fn a_three_pixel_speck_disappears_and_a_twenty_pixel_dash_stays() {
        let width = 64;
        let mut mask = canvas(width, 24);
        mask[4 * width + 4] = 1.0;
        mask[4 * width + 5] = 1.0;
        mask[5 * width + 4] = 1.0;
        segment(&mut mask, width, 20, 39, 12);
        clean_mask(&mut mask, width, 24, CleanOptions::default());
        assert!(!ink(&mask, width, 4, 4));
        assert!(!ink(&mask, width, 5, 4));
        assert!(!ink(&mask, width, 4, 5));
        assert!(
            (20..=39).all(|x| ink(&mask, width, x, 12)),
            "the dash went away"
        );
    }

    #[test]
    fn zero_options_leave_the_mask_bit_identical() {
        let width = 37;
        let height = 19;
        let mut mask = canvas(width, height);
        for (index, value) in mask.iter_mut().enumerate() {
            *value = ((index * 37) % 23) as f32 / 23.0;
        }
        let original: Vec<u32> = mask.iter().map(|value| value.to_bits()).collect();
        clean_mask(&mut mask, width, height, CleanOptions::disabled());
        let after: Vec<u32> = mask.iter().map(|value| value.to_bits()).collect();
        assert_eq!(original, after);
    }

    #[test]
    fn the_same_input_twice_gives_the_same_output() {
        let width = 80;
        let mut base = canvas(width, 32);
        segment(&mut base, width, 4, 24, 12);
        segment(&mut base, width, 29, 49, 12);
        segment(&mut base, width, 10, 60, 20);
        base[5 * width + 5] = 1.0;
        base[5 * width + 6] = 1.0;
        let mut first = base.clone();
        let mut second = base;
        clean_mask(&mut first, width, 32, CleanOptions::default());
        clean_mask(&mut second, width, 32, CleanOptions::default());
        assert_eq!(first, second);
    }

    #[test]
    fn four_k_stays_interactive() {
        let (width, height) = (3840usize, 2160usize);
        let mut mask = canvas(width, height);
        for y in 0..height {
            for x in 0..width {
                if x % 48 < 2 || y % 48 < 2 || (x + y) % 96 < 2 {
                    mask[y * width + x] = 1.0;
                }
            }
        }
        let start = std::time::Instant::now();
        clean_mask(&mut mask, width, height, CleanOptions::default());
        let elapsed = start.elapsed();
        eprintln!("4K cleanup: {elapsed:?}");
        assert!(elapsed.as_secs() < 5, "4K cleanup took {elapsed:?}");
    }
}

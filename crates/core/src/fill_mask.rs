//! A fill mask over a composed frame, for a host without a CPU document.

use crate::fill::{FillOptions, Mask};

/// The fill mask from a premultiplied RGBA8 composite over paper. This is the same pipeline the
/// editor runs, exposed for a GPU replay that has the composed frame but not a CPU document.
pub fn mask_from_composite(
    composite: &[u8],
    width: usize,
    height: usize,
    seed: (usize, usize),
    tolerance: f32,
    options: FillOptions,
    under: bool,
) -> Option<Mask> {
    let pixels = width.checked_mul(height)?;
    if width == 0
        || height == 0
        || composite.len() < pixels * 4
        || seed.0 >= width
        || seed.1 >= height
    {
        return None;
    }
    // The frame is read where it is: copying it into floats would cost a canvas-sized allocation
    // (268 MB at 4K) for a plan that can read premultiplied bytes directly.
    crate::fill::mask_for_bytes(composite, (width, height), seed, tolerance, options, under)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_barrier_stops_the_composite_mask() {
        let (width, height) = (8usize, 8usize);
        let mut composite = vec![255u8; width * height * 4];
        for y in 0..height {
            let at = (y * width + 4) * 4;
            composite[at..at + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
        let mask = mask_from_composite(
            &composite,
            width,
            height,
            (1, 1),
            0.0,
            FillOptions::default(),
            false,
        )
        .expect("a mask");
        assert!(mask.get(1, 1) > 0.9);
        assert!(mask.get(3, 7) > 0.9);
        assert_eq!(mask.get(6, 1), 0.0, "the fill crossed the barrier");
    }

    /// Reading the premultiplied bytes in place has to produce the mask the float path does, or a
    /// live fill would differ from the same fill rebuilt from the tape.
    #[test]
    fn reading_the_bytes_matches_reading_floats() {
        let (width, height) = (24usize, 18usize);
        let mut composite = vec![0u8; width * height * 4];
        let mut floats = vec![[0.0f32; 4]; width * height];
        for y in 0..height {
            for x in 0..width {
                let at = (y * width + x) * 4;
                let straight: [f32; 4] = if (10..12).contains(&x) {
                    [0.1, 0.1, 0.1, 1.0]
                } else if x == 9 || x == 12 {
                    [0.1, 0.1, 0.1, 0.5]
                } else if x == 8 || x == 13 {
                    [0.1, 0.1, 0.1, 0.12]
                } else {
                    [0.0, 0.0, 0.0, 0.0]
                };
                floats[y * width + x] = straight;
                for channel in 0..3 {
                    let premultiplied = straight[channel] * straight[3] * 255.0;
                    composite[at + channel] = premultiplied.round() as u8;
                }
                composite[at + 3] = (straight[3] * 255.0).round() as u8;
            }
        }
        let options = FillOptions { close: 1, expand: 1, feather: 0, smart: false };
        for under in [false, true] {
            let bytes = mask_from_composite(
                &composite, width, height, (3, 9), 0.1, options, under,
            )
            .expect("a mask");
            let reference = crate::fill::mask_for_flat(
                &floats, (width, height), (3, 9), 0.1, options, under,
            )
            .expect("a mask");
            for index in 0..width * height {
                let (a, b) = (bytes.data[index], reference.data[index]);
                assert!((a - b).abs() < 0.02, "under {under} at {index}: {a} vs {b}");
            }
        }
    }

    /// The fill has to reach under the anti-aliased edge of a line, which it can only do while it
    /// still knows that edge is partly transparent. Laid over the paper first, every pixel would
    /// read as opaque and the fill would stop a pixel short: the pale rim.
    #[test]
    fn an_under_fill_walks_into_the_soft_edge_of_a_line() {
        let (width, height) = (16usize, 16usize);
        let mut composite = vec![0u8; width * height * 4];
        for x in 0..width {
            for (y, alpha) in [(7, 128u8), (8, 255), (9, 128)] {
                let at = (y * width + x) * 4;
                // Black, premultiplied, as the resident stack reads the frame back.
                composite[at..at + 4].copy_from_slice(&[0, 0, 0, alpha]);
            }
        }
        let mask = mask_from_composite(
            &composite,
            width,
            height,
            (4, 12),
            0.1,
            FillOptions::default(),
            true,
        )
        .expect("a mask");
        assert!(
            mask.get(8, 12) > 0.9,
            "the fill did not reach the seed area"
        );
        assert!(
            mask.get(8, 9) > 0.0,
            "the fill stopped short of the soft edge"
        );
        assert_eq!(mask.get(8, 8), 0.0, "the fill wrote under the opaque core");
        // The walk stops at the core: crossing it would put paint under the far side of the line.
        assert_eq!(mask.get(8, 7), 0.0, "the fill crossed the opaque core");
    }
}

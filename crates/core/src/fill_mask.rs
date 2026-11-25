//! A fill mask over a composed frame, for a host without a CPU document.

use crate::color::Rgba;
use crate::fill::{FillOptions, Mask, close_ink, edge_coverage, erode, feather, flood, grow};

/// The fill mask from a premultiplied RGBA8 composite over paper. This is the same pipeline the
/// editor runs, exposed for a GPU replay that has the composed frame but not a CPU document.
pub fn mask_from_composite(
    composite: &[u8],
    width: usize,
    height: usize,
    seed: (usize, usize),
    tolerance: f32,
    options: FillOptions,
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
    let image: Vec<Rgba> = composite[..pixels * 4]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| {
            let alpha = f32::from(pixel[3]) / 255.0;
            let over = 1.0 - alpha;
            [
                f32::from(pixel[0]) / 255.0 + over,
                f32::from(pixel[1]) / 255.0 + over,
                f32::from(pixel[2]) / 255.0 + over,
                1.0,
            ]
        })
        .collect();
    let options = options.clamped();
    let sealed = close_ink(&image, width, height, crate::tools::PAPER, options.close);
    let mask = flood(&sealed, width, height, seed, tolerance);
    if mask.is_empty() {
        return None;
    }
    let grown = grow(&mask, options.expand);
    let core = erode(&mask, 1);
    let soft = edge_coverage(&image, &grown, &core, crate::tools::PAPER);
    Some(feather(&soft, options.feather))
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
        )
        .expect("a mask");
        assert!(mask.get(1, 1) > 0.9);
        assert!(mask.get(3, 7) > 0.9);
        assert_eq!(mask.get(6, 1), 0.0, "the fill crossed the barrier");
    }
}

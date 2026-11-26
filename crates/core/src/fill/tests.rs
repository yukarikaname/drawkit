//! The fill primitives: flood, morphology, gap closing and edge coverage.

use super::*;
use crate::color::over;

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

/// The bounded feather only visits the painted area, so the window at the edge of that area has to
/// divide by the canvas-clamped sample count, not by the samples it happened to visit.
#[test]
fn the_bounded_feather_matches_the_naive_blur() {
    fn naive(mask: &Mask, radius: usize) -> Vec<f32> {
        let (width, height) = (mask.width, mask.height);
        let mut horizontal = Mask::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let (start, end) = (x.saturating_sub(radius), (x + radius).min(width - 1));
                let mut sum = 0.0;
                for sample in start..=end {
                    sum += mask.get(sample, y);
                }
                horizontal.set(x, y, sum / (end - start + 1) as f32);
            }
        }
        let mut out = Vec::with_capacity(width * height);
        for y in 0..height {
            let (start, end) = (y.saturating_sub(radius), (y + radius).min(height - 1));
            for x in 0..width {
                let mut sum = 0.0;
                for sample in start..=end {
                    sum += horizontal.get(x, sample);
                }
                out.push(sum / (end - start + 1) as f32);
            }
        }
        out
    }
    for (width, height) in [(9usize, 7usize), (40, 23), (17, 5)] {
        for radius in [1usize, 2, 5] {
            let mut mask = Mask::new(width, height);
            for (x, y) in [
                (0, 0),
                (width - 1, 0),
                (width / 2, height / 3),
                (1, height - 1),
            ] {
                mask.set(x, y, 1.0);
            }
            let want = naive(&mask, radius);
            let got = feather(&mask, radius);
            for (index, value) in want.iter().enumerate() {
                assert!(
                    (value - got.data[index]).abs() < 1e-6,
                    "{width}x{height} r{radius} at {}: {value} vs {}",
                    index,
                    got.data[index]
                );
            }
        }
    }
}

#[test]
fn an_edge_map_closes_a_gap_the_plain_fill_leaks_through() {
    let (width, height) = (9usize, 9usize);
    let paper = [1.0, 1.0, 1.0, 1.0];
    let mut image = vec![paper; width * height];
    for x in 0..width {
        if x != 4 {
            image[4 * width + x] = [0.0, 0.0, 0.0, 1.0];
        }
    }
    let sealed = close_ink(&image, width, height, paper, 0);
    let leak = flood(&sealed, width, height, (4, 1), 0.1);
    assert!(
        leak.get(4, 7) > 0.5,
        "the plain fill should leak through the gap"
    );

    let mut edges = vec![0.0; width * height];
    edges[4 * width + 4] = 1.0;
    let mut closed = sealed;
    seal_with_edges(&mut closed, &edges, 0.4);
    let mask = flood(&closed, width, height, (4, 1), 0.1);
    assert_eq!(mask.get(4, 7), 0.0, "the model edge did not close the gap");
}

#[test]
fn under_fill_reaches_the_anti_aliased_edge() {
    // A vertical line with a two-pixel soft edge. The flood stops at the first partial pixel;
    // the fill has to continue under the partial coverage or the paper shows through it.
    let (width, height) = (12usize, 5usize);
    let mut composite = vec![[0.0f32; 4]; width * height];
    for y in 0..height {
        for x in [5usize, 6] {
            composite[y * width + x] = [0.0, 0.0, 0.0, 1.0];
        }
        for x in [3usize, 4, 7] {
            composite[y * width + x] = [0.0, 0.0, 0.0, 0.3];
        }
    }
    let options = crate::fill::FillOptions {
        expand: 1,
        close: 0,
        feather: 0,
        smart: false,
    };
    let mask =
        mask_for_flat(&composite, (width, height), (1, 2), 0.1, options, true).expect("a mask");
    let covered = |x: usize| mask.data[2 * width + x] > 0.0;
    for x in [1, 2, 3, 4] {
        assert!(covered(x), "the fill stops at x {x}");
    }
    for x in [5, 6, 7] {
        assert!(!covered(x), "the fill wrote under the core at x {x}");
    }
    for x in [8, 9, 10] {
        assert!(!covered(x), "the fill escaped past the line at x {x}");
    }
}

#[test]
fn under_fill_reaches_a_wide_soft_edge() {
    // A soft brush can put a six-pixel ramp between paper and its core. The fill has to reach the
    // whole ramp, not a fixed two-pixel ring.
    let (width, height) = (24usize, 5usize);
    let mut composite = vec![[0.0f32; 4]; width * height];
    for y in 0..height {
        for (x, alpha) in [
            (14usize, 1.0f32),
            (13, 0.8),
            (12, 0.6),
            (11, 0.4),
            (10, 0.2),
            (9, 0.05),
        ] {
            composite[y * width + x] = [0.0, 0.0, 0.0, alpha];
        }
    }
    let options = crate::fill::FillOptions {
        expand: 0,
        close: 0,
        feather: 0,
        smart: false,
    };
    let mask =
        mask_for_flat(&composite, (width, height), (1, 2), 0.1, options, true).expect("a mask");
    let covered = |x: usize| mask.data[2 * width + x] > 0.0;
    for x in 1..=13 {
        assert!(covered(x), "the fill stops at x {x}");
    }
    for x in [14, 15] {
        assert!(!covered(x), "the fill crossed the core at x {x}");
    }
}

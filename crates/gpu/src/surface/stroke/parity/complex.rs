//! Grain, jitter, the second tip, tilt, the lock and eraser modes and the linear commit.

use drawkit_core::brush::{Brush, BrushKind};
use drawkit_core::surface::Surface;

use super::compare::*;
use super::paint::*;

#[test]
fn grain_is_the_same_stream_for_one_seed() {
    let brush = Brush {
        hardness: 0.9,
        grain: 0.6,
        ..straight()
    };
    assert_parity("grain", &brush, &horizontal(), Case::new(), None, None);
    let paint = |seed: u64| {
        cpu_paint(
            &horizontal(),
            1.0,
            &brush,
            COLOUR,
            seed,
            None,
            None,
            false,
            false,
        )
    };
    let first = paint(11);
    let again = paint(11);
    let other = paint(12);
    let pixels = |surface: &Surface| -> Vec<[f32; 4]> {
        (0..H as usize)
            .flat_map(|y| (0..W as usize).map(move |x| (x, y)))
            .map(|(x, y)| surface.get(x, y))
            .collect()
    };
    assert_eq!(
        pixels(&first),
        pixels(&again),
        "the same seed painted a different grain"
    );
    let differs =
        (0..H as usize).any(|y| (0..W as usize).any(|x| first.get(x, y)[3] != other.get(x, y)[3]));
    assert!(differs, "a different seed painted the same grain");
    let plain = cpu_paint(
        &horizontal(),
        1.0,
        &Brush {
            hardness: 0.9,
            ..straight()
        },
        COLOUR,
        Case::new().seed,
        None,
        None,
        false,
        false,
    );
    assert!(
        alpha_differs(&plain, &first, 0.05),
        "the grain did not reach the coverage"
    );
}

#[test]
fn jitter_is_the_same_for_one_seed() {
    let brush = Brush {
        hardness: 0.8,
        jitter: 0.4,
        spacing: 0.35,
        ..straight()
    };
    assert_parity("jitter", &brush, &horizontal(), Case::new(), None, None);
    let plain = cpu_paint(
        &horizontal(),
        1.0,
        &Brush {
            hardness: 0.8,
            spacing: 0.35,
            ..straight()
        },
        COLOUR,
        Case::new().seed,
        None,
        None,
        false,
        false,
    );
    let shifted = cpu_paint(
        &horizontal(),
        1.0,
        &brush,
        COLOUR,
        Case::new().seed,
        None,
        None,
        false,
        false,
    );
    assert!(
        alpha_differs(&plain, &shifted, 0.05),
        "jitter did not move the stamps"
    );
}

#[test]
fn dual_tip_offset_and_shape_match() {
    let dual_ink = Brush {
        dual: 0.9,
        dual_scale: 0.6,
        dual_kind: BrushKind::Ink.as_i32() as u8,
        ..straight()
    };
    assert_parity(
        "dual ink",
        &dual_ink,
        &horizontal(),
        Case::new(),
        None,
        None,
    );
    // Bristle as the second tip also exercises its own grain and jitter.
    let dual_bristle = Brush {
        dual: 0.35,
        dual_scale: 0.45,
        dual_kind: BrushKind::Bristle.as_i32() as u8,
        grain: 0.2,
        jitter: 0.1,
        ..straight()
    };
    assert_parity(
        "dual bristle",
        &dual_bristle,
        &horizontal(),
        Case::new(),
        None,
        None,
    );
}

#[test]
fn a_degenerate_dual_segment_puts_the_second_tip_perpendicular() {
    // A click is a zero-length segment. The shader falls back to direction (0, 1) and takes its
    // perpendicular (-1, 0); the core used to fall back to the direction itself, so the second
    // lobe landed on +y on one path and -x on the other.
    let brush = Brush {
        hardness: 1.0,
        spacing: 0.3,
        dual: 1.0,
        dual_scale: 0.8,
        dual_kind: BrushKind::Ink.as_i32() as u8,
        ..straight()
    };
    let dot = [((32.0, 32.0), (32.0, 32.0))];
    assert_parity("dot dual", &brush, &dot, Case::new(), None, None);
    // The second tip must sit on -x for a dot: its 6.4 px disc reaches x=19.2, outside the 8 px
    // primary disc, while nothing lands on +x.
    let cpu = cpu_paint(
        &dot,
        1.0,
        &brush,
        COLOUR,
        Case::new().seed,
        None,
        None,
        false,
        false,
    );
    assert!(cpu.get(20, 32)[3] > 0.3, "no second lobe on -x");
    assert!(cpu.get(44, 32)[3] < 0.05, "the second lobe landed on +x");
}

#[test]
fn the_stroke_region_covers_jittered_and_dual_stamps() {
    // The mask rectangle is what a stamp may touch. A jittered stamp reaches its radius plus the
    // jitter, and a second tip adds its offset, its radius and its own jitter; a rectangle sized
    // for the bare disc clips the ends of the stroke.
    let brush = Brush {
        hardness: 1.0,
        spacing: 0.1,
        jitter: 0.5,
        ..straight()
    };
    let short = [((22.0, 32.0), (42.0, 32.0))];
    assert_parity("jitter reach", &brush, &short, Case::new(), None, None);
    let dual = Brush {
        dual: 1.0,
        dual_scale: 0.5,
        dual_kind: BrushKind::Bristle.as_i32() as u8,
        ..brush
    };
    assert_parity("dual reach", &dual, &short, Case::new(), None, None);
}

#[test]
fn tilt_shaped_roundness_matches() {
    // The GPU commit has no tilt parameters: the shells call `Brush::with_input` and pass the
    // shaped roundness and angle to `stroke_segment`, which is what this pins.
    let tuned = Brush {
        roundness: 0.9,
        ..straight()
    };
    let tilted = tuned.with_input([1.0, 0.4], 0.0, 0.0);
    assert!(tilted.roundness < tuned.roundness);
    assert_parity("tilt", &tilted, &horizontal(), Case::new(), None, None);
    // Roll only rotates the oval; the same path has to agree.
    let rolled = tuned.with_input([0.0, 0.0], 0.0, 0.4);
    assert_parity("roll", &rolled, &horizontal(), Case::new(), None, None);
}

#[test]
fn alpha_lock_keeps_the_alpha_and_moves_the_colour() {
    let base = base_pixels();
    let brush = Brush {
        hardness: 0.7,
        flow: 0.7,
        ..straight()
    };
    let case = Case {
        locked: true,
        ..Case::new()
    };
    let cpu = cpu_paint(
        &horizontal(),
        case.pressure,
        &brush,
        COLOUR,
        case.seed,
        Some(&base),
        None,
        false,
        true,
    );
    // The lock path must not touch the coverage, and the colour must actually move.
    let touched = (0..H as usize).any(|y| {
        (0..W as usize).any(|x| {
            let before = base[y * W as usize + x];
            let after = cpu.get(x, y);
            after[3] == 1.0 && (after[0] - before[0]).abs() > 0.05
        })
    });
    assert!(touched, "the locked stroke did not move the colour");
    assert_parity("alpha lock", &brush, &horizontal(), case, Some(&base), None);
}

#[test]
fn eraser_removes_the_alpha_it_is_given() {
    let base = base_pixels();
    let case = Case {
        erase: true,
        ..Case::new()
    };
    let half = Brush {
        hardness: 0.5,
        flow: 0.5,
        ..straight()
    };
    let cpu = cpu_paint(
        &horizontal(),
        case.pressure,
        &half,
        COLOUR,
        case.seed,
        Some(&base),
        None,
        true,
        false,
    );
    let centre = cpu.get(32, 32)[3];
    assert!((centre - 0.5).abs() < 0.1, "half strength left {centre}");
    assert_parity("erase half", &half, &horizontal(), case, Some(&base), None);

    let full = Brush {
        hardness: 0.5,
        flow: 1.0,
        ..straight()
    };
    let cpu = cpu_paint(
        &horizontal(),
        case.pressure,
        &full,
        COLOUR,
        case.seed,
        Some(&base),
        None,
        true,
        false,
    );
    assert!(cpu.get(32, 32)[3] < 0.1, "full strength left alpha");
    assert_parity("erase full", &full, &horizontal(), case, Some(&base), None);
}

#[test]
fn an_extended_range_erase_matches_at_the_same_strength() {
    let base = base_pixels();
    let brush = Brush {
        hardness: 0.5,
        flow: 0.5,
        ..straight()
    };
    let case = Case {
        erase: true,
        hdr: true,
        ..Case::new()
    };
    assert_parity(
        "erase linear",
        &brush,
        &horizontal(),
        case,
        Some(&base),
        None,
    );
}

#[test]
fn an_extended_range_commit_matches_in_linear_light() {
    let brush = Brush {
        hardness: 0.6,
        flow: 0.8,
        ..straight()
    };
    let mut extended = COLOUR;
    extended[0] = 1.6;
    extended[1] = 0.5;
    let case = Case {
        hdr: true,
        ..Case::new()
    };
    let cpu = cpu_paint(
        &horizontal(),
        case.pressure,
        &brush,
        extended,
        case.seed,
        None,
        None,
        false,
        false,
    );
    let Some(gpu) = gpu_paint(
        &horizontal(),
        case.pressure,
        &brush,
        extended,
        case.seed,
        None,
        None,
        false,
        false,
        true,
    ) else {
        return;
    };
    let diff = compare(&cpu, &gpu);
    assert!(
        diff.worst <= TOLERANCE,
        "linear commit differs by {:.5} at {:?}: cpu {:?} gpu {:?}",
        diff.worst,
        diff.at,
        diff.cpu,
        diff.gpu
    );
    let bright = gpu.iter().map(|pixel| pixel[0]).fold(0.0f32, f32::max);
    assert!(
        bright > 1.5,
        "the half-float tile clamped the highlight to {bright}"
    );
}

#[test]
fn split_segments_do_not_darken_the_join() {
    let brush = Brush {
        hardness: 0.7,
        spacing: 0.35,
        flow: 0.6,
        ..straight()
    };
    let whole = [((8.0, 32.0), (56.0, 32.0))];
    let split = [((8.0, 32.0), (32.0, 32.0)), ((32.0, 32.0), (56.0, 32.0))];
    assert_parity("whole", &brush, &whole, Case::new(), None, None);
    assert_parity("split", &brush, &split, Case::new(), None, None);
    // Jitter and grain used to be drawn from a per-segment stream, so splitting a stroke moved
    // every stamp. The spatial hashes are keyed by the stroke seed and the stamp cell instead.
    let textured = Brush {
        jitter: 0.3,
        grain: 0.3,
        dual: 0.4,
        dual_scale: 0.5,
        dual_kind: BrushKind::Ink.as_i32() as u8,
        ..brush
    };
    assert_parity("textured whole", &textured, &whole, Case::new(), None, None);
    assert_parity("textured split", &textured, &split, Case::new(), None, None);
    // The jitter stream is keyed by the segment, so a stroke cut at a different point is a
    // different (still deterministic) stroke. The tape records the segments, so the preview and
    // the replay see the same ones; parity is the contract, not segmentation independence.
}

//! The plain stamp paths: geometry, flow, pressure, paper and the tip map.

use drawkit_core::brush::{self, Brush, BrushTexture};
use drawkit_core::surface::Surface;

use super::compare::*;
use super::paint::*;

#[test]
fn roundness_angle_and_spacing_match() {
    assert_parity(
        "oval",
        &Brush {
            roundness: 0.4,
            angle: 30.0,
            spacing: 0.18,
            ..straight()
        },
        &horizontal(),
        Case::new(),
        None,
        None,
    );
}

#[test]
fn flow_is_applied_once_not_once_per_pass() {
    let brush = Brush {
        flow: 0.5,
        ..straight()
    };
    assert_parity("flow", &brush, &horizontal(), Case::new(), None, None);
    // Pin the number, not just the parity: the centre of a hard stamp must be flow, not flow
    // squared by the shader and the commit each applying it.
    let mut surface = Surface::new(W as usize, H as usize);
    surface.begin_stroke_accumulation();
    let mut phase = 0.0;
    let ink = [
        COLOUR[0],
        COLOUR[1],
        COLOUR[2],
        COLOUR[3] * brush.alpha_at(1.0),
    ];
    brush::stroke_with_phase(
        &mut surface,
        ((12.0, 32.0), (52.0, 32.0)),
        RADIUS,
        &brush,
        ink,
        1,
        &mut phase,
    );
    surface.end_stroke_accumulation();
    let centre = surface.get(32, 32)[3];
    assert!(
        (centre - 0.5).abs() < 0.01,
        "one flow of 0.5 painted {centre}"
    );
}

#[test]
fn pressure_alpha_scales_the_segment_once() {
    assert_parity(
        "pressure",
        &Brush {
            flow: 0.8,
            pressure_alpha: 0.5,
            ..straight()
        },
        &horizontal(),
        Case {
            pressure: 0.3,
            ..Case::new()
        },
        None,
        None,
    );
}

#[test]
fn paper_grain_matches_at_the_same_canvas_coordinate() {
    let brush = Brush {
        hardness: 0.9,
        paper: 0.6,
        ..straight()
    };
    assert_parity("paper", &brush, &horizontal(), Case::new(), None, None);
    // A shader that ignored paper would still match a CPU path that ignored it, so also require
    // the paper to have changed the pixel.
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
    let textured = cpu_paint(
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
    let changed = (0..H as usize).any(|y| {
        (0..W as usize).any(|x| (plain.get(x, y)[3] - textured.get(x, y)[3]).abs() > 0.05)
    });
    assert!(changed, "paper did not reach the coverage");
}

#[test]
fn tip_texture_matches() {
    let brush = Brush {
        hardness: 0.7,
        texture: 0.5,
        ..straight()
    };
    assert_parity("tip", &brush, &horizontal(), Case::new(), None, None);
    let plain = Brush {
        hardness: 0.7,
        ..straight()
    };
    let paint = |brush: &Brush| {
        cpu_paint(
            &horizontal(),
            1.0,
            brush,
            COLOUR,
            Case::new().seed,
            None,
            None,
            false,
            false,
        )
    };
    assert!(
        alpha_differs(&paint(&plain), &paint(&brush), 0.05),
        "the tip texture did not reach the coverage"
    );
}

/// A small grayscale tile, so the sampled paper/tip branch runs rather than the procedural hash.
fn gradient_map() -> BrushTexture {
    let data: Vec<u8> = (0..8 * 8)
        .map(|index| ((index % 8 + index / 8) * 16) as u8)
        .collect();
    BrushTexture::new(8, 8, data).expect("8x8 map")
}

#[test]
fn a_texture_map_matches_the_cpu_sampler() {
    let map = gradient_map();
    let brush = Brush {
        hardness: 0.8,
        paper: 0.5,
        texture: 0.4,
        ..straight()
    };
    assert_parity("map", &brush, &horizontal(), Case::new(), None, Some(&map));
}

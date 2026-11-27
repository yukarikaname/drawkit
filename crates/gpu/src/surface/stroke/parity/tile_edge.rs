//! A committed stroke has to agree with the tape across a tile boundary.
//!
//! The edit validation rebuilds a tile from the tape: every recorded sample is composited over the
//! pixels the previous samples left (`Editor::tape_region`). The commit reads one coverage buffer,
//! so each sample there has to composite the same way; taking the maximum instead leaves a sample
//! join lighter than the replay, and a join that lands on a tile edge makes the whole tile look
//! unpainted by the tape. This paints a 2048 canvas whose stroke joins exactly at the GPU's 1024
//! tile edge and compares the committed pixels with the rule the validation uses.

use drawkit_core::brush::{self, Brush, BrushKind};
use drawkit_core::surface::Surface;

use crate::GpuCompositor;

const SIDE: u32 = 2048;
const RADIUS: f32 = 18.0;
/// A fractional centre so the stamp rim lands inside the pixel row the test probes.
const Y: f32 = 512.39;
/// The middle of the stroke is the boundary between the two 1024-pixel tiles and a sample join.
const EDGE: u32 = 1024;

fn segments() -> Vec<((f32, f32), (f32, f32))> {
    let from = 409.6;
    let step = 102.4;
    (0..10)
        .map(|index| {
            let start = from + step * index as f32;
            ((start, Y), (start + step, Y))
        })
        .collect()
}

/// The validated comparison: a partly covered pixel is a blend and not compared, and the rest is
/// held to 0.05 on premultiplied colour and alpha.
fn unexplained(tape: [f32; 4], ink: [f32; 4]) -> bool {
    if tape[3] > 0.02 && tape[3] < 0.98 {
        return false;
    }
    let left = [
        tape[0] * tape[3],
        tape[1] * tape[3],
        tape[2] * tape[3],
        tape[3],
    ];
    let right = [ink[0] * ink[3], ink[1] * ink[3], ink[2] * ink[3], ink[3]];
    (0..4).any(|channel| (left[channel] - right[channel]).abs() > 0.05)
}

#[test]
fn a_sample_join_on_a_tile_edge_matches_the_tape() {
    let brush = Brush::for_kind(BrushKind::Ink);
    let colour = [0.2f32, 0.4, 0.8, 1.0];
    let path = segments();

    // The tape's replay: each recorded segment composited over the scratch surface in order, with
    // the spacing phase carried across samples.
    let mut tape = Surface::new(SIDE as usize, SIDE as usize);
    let mut phase = 0.0;
    for &(from, to) in &path {
        brush::stroke_with_phase(&mut tape, (from, to), RADIUS, &brush, colour, 0, &mut phase);
    }

    let mut gpu = GpuCompositor::new(SIDE, SIDE).expect("no wgpu adapter");
    gpu.reset(1, SIDE, SIDE);
    gpu.stroke_begin(
        SIDE,
        SIDE,
        colour,
        brush.flow,
        brush.pressure_alpha,
        brush.paper,
        brush.texture,
        [0.0; 7],
        brush.wet,
        brush.jitter,
        brush.grain,
        0.0,
        0.0,
        0,
        false,
        u32::MAX,
    );
    for &(from, to) in &path {
        gpu.stroke_segment(
            from,
            to,
            RADIUS,
            brush.hardness,
            brush.spacing,
            brush.roundness,
            brush.angle,
            1.0,
        );
    }
    assert!(gpu.commit_stroke(0, [colour[0], colour[1], colour[2]], 1.0) > 0);

    let tiles = gpu.materialize();
    let mut left = false;
    let mut right = false;
    for (_, rect, _) in &tiles {
        left |= rect.0 < EDGE;
        right |= rect.0 + rect.2 > EDGE;
    }
    assert!(left && right, "the stroke did not cross the tile edge");

    // The tape's alpha on the rim above the join is above the validation's blend cutoff, so the
    // pixel is compared and a maximum-only coverage buffer is rejected there.
    let probe = (EDGE, Y as u32 - 17);
    let expected = tape.get(probe.0 as usize, probe.1 as usize);
    assert!(
        expected[3] > 0.98,
        "the probe pixel is not opaque enough to be compared: {expected:?}"
    );

    // Compare the window around the join, which both committed tiles cover.
    let (x0, y0) = (EDGE - 64, probe.1 - 16);
    let (width, height) = (128_u32, 33_u32);
    // The commit rects are tight around the stroke, so the window is not covered edge to edge;
    // what matters is that both sides of the join were compared.
    let (mut compared_left, mut compared_right) = (false, false);
    for (_, rect, pixels) in &tiles {
        let (rx, ry, rw, rh) = (rect.0, rect.1, rect.2, rect.3);
        let sx = x0.max(rx);
        let ex = (x0 + width).min(rx + rw);
        let sy = y0.max(ry);
        let ey = (y0 + height).min(ry + rh);
        for y in sy..ey {
            for x in sx..ex {
                let at = ((y - ry) * rw + (x - rx)) as usize * 4;
                let ink = [
                    f32::from(pixels[at]) / 255.0,
                    f32::from(pixels[at + 1]) / 255.0,
                    f32::from(pixels[at + 2]) / 255.0,
                    f32::from(pixels[at + 3]) / 255.0,
                ];
                let expect = tape.get(x as usize, y as usize);
                assert!(
                    !unexplained(expect, ink),
                    "commit differs from the tape at {x},{y}: tape {expect:?} ink {ink:?}"
                );
                compared_left |= x < EDGE;
                compared_right |= x >= EDGE;
            }
        }
    }
    assert!(
        compared_left && compared_right,
        "the join did not land between two committed tiles"
    );
}

#[test]
fn scan_rim() {
    let brush = Brush::for_kind(BrushKind::Ink);
    let colour = [0.2f32, 0.4, 0.8, 1.0];
    let mut y = 512.25f32;
    while y < 512.61 {
        let path: Vec<_> = (0..10)
            .map(|i| {
                let s = 409.6 + 102.4 * i as f32;
                ((s, y), (s + 102.4, y))
            })
            .collect();
        let paint = |accum: bool| {
            let mut s = Surface::new(SIDE as usize, SIDE as usize);
            if accum {
                s.begin_stroke_accumulation();
            }
            let mut phase = 0.0;
            for &(from, to) in &path {
                brush::stroke_with_phase(&mut s, (from, to), RADIUS, &brush, colour, 0, &mut phase);
            }
            if accum {
                s.end_stroke_accumulation();
            }
            s.get(EDGE as usize, y as u32 as usize - 17)[3]
        };
        eprintln!(
            "Y {y:.3}: tape {:.4} accum {:.4}",
            paint(false),
            paint(true)
        );
        y += 0.02;
    }
}

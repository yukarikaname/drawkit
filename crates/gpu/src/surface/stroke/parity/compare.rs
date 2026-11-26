//! The comparison and the parity assertion the cases share.

use drawkit_core::brush::{Brush, BrushTexture};
use drawkit_core::surface::Surface;

use super::paint::{H, Segment, W, cpu_paint, gpu_paint};

pub(crate) const TOLERANCE: f32 = 1.0 / 255.0;
pub(crate) const COLOUR: [f32; 4] = [0.2, 0.4, 0.8, 1.0];

pub(crate) struct Diff {
    pub(crate) worst: f32,
    pub(crate) at: (usize, usize),
    pub(crate) cpu: [f32; 4],
    pub(crate) gpu: [f32; 4],
}

pub(crate) fn compare(cpu: &Surface, gpu: &[[f32; 4]]) -> Diff {
    let mut diff = Diff {
        worst: 0.0,
        at: (0, 0),
        cpu: [0.0; 4],
        gpu: [0.0; 4],
    };
    for y in 0..H as usize {
        for x in 0..W as usize {
            let expected = cpu.get(x, y);
            let actual = gpu[y * W as usize + x];
            // Straight colour under a nearly transparent pixel is not a visible difference; the
            // core zeroes a fully erased pixel's colour and the commit keeps it.
            let visible = expected[3].max(actual[3]);
            let colour = (0..3)
                .map(|c| (expected[c] - actual[c]).abs() * visible)
                .fold(0.0f32, f32::max);
            let alpha = (expected[3] - actual[3]).abs();
            let worst = colour.max(alpha);
            if worst > diff.worst {
                diff = Diff {
                    worst,
                    at: (x, y),
                    cpu: expected,
                    gpu: actual,
                };
            }
        }
    }
    diff
}

#[derive(Clone, Copy)]
pub(crate) struct Case {
    pub(crate) pressure: f32,
    pub(crate) seed: u64,
    pub(crate) erase: bool,
    pub(crate) locked: bool,
    pub(crate) hdr: bool,
}

impl Case {
    pub(crate) fn new() -> Self {
        Self {
            pressure: 1.0,
            seed: 0x1234_5678,
            erase: false,
            locked: false,
            hdr: false,
        }
    }
}

/// Whether the alpha of two CPU paints differs anywhere by more than `threshold`. A parity test
/// that only compares CPU against GPU would also pass if both ignored the parameter, so the
/// textured cases also check that the texture reached the pixels.
pub(crate) fn alpha_differs(a: &Surface, b: &Surface, threshold: f32) -> bool {
    (0..H as usize)
        .any(|y| (0..W as usize).any(|x| (a.get(x, y)[3] - b.get(x, y)[3]).abs() > threshold))
}

/// Assert the CPU reference and the GPU commit agree within 1/255 over the whole canvas.
pub(crate) fn assert_parity(
    name: &str,
    brush: &Brush,
    segments: &[Segment],
    case: Case,
    base: Option<&[[f32; 4]]>,
    texture: Option<&BrushTexture>,
) {
    let cpu = cpu_paint(
        segments,
        case.pressure,
        brush,
        COLOUR,
        case.seed,
        base,
        texture,
        case.erase,
        case.locked,
    );
    let Some(gpu) = gpu_paint(
        segments,
        case.pressure,
        brush,
        COLOUR,
        case.seed,
        base,
        texture,
        case.erase,
        case.locked,
        case.hdr,
    ) else {
        // No wgpu adapter in this environment; the CPU tests still cover the reference path.
        return;
    };
    let diff = compare(&cpu, &gpu);
    assert!(
        diff.worst <= TOLERANCE,
        "{name}: CPU and GPU differ by {:.5} at {:?}: cpu {:?} gpu {:?}",
        diff.worst,
        diff.at,
        diff.cpu,
        diff.gpu
    );
}

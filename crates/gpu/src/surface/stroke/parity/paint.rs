//! The two paint paths and the fixtures they share.

use std::sync::Arc;

use drawkit_core::brush::{self, Brush, BrushKind, BrushTexture};
use drawkit_core::surface::Surface;

use crate::{GpuCompositor, RectUpload};

/// One input segment: from and to, in canvas pixels.
pub(crate) type Segment = ((f32, f32), (f32, f32));

pub(crate) const W: u32 = 64;
pub(crate) const H: u32 = 64;
pub(crate) const RADIUS: f32 = 8.0;

pub(crate) fn rgba8(value: [f32; 4]) -> [u8; 4] {
    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [
        channel(value[0]),
        channel(value[1]),
        channel(value[2]),
        channel(value[3]),
    ]
}

/// The CPU brush path the editor uses: one phase carried across segments, `over` per segment on
/// the accumulated coverage, colour alpha already scaled by pressure.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cpu_paint(
    segments: &[Segment],
    pressure: f32,
    brush: &Brush,
    color: [f32; 4],
    seed: u64,
    base: Option<&[[f32; 4]]>,
    texture: Option<&BrushTexture>,
    erase: bool,
    locked: bool,
) -> Surface {
    let mut surface = Surface::new(W as usize, H as usize);
    if let Some(base) = base {
        for y in 0..H as usize {
            for x in 0..W as usize {
                surface.set(x, y, base[y * W as usize + x]);
            }
        }
    }
    surface.set_brush_texture(texture.map(|map| Arc::new(map.clone())));
    surface.begin_stroke_accumulation();
    let ink = [
        color[0],
        color[1],
        color[2],
        color[3] * brush.alpha_at(pressure),
    ];
    let painted = brush.radius_at(RADIUS, pressure);
    let mut phase = 0.0;
    for &(from, to) in segments {
        if erase {
            brush::erase_with_phase(
                &mut surface,
                (from, to),
                painted,
                brush,
                seed,
                &mut phase,
                brush.flow,
            );
        } else if locked {
            brush::stroke_locked_with_phase(
                &mut surface,
                (from, to),
                painted,
                brush,
                ink,
                seed,
                &mut phase,
            );
        } else {
            brush::stroke_with_phase(
                &mut surface,
                (from, to),
                painted,
                brush,
                ink,
                seed,
                &mut phase,
            );
        }
    }
    surface.end_stroke_accumulation();
    surface
}

/// The GPU path: coverage compute shader, then the tile commit shader, then read the committed
/// straight-alpha pixels back.
#[allow(clippy::too_many_arguments)]
pub(crate) fn gpu_paint(
    segments: &[Segment],
    pressure: f32,
    brush: &Brush,
    color: [f32; 4],
    seed: u64,
    base: Option<&[[f32; 4]]>,
    texture: Option<&BrushTexture>,
    erase: bool,
    locked: bool,
    hdr: bool,
) -> Option<Vec<[f32; 4]>> {
    let mut compositor = if hdr {
        GpuCompositor::new_hdr(W, H)?
    } else {
        GpuCompositor::new(W, H)?
    };
    compositor.reset(1, W, H);
    if let Some(map) = texture {
        compositor.set_brush_map(map.width as u32, map.height as u32, &map.data);
    }
    if let Some(base) = base {
        if hdr {
            let flat: Vec<f32> = base.iter().flatten().copied().collect();
            compositor.upload_layer_linear(0, &flat);
        } else {
            let bytes: Vec<u8> = base.iter().flat_map(|pixel| rgba8(*pixel)).collect();
            compositor.upload_layer(RectUpload {
                index: 0,
                x: 0,
                y: 0,
                width: W,
                height: H,
                pixels: &bytes,
            });
        }
    }
    let secondary = Brush::for_kind(BrushKind::from_i32(brush.dual_kind as i32));
    compositor.stroke_begin(
        W,
        H,
        color,
        // The shell passes the eraser's opacity as the coverage flow, so the harness uses the
        // brush's flow for both kinds.
        brush.flow,
        if erase { 0.0 } else { brush.pressure_alpha },
        brush.paper,
        brush.texture,
        [
            brush.dual,
            brush.dual_scale,
            secondary.hardness,
            secondary.roundness,
            secondary.angle,
            secondary.paper,
            secondary.texture,
        ],
        brush.wet,
        brush.jitter,
        brush.grain,
        secondary.jitter,
        secondary.grain,
        seed,
        erase,
        u32::MAX,
    );
    let painted = brush.radius_at(RADIUS, pressure);
    for &(from, to) in segments {
        compositor.stroke_segment(
            from,
            to,
            painted,
            brush.hardness,
            brush.spacing,
            brush.roundness,
            brush.angle,
            pressure,
        );
    }
    compositor.set_stroke_alpha_lock(locked);
    // The shells pass the brush flow here; the coverage buffer already carries it, so the commit
    // must not apply it again. See `StrokePreview::commit_alpha`.
    let committed = compositor.commit_stroke(0, [color[0], color[1], color[2]], brush.flow);
    compositor.stroke_end();
    if committed == 0 {
        return None;
    }
    if hdr {
        let premultiplied = compositor.composite_linear();
        return Some(
            premultiplied
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| {
                    let alpha = pixel[3].clamp(0.0, 1.0);
                    if alpha > 0.0 {
                        [pixel[0] / alpha, pixel[1] / alpha, pixel[2] / alpha, alpha]
                    } else {
                        [0.0; 4]
                    }
                })
                .collect(),
        );
    }
    // Only the committed tiles come back; the rest of the layer is whatever the base upload left.
    let mut straight: Vec<[f32; 4]> = base
        .map(|pixels| {
            pixels
                .iter()
                .map(|pixel| {
                    let bytes = rgba8(*pixel);
                    [
                        f32::from(bytes[0]) / 255.0,
                        f32::from(bytes[1]) / 255.0,
                        f32::from(bytes[2]) / 255.0,
                        f32::from(bytes[3]) / 255.0,
                    ]
                })
                .collect()
        })
        .unwrap_or_else(|| vec![[0.0; 4]; W as usize * H as usize]);
    for (_, rect, pixels) in compositor.materialize() {
        let (x, y, width, height) = (rect.0 as usize, rect.1 as usize, rect.2, rect.3);
        for row in 0..height as usize {
            for column in 0..width as usize {
                let at = (row * width as usize + column) * 4;
                straight[(y + row) * W as usize + x + column] = [
                    f32::from(pixels[at]) / 255.0,
                    f32::from(pixels[at + 1]) / 255.0,
                    f32::from(pixels[at + 2]) / 255.0,
                    f32::from(pixels[at + 3]) / 255.0,
                ];
            }
        }
    }
    Some(straight)
}

pub(crate) fn horizontal() -> [Segment; 1] {
    [((12.0, 32.0), (52.0, 32.0))]
}

pub(crate) fn straight() -> Brush {
    Brush {
        hardness: 0.6,
        spacing: 0.3,
        ..Brush::soft_round()
    }
}

pub(crate) fn base_pixels() -> Vec<[f32; 4]> {
    // Exact 8-bit values, so the CPU base and the uploaded tile start identical.
    (0..W * H)
        .map(|index| {
            let x = (index % W) as usize;
            if x < 32 {
                [0.2, 0.4, 0.8, 1.0]
            } else {
                [0.6, 0.2, 0.4, 1.0]
            }
        })
        .collect()
}

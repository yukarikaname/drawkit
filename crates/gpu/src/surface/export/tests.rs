//! Export tests: sizes, colour space, the cached frame staying intact.

use super::*;
use crate::{FrameFormat, LayerSpec, LayerStack, Renderer};

fn solid(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    pixel
        .iter()
        .copied()
        .cycle()
        .take((width * height * 4) as usize)
        .collect()
}

fn rgba(width: u32, pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

fn export(
    stack: &mut LayerStack,
    renderer: &Renderer,
    specs: &[LayerSpec],
    width: u32,
    height: u32,
    space: i32,
) -> Option<Vec<u8>> {
    let pipeline = crate::composite::pipeline_for(renderer.device(), stack.format().texture());
    render_export(
        stack,
        renderer.device(),
        renderer.queue(),
        &pipeline,
        specs,
        ExportRequest {
            width,
            height,
            space,
        },
    )
}

#[test]
fn a_red_layer_exports_red() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (width, height) = (8u32, 8u32);
    let mut stack = LayerStack::without_backups(width, height, FrameFormat::Rgba8);
    assert!(stack.set(
        renderer.device(),
        renderer.queue(),
        0,
        &solid(width, height, [255, 0, 0, 255])
    ));
    let pixels = export(
        &mut stack,
        &renderer,
        &[LayerSpec::new(0)],
        width,
        height,
        1,
    )
    .expect("an export");
    assert_eq!(pixels.len(), (width * height * 4) as usize);
    assert_eq!(rgba(width, &pixels, 3, 3), [255, 0, 0, 255]);
}

#[test]
fn a_transparent_document_exports_transparent() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (width, height) = (8u32, 8u32);
    let mut stack = LayerStack::without_backups(width, height, FrameFormat::Rgba8);
    let pixels = export(
        &mut stack,
        &renderer,
        &[LayerSpec::new(0)],
        width,
        height,
        1,
    )
    .expect("an export");
    assert_eq!(pixels.len(), (width * height * 4) as usize);
    assert!(pixels.iter().all(|value| *value == 0));
}

#[test]
fn a_half_scale_export_has_half_the_pixels_and_the_same_average() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (width, height) = (16u32, 8u32);
    let mut stack = LayerStack::without_backups(width, height, FrameFormat::Rgba8);
    let mut canvas = solid(width, height, [0, 0, 255, 255]);
    for y in 0..height {
        for x in 0..width / 2 {
            let at = ((y * width + x) * 4) as usize;
            canvas[at..at + 4].copy_from_slice(&[255, 0, 0, 255]);
        }
    }
    assert!(stack.set(renderer.device(), renderer.queue(), 0, &canvas));
    let pixels = export(&mut stack, &renderer, &[LayerSpec::new(0)], 8, 4, 1).expect("an export");
    assert_eq!(pixels.len(), 8 * 4 * 4);
    // Half red and half blue in, half red and half blue out: a scale rather than a crop.
    let mut sums = [0u64; 4];
    for pixel in pixels.as_chunks::<4>().0 {
        for (sum, value) in sums.iter_mut().zip(pixel) {
            *sum += u64::from(*value);
        }
    }
    let count = (8 * 4) as u64;
    let average = sums.map(|sum| sum / count);
    assert!(
        (127..=128).contains(&average[0]),
        "red average: {average:?}"
    );
    assert!(
        (127..=128).contains(&average[2]),
        "blue average: {average:?}"
    );
    assert_eq!(average[3], 255, "alpha average: {average:?}");
}

#[test]
fn an_export_leaves_the_presented_frame_in_the_cache() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (width, height) = (16u32, 16u32);
    let mut stack = LayerStack::without_backups(width, height, FrameFormat::Rgba8);
    assert!(stack.set(
        renderer.device(),
        renderer.queue(),
        0,
        &solid(width, height, [10, 200, 30, 255])
    ));
    let specs = [LayerSpec::new(0)];
    // What the next present copies from, before the export.
    let before = renderer.composite_stack(&mut stack, &specs);

    export(&mut stack, &renderer, &specs, 8, 8, 1).expect("a scaled export");
    // A present composites from the same cache; the export must not have replaced it with the
    // half-sized frame or with a cleared target.
    assert_eq!(renderer.composite_stack(&mut stack, &specs), before);

    // The canvas-size export reads that same cache directly; it must not consume it either.
    export(&mut stack, &renderer, &specs, width, height, 1).expect("a full-size export");
    let after = renderer.composite_stack(&mut stack, &specs);
    assert_eq!(after, before);
    assert_eq!(rgba(width, &after, 1, 1), [10, 200, 30, 255]);
}

#[test]
fn an_extended_range_export_reads_linear_for_exr() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (width, height) = (8u32, 8u32);
    let mut stack = LayerStack::without_backups(width, height, FrameFormat::Rgba16);
    let specs = [LayerSpec::new(0).fill([0.5, 0.25, 0.0, 1.0])];
    let pipeline = crate::composite::pipeline_for(renderer.device(), FrameFormat::Rgba16.texture());
    let pixels = render_export_linear(
        &mut stack,
        renderer.device(),
        renderer.queue(),
        &pipeline,
        &specs,
        width,
        height,
    )
    .expect("a linear export");
    let at = ((width + 1) * 4) as usize;
    assert!((pixels[at] - 0.5).abs() < 0.01, "red {}", pixels[at]);
    assert!(
        (pixels[at + 1] - 0.25).abs() < 0.01,
        "green {}",
        pixels[at + 1]
    );
    assert!(
        (pixels[at + 3] - 1.0).abs() < 0.01,
        "alpha {}",
        pixels[at + 3]
    );
}

#[test]
fn an_extended_range_export_is_encoded_into_the_document_space() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (width, height) = (8u32, 8u32);
    let mut stack = LayerStack::without_backups(width, height, FrameFormat::Rgba16);
    let specs = [LayerSpec::new(0).fill([0.5, 0.25, 0.0, 1.0])];
    let frame =
        export(&mut stack, &renderer, &specs, width, height, 1).expect("a half-float export");
    // sRGB-encoded P3: 0.5 linear is about 188, 0.25 about 137.
    let pixel = rgba(width, &frame, 1, 1);
    assert!((180..=196).contains(&pixel[0]), "red {pixel:?}");
    assert!((128..=146).contains(&pixel[1]), "green {pixel:?}");
}

/// Not part of the suite: `cargo test -p drawkit-gpu -- --ignored --nocapture measure`.
///
/// The old export path reads every layer back into the core and composites on the CPU; the
/// new one reads the composite the GPU already holds. Times and bytes are printed, not
/// asserted, because they depend on the machine.
#[test]
#[ignore = "4096 measurement"]
fn measure_export_at_4096() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let (side, count) = (4096u32, 4usize);
    let mut stack = LayerStack::without_backups(side, side, FrameFormat::Rgba8);
    for index in 0..count {
        let pixel = [(index * 60) as u8, 40, 200, 255];
        assert!(stack.set(device, queue, index, &solid(side, side, pixel)));
    }
    let specs: Vec<LayerSpec> = (0..count).map(LayerSpec::new).collect();
    // Warm the composite the way a present would.
    let _ = renderer.composite_stack(&mut stack, &specs);
    let frame_bytes = side as usize * side as usize * 4;
    let frame = frame_bytes as f64 / (1024.0 * 1024.0);

    // Before: one full readback per layer, then the CPU over-composite the core runs.
    let mut readback = std::time::Duration::ZERO;
    let mut compose = std::time::Duration::ZERO;
    let start = std::time::Instant::now();
    let mut canvas = vec![0u8; frame_bytes];
    for index in 0..count {
        let read_start = std::time::Instant::now();
        let tile = stack
            .read_layer_rect(device, queue, index, (0, 0, side, side))
            .expect("a full layer readback");
        readback += read_start.elapsed();
        let paint_start = std::time::Instant::now();
        for (out, src) in canvas
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(tile.pixels.as_chunks::<4>().0)
        {
            let rest = 255 - src[3] as u16;
            for channel in 0..3 {
                out[channel] =
                    (src[channel] as u16 + out[channel] as u16 * rest / 255).min(255) as u8;
            }
            out[3] = (src[3] as u16 + out[3] as u16 * rest / 255).min(255) as u8;
        }
        compose += paint_start.elapsed();
    }
    let before = start.elapsed();

    // After: one composite readback at canvas size.
    let start = std::time::Instant::now();
    let exported = export(&mut stack, &renderer, &specs, side, side, 1).expect("an export");
    let after = start.elapsed();
    assert_eq!(exported.len(), side as usize * side as usize * 4);

    println!(
        "4096 export, {count} layers: before {:.0} ms (readback {:.0} ms for {:.0} MiB, \
         CPU composite {:.0} ms for {:.0} MiB), after {:.0} ms ({:.0} MiB read back)",
        before.as_secs_f64() * 1000.0,
        readback.as_secs_f64() * 1000.0,
        frame * count as f64,
        compose.as_secs_f64() * 1000.0,
        frame,
        after.as_secs_f64() * 1000.0,
        frame,
    );
}

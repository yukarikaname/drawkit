//! GPU-runtime numbers, not assertions.
//!
//! Run by hand:
//!
//!   cargo test --release -p drawkit-gpu --test runtime -- --ignored --nocapture
//!
//! The compositor is offscreen, so this measures the same layer stack the WinUI and GTK shells
//! present, including the readback those shells need.

use std::hint::black_box;
use std::time::{Duration, Instant};

use drawkit_gpu::{GpuCompositor, LayerSpec, RectUpload, Renderer};

const K2: u32 = 2048;
const K4: u32 = 4096;
const LAYERS: usize = 100;
const SEGMENTS: usize = 30;
const STROKE_ROUNDS: usize = 5;
const COMPOSITE_FRAMES: usize = 4;

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn colour(index: usize) -> [f32; 4] {
    [
        (index % 7) as f32 / 7.0,
        (index % 5) as f32 / 5.0,
        (index % 3) as f32 / 3.0,
        1.0,
    ]
}

/// A filled disk covering roughly half the canvas, the mask a seed click produces after growth.
fn disk_mask(width: usize, height: usize) -> Vec<f32> {
    let centre = (width as f32 * 0.5, height as f32 * 0.5);
    let radius = width.min(height) as f32 * 0.4;
    let mut mask = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - centre.0;
            let dy = y as f32 - centre.1;
            if dx * dx + dy * dy <= radius * radius {
                mask[y * width + x] = 1.0;
            }
        }
    }
    mask
}

/// One pointer stroke of `SEGMENTS` segments, committed in batch mode. Materialising the touched
/// tiles is the only public call that waits for the GPU, so the number includes that readback.
fn stroke_round(
    compositor: &mut GpuCompositor,
    width: u32,
    height: u32,
    round: usize,
) -> (Duration, usize) {
    let started = Instant::now();
    compositor.stroke_begin(
        width,
        height,
        [1.0, 0.2, 0.1, 1.0],
        1.0,
        0.0,
        0.0,
        0.0,
        [0.0; 7],
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        round as u64 + 1,
        false,
        u32::MAX,
    );
    let centre = (width as f32 * 0.5, height as f32 * 0.5);
    let reach = width.min(height) as f32 * 0.15;
    let phase = round as f32 * 0.3;
    for segment in 0..SEGMENTS {
        let from = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32 + phase;
        let to = std::f32::consts::TAU * (segment + 1) as f32 / SEGMENTS as f32 + phase;
        compositor.stroke_segment(
            (centre.0 + reach * from.cos(), centre.1 + reach * from.sin()),
            (centre.0 + reach * to.cos(), centre.1 + reach * to.sin()),
            8.0,
            0.8,
            0.05,
            1.0,
            0.0,
            1.0,
        );
    }
    let committed = compositor.commit_stroke(0, [1.0, 0.2, 0.1], 1.0);
    compositor.stroke_end();
    let _ = compositor.materialize();
    (started.elapsed(), committed)
}

/// Mark every tile dirty and composite the whole 2K frame, including the readback.
fn composite_frame(compositor: &mut GpuCompositor) -> Duration {
    compositor.set_layer(0, LayerSpec::new(0).fill(colour(0)));
    let started = Instant::now();
    black_box(compositor.composite());
    started.elapsed()
}

fn stroke_timing(width: u32, height: u32) -> Option<(f64, usize)> {
    let mut compositor = GpuCompositor::new(width, height)?;
    compositor.reset(1, width, height);
    compositor.set_stroke_batch(true);
    stroke_round(&mut compositor, width, height, 0);
    let mut total = Duration::ZERO;
    let mut committed = 0;
    for round in 0..STROKE_ROUNDS {
        let (elapsed, rects) = stroke_round(&mut compositor, width, height, round + 1);
        total += elapsed;
        committed += rects;
    }
    Some((ms(total) / STROKE_ROUNDS as f64, committed))
}

fn tile_residency() -> Option<usize> {
    let renderer = Renderer::new()?;
    let mut stack = renderer.layer_stack(K2, K2);
    let pixels = vec![128u8; K2 as usize * K2 as usize * 4];
    if !stack.set(renderer.device(), renderer.queue(), 0, &pixels) {
        return None;
    }
    let specs = [LayerSpec::new(0)];
    renderer.composite_stack_texture(&mut stack, &specs)?;
    Some(stack.resident_bytes())
}

#[test]
#[ignore = "runtime benchmark; run with --ignored --nocapture"]
fn gpu_runtime() {
    let Some(mut compositor) = GpuCompositor::new(K2, K2) else {
        println!("no wgpu adapter; GPU timings unavailable");
        return;
    };
    println!("gpu runtime, offscreen compositor (release build)");
    println!();
    println!("{:<11} {:<36} {:>18}", "canvas", "operation", "time");

    compositor.reset(LAYERS, K2, K2);
    for index in 0..LAYERS {
        compositor.set_layer(index, LayerSpec::new(index).fill(colour(index)));
    }
    black_box(composite_frame(&mut compositor));
    let mut total = Duration::ZERO;
    for _ in 0..COMPOSITE_FRAMES {
        total += composite_frame(&mut compositor);
    }
    let composite = total / COMPOSITE_FRAMES as u32;
    println!(
        "{:<11} {:<36} {:>9.1} ms/frame ({} layers, {} frames)",
        "2048x2048",
        "composite + readback",
        ms(composite),
        LAYERS,
        COMPOSITE_FRAMES,
    );
    drop(compositor);

    for (label, size) in [("2048x2048", K2), ("4096x4096", K4)] {
        match stroke_timing(size, size) {
            Some((time, committed)) => println!(
                "{:<11} {:<36} {:>9.1} ms/stroke (30 segments, batch, {} rects)",
                label, "stroke commit + tile readback", time, committed,
            ),
            None => println!(
                "{:<11} {:<36} {:>18}",
                label, "stroke commit + tile readback", "unavailable"
            ),
        }
    }

    for (label, size) in [("2048x2048", K2), ("4096x4096", K4)] {
        let Some(mut compositor) = GpuCompositor::new(size, size) else {
            println!("no wgpu adapter; fill timing unavailable");
            return;
        };
        compositor.reset(1, size, size);
        let marker = [0u8; 4];
        compositor.upload_layer(RectUpload {
            index: 0,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            pixels: &marker,
        });
        let mask = disk_mask(size as usize, size as usize);
        let started = Instant::now();
        let applied = compositor.apply_fill(0, &mask, [1.0, 0.2, 0.1, 1.0], false, false);
        let elapsed = started.elapsed();
        println!(
            "{:<11} {:<36} {:>9.1} ms ({})",
            label,
            "fill apply, synthetic disk mask",
            ms(elapsed),
            if applied { "applied" } else { "no pixels" },
        );
    }

    match tile_residency() {
        Some(bytes) => println!(
            "{:<11} {:<36} {:>9.1} MiB (4 x 1024px tiles)",
            "2048x2048",
            "tile residency (public LayerStack)",
            bytes as f64 / (1024.0 * 1024.0),
        ),
        None => println!(
            "{:<11} {:<36} {:>18}",
            "2048x2048", "tile residency (public LayerStack)", "unavailable"
        ),
    }

    println!();
    println!("GpuCompositor has no resident_bytes accessor; the residency row uses the public");
    println!(
        "LayerStack it wraps. Composite and stroke rows include the readback that syncs the GPU."
    );
}

/// A full frame with many visible layers, which is the half of the comparison that Capy's tier
/// benchmarks measure and ours had no number for. The stack composites into a texture; there is
/// no readback, which is what the live present path does.
#[test]
#[ignore = "runtime benchmark; run with --ignored --nocapture"]
fn many_visible_layers() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    for (side, count) in [(K2, 32usize), (K4, 32)] {
        let mut stack = renderer.layer_stack(side, side);
        let specs: Vec<LayerSpec> = (0..count)
            .map(|layer| {
                let mut spec = LayerSpec::new(layer).fill(colour(layer));
                spec.opacity = 0.85;
                spec
            })
            .collect();
        stack.mark_all_dirty();
        let started = Instant::now();
        let first = renderer.composite_stack_texture(&mut stack, &specs);
        let full = started.elapsed();
        let started = Instant::now();
        let again = renderer.composite_stack_texture(&mut stack, &specs);
        let unchanged = started.elapsed();
        assert!(
            first.is_some() && again.is_some(),
            "the frame did not compose"
        );
        println!(
            "{side}x{side}, {count} visible fill layers: full frame {:.2} ms, unchanged {:.2} ms",
            ms(full),
            ms(unchanged)
        );
    }
}

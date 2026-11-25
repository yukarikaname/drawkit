//! Brush latency: the CPU work between one pen sample and the pixels the shell can upload.
//!
//! A stroke sample paints its segment, then the shell composites the dirty rectangle. The numbers
//! below include both, but not the GPU upload or the display present, which are platform-specific.
//! Run with `cargo run --release -p drawkit-core --example bench_brush`.

use std::time::{Duration, Instant};

use drawkit_core::surface::Rect;
use drawkit_core::tools::Editor;

fn percentile(sorted: &[Duration], fraction: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index]
}

fn report(label: &str, mut samples: Vec<Duration>) {
    samples.sort();
    let total: Duration = samples.iter().sum();
    let average = total / samples.len().max(1) as u32;
    println!(
        "{label:>10}: avg {:>7.2} ms  p50 {:>7.2}  p95 {:>7.2}  p99 {:>7.2}  max {:>7.2}",
        average.as_secs_f64() * 1000.0,
        percentile(&samples, 0.50).as_secs_f64() * 1000.0,
        percentile(&samples, 0.95).as_secs_f64() * 1000.0,
        percentile(&samples, 0.99).as_secs_f64() * 1000.0,
        percentile(&samples, 1.0).as_secs_f64() * 1000.0,
    );
}

fn measure(width: usize, height: usize, samples_per_stroke: usize, strokes: usize) {
    let mut editor = Editor::new(width, height, 1 << 30);
    let radius = 18.0;
    let color = [0.08, 0.07, 0.06, 1.0];
    let mut times = Vec::with_capacity(samples_per_stroke * strokes);
    let mut dirty_area = 0usize;
    editor.begin_stroke(0);
    for stroke in 0..strokes {
        let base = ((stroke * 37) % height) as f32;
        for sample in 0..samples_per_stroke {
            let x0 = (sample as f32 / samples_per_stroke as f32) * (width as f32 - 40.0) + 20.0;
            let x1 = x0 + 6.0;
            let y = base + ((sample * 13) % 24) as f32;
            let start = Instant::now();
            let dirty = editor.stroke_at(
                (x0, y),
                (x1, y + 2.0),
                radius,
                0.8,
                color,
                (stroke * samples_per_stroke + sample) as u64,
            );
            let _ = editor.document.composite_rect(dirty);
            times.push(start.elapsed());
            dirty_area += dirty.width * dirty.height;
        }
    }
    editor.end_stroke();
    println!(
        "{}x{}: {} samples, dirty {:.0} px per sample",
        width,
        height,
        times.len(),
        dirty_area as f64 / times.len() as f64
    );
    report("segment", times);

    let area = Rect::new(0, 0, width, height);
    let start = Instant::now();
    let _ = editor.document.composite_rect(area);
    println!(
        "{:>10}: {:>7.2} ms (full canvas)",
        "composite",
        start.elapsed().as_secs_f64() * 1000.0
    );
}

fn main() {
    measure(1920, 1080, 120, 8);
    measure(3840, 2160, 120, 8);
}

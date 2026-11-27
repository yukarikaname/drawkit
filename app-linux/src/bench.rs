//! A launch-time measurement of the Linux canvas: what one pen sample costs and what one dma-buf
//! present costs. Only runs when `DRAWKIT_BENCH_POINTER` is set.
//!
//! The macOS shell has the same bench through `DRAWKIT_BENCH_POINTER`; the numbers are not
//! comparable across the two (different window, driver and toolkit) but the shape is: painting a
//! sample must not wait for the compositor.

use std::rc::Rc;
use std::time::Instant;

use crate::canvas::Canvas;

fn report(line: &str) {
    eprintln!("DRAWKIT_LINUX_POINTER {line}");
}

fn stats(mut values: Vec<f64>) -> String {
    if values.is_empty() {
        return "none".to_string();
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let p50 = values[values.len() / 2];
    let p95 = values[(values.len() - 1).min((values.len() as f64 * 0.95) as usize)];
    format!(
        "p50 {p50:.2} ms p95 {p95:.2} ms max {:.2} ms",
        values[values.len() - 1]
    )
}

pub(crate) fn run(canvas: &Rc<Canvas>) {
    eprintln!("DRAWKIT_LINUX_POINTER bench start");
    let (width, height, ready) = {
        let state = canvas.state.borrow();
        (state.width as f32, state.height as f32, state.gpu_ready)
    };
    report(&format!(
        "canvas {width:.0}x{height:.0} gpu {ready} wayland {}",
        std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "none".into())
    ));

    // One pen sample: record the segment, which is what the drag handler does per event.
    let start = (width * 0.35, height * 0.4);
    let end = (width * 0.65, height * 0.4);
    let mut sample_ms = Vec::new();
    canvas.bench_begin_stroke(start);
    let mut last = start;
    for step in 1..=48 {
        let t = step as f32 / 48.0;
        let point = (start.0 + (end.0 - start.0) * t, start.1);
        let began = Instant::now();
        canvas.bench_segment(last, point);
        sample_ms.push(began.elapsed().as_secs_f64() * 1000.0);
        last = point;
    }
    canvas.bench_end_stroke();
    report(&format!("pen sample {}", stats(sample_ms)));

    // One frame: the dma-buf present the frame clock asks for.
    let mut present_ms = Vec::new();
    for _ in 0..24 {
        let began = Instant::now();
        canvas.bench_present();
        present_ms.push(began.elapsed().as_secs_f64() * 1000.0);
    }
    report(&format!("dmabuf present {}", stats(present_ms)));
    report(&format!("status {}", canvas.bench_status()));

    std::process::exit(0);
}

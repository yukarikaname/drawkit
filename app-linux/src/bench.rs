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

use std::cell::RefCell;

thread_local! {
    static FRAME_MS: RefCell<Vec<f64>> = RefCell::new(Vec::new());
    static READY: RefCell<bool> = RefCell::new(false);
}

/// Called by the canvas after a frame it actually drew. The bench asks for the next frame, so the
/// measurement runs at the frame clock's pace — the point of the exercise — instead of in a burst
/// that outruns GTK's buffer release.
pub(crate) fn record_present(canvas: &Rc<Canvas>, ms: f64) {
    if !READY.with(|ready| *ready.borrow()) {
        return;
    }
    let done = FRAME_MS.with(|frames| {
        let mut frames = frames.borrow_mut();
        frames.push(ms);
        frames.len() >= 30
    });
    if done {
        let frames = FRAME_MS.with(|frames| frames.borrow().clone());
        report(&format!(
            "dmabuf present (frame clock) {} of 30 drawn, {}",
            frames.len(),
            stats(frames)
        ));
        canvas.bench_report_ink();
        report(&format!("status {}", canvas.bench_status()));
        std::process::exit(0);
    }
    canvas.schedule();
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

    // The frames themselves come from the canvas's own frame clock: `record_present` keeps asking
    // for the next one so GTK can release the buffer it holds, and prints when it has enough.
    canvas.bench_set_ink_probe(((start.0 + 50.0) as usize, start.1 as usize));
    READY.with(|ready| *ready.borrow_mut() = true);
    canvas.schedule();
}

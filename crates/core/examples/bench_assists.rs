// Fill, grow/feather and prediction benchmarks: cargo run --release -p drawkit-core --example bench_assists
//
// These are the costs that decide whether an assist can run while the pen is down. The canvas is
// 4K for the flood fill; prediction is measured on 1024 because its radius is screen-sized.

use drawkit_core::fill::FillOptions;
use drawkit_core::tools::Editor;
use std::time::Instant;

fn main() {
    let mut editor = Editor::new(3840, 2160, 256 << 20);
    let ink = [0.1, 0.09, 0.08, 1.0];
    editor.stroke((40.0, 40.0), (3800.0, 40.0), 2.0, ink);
    editor.stroke((3800.0, 40.0), (3800.0, 2120.0), 2.0, ink);
    editor.stroke((3800.0, 2120.0), (40.0, 2120.0), 2.0, ink);
    editor.stroke((40.0, 2120.0), (40.0, 40.0), 2.0, ink);

    let started = Instant::now();
    let rect = editor.fill_with(
        1920,
        1080,
        0.12,
        [0.8, 0.2, 0.1, 1.0],
        FillOptions {
            close: 2,
            expand: 2,
            feather: 2,
        },
        0,
    );
    println!(
        "4K fill (close 2, grow 2, feather 2): {:?} {rect:?}",
        started.elapsed()
    );

    let started = Instant::now();
    let _ = editor.fill_with(
        200,
        200,
        0.12,
        [0.2, 0.5, 0.8, 1.0],
        FillOptions {
            close: 4,
            expand: 16,
            feather: 8,
        },
        1,
    );
    println!("4K grow 16 + feather 8: {:?}", started.elapsed());

    let mut small = Editor::new(1024, 1024, 64 << 20);
    small.prediction_enabled = true;
    let started = Instant::now();
    small.begin_stroke(0);
    for step in 0..600 {
        let x = 20.0 + step as f32 * 1.6;
        small.stroke_at(
            (x, 512.0),
            (x + 1.6, 512.0),
            8.0,
            1.0,
            [0.1, 0.09, 0.08, 1.0],
            step as u64,
        );
    }
    small.end_stroke();
    println!("600 predicted samples: {:?}", started.elapsed());
}

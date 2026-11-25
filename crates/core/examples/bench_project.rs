// A quick project open/save benchmark: cargo run --release -p drawkit-core --example bench_project
//
// The number that matters is open, because a project saved with the pixel cache should not replay
// thousands of strokes just to show the canvas.

use drawkit_core::tools::Editor;
use std::time::Instant;

fn main() {
    let width = 1024;
    let height = 1024;
    let mut editor = Editor::new(width, height, 256 << 20);
    editor.recording_enabled = true;
    let strokes = 3000;
    let started = Instant::now();
    let mut batch = Instant::now();
    for stroke in 0..strokes {
        let y = (stroke as f32 * 0.61) % height as f32;
        editor.begin_stroke(stroke as u64);
        for step in 0..8 {
            let x0 = 40.0 + step as f32 * 100.0;
            editor.stroke((x0, y), (x0 + 100.0, y), 8.0, [0.1, 0.1, 0.1, 0.5]);
        }
        editor.end_stroke();
        if (stroke + 1) % 500 == 0 {
            println!("  {} strokes: {:?}", stroke + 1, batch.elapsed());
            batch = Instant::now();
        }
    }
    println!(
        "paint {strokes} strokes / {} events: {:?}",
        editor.recording.events().len(),
        started.elapsed()
    );
    let path = std::env::temp_dir().join("drawkit-bench.dkp");
    let started = Instant::now();
    assert!(editor.save_tape(&path));
    let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    println!("save: {:?}, {bytes} bytes", started.elapsed());
    let started = Instant::now();
    let opened = Editor::open_file(&path).expect("the project opens");
    println!(
        "open: {:?}, {} events",
        started.elapsed(),
        opened.recording.events().len()
    );
    let started = Instant::now();
    for _ in 0..10 {
        let _ = opened.layer_thumbnail(0, 40, 40);
    }
    println!("10 layer thumbnails: {:?}", started.elapsed());
    // The scrub itself runs on the shell's GPU now; the core no longer owns a replay.
    let _ = std::fs::remove_file(&path);

    // A sparse 4K canvas shows what cropping the layer cache buys: the file carries one tile, not
    // thirty-three million pixels.
    let mut sparse = Editor::new(4096, 4096, 256 << 20);
    sparse.recording_enabled = true;
    sparse.begin_stroke(0);
    sparse.stroke(
        (2048.0, 2048.0),
        (2148.0, 2048.0),
        8.0,
        [0.1, 0.1, 0.1, 0.5],
    );
    sparse.end_stroke();
    let started = Instant::now();
    assert!(sparse.save_tape(&path));
    let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    println!("sparse 4K save: {:?}, {bytes} bytes", started.elapsed());
    let started = Instant::now();
    let opened = Editor::open_file(&path).expect("the sparse project opens");
    println!("sparse 4K open: {:?}", started.elapsed());
    assert!(opened.document.layers[0].surface.get(2098, 2048)[3] > 0.1);
    let _ = std::fs::remove_file(&path);
}

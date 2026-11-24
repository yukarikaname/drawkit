// How large a normal painting's `.dkp` is: the stroke tape alone, and the same tape with the
// per-layer pixel caches an explicit save writes.
//
//   cargo run --release -p drawkit-core --example bench_file_size

use drawkit_core::tools::Editor;
use std::time::Instant;

struct Scenario {
    name: &'static str,
    width: usize,
    height: usize,
    layers: usize,
    strokes: usize,
    segments: usize,
    radius: f32,
}

const PALETTE: [[f32; 4]; 6] = [
    [0.10, 0.12, 0.16, 0.9],
    [0.80, 0.30, 0.22, 0.8],
    [0.20, 0.45, 0.70, 0.7],
    [0.85, 0.70, 0.25, 0.8],
    [0.25, 0.55, 0.30, 0.7],
    [0.55, 0.25, 0.60, 0.8],
];

fn paint(editor: &mut Editor, scenario: &Scenario) {
    editor.recording_enabled = true;
    for layer in 0..scenario.layers {
        if layer > 0 {
            editor.document.add_layer(format!("paint {layer}"));
        }
        editor.document.active = layer;
        for stroke in 0..scenario.strokes {
            let t = stroke as f32;
            let y = ((t * 37.0) % scenario.height as f32).floor();
            let x0 = ((t * 91.0) % scenario.width as f32).floor();
            let length = 80.0 + (t * 13.0) % 260.0;
            let colour = PALETTE[stroke % PALETTE.len()];
            let radius = scenario.radius + (t % 5.0);
            editor.begin_stroke((layer * 1_000_000 + stroke) as u64);
            for step in 0..scenario.segments {
                let p = step as f32 / scenario.segments as f32;
                let from = (x0 + length * p, y + (p * 9.0).sin() * 6.0);
                let to = (
                    x0 + length * (p + 1.0 / scenario.segments as f32),
                    y + ((p + 0.1) * 9.0).sin() * 6.0,
                );
                editor.stroke(from, to, radius, colour);
            }
            editor.end_stroke();
        }
    }
    editor.document.active = 0;
}

fn main() {
    let scenarios = [
        Scenario {
            name: "sketch 1K, 1 layer",
            width: 1024,
            height: 1024,
            layers: 1,
            strokes: 1200,
            segments: 8,
            radius: 6.0,
        },
        Scenario {
            name: "painting 2K, 3 layers",
            width: 2048,
            height: 2048,
            layers: 3,
            strokes: 1200,
            segments: 8,
            radius: 8.0,
        },
        Scenario {
            name: "painting 4K, 2 layers",
            width: 3840,
            height: 2160,
            layers: 2,
            strokes: 1800,
            segments: 10,
            radius: 9.0,
        },
        Scenario {
            name: "finished 4K, 2 dense layers",
            width: 3840,
            height: 2160,
            layers: 2,
            strokes: 4000,
            segments: 10,
            radius: 10.0,
        },
    ];

    for scenario in scenarios {
        let mut editor = Editor::new(scenario.width, scenario.height, 512 << 20);
        let started = Instant::now();
        paint(&mut editor, &scenario);
        let painted = started.elapsed();

        let tape = editor.stroke_tape();
        let tape_bytes = drawkit_formats::write_dkp(&tape)
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        let path = std::env::temp_dir().join("drawkit-size.dkp");
        let started = Instant::now();
        assert!(editor.save_tape(&path));
        let save = started.elapsed();
        let bytes = std::fs::metadata(&path)
            .map(|meta| meta.len() as usize)
            .unwrap_or(0);
        let started = Instant::now();
        let opened = Editor::open_file(&path).expect("the project opens");
        let open = started.elapsed();
        let _ = std::fs::remove_file(&path);

        let megabytes = |value: usize| value as f64 / (1024.0 * 1024.0);
        println!("\n{}", scenario.name);
        println!(
            "  {} strokes, {} events, paint {:?}",
            scenario.layers * scenario.strokes,
            editor.recording.events().len(),
            painted
        );
        println!(
            "  tape only {:>7.2} MB   ({} bytes)",
            megabytes(tape_bytes),
            tape_bytes
        );
        println!(
            "  .dkp      {:>7.2} MB   ({} bytes, save {:?}, open {:?}, {} layers)",
            megabytes(bytes),
            bytes,
            save,
            open,
            opened.document.layers.len()
        );
    }
}

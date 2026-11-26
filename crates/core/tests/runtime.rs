//! Painting-runtime numbers, not assertions.
//!
//! Run by hand:
//!
//!   cargo test --release -p drawkit-core --test runtime -- --ignored --nocapture
//!
//! Every number is wall-clock on the machine that ran it. The stroke geometry is deterministic,
//! so two runs on the same machine are comparable.

use std::hint::black_box;
use std::time::{Duration, Instant};

use drawkit_core::fill::FillOptions;
use drawkit_core::surface::Rect;
use drawkit_core::tools::Editor;

const K2: usize = 2048;
const K4: usize = 4096;
const STROKES: usize = 200;
const SEGMENTS: usize = 30;
const MEMORY_STROKES: usize = 48;
const COMPOSITE_LAYERS: usize = 100;
const COMPOSITE_FRAMES: usize = 3;

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// Lay `strokes` small circular strokes on a grid. Each stroke is one pointer gesture of
/// `SEGMENTS` segments, so the editor records one undo step and one tape group per circle.
fn paint_strokes(editor: &mut Editor, strokes: usize, radius: f32) {
    let width = editor.document.width() as f32;
    let height = editor.document.height() as f32;
    let columns = 10.max((strokes as f32).sqrt().ceil() as usize);
    let rows = strokes.div_ceil(columns);
    let cell_width = width / columns as f32;
    let cell_height = height / rows as f32;
    let ink = [0.1, 0.1, 0.1, 1.0];
    for index in 0..strokes {
        let column = index % columns;
        let row = index / columns;
        let centre = (
            (column as f32 + 0.5) * cell_width,
            (row as f32 + 0.5) * cell_height,
        );
        let reach = cell_width.min(cell_height) * 0.3;
        let phase = index as f32 * 0.7;
        editor.begin_stroke(index as u64 * 17);
        for segment in 0..SEGMENTS {
            let from = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32 + phase;
            let to = std::f32::consts::TAU * (segment + 1) as f32 / SEGMENTS as f32 + phase;
            editor.stroke(
                (centre.0 + reach * from.cos(), centre.1 + reach * from.sin()),
                (centre.0 + reach * to.cos(), centre.1 + reach * to.sin()),
                radius,
                ink,
            );
        }
        editor.end_stroke();
    }
}

fn surface_bytes(editor: &Editor) -> usize {
    editor
        .document
        .layers
        .iter()
        .map(|layer| layer.surface.allocated_bytes())
        .sum()
}

#[test]
#[ignore = "runtime benchmark; run with --ignored --nocapture"]
fn painting_runtime() {
    println!("painting runtime, CPU raster path (release build)");
    println!();
    println!("{:<11} {:<36} {:>18}", "canvas", "operation", "time");

    let mut editor = Editor::new(K2, K2, 1 << 31);
    let started = Instant::now();
    paint_strokes(&mut editor, STROKES, 3.0);
    let painted = started.elapsed();
    let (depth, _) = editor.history.depth();
    println!(
        "{:<11} {:<36} {:>9.1} ms ({:.2} ms/stroke, {} steps)",
        "2048x2048",
        "apply 200 strokes x 30 segments",
        ms(painted),
        ms(painted) / STROKES as f64,
        depth,
    );

    let started = Instant::now();
    let mut undone = 0;
    while undone < STROKES && editor.undo().is_some() {
        undone += 1;
    }
    let undo = started.elapsed();
    println!(
        "{:<11} {:<36} {:>9.1} ms ({:.2} ms/step, {} steps)",
        "2048x2048",
        "undo all strokes",
        ms(undo),
        ms(undo) / undone.max(1) as f64,
        undone,
    );

    let started = Instant::now();
    let mut redone = 0;
    while redone < STROKES && editor.redo().is_some() {
        redone += 1;
    }
    let redo = started.elapsed();
    println!(
        "{:<11} {:<36} {:>9.1} ms ({:.2} ms/step, {} steps)",
        "2048x2048",
        "redo all strokes",
        ms(redo),
        ms(redo) / redone.max(1) as f64,
        redone,
    );
    drop(editor);

    for (label, size) in [("2048x2048", K2), ("4096x4096", K4)] {
        let mut editor = Editor::new(size, size, 1 << 31);
        let options = FillOptions {
            close: 1,
            expand: 1,
            feather: 0,
            smart: false,
        };
        let started = Instant::now();
        let filled = editor.fill_with(size / 2, size / 2, 0.1, [1.0, 0.0, 0.0, 1.0], options, 0);
        let fill = started.elapsed();
        let coverage = filled.map_or(0, |rect| rect.width * rect.height);
        println!(
            "{:<11} {:<36} {:>9.1} ms ({} px)",
            label,
            "one fill, tolerance 0.1, expand 1",
            ms(fill),
            coverage,
        );
    }

    for (label, size) in [("2048x2048", K2), ("4096x4096", K4)] {
        let mut editor = Editor::new(size, size, 1 << 31);
        paint_strokes(&mut editor, MEMORY_STROKES, 3.0);
        let bytes = surface_bytes(&editor);
        let layers = editor.document.layers.len();
        println!(
            "{:<11} {:<36} {:>9.1} MiB ({} layer)",
            label,
            "surface after 48 strokes",
            mib(bytes),
            layers,
        );
        let tape = editor.recording.tape(size, size, layers);
        match drawkit_formats::write_dkp(&tape) {
            Ok(encoded) => println!(
                "{:<11} {:<36} {:>9.1} KiB ({} events)",
                label,
                "tape for the same strokes (.dkp)",
                encoded.len() as f64 / 1024.0,
                editor.recording.events().len(),
            ),
            Err(error) => println!(
                "{:<11} {:<36} {:>18}",
                label,
                "tape for the same strokes (.dkp)",
                format!("unavailable: {error}"),
            ),
        }
    }

    let mut editor = Editor::new(1024, 1024, 1 << 20);
    for index in 0..COMPOSITE_LAYERS {
        if index > 0 {
            editor.document.add_layer(format!("layer {index}"));
        }
        let x = (index * 37 % 900) as f32 + 50.0;
        let y = (index * 53 % 900) as f32 + 50.0;
        editor.stroke((x, y), (x + 60.0, y + 30.0), 8.0, [0.2, 0.3, 0.4, 1.0]);
    }
    let rect = Rect::new(0, 0, 1024, 1024);
    black_box(editor.document.composite_rect(rect));
    let started = Instant::now();
    for _ in 0..COMPOSITE_FRAMES {
        black_box(editor.document.composite_rect(rect));
    }
    let composite = started.elapsed() / COMPOSITE_FRAMES as u32;
    println!(
        "{:<11} {:<36} {:>9.1} ms/frame ({} layers, {} frames)",
        "1024x1024",
        "composite the whole document",
        ms(composite),
        editor.document.layers.len(),
        COMPOSITE_FRAMES,
    );

    println!();
    println!(
        "timings are wall-clock; tape bytes are the encoded .dkp stream, not the in-memory Vec"
    );
}

/// A smart fill at document size: the first plan runs TEED, a second at the same revision reuses
/// the edge map. The GPU prepare path writes nothing, so both calls read the same revision.
#[test]
#[ignore = "runtime benchmark; run with --ignored --nocapture"]
fn a_smart_fill_plan_reuses_the_edge_map() {
    let side = K4;
    let mut editor = Editor::new(side, side, 1 << 32);
    let options = FillOptions {
        close: 1,
        expand: 1,
        feather: 0,
        smart: true,
    };
    let ink = [0.1, 0.1, 0.1, 1.0];
    let middle = side as f32 * 0.5;
    editor.stroke((0.0, middle), (side as f32, middle), 4.0, ink);

    let (first, bounds) = prepare_smart_fill(&mut editor, (side / 4, side / 4), 0.1, options);
    let builds = editor.fill_edge_builds();
    let (second, _) = prepare_smart_fill(&mut editor, (side / 4, side / 4), 0.1, options);
    let (third, _) = prepare_smart_fill(&mut editor, (side * 3 / 4, side / 4), 0.25, options);
    let builds_after = editor.fill_edge_builds();
    println!(
        "smart fill {side}x{side}: first {:.1} ms ({} model run), second {:.1} ms ({}) \
         same seed/tolerance, third {:.1} ms ({}) different seed/tolerance, \
         model saved {:.1} ms, edge map {:.1} MB, bounds {:?}",
        ms(first),
        builds,
        ms(second),
        builds_after,
        ms(third),
        builds_after,
        ms(first.saturating_sub(second)),
        editor.fill_edge_len() as f64 * 4.0 / (1u64 << 20) as f64,
        bounds,
    );
    assert_eq!(builds, 1, "the first smart fill must run the model");
    assert_eq!(
        builds_after, 1,
        "a later fill at the same revision re-ran the model"
    );
}

fn prepare_smart_fill(
    editor: &mut Editor,
    seed: (usize, usize),
    tolerance: f32,
    options: FillOptions,
) -> (Duration, Rect) {
    let started = Instant::now();
    let bounds = editor
        .begin_gpu_fill(seed.0, seed.1, tolerance, [1.0, 0.0, 0.0, 1.0], options, 0)
        .expect("a fill plan");
    let elapsed = started.elapsed();
    editor.cancel_gpu_fill();
    (elapsed, bounds)
}

/// What the fill plan costs alone at document size. The GPU blend is measured in the gpu crate;
/// this is the CPU mask a shell waits for before it can upload anything.
#[test]
#[ignore]
fn a_fill_plan_at_document_size() {
    for side in [K2, K4] {
        let mut editor = Editor::new(side, side, 1 << 32);
        let started = Instant::now();
        let bounds = editor
            .begin_gpu_fill(
                side / 2,
                side / 2,
                0.1,
                [1.0, 0.0, 0.0, 1.0],
                FillOptions::default(),
                0,
            )
            .expect("a fill plan");
        let plan = started.elapsed();
        let started = Instant::now();
        let mask = editor.gpu_fill_mask().expect("a mask").to_vec();
        let copy = started.elapsed();
        let covered = mask.iter().filter(|value| **value > 0.0).count();
        black_box(&mask);
        println!(
            "fill plan {side}x{side}: bounds {:?}, plan {:.1} ms, mask copy {:.1} ms, \
             covered {covered} px, {:.1} MB",
            bounds,
            plan.as_secs_f64() * 1000.0,
            copy.as_secs_f64() * 1000.0,
            mask.len() as f64 * 4.0 / (1u64 << 20) as f64,
        );
    }
}

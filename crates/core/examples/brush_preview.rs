//! Side-by-side brush preview: the old per-segment composite on the left, the live stroke on the
//! right. Used to inspect stamping artifacts without launching the app.
//!
//! cargo run --release -p drawkit-core --example brush_preview

use std::path::Path;

use drawkit_core::brush::{self, Brush, BrushKind};
use drawkit_core::tools::Editor;

const WIDTH: usize = 560;
const HEIGHT: usize = 720;
const ROWS: usize = 9;

fn paint(editor: &mut Editor, kind: BrushKind, y: f32, live: bool) {
    let pen = Brush::for_kind(kind);
    let color = [0.05, 0.05, 0.05, 1.0];
    if live {
        editor.brush = pen;
        editor.begin_stroke(0);
    }
    for step in 0..24 {
        let x = 30.0 + step as f32 * 20.0;
        if live {
            editor.stroke_at((x, y), (x + 20.0, y), 16.0, 0.9, color, step as u64);
        } else {
            let surface = &mut editor.document.layers[0].surface;
            brush::stroke_with(surface, (x, y), (x + 20.0, y), 16.0, &pen, color, 7);
        }
    }
    if live {
        editor.end_stroke();
    }
}

fn write_png(path: &Path, width: usize, height: usize, pixels: &[[f32; 4]]) -> bool {
    let Ok(file) = std::fs::File::create(path) else {
        return false;
    };
    let mut encoder = png::Encoder::new(file, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let Ok(mut writer) = encoder.write_header() else {
        return false;
    };
    let mut bytes = Vec::with_capacity(width * height * 4);
    for pixel in pixels {
        let alpha = pixel[3].clamp(0.0, 1.0);
        for channel in &pixel[..3] {
            bytes.push((channel.clamp(0.0, 1.0) * alpha * 255.0).round() as u8);
        }
        bytes.push((alpha * 255.0).round() as u8);
    }
    writer.write_image_data(&bytes).is_ok()
}

fn main() {
    let mut old = Editor::new(WIDTH, HEIGHT, 512 << 20);
    let mut new = Editor::new(WIDTH, HEIGHT, 512 << 20);
    for (index, kind) in BrushKind::all().iter().enumerate() {
        let y = 50.0 + index as f32 * (HEIGHT as f32 - 80.0) / ROWS as f32;
        paint(&mut old, *kind, y, false);
        paint(&mut new, *kind, y, true);
    }
    let mut combined = vec![[0.0; 4]; WIDTH * 2 * HEIGHT];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            combined[y * WIDTH * 2 + x] = old.document.layers[0].surface.get(x, y);
            combined[y * WIDTH * 2 + WIDTH + x] = new.document.layers[0].surface.get(x, y);
        }
    }
    let path = Path::new("/tmp/brush-preview.png");
    println!(
        "wrote {}: {}",
        path.display(),
        write_png(path, WIDTH * 2, HEIGHT, &combined)
    );
}

use super::super::*;

#[test]
fn splitting_a_live_stroke_does_not_darken_the_join() {
    let mut one = Editor::new(128, 64, 1 << 20);
    let mut two = Editor::new(128, 64, 1 << 20);
    for editor in [&mut one, &mut two] {
        editor.prediction_enabled = false;
        editor.stroke_dedupe = false;
        editor.brush = crate::brush::Brush {
            hardness: 0.7,
            flow: 0.5,
            spacing: 0.35,
            ..crate::brush::Brush::soft_round()
        };
        editor.begin_stroke(0);
    }
    one.stroke_at(
        (8.0, 32.0),
        (120.0, 32.0),
        8.0,
        1.0,
        [0.0, 0.0, 0.0, 1.0],
        0,
    );
    two.stroke_at((8.0, 32.0), (64.0, 32.0), 8.0, 1.0, [0.0, 0.0, 0.0, 1.0], 0);
    two.stroke_at(
        (64.0, 32.0),
        (120.0, 32.0),
        8.0,
        1.0,
        [0.0, 0.0, 0.0, 1.0],
        1,
    );
    one.end_stroke();
    two.end_stroke();
    let mut worst: f32 = 0.0;
    for y in 20..44 {
        for x in 8..120 {
            worst = worst.max(
                (one.document.layers[0].surface.get(x, y)[3]
                    - two.document.layers[0].surface.get(x, y)[3])
                    .abs(),
            );
        }
    }
    assert!(worst < 0.02, "the live join darkened by {worst}");
}

#[test]
fn a_custom_brush_survives_the_project() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.set_brush(crate::brush::BrushKind::Marker);
    editor.set_brush_param(1, 0.42);
    editor.set_brush_param(4, 0.18);
    editor.begin_stroke(0);
    editor.stroke_at((8.0, 32.0), (56.0, 32.0), 6.0, 1.0, [0.0, 0.0, 0.0, 1.0], 0);
    editor.end_stroke();
    let path = std::env::temp_dir().join("drawkit-custom-brush.dkp");
    assert!(editor.save_tape(&path));
    let opened = Editor::open_file(&path).expect("open");
    let _ = std::fs::remove_file(&path);
    let marker = opened.brushes[crate::brush::BrushKind::Marker.as_i32() as usize];
    assert!((marker.flow - 0.42).abs() < 1e-4);
    assert!((marker.spacing - 0.18).abs() < 1e-4);
}

#[test]
fn resetting_a_brush_restores_its_preset_slot() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.set_brush(crate::brush::BrushKind::Ink);
    editor.set_brush_param(1, 0.3);
    let factory = crate::brush::Brush::for_kind(crate::brush::BrushKind::Ink).flow;
    editor.set_brush(crate::brush::BrushKind::Ink);
    assert!((editor.brush.flow - 0.3).abs() < 1e-4);
    let slot = crate::brush::BrushKind::Ink.as_i32() as usize;
    editor.brushes[slot] = crate::brush::Brush::for_kind(crate::brush::BrushKind::Ink);
    editor.set_brush(crate::brush::BrushKind::Ink);
    assert!((editor.brush.flow - factory).abs() < 1e-4);
}

#[test]
fn a_brush_texture_survives_the_project() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    let texture = crate::brush::BrushTexture::new(4, 4, vec![10; 16]).expect("texture");
    editor.brush_texture = Some(std::sync::Arc::new(texture));
    let path = std::env::temp_dir().join("drawkit-brush-texture.dkp");
    assert!(editor.save_tape(&path));
    let opened = Editor::open_file(&path).expect("open");
    let _ = std::fs::remove_file(&path);
    let texture = opened.brush_texture.expect("texture");
    assert_eq!((texture.width, texture.height), (4, 4));
    assert_eq!(texture.data[0], 10);
}

#[test]
fn a_deferred_stroke_rasterises_once_at_pen_up() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.defer_raster = true;
    editor.begin_stroke(0);
    editor.stroke_at((8.0, 32.0), (56.0, 32.0), 5.0, 1.0, [0.0, 0.0, 0.0, 1.0], 0);
    assert_eq!(editor.document.layers[0].surface.get(32, 32), [0.0; 4]);
    editor.end_stroke();
    assert!(editor.deferred_pending());
    assert_eq!(editor.document.layers[0].surface.get(32, 32), [0.0; 4]);
    while !editor.advance_deferred(8) {}
    assert!(editor.document.layers[0].surface.get(32, 32)[3] > 0.0);
    assert_eq!(editor.history.depth().0, 1);
    editor.undo();
    assert_eq!(editor.document.layers[0].surface.get(32, 32), [0.0; 4]);
}

#[test]
fn gpu_tiles_land_as_one_history_step() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.brush = crate::brush::Brush::soft_round();
    editor.defer_raster = true;
    editor.begin_stroke(0);
    editor.stroke_at((8.0, 32.0), (56.0, 32.0), 6.0, 1.0, [1.0, 0.0, 0.0, 1.0], 0);
    editor.end_stroke();
    assert!(editor.begin_tile_commit());
    // The commit takes the stroke without rasterising it; only the GPU tiles have pixels.
    assert_eq!(editor.document.layers[0].surface.get(32, 32), [0.0; 4]);
    let rect = crate::surface::Rect::new(10, 28, 16, 8);
    assert!(editor.end_gpu_commit(&[rect]));
    assert!(editor.undo_is_gpu());
    assert_eq!(editor.history.depth().0, 1);
    // Undo only moves the marker; the pixels live in the GPU journal.
    assert!(editor.undo().is_some());
    assert_eq!(editor.document.layers[0].surface.get(12, 30), [0.0; 4]);
    // Materialising writes the cache without pushing another step.
    let pixels = vec![255_u8; 16 * 8 * 4];
    assert!(editor.apply_tiles(0, rect, &pixels));
    let pixel = editor.document.layers[0].surface.get(12, 30);
    assert!((pixel[0] - 1.0).abs() < 1e-3);
    assert_eq!(editor.history.depth().0, 0);
}

#[test]
fn linear_tiles_return_through_the_colour_space() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.document.color_space = crate::document::ColorSpace::Srgb;
    let rect = crate::surface::Rect::new(2, 2, 4, 4);
    // Straight linear Display P3; the srgb matrix keeps a neutral close to itself.
    let pixels = vec![0.5f32; 4 * 4 * 4];
    assert!(editor.apply_tiles_linear(0, rect, &pixels));
    let pixel = editor.document.layers[0].surface.get(3, 3);
    let expected = crate::document::ColorSpace::Srgb.encode(0.5);
    assert!((pixel[0] - expected).abs() < 1e-3, "red {}", pixel[0]);
    assert!((pixel[3] - 0.5).abs() < 1e-3, "alpha {}", pixel[3]);
}

#[test]
fn a_deferred_smudge_records_without_touching_the_surface() {
    let mut editor = Editor::new(32, 32, 1 << 20);
    editor.document.layers[0]
        .surface
        .set(16, 16, [1.0, 0.0, 0.0, 1.0]);
    editor.defer_raster = true;
    editor.begin_stroke(0);
    let full = crate::surface::Rect::new(0, 0, 32, 32);
    let before = editor.document.layers[0].surface.snapshot(full);
    editor.smudge_at((10.0, 16.0), (22.0, 16.0), 5.0, 1.0, 1.0, 0);
    assert_eq!(
        editor.document.layers[0].surface.snapshot(full),
        before,
        "the deferred smudge rasterised on the CPU"
    );
    assert_eq!(editor.recording.events().len(), 1);
    editor.end_stroke();
    assert!(!editor.deferred_pending());
    assert_eq!(editor.history.depth().0, 0);
}

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
fn an_erase_replays_like_the_original() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.recording_enabled = true;
    editor.begin_stroke(100);
    editor.stroke_at(
        (8.0, 32.0),
        (56.0, 32.0),
        4.0,
        1.0,
        [0.0, 0.0, 0.0, 1.0],
        100,
    );
    editor.erase_at((20.0, 32.0), (44.0, 32.0), 4.0, 1.0, 200);
    editor.end_stroke();
    assert_eq!(editor.recording.events().len(), 2);
    let replayed = editor.replay(300);
    for y in (0..64).step_by(5) {
        for x in (0..64).step_by(5) {
            assert_eq!(
                editor.document.layers[0].surface.get(x, y),
                replayed.document.layers[0].surface.get(x, y),
                "differs at {x},{y}"
            );
        }
    }
    editor.undo();
    assert!(editor.recording.is_empty(), "the erase stayed on the tape");
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

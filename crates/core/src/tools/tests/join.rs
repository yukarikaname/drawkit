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

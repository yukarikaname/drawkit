//! The GPU fill plan: the mask is built here and applied by the shell's tiles.

use super::*;

#[test]
fn a_gpu_fill_builds_the_mask_without_writing_pixels() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    let before = editor.document.layers[0].surface.get(4, 4);
    let bounds = editor
        .begin_gpu_fill(4, 4, 0.1, [1.0, 0.0, 0.0, 1.0], FillOptions::default(), 0)
        .expect("the mask around a transparent seed");
    assert_eq!(bounds, Rect::new(0, 0, 16, 16));
    assert_eq!(
        editor.document.layers[0].surface.get(4, 4),
        before,
        "the core wrote a pixel it promised the surface would write"
    );
    let mask = editor.gpu_fill_mask().expect("a prepared mask");
    assert_eq!(mask.len(), 16 * 16);
    assert!(mask[4 * 16 + 4] > 0.0, "the seed is not covered");
    let (layer, state, color, under, alpha_locked) =
        editor.gpu_fill_state().expect("a prepared state");
    assert_eq!(layer, 0);
    assert_eq!(state, bounds);
    assert_eq!(color, [1.0, 0.0, 0.0, 1.0]);
    // The seed is on an empty layer, so the fill goes under the line it may meet.
    assert!(under);
    assert!(!alpha_locked);
}

#[test]
fn a_gpu_fill_is_one_gpu_undo_step_and_one_tape_event() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    editor
        .begin_gpu_fill(4, 4, 0.1, [1.0, 0.0, 0.0, 1.0], FillOptions::default(), 7)
        .expect("a mask");
    let rects = [Rect::new(0, 0, 16, 16)];
    assert!(editor.finish_gpu_fill(&rects));
    assert!(editor.undo_is_gpu(), "the fill must undo through the tiles");
    assert_eq!(editor.history.depth(), (1, 0));
    assert_eq!(editor.recording.tape(16, 16, 1).events.len(), 1);

    editor.undo().expect("an undo step");
    assert_eq!(editor.history.depth(), (0, 1));
    assert_eq!(
        editor.recording.tape(16, 16, 1).events.len(),
        0,
        "the tape kept a fill the painter took back"
    );
    assert!(editor.redo_is_gpu());
    editor.redo().expect("a redo step");
    assert_eq!(editor.history.depth(), (1, 0));
    // The tape half of the undo comes back with it.
    assert_eq!(editor.recording.tape(16, 16, 1).events.len(), 1);

    // A surface that wrote nothing is not an undo step either.
    editor
        .begin_gpu_fill(4, 4, 0.1, [1.0, 0.0, 0.0, 1.0], FillOptions::default(), 8)
        .expect("a mask");
    assert!(!editor.finish_gpu_fill(&[]));
    assert_eq!(editor.history.depth(), (1, 0));
    assert_eq!(editor.recording.tape(16, 16, 1).events.len(), 1);
}

#[test]
fn cancelling_a_gpu_fill_leaves_no_step() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    editor
        .begin_gpu_fill(4, 4, 0.1, [1.0, 0.0, 0.0, 1.0], FillOptions::default(), 0)
        .expect("a mask");
    assert!(editor.cancel_gpu_fill());
    assert!(!editor.fill_animation_active());
    assert_eq!(editor.history.depth(), (0, 0));
    assert_eq!(editor.recording.tape(16, 16, 1).events.len(), 0);
    // The CPU animation path does not see a GPU plan as a pending fill.
    assert!(!editor.finish_fill_animation());
}

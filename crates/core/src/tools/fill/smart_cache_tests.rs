//! The smart fill edge cache: one model run per document revision.

use super::*;

fn smart_options() -> FillOptions {
    FillOptions {
        close: 1,
        expand: 1,
        feather: 0,
        smart: true,
    }
}

/// A vertical line splits the canvas so a fill has a barrier and a region to reach.
fn paint_line(editor: &mut Editor, side: usize) {
    let middle = side as f32 * 0.5;
    editor.stroke(
        (middle, 0.0),
        (middle, side as f32),
        1.0,
        [0.0, 0.0, 0.0, 1.0],
    );
}

#[test]
fn a_cached_mask_matches_the_uncached_plan() {
    let side = 24;
    let mut editor = Editor::new(side, side, 1 << 20);
    paint_line(&mut editor, side);
    let options = smart_options();
    let rect = Rect::new(0, 0, side, side);
    let composite = editor.document.composite_rect(rect);
    let under = editor.document.layers[0].surface.get(4, 4)[3] < 0.5;
    let reference =
        crate::fill::mask_for_flat(&composite, (side, side), (4, 4), 0.1, options, under)
            .expect("an uncached mask");
    assert_eq!(
        editor.fill_edge_builds(),
        0,
        "the uncached plan must not touch the editor cache"
    );

    editor
        .begin_gpu_fill(4, 4, 0.1, [1.0, 0.0, 0.0, 1.0], options, 0)
        .expect("a cached mask");
    let cached = editor.gpu_fill_mask().expect("a prepared mask");
    assert_eq!(
        cached,
        reference.data.as_slice(),
        "the cached edge map changed the mask"
    );
}

#[test]
fn a_second_fill_at_the_same_revision_reuses_the_edges() {
    let side = 24;
    let mut editor = Editor::new(side, side, 1 << 20);
    paint_line(&mut editor, side);
    let options = smart_options();
    editor
        .begin_gpu_fill(4, 4, 0.1, [1.0, 0.0, 0.0, 1.0], options, 0)
        .expect("the first fill");
    assert_eq!(
        editor.fill_edge_builds(),
        1,
        "the first fill runs the model"
    );
    assert_eq!(editor.fill_edge_len(), side * side, "one canvas-sized map");
    editor.cancel_gpu_fill();

    // A different seed and tolerance, the same picture: still one model run.
    editor
        .begin_gpu_fill(side - 4, 4, 0.25, [1.0, 0.0, 0.0, 1.0], options, 0)
        .expect("the second fill");
    assert_eq!(
        editor.fill_edge_builds(),
        1,
        "a different seed or tolerance re-ran the model"
    );
    assert_eq!(
        editor.fill_edge_len(),
        side * side,
        "the cache grew a second map"
    );
}

#[test]
fn a_stroke_between_fills_rebuilds_the_edges() {
    let side = 24;
    let mut editor = Editor::new(side, side, 1 << 20);
    paint_line(&mut editor, side);
    let options = smart_options();
    editor
        .begin_gpu_fill(4, 4, 0.1, [1.0, 0.0, 0.0, 1.0], options, 0)
        .expect("the first fill");
    assert_eq!(editor.fill_edge_builds(), 1);
    editor.cancel_gpu_fill();

    editor.stroke(
        (1.0, 20.0),
        ((side - 1) as f32, 20.0),
        1.0,
        [0.0, 0.0, 0.0, 1.0],
    );
    editor
        .begin_gpu_fill(4, 4, 0.1, [1.0, 0.0, 0.0, 1.0], options, 0)
        .expect("the second fill");
    assert_eq!(
        editor.fill_edge_builds(),
        2,
        "a paint write did not invalidate the edge map"
    );
}

#[test]
fn a_layer_change_between_fills_rebuilds_the_edges() {
    let side = 24;
    let mut editor = Editor::new(side, side, 1 << 20);
    paint_line(&mut editor, side);
    let options = smart_options();
    editor
        .begin_gpu_fill(4, 4, 0.1, [1.0, 0.0, 0.0, 1.0], options, 0)
        .expect("the first fill");
    assert_eq!(editor.fill_edge_builds(), 1);
    editor.cancel_gpu_fill();

    editor.document.layers[0].visible = false;
    editor
        .begin_gpu_fill(4, 4, 0.1, [1.0, 0.0, 0.0, 1.0], options, 0)
        .expect("the second fill");
    assert_eq!(
        editor.fill_edge_builds(),
        2,
        "a layer change did not invalidate the edge map"
    );
}

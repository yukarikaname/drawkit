use super::super::*;

#[test]
fn a_stroke_is_one_undo_step_and_undoes_cleanly() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.stroke((10.0, 10.0), (40.0, 40.0), 3.0, [0.0, 0.0, 0.0, 1.0]);
    assert!(editor.document.layers[0].surface.get(25, 25)[3] > 0.0);
    editor.undo();
    assert_eq!(editor.document.layers[0].surface.get(25, 25), [0.0; 4]);
}

#[test]
fn a_thumbnail_averages_in_premultiplied_space() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.document.layers[0]
        .surface
        .set(32, 32, [1.0, 0.0, 0.0, 1.0]);
    let thumbnail = editor.layer_thumbnail(0, 8, 8).expect("a thumbnail");
    let hit = thumbnail[4 * 8 + 4];
    assert!(
        hit[0] > 0.0 && hit[3] > 0.0,
        "the painted block is empty: {hit:?}"
    );
    assert_eq!(thumbnail[0][3], 0.0, "an empty block gained opacity");
    assert!(editor.layer_thumbnail(9, 8, 8).is_none());
}

#[test]
fn horizontal_mirror_paints_both_sides_as_one_step() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    editor.recording_enabled = true;
    editor.set_mirror(Mirror {
        enabled: true,
        angle: std::f32::consts::FRAC_PI_2,
        both: false,
    });
    editor.begin_stroke(0);
    editor.stroke((2.0, 8.0), (4.0, 8.0), 1.0, [1.0, 0.0, 0.0, 1.0]);
    editor.end_stroke();
    assert!(editor.document.layers[0].surface.get(2, 8)[0] > 0.8);
    assert!(editor.document.layers[0].surface.get(13, 8)[0] > 0.8);
    assert_eq!(
        editor.history.depth().0,
        1,
        "the mirrored pair became two undo steps"
    );
    assert_eq!(editor.stroke_tape().events.len(), 2);
}

#[test]
fn both_mirrors_make_four_strokes() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    editor.set_mirror(Mirror {
        enabled: true,
        angle: 0.0,
        both: true,
    });
    editor.begin_stroke(0);
    editor.stroke((2.0, 3.0), (4.0, 3.0), 1.0, [0.0, 0.0, 1.0, 1.0]);
    editor.end_stroke();
    for (x, y) in [(2, 3), (13, 3), (2, 12), (13, 12)] {
        assert!(
            editor.document.layers[0].surface.get(x, y)[2] > 0.8,
            "missing {x},{y}"
        );
    }
}

#[test]
fn a_diagonal_axis_mirrors_across_the_rotated_line() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    editor.set_mirror(Mirror {
        enabled: true,
        angle: std::f32::consts::FRAC_PI_4,
        both: false,
    });
    editor.begin_stroke(0);
    editor.stroke((2.0, 8.0), (2.0, 8.0), 1.0, [0.0, 0.0, 1.0, 1.0]);
    editor.end_stroke();
    assert!(editor.document.layers[0].surface.get(8, 2)[2] > 0.8);
}

#[test]
fn fill_growth_reaches_under_the_line() {
    let mut editor = Editor::new(32, 32, 1 << 20);
    editor.stroke((16.0, 0.0), (16.0, 32.0), 1.0, [0.0, 0.0, 0.0, 1.0]);
    let options = crate::fill::FillOptions {
        close: 1,
        expand: 0,
        feather: 0,
    };
    editor.fill_with(8, 16, 0.1, [1.0, 0.0, 0.0, 1.0], options, 0);
    let plain = editor.document.layers[0].surface.get(18, 16);
    assert!(plain[0] < 0.1, "expand 0 crossed the line: {plain:?}");

    let mut expanded = Editor::new(32, 32, 1 << 20);
    expanded.stroke((16.0, 0.0), (16.0, 32.0), 1.0, [0.0, 0.0, 0.0, 1.0]);
    let options = crate::fill::FillOptions {
        close: 1,
        expand: 4,
        feather: 0,
    };
    expanded.fill_with(8, 16, 0.1, [1.0, 0.0, 0.0, 1.0], options, 0);
    let grown = expanded.document.layers[0].surface.get(18, 16);
    assert!(grown[0] > 0.5, "expand 4 did not reach the line: {grown:?}");
}

#[test]
fn importing_fits_the_document_and_restarts_history() {
    let mut editor = Editor::new(8, 8, 64 * 1024 * 1024);
    let source = drawkit_formats::ImportedDocument {
        width: 4,
        height: 2,
        color_space: 0,
        layers: vec![
            drawkit_formats::ImportedLayer {
                name: "background".into(),
                visible: true,
                opacity: 1.0,
                x: 0,
                y: 0,
                width: 4,
                height: 2,
                rgba: [255, 0, 0, 255].repeat(8),
            },
            drawkit_formats::ImportedLayer {
                name: "ink".into(),
                visible: false,
                opacity: 0.5,
                x: 0,
                y: 0,
                width: 4,
                height: 2,
                rgba: [0, 0, 0, 0].repeat(8),
            },
        ],
    };
    assert!(editor.import_document(&source));
    assert_eq!(editor.document.layers.len(), 2);
    assert_eq!(editor.document.layers[0].name, "background");
    assert_eq!(editor.document.layers[1].name, "ink");
    assert!(!editor.document.layers[1].visible);
    assert_eq!(editor.document.active, 1);
    assert!(editor.history.depth().0 == 0, "import left history behind");
    // 4×2 fits into 8×8 at 2× with a two-row letterbox.
    assert_eq!(
        editor.document.layers[0].surface.get(0, 2),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        editor.document.layers[0].surface.get(0, 0),
        [0.0, 0.0, 0.0, 0.0]
    );
}

#[test]
fn a_premultiplied_image_becomes_a_straight_alpha_layer() {
    let mut editor = Editor::new(2, 2, 1 << 20);
    let pixels = [255, 0, 0, 255, 0, 0, 0, 0, 128, 0, 0, 128, 0, 0, 0, 0];
    assert!(editor.add_image_layer(&pixels, 2, 2));
    assert_eq!(editor.document.layers.len(), 2);
    let surface = &editor.document.layers[editor.document.active].surface;
    assert_eq!(surface.get(0, 0), [1.0, 0.0, 0.0, 1.0]);
    let half = surface.get(0, 1);
    assert!(
        (half[0] - 1.0).abs() < 1e-3 && (half[3] - 128.0 / 255.0).abs() < 1e-3,
        "half-covered red became {half:?}"
    );
    assert_eq!(
        surface.get(1, 0),
        [0.0; 4],
        "a transparent pixel was stored"
    );
}

#[test]
fn picking_reads_the_composite_over_the_paper() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    assert_eq!(editor.pick(0, 0), Some(PAPER));
    editor.stroke((0.0, 8.0), (16.0, 8.0), 4.0, [0.0, 0.0, 0.0, 1.0]);
    let picked = editor.pick(8, 8).expect("a pixel in the canvas");
    assert!(picked[0] < 0.1 && picked[3] > 0.99, "picked {picked:?}");
    assert_eq!(editor.pick(16, 16), None, "an out-of-canvas pick answered");
}

#[test]
fn picking_an_hdr_pixel_keeps_the_highlight() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.document.dynamic_range = crate::document::DynamicRange::Hdr;
    editor
        .document
        .active_mut()
        .surface
        .set(1, 1, [2.0, 0.5, 0.25, 1.0]);
    let picked = editor.pick(1, 1).expect("a pixel in the canvas");
    assert!(picked[0] > 1.9, "picked {picked:?}");
    assert!((picked[1] - 0.5).abs() < 1e-4);
}

#[test]
fn erasing_clears_paint_and_one_undo_restores_it() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.begin_stroke(0);
    editor.stroke((8.0, 32.0), (56.0, 32.0), 4.0, [0.0, 0.0, 0.0, 1.0]);
    editor.erase((20.0, 32.0), (44.0, 32.0), 4.0, 1.0);
    editor.end_stroke();
    assert!(editor.document.layers[0].surface.get(32, 32)[3] < 0.1);
    assert_eq!(
        editor.history.depth().0,
        1,
        "erase and stroke became separate steps"
    );
    editor.undo();
    assert_eq!(editor.document.layers[0].surface.get(32, 32), [0.0; 4]);
    editor.redo();
    assert!(editor.document.layers[0].surface.get(32, 32)[3] < 0.1);
}

#[test]
fn stabilization_trails_a_fast_corner_instead_of_reaching_it() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.set_stabilization(0.9);
    editor.begin_stroke(0);
    editor.stroke((10.0, 10.0), (20.0, 10.0), 2.0, [0.0, 0.0, 0.0, 1.0]);
    editor.stroke((20.0, 10.0), (20.0, 20.0), 2.0, [0.0, 0.0, 0.0, 1.0]);
    editor.end_stroke();
    assert_eq!(
        editor.document.layers[0].surface.get(20, 20),
        [0.0; 4],
        "the ink jumped to the corner despite the stabiliser"
    );
    assert!(editor.document.layers[0].surface.get(11, 10)[3] > 0.5);
}

#[test]
fn a_stabilized_stroke_replays_the_line_that_was_painted() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.recording_enabled = true;
    editor.set_stabilization(0.8);
    editor.begin_stroke(100);
    editor.stroke_at(
        (10.0, 10.0),
        (20.0, 12.0),
        2.0,
        1.0,
        [0.0, 0.0, 0.0, 1.0],
        100,
    );
    editor.stroke_at(
        (20.0, 12.0),
        (40.0, 30.0),
        2.0,
        1.0,
        [0.0, 0.0, 0.0, 1.0],
        120,
    );
    editor.end_stroke();
    let replayed = editor.replay(200);
    for y in (0..64).step_by(5) {
        for x in (0..64).step_by(5) {
            assert_eq!(
                editor.document.layers[0].surface.get(x, y),
                replayed.document.layers[0].surface.get(x, y),
                "differs at {x},{y}"
            );
        }
    }
}

#[test]
fn a_pointer_stroke_is_one_undo_step() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.begin_stroke(10);
    editor.stroke((5.0, 5.0), (20.0, 8.0), 2.0, [0.0, 0.0, 0.0, 1.0]);
    editor.stroke((20.0, 8.0), (40.0, 20.0), 2.0, [0.0, 0.0, 0.0, 1.0]);
    editor.stroke((40.0, 20.0), (50.0, 50.0), 2.0, [0.0, 0.0, 0.0, 1.0]);
    editor.end_stroke();
    assert_eq!(
        editor.history.depth().0,
        1,
        "segments became separate undo steps"
    );
    editor.undo();
    for (x, y) in [(5, 5), (20, 8), (40, 20), (50, 50)] {
        assert_eq!(
            editor.document.layers[0].surface.get(x, y),
            [0.0; 4],
            "undo left paint at {x},{y}"
        );
    }
    editor.redo();
    assert!(editor.document.layers[0].surface.get(40, 20)[3] > 0.0);
}

#[test]
fn undoing_a_stroke_removes_all_of_its_tape_segments() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.recording_enabled = true;
    editor.begin_stroke(10);
    editor.stroke((5.0, 5.0), (20.0, 8.0), 2.0, [0.0, 0.0, 0.0, 1.0]);
    editor.stroke((20.0, 8.0), (40.0, 20.0), 2.0, [0.0, 0.0, 0.0, 1.0]);
    editor.end_stroke();
    assert_eq!(editor.recording.events().len(), 2);
    editor.undo();
    assert!(editor.recording.is_empty(), "a segment stayed on the tape");
    assert_eq!(
        editor.replay(1_000).document.layers[0].surface.get(20, 8),
        [0.0; 4]
    );
}

#[test]
fn a_stroke_reports_only_the_rectangle_it_touched() {
    let mut editor = Editor::new(256, 256, 1 << 20);
    editor.stroke((40.0, 40.0), (60.0, 60.0), 3.0, [0.0, 0.0, 0.0, 1.0]);
    let dirty = editor
        .document
        .take_dirty()
        .expect("the stroke dirtied nothing");
    assert!(
        dirty.x >= 30 && dirty.y >= 30,
        "dirty {dirty:?} is wider than the stroke"
    );
    assert!(
        dirty.x + dirty.width <= 80 && dirty.y + dirty.height <= 80,
        "dirty {dirty:?} is wider than the stroke"
    );
    assert!(
        editor.document.take_dirty().is_none(),
        "the rectangle was not consumed"
    );
}

#[test]
fn filling_a_ring_does_not_leak_outside_it() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    for (a, b) in [
        ((16.0, 16.0), (48.0, 16.0)),
        ((48.0, 16.0), (48.0, 48.0)),
        ((48.0, 48.0), (16.0, 48.0)),
        ((16.0, 48.0), (16.0, 16.0)),
    ] {
        editor.stroke(a, b, 1.5, [0.0, 0.0, 0.0, 1.0]);
    }
    editor.fill(32, 32, 0.2, [1.0, 0.0, 0.0, 1.0]);
    let surface = &editor.document.layers[0].surface;
    assert!(surface.get(32, 32)[0] > 0.9, "the inside was not filled");
    assert_eq!(surface.get(2, 2), [0.0; 4], "the fill escaped the outline");
}

#[test]
fn a_one_pixel_gap_in_the_outline_does_not_drain_the_fill() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    for (a, b) in [
        ((16.0, 16.0), (48.0, 16.0)),
        ((48.0, 16.0), (48.0, 48.0)),
        ((48.0, 48.0), (16.0, 48.0)),
    ] {
        editor.stroke(a, b, 1.5, [0.0, 0.0, 0.0, 1.0]);
    }
    // The left side stops short on both ends, leaving gaps a pixel wide.
    editor.stroke((16.0, 18.0), (16.0, 46.0), 1.5, [0.0, 0.0, 0.0, 1.0]);
    editor.fill(32, 32, 0.2, [1.0, 0.0, 0.0, 1.0]);
    let surface = &editor.document.layers[0].surface;
    assert!(surface.get(32, 32)[0] > 0.9, "the inside was not filled");
    assert_eq!(
        surface.get(2, 2),
        [0.0; 4],
        "the fill drained out through the gaps"
    );
}

#[test]
fn a_fill_reaches_the_line_instead_of_stopping_short() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    // One vertical line with a soft (anti-aliased) edge.
    editor.stroke((32.0, 0.0), (32.0, 64.0), 1.0, [0.0, 0.0, 0.0, 1.0]);
    editor.fill(8, 32, 0.2, [1.0, 0.0, 0.0, 1.0]);
    let surface = &editor.document.layers[0].surface;
    // The pixels just before the line must be painted, not left as paper.
    let near = surface.get(30, 32);
    assert!(near[0] > 0.5 && near[3] > 0.5, "left a rim: {near:?}");
}

#[test]
fn a_recording_replays_to_the_same_pixels() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.recording_enabled = true;
    editor.stroke_at(
        (4.0, 64.0),
        (60.0, 4.0),
        2.0,
        1.0,
        [0.0, 0.0, 0.0, 1.0],
        100,
    );
    editor.fill_at(32, 32, 0.2, [0.0, 0.0, 1.0, 1.0], 200);
    let replayed = editor.replay(200);
    for y in (0..64).step_by(7) {
        for x in (0..64).step_by(7) {
            assert_eq!(
                editor.document.layers[0].surface.get(x, y),
                replayed.document.layers[0].surface.get(x, y),
                "differs at {x},{y}"
            );
        }
    }
}

#[test]
fn a_thumbnail_keeps_the_painted_area() {
    let mut editor = Editor::new(200, 200, 1 << 20);
    editor.document.layers[0]
        .surface
        .set(100, 100, [1.0, 0.0, 0.0, 1.0]);
    let thumbnail = editor.layer_thumbnail(0, 20, 20).expect("a preview");
    assert!(
        thumbnail[10 * 20 + 10][3] > 0.0,
        "the painted block vanished"
    );
    assert!(thumbnail[0][3] <= 0.0, "an empty block has ink");
}

#[test]
fn a_quick_shape_can_snap_with_auto_shape_off() {
    let mut editor = Editor::new(128, 128, 1 << 20);
    editor.auto_shape = false;
    editor.quick_shape = true;
    editor.begin_stroke(0);
    editor.stroke((12.0, 64.0), (116.0, 64.0), 2.0, [0.0, 0.0, 0.0, 1.0]);
    editor.end_stroke();
    assert!(editor.auto_shape(), "the held line did not snap");
    editor.undo();
    assert_eq!(
        editor.document.layers[0].surface.get(64, 64),
        [0.0; 4],
        "the snapped stroke was not one undo step"
    );
}

#[test]
fn an_animated_fill_is_one_undo_step_after_it_finishes() {
    use crate::fill::FillOptions;

    let mut editor = Editor::new(32, 32, 1 << 20);
    editor.clear([1.0, 1.0, 1.0, 1.0]);
    let bounds = editor
        .begin_fill_animation(
            16,
            16,
            0.1,
            [1.0, 0.0, 0.0, 1.0],
            FillOptions::default(),
            10,
        )
        .expect("a fill region");
    assert!(bounds.width > 1 && bounds.height > 1);
    assert!(editor.fill_animation_active());
    assert!(editor.fill_animation_step(0.5).is_some());
    assert!(editor.fill_animation_step(1.0).is_some());
    assert!(editor.finish_fill_animation());
    assert!(!editor.fill_animation_active());
    assert!(editor.document.layers[0].surface.get(16, 16)[0] > 0.9);
    editor.undo();
    assert_eq!(editor.document.layers[0].surface.get(16, 16), [1.0; 4]);
}

use super::*;
use crate::tools::Dynamics;

fn tape_with_stroke() -> TapeDocument {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.recording_enabled = true;
    editor.begin_stroke(0);
    editor.stroke_sample(
        (20.0, 32.0),
        (44.0, 32.0),
        6.0,
        Dynamics::pressure(1.0),
        [1.0, 0.0, 0.0, 1.0],
        10,
    );
    editor.end_stroke();
    editor.stroke_tape()
}

#[test]
fn a_tape_replays_at_another_scale() {
    let tape = tape_with_stroke();
    assert_eq!(tape.events.len(), 1);
    let half = replay_tape(
        &tape,
        TapeOutput {
            scale: 0.5,
            brush_scale: BrushScale::Canvas,
        },
    );
    assert_eq!((half.document.width(), half.document.height()), (32, 32));
    let double = replay_tape(
        &tape,
        TapeOutput {
            scale: 2.0,
            brush_scale: BrushScale::Canvas,
        },
    );
    assert_eq!(
        (double.document.width(), double.document.height()),
        (128, 128)
    );
    let centre = double.document.layers[0].surface.get(64, 64);
    assert!(
        centre[0] > 0.9 && centre[3] > 0.9,
        "the replay lost the stroke: {centre:?}"
    );
}

#[test]
fn a_screen_brush_does_not_grow_with_the_canvas() {
    let tape = tape_with_stroke();
    let canvas = replay_tape(
        &tape,
        TapeOutput {
            scale: 2.0,
            brush_scale: BrushScale::Canvas,
        },
    );
    let screen = replay_tape(
        &tape,
        TapeOutput {
            scale: 2.0,
            brush_scale: BrushScale::Screen,
        },
    );
    let row = |editor: &Editor| {
        (0..editor.document.width())
            .filter(|x| editor.document.layers[0].surface.get(*x, 64)[3] > 0.1)
            .count()
    };
    let canvas_width = row(&canvas);
    let screen_width = row(&screen);
    assert!(
        canvas_width > screen_width + 8,
        "canvas {canvas_width}px, screen {screen_width}px"
    );
}

#[test]
fn a_tape_replays_into_its_layers() {
    let mut editor = Editor::new(32, 32, 1 << 20);
    editor.recording_enabled = true;
    editor.document.add_layer("top");
    editor.begin_stroke(0);
    editor.stroke((4.0, 4.0), (28.0, 4.0), 3.0, [1.0, 0.0, 0.0, 1.0]);
    editor.end_stroke();
    editor.document.active = 0;
    editor.begin_stroke(10);
    editor.stroke((4.0, 20.0), (28.0, 20.0), 3.0, [0.0, 0.0, 1.0, 1.0]);
    editor.end_stroke();

    let tape = editor.stroke_tape();
    assert_eq!(tape.layers, 2);
    let replay = replay_tape(&tape, TapeOutput::default());
    assert_eq!(replay.document.layers.len(), 2);
    assert!(replay.document.layers[1].surface.get(16, 4)[0] > 0.8);
    assert!(replay.document.layers[0].surface.get(16, 20)[2] > 0.8);
}

#[test]
fn a_layer_tree_and_a_raster_survive_a_tape_round_trip() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    editor.recording_enabled = true;
    let group = editor.document.add_group("group");
    editor.document.layers[group].opacity = 0.4;
    let child = editor.document.add_layer("child");
    editor.document.layers[child]
        .surface
        .set(3, 3, [0.0, 1.0, 0.0, 1.0]);
    editor.document.layers[child].blend = crate::document::BlendMode::Multiply;
    editor.document.layers[child].locked = true;
    editor.document.layers[child].alpha_locked = true;
    // A layer with pixels but no events is an import or a bake and must travel as a raster.
    let tape = editor.stroke_tape();
    assert!(tape.layer_info[child].raster.is_some());
    let bytes = drawkit_formats::write_dkp(&tape).unwrap();
    let read = drawkit_formats::read_dkp_bytes(&bytes).unwrap();
    let replay = replay_tape(&read, TapeOutput::default());
    assert_eq!(replay.document.layers.len(), 3);
    assert_eq!(replay.document.layers[group].kind, LayerKind::Group);
    assert_eq!(replay.document.layers[group].opacity, 0.4);
    assert_eq!(replay.document.layers[child].parent, Some(group));
    assert_eq!(
        replay.document.layers[child].blend,
        crate::document::BlendMode::Multiply
    );
    assert!(replay.document.layers[child].locked);
    assert!(replay.document.layers[child].alpha_locked);
    assert!(replay.document.layers[child].surface.get(3, 3)[1] > 0.99);
}

#[test]
fn a_dkp_file_opens_with_its_layers_and_tape() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.recording_enabled = true;
    editor.document.add_layer("ink");
    editor.begin_stroke(0);
    editor.stroke((2.0, 4.0), (6.0, 4.0), 1.0, [1.0, 0.0, 0.0, 1.0]);
    editor.end_stroke();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("drawkit-tape-{}-{stamp}.dkp", std::process::id()));
    assert!(editor.save_tape(&path));
    let opened = Editor::open_file(&path).expect("a dkp project");
    let _ = std::fs::remove_file(&path);
    assert_eq!(opened.document.layers.len(), 2);
    assert_eq!(opened.document.layers[1].name, "ink");
    assert!(opened.document.layers[1].surface.get(4, 4)[0] > 0.8);
    assert_eq!(opened.recording.events().len(), 1, "the tape was not kept");
}

#[test]
fn a_cached_project_opens_without_replaying_its_strokes() {
    let mut editor = Editor::new(32, 32, 1 << 20);
    editor.recording_enabled = true;
    editor.begin_stroke(0);
    editor.stroke((8.0, 16.0), (24.0, 16.0), 4.0, [0.0, 0.0, 0.0, 0.4]);
    editor.end_stroke();
    let before = editor.document.layers[0].surface.get(16, 16)[3];
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("drawkit-cache-{}-{stamp}.dkp", std::process::id()));
    assert!(editor.save_tape(&path));
    let opened = Editor::open_file(&path).expect("a cached project");
    let _ = std::fs::remove_file(&path);
    let after = opened.document.layers[0].surface.get(16, 16)[3];
    assert!(
        (after - before).abs() < 0.02,
        "the cached pixel was replayed again: {before} then {after}"
    );
    assert_eq!(opened.recording.events().len(), 1, "the tape was dropped");
}

#[test]
fn a_cached_layer_opens_where_it_was_painted() {
    let mut editor = Editor::new(1024, 1024, 1 << 20);
    editor.document.layers[0]
        .surface
        .set(600, 700, [0.2, 0.8, 0.3, 1.0]);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("drawkit-crop-{}-{stamp}.dkp", std::process::id()));
    assert!(editor.save_tape(&path));
    let opened = Editor::open_file(&path).expect("a cropped project");
    let _ = std::fs::remove_file(&path);
    let pixel = opened.document.layers[0].surface.get(600, 700);
    assert!((pixel[1] - 0.8).abs() < 0.01, "{pixel:?}");
    assert!(opened.document.layers[0].surface.get(0, 0)[3] <= 0.0);
}

#[test]
fn a_project_remembers_the_selected_layer_and_hdr_pixels() {
    let mut editor = Editor::new(24, 24, 1 << 20);
    editor.document.dynamic_range = crate::document::DynamicRange::Hdr;
    editor.recording_enabled = true;
    editor.document.add_layer("second");
    editor.document.layers[1]
        .surface
        .set(12, 12, [0.2, 0.4, 0.6, 1.0]);
    editor.document.active = 0;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("drawkit-hdr-{}-{stamp}.dkp", std::process::id()));
    assert!(editor.save_tape(&path));
    let opened = Editor::open_file(&path).expect("an hdr project");
    let _ = std::fs::remove_file(&path);
    assert_eq!(opened.document.active, 0);
    assert!(opened.document.hdr());
    let pixel = opened.document.layers[1].surface.get(12, 12);
    assert!((pixel[2] - 0.6).abs() < 0.01, "{pixel:?}");
    assert!(opened.recording.events().is_empty());
}

#[test]
fn a_smudge_replays_from_the_tape() {
    let mut editor = Editor::new(32, 16, 1 << 20);
    editor.recording_enabled = true;
    editor.stroke((8.0, 8.0), (24.0, 8.0), 3.0, [0.2, 0.3, 0.8, 1.0]);
    editor.smudge_at((10.0, 8.0), (20.0, 8.0), 3.0, 1.0, 0.9, 100);
    let expected = editor.document.layers[0].surface.get(20, 8);
    let replay = replay_tape(&editor.stroke_tape(), TapeOutput::default());
    let replayed = replay.document.layers[0].surface.get(20, 8);
    assert!(
        (replayed[3] - expected[3]).abs() < 1e-4,
        "{replayed:?} vs {expected:?}"
    );
    assert!(
        (replayed[2] - expected[2]).abs() < 1e-3,
        "{replayed:?} vs {expected:?}"
    );
}

#[test]
fn the_scrub_follows_visibility_and_fill_layers() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.recording_enabled = true;
    editor
        .document
        .add_fill_layer("paper tint", [0.0, 1.0, 0.0, 1.0]);
    editor.document.add_layer("ink");
    editor.begin_stroke(0);
    editor.stroke((0.0, 4.0), (7.0, 4.0), 3.0, [0.8, 0.1, 0.1, 1.0]);
    editor.end_stroke();
    editor.document.add_layer("top");
    editor.begin_stroke(10);
    editor.stroke((0.0, 4.0), (7.0, 4.0), 3.0, [0.1, 0.1, 0.8, 1.0]);
    editor.end_stroke();
    editor.document.active = 0;

    editor.document.layers[3].visible = false;
    let frame = editor.replay_composite(10);
    let centre = frame[4 * 8 + 4];
    assert!(centre[0] > 0.5 && centre[2] < 0.3, "hidden top: {centre:?}");
    assert!(
        frame[0][1] > 0.9,
        "the fill layer is missing: {:?}",
        frame[0]
    );

    editor.document.layers[3].visible = true;
    editor.document.layers[3].opacity = 0.0;
    let frame = editor.replay_composite(10);
    let centre = frame[4 * 8 + 4];
    assert!(centre[0] > 0.5, "opacity zero did not hide the top layer");

    editor.document.layers[3].opacity = 1.0;
    let frame = editor.replay_composite(10);
    let centre = frame[4 * 8 + 4];
    assert!(centre[2] > 0.5, "top layer is still hidden: {centre:?}");
}

#[test]
fn the_cached_replay_advances_and_rewinds() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    editor.recording_enabled = true;
    editor.begin_stroke(0);
    editor.stroke((2.0, 4.0), (14.0, 4.0), 1.0, [1.0, 0.0, 0.0, 1.0]);
    editor.end_stroke();
    editor.begin_stroke(100);
    editor.stroke((2.0, 12.0), (14.0, 12.0), 1.0, [0.0, 0.0, 1.0, 1.0]);
    editor.end_stroke();

    let first = editor.replay_composite(0);
    assert!(first[4 * 16 + 8][3] > 0.5);
    assert!(first[12 * 16 + 8][3] < 0.1);

    let both = editor.replay_composite(100);
    assert!(both[12 * 16 + 8][3] > 0.5);

    let rewound = editor.replay_composite(0);
    assert!(rewound[4 * 16 + 8][3] > 0.5);
    assert!(rewound[12 * 16 + 8][3] < 0.1);
}

#[test]
fn undo_removes_the_stroke_from_the_tape() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.recording_enabled = true;
    editor.begin_stroke(0);
    editor.stroke((10.0, 10.0), (30.0, 10.0), 4.0, [0.0, 0.0, 1.0, 1.0]);
    editor.end_stroke();
    assert_eq!(editor.stroke_tape().events.len(), 1);
    editor.undo();
    assert!(
        editor.stroke_tape().events.is_empty(),
        "undo left the stroke on the tape"
    );
}

use super::super::*;

#[test]
fn a_prediction_paints_ahead_and_is_retracted() {
    let mut editor = Editor::new(64, 16, 1 << 20);
    editor.prediction_enabled = true;
    editor.recording_enabled = true;
    editor.begin_stroke(0);
    editor.stroke_at((10.0, 8.0), (20.0, 8.0), 4.0, 1.0, [0.0, 0.0, 0.0, 1.0], 0);
    editor.stroke_at((20.0, 8.0), (30.0, 8.0), 4.0, 1.0, [0.0, 0.0, 0.0, 1.0], 10);
    assert!(
        editor.document.layers[0].surface.get(36, 8)[3] > 0.0,
        "nothing was predicted ahead of the pen"
    );
    editor.end_stroke();
    assert!(
        editor.document.layers[0].surface.get(36, 8)[3] <= 0.0,
        "the predicted segment stayed after the stroke ended"
    );
    assert!(editor.document.layers[0].surface.get(30, 8)[3] > 0.0);
    assert_eq!(
        editor.recording.events().len(),
        2,
        "prediction reached the tape"
    );

    editor.prediction_enabled = false;
    editor.begin_stroke(20);
    editor.stroke_at((10.0, 8.0), (20.0, 8.0), 4.0, 1.0, [0.0, 0.0, 0.0, 1.0], 20);
    editor.stroke_at((20.0, 8.0), (30.0, 8.0), 4.0, 1.0, [0.0, 0.0, 0.0, 1.0], 30);
    assert_eq!(
        editor.document.layers[0].surface.get(36, 8)[3],
        0.0,
        "prediction ran with the flag off"
    );
}

#[test]
fn a_stationary_pen_does_not_darken_the_same_spot() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    editor.recording_enabled = true;
    editor.begin_stroke(0);
    editor.stroke((8.0, 8.0), (8.0, 8.0), 3.0, [0.0, 0.0, 0.0, 0.5]);
    let once = editor.document.layers[0].surface.get(8, 8);
    for _ in 0..5 {
        editor.stroke((8.0, 8.0), (8.0, 8.0), 3.0, [0.0, 0.0, 0.0, 0.5]);
    }
    let after = editor.document.layers[0].surface.get(8, 8);
    assert_eq!(after, once, "repeated stationary samples kept blending");
    assert_eq!(
        editor.recording.events().len(),
        1,
        "duplicates reached the tape"
    );
    editor.end_stroke();

    // A different pressure at the same place is new ink, not a duplicate.
    editor.begin_stroke(10);
    editor.stroke_at((8.0, 8.0), (8.0, 8.0), 3.0, 1.0, [0.0, 0.0, 0.0, 0.5], 10);
    editor.stroke_at((8.0, 8.0), (8.0, 8.0), 3.0, 0.6, [0.0, 0.0, 0.0, 0.5], 20);
    assert_eq!(editor.recording.events().len(), 3);
}

#[test]
fn a_close_second_fill_keeps_the_first_colour() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.recording_enabled = true;
    editor.fill_with(4, 4, 0.1, [0.8, 0.1, 0.1, 1.0], FillOptions::default(), 0);
    let first = editor.document.layers[0].surface.get(4, 4);

    editor.fill_with(
        4,
        4,
        0.1,
        [0.82, 0.09, 0.09, 1.0],
        FillOptions::default(),
        10,
    );
    let snapped = editor.document.layers[0].surface.get(4, 4);
    assert_eq!(snapped, first, "a close refill shifted the colour");

    editor.fill_snap = false;
    editor.fill_with(
        4,
        4,
        0.1,
        [0.82, 0.09, 0.09, 1.0],
        FillOptions::default(),
        20,
    );
    let free = editor.document.layers[0].surface.get(4, 4);
    assert!(
        (free[0] - 0.82).abs() < 0.01,
        "snap off still changed the colour: {free:?}"
    );
}

#[test]
fn a_smudge_moves_paint_and_is_recorded() {
    let mut editor = Editor::new(48, 16, 1 << 20);
    editor.recording_enabled = true;
    editor.stroke((10.0, 8.0), (38.0, 8.0), 4.0, [0.8, 0.1, 0.1, 1.0]);
    editor.begin_stroke(10);
    editor.smudge_at((12.0, 8.0), (28.0, 8.0), 4.0, 1.0, 0.8, 10);
    editor.end_stroke();
    let moved = editor.document.layers[0].surface.get(28, 8);
    assert!(
        moved[3] > 0.1,
        "smudge did not carry paint forward: {moved:?}"
    );
    assert!(
        editor
            .recording
            .events()
            .iter()
            .any(|event| matches!(event.command, Command::Smudge { .. })),
        "smudge is missing from the tape"
    );
    let after = moved;
    editor.undo();
    assert_ne!(
        editor.document.layers[0].surface.get(28, 8),
        after,
        "undo did not restore the smudge"
    );
}

#[test]
fn an_alpha_locked_layer_keeps_its_transparency() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    editor.document.layers[0]
        .surface
        .set(8, 8, [0.0, 0.0, 0.0, 1.0]);
    editor.document.layers[0]
        .surface
        .set(6, 8, [0.0, 0.0, 0.0, 0.5]);
    editor.document.layers[0].alpha_locked = true;

    editor.stroke((2.0, 8.0), (13.0, 8.0), 2.0, [1.0, 0.0, 0.0, 1.0]);
    let opaque = editor.document.layers[0].surface.get(8, 8);
    assert!(
        opaque[0] > 0.9 && opaque[1] < 0.1,
        "existing pixel not coloured: {opaque:?}"
    );
    assert_eq!(opaque[3], 1.0, "alpha lock changed an opaque pixel");
    let half = editor.document.layers[0].surface.get(6, 8);
    assert!(
        (half[3] - 0.5).abs() < 1e-4,
        "alpha lock changed transparency: {half:?}"
    );
    assert!(
        half[0] > 0.4,
        "the half-transparent pixel stayed black: {half:?}"
    );
    assert_eq!(
        editor.document.layers[0].surface.get(2, 8)[3],
        0.0,
        "alpha lock painted a transparent pixel"
    );

    // A fill with alpha lock colours existing coverage without reaching transparent pixels.
    for y in 4..10 {
        for x in 4..10 {
            editor.document.layers[0]
                .surface
                .set(x, y, [0.5, 0.5, 0.5, 1.0]);
        }
    }
    editor.fill_with(6, 6, 0.2, [0.0, 1.0, 0.0, 1.0], FillOptions::default(), 0);
    let filled = editor.document.layers[0].surface.get(6, 6);
    assert!(
        filled[1] > 0.9,
        "fill did not colour the locked pixels: {filled:?}"
    );
    assert_eq!(
        editor.document.layers[0].surface.get(0, 0)[3],
        0.0,
        "fill painted a transparent pixel"
    );

    // Erase and clear must not punch holes in a locked layer.
    editor.erase((2.0, 8.0), (13.0, 8.0), 3.0, 1.0);
    assert_eq!(editor.document.layers[0].surface.get(8, 8)[3], 1.0);
    editor.clear([0.0; 4]);
    assert_eq!(editor.document.layers[0].surface.get(8, 8)[3], 1.0);
}

#[test]
fn paper_is_removed_from_an_imported_layer() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.document.layers[0]
        .surface
        .set(2, 2, [0.0, 0.0, 0.0, 1.0]);
    editor.document.layers[0]
        .surface
        .set(3, 2, [1.0, 0.0, 0.0, 1.0]);
    editor.document.layers[0]
        .surface
        .set(4, 2, [1.0, 1.0, 1.0, 1.0]);
    assert!(editor.remove_paper_from_layer());
    assert_eq!(editor.document.layers[0].surface.get(4, 2), [0.0; 4]);
    let black = editor.document.layers[0].surface.get(2, 2);
    assert_eq!(black, [0.0, 0.0, 0.0, 1.0]);
    let red = editor.document.layers[0].surface.get(3, 2);
    assert!((red[0] - 1.0).abs() < 0.01 && red[1] < 0.01, "{red:?}");

    // A painted layer already stores ink, not ink-over-paper; refuse it.
    editor.document.add_layer("paint");
    editor.stroke((1.0, 1.0), (6.0, 6.0), 1.0, [0.0, 0.0, 0.0, 1.0]);
    assert!(!editor.remove_paper_from_layer());
}

#[test]
fn every_layer_exports_as_a_png() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.document.layers[0]
        .surface
        .set(2, 2, [1.0, 0.0, 0.0, 1.0]);
    editor
        .document
        .add_fill_layer("paper tint", [0.1, 0.2, 0.3, 1.0]);
    editor.document.add_layer("ink/line");
    editor.document.add_group("empty group");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("drawkit-layers-{}-{stamp}", std::process::id()));
    let written = editor.export_layers(&directory);
    assert_eq!(written, 3, "the group should not become a file");
    let names: Vec<String> = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().any(|name| name.ends_with("ink_line.png")),
        "{names:?}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn jpeg_and_webp_exports_open_as_images() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    editor.stroke((4.0, 8.0), (12.0, 8.0), 2.0, [0.8, 0.1, 0.1, 1.0]);
    for (extension, format) in [
        ("jpg", drawkit_formats::RasterFormat::Jpeg),
        ("webp", drawkit_formats::RasterFormat::WebP),
    ] {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "drawkit-export-{}-{stamp}.{extension}",
            std::process::id()
        ));
        assert!(
            editor.export_raster(&path, format, 90),
            "exporting a {extension}"
        );
        let opened = Editor::open_file(&path).expect("the export opens as an image");
        assert_eq!(
            (opened.document.width(), opened.document.height()),
            (16, 16)
        );
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
fn layer_operations_are_undoable() {
    let mut editor = Editor::new(16, 16, 1 << 20);
    let before = editor.document.active;
    let index = editor.document.add_layer("ink");
    let layer = editor.document.layers[index].clone();
    editor.record_layer_insertion(index, layer, before, editor.document.active);
    editor.stroke((2.0, 8.0), (13.0, 8.0), 2.0, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(editor.document.layers.len(), 2);
    assert_eq!(editor.recording.events().len(), 1);

    // Undo the stroke, then the add; the tape loses only the stroke.
    editor.undo();
    assert_eq!(editor.recording.events().len(), 0);
    assert_eq!(editor.document.layers.len(), 2);
    editor.undo();
    assert_eq!(editor.document.layers.len(), 1);
    assert_eq!(
        editor.recording.events().len(),
        0,
        "layer undo touched the tape"
    );

    editor.redo();
    assert_eq!(editor.document.layers.len(), 2);
    editor.redo();
    assert_eq!(editor.document.layers[1].surface.get(8, 8)[3], 1.0);
}

#[test]
fn a_layer_state_change_is_undoable_without_the_tape() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.stroke((2.0, 4.0), (6.0, 4.0), 1.0, [0.0, 0.0, 0.0, 1.0]);
    let before = editor.document.layers[0].meta();
    editor.document.layers[0].opacity = 0.25;
    editor.record_layer_state(0, LayerField::Opacity, before);
    assert_eq!(editor.document.layers[0].opacity, 0.25);
    assert_eq!(editor.recording.events().len(), 1);

    editor.undo();
    assert_eq!(editor.document.layers[0].opacity, 1.0);
    assert_eq!(
        editor.recording.events().len(),
        1,
        "opacity undo touched the tape"
    );
    editor.redo();
    assert_eq!(editor.document.layers[0].opacity, 0.25);
}

#[test]
fn removing_a_layer_restores_its_pixels() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.document.add_layer("second");
    editor.document.layers[1]
        .surface
        .set(3, 3, [1.0, 0.0, 0.0, 1.0]);
    let before = editor.document.active;
    let removal = editor
        .document
        .remove_active_collect()
        .expect("a layer was removed");
    editor.record_layer_removal(
        removal.removed,
        removal.inserted,
        removal.active_before,
        removal.active_after,
    );
    assert_eq!(editor.document.layers.len(), 1);
    editor.undo();
    assert_eq!(editor.document.layers.len(), 2);
    assert_eq!(editor.document.layers[1].surface.get(3, 3)[0], 1.0);
    editor.redo();
    assert_eq!(editor.document.layers.len(), 1);
    let _ = before;
}

#[test]
fn renaming_a_layer_is_undoable() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    let before = editor.document.layers[0].meta();
    editor.document.layers[0].name = "ink".to_owned();
    editor.record_layer_state(0, LayerField::Name, before);
    assert_eq!(editor.document.layers[0].name, "ink");
    editor.undo();
    assert_eq!(editor.document.layers[0].name, "Layer 1");
    editor.redo();
    assert_eq!(editor.document.layers[0].name, "ink");
}

#[test]
fn a_layer_move_is_undoable() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.document.add_layer("second");
    let before = editor.document.active;
    let (after_index, applied) = editor.document.move_layer(0, 1).expect("the layer moved");
    let after = editor.document.active;
    editor.record_layer_move(0, after_index, applied, before, after);
    assert_eq!(editor.document.layers[0].name, "second");
    assert_eq!(editor.document.layers[1].name, "Layer 1");

    editor.undo();
    assert_eq!(editor.document.layers[0].name, "Layer 1");
    assert_eq!(editor.document.layers[1].name, "second");
    assert_eq!(editor.document.active, before);
    editor.redo();
    assert_eq!(editor.document.layers[0].name, "second");
    assert_eq!(editor.document.active, after);
}

#[test]
fn an_overview_is_opaque_paper() {
    let mut editor = Editor::new(32, 16, 1 << 20);
    editor.stroke((4.0, 8.0), (28.0, 8.0), 3.0, [0.8, 0.1, 0.1, 1.0]);
    let overview = editor.overview(8, 4);
    assert_eq!(overview.len(), 8 * 4 * 4);
    assert_eq!(&overview[..4], &[255, 255, 255, 255]);
    assert!(overview.iter().any(|channel| *channel < 200));
}

#[test]
fn undoing_drops_the_stroke_from_the_tape() {
    let mut editor = Editor::new(32, 32, 1 << 20);
    editor.recording_enabled = true;
    editor.stroke((2.0, 2.0), (20.0, 20.0), 2.0, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(editor.recording.events().len(), 1);
    editor.undo();
    assert!(
        editor.recording.is_empty(),
        "the undone stroke stayed on the tape"
    );
    assert_eq!(editor.document.layers[0].surface.get(10, 10), [0.0; 4]);
}

#[test]
fn a_colour_tag_is_undoable() {
    use super::super::*;
    let mut editor = Editor::new(8, 8, 1 << 20);
    let before = editor.document.layers[0].meta();
    editor.document.layers[0].color_tag = 4;
    editor.record_layer_state(0, LayerField::ColorTag, before);
    assert_eq!(editor.document.layers[0].color_tag, 4);
    editor.undo();
    assert_eq!(editor.document.layers[0].color_tag, 0);
    editor.redo();
    assert_eq!(editor.document.layers[0].color_tag, 4);
}

#[test]
fn a_reparent_is_one_undo_step() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.document.add_group("group");
    editor.document.add_layer("inside");
    editor.document.active = 0;
    editor.document.add_layer("outside");
    let group = editor
        .document
        .layers
        .iter()
        .position(|layer| layer.name == "group")
        .expect("group");
    let outside = editor
        .document
        .layers
        .iter()
        .position(|layer| layer.name == "outside")
        .expect("outside");
    let moved = editor
        .reparent_layer(outside, Some(group))
        .expect("moved into the group");
    assert_eq!(
        editor.document.layers[moved]
            .parent
            .map(|p| editor.document.layers[p].name.clone()),
        Some("group".to_owned())
    );

    editor.undo();
    let outside = editor
        .document
        .layers
        .iter()
        .position(|layer| layer.name == "outside")
        .expect("outside after undo");
    assert_eq!(editor.document.layers[outside].parent, None);

    editor.redo();
    let outside = editor
        .document
        .layers
        .iter()
        .position(|layer| layer.name == "outside")
        .expect("outside after redo");
    assert!(editor.document.layers[outside].parent.is_some());
}

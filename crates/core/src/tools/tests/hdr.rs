use super::super::*;

#[test]
fn an_hdr_project_replays_highlights_instead_of_a_clipped_cache() {
    let mut editor = Editor::new(32, 32, 1 << 20);
    editor.document.dynamic_range = crate::document::DynamicRange::Hdr;
    editor.begin_stroke(0);
    editor.stroke((4.0, 16.0), (28.0, 16.0), 6.0, [3.0, 1.0, 0.5, 1.0]);
    editor.end_stroke();
    let path = std::env::temp_dir().join("drawkit-hdr-replay.dkp");
    assert!(editor.save_tape(&path));
    let opened = Editor::open_file(&path).expect("open");
    let _ = std::fs::remove_file(&path);
    let picked = opened.pick(16, 16).expect("a pixel in the canvas");
    assert!(picked[0] > 2.5, "the highlight was clipped: {picked:?}");
}

#[test]
fn an_hdr_layer_export_keeps_exr_files() {
    let mut editor = Editor::new(8, 8, 1 << 20);
    editor.document.dynamic_range = crate::document::DynamicRange::Hdr;
    editor
        .document
        .active_mut()
        .surface
        .set(1, 1, [2.0, 0.5, 0.25, 1.0]);
    let directory = std::env::temp_dir().join("drawkit-hdr-layer-export");
    let _ = std::fs::remove_dir_all(&directory);
    assert_eq!(editor.export_layers(&directory), 1);
    let name = std::fs::read_dir(&directory)
        .expect("exported directory")
        .next()
        .and_then(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(&directory);
    assert!(name.ends_with(".exr"), "exported {name}");
}

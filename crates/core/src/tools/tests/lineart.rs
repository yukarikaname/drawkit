use super::super::*;

#[test]
fn extraction_adds_one_line_layer_and_undo_removes_it() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.clear([1.0, 1.0, 1.0, 1.0]);
    for y in 16..48 {
        for x in 16..48 {
            let edge = x == 16 || x == 47 || y == 16 || y == 47;
            if edge {
                editor.document.layers[0]
                    .surface
                    .set(x, y, [0.0, 0.0, 0.0, 1.0]);
            }
        }
    }
    let before = editor.document.layers.len();
    let index = editor
        .extract_line_art(crate::tools::LineArtOptions::default())
        .expect("the square has edges");
    assert_eq!(editor.document.layers.len(), before + 1);
    assert_eq!(index + 1, editor.document.layers.len());
    let layer = &editor.document.layers[index];
    let inked = (0..64).any(|y| (0..64).any(|x| layer.surface.get(x, y)[3] > 0.1));
    assert!(inked, "the line layer is not empty");

    assert!(editor.undo().is_some());
    assert_eq!(editor.document.layers.len(), before);
}

#[test]
fn extraction_with_cleanup_inserts_one_layer_and_undoes_in_one_step() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    editor.clear([1.0, 1.0, 1.0, 1.0]);
    for y in 16..48 {
        for x in 16..48 {
            let edge = x == 16 || x == 47 || y == 16 || y == 47;
            if edge {
                editor.document.layers[0]
                    .surface
                    .set(x, y, [0.0, 0.0, 0.0, 1.0]);
            }
        }
    }
    let before = editor.document.layers.len();
    let index = editor
        .extract_line_art_clean(
            crate::tools::LineArtOptions::default(),
            crate::tools::CleanOptions::default(),
        )
        .expect("the square has edges");
    assert_eq!(editor.document.layers.len(), before + 1);
    let layer = &editor.document.layers[index];
    let inked = (0..64).any(|y| (0..64).any(|x| layer.surface.get(x, y)[3] > 0.1));
    assert!(inked, "the cleaned line layer is not empty");

    assert!(editor.undo().is_some(), "one cleanup insertion is one step");
    assert_eq!(editor.document.layers.len(), before);
}

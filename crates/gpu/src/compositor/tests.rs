use super::*;

#[test]
fn a_headless_compositor_blends_two_layers() {
    let Some(mut compositor) = GpuCompositor::new(2, 2) else {
        return;
    };
    compositor.reset(2, 2, 2);
    let red = [255, 0, 0, 255].repeat(4);
    let blue = [0, 0, 255, 128].repeat(4);
    assert!(compositor.upload_layer(RectUpload {
        index: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        pixels: &red,
    }));
    assert!(compositor.upload_layer(RectUpload {
        index: 1,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        pixels: &blue,
    }));
    compositor.set_layer(0, LayerSpec::new(0));
    compositor.set_layer(1, LayerSpec::new(1));
    let out = compositor.composite();
    assert!((out[0] as i32 - 127).abs() <= 2, "red {}", out[0]);
    assert!((out[2] as i32 - 128).abs() <= 2, "blue {}", out[2]);
}

#[test]
fn a_replayed_stroke_commits_into_a_resident_tile() {
    let Some(mut compositor) = GpuCompositor::new(16, 16) else {
        return;
    };
    compositor.reset(1, 16, 16);
    compositor.stroke_begin(
        16,
        16,
        [1.0, 0.0, 0.0, 1.0],
        1.0,
        0.0,
        0.0,
        0.0,
        [0.0; 7],
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        7,
        false,
        u32::MAX,
    );
    compositor.stroke_segment((2.0, 8.0), (14.0, 8.0), 4.0, 1.0, 0.1, 1.0, 0.0, 1.0);
    assert!(compositor.commit_stroke(0, [1.0, 0.0, 0.0], 1.0) > 0);
    compositor.stroke_end();
    let pixels = compositor.composite();
    let centre = (8 * 16 + 8) * 4;
    assert!(pixels[centre] > 200, "red {}", pixels[centre]);
    assert!(pixels[centre + 3] > 200, "alpha {}", pixels[centre + 3]);
}

#[test]
fn a_half_float_compositor_keeps_highlights() {
    let Some(mut compositor) = GpuCompositor::new_hdr(1, 1) else {
        return;
    };
    compositor.reset(1, 1, 1);
    assert!(compositor.upload_layer_linear(0, &[2.0, 0.25, 0.5, 1.0]));
    let pixels = compositor.composite_linear();
    assert!((pixels[0] - 2.0).abs() < 0.02, "red {}", pixels[0]);
    assert!((pixels[1] - 0.25).abs() < 0.01, "green {}", pixels[1]);
}

#[test]
fn a_half_float_fill_keeps_highlights() {
    let Some(mut compositor) = GpuCompositor::new_hdr(2, 2) else {
        return;
    };
    compositor.reset(1, 2, 2);
    assert!(compositor.upload_layer_linear(0, &[0.0; 16]));
    let mask = [1.0f32, 0.0, 0.0, 0.0];
    assert!(compositor.apply_fill(0, &mask, [3.0, 0.5, 0.25, 1.0], false, false));
    let pixels = compositor.composite_linear();
    assert!((pixels[0] - 3.0).abs() < 0.03, "red {}", pixels[0]);
    assert_eq!(pixels[4], 0.0, "outside the mask");
}

#[test]
fn undo_and_redo_publish_the_commit_rectangles() {
    let Some(mut compositor) = GpuCompositor::new(16, 16) else {
        return;
    };
    compositor.reset(1, 16, 16);
    compositor.stroke_begin(
        16,
        16,
        [1.0, 0.0, 0.0, 1.0],
        1.0,
        0.0,
        0.0,
        0.0,
        [0.0; 7],
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        7,
        false,
        u32::MAX,
    );
    compositor.stroke_segment((2.0, 8.0), (14.0, 8.0), 4.0, 1.0, 0.1, 1.0, 0.0, 1.0);
    assert!(compositor.commit_stroke(0, [1.0, 0.0, 0.0], 1.0) > 0);
    compositor.stroke_end();

    let mut committed = Vec::new();
    while let Some(rect) = compositor.committed_rect(committed.len()) {
        committed.push(rect);
    }
    assert!(!committed.is_empty(), "the commit touched no tile");

    assert!(compositor.undo_tiles());
    let mut undone = Vec::new();
    while let Some(rect) = compositor.committed_rect(undone.len()) {
        undone.push(rect);
    }
    assert_eq!(undone, committed, "undo reported different rectangles");

    assert!(compositor.redo_tiles());
    let mut redone = Vec::new();
    while let Some(rect) = compositor.committed_rect(redone.len()) {
        redone.push(rect);
    }
    assert_eq!(redone, committed, "redo reported different rectangles");
}

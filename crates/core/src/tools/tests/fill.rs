//! Fills against antialiased line art: the white rim, leaks and refills.

use super::super::*;

/// The white edge is the line's antialiased rim composited over paper. A fill under the line has
/// to replace that paper; at any scale the rim must not come back.
#[test]
fn a_fill_under_the_line_leaves_no_pale_rim() {
    for scale in [1usize, 2] {
        let side = 64 * scale;
        let mut editor = Editor::new(side, side, 1 << 20);
        let x = 32.0 * scale as f32;
        editor.stroke(
            (x, 0.0),
            (x, side as f32),
            scale as f32,
            [0.0, 0.0, 0.0, 1.0],
        );
        // The replay scales the pixel-sized fill parameters with the output, so a 2x render grows
        // by two pixels and still reaches under the wider antialiased rim.
        let options = FillOptions {
            close: scale,
            expand: scale,
            feather: 0,
            smart: false,
        };
        editor.fill_with(
            16 * scale,
            32 * scale,
            0.1,
            [1.0, 0.0, 0.0, 1.0],
            options,
            0,
        );
        let rect = Rect::new(0, 0, side, side);
        let composite = editor.document.composite_rect(rect);
        let paper = crate::tools::PAPER;
        let row = 32 * scale;
        // Only the side the fill was seeded on: the line's outer rim legitimately blends into the
        // bare paper on the other side.
        for column in 0..=(32 * scale) {
            // Only pixels the layer actually painted: bare paper to the right of the line is
            // white by design, and skipping it keeps the rim check honest.
            let painted = editor.document.layers[0].surface.get(column, row)[3];
            if painted <= 0.02 {
                continue;
            }
            let shown = crate::color::over(composite[row * side + column], paper);
            assert!(
                shown[1] < 0.06,
                "scale {scale}: pale rim at x={column}: {shown:?}"
            );
        }
    }
}

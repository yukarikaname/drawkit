// The presented rectangle has to be the visible part of the sheet, not the whole canvas, and it
// has to clamp where the sheet leaves the canvas.

use super::visible_rect;

const FIT: (u32, u32) = (200, 100);

#[test]
fn a_fitted_paper_shows_the_whole_canvas() {
    // The paper fills the drawable: every texel can be seen.
    assert_eq!(
        visible_rect([0.0, 0.0, 200.0, 100.0], FIT, FIT, 0.0),
        (0, 0, 200, 100)
    );
}

#[test]
fn a_zoomed_paper_shows_only_the_middle() {
    // The paper is four times the drawable, so only the middle quarter is on screen.
    assert_eq!(
        visible_rect([-100.0, -50.0, 400.0, 200.0], FIT, (100, 50), 0.0),
        (25, 12, 50, 26)
    );
}

#[test]
fn the_region_clamps_at_the_canvas_edge() {
    // A wide drawable maps past the right edge of the canvas; the region stops there.
    assert_eq!(
        visible_rect([-100.0, -50.0, 400.0, 200.0], (1000, 100), (100, 50), 0.0),
        (25, 12, 75, 26)
    );
}

#[test]
fn the_region_clamps_at_the_top_edge() {
    // The sheet starts below the top of the drawable: the surround above it must not pull the
    // region up past canvas row 0, and only the visible strip is composed.
    assert_eq!(
        visible_rect([0.0, 50.0, 100.0, 300.0], FIT, (100, 100), 0.0),
        (0, 0, 100, 17)
    );
}

#[test]
fn rotation_expands_the_region_to_the_bounding_box() {
    // Same zoomed sheet upright and at 45 degrees. The turned drawable covers a larger
    // axis-aligned rectangle in canvas space, and the origin stays symmetric around the middle.
    let upright = visible_rect([-100.0, -100.0, 300.0, 300.0], (100, 100), (100, 100), 0.0);
    assert_eq!(upright, (33, 33, 34, 34));
    let turned = visible_rect(
        [-100.0, -100.0, 300.0, 300.0],
        (100, 100),
        (100, 100),
        std::f32::consts::FRAC_PI_4,
    );
    assert_eq!(turned, (26, 26, 48, 48));
    assert!(
        turned.2 > upright.2 && turned.3 > upright.3,
        "rotation must widen the region, {turned:?} vs {upright:?}"
    );
}

#[test]
fn a_paper_off_screen_still_yields_a_valid_texture() {
    // Nothing of the sheet is visible, so the shader only paints the surround. The one-pixel
    // region keeps the texture allocation valid without compositing the canvas.
    assert_eq!(
        visible_rect([500.0, 500.0, 100.0, 100.0], (10, 10), (100, 100), 0.0),
        (0, 0, 1, 1)
    );
}

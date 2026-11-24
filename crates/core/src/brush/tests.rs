use super::*;

fn soft() -> Brush {
    Brush::soft_round()
}

#[test]
fn a_stroke_has_no_holes() {
    let mut surface = Surface::new(64, 64);
    stroke(
        &mut surface,
        (4.0, 32.0),
        (60.0, 32.0),
        3.0,
        0.0,
        [1.0, 0.0, 0.0, 1.0],
    );
    for x in 5..60 {
        assert!(surface.get(x, 32)[3] > 0.9, "hole at {x}");
    }
}

#[test]
fn erasing_removes_alpha_and_leaves_neighbours_alone() {
    let mut surface = Surface::new(64, 32);
    surface.set(2, 2, [0.0, 0.0, 1.0, 1.0]);
    stroke(
        &mut surface,
        (8.0, 16.0),
        (56.0, 16.0),
        4.0,
        0.0,
        [0.0, 0.0, 0.0, 1.0],
    );
    assert!(surface.get(32, 16)[3] > 0.9);
    erase(&mut surface, (20.0, 16.0), (44.0, 16.0), 4.0, 0.0);
    assert!(
        surface.get(32, 16)[3] < 0.1,
        "the eraser left ink at the centre"
    );
    assert_eq!(
        surface.get(2, 2),
        [0.0, 0.0, 1.0, 1.0],
        "the eraser painted elsewhere"
    );
}

#[test]
fn one_stroke_does_not_saturate_its_soft_edge() {
    let mut surface = Surface::new(64, 32);
    stroke(
        &mut surface,
        (8.0, 16.0),
        (56.0, 16.0),
        4.0,
        0.0,
        [0.0, 0.0, 0.0, 1.0],
    );
    let centre = surface.get(32, 16)[3];
    let rim = surface.get(32, 19)[3];
    assert!(centre > 0.95, "the line is hollow: {centre}");
    assert!(rim > 0.0 && rim < 0.4, "the rim saturated: {rim}");
}

#[test]
fn two_strokes_still_build_up() {
    let mut surface = Surface::new(64, 32);
    for _ in 0..2 {
        stroke(
            &mut surface,
            (8.0, 16.0),
            (56.0, 16.0),
            4.0,
            0.0,
            [0.0, 0.0, 0.0, 1.0],
        );
    }
    let alpha = surface.get(32, 19)[3];
    assert!(alpha > 0.2, "the second stroke did nothing: {alpha}");
}

#[test]
fn hardness_makes_the_disc_solid_and_softness_leaves_a_falloff() {
    let mut hard = Surface::new(32, 32);
    stamp(&mut hard, (16.0, 16.0), 8.0, 1.0, [0.0, 0.0, 0.0, 1.0]);
    assert!(hard.get(16, 22)[3] > 0.9, "the hard rim is not solid");

    let mut soft_surface = Surface::new(32, 32);
    stamp(
        &mut soft_surface,
        (16.0, 16.0),
        8.0,
        0.0,
        [0.0, 0.0, 0.0, 1.0],
    );
    let rim = soft_surface.get(16, 22)[3];
    assert!(rim > 0.0 && rim < 0.6, "the soft rim is {rim}");
}

#[test]
fn an_ink_pen_keeps_its_width_and_a_soft_brush_does_not() {
    let soft_width = soft().radius_at(10.0, 0.2);
    let ink_width = Brush::for_kind(BrushKind::Ink).radius_at(10.0, 0.2);
    assert!(ink_width > soft_width * 1.5, "{soft_width} vs {ink_width}");
    assert!(
        (Brush::for_kind(BrushKind::Ink).alpha_at(0.2) - 1.0).abs() < 1e-5,
        "ink lost opacity at light pressure"
    );
}

#[test]
fn smudging_carries_paint_forward() {
    let mut surface = Surface::new(64, 32);
    surface.set(10, 16, [1.0, 0.0, 0.0, 1.0]);
    surface.set(11, 16, [1.0, 0.0, 0.0, 1.0]);
    smudge(&mut surface, (10.0, 16.0), (30.0, 16.0), 4.0, 0.0, 1.0);
    assert!(
        surface.get(30, 16)[3] > 0.0,
        "the smudge left no paint behind"
    );
    assert!(surface.get(10, 16)[3] > 0.0, "the smudge erased its source");
}

#[test]
fn a_marker_lays_down_less_than_half_per_segment() {
    let marker = Brush::for_kind(BrushKind::Marker);
    assert!(marker.alpha_at(1.0) < 0.5);
    let mut surface = Surface::new(64, 32);
    stroke(
        &mut surface,
        (8.0, 16.0),
        (56.0, 16.0),
        4.0,
        marker.hardness,
        [0.0, 0.0, 0.0, marker.alpha_at(1.0)],
    );
    assert!(surface.get(32, 16)[3] < 0.5);
}

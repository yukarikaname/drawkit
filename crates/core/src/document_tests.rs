//! Tests for the layer tree and the compositor.

use super::*;

#[test]
fn a_new_document_has_one_layer_to_draw_on() {
    let document = Document::new(64, 64);
    assert_eq!(document.layers.len(), 1);
    assert_eq!(document.active, 0);
    assert!(document.can_paint());
}

#[test]
fn the_last_layer_cannot_be_removed() {
    let mut document = Document::new(8, 8);
    assert!(!document.remove_active());
    document.add_layer("ink");
    assert!(document.remove_active());
    assert!(!document.remove_active());
}

#[test]
fn compositing_puts_the_upper_layer_on_top() {
    let mut document = Document::new(4, 4);
    document
        .active_mut()
        .surface
        .set(1, 1, [1.0, 0.0, 0.0, 1.0]);
    document.add_layer("ink");
    document
        .active_mut()
        .surface
        .set(1, 1, [0.0, 0.0, 1.0, 1.0]);
    let pixels = document.composite_rect(Rect::new(0, 0, 4, 4));
    assert_eq!(pixels[5], [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn opacity_and_visibility_are_respected() {
    let mut document = Document::new(2, 2);
    document.add_layer("ink");
    document
        .active_mut()
        .surface
        .set(0, 0, [1.0, 1.0, 1.0, 1.0]);
    document.active_mut().opacity = 0.5;
    let pixels = document.composite_rect(Rect::new(0, 0, 2, 2));
    assert!((pixels[0][3] - 0.5).abs() < 1e-5);
    document.active_mut().visible = false;
    let hidden = document.composite_rect(Rect::new(0, 0, 2, 2));
    assert_eq!(hidden[0][3], 0.0);
}

#[test]
fn the_dirty_rectangle_is_the_union_of_the_layers() {
    let mut document = Document::new(512, 512);
    document.layers[0].surface.set(4, 4, [1.0; 4]);
    document.add_layer("ink");
    document.active_mut().surface.set(300, 300, [1.0; 4]);
    let dirty = document.take_dirty().unwrap();
    assert!(dirty.x <= 4 && dirty.y <= 4);
    assert!(dirty.x + dirty.width >= 301);
    assert!(dirty.y + dirty.height >= 301);
}

#[test]
fn a_group_owns_layers_and_applies_its_opacity_once() {
    let mut document = Document::new(4, 4);
    let group = document.add_group("group");
    assert!(document.is_group(group));
    let child = document.add_layer("inside");
    assert_eq!(document.layers[child].parent, Some(group));
    document.layers[child]
        .surface
        .set(0, 0, [1.0, 0.0, 0.0, 1.0]);
    document.layers[group].opacity = 0.5;
    let pixels = document.composite_rect(Rect::new(0, 0, 4, 4));
    assert!((pixels[0][3] - 0.5).abs() < 1e-5);
    document.layers[group].visible = false;
    let hidden = document.composite_rect(Rect::new(0, 0, 4, 4));
    assert_eq!(hidden[0][3], 0.0);
}

#[test]
fn a_fill_layer_composites_and_respects_opacity() {
    let mut document = Document::new(4, 4);
    document.add_fill_layer("paper", [0.0, 0.0, 1.0, 0.5]);
    let pixels = document.composite_rect(Rect::new(0, 0, 4, 4));
    assert!((pixels[0][3] - 0.5).abs() < 1e-5);
    assert!(pixels[0][2] > 0.99);
    document.active_mut().visible = false;
    let hidden = document.composite_rect(Rect::new(0, 0, 4, 4));
    assert_eq!(hidden[0][3], 0.0);
}

#[test]
fn removing_a_group_removes_its_children() {
    let mut document = Document::new(8, 8);
    let group = document.add_group("group");
    document.add_layer("inside");
    assert_eq!(document.layers.len(), 3);
    document.active = group;
    assert!(document.remove_active());
    assert_eq!(document.layers.len(), 1);
    assert!(document.can_paint());
}

#[test]
fn moving_a_layer_moves_its_siblings_and_keeps_parents() {
    let mut document = Document::new(8, 8);
    let top = document.add_layer("top");
    let group = document.add_group("group");
    let first = document.add_layer("first");
    let second = document.add_layer("second");
    assert_eq!((top, group, first, second), (1, 2, 3, 4));

    // The display order is the reverse of the vector, so a higher index is closer to the front.
    assert_eq!(
        document.move_layer(second, 1),
        None,
        "already the front child"
    );
    assert_eq!(document.move_layer(first, 1), Some((4, 1)));
    assert_eq!(document.layers[3].name, "second");
    assert_eq!(document.layers[4].name, "first");
    assert_eq!(document.layers[4].parent, Some(2));

    // Moving the group down keeps both children inside it and moves the whole block.
    assert_eq!(document.move_layer(2, -1), Some((1, -1)));
    assert_eq!(document.layers[1].name, "group");
    assert_eq!(document.layers[2].parent, Some(1));
    assert_eq!(document.layers[3].parent, Some(1));
    assert_eq!(document.layers[4].name, "top");
}

#[test]
fn depth_and_effective_visibility_follow_parents() {
    let mut document = Document::new(8, 8);
    let outer = document.add_group("outer");
    let inner = document.add_group("inner");
    let child = document.add_layer("child");
    assert_eq!(document.depth(child), 2);
    document.layers[inner].visible = false;
    assert!(!document.effective_visible(child));
    document.layers[inner].visible = true;
    document.layers[outer].opacity = 0.25;
    assert!((document.effective_opacity(child) - 0.25).abs() < 1e-5);
}

#[test]
fn a_linear_composite_keeps_a_highlight_and_white_stays_white() {
    let mut document = Document::new(2, 2);
    document.dynamic_range = DynamicRange::Hdr;
    document
        .active_mut()
        .surface
        .set(0, 0, [2.0, 2.0, 2.0, 1.0]);
    document
        .active_mut()
        .surface
        .set(1, 0, [1.0, 1.0, 1.0, 1.0]);
    let linear = document.composite_linear_rect(Rect::new(0, 0, 2, 1));
    assert!(linear[0][0] > 1.5, "{:?}", linear[0]);
    assert!((linear[1][0] - 1.0).abs() < 1e-4, "{:?}", linear[1]);
}

#[test]
fn multiply_darkens_and_screen_lightens_the_layer_below() {
    let mut document = Document::new(2, 2);
    document.layers[0].surface.set(0, 0, [0.5, 0.5, 0.5, 1.0]);
    document.add_layer("top");
    document
        .active_mut()
        .surface
        .set(0, 0, [0.5, 0.5, 0.5, 1.0]);

    document.layers[1].blend = BlendMode::Multiply;
    let multiplied = document.composite_rect(Rect::new(0, 0, 1, 1));
    assert!(
        (multiplied[0][0] - 0.25).abs() < 1e-4,
        "{:?}",
        multiplied[0]
    );

    document.layers[1].blend = BlendMode::Screen;
    let screened = document.composite_rect(Rect::new(0, 0, 1, 1));
    assert!((screened[0][0] - 0.75).abs() < 1e-4, "{:?}", screened[0]);
}

#[test]
fn a_locked_layer_refuses_paint() {
    let mut document = Document::new(8, 8);
    document.active_mut().locked = true;
    assert!(!document.can_paint());
    document.active_mut().locked = false;
    assert!(document.can_paint());
}

#[test]
fn color_and_luminosity_take_one_attribute_each() {
    let mut document = Document::new(1, 1);
    document.layers[0]
        .surface
        .set(0, 0, [0.25, 0.25, 0.25, 1.0]);
    document.add_layer("top");
    document
        .active_mut()
        .surface
        .set(0, 0, [0.9, 0.1, 0.1, 1.0]);

    document.layers[1].blend = BlendMode::Luminosity;
    let luminosity = document.composite_rect(Rect::new(0, 0, 1, 1))[0];
    let expected = 0.30 * 0.9 + 0.59 * 0.1 + 0.11 * 0.1;
    let got = 0.30 * luminosity[0] + 0.59 * luminosity[1] + 0.11 * luminosity[2];
    assert!((got - expected).abs() < 1e-3, "{luminosity:?}");
    assert!(
        (luminosity[0] - luminosity[1]).abs() < 1e-3,
        "luminosity should leave the grey backdrop neutral: {luminosity:?}"
    );

    document.layers[1].blend = BlendMode::Color;
    let color = document.composite_rect(Rect::new(0, 0, 1, 1))[0];
    let got = 0.30 * color[0] + 0.59 * color[1] + 0.11 * color[2];
    assert!((got - 0.25).abs() < 1e-3, "{color:?}");
    assert!(color[0] > color[2], "the red hue was lost");
}

#[test]
fn a_subrect_outside_the_painted_tiles_stays_empty() {
    let mut document = Document::new(600, 600);
    document.layers[0]
        .surface
        .set(520, 540, [1.0, 0.0, 0.0, 1.0]);
    let near = document.composite_rect(Rect::new(500, 512, 64, 64));
    assert!(
        near[28 * 64 + 20][3] > 0.99,
        "the painted pixel was clipped out"
    );
    let far = document.composite_rect(Rect::new(0, 0, 64, 64));
    assert!(
        far.iter().all(|pixel| pixel[3] <= 0.0),
        "an empty rect has ink"
    );
}

#[test]
fn a_clipped_layer_shows_only_through_the_base() {
    let mut document = Document::new(6, 6);
    for y in 1..4 {
        for x in 1..4 {
            document.layers[0].surface.set(x, y, [0.0, 0.0, 0.0, 1.0]);
        }
    }
    document.add_layer("clip");
    document.active_mut().clipping = true;
    for y in 0..6 {
        for x in 0..6 {
            document
                .active_mut()
                .surface
                .set(x, y, [1.0, 0.0, 0.0, 1.0]);
        }
    }
    let pixels = document.composite_rect(Rect::new(0, 0, 6, 6));
    assert!(
        pixels[2 * 6 + 2][0] > 0.9,
        "clipped ink is missing over the base"
    );
    assert_eq!(pixels[0][3], 0.0, "clipped ink appeared outside the base");
    assert_eq!(pixels[5 * 6 + 5][3], 0.0);

    // A hidden base hides everything clipped to it.
    document.layers[0].visible = false;
    let pixels = document.composite_rect(Rect::new(0, 0, 6, 6));
    assert_eq!(pixels[2 * 6 + 2][3], 0.0);
}

#[test]
fn a_clipping_base_can_be_a_group() {
    let mut document = Document::new(4, 4);
    let group = document.add_group("base group");
    let child = document.add_layer("inside");
    document.layers[child]
        .surface
        .set(2, 2, [0.0, 0.0, 0.0, 1.0]);
    document.layers[group].opacity = 1.0;
    document.add_layer("clip");
    document.active_mut().clipping = true;
    document
        .active_mut()
        .surface
        .set(2, 2, [1.0, 0.0, 0.0, 1.0]);
    document
        .active_mut()
        .surface
        .set(0, 0, [1.0, 0.0, 0.0, 1.0]);
    let pixels = document.composite_rect(Rect::new(0, 0, 4, 4));
    assert!(
        pixels[2 * 4 + 2][0] > 0.9,
        "clip through a group base failed"
    );
    assert_eq!(pixels[0][3], 0.0);
}

#[test]
fn a_reference_layer_is_shown_but_not_exported() {
    let mut document = Document::new(2, 2);
    document
        .active_mut()
        .surface
        .set(0, 0, [0.0, 0.0, 1.0, 1.0]);
    document.add_layer("reference");
    document.active_mut().reference = true;
    document
        .active_mut()
        .surface
        .set(0, 0, [1.0, 0.0, 0.0, 1.0]);
    let screen = document.composite_rect(Rect::new(0, 0, 2, 2));
    assert_eq!(screen[0], [1.0, 0.0, 0.0, 1.0]);
    let export = document.composite_rect_export(Rect::new(0, 0, 2, 2));
    assert_eq!(export[0], [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn reparenting_moves_a_layer_into_and_out_of_a_group() {
    let mut document = Document::new(8, 8);
    document.add_group("group");
    document.add_layer("inside");
    document.active = 0;
    document.add_layer("outside");
    let outside = document
        .layers
        .iter()
        .position(|layer| layer.name == "outside")
        .expect("outside layer");
    let group = document
        .layers
        .iter()
        .position(|layer| layer.name == "group")
        .expect("group layer");

    let (moved, _) = document
        .reparent_layer(outside, Some(group))
        .expect("layer moved into the group");
    let parent = document.layers[moved].parent.expect("moved into a group");
    assert_eq!(document.layers[parent].name, "group");
    assert_eq!(document.depth(moved), 1);

    let (back, _) = document
        .reparent_layer(moved, None)
        .expect("layer moved out of the group");
    assert_eq!(document.layers[back].parent, None);
    assert_eq!(document.layers[back].name, "outside");
    assert_eq!(document.depth(back), 0);
}

#[test]
fn a_group_cannot_be_reparented_into_its_own_child() {
    let mut document = Document::new(8, 8);
    let group = document.add_group("group");
    let child = document.add_layer("child");
    assert_eq!(document.reparent_layer(group, Some(child)), None);
}

#[test]
fn clip_base_names_the_layer_below_even_when_it_is_a_group() {
    let mut document = Document::new(8, 8);
    let base = document.add_layer("base");
    let clipped = document.add_layer("clipped");
    document.layers[clipped].clipping = true;
    assert_eq!(document.clip_base(clipped), Some(base));

    let group = document.add_group("group");
    let inside = document.add_layer("child");
    let child = document
        .reparent_layer(inside, None)
        .expect("the child moves out of the group")
        .0;
    document.layers[child].clipping = true;
    assert_eq!(document.clip_base(child), Some(group));
    assert!(document.is_group(group));
}

// Incremental layer structure changes. A structural edit must not drop the other layers' tiles:
// the stack only rearranges hosts and the shell keeps painting without a full re-upload.

use super::*;
use crate::Renderer;

fn solid(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    pixel
        .iter()
        .copied()
        .cycle()
        .take((width * height * 4) as usize)
        .collect()
}

fn pixel(width: u32, pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

#[test]
fn inserting_a_layer_keeps_the_other_pixels_resident() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let mut stack = LayerStack::new(4, 4);
    assert!(stack.set(device, queue, 0, &solid(4, 4, [255, 0, 0, 255])));
    assert!(stack.set(device, queue, 1, &solid(4, 4, [0, 0, 255, 255])));
    let specs = [LayerSpec::new(0), LayerSpec::new(1)];
    let out = renderer.composite_stack(&mut stack, &specs);
    assert_eq!(pixel(4, &out, 0, 0), [0, 0, 255, 255], "blue is on top");

    let resident = stack.resident_bytes();
    assert!(stack.insert_layer(1, 4, 4));
    assert_eq!(
        stack.resident_bytes(),
        resident,
        "insert dropped a resident tile"
    );
    assert!(!stack.layer_missing(0));
    assert!(!stack.layer_missing(2));

    // The old top layer moved up and still paints over the empty middle layer.
    let mut above = LayerSpec::new(2);
    let specs = [LayerSpec::new(0), LayerSpec::new(1), above];
    let out = renderer.composite_stack(&mut stack, &specs);
    assert_eq!(pixel(4, &out, 0, 0), [0, 0, 255, 255]);
    let middle = stack
        .read_layer_rect(device, queue, 1, (0, 0, 4, 4))
        .expect("an empty layer still reads");
    assert_eq!(pixel(4, &middle.pixels, 1, 1), [0, 0, 0, 0]);
    let top = stack
        .read_layer_rect(device, queue, 2, (0, 0, 4, 4))
        .expect("the shifted layer kept its tiles");
    assert_eq!(pixel(4, &top.pixels, 1, 1), [0, 0, 255, 255]);

    // Hiding the old top layer shows the layer that used to be under it.
    above.visible = false;
    stack.mark_all_dirty();
    let specs = [LayerSpec::new(0), LayerSpec::new(1), above];
    let out = renderer.composite_stack(&mut stack, &specs);
    assert_eq!(pixel(4, &out, 0, 0), [255, 0, 0, 255]);
}

#[test]
fn removing_a_layer_leaves_the_rest_intact() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let mut stack = LayerStack::new(4, 4);
    assert!(stack.set(device, queue, 0, &solid(4, 4, [255, 0, 0, 255])));
    assert!(stack.set(device, queue, 1, &solid(4, 4, [0, 0, 255, 255])));
    let specs = [LayerSpec::new(0), LayerSpec::new(1)];
    renderer.composite_stack(&mut stack, &specs);

    assert!(stack.remove_layer(1));
    assert_eq!(stack.len(), 1);
    let out = renderer.composite_stack(&mut stack, &specs);
    assert_eq!(pixel(4, &out, 0, 0), [255, 0, 0, 255]);
    let bottom = stack
        .read_layer_rect(device, queue, 0, (0, 0, 4, 4))
        .expect("the remaining layer kept its tiles");
    assert_eq!(pixel(4, &bottom.pixels, 2, 2), [255, 0, 0, 255]);
}

#[test]
fn moving_a_layer_changes_only_the_draw_order() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let mut stack = LayerStack::new(4, 4);
    assert!(stack.set(device, queue, 0, &solid(4, 4, [255, 0, 0, 255])));
    assert!(stack.set(device, queue, 1, &solid(4, 4, [0, 0, 255, 255])));
    let specs = [LayerSpec::new(0), LayerSpec::new(1)];
    let out = renderer.composite_stack(&mut stack, &specs);
    assert_eq!(pixel(4, &out, 0, 0), [0, 0, 255, 255]);
    let resident = stack.resident_bytes();

    assert!(stack.move_layer(1, 0));
    assert_eq!(
        stack.resident_bytes(),
        resident,
        "a reorder reallocated a tile"
    );
    let out = renderer.composite_stack(&mut stack, &specs);
    assert_eq!(pixel(4, &out, 0, 0), [255, 0, 0, 255], "red is now on top");
    let moved = stack
        .read_layer_rect(device, queue, 0, (0, 0, 4, 4))
        .expect("the moved layer kept its tiles");
    assert_eq!(pixel(4, &moved.pixels, 0, 0), [0, 0, 255, 255]);
    let behind = stack
        .read_layer_rect(device, queue, 1, (0, 0, 4, 4))
        .expect("the layer behind kept its tiles");
    assert_eq!(pixel(4, &behind.pixels, 0, 0), [255, 0, 0, 255]);
}

#[test]
fn a_structure_change_checks_its_bounds() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let mut stack = LayerStack::new(4, 4);
    stack.set(
        renderer.device(),
        renderer.queue(),
        0,
        &solid(4, 4, [255, 0, 0, 255]),
    );
    assert!(!stack.insert_layer(2, 4, 4), "past the end");
    assert!(!stack.insert_layer(0, 8, 8), "wrong canvas size");
    assert!(!stack.remove_layer(1), "not a layer");
    assert!(!stack.move_layer(0, 1), "not a layer");
    assert!(stack.insert_layer(1, 4, 4), "appending is allowed");
}
